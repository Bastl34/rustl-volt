// the converter window: level and car lists, settings, convert button, log

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};

use eframe::egui::{self, RichText};

use revolt_converter::convert::{self, cars, Log, LogLevel, Message, Settings};
use revolt_converter::download;
use revolt_converter::revolt::{find_file, has_reversed, inf::Inf, list_levels, MIRRORED, REVERSED};
use crate::log_view::LogView;
use revolt_converter::steam;

// a level or a car: folder name, name in the game
struct Entry
{
    id: String,
    name: String,
    selected: bool,
}

pub struct ConverterApp
{
    game_dir: String,
    project_dir: String,
    out_dir: String,
    scale: f32,
    unlit: bool,
    uv_inset: bool,
    add_to_project: bool,
    // the car every level scene gets on its start position, a folder in cars/
    start_car: String,
    levels: Vec<Entry>,
    cars: Vec<Entry>,
    log: LogView,
    progress: f32,
    // the last heading and step of the log - text on the progress bar
    progress_text: String,
    current_heading: String,
    // Some while a conversion or a download runs
    receiver: Option<Receiver<Message>>,
    // the soundtrack too when downloading from re-volt.io
    download_music: bool,
    // the folder of the running download - the game folder once it is done
    downloading: Option<PathBuf>,
}

impl ConverterApp
{
    pub fn new() -> Self
    {
        // the re-volt project folder is the parent of the converter
        let project_dir = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap_or(Path::new(".")).to_path_buf();

        let defaults = Settings::defaults(PathBuf::new(), project_dir);
        let mut app = Self
        {
            game_dir: String::new(),
            project_dir: defaults.project_dir.display().to_string(),
            out_dir: defaults.out_dir.display().to_string(),
            scale: defaults.scale,
            unlit: defaults.unlit,
            uv_inset: defaults.uv_inset,
            add_to_project: defaults.add_to_project,
            start_car: defaults.start_car,
            levels: vec![],
            cars: vec![],
            log: LogView::default(),
            progress: 0.0,
            progress_text: String::new(),
            current_heading: String::new(),
            receiver: None,
            download_music: true,
            downloading: None,
        };

        app.find_game();
        app
    }

    fn find_game(&mut self)
    {
        match steam::find_revolt()
        {
            Some(dir) =>
            {
                self.game_dir = dir.display().to_string();
                self.log.push(LogLevel::Info, format!("Re-Volt found: {}", self.game_dir));
            },
            None => self.log.push(LogLevel::Warning, "Re-Volt not found in Steam - enter the game folder".to_string()),
        }

        self.scan();
    }

    fn scan(&mut self)
    {
        let game_dir = PathBuf::from(&self.game_dir);

        // the variants of a race track right below it: <level>/reversed, <level>/mirrored, <level>/reversed/mirrored
        self.levels = list_levels(&game_dir).into_iter().flat_map(|id|
        {
            let level_dir = game_dir.join("levels").join(&id);
            let name = find_file(&level_dir, &format!("{id}.inf")).and_then(|path| Inf::load(&path).name()).unwrap_or_default();
            let selected = id.eq_ignore_ascii_case("garden1");
            let variants = match has_reversed(&game_dir, &id)
            {
                true => vec![
                    Entry { id: format!("{id}/{REVERSED}"), name: format!("{name} (reversed)"), selected: false },
                    Entry { id: format!("{id}/{MIRRORED}"), name: format!("{name} (mirrored)"), selected: false },
                    Entry { id: format!("{id}/{REVERSED}/{MIRRORED}"), name: format!("{name} (reversed, mirrored)"), selected: false },
                ],
                false => vec![],
            };
            std::iter::once(Entry { id, name, selected }).chain(variants)
        }).collect();

        self.cars = cars::list(&game_dir).into_iter().map(|(id, name)|
        {
            let selected = id.eq_ignore_ascii_case("rc");
            Entry { id, name, selected }
        }).collect();
    }

    fn start(&mut self)
    {
        let settings = Settings
        {
            game_dir: PathBuf::from(&self.game_dir),
            project_dir: PathBuf::from(&self.project_dir),
            files_dir: PathBuf::from(&self.project_dir),
            out_dir: PathBuf::from(&self.out_dir),
            scale: self.scale,
            unlit: self.unlit,
            uv_inset: self.uv_inset,
            add_to_project: self.add_to_project,
            start_car: self.start_car.clone(),
            mirrored: false,
        };
        let selected = |entries: &Vec<Entry>| entries.iter().filter(|e| e.selected).map(|e| e.id.clone()).collect::<Vec<_>>();
        let (levels, cars) = (selected(&self.levels), selected(&self.cars));

        let (sender, receiver) = channel();
        let log = Log::new(sender);
        std::thread::spawn(move || convert::run(settings, levels, cars, log));

        self.log.clear();
        self.progress = 0.0;
        self.progress_text.clear();
        self.current_heading.clear();
        self.receiver = Some(receiver);
    }

    // re-volt.io into the project folder (the game finds it there too)
    fn start_download(&mut self)
    {
        let dir = download::dir(Path::new(&self.project_dir));
        let (sender, receiver) = channel();
        let log = Log::new(sender);
        let (target, music) = (dir.clone(), self.download_music);
        std::thread::spawn(move || download::run(target, music, log));

        self.log.clear();
        self.progress = 0.0;
        self.progress_text.clear();
        self.current_heading.clear();
        self.receiver = Some(receiver);
        self.downloading = Some(dir);
    }

    fn downloaded(&mut self, dir: PathBuf)
    {
        if !revolt_converter::is_game_dir(&dir)
        {
            self.log.push(LogLevel::Error, format!("{} has no Re-Volt files after the download", dir.display()));
            return;
        }
        self.game_dir = dir.display().to_string();
        self.log.push(LogLevel::Info, format!("Game folder: {}", self.game_dir));
        self.scan();
    }

    fn poll(&mut self)
    {
        let Some(receiver) = &self.receiver else { return; };

        let mut done = false;
        for message in receiver.try_iter()
        {
            match message
            {
                Message::Log(level, text) =>
                {
                    match level
                    {
                        LogLevel::Heading => { self.current_heading = text.clone(); self.progress_text = text.clone(); },
                        LogLevel::Step => self.progress_text = format!("{}  ·  {}", self.current_heading, text),
                        _ => {},
                    }
                    self.log.push(level, text);
                },
                Message::Progress(value) => self.progress = value,
                Message::Done => done = true,
            }
        }

        if done
        {
            self.receiver = None;
            if let Some(dir) = self.downloading.take() { self.downloaded(dir); }
        }
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui)
    {
        let running = self.receiver.is_some();

        egui::Grid::new("settings").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui|
        {
            ui.label("Game folder");
            ui.horizontal(|ui|
            {
                if ui.add(egui::TextEdit::singleline(&mut self.game_dir).desired_width(500.0)).lost_focus()
                {
                    self.scan();
                }
                if ui.button("Find in Steam").clicked() { self.find_game(); }
                let megabytes = download::missing_megabytes(&download::dir(Path::new(&self.project_dir)), self.download_music);
                let label = if megabytes > 0 { format!("Download from re-volt.io ({megabytes} MB)") } else { "Use re-volt.io download".to_string() };
                if ui.add_enabled(!running, egui::Button::new(label)).on_hover_text(download::SITE).clicked() { self.start_download(); }
                ui.checkbox(&mut self.download_music, "with soundtrack");
            });
            ui.end_row();

            ui.label("Project folder");
            ui.add(egui::TextEdit::singleline(&mut self.project_dir).desired_width(500.0));
            ui.end_row();

            ui.label("Output folder");
            ui.add(egui::TextEdit::singleline(&mut self.out_dir).desired_width(500.0));
            ui.end_row();

            ui.label("Scale");
            ui.horizontal(|ui|
            {
                ui.add(egui::DragValue::new(&mut self.scale).speed(0.0001).range(0.0001..=1.0).max_decimals(4));
                ui.label("meters per Re-Volt unit");
            });
            ui.end_row();

            ui.label("Materials");
            ui.checkbox(&mut self.unlit, "unlit (the levels are pre-lit through vertex colors)");
            ui.end_row();

            ui.label("Textures");
            ui.checkbox(&mut self.uv_inset, "UV inset (half a texel inward, against seams between atlas tiles)");
            ui.end_row();

            ui.label("Scenes");
            ui.checkbox(&mut self.add_to_project, "add the level scenes and cars.scene to the .project (only if not in there yet)");
            ui.end_row();

            ui.label("Start car");
            ui.horizontal(|ui|
            {
                let selected = self.cars.iter().find(|c| c.id.eq_ignore_ascii_case(&self.start_car)).map_or(self.start_car.clone(), |c| format!("{} ({})", c.name, c.id));
                egui::ComboBox::from_id_salt("start_car").selected_text(selected).show_ui(ui, |ui|
                {
                    for car in &self.cars
                    {
                        ui.selectable_value(&mut self.start_car, car.id.clone(), format!("{} ({})", car.name, car.id));
                    }
                });
                ui.label("on the start position of every level, with a vehicle controller - convert the car too");
            });
            ui.end_row();
        });

        ui.add_space(6.0);
        ui.horizontal(|ui|
        {
            let levels = self.levels.iter().filter(|e| e.selected).count();
            let cars = self.cars.iter().filter(|e| e.selected).count();
            let button = egui::Button::new(RichText::new(format!("Convert {levels} level(s) · {cars} car(s)")).strong());
            if ui.add_enabled(!running && levels + cars > 0, button).clicked()
            {
                self.start();
            }
            let text = format!("{:.0} %   {}", self.progress * 100.0, self.progress_text);
            ui.add(egui::ProgressBar::new(self.progress).text(text).animate(running));
        });
    }

    // the levels and the cars to convert, each list with all / none
    fn lists_ui(&mut self, ui: &mut egui::Ui)
    {
        egui::ScrollArea::vertical().show(ui, |ui|
        {
            entry_list(ui, "Levels", &mut self.levels);
            ui.add_space(6.0);
            entry_list(ui, "Cars", &mut self.cars);
        });
    }
}

fn entry_list(ui: &mut egui::Ui, title: &str, entries: &mut Vec<Entry>)
{
    let selected = entries.iter().filter(|e| e.selected).count();
    egui::CollapsingHeader::new(RichText::new(format!("{title}   {selected} / {}", entries.len())).heading())
        .id_salt(title)
        .default_open(true)
        .show(ui, |ui|
        {
            ui.horizontal(|ui|
            {
                if ui.small_button("all").clicked() { entries.iter_mut().for_each(|e| e.selected = true); }
                if ui.small_button("none").clicked() { entries.iter_mut().for_each(|e| e.selected = false); }
            });

            egui::Grid::new(title).num_columns(2).spacing([10.0, 2.0]).show(ui, |ui|
            {
                for entry in entries.iter_mut()
                {
                    ui.checkbox(&mut entry.selected, &entry.id);
                    ui.label(RichText::new(&entry.name).weak());
                    ui.end_row();
                }
            });
        });
}

impl eframe::App for ConverterApp
{
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame)
    {
        self.poll();
        if self.receiver.is_some()
        {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
        }

        egui::Panel::left("lists").resizable(true).default_size(260.0).show(ui, |ui| self.lists_ui(ui));
        egui::Panel::top("settings").show(ui, |ui| { ui.add_space(6.0); self.settings_ui(ui); ui.add_space(6.0); });
        egui::CentralPanel::default().show(ui, |ui| self.log.ui(ui));
    }
}

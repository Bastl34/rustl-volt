// the start of the game in the boot scene: the files of Re-Volt (the last folder, Steam or a chosen one, or the download of re-volt.io), convert them if they are missing or old, then the menu
// the converter (../converter, library) writes into the project folder - or into the user data folder when that can not be written (exports in Program Files)

use std::path::{Path, PathBuf};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc::{channel, Receiver};

use rustl::egui::{self, Align2, Color32, FontId, LayerId, RichText, TextureHandle, TextureOptions};
use rustl::prelude::*;
use rustl::resources::resources::{exists, load_binary};
use rustl::state::state::MouseCapture;

use crate::assets::AssetInfo;
use crate::frontend;
use crate::hud::{Anchor, Painter, WIDTH};
use crate::menu::{LOADING_BAR, LOADING_FILE_CHARS, LOADING_FILL, LOADING_FRAME, LOADING_INSET, LOADING_LOGO_WIDTH, LOADING_LOGO_Y, LOADING_PROGRESS, LOADING_RADIUS, LOADING_RIM, LOADING_RIM_WIDTH, LOGO_ASPECT, LOGO_FILE};
use crate::pad::Buttons;

// the only scene shipped with the game - the others are links in the .project until the converter writes them
pub const BOOT_SCENE: &str = "boot";
// the user data folder (%APPDATA%/rustl-volt, the folder of the save data too) when the project folder is read only
const DATA_FOLDER: &str = "rustl-volt";
// own picture of the setup screens (no file of the original game) - the converted frontend loading screen of the game replaces it
const PICTURE_FILE: &str = "ui/loading.png";
const ORIGINAL_PICTURE_FILE: &str = "loading_front.png";
// everything the converter logged, next to convert.json
const LOG_FILE: &str = "convert.log";
const DOWNLOAD_LOG_FILE: &str = "download.log";
// the key help below the text of the screens with buttons
const KEYS_HINT: &str = "Arrows: select    Enter: OK    Backspace: quit";
// the screen before the download of re-volt.io - 4 lines and the note fit above the buttons
const DISCLAIMER: [&str; 4] = [
    "The files are downloaded from re-volt.io, a site of the Re-Volt community.",
    "Rustl-Volt only offers that download - it does not host, own or share them",
    "and is not affiliated with re-volt.io or the rights holders of Re-Volt.",
    "It is not responsible for the contents of the files or their distribution.",
];
const DISCLAIMER_NOTE: &str = "You download and use them at your own risk and responsibility.";
// a longer error line ends with ... on the screen
const MESSAGE_CHARS: usize = 90;

const FILL: usize = 0;
const LOGO: usize = 1;
const PICTURE: usize = 2;
const BACKGROUND: Color32 = Color32::from_rgb(0x05, 0x05, 0x0a);
// the dark band behind the text over the picture
const PANEL: [f32; 4] = [40.0, 150.0, 560.0, 230.0];
const PANEL_COLOR: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 170);
const TITLE_Y: f32 = 166.0;
const TITLE_SIZE: f32 = 22.0;
const TEXT_Y: f32 = 206.0;
const TEXT_STEP: f32 = 22.0;
const TEXT_SIZE: f32 = 13.0;
const BUTTONS_Y: f32 = 326.0;
const BUTTON_SIZE: f32 = 15.0;
const BAR_TEXT_SIZE: f32 = 12.0;
const TITLE_COLOR: Color32 = Color32::WHITE;
const TEXT_COLOR: Color32 = Color32::from_rgb(0xc8, 0xd0, 0xff);
const NOTE_COLOR: Color32 = Color32::from_rgb(0xff, 0xc8, 0x40);
// the picker opens once the screen behind it was drawn
const PICK_AFTER_FRAMES: u32 = 2;

enum Phase
{
    // the first update looks at the files and for the game
    Check,
    // nothing converted yet and no folder of an earlier run: the Steam files or the download of re-volt.io
    #[cfg(not(target_arch = "wasm32"))]
    Choose(Choice),
    // before the download of re-volt.io: the files are third party, at the user's own risk - agree or back
    #[cfg(not(target_arch = "wasm32"))]
    Disclaimer,
    // re-volt.io: the packs into revolt_io next to the converted files - the conversion follows
    #[cfg(not(target_arch = "wasm32"))]
    Downloading(Conversion),
    // Re-Volt not found: the folder picker opens by itself (cancel quits), a wrong folder is said here
    NotFound(String),
    #[cfg(not(target_arch = "wasm32"))]
    Converting(Conversion),
    // nothing to play: the reason, the buttons choose another folder or quit
    Failed(String),
    // the frontend loads - then the menu takes over
    Loading,
}

#[derive(Clone, Copy, PartialEq)]
enum Action
{
    Steam,
    Download,
    // the disclaimer of the download was accepted
    Agree,
    Pick,
    Retry,
    // to the choice of the files
    Back,
    Quit,
}

#[cfg(not(target_arch = "wasm32"))]
struct Choice
{
    // the Re-Volt of Steam, if found
    steam: Option<PathBuf>,
    // what the download still needs, 0 = done already
    megabytes: u64,
}

#[cfg(not(target_arch = "wasm32"))]
struct Conversion
{
    receiver: Receiver<revolt_converter::convert::Message>,
    game_dir: PathBuf,
    progress: f32,
    heading: String,
    step: String,
    // every line for assets/convert.log
    lines: Vec<String>,
    warnings: usize,
    errors: usize,
}

struct Textures
{
    logo: Option<TextureHandle>,
    picture: Option<TextureHandle>,
}

pub struct Setup
{
    pub boot_scene: u32,
    // the project folder as the scenes see it (relative in exports) and on the disk
    project: String,
    project_dir: PathBuf,
    // where the converter writes (the project folder or the user data folder) - set by the check
    target: Option<PathBuf>,
    // why it converts
    reason: String,
    phase: Phase,
    // frames the current phase was drawn
    drawn: u32,
    // a button of the screen, used by the next update
    clicked: Option<Action>,
    // the button Enter presses (arrows move it)
    selected: usize,
    textures: Option<Textures>,
    // the frontend was a shell and the setup loaded it - Stop unloads it again
    pub loaded_frontend: Option<u32>,
    // the mouse setting of the project - the setup needs the cursor (buttons, folder picker)
    mouse_capture: MouseCapture,
}

impl Setup
{
    pub fn new(state: &mut State, boot_scene: u32, project: String) -> Self
    {
        let project_dir = disk_path(&project);
        let mouse_capture = state.input.mouse_capture;
        state.input.mouse_capture = MouseCapture::Never;
        console_log!("setup: project folder {}", project_dir.display());

        Self { boot_scene, project, project_dir, target: None, reason: String::new(), phase: Phase::Check, drawn: 0, clicked: None, selected: 0, textures: None, loaded_frontend: None, mouse_capture }
    }

    // the menu runs - the mouse setting of the project again
    pub fn finish(self, state: &mut State)
    {
        state.input.mouse_capture = self.mouse_capture;
    }

    // Some(frontend): it was loaded already and is active now - the menu starts without a scene_loaded
    pub fn update(&mut self, state: &mut State, pad: &Buttons) -> Option<u32>
    {
        let buttons = self.buttons();
        let mut action = self.clicked.take();
        if action.is_none() && !buttons.is_empty()
        {
            if pad.left || pad.up { self.selected = self.selected.saturating_sub(1); }
            if pad.right || pad.down { self.selected = (self.selected + 1).min(buttons.len() - 1); }
            if pad.enter || pad.start { action = buttons.get(self.selected).map(|(_, action)| *action); }
            else if pad.back || pad.pause { action = Some(Action::Quit); }
        }

        match &mut self.phase
        {
            Phase::Check => return self.check(state),
            #[cfg(not(target_arch = "wasm32"))]
            Phase::Choose(choice) => match action
            {
                Some(Action::Steam) => match choice.steam.clone()
                {
                    Some(dir) => self.convert(dir),
                    None =>
                    {
                        console_warning!("setup: Re-Volt not found in Steam - asking for the folder");
                        self.go(Phase::NotFound(String::new()));
                    },
                },
                Some(Action::Download) => self.go(Phase::Disclaimer),
                Some(Action::Quit) => quit(state),
                _ => {},
            },
            #[cfg(not(target_arch = "wasm32"))]
            Phase::Disclaimer => match action
            {
                Some(Action::Agree) => self.download(),
                // Backspace goes back here, not out of the game
                Some(Action::Back | Action::Quit) => self.choose(),
                _ => {},
            },
            Phase::NotFound(_) =>
            {
                let first_pick = self.drawn >= PICK_AFTER_FRAMES && self.reason_is_first_search();
                match action
                {
                    Some(Action::Quit) => quit(state),
                    Some(Action::Back) => self.choose(),
                    Some(_) => return self.pick(state),
                    None if first_pick => return self.pick(state),
                    None => {},
                }
            },
            #[cfg(not(target_arch = "wasm32"))]
            Phase::Downloading(download) =>
            {
                if download.poll()
                {
                    self.downloaded();
                }
            },
            #[cfg(not(target_arch = "wasm32"))]
            Phase::Converting(conversion) =>
            {
                if conversion.poll()
                {
                    return self.converted(state);
                }
            },
            Phase::Failed(_) => match action
            {
                Some(Action::Quit) => quit(state),
                Some(Action::Retry) => self.go(Phase::Check),
                Some(Action::Pick) => return self.pick(state),
                Some(Action::Back) => self.choose(),
                _ => {},
            },
            Phase::Loading => {},
        }
        None
    }

    fn go(&mut self, phase: Phase)
    {
        self.phase = phase;
        self.drawn = 0;
        self.selected = 0;
    }

    // the choice of the files: Steam or re-volt.io
    fn choose(&mut self)
    {
        #[cfg(target_arch = "wasm32")]
        self.go(Phase::Check);

        #[cfg(not(target_arch = "wasm32"))]
        {
            let steam = revolt_converter::steam::find_revolt().filter(|dir| revolt_converter::is_game_dir(dir));
            let megabytes = self.download_dir().map_or(0, |dir| revolt_converter::download::missing_megabytes(&dir, true));
            self.go(Phase::Choose(Choice { steam, megabytes }));
        }
    }

    // next to the converted files (the folder of the game, user data only if that is read only) - None before check
    #[cfg(not(target_arch = "wasm32"))]
    fn download_dir(&self) -> Option<PathBuf>
    {
        self.target.as_deref().map(revolt_converter::download::dir)
    }

    // the packs on a worker thread - the conversion of the folder follows (downloaded)
    #[cfg(not(target_arch = "wasm32"))]
    fn download(&mut self)
    {
        let Some(dir) = self.download_dir() else
        {
            self.go(Phase::Failed("No folder for the download.".to_string()));
            return;
        };
        console_log!("setup: downloading the files of re-volt.io into {}", dir.display());
        let (sender, receiver) = channel();
        let log = revolt_converter::convert::Log::new(sender);
        let target = dir.clone();
        std::thread::spawn(move || revolt_converter::download::run(target, true, log));

        self.go(Phase::Downloading(Conversion::new(receiver, dir)));
    }

    // its log next to convert.json, then the conversion - or why not
    #[cfg(not(target_arch = "wasm32"))]
    fn downloaded(&mut self)
    {
        let Phase::Downloading(download) = std::mem::replace(&mut self.phase, Phase::Check) else { return; };

        if let Some(target) = &self.target
        {
            let path = target.join("assets").join(DOWNLOAD_LOG_FILE);
            match std::fs::write(&path, download.lines.join("\n"))
            {
                Ok(()) => { console_log!("setup: download log {}", path.display()); },
                Err(error) => { console_warning!("setup: {}: {}", path.display(), error); },
            }
        }
        let error = download.lines.iter().rev().find_map(|line| line.strip_prefix("E ")).map(|line| line.trim().to_string());
        if error.is_some() || !revolt_converter::is_game_dir(&download.game_dir)
        {
            let reason = error.unwrap_or_else(|| format!("{} has no Re-Volt files", download.game_dir.display()));
            console_error!("setup: download failed - {}", reason);
            self.go(Phase::Failed(format!("Download failed: {reason}")));
            return;
        }
        self.convert(download.game_dir);
    }

    // the picker opens by itself only when nothing was picked yet (no message on the screen)
    fn reason_is_first_search(&self) -> bool
    {
        matches!(&self.phase, Phase::NotFound(message) if message.is_empty())
    }

    // where to write, whether to convert, which game folder
    fn check(&mut self, state: &mut State) -> Option<u32>
    {
        #[cfg(target_arch = "wasm32")]
        {
            return self.load_menu(state);
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let target = match self.locate()
            {
                Ok(target) => target,
                Err(error) => { console_error!("setup: {}", error); self.go(Phase::Failed(error)); return None; },
            };
            let info = AssetInfo::load(&target.join("assets").to_string_lossy());
            let reason = convert_reason(state, info.as_ref());
            self.target = Some(target.clone());
            // the converted picture of the game, if there is one
            self.textures = None;

            let Some(reason) = reason else
            {
                if let Some(info) = &info { console_log!("setup: assets version {} (converter {}, {}) in {}", info.oldest.unwrap_or_default(), info.converter, info.converted_at, target.display()); }
                return self.load_menu(state);
            };
            console_log!("setup: convert - {}", reason);
            self.reason = reason;

            // the folder of the last run, else the choice of the files
            let last = info.map(|info| PathBuf::from(info.game_dir)).filter(|dir| !dir.as_os_str().is_empty() && revolt_converter::is_game_dir(dir));
            match last
            {
                Some(game_dir) => self.convert(game_dir),
                None => self.choose(),
            }
            None
        }
    }

    // the project folder if it can be written, else the user data folder - converted files found in one of them win
    #[cfg(not(target_arch = "wasm32"))]
    fn locate(&self) -> Result<PathBuf, String>
    {
        let converted = |dir: &Path| dir.join("assets").join("convert.json").is_file();
        let relative = Path::new(&self.project).is_relative();
        let data_root = rustl::resources::save_data::save_folder(DATA_FOLDER).filter(|_| relative);
        let in_data = data_root.as_ref().map(|root| root.join(&self.project));

        let use_data = |root: &PathBuf, dir: PathBuf| -> PathBuf
        {
            // the scenes and assets are relative paths - the engine looks there before the resource folders
            rustl::resources::resources::add_resource_root(root.clone());
            console_log!("setup: converted files in the user data folder {}", dir.display());
            dir
        };

        if converted(&self.project_dir)
        {
            return Ok(self.project_dir.clone());
        }
        if let (Some(root), Some(dir)) = (&data_root, &in_data)
        {
            if converted(dir) { return Ok(use_data(root, dir.clone())); }
        }
        if writable(&self.project_dir)
        {
            return Ok(self.project_dir.clone());
        }
        console_warning!("setup: {} is read only", self.project_dir.display());
        match (&data_root, in_data)
        {
            (Some(root), Some(dir)) => Ok(use_data(root, dir)),
            _ => Err(format!("Can not write into {}", self.project_dir.display())),
        }
    }

    // the folder picker - cancel goes back to the choice of the files, a folder without Re-Volt is said on the screen
    #[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
    fn pick(&mut self, state: &mut State) -> Option<u32>
    {
        #[cfg(target_arch = "wasm32")]
        {
            quit(state);
            return None;
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let picked = rfd::FileDialog::new().set_title("Choose the Re-Volt folder").pick_folder();
            let Some(dir) = picked else
            {
                console_log!("setup: no folder chosen");
                self.choose();
                return None;
            };
            if !revolt_converter::is_game_dir(&dir)
            {
                console_warning!("setup: {} has no Re-Volt files (levels, cars)", dir.display());
                self.go(Phase::NotFound(format!("{} has no Re-Volt files (levels and cars).", dir.display())));
                return None;
            }
            if self.target.is_none()
            {
                // Failed before the check found a folder to write into
                self.go(Phase::Check);
                return None;
            }
            self.convert(dir);
            None
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn convert(&mut self, game_dir: PathBuf)
    {
        use revolt_converter::convert::{self, Log, Settings};

        let Some(target) = self.target.clone() else { return; };
        if self.reason.is_empty() { self.reason = "Another Re-Volt folder".to_string(); }
        console_log!("setup: converting {} into {}", game_dir.display(), target.display());

        // the scenes are links in the .project already, the logo comes from the project folder
        let settings = Settings { add_to_project: false, files_dir: self.project_dir.clone(), ..Settings::defaults(game_dir.clone(), target) };
        let (levels, cars) = revolt_converter::everything(&game_dir);
        let (sender, receiver) = channel();
        let log = Log::new(sender);
        std::thread::spawn(move || convert::run(settings, levels, cars, log));

        self.go(Phase::Converting(Conversion::new(receiver, game_dir)));
    }

    // the run is done: its log next to convert.json, then the menu - or why not
    #[cfg(not(target_arch = "wasm32"))]
    fn converted(&mut self, state: &mut State) -> Option<u32>
    {
        let Phase::Converting(conversion) = std::mem::replace(&mut self.phase, Phase::Loading) else { return None; };
        let Some(target) = self.target.clone() else { return None; };

        let log_path = target.join("assets").join(LOG_FILE);
        match std::fs::write(&log_path, conversion.lines.join("\n"))
        {
            Ok(()) => { console_log!("setup: converter log {}", log_path.display()); },
            Err(error) => { console_warning!("setup: {}: {}", log_path.display(), error); },
        }

        let summary = format!("{} warnings, {} errors", conversion.warnings, conversion.errors);
        if !target.join(format!("{}.scene", frontend::SCENE)).is_file()
        {
            console_error!("setup: no {}.scene after the conversion ({}) - see {}", frontend::SCENE, summary, log_path.display());
            self.go(Phase::Failed(format!("The menu was not converted ({summary}). See {}", log_path.display())));
            return None;
        }
        match conversion.errors
        {
            0 => { console_success!("setup: converted ({})", summary); },
            _ => { console_warning!("setup: converted with errors ({}) - see {}", summary, log_path.display()); },
        }
        self.reason.clear();
        self.textures = None;
        self.load_menu(state)
    }

    // the frontend becomes the active scene - a shell loads first (scene_loaded starts the menu)
    fn load_menu(&mut self, state: &mut State) -> Option<u32>
    {
        let Some((id, loaded)) = state.scenes.iter().find(|scene| scene.name.eq_ignore_ascii_case(frontend::SCENE)).map(|scene| (scene.id, scene.is_loaded())) else
        {
            console_error!("setup: the project has no scene '{}'", frontend::SCENE);
            self.go(Phase::Failed(format!("The project has no scene '{}'.", frontend::SCENE)));
            return None;
        };
        self.go(Phase::Loading);
        if !loaded { self.loaded_frontend = Some(id); }
        state.set_active_scene(id);
        loaded.then_some(id)
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, state: &State)
    {
        self.drawn = self.drawn.saturating_add(1);
        let ctx = ui.ctx().clone();
        if self.textures.is_none()
        {
            self.textures = Some(self.load_textures(&ctx));
        }
        let Some(textures) = &self.textures else { return; };

        let white = egui::TextureId::default();
        let rect = ctx.content_rect();
        let mut painter = Painter::new(rect, vec![white, textures.logo.as_ref().map_or(white, |t| t.id()), textures.picture.as_ref().map_or(white, |t| t.id())]);
        painter.shapes.push(egui::Shape::rect_filled(rect, egui::CornerRadius::ZERO, BACKGROUND));

        let [x, y, w, h] = LOADING_FRAME;
        painter.rounded(Anchor::Center, LOADING_FRAME, LOADING_FILL, LOADING_RIM, LOADING_RIM_WIDTH, LOADING_RADIUS);
        if textures.picture.is_some()
        {
            painter.image(PICTURE, Anchor::Center, [x + LOADING_INSET, y + LOADING_INSET, w - 2.0 * LOADING_INSET, h - 2.0 * LOADING_INSET], Color32::WHITE);
        }
        if textures.logo.is_some()
        {
            let height = LOADING_LOGO_WIDTH / LOGO_ASPECT;
            painter.image(LOGO, Anchor::Center, [(WIDTH - LOADING_LOGO_WIDTH) * 0.5, LOADING_LOGO_Y, LOADING_LOGO_WIDTH, height], Color32::WHITE);
        }
        let (title, lines, note, bar) = self.content(state);
        let buttons = self.buttons();
        // paths always with forward slashes on the screen
        let slashes = |text: String| text.replace('\\', "/");
        let (lines, note, bar): (Vec<String>, _, _) = (lines.into_iter().map(slashes).collect(), note.map(slashes), bar.map(|(progress, text)| (progress, slashes(text))));
        // only behind text - the loading screen shows the picture alone
        if !lines.is_empty()
        {
            painter.gradient(FILL, Anchor::Center, PANEL, PANEL_COLOR, PANEL_COLOR);
        }

        let [x, y, w, h] = LOADING_BAR;
        painter.rounded(Anchor::Center, LOADING_BAR, LOADING_FILL, LOADING_RIM, LOADING_RIM_WIDTH, LOADING_RADIUS);
        if let Some((progress, _)) = &bar
        {
            let inner = [x + LOADING_INSET, y + LOADING_INSET, (w - 2.0 * LOADING_INSET) * progress.clamp(0.0, 1.0), h - 2.0 * LOADING_INSET];
            painter.gradient(FILL, Anchor::Center, inner, LOADING_PROGRESS, LOADING_PROGRESS);
        }

        let scale = painter.scale;
        let at = |gx: f32, gy: f32| painter.screen(Anchor::Center, gx, gy);
        let (title_pos, bar_pos) = (at(WIDTH * 0.5, TITLE_Y), at(WIDTH * 0.5, y + h * 0.5));
        let text_pos: Vec<egui::Pos2> = (0..lines.len() + 1).map(|i| at(WIDTH * 0.5, TEXT_Y + i as f32 * TEXT_STEP)).collect();
        let buttons_pos = at(WIDTH * 0.5, BUTTONS_Y);
        painter.finish(&ctx, &[PICTURE, FILL, LOGO]);

        let text = ctx.layer_painter(LayerId::background());
        if !title.is_empty()
        {
            text.text(title_pos, Align2::CENTER_TOP, title, FontId::proportional(TITLE_SIZE * scale), TITLE_COLOR);
        }
        for (line, pos) in lines.iter().zip(&text_pos)
        {
            text.text(*pos, Align2::CENTER_TOP, line, FontId::proportional(TEXT_SIZE * scale), TEXT_COLOR);
        }
        if let (Some(note), Some(pos)) = (note, text_pos.last())
        {
            text.text(*pos, Align2::CENTER_TOP, note, FontId::proportional(TEXT_SIZE * scale), NOTE_COLOR);
        }
        if let Some((_, label)) = bar
        {
            // the end of a long path - the bar holds LOADING_FILE_CHARS
            let skip = label.chars().count().saturating_sub(LOADING_FILE_CHARS);
            let label: String = label.chars().skip(skip).collect();
            text.text(bar_pos, Align2::CENTER_CENTER, label, FontId::proportional(BAR_TEXT_SIZE * scale), TITLE_COLOR);
        }

        if !buttons.is_empty()
        {
            egui::Area::new(egui::Id::unique("revolt_setup_buttons")).pivot(Align2::CENTER_TOP).fixed_pos(buttons_pos).show(&ctx, |ui|
            {
                ui.horizontal(|ui|
                {
                    for (index, (label, action)) in buttons.into_iter().enumerate()
                    {
                        let button = egui::Button::new(RichText::new(label).size(BUTTON_SIZE * scale)).selected(index == self.selected);
                        if ui.add(button).clicked()
                        {
                            self.selected = index;
                            self.clicked = Some(action);
                        }
                    }
                });
            });
        }
    }

    // the buttons of the phase - Enter presses the selected one
    fn buttons(&self) -> Vec<(&'static str, Action)>
    {
        match &self.phase
        {
            #[cfg(not(target_arch = "wasm32"))]
            Phase::Choose(_) => vec![("Steam files", Action::Steam), ("Download from re-volt.io", Action::Download), ("Quit", Action::Quit)],
            #[cfg(not(target_arch = "wasm32"))]
            Phase::Disclaimer => vec![("I agree, download", Action::Agree), ("Back", Action::Back)],
            Phase::NotFound(_) => vec![("Choose folder...", Action::Pick), ("Back", Action::Back), ("Quit", Action::Quit)],
            Phase::Failed(_) => vec![("Try again", Action::Retry), ("Choose folder...", Action::Pick), ("Back", Action::Back), ("Quit", Action::Quit)],
            _ => vec![],
        }
    }

    // title, lines, a note (warning color), the bar (progress, text) of the phase
    fn content(&self, state: &State) -> (String, Vec<String>, Option<String>, Option<(f32, String)>)
    {
        let target = self.target.as_ref().map(|target| target.display().to_string()).unwrap_or_default();
        match &self.phase
        {
            Phase::Check => (String::new(), vec![], None, None),
            #[cfg(not(target_arch = "wasm32"))]
            Phase::Choose(choice) =>
            {
                let steam = match &choice.steam
                {
                    Some(dir) => format!("Steam files: found in {}", dir.display()),
                    None => "Steam files: not found - you choose the Re-Volt folder.".to_string(),
                };
                let download = match choice.megabytes
                {
                    0 => "re-volt.io: downloaded already.".to_string(),
                    megabytes => format!("re-volt.io: downloads the files of the Re-Volt community ({megabytes} MB)."),
                };
                let lines = vec!["Rustl-Volt plays with the files of the original Re-Volt.".to_string(), steam, download, KEYS_HINT.to_string()];
                ("Re-Volt files".to_string(), lines, None, None)
            },
            #[cfg(not(target_arch = "wasm32"))]
            Phase::Disclaimer =>
            {
                let lines = DISCLAIMER.iter().map(|line| line.to_string()).collect();
                ("Before the download".to_string(), lines, Some(DISCLAIMER_NOTE.to_string()), None)
            },
            #[cfg(not(target_arch = "wasm32"))]
            Phase::Downloading(download) =>
            {
                let lines = vec![
                    "from re-volt.io: game files, car boxes of RVGL, Dreamcast soundtrack".to_string(),
                    format!("into {}", download.game_dir.display()),
                ];
                let note = (download.warnings + download.errors > 0).then(|| format!("{} warnings, {} errors", download.warnings, download.errors));
                ("Downloading Re-Volt".to_string(), lines, note, Some((download.progress, download.step.clone())))
            },
            Phase::NotFound(message) =>
            {
                let lines = vec![
                    "Re-Volt was not found in Steam.".to_string(),
                    "Choose its folder (the one with levels and cars).".to_string(),
                    KEYS_HINT.to_string(),
                ];
                let note = (!message.is_empty()).then(|| message.clone());
                ("Re-Volt not found".to_string(), lines, note, None)
            },
            #[cfg(not(target_arch = "wasm32"))]
            Phase::Converting(conversion) =>
            {
                let lines = vec![
                    self.reason.clone(),
                    format!("from {}", conversion.game_dir.display()),
                    format!("into {}", target),
                ];
                let note = (conversion.warnings + conversion.errors > 0).then(|| format!("{} warnings, {} errors", conversion.warnings, conversion.errors));
                ("Converting Re-Volt".to_string(), lines, note, Some((conversion.progress, conversion.step.clone())))
            },
            Phase::Failed(message) =>
            {
                let message = match message.chars().count() > MESSAGE_CHARS
                {
                    true => format!("{}...", message.chars().take(MESSAGE_CHARS).collect::<String>()),
                    false => message.clone(),
                };
                ("Can not start".to_string(), vec![message, KEYS_HINT.to_string()], None, None)
            },
            Phase::Loading =>
            {
                let (progress, file) = { let status = state.scene_loading.read().unwrap(); (status.progress(), status.current.clone()) };
                (String::new(), vec![], None, Some((progress, file)))
            },
        }
    }

    // the logo of the project folder; the picture: the frontend loading screen of the game once it is converted, else the own one (shipped)
    fn load_textures(&self, ctx: &egui::Context) -> Textures
    {
        let texture = |path: &str, name: &str| -> Option<TextureHandle>
        {
            let image = load_binary(path).ok().and_then(|bytes| rustl::image::load_from_memory(&bytes).ok())?.to_rgba8();
            let size = [image.width() as usize, image.height() as usize];
            Some(ctx.load_texture(format!("revolt_setup_{name}"), egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()), TextureOptions::LINEAR))
        };
        let own = |file: &str| if self.project.is_empty() { file.to_string() } else { format!("{}/{file}", self.project) };
        let warn = |path: String| { console_warning!("setup: {} missing", path); };

        let logo = texture(&own(LOGO_FILE), "logo");
        if logo.is_none() { warn(own(LOGO_FILE)); }
        let original = self.target.as_ref().map(|target| target.join("assets").join("gfx").join(ORIGINAL_PICTURE_FILE)).filter(|path| path.is_file());
        let picture = original.and_then(|path| texture(&path.to_string_lossy(), "original")).or_else(|| texture(&own(PICTURE_FILE), "picture"));
        if picture.is_none() { warn(own(PICTURE_FILE)); }
        Textures { logo, picture }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Conversion
{
    fn new(receiver: Receiver<revolt_converter::convert::Message>, game_dir: PathBuf) -> Self
    {
        Self { receiver, game_dir, progress: 0.0, heading: String::new(), step: "Starting".to_string(), lines: vec![], warnings: 0, errors: 0 }
    }

    // the messages of the converter thread - true when it is done
    fn poll(&mut self) -> bool
    {
        use revolt_converter::convert::{LogLevel, Message};

        let mut done = false;
        for message in self.receiver.try_iter()
        {
            match message
            {
                Message::Log(level, text) =>
                {
                    match level
                    {
                        LogLevel::Heading => { console_log!("convert: {}", text); self.heading = text.clone(); self.step = text.clone(); },
                        LogLevel::Step => { console_log!("convert:   {}", text); self.step = format!("{}  -  {}", self.heading, text); },
                        LogLevel::Warning => { console_warning!("convert: {}", text.trim()); self.warnings += 1; },
                        LogLevel::Error => { console_error!("convert: {}", text.trim()); self.errors += 1; },
                        // the details only into the log file
                        LogLevel::Info => {},
                    }
                    let tag = match level { LogLevel::Heading => "#", LogLevel::Step => "-", LogLevel::Info => " ", LogLevel::Warning => "W", LogLevel::Error => "E" };
                    self.lines.push(format!("{tag} {text}"));
                },
                Message::Progress(value) => self.progress = value,
                Message::Done => done = true,
            }
        }
        done
    }
}

// what needs a conversion: no or old converted files, scenes of the project without their file
fn convert_reason(state: &State, info: Option<&AssetInfo>) -> Option<String>
{
    match info
    {
        None => return Some("First start: converting the files of the game".to_string()),
        Some(info) if info.outdated() => return Some(format!("The converted files are older (version {}) than this game (version {})", info.oldest.map_or("?".to_string(), |v| v.to_string()), crate::assets::ASSET_VERSION)),
        _ => {},
    }

    let missing: Vec<String> = state.scenes.iter()
        .filter(|scene| !scene.is_engine_internal() && !scene.is_loaded())
        .filter(|scene| scene.source.as_ref().is_some_and(|source| !exists(&source.origin_path)))
        .map(|scene| scene.name.clone())
        .collect();
    let shown: Vec<&str> = missing.iter().take(3).map(String::as_str).collect();
    let more = if missing.len() > shown.len() { ", ..." } else { "" };
    (!missing.is_empty()).then(|| format!("{} scenes are missing ({}{more})", missing.len(), shown.join(", ")))
}

// the folder on the disk: relative ones (exports) through the resource folders of the engine
fn disk_path(folder: &str) -> PathBuf
{
    let path = Path::new(folder);
    if path.is_absolute() { return path.to_path_buf(); }
    PathBuf::from(rustl::resources::resources::get_path(if folder.is_empty() { "." } else { folder }))
}

// a file can be written into <dir>/assets
#[cfg(not(target_arch = "wasm32"))]
fn writable(dir: &Path) -> bool
{
    let assets = dir.join("assets");
    if std::fs::create_dir_all(&assets).is_err() { return false; }
    let probe = assets.join(".write_test");
    let ok = std::fs::write(&probe, b"rustl-volt").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

fn quit(state: &mut State)
{
    state.stop_request = true;
}

// the conversion jobs - they run on a worker thread and report to the UI through a channel

pub mod bodies;
pub mod cars;
pub mod clouds;
pub mod collision;
pub mod countdown;
pub mod effects;
pub mod frontend;
pub mod geometry;
pub mod instances;
pub mod level;
pub mod lights;
pub mod mirror;
pub mod motion;
pub mod music;
pub mod objects;
pub mod playground;
pub mod scene;
pub mod skybox;
pub mod sounds;
pub mod start_car;
pub mod texture;
pub mod track;
pub mod version;
pub mod weapon_models;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

#[derive(Clone)]
pub struct Settings
{
    pub game_dir: PathBuf,
    // the rustl project: the scenes go here, the paths in them are relative to it
    pub project_dir: PathBuf,
    // the own files of the project (the logo) - the project folder, unless the output goes to another folder
    pub files_dir: PathBuf,
    pub out_dir: PathBuf,
    // re-volt units -> meters
    pub scale: f32,
    // KHR_materials_unlit - the levels are pre-lit through vertex colors
    pub unlit: bool,
    // pull the uvs half a texel inward - no bleeding from the neighbour tiles of the atlas
    pub uv_inset: bool,
    // list the written scene in the .project - only if it is not in there yet
    pub add_to_project: bool,
    // the car on the start position of every level scene, a folder in cars/
    pub start_car: String,
    // the level is converted mirrored (left / right, like the game's mirror mode) - set per level by the conversion
    pub mirrored: bool,
}

impl Settings
{
    // the values of the game: scenes into project_dir, files into its assets/, the project logo from there too
    pub fn defaults(game_dir: PathBuf, project_dir: PathBuf) -> Self
    {
        Self
        {
            game_dir,
            files_dir: project_dir.clone(),
            out_dir: project_dir.join("assets"),
            project_dir,
            scale: 0.01,
            unlit: true,
            uv_inset: true,
            add_to_project: true,
            start_car: "rc".to_string(),
            mirrored: false,
        }
    }

    // assets/levels/<level>, the mirrored world has its own folder <level>_mirrored
    pub fn level_dir(&self, level: &str) -> PathBuf
    {
        let name = if self.mirrored { format!("{}_{}", level.to_lowercase(), crate::revolt::MIRRORED) } else { level.to_lowercase() };
        self.out_dir.join("levels").join(name)
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum LogLevel
{
    // a level or a job
    Heading,
    // one step inside of it
    Step,
    Info,
    Warning,
    Error,
}

pub enum Message
{
    Log(LogLevel, String),
    Progress(f32),
    Done,
}

#[derive(Clone)]
pub struct Log
{
    sender: Sender<Message>,
    // shared by the clones - the counts of the run for convert.json
    errors: Arc<AtomicUsize>,
    warnings: Arc<AtomicUsize>,
}

impl Log
{
    pub fn new(sender: Sender<Message>) -> Self
    {
        Self { sender, errors: Arc::default(), warnings: Arc::default() }
    }

    pub fn heading(&self, text: impl Into<String>) { self.send(Message::Log(LogLevel::Heading, text.into())); }
    pub fn step(&self, text: impl Into<String>) { self.send(Message::Log(LogLevel::Step, text.into())); }
    pub fn info(&self, text: impl Into<String>) { self.send(Message::Log(LogLevel::Info, text.into())); }
    pub fn warn(&self, text: impl Into<String>) { self.warnings.fetch_add(1, Ordering::Relaxed); self.send(Message::Log(LogLevel::Warning, text.into())); }
    pub fn error(&self, text: impl Into<String>) { self.errors.fetch_add(1, Ordering::Relaxed); self.send(Message::Log(LogLevel::Error, text.into())); }
    pub fn progress(&self, value: f32) { self.send(Message::Progress(value)); }
    pub fn done(&self) { self.send(Message::Done); }
    pub fn errors(&self) -> usize { self.errors.load(Ordering::Relaxed) }
    pub fn warnings(&self) -> usize { self.warnings.load(Ordering::Relaxed) }

    pub fn read(&self, path: &Path) { self.info(format!("  read      {} ({})", path.display(), file_size(path))); }
    pub fn write(&self, path: &Path) { self.info(format!("  write     {} ({})", path.display(), file_size(path))); }

    // the UI may be gone already - nothing to do then
    fn send(&self, message: Message)
    {
        let _ = self.sender.send(message);
    }
}

fn file_size(path: &Path) -> String
{
    let bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    if bytes >= 1_000_000 { format!("{:.1} MB", bytes as f64 / 1e6) } else { format!("{:.1} KB", bytes as f64 / 1e3) }
}

pub fn run(settings: Settings, levels: Vec<String>, cars: Vec<String>, log: Log)
{
    let started = std::time::Instant::now();

    log.heading("Shared files");
    log.step("Env maps (gfx/*.bmp --> png)");
    if let Err(err) = texture::convert_env_maps(&settings, &log)
    {
        log.error(format!("env maps: {err:#}"));
    }
    log.step("HUD pages (ui/font.png of the project, gfx/fxpage2, spru --> png, black transparent)");
    if let Err(err) = texture::convert_hud_pages(&settings, &log)
    {
        log.error(format!("hud pages: {err:#}"));
    }
    log.step("Loading screens (gfx/loadlevel1-4, loadfront1-4 --> one png each)");
    if let Err(err) = texture::convert_loading_screens(&settings, &log)
    {
        log.error(format!("loading screens: {err:#}"));
    }
    log.step("Countdown (models/go3, go2, go1, gogo.m --> assets/models/countdown.json)");
    if let Err(err) = countdown::convert(&settings, &log)
    {
        log.error(format!("countdown: {err:#}"));
    }
    log.step("Weapon models (models/firework.m, ball.m + a spark star --> assets/models/*.glb)");
    if let Err(err) = weapon_models::convert(&settings, &log)
    {
        log.error(format!("weapon models: {err:#}"));
    }
    log.step("Sounds (wavs/**/*.wav --> assets/sounds/*.wav, 16 bit PCM)");
    if let Err(err) = sounds::convert_all(&settings, &log)
    {
        log.error(format!("sounds: {err:#}"));
    }
    log.step("Music (music/TrackNN.ogg or redbook/trackNN.ogg --> assets/music/trackNN.ogg)");
    if let Err(err) = music::copy_all(&settings, &log)
    {
        log.error(format!("music: {err:#}"));
    }

    // a level counts like 5 cars
    let total = (levels.len() * 5 + cars.len()).max(1) as f32;
    let mut converted_levels = vec![];
    for (i, level) in levels.iter().enumerate()
    {
        // 0..1 inside of the level -> the share of the level in the whole run
        let progress = |fraction: f32| log.progress((i as f32 + fraction.clamp(0.0, 1.0)) * 5.0 / total);
        progress(0.0);
        match level::convert(&settings, level, &log, &progress)
        {
            Ok(_) => converted_levels.push(level.clone()),
            Err(err) => log.error(format!("{level}: {err:#}")),
        }
    }

    let mut converted = vec![];
    let mut converted_cars = vec![];
    for (i, car) in cars.iter().enumerate()
    {
        log.progress((levels.len() * 5 + i) as f32 / total);
        match cars::convert(&settings, car, &log)
        {
            Ok(done) => { converted.push(done); converted_cars.push(car.clone()); },
            Err(err) => log.error(format!("car {car}: {err:#}")),
        }
    }
    if !converted.is_empty()
    {
        if let Err(err) = cars::write_showroom(&settings, &converted, &log)
        {
            log.error(format!("cars.scene: {err:#}"));
        }
    }

    if converted_levels.iter().any(|level| level.eq_ignore_ascii_case(playground::LEVEL)) && !converted.is_empty()
    {
        log.step("Physics test (physics_test.scene: the showroom floor, the start car of markar, the physics objects of the levels)");
        playground::write_tests(&settings, &converted_levels, &log);
    }

    log.step(format!("Asset version (--> assets/{})", version::FILE));
    let run_info = version::RunInfo
    {
        levels: &converted_levels,
        cars: &converted_cars,
        seconds: started.elapsed().as_secs_f32(),
        errors: log.errors.load(Ordering::Relaxed),
        warnings: log.warnings.load(Ordering::Relaxed),
    };
    if let Err(err) = version::write(&settings, &run_info, &log)
    {
        log.error(format!("{}: {err:#}", version::FILE));
    }

    log.progress(1.0);
    log.heading("Done");
    log.info(format!("  {} level(s), {} car(s) in {:.1} s", levels.len(), converted.len(), started.elapsed().as_secs_f32()));
    log.send(Message::Done);
}

// the menu of the game in the frontend level: title, main menu, select race, enter name (the wheel), select car (the boxes), select track (the big screen), options,
// select cup + its summary (championship), best trial times, progress table
// flow, views and colors like revolt.exe (formats/README.md, Frontend), drawn with the HUD painter in the 640 x 480 space of the game

use std::collections::HashMap;

use rustl::egui::{self, Color32, TextureHandle, TextureOptions};
use rustl::input::keyboard::{Key, Keyboard};
use rustl::prelude::*;
use rustl::resources::resources::load_binary;
use rustl::serde_json::{self, Value};

use crate::assist::DriveMode;
use crate::bot::Difficulty;
use crate::frontend::{self, Target, WHEEL_LETTERS};
use crate::hud::{Anchor, Painter, HEIGHT, WIDTH};
use crate::pad::Buttons;
use crate::cup::{CUPS, LEVELS};
use crate::race::{format_time, RaceSettings};
use crate::save::{SaveData, TRIAL_TIMES};

// the cars of the game in the order of their ids - the id picks the picture of the car box (the ones from 28 have none, they get the plain box)
const CAR_IDS: [&str; 35] =
[
    "rc", "mite", "phat", "moss", "mud", "beatall", "volken", "tc6", "dino", "candy", "gencar", "tc4", "mouse", "flag", "tc2", "r5", "tc5", "sgt",
    "tc3", "adeon", "fone", "tc1", "rotor", "cougar", "sugo", "toyeca", "amw", "panga", "trolley", "wincar", "wincar2", "wincar3", "wincar4", "ufo", "q",
];
const BOXED_CARS: usize = 28;

// the cup screens look at the trophies (6-9), the summary, best trial times and the progress table have their own views
const VIEW_CUPS: usize = 6;
const VIEW_PROGRESS: usize = 10;
const VIEW_TRIAL_TIMES: usize = 15;
const VIEW_SUMMARY: usize = 17;

// the colors of the menu items: normal, selected, not available, selected and not available
const ITEM: Color32 = Color32::WHITE;
const SELECTED: Color32 = Color32::from_rgb(0xff, 0x77, 0x22);
const DISABLED: Color32 = Color32::from_rgb(0x44, 0x44, 0x44);
const SELECTED_DISABLED: Color32 = Color32::from_rgb(0x80, 0x30, 0x10);
const NAME: Color32 = Color32::from_rgb(0xff, 0xff, 0x00);
// the stars of the stunt arena: collected gold, the others dark
const STAR_GOLD: [Color32; 3] = [Color32::from_rgb(0xff, 0xfa, 0xc8), Color32::from_rgb(0xff, 0xb8, 0x00), Color32::from_rgb(0x80, 0x48, 0x00)];
const STAR_DARK: [Color32; 3] = [Color32::from_rgba_premultiplied(0x30, 0x30, 0x38, 0xc0), Color32::from_rgba_premultiplied(0x14, 0x14, 0x1a, 0xc0), Color32::from_rgb(0x60, 0x60, 0x70)];
const STAR_STEP: f32 = 22.0;
const STAR_RADIUS: f32 = 9.0;
const HINT: Color32 = Color32::from_rgb(0x88, 0x88, 0x88);
// the boxes behind titles and items: dark, rounded, a red rim; the tile of the title icon dark red
const BOX: Color32 = Color32::from_rgba_premultiplied(14, 14, 16, 0xc0);
const RIM: Color32 = Color32::from_rgb(0xc0, 0x30, 0x28);
const ICON_TILE: Color32 = Color32::from_rgba_premultiplied(0x2c, 0x08, 0x06, 0xd8);
const RIM_WIDTH: f32 = 2.5;
const BOX_RADIUS: f32 = 8.0;
// the value bars of the cars: red (low) to green (high)
const BAR_LOW: Color32 = Color32::from_rgb(0xff, 0x30, 0x20);
const BAR_HIGH: Color32 = Color32::from_rgb(0x40, 0xe0, 0x30);
const BAR_WIDTH: f32 = 96.0;

// meshes of the painter: the font first (the painter writes text into mesh 0), drawn last
const FONT: usize = 0;
const FILL: usize = 1;
const LOGO: usize = 2;
const ICONS: usize = 3;
// the loading screen has no icons - its picture takes their mesh
const PICTURE: usize = 3;
const DRAW_ORDER: [usize; 4] = [FILL, LOGO, ICONS, FONT];

// the icons of the titles on frontendf.bmp: 5 per row, 48 texels
const ICON_SIZE: f32 = 48.0;
const ICON_WATCH: usize = 0;
const ICON_CAR: usize = 1;
const ICON_CUP: usize = 3;
const ICON_PROGRESS: usize = 5;
const ICON_TRACK: usize = 2;
const ICON_RACE: usize = 4;
const ICON_OPTIONS: usize = 7;
const ICON_NAME: usize = 8;

// the logo of the project (rustl-volt.png with the checkerboard cut out), its width / height
pub const LOGO_FILE: &str = "rustl-volt_alpha.png";
pub const LOGO_ASPECT: f32 = 1024.0 / 239.0;
// the logo fades in like the one of the game (0.5 s)
const LOGO_FADE: f32 = 2.0;

// rows of 16, chars 8 x 12 - the items of the game
const ROW: f32 = 16.0;
const CHAR_W: f32 = 8.0;
const CHAR_H: f32 = 12.0;

// the loading screen of a race (x, y, w, h of the 640 x 480 screen, measured on a screenshot of the game): the framed picture, the bar below
pub const LOADING_FRAME: [f32; 4] = [10.0, 7.0, 620.0, 408.0];
pub const LOADING_BAR: [f32; 4] = [10.0, 434.0, 620.0, 38.0];
pub const LOADING_INSET: f32 = 4.0;
pub const LOADING_RADIUS: f32 = 8.0;
pub const LOADING_RIM_WIDTH: f32 = 2.0;
pub const LOADING_RIM: Color32 = Color32::from_rgb(0x30, 0x44, 0xd8);
pub const LOADING_FILL: Color32 = Color32::from_rgb(0x0c, 0x0c, 0x14);
pub const LOADING_PROGRESS: Color32 = Color32::from_rgb(0x1c, 0x2c, 0x90);
pub const LOADING_LOGO_Y: f32 = 36.0;
pub const LOADING_LOGO_WIDTH: f32 = 384.0;
// the rows: labels end at LOADING_LABEL_END, values start at LOADING_VALUE_X, chars w x h
const LOADING_ROWS_Y: f32 = 158.0;
const LOADING_ROW_STEP: f32 = 33.0;
const LOADING_LABEL_END: f32 = 312.0;
const LOADING_VALUE_X: f32 = 328.0;
const LOADING_CHAR: [f32; 2] = [8.0, 16.0];
pub const LOADING_FILE_CHARS: usize = 70;
const LOADING_PICTURE_FILE: &str = "loading_level.png";

// the menu sounds play at 44100 Hz, their wavs have 22050 Hz
pub const MENU_SOUND_SPEED: f32 = 2.0;

// the keys typed into the name
const TYPED_KEYS: [(Key, char); 37] =
[
    (Key::A, 'A'), (Key::B, 'B'), (Key::C, 'C'), (Key::D, 'D'), (Key::E, 'E'), (Key::F, 'F'), (Key::G, 'G'), (Key::H, 'H'), (Key::I, 'I'), (Key::J, 'J'), (Key::K, 'K'), (Key::L, 'L'), (Key::M, 'M'),
    (Key::N, 'N'), (Key::O, 'O'), (Key::P, 'P'), (Key::Q, 'Q'), (Key::R, 'R'), (Key::S, 'S'), (Key::T, 'T'), (Key::U, 'U'), (Key::V, 'V'), (Key::W, 'W'), (Key::X, 'X'), (Key::Y, 'Y'), (Key::Z, 'Z'),
    (Key::Key1, '1'), (Key::Key2, '2'), (Key::Key3, '3'), (Key::Key4, '4'), (Key::Key5, '5'), (Key::Key6, '6'), (Key::Key7, '7'), (Key::Key8, '8'), (Key::Key9, '9'), (Key::Key0, '0'), (Key::Space, ' '),
];

// s the chosen car is shown before the track menu (the box lands in 1 s, the car comes out), Enter goes on after the first
const CHOSEN_TIME: f32 = 4.5;
const CHOSEN_SKIP: f32 = 1.5;
// the longest name
const NAME_LENGTH: usize = 14;
const LETTER_DELETE: usize = 28;
const LETTER_END: usize = 29;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Screen
{
    Title,
    Main,
    SelectRace,
    SelectMode,
    EnterName,
    SelectCar,
    CarChosen,
    SelectTrack,
    Options,
    GameSettings,
    AudioSettings,
    VideoSettings,
    Controls,
    Quit,
    SelectCup,
    CupSummary,
    TrialTimes,
    Progress,
}

#[derive(Clone, Copy, PartialEq)]
enum Mode
{
    Championship,
    SingleRace,
    TimeTrial,
    Practice,
    StuntArena,
}

pub struct CarInfo
{
    pub id: String,
    // the car box (-1 the plain one)
    pub box_id: i32,
    pub name: String,
    class: i64,
    rating: i64,
    // mph
    speed: f32,
    acc: f32,
    weight: f32,
    trans: i64,
}

pub struct TrackInfo
{
    pub level: String,
    pub name: String,
    pub scene_id: u32,
    // the scenes of the reversed, mirrored and reversed mirrored track, if converted
    pub reversed: Option<u32>,
    pub mirrored: Option<u32>,
    pub reversed_mirrored: Option<u32>,
    // m of the game
    pub length: f32,
}

#[derive(Clone)]
pub struct RaceChoice
{
    pub scene_id: u32,
    pub car: String,
    pub player_name: String,
    pub settings: RaceSettings,
    // the stunt arena: free driving, no race
    pub stunts: bool,
    // a race of the championship: stage and stages of the cup (the loading screen)
    pub cup_stage: Option<(usize, usize)>,
}

#[derive(Clone)]
pub struct ChampionshipChoice
{
    pub cup: usize,
    pub car: String,
    pub player_name: String,
    pub settings: RaceSettings,
}

impl TrackInfo
{
    // the scene of a direction, None if that one is not converted
    pub fn variant(&self, reversed: bool, mirrored: bool) -> Option<u32>
    {
        match (reversed, mirrored) { (false, false) => Some(self.scene_id), (true, false) => self.reversed, (false, true) => self.mirrored, (true, true) => self.reversed_mirrored }
    }

    // reversed and mirrored of one of its scenes
    pub fn direction_of(&self, scene_id: u32) -> Option<(bool, bool)>
    {
        [(false, false), (true, false), (false, true), (true, true)].into_iter().find(|(r, m)| self.variant(*r, *m) == Some(scene_id))
    }
}

// <level>[_reversed][_mirrored] - the key of the records
pub fn variant_key(level: &str, reversed: bool, mirrored: bool) -> String
{
    crate::cup::scene_name(level, reversed, mirrored)
}

// " R", " M", " R M" behind the track name
fn variant_tag(reversed: bool, mirrored: bool) -> &'static str
{
    match (reversed, mirrored) { (false, false) => "", (true, false) => " R", (false, true) => " M", (true, true) => " R M" }
}

fn direction_name(reversed: bool, mirrored: bool) -> &'static str
{
    match (reversed, mirrored) { (false, false) => "Normal", (true, false) => "Reversed", (false, true) => "Mirrored", (true, true) => "Reversed Mirrored" }
}

pub enum MenuEvent
{
    // the stunt arena: car, player name
    Stunts(String, String),
    // name and speed
    Sound(&'static str, f32),
    Start(RaceChoice),
    Championship(ChampionshipChoice),
    Quit,
}

struct Textures
{
    font: TextureHandle,
    icons: Option<TextureHandle>,
    logo: Option<TextureHandle>,
    // the blue picture of the loading screen (gfx/loadlevel1-4.bmp)
    loading: Option<TextureHandle>,
}

struct Input
{
    up: bool,
    down: bool,
    left: bool,
    right: bool,
    enter: bool,
    escape: bool,
    backspace: bool,
    text: String,
}

// the audio options, 0..=AUDIO_STEPS each - the app sets the volumes of the audio device from them
#[derive(Clone, Copy, PartialEq)]
pub struct AudioSettings
{
    pub master: u32,
    pub music: u32,
    pub effects: u32,
}

pub const AUDIO_STEPS: u32 = 10;

// the video options - fullscreen only in exports (the editor has its own, F)
#[derive(Clone, Copy, PartialEq)]
pub struct VideoSettings
{
    pub fullscreen: bool,
    pub show_fps: bool,
}

impl Default for VideoSettings
{
    fn default() -> Self { Self { fullscreen: false, show_fps: true } }
}

// the keys of the Controller Settings screen: action, keyboard, gamepad
const CONTROLS: [(&str, &str, &str); 13] =
[
    ("Accelerate", "W / Up", "RT"),
    ("Brake / Reverse", "S / Down", "LT"),
    ("Steer", "A D / Left Right", "Left Stick"),
    ("Handbrake", "Space", "A"),
    ("Fire Weapon", "Ctrl", "X / RB"),
    ("Reset Car", "Backspace / R", "Back / Y"),
    ("Camera", "C", "Right Stick Click"),
    ("Look Around", "", "Right Stick"),
    ("Watch Others", "B", "LB"),
    ("Split View", "V", ""),
    ("Pause Menu", "Enter / Esc", "Start"),
    ("Fullscreen", "F", ""),
    ("Menu", "Arrows Enter Bksp", "D-Pad A B"),
];

impl Default for AudioSettings
{
    fn default() -> Self { Self { master: AUDIO_STEPS, music: AUDIO_STEPS, effects: AUDIO_STEPS } }
}

pub struct Menu
{
    screen: Screen,
    // the selected item per screen
    cursor: [usize; 20],
    mode: Mode,
    pub settings: RaceSettings,
    pub audio: AudioSettings,
    pub video: VideoSettings,
    name: String,
    letter: usize,
    car: usize,
    track: usize,
    reversed: bool,
    mirrored: bool,
    // best trial times: its track and direction
    trial_track: usize,
    trial_reversed: bool,
    trial_mirrored: bool,
    // a message over the main menu (the end of a cup), Enter closes it
    message: Vec<String>,
    // s in the current screen, s of the menu
    screen_time: f32,
    clock: f32,
    events: Vec<MenuEvent>,
    // the project folder (logo) and its assets
    folder: String,
    // "Rustl-Volt v0.0.1" bottom right
    pub version: String,
    // the saved records and stars - shown in the track panel and the stunt arena
    pub saved: SaveData,
    textures: Option<Textures>,
    failed: bool,
}

impl Menu
{
    pub fn new(folder: &str) -> Self
    {
        Self
        {
            screen: Screen::Title, cursor: [0; 20], mode: Mode::SingleRace, settings: RaceSettings::default(), audio: AudioSettings::default(), video: VideoSettings::default(),
            name: String::new(), letter: 0, car: 0, track: 0, reversed: false, mirrored: false, trial_track: 0, trial_reversed: false, trial_mirrored: false,
            message: vec![], screen_time: 0.0, clock: 0.0, events: vec![],
            folder: folder.to_string(), version: String::new(), saved: SaveData::default(), textures: None, failed: false,
        }
    }

    // the name, settings and audio options of the save, its records for the panels
    pub fn restore(&mut self, saved: &SaveData)
    {
        self.name = saved.name.chars().take(NAME_LENGTH).collect();
        self.settings = saved.race_settings(self.settings);
        self.audio = saved.audio_settings();
        self.video = saved.video_settings();
        self.saved = saved.clone();
    }

    // name, settings and audio options into the save - true if they changed
    pub fn store(&self, saved: &mut SaveData) -> bool
    {
        let before = saved.clone();
        saved.name = self.name.clone();
        saved.set_race_settings(&self.settings);
        saved.set_audio_settings(&self.audio);
        saved.set_video_settings(&self.video);
        *saved != before
    }

    pub fn take_events(&mut self) -> Vec<MenuEvent>
    {
        std::mem::take(&mut self.events)
    }

    // the next track of the race menu - the track menu shows it next time
    pub fn set_track(&mut self, index: usize)
    {
        self.track = index;
        self.reversed = false;
        self.mirrored = false;
    }

    // after a race: the main menu
    pub fn back_from_race(&mut self)
    {
        self.go(Screen::Main);
    }

    // after a cup: the main menu with its message
    pub fn show_message(&mut self, lines: Vec<String>)
    {
        self.go(Screen::Main);
        self.message = lines;
    }

    // the 3D part of the current screen
    pub fn target(&self, cars: &[CarInfo], tracks: &[TrackInfo]) -> Target
    {
        let car = cars.get(self.car);
        let view = match self.screen
        {
            Screen::Title => frontend::VIEW_TITLE,
            Screen::Main | Screen::Quit => frontend::VIEW_MAIN,
            Screen::SelectRace | Screen::SelectMode => frontend::VIEW_SELECT_RACE,
            Screen::EnterName => frontend::VIEW_NAME,
            Screen::SelectCar => frontend::VIEW_CAR_BOXES,
            Screen::CarChosen => frontend::VIEW_CAR_FLOOR,
            Screen::SelectTrack => frontend::VIEW_TRACK_SCREEN,
            Screen::Options | Screen::GameSettings | Screen::AudioSettings | Screen::VideoSettings | Screen::Controls => frontend::VIEW_OPTIONS,
            Screen::SelectCup => VIEW_CUPS + available(&self.items(), self.cursor[Screen::SelectCup as usize], true).min(3),
            Screen::CupSummary => VIEW_SUMMARY,
            Screen::TrialTimes => VIEW_TRIAL_TIMES,
            Screen::Progress => VIEW_PROGRESS,
        };
        Target
        {
            view,
            presented: car.filter(|_| self.screen == Screen::SelectCar).map(|car| car.box_id),
            landing: car.filter(|_| self.screen == Screen::CarChosen).map(|car| (car.box_id, car.id.clone())),
            letter: self.letter,
            picture: tracks.get(self.track).map(|track| track.level.clone()),
        }
    }

    // the stars of the stunt arena: a row at the top, the collected ones gold, the count below
    pub fn stunt_ui(&mut self, ui: &mut egui::Ui, collected: &[bool], menu_open: bool)
    {
        let Some(textures) = &self.textures else { return; };
        let mut painter = Painter::new(ui.ctx().content_rect(), vec![textures.font.id(), egui::TextureId::default()]);
        let total = collected.len();
        let count = collected.iter().filter(|c| **c).count();
        let width = total as f32 * STAR_STEP;
        let left = (WIDTH - width) * 0.5;
        painter.blue_box(Anchor::Center, [left - 10.0, 10.0, width + 20.0, 46.0]);
        for (i, done) in collected.iter().enumerate()
        {
            let [inner, outer, rim] = if *done { STAR_GOLD } else { STAR_DARK };
            painter.star(Anchor::Center, [left + (i as f32 + 0.5) * STAR_STEP, 28.0], STAR_RADIUS, inner, outer, rim);
        }
        let text = format!("{count}/{total}");
        painter.text(Anchor::Center, (WIDTH - text.len() as f32 * CHAR_W) * 0.5, 40.0, CHAR_W, CHAR_H, if count == total { NAME } else { ITEM }, &text);
        if !menu_open { centered(&mut painter, 460.0, CHAR_W, CHAR_H, HINT, "Enter / Esc: menu"); }
        painter.version(&self.version);
        painter.finish(ui.ctx(), &[FILL, FONT]);
    }

    // while the level of a race loads, like the game: the blue picture in a frame, the logo, what was chosen - below a bar with the file loaded right now
    pub fn loading_ui(&mut self, ui: &mut egui::Ui, choice: &RaceChoice, cars: &[CarInfo], tracks: &[TrackInfo], progress: f32, file: &str)
    {
        if self.textures.is_none() && !self.failed
        {
            match self.load(ui.ctx())
            {
                Ok(textures) => self.textures = Some(textures),
                Err(error) => { console_warning!("menu: {} - re-convert for the menu pages", error); self.failed = true; },
            }
        }
        let Some(textures) = &self.textures else { return; };
        let white = egui::TextureId::default();
        let mut painter = Painter::new(ui.ctx().content_rect(), vec![textures.font.id(), white, textures.logo.as_ref().map_or(white, |t| t.id()), textures.loading.as_ref().map_or(white, |t| t.id())]);

        let [x, y, w, h] = LOADING_FRAME;
        painter.rounded(Anchor::Center, LOADING_FRAME, LOADING_FILL, LOADING_RIM, LOADING_RIM_WIDTH, LOADING_RADIUS);
        if textures.loading.is_some()
        {
            painter.image(PICTURE, Anchor::Center, [x + LOADING_INSET, y + LOADING_INSET, w - 2.0 * LOADING_INSET, h - 2.0 * LOADING_INSET], Color32::WHITE);
        }
        if textures.logo.is_some() { logo(&mut painter, LOADING_LOGO_Y, LOADING_LOGO_WIDTH, 1.0); }

        for (i, (label, value)) in self.loading_rows(choice, cars, tracks).iter().enumerate()
        {
            let row_y = LOADING_ROWS_Y + i as f32 * LOADING_ROW_STEP;
            painter.text(Anchor::Center, LOADING_LABEL_END - label.len() as f32 * LOADING_CHAR[0], row_y, LOADING_CHAR[0], LOADING_CHAR[1], ITEM, label);
            painter.text(Anchor::Center, LOADING_VALUE_X, row_y, LOADING_CHAR[0], LOADING_CHAR[1], crate::hud::CYAN, value);
        }

        let [x, y, w, h] = LOADING_BAR;
        painter.rounded(Anchor::Center, LOADING_BAR, LOADING_FILL, LOADING_RIM, LOADING_RIM_WIDTH, LOADING_RADIUS);
        let fraction = progress.clamp(0.0, 1.0);
        let inner = [x + LOADING_INSET, y + LOADING_INSET, (w - 2.0 * LOADING_INSET) * fraction, h - 2.0 * LOADING_INSET];
        painter.gradient(FILL, Anchor::Center, inner, LOADING_PROGRESS, LOADING_PROGRESS);

        // the end of a long path - the bar holds LOADING_FILE_CHARS
        let skip = file.chars().count().saturating_sub(LOADING_FILE_CHARS);
        let file: String = file.chars().skip(skip).collect();
        centered(&mut painter, y + (h - LOADING_CHAR[1]) * 0.5, LOADING_CHAR[0], LOADING_CHAR[1], ITEM, &file);
        painter.finish(ui.ctx(), &[PICTURE, FILL, LOGO, FONT]);
    }

    // Game Mode, Track, Length, Laps, Car, Won Race - the rows that fit the mode
    fn loading_rows(&self, choice: &RaceChoice, cars: &[CarInfo], tracks: &[TrackInfo]) -> Vec<(&'static str, String)>
    {
        let track = tracks.iter().find(|track| track.direction_of(choice.scene_id).is_some());
        let (reversed, mirrored) = track.and_then(|track| track.direction_of(choice.scene_id)).unwrap_or((false, false));
        let mode = if choice.stunts { Mode::StuntArena } else if choice.cup_stage.is_some() { Mode::Championship } else { self.mode };
        let mode_name = match mode { Mode::Championship => "Championship", Mode::SingleRace => "Single Race", Mode::TimeTrial => "Time Trial", Mode::Practice => "Practice", Mode::StuntArena => "Stunt Arena" };

        let mut rows = vec![("Game Mode:", mode_name.to_string())];
        if let Some((stage, stages)) = choice.cup_stage
        {
            rows.push(("Stage:", format!("{} of {stages}", stage + 1)));
        }
        if let Some(track) = track
        {
            rows.push(("Track:", format!("{}{}", track.name, variant_tag(reversed, mirrored))));
            rows.push(("Length:", format!("{:.0}m", track.length)));
        }
        else if mode == Mode::StuntArena
        {
            rows.push(("Track:", "Stunt Arena".to_string()));
        }
        if matches!(mode, Mode::SingleRace | Mode::TimeTrial | Mode::Championship)
        {
            rows.push(("Laps:", choice.settings.laps.to_string()));
        }
        let car = cars.iter().find(|car| car.id == choice.car).map_or_else(|| choice.car.clone(), |car| car.name.clone());
        rows.push(("Car:", car));
        if mode == Mode::SingleRace
        {
            let level = track.map(|track| variant_key(&track.level, reversed, mirrored)).unwrap_or_default();
            rows.push(("Won Race:", if self.saved.won(&level) { "Yes" } else { "No" }.to_string()));
        }
        rows
    }

    // the keys of the game input (not egui - the editor in Play does not hand egui every key), the timed steps
    pub fn update(&mut self, keyboard: &mut Keyboard, pad: &Buttons, cars: &[CarInfo], tracks: &[TrackInfo], dt: f32)
    {
        self.clock += dt;
        self.screen_time += dt;
        let enter = pad.enter || pad.start;
        let skip = self.screen == Screen::CarChosen && self.screen_time >= CHOSEN_SKIP && enter;
        if self.screen == Screen::CarChosen && (self.screen_time >= CHOSEN_TIME || skip)
        {
            match self.mode
            {
                // the stunt arena has no track menu: straight into the stunts level
                Mode::StuntArena =>
                {
                    self.go(Screen::SelectCar);
                    let car = cars.get(self.car).map_or("rc".to_string(), |car| car.id.clone());
                    self.events.push(MenuEvent::Stunts(car, self.player_name()));
                },
                Mode::Championship => self.go(Screen::SelectCup),
                _ => self.go(Screen::SelectTrack),
            }
            // the Enter of this frame is used up
            return;
        }

        // typing only on the name screen and the title - the other keys of the keyboard are not the menu's
        let typing = matches!(self.screen, Screen::EnterName | Screen::Title);
        let text: String = if typing { TYPED_KEYS.iter().filter(|(key, _)| keyboard.is_pressed(*key)).map(|(_, c)| *c).collect() } else { String::new() };
        // Esc stays the keyboard's - the pad goes back with B (Backspace)
        let input = Input
        {
            up: pad.up,
            down: pad.down,
            left: pad.left,
            right: pad.right,
            enter,
            escape: pad.pause && !pad.start,
            backspace: pad.back,
            text,
        };
        self.handle(&input, cars, tracks);
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, cars: &[CarInfo], tracks: &[TrackInfo])
    {
        if self.textures.is_none() && !self.failed
        {
            match self.load(ui.ctx())
            {
                Ok(textures) => self.textures = Some(textures),
                Err(error) => { console_warning!("menu: {} - re-convert for the menu pages", error); self.failed = true; },
            }
        }
        let Some(textures) = &self.textures else { return; };
        let white = egui::TextureId::default();
        let mut painter = Painter::new(ui.ctx().content_rect(), vec![textures.font.id(), white, textures.logo.as_ref().map_or(white, |t| t.id()), textures.icons.as_ref().map_or(white, |t| t.id())]);
        let has_logo = textures.logo.is_some();
        let has_icons = textures.icons.is_some();
        self.draw(&mut painter, cars, tracks, has_logo, has_icons);
        painter.version(&self.version);
        painter.finish(ui.ctx(), &DRAW_ORDER);
    }

    // Esc and Backspace go back (the editor ends Play on Esc) - on the name screen Backspace deletes first
    fn handle(&mut self, input: &Input, cars: &[CarInfo], tracks: &[TrackInfo])
    {
        let back = input.escape || input.backspace;
        // the message of a cup over the main menu: Enter or back closes it
        if !self.message.is_empty()
        {
            if input.enter || back { self.message.clear(); self.sound("menunext"); }
            return;
        }
        match self.screen
        {
            Screen::Title =>
            {
                if input.enter || !input.text.is_empty() { self.sound("menunext"); self.go(Screen::Main); }
            },
            Screen::EnterName => self.handle_name(input),
            Screen::SelectCar =>
            {
                if cars.is_empty() { if back { self.back(); } return; }
                if input.left || input.right
                {
                    self.car = step(self.car, cars.len(), input.right);
                    self.sound("boxslide");
                }
                if input.enter { self.sound("menunext"); self.go(Screen::CarChosen); }
                else if back { self.back(); }
            },
            Screen::CarChosen => if back { self.back(); },
            Screen::SelectTrack =>
            {
                if tracks.is_empty() { if back { self.back(); } return; }
                if input.left || input.right
                {
                    self.track = step(self.track, tracks.len(), input.right);
                    self.reversed = false;
                    self.mirrored = false;
                    self.sound("tvstatic");
                }
                // like the game: up turns mirrored on and off, down reversed
                if let Some(track) = tracks.get(self.track).filter(|_| input.up || input.down)
                {
                    let (reversed, mirrored) = if input.up { (self.reversed, !self.mirrored) } else { (!self.reversed, self.mirrored) };
                    if track.variant(reversed, mirrored).is_some()
                    {
                        (self.reversed, self.mirrored) = (reversed, mirrored);
                        self.sound("menuupdown");
                    }
                }
                if input.enter
                {
                    let track = &tracks[self.track];
                    let car = cars.get(self.car).map_or("rc".to_string(), |car| car.id.clone());
                    let mut settings = self.settings;
                    // time trial and practice: alone, no pickups - practice never ends
                    if matches!(self.mode, Mode::TimeTrial | Mode::Practice)
                    {
                        settings.cars = 1;
                        settings.pickups = false;
                    }
                    settings.practice = self.mode == Mode::Practice;
                    settings.trial = self.mode == Mode::TimeTrial;
                    self.sound("menunext");
                    self.events.push(MenuEvent::Start(RaceChoice
                    {
                        scene_id: track.variant(self.reversed, self.mirrored).unwrap_or(track.scene_id),
                        car,
                        player_name: self.player_name(),
                        settings,
                        stunts: false,
                        cup_stage: None,
                    }));
                }
                else if back { self.back(); }
            },
            Screen::CupSummary =>
            {
                if input.enter
                {
                    let car = cars.get(self.car).map_or("rc".to_string(), |car| car.id.clone());
                    let cup = self.cursor[Screen::SelectCup as usize].min(CUPS.len() - 1);
                    self.sound("menunext");
                    self.events.push(MenuEvent::Championship(ChampionshipChoice { cup, car, player_name: self.player_name(), settings: self.settings }));
                }
                else if back { self.back(); }
            },
            Screen::TrialTimes =>
            {
                if !tracks.is_empty() && (input.left || input.right)
                {
                    self.trial_track = step(self.trial_track.min(tracks.len() - 1), tracks.len(), input.right);
                    (self.trial_reversed, self.trial_mirrored) = (false, false);
                    self.sound("menuleftright");
                }
                if input.up || input.down
                {
                    let (reversed, mirrored) = if input.up { (self.trial_reversed, !self.trial_mirrored) } else { (!self.trial_reversed, self.trial_mirrored) };
                    (self.trial_reversed, self.trial_mirrored) = (reversed, mirrored);
                    self.sound("menuupdown");
                }
                if back || input.enter { self.back(); }
            },
            Screen::Progress | Screen::Controls => if back || input.enter { self.back(); },
            _ =>
            {
                let items = self.items();
                if items.is_empty() { return; }
                let cursor = available(&items, self.cursor[self.screen as usize], true);
                if input.up || input.down
                {
                    self.cursor[self.screen as usize] = available(&items, step(cursor, items.len(), input.down), input.down);
                    self.sound("menuupdown");
                }
                if (input.left || input.right) && self.screen == Screen::GameSettings
                {
                    self.change_setting(cursor, input.right);
                    self.sound("menuleftright");
                }
                if (input.left || input.right) && self.screen == Screen::AudioSettings
                {
                    self.change_audio(cursor, input.right);
                    self.sound("menuleftright");
                }
                if (input.left || input.right) && self.screen == Screen::VideoSettings
                {
                    self.toggle_video(cursor);
                    self.sound("menuleftright");
                }
                if input.enter && items[cursor].1
                {
                    self.sound("menunext");
                    self.select(cursor);
                }
                else if back { self.back(); }
            },
        }
    }

    // left / right turn the wheel, up takes the letter (DEL deletes, END is done), down / backspace delete, enter is done; typing works too
    fn handle_name(&mut self, input: &Input)
    {
        if input.escape || (input.backspace && self.name.is_empty()) { self.back(); return; }
        if input.left || input.right
        {
            self.letter = step(self.letter, WHEEL_LETTERS, input.right);
            if self.name.chars().count() >= NAME_LENGTH { self.letter = LETTER_END; }
            self.sound("menuleftright");
        }
        for c in input.text.chars().filter(|c| c.is_ascii_alphanumeric() || *c == ' ' || *c == '!')
        {
            if self.name.chars().count() < NAME_LENGTH { self.name.push(c.to_ascii_uppercase()); }
            if let Some(index) = letter_index(c) { self.letter = index; }
        }
        if input.backspace || input.down
        {
            self.name.pop();
            self.sound("menuprev");
        }
        // Enter: the name is done (the END of the wheel does the same)
        if input.enter
        {
            self.sound("menunext");
            self.go(Screen::SelectCar);
            return;
        }
        // up takes the letter of the wheel
        if input.up
        {
            match self.letter
            {
                LETTER_END => { self.sound("menunext"); self.go(Screen::SelectCar); },
                LETTER_DELETE => { self.name.pop(); self.sound("menuprev"); },
                index =>
                {
                    if self.name.chars().count() < NAME_LENGTH
                    {
                        self.name.push(letter_char(index));
                        self.sound("menuleftright");
                    }
                    if self.name.chars().count() >= NAME_LENGTH { self.letter = LETTER_END; }
                },
            }
        }
    }

    fn player_name(&self) -> String
    {
        let name = self.name.trim();
        if name.is_empty() { "Player 1".to_string() } else { name.to_string() }
    }

    // label and available
    fn items(&self) -> Vec<(String, bool)>
    {
        let item = |label: &str, available: bool| (label.to_string(), available);
        match self.screen
        {
            Screen::Main => vec![item("Start Race", true), item("Best Trial Times", true), item("Progress Table", true), item("Options", true), item("Quit", true)],
            Screen::SelectMode => DriveMode::ALL.iter().map(|mode| item(mode.label(), true)).collect(),
            Screen::SelectCup => CUPS.iter().enumerate().map(|(i, cup)| item(cup.name, self.saved.cup_open(i))).collect(),
            Screen::SelectRace => vec![item("Single Race", true), item("Championship", true), item("Clockwork Carnage", false), item("Time Trial", true), item("Practice", true), item("Stunt Arena", true), item("Multi-Player", false)],
            Screen::Options => vec![item("Game Settings", true), item("Video Settings", true), item("Audio Settings", true), item("Controller Settings", true), item("View Credits", false)],
            Screen::VideoSettings => vec![item("Fullscreen", true), item("Show FPS", true)],
            Screen::GameSettings => vec![item("Number of Cars", true), item("Number of Laps", true), item("Difficulty", true), item("Pick ups", true), item("Speed Units", true)],
            Screen::AudioSettings => vec![item("Master Volume", true), item("Music Volume", true), item("Effects Volume", true)],
            Screen::Quit => vec![item("Yes", true), item("No", true)],
            _ => vec![],
        }
    }

    fn setting_value(&self, index: usize) -> String
    {
        match index
        {
            0 => self.settings.cars.to_string(),
            1 => self.settings.laps.to_string(),
            2 => self.settings.difficulty.label().to_string(),
            3 => if self.settings.pickups { "On" } else { "Off" }.to_string(),
            _ => if self.settings.kmh { "km/h" } else { "mph" }.to_string(),
        }
    }

    // the ranges of the game: 2-12 cars, 1-20 laps
    fn change_setting(&mut self, index: usize, up: bool)
    {
        let settings = &mut self.settings;
        match index
        {
            0 => settings.cars = if up { (settings.cars + 1).min(12) } else { settings.cars.saturating_sub(1).max(2) },
            1 => settings.laps = if up { (settings.laps + 1).min(20) } else { settings.laps.saturating_sub(1).max(1) },
            2 =>
            {
                let all = Difficulty::ALL;
                let current = all.iter().position(|d| *d == settings.difficulty).unwrap_or(2);
                settings.difficulty = all[if up { (current + 1).min(all.len() - 1) } else { current.saturating_sub(1) }];
            },
            3 => settings.pickups = !settings.pickups,
            _ => settings.kmh = !settings.kmh,
        }
    }

    // 0..=AUDIO_STEPS: master, music, effects
    fn audio_value(&self, index: usize) -> String
    {
        let value = match index { 0 => self.audio.master, 1 => self.audio.music, _ => self.audio.effects };
        format!("{value:>2}")
    }

    fn change_audio(&mut self, index: usize, up: bool)
    {
        let audio = &mut self.audio;
        let value = match index { 0 => &mut audio.master, 1 => &mut audio.music, _ => &mut audio.effects };
        *value = if up { (*value + 1).min(AUDIO_STEPS) } else { value.saturating_sub(1) };
    }

    fn video_value(&self, index: usize) -> String
    {
        let on = if index == 0 { self.video.fullscreen } else { self.video.show_fps };
        if on { "On" } else { "Off" }.to_string()
    }

    fn toggle_video(&mut self, index: usize)
    {
        match index
        {
            0 => self.video.fullscreen = !self.video.fullscreen,
            _ => self.video.show_fps = !self.video.show_fps,
        }
    }

    // the name screen takes letters - F is no fullscreen key there
    pub fn typing(&self) -> bool
    {
        self.screen == Screen::EnterName
    }

    fn select(&mut self, index: usize)
    {
        match (self.screen, index)
        {
            (Screen::Main, 0) => self.go(Screen::SelectRace),
            (Screen::Main, 1) => self.go(Screen::TrialTimes),
            (Screen::Main, 2) => self.go(Screen::Progress),
            (Screen::Main, 3) => self.go(Screen::Options),
            (Screen::Main, _) => { self.cursor[Screen::Quit as usize] = 1; self.go(Screen::Quit); },
            // races go through Select Mode first like the game (the stunt arena not)
            (Screen::SelectRace, 0) => { self.mode = Mode::SingleRace; self.go_select_mode(); },
            (Screen::SelectRace, 1) => { self.mode = Mode::Championship; self.go_select_mode(); },
            (Screen::SelectRace, 3) => { self.mode = Mode::TimeTrial; self.go_select_mode(); },
            (Screen::SelectRace, 4) => { self.mode = Mode::Practice; self.go_select_mode(); },
            (Screen::SelectMode, index) => { self.settings.mode = DriveMode::ALL[index.min(DriveMode::ALL.len() - 1)]; self.go(Screen::EnterName); },
            (Screen::SelectRace, 5) => { self.mode = Mode::StuntArena; self.go(Screen::EnterName); },
            (Screen::Options, 0) => self.go(Screen::GameSettings),
            (Screen::Options, 1) => self.go(Screen::VideoSettings),
            (Screen::Options, 2) => self.go(Screen::AudioSettings),
            (Screen::Options, 3) => self.go(Screen::Controls),
            (Screen::VideoSettings, index) => self.toggle_video(index),
            (Screen::SelectCup, _) => self.go(Screen::CupSummary),
            (Screen::Quit, 0) => self.events.push(MenuEvent::Quit),
            (Screen::Quit, _) => self.go(Screen::Main),
            _ => {},
        }
    }

    fn back(&mut self)
    {
        let previous = match self.screen
        {
            Screen::Title => return,
            Screen::Main => { self.cursor[Screen::Quit as usize] = 1; Screen::Quit },
            Screen::SelectRace | Screen::Options | Screen::Quit | Screen::TrialTimes | Screen::Progress => Screen::Main,
            Screen::SelectCup => Screen::SelectCar,
            Screen::CupSummary => Screen::SelectCup,
            Screen::EnterName if self.mode == Mode::StuntArena => Screen::SelectRace,
            Screen::EnterName => Screen::SelectMode,
            Screen::SelectMode => Screen::SelectRace,
            Screen::SelectCar => Screen::EnterName,
            Screen::CarChosen => Screen::SelectCar,
            Screen::SelectTrack => Screen::SelectCar,
            Screen::GameSettings | Screen::AudioSettings | Screen::VideoSettings | Screen::Controls => Screen::Options,
        };
        self.sound("menuprev");
        self.go(previous);
    }

    // the cursor on the mode of the last race
    fn go_select_mode(&mut self)
    {
        self.cursor[Screen::SelectMode as usize] = DriveMode::ALL.iter().position(|mode| *mode == self.settings.mode).unwrap_or(1);
        self.go(Screen::SelectMode);
    }

    fn go(&mut self, screen: Screen)
    {
        if screen == Screen::EnterName
        {
            self.letter = 0;
        }
        self.screen = screen;
        self.screen_time = 0.0;
    }

    // the menu sounds (menunext, menuprev, menuupdown, menuleftright) play at 44100 Hz in the game - twice their rate
    fn sound(&mut self, name: &'static str)
    {
        let speed = if name.starts_with("menu") { MENU_SOUND_SPEED } else { 1.0 };
        self.events.push(MenuEvent::Sound(name, speed));
    }

    fn load(&self, ctx: &egui::Context) -> Result<Textures, String>
    {
        let texture = |path: String, name: &str| -> Result<TextureHandle, String>
        {
            let bytes = load_binary(&path).map_err(|error| format!("{path}: {error}"))?;
            let image = rustl::image::load_from_memory(&bytes).map_err(|error| format!("{path}: {error}"))?.to_rgba8();
            let size = [image.width() as usize, image.height() as usize];
            Ok(ctx.load_texture(format!("revolt_menu_{name}"), egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()), TextureOptions::LINEAR))
        };
        let assets = assets_of(&self.folder);
        let icons = texture(format!("{assets}/gfx/menu_icons.png"), "icons").map_err(|error| { console_warning!("menu: {} - no title icons", error); }).ok();
        let logo = texture(format!("{}{LOGO_FILE}", prefix(&self.folder)), "logo").map_err(|error| { console_warning!("menu: {} - no logo", error); }).ok();
        let loading = texture(format!("{assets}/gfx/{LOADING_PICTURE_FILE}"), "loading").map_err(|error| { console_warning!("menu: {} - the loading screen has no picture (re-convert)", error); }).ok();
        Ok(Textures { font: texture(format!("{assets}/gfx/font.png"), "font")?, icons, logo, loading })
    }

    fn draw(&self, painter: &mut Painter, cars: &[CarInfo], tracks: &[TrackInfo], has_logo: bool, has_icons: bool)
    {
        let fade = (self.screen_time * LOGO_FADE).min(1.0);
        if !self.message.is_empty()
        {
            if has_logo { logo(painter, 48.0, 400.0, fade); }
            self.draw_message(painter);
            return;
        }
        match self.screen
        {
            Screen::Title =>
            {
                if has_logo { logo(painter, 120.0, 480.0, fade); }
                if (self.clock * 2.0) as u32 % 2 == 0 { centered(painter, 380.0, 8.0, 16.0, ITEM, "Press Enter"); }
            },
            Screen::Main =>
            {
                if has_logo { logo(painter, 48.0, 400.0, fade); }
                self.draw_items(painter, None, 280.0);
            },
            Screen::Quit =>
            {
                // a question in the middle, like the main menu below it
                let question = "Really Quit?";
                let width = question.len() as f32 * 16.0 + 64.0;
                framed_box(painter, Anchor::Center, [(WIDTH - width) * 0.5, 180.0, width, 32.0]);
                painter.text(Anchor::Center, (WIDTH - question.len() as f32 * 16.0) * 0.5, 184.0, 16.0, 24.0, ITEM, question);
                self.draw_items(painter, None, 260.0);
            },
            Screen::SelectRace => { title(painter, "Select Race", Some(ICON_RACE), has_icons); self.draw_items(painter, Some(100.0), 150.0); },
            Screen::SelectMode =>
            {
                title(painter, "Select Mode", Some(ICON_RACE), has_icons);
                self.draw_items(painter, Some(100.0), 150.0);
                let cursor = self.cursor[Screen::SelectMode as usize].min(DriveMode::ALL.len() - 1);
                centered(painter, 440.0, CHAR_W, CHAR_H, HINT, DriveMode::ALL[cursor].hint());
            },
            Screen::Options => { title(painter, "Options", Some(ICON_OPTIONS), has_icons); self.draw_items(painter, Some(100.0), 150.0); },
            Screen::GameSettings => { title(painter, "Game Settings", Some(ICON_OPTIONS), has_icons); self.draw_items(painter, Some(100.0), 150.0); },
            Screen::AudioSettings => { title(painter, "Audio Settings", Some(ICON_OPTIONS), has_icons); self.draw_items(painter, Some(100.0), 150.0); },
            Screen::VideoSettings => { title(painter, "Video Settings", Some(ICON_OPTIONS), has_icons); self.draw_items(painter, Some(100.0), 150.0); },
            Screen::Controls => { title(painter, "Controller Settings", Some(ICON_OPTIONS), has_icons); draw_controls(painter); },
            Screen::EnterName => { title(painter, "Enter Name", Some(ICON_NAME), has_icons); self.draw_name(painter); },
            Screen::SelectCar | Screen::CarChosen => { title(painter, "Select Car", Some(ICON_CAR), has_icons); self.draw_car(painter, cars); },
            Screen::SelectTrack => { title(painter, "Select Track", Some(ICON_TRACK), has_icons); self.draw_track(painter, tracks); },
            Screen::SelectCup => { title(painter, "Select Cup", Some(ICON_CUP), has_icons); self.draw_items(painter, Some(100.0), 150.0); },
            Screen::CupSummary => { title(painter, "Summary", Some(ICON_CUP), has_icons); self.draw_summary(painter, tracks); },
            Screen::TrialTimes => { title(painter, "Best Trial Times", Some(ICON_WATCH), has_icons); self.draw_trial_times(painter, tracks); },
            Screen::Progress => { title(painter, "Progress Table", Some(ICON_PROGRESS), has_icons); self.draw_progress(painter, tracks); },
        }
    }

    // the end of a cup, white in a box in the middle
    fn draw_message(&self, painter: &mut Painter)
    {
        let width = self.message.iter().map(|line| line.len()).max().unwrap_or(0) as f32 * 16.0;
        let height = self.message.len() as f32 * 28.0;
        let y = 240.0 - height * 0.5;
        framed_box(painter, Anchor::Center, [(WIDTH - width) * 0.5 - 24.0, y - 16.0, width + 48.0, height + 24.0]);
        for (i, line) in self.message.iter().enumerate()
        {
            painter.text(Anchor::Center, (WIDTH - line.len() as f32 * 16.0) * 0.5, y + i as f32 * 28.0, 16.0, 24.0, if i == 0 { SELECTED } else { ITEM }, line);
        }
        centered(painter, 460.0, CHAR_W, CHAR_H, HINT, "Enter: continue");
    }

    // the cup in orange and what it takes, like the summary of the game - the races below
    fn draw_summary(&self, painter: &mut Painter, tracks: &[TrackInfo])
    {
        let cup = self.cursor[Screen::SelectCup as usize].min(CUPS.len() - 1);
        let (x, y) = (100.0, 150.0);
        let last = cup + 1 == CUPS.len();
        let mut rows: Vec<(String, Color32)> = vec![(CUPS[cup].name.to_string(), SELECTED), (String::new(), ITEM), ("Place 3rd or better in each".to_string(), ITEM), ("race to progress!".to_string(), ITEM)];
        rows.push((if last { "Finish top of the table and" } else { "Finish top of the table to" }.to_string(), ITEM));
        rows.push((if last { "be crowned \"Re-Volt Master\"!" } else { "access the next challenge!" }.to_string(), ITEM));
        rows.push((String::new(), ITEM));
        for (i, race) in CUPS[cup].races.iter().enumerate()
        {
            let track = tracks.iter().find(|track| track.level == race.level);
            let name = track.map_or(race.level.to_string(), |track| track.name.clone());
            let converted = track.and_then(|track| track.variant(race.reversed, race.mirrored)).is_some();
            rows.push((format!("{}  {}{}  {} laps", i + 1, name, variant_tag(race.reversed, race.mirrored), race.laps), if converted { ITEM } else { DISABLED }));
        }
        let width = rows.iter().map(|(row, _)| row.len()).max().unwrap_or(0) as f32 * CHAR_W;
        dark_box(painter, [x - 20.0, y - 24.0, width + 40.0, rows.len() as f32 * ROW + 40.0]);
        for (i, (row, color)) in rows.iter().enumerate()
        {
            painter.text(Anchor::Left, x, y + i as f32 * ROW, CHAR_W, CHAR_H, *color, row);
        }
        centered(painter, 460.0, CHAR_W, CHAR_H, HINT, "Enter: start the championship");
    }

    // the 10 best time trial laps of a track: time, name, car - like the game
    fn draw_trial_times(&self, painter: &mut Painter, tracks: &[TrackInfo])
    {
        let (x, y) = (100.0, 150.0);
        let Some(track) = tracks.get(self.trial_track.min(tracks.len().saturating_sub(1))) else
        {
            painter.text(Anchor::Left, x, y, CHAR_W, CHAR_H, ITEM, "No tracks - convert the levels first");
            return;
        };
        let (reversed, mirrored) = (self.trial_reversed, self.trial_mirrored);
        let converted = track.variant(reversed, mirrored).is_some();
        let name = format!("{}{}{}", track.name, if mirrored { " (M)" } else { "" }, if reversed { " (R)" } else { "" });
        let times = self.saved.trial_times.get(&variant_key(&track.level, reversed, mirrored));
        let width = 43.0 * CHAR_W;
        dark_box(painter, [x - 20.0, y - 24.0, width + 40.0, (TRIAL_TIMES + 2) as f32 * ROW + 40.0]);
        painter.text(Anchor::Left, x, y, CHAR_W, CHAR_H, SELECTED, &format!("< {name} >"));
        for i in 0..TRIAL_TIMES
        {
            let row = match times.and_then(|times| times.get(i))
            {
                Some(record) => format!("{} {:<16.16} {:<16.16}", format_time(record.time), record.name, car_name(&record.car)),
                None => "--------- ---------------- ----------------".to_string(),
            };
            painter.text(Anchor::Left, x, y + (i + 2) as f32 * ROW, CHAR_W, CHAR_H, if converted { ITEM } else { DISABLED }, &row);
        }
        centered(painter, 460.0, CHAR_W, CHAR_H, HINT, "Left/Right: track   Up: mirrored   Down: reversed");
    }

    // per cup and track: won a race, challenge time beaten normal / reversed / mirrored, practice star - a tick or a cross (font.png) like the game
    fn draw_progress(&self, painter: &mut Painter, tracks: &[TrackInfo])
    {
        let (x0, y0) = (60.0, 112.0);
        let row = 12.0;
        dark_box(painter, [x0, y0, 520.0, 272.0]);
        dark_box(painter, [x0, y0 + 292.0, 520.0, 24.0]);
        let centered_at = |painter: &mut Painter, x: f32, y: f32, text: &str| painter.text(Anchor::Left, x - text.len() as f32 * 4.0, y, CHAR_W, CHAR_H, ITEM, text);
        painter.text(Anchor::Left, x0 + 4.0, y0 + 4.0, CHAR_W, CHAR_H, ITEM, "Tracks");
        centered_at(painter, x0 + 180.0, y0 + 4.0, "Won");
        for (column, label) in [(180.0, "Race"), (244.0, "Norm"), (324.0, "Rev"), (404.0, "Mirr"), (484.0, "Stars")]
        {
            centered_at(painter, x0 + column, y0 + 4.0 + row, label);
        }
        let mark = |painter: &mut Painter, x: f32, y: f32, done: bool| painter.sprite(FONT, Anchor::Left, [x, y, 12.0, 12.0], [if done { 244.0 } else { 231.0 }, 244.0, 11.0, 11.0], if done { Color32::from_rgb(0, 255, 0) } else { Color32::from_rgb(255, 0, 0) });
        let mut y = y0 + 4.0 + 3.0 * row;
        let mut cup = usize::MAX;
        for (level, level_cup, _, _) in LEVELS
        {
            if level_cup != cup
            {
                cup = level_cup;
                let name = ["Bronze", "Silver", "Gold", "Platinum"][cup];
                painter.text(Anchor::Left, x0 + 4.0, y, CHAR_W, CHAR_H, ITEM, name);
                mark(painter, x0 + 4.0 + name.len() as f32 * CHAR_W + 8.0, y, self.saved.cups_won.contains(&cup));
                y += row;
            }
            let name = tracks.iter().find(|track| track.level == level).map_or(level.to_string(), |track| track.name.clone());
            painter.text(Anchor::Left, x0 + 12.0, y, CHAR_W, CHAR_H, Color32::from_rgb(0x80, 0x80, 0x80), &name.chars().take(19).collect::<String>());
            for (done, column) in self.saved.progress(level).into_iter().zip([172.0, 240.0, 320.0, 400.0, 476.0])
            {
                mark(painter, x0 + column, y, done);
            }
            y += row;
        }
        let stars = format!("Stunt Arena - Stars Collected: {} of 20", self.saved.stunt_stars);
        painter.text(Anchor::Left, x0 + (520.0 - stars.len() as f32 * CHAR_W) * 0.5, y0 + 298.0, CHAR_W, CHAR_H, ITEM, &stars);
    }

    // the items in a dark box at x (None: centered), the settings with their value
    fn draw_items(&self, painter: &mut Painter, x: Option<f32>, y: f32)
    {
        let items = self.items();
        let cursor = available(&items, self.cursor[self.screen as usize], true);
        let label_width = items.iter().map(|(label, _)| label.len()).max().unwrap_or(0) as f32 * CHAR_W;
        let values: Vec<String> = (0..items.len()).map(|i| match self.screen { Screen::GameSettings => self.setting_value(i), Screen::AudioSettings => self.audio_value(i), Screen::VideoSettings => self.video_value(i), _ => String::new() }).collect();
        let value_width = values.iter().map(|v| v.len()).max().unwrap_or(0) as f32 * CHAR_W;
        let width = label_width + if value_width > 0.0 { 3.0 * CHAR_W + value_width } else { 0.0 };
        let anchor = if x.is_some() { Anchor::Left } else { Anchor::Center };
        let x = x.unwrap_or((WIDTH - width) * 0.5);
        framed_box(painter, anchor, [x - 20.0, y - 24.0, width + 40.0, items.len() as f32 * ROW + 44.0 - 4.0]);
        for (i, (label, available)) in items.iter().enumerate()
        {
            let color = match (i == cursor, *available) { (true, true) => SELECTED, (false, true) => ITEM, (true, false) => SELECTED_DISABLED, (false, false) => DISABLED };
            let row_y = y + i as f32 * ROW;
            let label_x = if matches!(self.screen, Screen::Main | Screen::Quit) { x + (width - label.len() as f32 * CHAR_W) * 0.5 } else { x };
            painter.text(anchor, label_x, row_y, CHAR_W, CHAR_H, color, label);
            if !values[i].is_empty()
            {
                let arrows = if i == cursor { format!("<{}>", values[i]) } else { format!(" {} ", values[i]) };
                painter.text(anchor, x + label_width + 2.0 * CHAR_W, row_y, CHAR_W, CHAR_H, color, &arrows);
            }
        }
    }

    // "Player 1" and the name with a blinking _ behind it (DEL / END of the wheel spelled out)
    fn draw_name(&self, painter: &mut Painter)
    {
        let (x, y) = (100.0, 120.0);
        let label = "Player 1";
        let hint = "Left/Right: letter  Up: take  Down: delete  Enter: done";
        let name_x = x + label.len() as f32 * CHAR_W + 32.0;
        let width = ((label.len() + NAME_LENGTH + 3) as f32 * CHAR_W + 32.0).max(hint.len() as f32 * CHAR_W);
        dark_box(painter, [x - 20.0, y - 24.0, width + 40.0, 2.0 * ROW + 40.0]);
        painter.text(Anchor::Left, x, y, CHAR_W, CHAR_H, SELECTED, label);
        painter.text(Anchor::Left, name_x, y, CHAR_W, CHAR_H, NAME, &self.name);
        if (self.clock * 2.0) as u32 % 2 == 0
        {
            let cursor = match self.letter { LETTER_DELETE => "DEL", LETTER_END => "END", _ => "_" };
            let color = if self.letter >= LETTER_DELETE { SELECTED } else { NAME };
            painter.text(Anchor::Left, name_x + self.name.len() as f32 * CHAR_W, y, CHAR_W, CHAR_H, color, &cursor);
        }
        painter.text(Anchor::Left, x, y + 1.5 * ROW, CHAR_W, CHAR_H, HINT, hint);
    }

    // the car of the presented box and its values
    fn draw_car(&self, painter: &mut Painter, cars: &[CarInfo])
    {
        let Some(car) = cars.get(self.car) else
        {
            painter.text(Anchor::Left, 100.0, 150.0, CHAR_W, CHAR_H, ITEM, "No cars - convert the cars first");
            return;
        };
        let (x, y) = (100.0, 150.0);
        let class = match car.class { 0 => "Electric", 1 => "Glow", _ => "Special" };
        let rating = match car.rating { 0 => "Rookie", 1 => "Amateur", 2 => "Advanced", 3 => "Semi-Pro", 4 => "Pro", _ => "Unknown" };
        let trans = match car.trans { 0 => "4WD", 1 => "FWD", 2 => "RWD", _ => "Unknown" };
        // speed, acc and weight as bars over the range of all cars
        let share = |value: f32, of: fn(&CarInfo) -> f32| -> f32
        {
            let (low, high) = cars.iter().map(of).fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)));
            if high - low < 1e-6 { 1.0 } else { (0.08 + 0.92 * (value - low) / (high - low)).clamp(0.08, 1.0) }
        };
        let bars = [("Speed", share(car.speed, |c| c.speed)), ("Acc", share(car.acc, |c| c.acc)), ("Weight", share(car.weight, |c| c.weight))];
        let texts = [("Class", class), ("Rating", rating)];
        let rows = texts.len() + bars.len() + 1;
        dark_box(painter, [x - 20.0, y - 24.0, 8.0 * CHAR_W + BAR_WIDTH + 52.0, (rows + 2) as f32 * ROW + 40.0]);
        painter.text(Anchor::Left, x, y, CHAR_W, CHAR_H, NAME, &car.name);
        let value_x = x + 8.0 * CHAR_W;
        let mut row_y = y + 2.0 * ROW;
        for (label, value) in texts
        {
            painter.text(Anchor::Left, x, row_y, CHAR_W, CHAR_H, ITEM, label);
            painter.text(Anchor::Left, value_x, row_y, CHAR_W, CHAR_H, ITEM, value);
            row_y += ROW;
        }
        for (label, fraction) in bars
        {
            painter.text(Anchor::Left, x, row_y, CHAR_W, CHAR_H, ITEM, label);
            let to = lerp_color(BAR_LOW, BAR_HIGH, fraction);
            painter.gradient(FILL, Anchor::Left, [value_x, row_y + 1.0, BAR_WIDTH * fraction, CHAR_H - 2.0], BAR_LOW, to);
            row_y += ROW;
        }
        painter.text(Anchor::Left, x, row_y, CHAR_W, CHAR_H, ITEM, "Trans");
        painter.text(Anchor::Left, value_x, row_y, CHAR_W, CHAR_H, ITEM, trans);
        if self.screen == Screen::SelectCar
        {
            centered(painter, 448.0, CHAR_W, CHAR_H, HINT, &format!("< {} / {} >   Enter: take this car", self.car + 1, cars.len()));
        }
    }

    fn draw_track(&self, painter: &mut Painter, tracks: &[TrackInfo])
    {
        let (x, y) = (100.0, 380.0);
        let Some(track) = tracks.get(self.track) else
        {
            painter.text(Anchor::Left, x, y, CHAR_W, CHAR_H, ITEM, "No tracks - convert the levels first");
            return;
        };
        let direction = direction_name(self.reversed, self.mirrored);
        let laps = match self.mode { Mode::TimeTrial => "Time Trial".to_string(), Mode::Practice => "Practice - find the star".to_string(), _ => format!("{} laps, {} cars", self.settings.laps, self.settings.cars) };
        let laps = format!("{laps}, {}", self.settings.mode.label());
        let key = variant_key(&track.level, self.reversed, self.mirrored);
        let records = self.saved.tracks.get(&key);
        let best_lap = records.and_then(|r| r.best_lap.as_ref()).map_or("Best lap   -".to_string(), |r| format!("Best lap   {}  {}", format_time(r.time), r.car));
        let best_race = records.and_then(|r| r.best_race.get(&self.settings.laps)).map_or(format!("Best race  -  ({} laps)", self.settings.laps), |r| format!("Best race  {}  {} ({} laps)", format_time(r.time), r.car, self.settings.laps));
        let star = if self.saved.practice_stars.contains(&key) { "Practice star found" } else { "Practice star not found" };
        let others = track.reversed.is_some() || track.mirrored.is_some();
        let mut rows = vec![format!("{}  ({:.0} m)", track.name, track.length), format!("{direction}, {laps}"), best_lap];
        match self.mode
        {
            Mode::Practice => rows.push(star.to_string()),
            Mode::TimeTrial => {},
            _ => rows.push(best_race),
        }
        // the box grows upwards with its rows - it ends above the hint line
        let y = 420.0 - (rows.len() - 1) as f32 * ROW;
        let width = rows.iter().map(|r| r.len()).max().unwrap_or(0) as f32 * CHAR_W;
        dark_box(painter, [x - 20.0, y - 24.0, width + 40.0, rows.len() as f32 * ROW + 40.0]);
        for (i, row) in rows.iter().enumerate()
        {
            painter.text(Anchor::Left, x, y + i as f32 * ROW, CHAR_W, CHAR_H, if i == 0 { SELECTED } else { ITEM }, row);
        }
        let directions = if others { "   Up: mirrored  Down: reversed" } else { "" };
        centered(painter, 460.0, CHAR_W, CHAR_H, HINT, &format!("< {} / {} >{directions}   Enter: race", self.track + 1, tracks.len()));
    }
}

// the keys: action, keyboard, gamepad in three columns
fn draw_controls(painter: &mut Painter)
{
    let (x, y) = (60.0, 120.0);
    let columns = [0.0, 18.0 * CHAR_W, 38.0 * CHAR_W];
    dark_box(painter, [x - 20.0, y - 24.0, 57.0 * CHAR_W + 40.0, (CONTROLS.len() + 2) as f32 * ROW + 40.0]);
    for (label, column) in ["", "Keyboard", "Gamepad"].iter().zip(columns)
    {
        painter.text(Anchor::Left, x + column, y, CHAR_W, CHAR_H, SELECTED, label);
    }
    for (i, (action, keys, pad)) in CONTROLS.iter().enumerate()
    {
        let row_y = y + (i + 2) as f32 * ROW;
        for (text, column) in [action, keys, pad].into_iter().zip(columns)
        {
            painter.text(Anchor::Left, x + column, row_y, CHAR_W, CHAR_H, ITEM, text);
        }
    }
    centered(painter, 460.0, CHAR_W, CHAR_H, HINT, "Enter / Backspace: back");
}

// the title of a screen in a dark box, its icon in a box of its own to the left
fn title(painter: &mut Painter, text: &str, icon: Option<usize>, has_icons: bool)
{
    dark_box(painter, [108.0, 40.0, text.len() as f32 * 16.0 + 64.0, 32.0]);
    painter.text(Anchor::Left, 140.0, 44.0, 16.0, 24.0, ITEM, text);
    if let Some(index) = icon.filter(|_| has_icons)
    {
        // the icon on a dark tile, joined to the title by a red bar; the embossed lines of the icon added twice (light on dark like the game)
        painter.rounded(Anchor::Left, [90.0, 54.0, 20.0, 4.0], RIM, RIM, 0.0, 0.0);
        painter.rounded(Anchor::Left, [20.0, 20.0, 72.0, 72.0], ICON_TILE, RIM, RIM_WIDTH, BOX_RADIUS);
        let (u, v) = ((index % 5) as f32 * ICON_SIZE, (index / 5) as f32 * ICON_SIZE);
        for _ in 0..2
        {
            painter.sprite(ICONS, Anchor::Left, [24.0, 24.0, 64.0, 64.0], [u, v, ICON_SIZE, ICON_SIZE], Color32::from_rgba_premultiplied(255, 236, 228, 0));
        }
    }
}

fn dark_box(painter: &mut Painter, rect: [f32; 4])
{
    framed_box(painter, Anchor::Left, rect);
}

// the boxes of the menu: dark, rounded, with a red rim
fn framed_box(painter: &mut Painter, anchor: Anchor, rect: [f32; 4])
{
    painter.rounded(anchor, rect, BOX, RIM, RIM_WIDTH, BOX_RADIUS);
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32
{
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

fn logo(painter: &mut Painter, y: f32, width: f32, fade: f32)
{
    let height = width / LOGO_ASPECT;
    let a = (fade * 255.0) as u8;
    painter.image(LOGO, Anchor::Center, [(WIDTH - width) * 0.5, y, width, height], Color32::from_rgba_unmultiplied(255, 255, 255, a));
}

fn centered(painter: &mut Painter, y: f32, w: f32, h: f32, color: Color32, text: &str)
{
    painter.text(Anchor::Center, (WIDTH - text.len() as f32 * w) * 0.5, y.min(HEIGHT - h), w, h, color, text);
}

// the item at index or the next available one in that direction - the gray ones can not be selected
fn available(items: &[(String, bool)], index: usize, forward: bool) -> usize
{
    let count = items.len();
    if count == 0 { return 0; }
    let mut i = index.min(count - 1);
    for _ in 0..count
    {
        if items[i].1 { return i; }
        i = step(i, count, forward);
    }
    index.min(count - 1)
}

fn step(index: usize, count: usize, forward: bool) -> usize
{
    if forward { (index + 1) % count } else { (index + count - 1) % count }
}

// the letters of the wheel: A-Z, space, !, then DEL and END
fn letter_char(index: usize) -> char
{
    match index
    {
        0..=25 => (b'A' + index as u8) as char,
        26 => ' ',
        _ => '!',
    }
}

fn letter_index(c: char) -> Option<usize>
{
    match c.to_ascii_uppercase()
    {
        c @ 'A'..='Z' => Some(c as usize - 'A' as usize),
        ' ' => Some(26),
        '!' => Some(27),
        _ => None,
    }
}

fn prefix(folder: &str) -> String
{
    if folder.is_empty() { String::new() } else { format!("{folder}/") }
}

fn assets_of(folder: &str) -> String
{
    format!("{}assets", prefix(folder))
}

// the cars with a converted glb, their values from the model node (Parameters.txt)
pub fn load_cars(folder: &str) -> Vec<CarInfo>
{
    let assets = assets_of(folder);
    let cars: Vec<CarInfo> = CAR_IDS.iter().enumerate().filter_map(|(index, id)|
    {
        let bytes = load_binary(&format!("{assets}/cars/{id}/{id}.glb")).ok()?;
        let extras = model_extras(&bytes).unwrap_or(Value::Null);
        let number = |key: &str| extras[key].as_f64().unwrap_or(0.0) as f32;
        Some(CarInfo
        {
            id: id.to_string(),
            box_id: if index < BOXED_CARS { index as i32 } else { -1 },
            name: extras["revolt_name"].as_str().unwrap_or(id).to_string(),
            class: extras["revolt_class"].as_i64().unwrap_or(0),
            rating: extras["revolt_rating"].as_i64().unwrap_or(0),
            speed: number("revolt_topspeed"),
            acc: number("revolt_acc"),
            weight: number("revolt_weight"),
            trans: extras["revolt_trans"].as_i64().unwrap_or(0),
        })
    }).collect();
    CAR_NAMES.with(|names| names.borrow_mut().extend(cars.iter().map(|car| (car.id.clone(), car.name.clone()))));
    console_log!("menu: {} cars", cars.len());
    cars
}

// the name of a car id for the lists - the id itself if unknown
fn car_name(id: &str) -> String
{
    CAR_NAMES.with(|names| names.borrow().get(id).cloned()).unwrap_or_else(|| id.to_string())
}

thread_local!
{
    // car id -> name, filled by load_cars
    static CAR_NAMES: std::cell::RefCell<HashMap<String, String>> = std::cell::RefCell::new(HashMap::new());
}

// the extras of the node "model" in the JSON chunk of a glb
fn model_extras(bytes: &[u8]) -> Option<Value>
{
    let length = u32::from_le_bytes(bytes.get(12..16)?.try_into().ok()?) as usize;
    let json: Value = serde_json::from_slice(bytes.get(20..20 + length)?).ok()?;
    json["nodes"].as_array()?.iter().find(|node| node["name"] == "model").map(|node| node["extras"].clone())
}

// the race tracks among the loaded scenes: a level object with track data - reversed scenes go with their level
// length and level name of a track scene by scene id - a not loaded scene is read from its file once
pub type TrackCache = HashMap<u32, Option<(f32, Option<String>)>>;

pub fn load_tracks(state: &State, cache: &mut TrackCache) -> Vec<TrackInfo>
{
    let mut tracks: Vec<TrackInfo> = vec![];
    // the other directions: level, reversed, mirrored, scene
    let mut variants: Vec<(String, bool, bool, u32)> = vec![];
    for scene in &state.scenes
    {
        let info = cache.entry(scene.id).or_insert_with(|| match scene.is_loaded()
        {
            true => crate::track::Track::from_scene(scene).map(|track| (track.length, level_name(scene))),
            false => crate::track::Track::from_scene_file(scene).map(|(track, name)| (track.length, name)),
        });
        let Some((length, name)) = info.clone() else { continue; };
        let scene_name = scene.name.to_lowercase();
        let (rest, mirrored) = match scene_name.strip_suffix("_mirrored") { Some(rest) => (rest, true), None => (scene_name.as_str(), false) };
        let (level, reversed) = match rest.strip_suffix("_reversed") { Some(level) => (level, true), None => (rest, false) };
        if reversed || mirrored
        {
            variants.push((level.to_string(), reversed, mirrored, scene.id));
            continue;
        }
        let name = name.unwrap_or_else(|| scene.name.clone());
        tracks.push(TrackInfo { level: scene_name.clone(), name, scene_id: scene.id, reversed: None, mirrored: None, reversed_mirrored: None, length: length * 0.5 });
    }
    for (level, reversed, mirrored, id) in variants
    {
        let Some(track) = tracks.iter_mut().find(|track| track.level == level) else { continue; };
        match (reversed, mirrored)
        {
            (true, false) => track.reversed = Some(id),
            (false, true) => track.mirrored = Some(id),
            _ => track.reversed_mirrored = Some(id),
        }
    }
    // the level order of the game, the levels not in it after them
    let rank = |level: &str| LEVELS.iter().position(|(l, ..)| *l == level).unwrap_or(LEVELS.len());
    tracks.sort_by(|a, b| rank(&a.level).cmp(&rank(&b.level)).then(a.level.cmp(&b.level)));
    tracks
}

// NAME of the .inf on the level glb root
fn level_name(scene: &Scene) -> Option<String>
{
    let node = crate::track::find_node(&scene.nodes, &|node| node.extras.get::<String>("revolt_level").is_some())?;
    let name = node.read().unwrap().extras.get::<String>("name").cloned();
    name
}

// the menu over a race, like the in-game menu of the game: Esc pauses (Resume / Restart Race / Main Menu), Enter after the finish (Restart Race / Next Track / Main Menu)
#[derive(Clone, Copy, PartialEq)]
pub enum RaceMenuKind
{
    Pause,
    Finished,
    Stunts,
    // the pause of a championship race: no restart, a try can be given up
    Championship,
}

#[derive(Clone, Copy, PartialEq)]
pub enum RaceAction
{
    Resume,
    Restart,
    NextTrack,
    MainMenu,
    GiveUp,
}

pub struct RaceMenu
{
    pub kind: RaceMenuKind,
    items: Vec<(&'static str, RaceAction)>,
    cursor: usize,
    assets: String,
    font: Option<TextureHandle>,
    failed: bool,
}

impl RaceMenu
{
    // without the menu (Play in a level scene) only the race itself
    pub fn new(kind: RaceMenuKind, assets: &str, with_menu: bool) -> Self
    {
        let mut items = match kind
        {
            RaceMenuKind::Pause => vec![("Continue", RaceAction::Resume), ("Restart Race", RaceAction::Restart)],
            RaceMenuKind::Finished => vec![("Restart Race", RaceAction::Restart)],
            RaceMenuKind::Stunts => vec![("Resume", RaceAction::Resume), ("Restart", RaceAction::Restart)],
            RaceMenuKind::Championship => vec![("Continue Championship", RaceAction::Resume), ("Give Up Try", RaceAction::GiveUp), ("End Championship", RaceAction::MainMenu)],
        };
        if with_menu && kind != RaceMenuKind::Championship
        {
            if matches!(kind, RaceMenuKind::Finished | RaceMenuKind::Pause) { items.push(("Next Track", RaceAction::NextTrack)); }
            items.push(("Main Menu", RaceAction::MainMenu));
        }
        Self { kind, items, cursor: 0, assets: assets.to_string(), font: None, failed: false }
    }

    // the chosen action and the sounds (with their speed) of this frame; Esc / Backspace resume a pause
    pub fn update(&mut self, pad: &Buttons) -> (Option<RaceAction>, Vec<(&'static str, f32)>)
    {
        let mut sounds = vec![];
        if pad.up || pad.down
        {
            self.cursor = step(self.cursor, self.items.len(), pad.down);
            sounds.push(("menuupdown", MENU_SOUND_SPEED));
        }
        if pad.enter
        {
            sounds.push(("menunext", MENU_SOUND_SPEED));
            return (Some(self.items[self.cursor].1), sounds);
        }
        if self.kind != RaceMenuKind::Finished && (pad.pause || pad.back)
        {
            sounds.push(("menuprev", MENU_SOUND_SPEED));
            return (Some(RaceAction::Resume), sounds);
        }
        (None, sounds)
    }

    // a dark box in the middle - below the results after the finish
    pub fn ui(&mut self, ui: &mut egui::Ui)
    {
        if self.font.is_none() && !self.failed
        {
            let path = format!("{}/gfx/font.png", self.assets);
            match load_binary(&path).map_err(|e| e.to_string()).and_then(|bytes| rustl::image::load_from_memory(&bytes).map_err(|e| e.to_string()))
            {
                Ok(image) =>
                {
                    let image = image.to_rgba8();
                    let size = [image.width() as usize, image.height() as usize];
                    self.font = Some(ui.ctx().load_texture("revolt_race_menu_font", egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()), TextureOptions::LINEAR));
                },
                Err(error) => { console_warning!("race menu: {}: {}", path, error); self.failed = true; },
            }
        }
        let Some(font) = &self.font else { return; };
        let mut painter = Painter::new(ui.ctx().content_rect(), vec![font.id(), egui::TextureId::default()]);

        let title = match self.kind { RaceMenuKind::Pause | RaceMenuKind::Championship => "Paused", RaceMenuKind::Finished => "Race Over", RaceMenuKind::Stunts => "Stunt Arena" };
        let width = self.items.iter().map(|(label, _)| label.len()).max().unwrap_or(0).max(title.len() * 2) as f32 * CHAR_W;
        let height = 32.0 + self.items.len() as f32 * ROW;
        let x = (WIDTH - width) * 0.5;
        let y = match self.kind { RaceMenuKind::Pause | RaceMenuKind::Stunts | RaceMenuKind::Championship => 180.0, RaceMenuKind::Finished => HEIGHT - height - 40.0 };
        framed_box(&mut painter, Anchor::Center, [x - 20.0, y - 16.0, width + 40.0, height + 28.0]);
        painter.text(Anchor::Center, (WIDTH - title.len() as f32 * 16.0) * 0.5, y - 4.0, 16.0, 24.0, ITEM, title);
        for (i, (label, _)) in self.items.iter().enumerate()
        {
            let color = if i == self.cursor { SELECTED } else { ITEM };
            painter.text(Anchor::Center, (WIDTH - label.len() as f32 * CHAR_W) * 0.5, y + 32.0 + i as f32 * ROW, CHAR_W, CHAR_H, color, label);
        }
        painter.finish(ui.ctx(), &[FILL, FONT]);
    }
}

// what the game keeps between runs, one json file (engine save_data: %APPDATA%/<project name>/save.json, web: localStorage)
// the menu settings, the audio options, the name, the records per track and the stars found

use std::collections::{BTreeMap, BTreeSet};

use rustl::prelude::*;
use rustl::resources::save_data::{load_text, save_text};
use rustl::serde::{Deserialize, Serialize};
use rustl::serde_json;

use crate::assist::DriveMode;
use crate::bot::Difficulty;
use crate::menu::{AudioSettings, MultiSetup, PlayerSetup, Shadows, VideoSettings, MAX_PLAYERS};
use crate::pad::Device;
use crate::race::RaceSettings;

const KEY: &str = "save";

#[derive(Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(crate = "rustl::serde", default)]
pub struct SaveData
{
    pub name: String,
    pub settings: Option<Settings>,
    pub audio: Option<Audio>,
    pub video: Option<Video>,
    // by scene name (garden1, garden1_reversed, ...)
    pub tracks: BTreeMap<String, TrackRecords>,
    // practice: the tracks whose star was found
    pub practice_stars: BTreeSet<String>,
    // stunt arena: the stars found (count + index in scene order) - they stay collected like in the game
    pub stunt_stars: u32,
    pub stunt_found: BTreeSet<usize>,
    // time trial: the 10 best laps per track (Best Trial Times)
    pub trial_times: BTreeMap<String, Vec<Record>>,
    // time trial: the tracks whose challenge lap time was beaten (Progress Table: Norm / Rev / Mirr)
    pub challenges: BTreeSet<String>,
    // championship: the won cups (0 Bronze .. 3 Platinum) - each opens the next
    pub cups_won: BTreeSet<usize>,
    // the Split Screen menu: mode, players, opponents, per player name, device, car, driving mode
    pub multi: Option<Multi>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(crate = "rustl::serde", default)]
pub struct Multi
{
    pub trial: bool,
    pub players: usize,
    // the first version had only on / off - read when the count is missing
    pub bots: bool,
    pub opponents: Option<usize>,
    pub difficulty: String,
    pub slots: Vec<MultiSlot>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(crate = "rustl::serde", default)]
pub struct MultiSlot
{
    pub name: String,
    pub device: String,
    // index in the car list of the menu
    pub car: usize,
    pub mode: String,
}

// the Best Trial Times of a track hold this many laps
pub const TRIAL_TIMES: usize = 10;

#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(crate = "rustl::serde")]
pub struct Settings
{
    pub cars: usize,
    pub laps: u32,
    pub difficulty: String,
    pub pickups: bool,
    #[serde(default = "yes")]
    pub kmh: bool,
    #[serde(default)]
    pub mode: String,
}

fn yes() -> bool { true }

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq)]
#[serde(crate = "rustl::serde")]
pub struct Audio
{
    pub master: u32,
    pub music: u32,
    pub effects: u32,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq)]
#[serde(crate = "rustl::serde")]
pub struct Video
{
    pub fullscreen: bool,
    pub show_fps: bool,
    // older saves have no graphics - the defaults of the project
    #[serde(default)]
    pub msaa: Option<u32>,
    #[serde(default)]
    pub shadows: Option<Shadows>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(crate = "rustl::serde", default)]
pub struct TrackRecords
{
    pub best_lap: Option<Record>,
    // the best race time by the number of laps
    pub best_race: BTreeMap<u32, Record>,
    // a race against others finished first (Won Race of the loading screen)
    pub won: bool,
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(crate = "rustl::serde")]
pub struct Record
{
    // s
    pub time: f64,
    pub car: String,
    pub name: String,
}

impl SaveData
{
    // the saved data of the game, empty if there is none or it does not parse (then it is overwritten on the next save)
    pub fn load(game: &str) -> Self
    {
        let Some(text) = load_text(game, KEY) else { return Self::default(); };
        serde_json::from_str(&text).unwrap_or_else(|error|
        {
            console_warning!("save: {} - starting fresh", error);
            Self::default()
        })
    }

    pub fn store(&self, game: &str)
    {
        let result = serde_json::to_string_pretty(self).map_err(|error| error.to_string()).and_then(|text| save_text(game, KEY, &text));
        if let Err(error) = result
        {
            console_warning!("save: {}", error);
        }
    }

    pub fn race_settings(&self, mut settings: RaceSettings) -> RaceSettings
    {
        if let Some(saved) = &self.settings
        {
            settings.cars = saved.cars.clamp(1, 12);
            settings.laps = saved.laps.clamp(1, 20);
            settings.pickups = saved.pickups;
            settings.kmh = saved.kmh;
            if let Some(mode) = DriveMode::ALL.iter().find(|m| m.label() == saved.mode) { settings.mode = *mode; }
            if let Some(difficulty) = Difficulty::ALL.iter().find(|d| d.label() == saved.difficulty) { settings.difficulty = *difficulty; }
        }
        settings
    }

    pub fn set_race_settings(&mut self, settings: &RaceSettings)
    {
        self.settings = Some(Settings { cars: settings.cars, laps: settings.laps, difficulty: settings.difficulty.label().to_string(), pickups: settings.pickups, kmh: settings.kmh, mode: settings.mode.label().to_string() });
    }

    pub fn audio_settings(&self) -> AudioSettings
    {
        let steps = crate::menu::AUDIO_STEPS;
        self.audio.map_or(AudioSettings::default(), |a| AudioSettings { master: a.master.min(steps), music: a.music.min(steps), effects: a.effects.min(steps) })
    }

    pub fn set_audio_settings(&mut self, audio: &AudioSettings)
    {
        self.audio = Some(Audio { master: audio.master, music: audio.music, effects: audio.effects });
    }

    pub fn multi_setup(&self) -> MultiSetup
    {
        let mut setup = MultiSetup::default();
        let Some(saved) = &self.multi else { return setup; };
        setup.trial = saved.trial;
        setup.players = saved.players.clamp(2, MAX_PLAYERS);
        setup.opponents = saved.opponents.unwrap_or(if saved.bots { setup.opponents } else { 0 }).min(12 - setup.players);
        if let Some(difficulty) = Difficulty::ALL.iter().find(|d| d.label() == saved.difficulty) { setup.difficulty = *difficulty; }
        for (slot, saved) in setup.slots.iter_mut().zip(&saved.slots)
        {
            *slot = PlayerSetup
            {
                name: saved.name.clone(),
                device: Device::from_key(&saved.device),
                car: saved.car,
                mode: DriveMode::ALL.iter().copied().find(|m| m.label() == saved.mode).unwrap_or_default(),
            };
        }
        setup
    }

    pub fn set_multi_setup(&mut self, setup: &MultiSetup)
    {
        let slots = setup.slots.iter().map(|slot| MultiSlot { name: slot.name.clone(), device: slot.device.map(Device::key).unwrap_or_default(), car: slot.car, mode: slot.mode.label().to_string() }).collect();
        self.multi = Some(Multi { trial: setup.trial, players: setup.players, bots: setup.opponents > 0, opponents: Some(setup.opponents), difficulty: setup.difficulty.label().to_string(), slots });
    }

    pub fn video_settings(&self) -> VideoSettings
    {
        let default = VideoSettings::default();
        self.video.map_or(default, |v| VideoSettings { fullscreen: v.fullscreen, show_fps: v.show_fps, msaa: v.msaa.unwrap_or(default.msaa), shadows: v.shadows.unwrap_or(default.shadows) })
    }

    pub fn set_video_settings(&mut self, video: &VideoSettings)
    {
        self.video = Some(Video { fullscreen: video.fullscreen, show_fps: video.show_fps, msaa: Some(video.msaa), shadows: Some(video.shadows) });
    }

    // a lap or race time - true if it is a new record
    pub fn record_lap(&mut self, track: &str, time: f64, car: &str, name: &str) -> bool
    {
        let records = self.tracks.entry(track.to_string()).or_default();
        if records.best_lap.as_ref().is_some_and(|best| best.time <= time) { return false; }
        records.best_lap = Some(Record { time, car: car.to_string(), name: name.to_string() });
        true
    }

    pub fn record_race(&mut self, track: &str, laps: u32, time: f64, car: &str, name: &str) -> bool
    {
        let records = self.tracks.entry(track.to_string()).or_default();
        if records.best_race.get(&laps).is_some_and(|best| best.time <= time) { return false; }
        records.best_race.insert(laps, Record { time, car: car.to_string(), name: name.to_string() });
        true
    }

    // true if the track was not won before
    pub fn record_win(&mut self, track: &str) -> bool
    {
        let records = self.tracks.entry(track.to_string()).or_default();
        !std::mem::replace(&mut records.won, true)
    }

    pub fn won(&self, track: &str) -> bool
    {
        self.tracks.get(track).is_some_and(|records| records.won)
    }

    // a time trial lap into the 10 best of the track - its place (0 = best) if it got in
    pub fn record_trial(&mut self, track: &str, time: f64, car: &str, name: &str) -> Option<usize>
    {
        let times = self.trial_times.entry(track.to_string()).or_default();
        let place = times.iter().position(|record| time < record.time).unwrap_or(times.len());
        if place >= TRIAL_TIMES { return None; }
        times.insert(place, Record { time, car: car.to_string(), name: name.to_string() });
        times.truncate(TRIAL_TIMES);
        Some(place)
    }

    // the championship: Bronze is open, every won cup opens the next
    pub fn cup_open(&self, cup: usize) -> bool
    {
        cup == 0 || self.cups_won.contains(&(cup - 1))
    }

    // the Progress Table of a level: won a race on it (any direction), challenge beaten forward / reversed / mirrored, practice star found
    pub fn progress(&self, level: &str) -> [bool; 5]
    {
        let variants = [level.to_string(), format!("{level}_reversed"), format!("{level}_mirrored"), format!("{level}_reversed_mirrored")];
        let won = variants.iter().any(|key| self.won(key));
        let star = variants.iter().any(|key| self.practice_stars.contains(key));
        let mirrored = self.challenges.contains(&variants[2]) || self.challenges.contains(&variants[3]);
        [won, self.challenges.contains(&variants[0]), self.challenges.contains(&variants[1]), mirrored, star]
    }
}

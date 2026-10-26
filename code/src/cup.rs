// the championship of the game (formats/README.md, Championship): 4 cups of 4-5 races against 7 opponents of the cup's ratings,
// points 10-6-4-3-2-1-0-0, 3rd or better in every race to go on (3 tries to repeat a race), first in the table wins the cup and opens the next

use rustl::egui::{self, Color32, TextureHandle, TextureOptions};
use rustl::prelude::*;
use rustl::resources::resources::load_binary;

use crate::hud::{suffix, Anchor, Painter, CYAN, HEIGHT, WHITE, WIDTH};
use crate::pad::Buttons;
use crate::race::{Race, RaceSettings};
use crate::random::Random;

pub struct CupRace
{
    pub level: &'static str,
    pub laps: u32,
    pub mirrored: bool,
    pub reversed: bool,
}

pub struct Cup
{
    pub name: &'static str,
    pub races: &'static [CupRace],
    // opponents per rating (rookie .. pro)
    pub ratings: [usize; 5],
}

const fn race(level: &'static str, laps: u32, mirrored: bool, reversed: bool) -> CupRace
{
    CupRace { level, laps, mirrored, reversed }
}

pub const CUPS: [Cup; 4] =
[
    Cup { name: "Bronze Cup", races: &[race("nhood1", 3, false, false), race("market2", 4, false, false), race("muse2", 3, false, false), race("garden1", 5, false, false)], ratings: [7, 0, 0, 0, 0] },
    Cup { name: "Silver Cup", races: &[race("toylite", 6, false, false), race("nhood1", 4, true, false), race("wild_west1", 4, false, false), race("toy2", 4, false, false)], ratings: [0, 4, 3, 0, 0] },
    Cup { name: "Gold Cup", races: &[race("nhood2", 5, false, false), race("toylite", 8, true, false), race("ship1", 5, false, false), race("muse1", 5, false, false)], ratings: [0, 0, 4, 3, 0] },
    Cup { name: "Platinum Cup", races: &[race("market1", 6, false, false), race("wild_west2", 6, false, false), race("toylite", 10, true, true), race("muse1", 6, true, false), race("ship2", 6, false, false)], ratings: [0, 0, 1, 3, 3] },
];

// the race tracks of the game in its level order: level, cup (0 Bronze .. 3 Platinum), challenge lap time of time trial (s) forward / reversed - mirrored uses the forward one
pub const LEVELS: [(&str, usize, f64, f64); 13] =
[
    ("nhood1", 0, 54.0, 54.0), ("market2", 0, 24.0, 24.0), ("muse2", 0, 45.0, 45.0), ("garden1", 0, 29.0, 29.0),
    ("toylite", 1, 27.0, 34.0), ("wild_west1", 1, 30.0, 30.0), ("toy2", 1, 37.0, 37.0),
    ("nhood2", 2, 45.0, 48.0), ("ship1", 2, 55.0, 55.0), ("muse1", 2, 61.0, 59.0),
    ("market1", 3, 39.0, 39.0), ("wild_west2", 3, 38.0, 38.0), ("ship2", 3, 52.0, 52.0),
];

pub const POINTS: [u32; 8] = [10, 6, 4, 3, 2, 1, 0, 0];
pub const TRIES: u32 = 3;
// the place a race has to end on at least
const QUALIFY: u32 = 3;
// one point moves from the race points to the total this often (s)
const POINT_STEP: f32 = 0.2;
pub const CARS: usize = 8;

// the scene of a track variant: <level>[_reversed][_mirrored]
pub fn scene_name(level: &str, reversed: bool, mirrored: bool) -> String
{
    format!("{level}{}{}", if reversed { "_reversed" } else { "" }, if mirrored { "_mirrored" } else { "" })
}

// the opponents of a cup: per rating that many random cars of it, no car twice (the player's own car can be among them like in the game)
pub fn opponents(cup: usize, random: &mut Random) -> Vec<String>
{
    let mut picked = vec![];
    for (rating, &count) in CUPS[cup].ratings.iter().enumerate()
    {
        let mut bucket = crate::field::cars_of_rating(rating as u32);
        for _ in 0..count.min(bucket.len())
        {
            let index = ((random.next() * bucket.len() as f32) as usize).min(bucket.len() - 1);
            picked.push(bucket.remove(index).to_string());
        }
    }
    picked
}

pub struct Standing
{
    pub name: String,
    pub car: String,
    pub player: bool,
    pub points: u32,
    // the points of the last race still counted into the total
    pub pending: u32,
    // the place per race (1 = first)
    pub places: Vec<u32>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Phase
{
    Racing,
    // the table after a race: the points count into the total, then Enter goes on
    Table,
    // worse than 3rd: try again (Yes / No) while tries are left, else game over
    Failed,
    GameOver,
}

pub struct Championship
{
    pub cup: usize,
    // the race of the cup (0..)
    pub stage: usize,
    pub tries: u32,
    pub phase: Phase,
    // the player first, then the opponents in the order of the cup
    pub standings: Vec<Standing>,
    pub opponents: Vec<String>,
    pub car: String,
    pub player_name: String,
    pub settings: RaceSettings,
    // the standings sorted, indices
    order: Vec<usize>,
    timer: f32,
    // Try Again?: Yes (0) / No (1)
    answer: usize,
    // the Enter that ended the race is not one for the table
    skip_input: bool,
}

pub enum CupEvent
{
    // name and speed
    Sound(&'static str, f32),
    // the next race of the cup, or the failed one again
    Race,
    // the cup is over: the place in the table (1 = won), None = game over / given up
    End(Option<usize>),
}

impl Championship
{
    pub fn new(cup: usize, car: &str, player_name: &str, settings: RaceSettings) -> Self
    {
        let opponents = opponents(cup, &mut Random::default());
        let mut standings = vec![Standing { name: player_name.to_string(), car: car.to_string(), player: true, points: 0, pending: 0, places: vec![] }];
        standings.extend(opponents.iter().map(|car| Standing { name: car.clone(), car: car.clone(), player: false, points: 0, pending: 0, places: vec![] }));
        let order = (0..standings.len()).collect();
        console_log!("championship: {} with {} against {}", CUPS[cup].name, car, opponents.join(", "));
        Self { cup, stage: 0, tries: TRIES, phase: Phase::Racing, standings, opponents, car: car.to_string(), player_name: player_name.to_string(), settings, order, timer: 0.0, answer: 0, skip_input: false }
    }

    pub fn current(&self) -> &CupRace
    {
        &CUPS[self.cup].races[self.stage.min(CUPS[self.cup].races.len() - 1)]
    }

    pub fn stages(&self) -> usize
    {
        CUPS[self.cup].races.len()
    }

    // the settings of the current race: its laps, 8 cars
    pub fn race_settings(&self) -> RaceSettings
    {
        let mut settings = self.settings;
        settings.laps = self.current().laps;
        settings.cars = CARS;
        settings.practice = false;
        settings
    }

    pub fn scene_name(&self) -> String
    {
        let race = self.current();
        scene_name(race.level, race.reversed, race.mirrored)
    }

    pub fn last_stage(&self) -> bool
    {
        self.stage + 1 >= self.stages()
    }

    // the race ended (Enter after the finish): the places, the points of the race - the table, or failed when worse than 3rd
    pub fn finish_race(&mut self, race: &Race) -> Vec<CupEvent>
    {
        let player_place = race.player().position;
        let mut used = vec![false; self.standings.len()];
        for racer in &race.racers
        {
            let index = match racer.is_local()
            {
                true => Some(0),
                false =>
                {
                    let car = racer.node.read().unwrap().extras.get::<String>("revolt_car").cloned().unwrap_or_default();
                    (1..self.standings.len()).find(|&i| !used[i] && self.standings[i].car == car)
                },
            };
            let Some(index) = index else { continue; };
            used[index] = true;
            if index > 0 { self.standings[index].name = racer.name.clone(); }
            let standing = &mut self.standings[index];
            standing.places.truncate(self.stage);
            standing.places.resize(self.stage, 0);
            standing.places.push(racer.position);
            standing.pending = POINTS.get(racer.position.saturating_sub(1) as usize).copied().unwrap_or(0);
        }
        self.sort();
        self.timer = 0.0;
        self.answer = 0;
        self.skip_input = true;
        self.phase = if player_place <= QUALIFY { Phase::Table } else if self.tries > 0 { Phase::Failed } else { Phase::GameOver };
        console_log!("championship: race {} of {} - place {}, {:?}", self.stage + 1, self.stages(), player_place, self.phase);
        vec![CupEvent::Sound("menuprev", crate::menu::MENU_SOUND_SPEED)]
    }

    // Give Up Try of the pause menu: a try less, the race again - the last try ends the cup
    pub fn give_up(&mut self) -> Vec<CupEvent>
    {
        if self.tries == 0 { return vec![CupEvent::End(None)]; }
        self.tries -= 1;
        self.phase = Phase::Racing;
        vec![CupEvent::Race]
    }

    // the points count in, Enter goes on - the race once more or the end
    pub fn update(&mut self, pad: &Buttons, dt: f32) -> Vec<CupEvent>
    {
        let mut events = vec![];
        if std::mem::take(&mut self.skip_input) { return events; }
        let enter = pad.enter || pad.start;
        match self.phase
        {
            Phase::Racing => {},
            Phase::Table =>
            {
                let counting = self.standings.iter().any(|s| s.pending > 0);
                if counting
                {
                    self.timer += dt;
                    if enter
                    {
                        // a key puts all points in at once
                        for standing in &mut self.standings { standing.points += std::mem::take(&mut standing.pending); }
                        events.push(CupEvent::Sound("menunext", crate::menu::MENU_SOUND_SPEED));
                    }
                    else if self.timer >= POINT_STEP
                    {
                        self.timer = 0.0;
                        for standing in self.standings.iter_mut().filter(|s| s.pending > 0)
                        {
                            standing.pending -= 1;
                            standing.points += 1;
                        }
                        events.push(CupEvent::Sound("menuleftright", 1.0));
                    }
                    self.sort();
                }
                else if enter
                {
                    events.push(CupEvent::Sound("menunext", crate::menu::MENU_SOUND_SPEED));
                    match self.last_stage()
                    {
                        true => events.push(CupEvent::End(Some(self.player_rank()))),
                        false =>
                        {
                            self.stage += 1;
                            self.phase = Phase::Racing;
                            events.push(CupEvent::Race);
                        },
                    }
                }
            },
            Phase::Failed =>
            {
                if pad.left || pad.right || pad.up || pad.down
                {
                    self.answer = 1 - self.answer;
                    events.push(CupEvent::Sound("menuupdown", crate::menu::MENU_SOUND_SPEED));
                }
                if enter
                {
                    events.push(CupEvent::Sound("menunext", crate::menu::MENU_SOUND_SPEED));
                    // a failed race counts nothing
                    for standing in &mut self.standings
                    {
                        standing.pending = 0;
                        standing.places.truncate(self.stage);
                    }
                    self.sort();
                    match self.answer
                    {
                        0 => { self.tries -= 1; self.phase = Phase::Racing; events.push(CupEvent::Race); },
                        _ => events.push(CupEvent::End(None)),
                    }
                }
            },
            Phase::GameOver =>
            {
                if enter
                {
                    events.push(CupEvent::Sound("menunext", crate::menu::MENU_SOUND_SPEED));
                    events.push(CupEvent::End(None));
                }
            },
        }
        events
    }

    // the first three of the table: car id and name
    pub fn podium(&self) -> Vec<(String, String)>
    {
        self.order.iter().take(3).map(|&i| (self.standings[i].car.clone(), self.standings[i].name.clone())).collect()
    }

    // 1 = top of the table
    pub fn player_rank(&self) -> usize
    {
        self.order.iter().position(|&i| self.standings[i].player).map_or(self.standings.len(), |p| p + 1)
    }

    // the most points first, the same points: the better place in the last race
    fn sort(&mut self)
    {
        let stage = self.stage;
        let standings = &self.standings;
        self.order.sort_by(|&a, &b|
        {
            let (a, b) = (&standings[a], &standings[b]);
            let place = |s: &Standing| s.places.get(stage).copied().unwrap_or(u32::MAX);
            b.points.cmp(&a.points).then(place(a).cmp(&place(b)))
        });
    }
}


// the table of the cup over the race, like the game: Car, one column per race so far, Pnts - the messages above it
pub struct CupScreen
{
    assets: String,
    font: Option<TextureHandle>,
    failed: bool,
}

const GREEN: Color32 = Color32::from_rgb(0, 255, 0);
const YELLOW: Color32 = Color32::from_rgb(255, 255, 0);
const RED: Color32 = Color32::from_rgb(255, 0, 0);
const SELECTED: Color32 = Color32::from_rgb(0xff, 0x77, 0x22);
const BOX: Color32 = Color32::from_rgba_premultiplied(14, 14, 16, 0xa0);
const RIM: Color32 = Color32::from_rgb(0xc0, 0x30, 0x28);
// chars 8 x 16, a column per race 56 wide, the header at 176, rows from 208
const CHAR_W: f32 = 8.0;
const CHAR_H: f32 = 16.0;
const COLUMN: f32 = 56.0;
const HEADER_Y: f32 = 176.0;
const ROWS_Y: f32 = 208.0;
const MESSAGE_Y: f32 = 100.0;

impl CupScreen
{
    pub fn new(assets: &str) -> Self
    {
        Self { assets: assets.to_string(), font: None, failed: false }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, cup: &Championship)
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
                    self.font = Some(ui.ctx().load_texture("revolt_cup_font", egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()), TextureOptions::LINEAR));
                },
                Err(error) => { console_warning!("cup: {}: {}", path, error); self.failed = true; },
            }
        }
        let Some(font) = &self.font else { return; };
        let mut painter = Painter::new(ui.ctx().content_rect(), vec![font.id(), egui::TextureId::default()]);
        let races = cup.stage + 1;
        let x0 = (400.0 - (races - 1) as f32 * COLUMN) * 0.5;
        let rows = cup.standings.len() as f32;
        let right = x0 + races as f32 * COLUMN + 200.0;
        painter.rounded(Anchor::Center, [x0 - 16.0, HEADER_Y - 12.0, right - x0 + 16.0, ROWS_Y - HEADER_Y + 12.0 + rows * CHAR_H + 12.0], BOX, RIM, 2.0, 8.0);

        painter.text(Anchor::Center, x0, HEADER_Y, CHAR_W, CHAR_H, CYAN, "Car");
        for i in 0..races
        {
            painter.text(Anchor::Center, x0 + i as f32 * COLUMN + 152.0, HEADER_Y, CHAR_W, CHAR_H, CYAN, &(i + 1).to_string());
        }
        painter.text(Anchor::Center, x0 + (races - 1) as f32 * COLUMN + 212.0 - 2.0 * CHAR_W, HEADER_Y, CHAR_W, CHAR_H, CYAN, "Pnts");
        for (row, &index) in cup.order.iter().enumerate()
        {
            let standing = &cup.standings[index];
            let y = ROWS_Y + row as f32 * CHAR_H;
            let name: String = standing.name.chars().take(15).collect();
            painter.text(Anchor::Center, x0, y, CHAR_W, CHAR_H, if standing.player { SELECTED } else { WHITE }, &name);
            for (i, place) in standing.places.iter().enumerate().take(races)
            {
                if *place == 0 { continue; }
                painter.text(Anchor::Center, x0 + i as f32 * COLUMN + 144.0, y, CHAR_W, CHAR_H, GREEN, &format!("{place}{}", suffix(*place)));
            }
            let points_x = x0 + races as f32 * COLUMN + 148.0;
            painter.text(Anchor::Center, points_x, y, CHAR_W, CHAR_H, YELLOW, &format!("{:02}", standing.points));
            if standing.pending > 0
            {
                painter.text(Anchor::Center, points_x + 24.0, y, CHAR_W, CHAR_H, RED, &format!("+{}", standing.pending));
            }
        }

        let centered = |painter: &mut Painter, y: f32, color: Color32, text: &str| painter.text(Anchor::Center, (WIDTH - text.len() as f32 * CHAR_W) * 0.5, y, CHAR_W, CHAR_H, color, text);
        let bottom = (ROWS_Y + (rows + 1.5) * CHAR_H).min(HEIGHT - 40.0);
        match cup.phase
        {
            Phase::Table if cup.last_stage() => { centered(&mut painter, MESSAGE_Y, WHITE, "CONGRATULATIONS!"); centered(&mut painter, MESSAGE_Y + CHAR_H, WHITE, "CHAMPIONSHIP FINISHED!"); },
            Phase::Table => { centered(&mut painter, MESSAGE_Y, WHITE, "CONGRATULATIONS!"); centered(&mut painter, MESSAGE_Y + CHAR_H, WHITE, "YOU QUALIFIED!"); },
            Phase::Failed =>
            {
                centered(&mut painter, MESSAGE_Y, WHITE, "YOU FAILED TO QUALIFY!");
                let tries = format!("You have {} {}  Try Again?", cup.tries, if cup.tries == 1 { "Try Left." } else { "Tries Left." });
                centered(&mut painter, bottom, WHITE, &tries);
                let (yes, no) = if cup.answer == 0 { (SELECTED, WHITE) } else { (WHITE, SELECTED) };
                painter.text(Anchor::Center, WIDTH * 0.5 - 6.0 * CHAR_W, bottom + CHAR_H * 1.5, CHAR_W, CHAR_H, yes, "Yes");
                painter.text(Anchor::Center, WIDTH * 0.5 + 3.0 * CHAR_W, bottom + CHAR_H * 1.5, CHAR_W, CHAR_H, no, "No");
            },
            Phase::GameOver =>
            {
                centered(&mut painter, MESSAGE_Y, WHITE, "YOU FAILED TO QUALIFY!");
                centered(&mut painter, bottom, WHITE, "Game Over!  Better Luck Next Time!");
            },
            Phase::Racing => {},
        }
        if cup.phase == Phase::Table && !cup.standings.iter().any(|s| s.pending > 0)
        {
            centered(&mut painter, bottom, CYAN, "Press Enter to continue");
        }
        painter.finish(ui.ctx(), &[1, 0]);
    }
}

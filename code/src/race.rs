// a race like the game (formats/README.md, Race progress): every car is a racer - driven by a local player or a bot, later also over the network
// the zones (.taz) gate the progress on the position nodes (.pan); the places come from the progress, the HUD shows a local racer

use std::f32::consts::PI;

use crate::assist::{self, DriveMode};
use crate::pad::Buttons;
use rustl::prelude::*;
use rustl::serde_json;
use rustl::state::scene::camera::Camera;
use rustl::state::scene::scene_controller::vehicle_controller::{VehicleController, VehicleDriveInput};

use crate::bot::{self, Bot, BotAction, BotView, Difficulty};
use crate::cars;
use crate::field::{self, Opponent, Slot};
use crate::random::Random;
use crate::track::{find_node, AiPlace, Track};

// the settings of the menu (Game Settings) - the defaults of the game: 3 laps, 8 cars, pickups on
#[derive(Clone, Copy, Debug)]
pub struct RaceSettings
{
    pub laps: u32,
    // all cars of the race, the player included - 1 = alone
    pub cars: usize,
    pub difficulty: Difficulty,
    pub pickups: bool,
    // practice (game mode 8): laps count, but nobody finishes - free driving, the practice star to find
    pub practice: bool,
    // time trial: alone, the laps go into the Best Trial Times, the challenge lap time of the track to beat
    pub trial: bool,
    // the speed in the HUD: km/h, else mph like the game
    pub kmh: bool,
    // Select Mode of the game: the driving aids of the player cars (split screen: Racer.mode per player)
    pub mode: DriveMode,
}

impl Default for RaceSettings
{
    fn default() -> Self
    {
        Self { laps: 3, cars: field::CARS, difficulty: Difficulty::Hard, pickups: true, practice: false, trial: false, kmh: true, mode: DriveMode::Arcade }
    }
}
// the HUD shows wrong way only after the state held this long (s)
const WRONG_WAY_DELAY: f32 = 1.0;
// Junior RC: (m of lead over the best Junior RC player, share of its speed) - in between linear, beyond the last one it keeps driving that slow
// close in front nearly full speed (it must not look slow), only out of sight it waits
const CATCH_UP: [(f32, f32); 6] = [(-20.0, 1.0), (0.0, 0.93), (25.0, 0.9), (60.0, 0.7), (120.0, 0.45), (200.0, 0.25)];
// d . look above this: the car looks against the track
const WRONG_WAY_DOT: f32 = 0.6;
// rad/s the direction arrow turns at most
const ARROW_RATE: f32 = 3.0;
// the lap fraction wraps from below this to above 1 - this at the finish line
const FINISH_LOW: f32 = 0.25;
const FINISH_HIGH: f32 = 0.75;
// 8 speed steps per TopEnd for the 7 speed dots (the exe uses the wheel speed)
const SPEED_STEPS: f32 = 8.0;
// the countdown of the exe: 5 s, the numbers in the last 3, a beep (22050 Hz) at 3, 2, 1 below 3.5 s, at GO the beep at 31000 and 37000 Hz together
pub const COUNTDOWN: f64 = 5.0;
const COUNTDOWN_BEEPS: f64 = 3.5;
const BEEP_PITCH: f32 = 1.0;
const GO_PITCHES: [f32; 2] = [31000.0 / 22050.0, 37000.0 / 22050.0];
// direction signs: the timer starts at 640 and runs 720/s, sign 11 shows nothing
pub const SIGN_TIME: f32 = 640.0;
const SIGN_RATE: f32 = 720.0;
const SIGN_NONE: usize = 11;
// the race ends this long after the last local player finished, if the bots are not through yet (s)
const FINISH_WAIT: f32 = 20.0;
// m above the track a car is put back
const RESET_LIFT: f32 = 0.3;
// m/s above which a held car brakes instead of only the handbrake
const HOLD_SPEED: f32 = 1.0;
// a bot that gets less than this many m further along the track in this many s is put back onto the line - generous, the reverse gets the first try
const STALL_PROGRESS: f32 = 4.0;
const STALL_TIME: f32 = 7.0;
// m along the track after the last stall before a stall counts as the first again - the jump of the reset itself is no way out
const STALL_CLEAR: f32 = 50.0;
// game metres per metre of ours (the HUD of the game: 0.005 m per unit, ours 0.01)
const GAME_METRES: f32 = 0.5;

pub enum Driver
{
    // the index of the local player (split screen: 0..players)
    Local(u32),
    Bot(Bot),
}

// a car ahead / behind in the race: metres of the game and its name
pub struct Neighbour
{
    pub metres: f32,
    pub name: String,
}

pub struct Racer
{
    pub node: NodeItem,
    id: u32,
    pub name: String,
    pub driver: Driver,
    // the grid slot, None without grid (stays where the converter put it)
    slot: Option<usize>,
    placed: bool,
    // m/s of TopEnd
    top_speed: f32,

    zone: i32,
    pan_node: usize,
    // distance to the finish line along the track, m
    remaining: f32,
    // remaining / lap length, 0..1
    fraction: f32,
    // the first progress update only takes the fraction - from 0 it looked like a crossing of the line (a lap of 0 s later)
    fraction_known: bool,
    // crossed the line backwards - the next forward crossing only undoes it (at the start: the cars stand behind the line)
    backwards: bool,
    // the lap clock starts at the first crossing
    not_crossed: bool,
    wrong_way_flag: bool,
    wrong_way_timer: f32,
    // the AI segment a local car is on (put back onto the track)
    ai: Option<AiPlace>,

    pub laps: u32,
    lap_start: f64,
    pub last_lap: Option<f64>,
    pub best_lap: Option<f64>,
    pub finish_time: Option<f64>,
    pub wrong_way: bool,
    // turn of the direction arrow, rad, positive = clockwise (the track goes right)
    pub arrow: f32,
    // m/s
    pub speed: f32,
    // speed / TopEnd in speed steps (7 dots)
    pub speed_steps: f32,
    // 1 = leading
    pub position: u32,
    // the direction sign on screen (0..10, fxpage2) and its timer (640 -> 0, the exe fades it in above 512 and out below 128)
    pub sign: Option<usize>,
    pub sign_timer: f32,
    sign_order: i32,
    // the best progress score so far, the time without STALL_PROGRESS beyond it, the stalls in a row at one place
    best_score: f32,
    stall_time: f32,
    // Junior RC: s the player car is lost (upside down, the wrong way, ...) - it is put back onto the track after a while
    lost_time: f32,
    stuck: assist::StuckWatch,
    stalls: usize,
    stall_score: f32,
    pub ahead: Option<Neighbour>,
    pub behind: Option<Neighbour>,
    // the driving aids of a local player
    pub mode: DriveMode,
}

// a viewport: racer index, camera id, viewport (x, y from the bottom left, width, height)
pub type View = (usize, u32, [f32; 4]);

pub struct Race
{
    pub settings: RaceSettings,
    scene_id: u32,
    track: Track,
    grid: Vec<Slot>,
    pub racers: Vec<Racer>,
    // the opponents are loaded - the countdown waits for them
    ready: bool,
    random: Random,
    // s until GO, 0 once the race runs
    pub countdown: f64,
    // s since GO
    pub race_time: f64,
    // s since the last local player finished
    finish_wait: f32,
    // the race is over - the results show, Enter starts it again
    pub over: bool,
    // the racer the camera follows instead of the player car (B), the player car waits meanwhile
    watching: Option<usize>,
    // the game camera the player car drives
    cam_name: Option<String>,
    // V: every car in a viewport of its own
    split: Vec<View>,
    // split screen of several local players: their viewports (the app made their cameras), the cells left over show bots
    multi: Vec<View>,
    empty_cells: Vec<[f32; 4]>,
    // the bot cameras of the left over cells - removed on Stop
    cell_cams: Vec<u32>,
    // the viewport of the player camera before
    split_restore: Option<[f32; 4]>,
    // Simulation: the wall slide of a player car before it was turned off - back on Stop
    wall_slides: Vec<(u32, f32)>,
}

impl Racer
{
    fn new(node: NodeItem, driver: Driver, slot: Option<usize>, start_node: usize) -> Self
    {
        let (id, name, top_speed) =
        {
            let car = node.read().unwrap();
            let model = find_node(&car.nodes, &|n| n.name == "model");
            let model_number = |key: &str| model.as_ref().and_then(|model| number(&model.read().unwrap(), key));
            let name = car.extras.get::<String>("revolt_name").cloned().unwrap_or_else(|| car.name.clone());
            // TopEnd of Parameters.txt on the model node, units/s
            let top = model.as_ref().and_then(|model| crate::cars::top_end(&model.read().unwrap())).map(|top| top * model_number("revolt_scale").unwrap_or(0.01)).unwrap_or(10.0);
            (car.id, name, top)
        };

        Self
        {
            node, id, name, driver, slot, placed: false, top_speed,
            zone: 0, pan_node: start_node, remaining: 0.0, fraction: 0.0, fraction_known: false, backwards: true, not_crossed: true,
            wrong_way_flag: false, wrong_way_timer: 0.0, ai: None,
            laps: 0, lap_start: 0.0, last_lap: None, best_lap: None, finish_time: None, wrong_way: false, arrow: 0.0,
            speed: 0.0, speed_steps: 0.0, position: 1, sign: None, sign_timer: 0.0, sign_order: 0, ahead: None, behind: None,
            best_score: f32::MIN, stall_time: 0.0, lost_time: 0.0, stuck: assist::StuckWatch::default(), stalls: 0, stall_score: f32::MIN,
            mode: DriveMode::default(),
        }
    }

    pub fn finished(&self) -> bool
    {
        self.finish_time.is_some()
    }

    pub fn is_local(&self) -> bool
    {
        matches!(self.driver, Driver::Local(_))
    }

    // the time of the running lap, s
    pub fn lap_time(&self, race_time: f64) -> f64
    {
        race_time - self.lap_start
    }

    // higher is further ahead, m: laps minus the open backward crossing plus the share of this lap - by the wrapped fraction, remaining jumps by a lap behind the start node
    fn score(&self, length: f32) -> f32
    {
        (self.laps as f32 - self.backwards as i32 as f32 + 1.0 - self.fraction) * length
    }

    // back to the start of a race, on the grid again
    fn restart(&mut self, start_node: usize, random: &mut Random, difficulty: Difficulty)
    {
        let driver = match &self.driver
        {
            Driver::Local(index) => Driver::Local(*index),
            Driver::Bot(_) => Driver::Bot(Bot::new(random, difficulty)),
        };
        // the name of the menu and the mode stay (new() takes the car name)
        let (name, mode) = (std::mem::take(&mut self.name), self.mode);
        *self = Self::new(self.node.clone(), driver, self.slot, start_node);
        self.name = name;
        self.mode = mode;
    }
}

impl Race
{
    // a level scene with track data and a start car - None otherwise (showroom, battle levels)
    // the player drives the start car, or the cars of the menu (the start car then only brings the grid) - several of them in split screen
    pub fn start(scene: &Scene, players: Vec<NodeItem>, settings: RaceSettings) -> Option<Self>
    {
        let track = Track::from_scene(scene)?;
        let start_car = find_node(&scene.nodes, &|node| node.tags.contains("revolt_start_car"))?;
        let grid = field::grid(&start_car.read().unwrap());
        let players = if players.is_empty() { vec![start_car] } else { players };
        if grid.is_empty() { console_warning!("race: no start grid on the start car - re-convert the level for opponents"); }
        if !track.has_ai() { console_warning!("race: no AI nodes - re-convert the level for opponents"); }

        console_log!("race: {} zones, {} position nodes, {} AI nodes, lap {:.0} m, {} laps", track.zones.len(), track.nodes.len(), track.ai.len(), track.length, settings.laps);
        let start = track.start;
        let racers = players.into_iter().enumerate().map(|(index, car)| { let mut racer = Racer::new(car, Driver::Local(index as u32), None, start); racer.mode = settings.mode; racer }).collect();
        Some(Self
        {
            settings,
            scene_id: scene.id,
            grid,
            racers,
            track,
            ready: false,
            random: Random::default(),
            countdown: COUNTDOWN,
            race_time: 0.0,
            finish_wait: 0.0,
            over: false,
            watching: None,
            cam_name: None,
            split: vec![],
            multi: vec![],
            empty_cells: vec![],
            cell_cams: vec![],
            split_restore: None,
            wall_slides: vec![],
        })
    }

    pub fn scene_id(&self) -> u32
    {
        self.scene_id
    }

    // the first local racer - the one the HUD shows
    pub fn player(&self) -> &Racer
    {
        &self.racers[self.player_index()]
    }

    pub fn player_index(&self) -> usize
    {
        self.racers.iter().position(Racer::is_local).unwrap_or(0)
    }

    // the racer indices of the local players, in the order of the players
    pub fn locals(&self) -> Vec<usize>
    {
        let mut locals: Vec<(u32, usize)> = self.racers.iter().enumerate().filter_map(|(i, racer)| match racer.driver { Driver::Local(player) => Some((player, i)), _ => None }).collect();
        locals.sort();
        locals.into_iter().map(|(_, i)| i).collect()
    }

    // split screen of several local players
    pub fn multi(&self) -> bool
    {
        !self.multi.is_empty()
    }

    // every local player is through (alone: the player)
    pub fn locals_finished(&self) -> bool
    {
        self.racers.iter().filter(|racer| racer.is_local()).all(Racer::finished)
    }

    // the viewports of the local players (the app made their cameras) and the cells left for bots
    pub fn set_multi(&mut self, views: Vec<View>, empty_cells: Vec<[f32; 4]>)
    {
        self.multi = views;
        self.empty_cells = empty_cells;
    }

    // the racer the HUD shows: the watched one, else the player
    pub fn viewed_index(&self) -> usize
    {
        self.watching.unwrap_or_else(|| self.player_index())
    }

    pub fn viewed(&self) -> &Racer
    {
        &self.racers[self.viewed_index()]
    }

    pub fn watching(&self) -> Option<usize>
    {
        self.watching
    }

    // the viewports of the split view or the split screen, empty without them
    pub fn split_views(&self) -> &[View]
    {
        if self.multi.is_empty() { &self.split } else { &self.multi }
    }

    // split screen: the cells no player has show the first bots, once they are there
    fn fill_cells(&mut self, scene: &mut Scene)
    {
        if self.empty_cells.is_empty() || !self.cell_cams.is_empty() { return; }
        let bots: Vec<usize> = (0..self.racers.len()).filter(|&i| !self.racers[i].is_local()).collect();
        if bots.is_empty() { return; }
        let Some(&(_, main_id, _)) = self.multi.first() else { return; };
        let Some(data) = scene.cameras.iter().find(|cam| cam.id == main_id).map(|cam| serde_json::to_value(cam.get_data()).unwrap_or_default()) else { return; };
        // more cells than bots: the rest stays black
        let cells = std::mem::take(&mut self.empty_cells);
        self.empty_cells = cells.iter().skip(bots.len()).copied().collect();
        for (cell, (view, index)) in cells.into_iter().zip(bots).enumerate()
        {
            let name = format!("Re-Volt cell {}", cell);
            let id = add_camera(scene, &data, &name, view);
            if let Some(Some(error)) = with_vehicle(scene, self.racers[index].id, |vehicle, scene| vehicle.attach_camera(scene, &name))
            {
                console_error!("race: split screen cell - {}", error);
            }
            self.cell_cams.push(id);
            self.multi.push((index, id, view));
        }
    }

    // V: every car in a viewport of its own, the player in the first one and still driving - V again goes back
    fn toggle_split(&mut self, scene: &mut Scene)
    {
        if !self.split.is_empty()
        {
            self.end_split(scene);
            return;
        }
        if self.watching.is_some()
        {
            self.watch(scene, None);
        }

        let player = self.player_index();
        let player_id = self.racers[player].id;
        if self.cam_name.is_none()
        {
            self.cam_name = vehicle(scene, player_id).map(|vehicle| vehicle.cam_name.clone());
        }
        let cam_name = self.cam_name.clone().unwrap_or_default();
        let Some(main) = scene.get_game_camera_mut(&cam_name) else { return; };
        let data = serde_json::to_value(main.get_data()).unwrap_or_default();
        let viewport = |key: &str, default: f64| data["viewport"][key].as_f64().unwrap_or(default) as f32;
        self.split_restore = Some([viewport("x", 0.0), viewport("y", 0.0), viewport("width", 1.0), viewport("height", 1.0)]);
        let main_id = main.id;
        let resolution = resolution(&data);

        // rows for the screen shape - 8 cars on 16:9 are 4 x 2; the player first
        let order: Vec<usize> = std::iter::once(player).chain((0..self.racers.len()).filter(|&i| i != player)).collect();
        let aspect = resolution[0] as f32 / resolution[1].max(1) as f32;
        let rows = ((order.len() as f32 / aspect).sqrt().round() as usize).clamp(1, order.len().max(1));
        let columns = order.len().div_ceil(rows);
        for (cell, &index) in order.iter().enumerate()
        {
            let (column, row) = (cell % columns, cell / columns);
            let (w, h) = (1.0 / columns as f32, 1.0 / rows as f32);
            let view = [column as f32 * w, 1.0 - (row + 1) as f32 * h, w, h];

            if index == player
            {
                if let Some(main) = scene.cameras.iter_mut().find(|cam| cam.id == main_id) { main.update_viewport(view[0], view[1], view[2], view[3]); }
                self.split.push((index, main_id, view));
                continue;
            }

            let name = format!("Re-Volt view {}", cell);
            let id = add_camera(scene, &data, &name, view);
            if let Some(Some(error)) = with_vehicle(scene, self.racers[index].id, |vehicle, scene| vehicle.attach_camera(scene, &name))
            {
                console_error!("race: split view - {}", error);
            }
            self.split.push((index, id, view));
        }
        console_log!("race: split view of {} cars - V goes back", self.split.len());
    }

    // the extra cameras go, the player camera gets its viewport back
    fn end_split(&mut self, scene: &mut Scene)
    {
        let player = self.player_index();
        for (index, cam_id, _) in std::mem::take(&mut self.split)
        {
            if index == player
            {
                let [x, y, w, h] = self.split_restore.unwrap_or([0.0, 0.0, 1.0, 1.0]);
                if let Some(main) = scene.cameras.iter_mut().find(|cam| cam.id == cam_id) { main.update_viewport(x, y, w, h); }
                continue;
            }
            with_vehicle(scene, self.racers[index].id, |vehicle, _| vehicle.camera.enabled = false);
            scene.delete_camera_by_id(cam_id);
        }
        console_log!("race: split view off");
    }

    // B: the camera to the next bot, after the last one back to the player car
    fn watch_next(&mut self, scene: &mut Scene)
    {
        let bots: Vec<usize> = (0..self.racers.len()).filter(|&i| !self.racers[i].is_local()).collect();
        let next = match self.watching
        {
            None => bots.first().copied(),
            Some(current) => bots.iter().copied().find(|&i| i > current),
        };
        self.watch(scene, next);
    }

    fn watch(&mut self, scene: &mut Scene, next: Option<usize>)
    {
        let player = self.racers[self.player_index()].id;
        if self.cam_name.is_none()
        {
            self.cam_name = vehicle(scene, player).map(|vehicle| vehicle.cam_name.clone());
        }
        let cam_name = self.cam_name.clone().unwrap_or_default();

        let from = self.racers[self.viewed_index()].id;
        let to = next.map_or(player, |index| self.racers[index].id);
        if from == to { return; }

        with_vehicle(scene, from, |vehicle, _| vehicle.camera.enabled = false);
        if let Some(Some(error)) = with_vehicle(scene, to, |vehicle, scene| vehicle.attach_camera(scene, &cam_name))
        {
            console_error!("race: camera - {}", error);
        }
        self.watching = next;
        match next
        {
            Some(index) => { console_log!("race: watching {} - B for the next car", self.racers[index].name); },
            None => { console_log!("race: back in the own car"); },
        }
    }

    // opponents are possible: more cars than players, a grid and the AI nodes
    pub fn wants_opponents(&self) -> bool
    {
        !self.ready && self.settings.cars > self.racers.len() && !self.grid.is_empty() && self.track.has_ai()
    }

    pub fn ready(&self) -> bool
    {
        self.ready
    }

    pub fn grid(&self) -> &[Slot]
    {
        &self.grid
    }

    // the opponents take the front slots, the local players the ones behind them - then the countdown runs
    pub fn add_opponents(&mut self, opponents: Vec<Opponent>)
    {
        let start = self.track.start;
        let bots = opponents.len();
        for (index, opponent) in opponents.into_iter().enumerate()
        {
            let bot = Bot::new(&mut self.random, self.settings.difficulty);
            self.racers.push(Racer::new(opponent.node, Driver::Bot(bot), Some(index), start));
        }
        let mut slot = bots;
        for racer in self.racers.iter_mut().filter(|racer| racer.is_local())
        {
            racer.slot = (slot < self.grid.len()).then_some(slot);
            slot += 1;
        }
        self.ready = true;
    }

    // no opponents (no grid or no AI nodes): the start car stays where it is
    // split screen: the local players keep the front slots they were spawned on, so a restart puts them back there
    pub fn set_ready(&mut self)
    {
        let grid = self.grid.len();
        if !self.multi.is_empty()
        {
            for (slot, racer) in self.racers.iter_mut().filter(|racer| racer.is_local()).enumerate()
            {
                racer.slot = (slot < grid).then_some(slot);
            }
        }
        self.ready = true;
    }

    // split screen: the cells no player has and no bot fills
    pub fn empty_cells(&self) -> &[[f32; 4]]
    {
        if self.ready { &self.empty_cells } else { &[] }
    }

    // the pitches of the countdown beeps to play this frame - pad: the buttons of all devices, locals: the buttons of each local player
    pub fn update(&mut self, scene: &mut Scene, pad: &Buttons, locals: &[Buttons], dt: f32) -> Vec<f32>
    {
        let mut beeps = vec![];
        // the brake would turn into reverse at a standstill - only the handbrake holds the car
        let locked = Some(VehicleDriveInput { handbrake: true, ..Default::default() });
        if !self.ready
        {
            for racer in &self.racers { set_input(scene, racer.id, locked); }
            return beeps;
        }

        self.place_on_grid(scene);
        self.fill_cells(scene);

        // watching and the split view are for one player - split screen shows the cars already
        if self.multi.is_empty() && pad.split
        {
            self.toggle_split(scene);
        }
        else if self.multi.is_empty() && pad.watch && self.split.is_empty()
        {
            self.watch_next(scene);
        }

        // the countdown: the cars wait, zone and progress follow them, no clock
        if self.countdown > 0.0
        {
            let old = self.countdown;
            self.countdown -= dt as f64;
            if self.countdown > 0.0
            {
                if self.countdown < COUNTDOWN_BEEPS && self.countdown.floor() < old.floor()
                {
                    beeps.push(BEEP_PITCH);
                }
            }
            else
            {
                // GO - the clock starts with what is left of the frame
                beeps.extend(GO_PITCHES);
                self.race_time = -self.countdown - dt as f64;
                self.countdown = 0.0;
                console_log!("race: go");
            }
        }
        let running = self.countdown <= 0.0;
        if running && !self.over
        {
            self.race_time += dt as f64;
        }

        for index in 0..self.racers.len()
        {
            self.update_racer(scene, index, running, dt);
            // the race is over: every car brakes to a stop and stays there
            let mut action = if running && !self.over { self.control(scene, index, locals, dt) } else { None };
            if running && !self.over && !matches!(action, Some(BotAction::Reset { .. }))
            {
                action = self.check_stall(index, dt).or(action);
            }
            let watching = self.watching.is_some();
            // finished cars (the bots too) brake to a stop; the player car also waits while the camera is on another car
            let (id, hold, speed) = { let racer = &self.racers[index]; (racer.id, !running || self.over || racer.finished() || racer.is_local() && watching, racer.speed) };
            match action
            {
                Some(BotAction::Reset { position, heading, reason }) => self.put_on_track(scene, index, position, heading, reason),
                Some(BotAction::Drive(drive)) => set_input(scene, id, Some(drive)),
                // a held car brakes to a stop, then the handbrake holds it
                None if hold && speed > HOLD_SPEED => set_input(scene, id, Some(VehicleDriveInput { brake: 1.0, ..Default::default() })),
                None if hold => set_input(scene, id, locked),
                None => set_input(scene, id, None),
            }
            if self.racers[index].is_local()
            {
                self.assist(scene, index, running && !self.over && !hold, dt);
            }
        }

        self.update_positions();
        self.update_end(dt);
        beeps
    }

    // the cars on their slots once their bodies exist
    fn place_on_grid(&mut self, scene: &mut Scene)
    {
        for racer in self.racers.iter_mut().filter(|racer| !racer.placed)
        {
            let Some(slot) = racer.slot.and_then(|slot| self.grid.get(slot)).copied() else { racer.placed = true; continue; };
            if !scene.physics.has_vehicle(racer.id) { continue; }
            with_vehicle(scene, racer.id, |vehicle, scene| vehicle.place(scene, slot.floor + Vector3::y() * (cars::ground_height(vehicle) + cars::LIFT), slot.heading));
            racer.placed = true;
        }
    }

    // a local player drives with the bindings (Backspace puts the car back onto the track), a bot by the racing line
    fn control(&mut self, scene: &Scene, index: usize, locals: &[Buttons], dt: f32) -> Option<BotAction>
    {
        let catch_up = self.catch_up(index);
        let view = view(scene, &self.racers[index]).map(|view| BotView { catch_up, ..view });
        let track = &self.track;
        let racer = &mut self.racers[index];
        let (finished, ai) = (racer.finished(), racer.ai);
        match &mut racer.driver
        {
            Driver::Local(_) if finished || self.watching.is_some() => None,
            Driver::Local(player) =>
            {
                if !locals.get(*player as usize).is_some_and(|pad| pad.reset) { return None; }
                ai.map(|place| bot::reset_action(track, place, 0, "by the player"))
            },
            Driver::Bot(_) if finished => None,
            Driver::Bot(bot) => view.map(|view| bot.drive(track, &view, dt)),
        }
    }

    // Junior RC: a bot ahead of the best Junior RC player that still races slows down by its lead - the player can catch up
    fn catch_up(&self, index: usize) -> f32
    {
        let length = self.track.length;
        let best = self.racers.iter()
            .filter(|racer| matches!(racer.driver, Driver::Local(_)) && racer.mode == DriveMode::JuniorRc && !racer.finished())
            .map(|racer| racer.score(length))
            .fold(f32::MIN, f32::max);
        if best == f32::MIN { return 1.0; }
        let lead = self.racers[index].score(length) - best;
        let (last_lead, slowest) = CATCH_UP[CATCH_UP.len() - 1];
        if lead >= last_lead { return slowest; }
        CATCH_UP.windows(2)
            .find(|pair| lead < pair[1].0)
            .map(|pair| if lead <= pair[0].0 { pair[0].1 } else { pair[0].1 + (lead - pair[0].0) / (pair[1].0 - pair[0].0) * (pair[1].1 - pair[0].1) })
            .unwrap_or(1.0)
    }

    // a bot that makes no progress along the track for STALL_TIME is put back ahead - one node further each time it stalls in a row
    fn check_stall(&mut self, index: usize, dt: f32) -> Option<BotAction>
    {
        let length = self.track.length;
        let track = &self.track;
        let racer = &mut self.racers[index];
        if racer.finished() { return None; }
        let score = racer.score(length);
        let Driver::Bot(bot) = &mut racer.driver else { return None; };

        if score > racer.best_score + STALL_PROGRESS
        {
            racer.best_score = score;
            racer.stall_time = 0.0;
            if score > racer.stall_score + STALL_CLEAR { racer.stalls = 0; }
            return None;
        }
        racer.stall_time += dt;
        if racer.stall_time < STALL_TIME { return None; }

        racer.stall_time = 0.0;
        racer.stalls += 1;
        racer.stall_score = score;
        console_log!("race: {} made no progress for {:.0} s ({}. time)", racer.name, STALL_TIME, racer.stalls);
        bot.reset_ahead(track, racer.stalls)
    }

    fn put_on_track(&mut self, scene: &mut Scene, index: usize, position: Vector3<f32>, heading: Vector3<f32>, reason: &str)
    {
        let racer = &self.racers[index];
        console_log!("race: {} put back onto the track ({})", racer.name, reason);
        with_vehicle(scene, racer.id, |vehicle, scene| vehicle.place(scene, position + Vector3::y() * (cars::ground_height(vehicle) + RESET_LIFT), heading));
    }

    fn update_racer(&mut self, scene: &Scene, index: usize, running: bool, dt: f32)
    {
        let race_time = self.race_time;
        let track = &self.track;
        let racer = &mut self.racers[index];
        let transform = racer.node.read().unwrap().get_full_transform();
        let position = transform.column(3).xyz();
        let forward = -transform.column(2).xyz().normalize();
        let right = transform.column(0).xyz().normalize();

        racer.speed = vehicle(scene, racer.id).map_or(0.0, |vehicle| vehicle.telemetry().speed_kmh.abs() / 3.6);
        racer.speed_steps = racer.speed / racer.top_speed.max(0.1) * SPEED_STEPS;
        if racer.is_local() { racer.ai = track.ai_place(position, racer.ai); }

        update_sign(track, racer, position, dt);
        update_zone(track, racer, position);
        let crossing = match track.in_zone(racer.zone, position)
        {
            true => update_progress(track, racer, position, forward, right, dt),
            false => { racer.wrong_way_flag = true; None },
        };

        if running
        {
            if racer.not_crossed
            {
                racer.lap_start = race_time;
            }
            // seconds since the crossing inside this frame
            if let Some((since, counts)) = crossing
            {
                if racer.not_crossed
                {
                    racer.lap_start = race_time - since;
                }
                racer.not_crossed = false;
                if counts && !racer.finished()
                {
                    let lap = race_time - since - racer.lap_start;
                    racer.laps += 1;
                    racer.sign_order = 0;
                    racer.last_lap = Some(lap);
                    racer.best_lap = Some(racer.best_lap.map_or(lap, |best| best.min(lap)));
                    racer.lap_start = race_time - since;
                    if racer.laps >= self.settings.laps && !self.settings.practice
                    {
                        racer.finish_time = Some(race_time - since);
                        console_log!("race: {} finished in {}", racer.name, format_time(race_time - since));
                    }
                    else if racer.is_local()
                    {
                        console_log!("race: lap {} in {}", racer.laps, format_time(lap));
                    }
                }
            }
        }

        // the HUD switches only after the state held for a second
        match racer.wrong_way_flag == racer.wrong_way
        {
            true => racer.wrong_way_timer = 0.0,
            false =>
            {
                racer.wrong_way_timer += dt;
                if racer.wrong_way_timer >= WRONG_WAY_DELAY
                {
                    racer.wrong_way = racer.wrong_way_flag;
                    racer.wrong_way_timer = 0.0;
                }
            },
        }
    }

    // finished cars by their time, the others by their progress; ahead / behind for the HUD
    fn update_positions(&mut self)
    {
        let length = self.track.length;
        let mut order: Vec<usize> = (0..self.racers.len()).collect();
        order.sort_by(|&a, &b|
        {
            let (a, b) = (&self.racers[a], &self.racers[b]);
            match (a.finish_time, b.finish_time)
            {
                (Some(ta), Some(tb)) => ta.total_cmp(&tb),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => b.score(length).total_cmp(&a.score(length)),
            }
        });

        for (place, &index) in order.iter().enumerate()
        {
            self.racers[index].position = place as u32 + 1;
        }
        for (place, &index) in order.iter().enumerate()
        {
            let score = self.racers[index].score(length);
            let neighbour = |other: usize| { let other = &self.racers[other]; Neighbour { metres: (other.score(length) - score).abs() * GAME_METRES, name: other.name.clone() } };
            let ahead = place.checked_sub(1).map(|p| neighbour(order[p]));
            let behind = order.get(place + 1).map(|&o| neighbour(o));
            let racer = &mut self.racers[index];
            racer.ahead = ahead;
            racer.behind = behind;
        }
    }

    // over when every car is through, or a while after the last local player
    fn update_end(&mut self, dt: f32)
    {
        if self.over { return; }
        let locals_done = self.racers.iter().filter(|racer| racer.is_local()).all(Racer::finished);
        if !locals_done { return; }
        self.finish_wait += dt;
        if self.racers.iter().all(Racer::finished) || self.finish_wait > FINISH_WAIT
        {
            self.over = true;
            console_log!("race: over - Enter starts it again");
        }
    }

    // the results are on screen: back to the grid and the countdown
    pub fn restart(&mut self)
    {
        let start = self.track.start;
        for racer in &mut self.racers
        {
            racer.restart(start, &mut self.random, self.settings.difficulty);
        }
        self.countdown = COUNTDOWN;
        self.race_time = 0.0;
        self.finish_wait = 0.0;
        self.over = false;
        console_log!("race: restart");
    }

    // the driving aids of the mode for a local player: steering back onto the track, a steady car, no wall slide in Simulation
    fn assist(&mut self, scene: &mut Scene, index: usize, active: bool, dt: f32)
    {
        let racer = &self.racers[index];
        let mode = racer.mode;
        let id = racer.id;
        if !mode.wall_slide() && !self.wall_slides.iter().any(|(car, _)| *car == id)
        {
            if let Some(before) = with_vehicle(scene, id, |vehicle, _| std::mem::replace(&mut vehicle.chassis.wall_slide, 0.0))
            {
                self.wall_slides.push((id, before));
            }
        }

        let steer = if active && mode == DriveMode::JuniorRc
        {
            let transform = racer.node.read().unwrap().get_full_transform();
            let forward = -transform.column(2).xyz().normalize();
            let right = transform.column(0).xyz().normalize();
            let speed = vehicle(scene, id).map_or(0.0, |vehicle| vehicle.telemetry().speed_kmh / 3.6);
            assist::steer_back(&self.track, racer.ai, transform.column(3).xyz(), forward, right, speed)
        }
        else { None };
        with_vehicle(scene, id, |vehicle, _| vehicle.steer_assist = steer);
        if active { assist::stabilize(scene, id, mode, dt); }

        // Junior RC: lost for a while - back onto the racing line ahead
        let lost = if active && mode == DriveMode::JuniorRc
        {
            let racer = &self.racers[index];
            let up_y = racer.node.read().unwrap().get_full_transform().column(1).xyz().normalize().y;
            let (throttle, brake) = vehicle(scene, id).map_or((0.0, 0.0), |vehicle| { let t = vehicle.telemetry(); (t.throttle, t.brake) });
            let position = racer.node.read().unwrap().get_full_transform().column(3).xyz();
            let stuck = self.racers[index].stuck.update(position, throttle, brake, dt);
            let racer = &self.racers[index];
            assist::lost(&self.track, racer.ai, up_y, racer.wrong_way, stuck)
        }
        else { self.racers[index].stuck.clear(); None };
        let racer = &mut self.racers[index];
        racer.lost_time = if lost.is_some() { racer.lost_time + dt } else { 0.0 };
        let Some((reason, _)) = lost.filter(|(_, after)| racer.lost_time > *after) else { return; };
        racer.lost_time = 0.0;
        racer.stuck.clear();
        let Some(place) = racer.ai else { return; };
        if let BotAction::Reset { position, heading, .. } = bot::reset_action(&self.track, place, 0, reason)
        {
            self.put_on_track(scene, index, position, heading, reason);
        }
    }

    // Play stops - every car drives with its bindings again, the camera back on the player car (the editor saves its controller)
    pub fn stop(&mut self, scene: &mut Scene)
    {
        for (id, wall_slide) in self.wall_slides.drain(..)
        {
            with_vehicle(scene, id, |vehicle, _| vehicle.chassis.wall_slide = wall_slide);
        }
        for racer in &self.racers
        {
            with_vehicle(scene, racer.id, |vehicle, _| vehicle.steer_assist = None);
        }
        if !self.split.is_empty()
        {
            self.end_split(scene);
        }
        // split screen: the bot cameras go, the player cameras are the app's
        for (index, cam_id, _) in std::mem::take(&mut self.multi)
        {
            if !self.cell_cams.contains(&cam_id) { continue; }
            with_vehicle(scene, self.racers[index].id, |vehicle, _| vehicle.camera.enabled = false);
            scene.delete_camera_by_id(cam_id);
        }
        self.cell_cams.clear();
        if self.watching.is_some()
        {
            self.watch(scene, None);
        }
        for racer in &self.racers
        {
            set_input(scene, racer.id, None);
        }
    }
}

// a direction trigger with the next order of the lap shows its sign (sign 11 = none), the timer runs 720/s
fn update_sign(track: &Track, racer: &mut Racer, position: Vector3<f32>, dt: f32)
{
    racer.sign_timer = (racer.sign_timer - SIGN_RATE * dt).max(0.0);
    let hit = track.signs.iter().find(|sign| sign.order == racer.sign_order && sign.zone.contains(position)).map(|sign| sign.sign);
    if let Some(sign) = hit
    {
        if sign != SIGN_NONE
        {
            racer.sign = Some(sign);
            racer.sign_timer = SIGN_TIME;
        }
        racer.sign_order += 1;
    }
    if racer.sign_timer <= 0.0 { racer.sign = None; }
}

// in the zone: stays; else the next one, else the one before - one step per frame, a skipped zone stops the progress
fn update_zone(track: &Track, racer: &mut Racer, position: Vector3<f32>)
{
    let count = track.zone_ids.max(1);
    if track.in_zone(racer.zone, position)
    {
        return;
    }
    let next = (racer.zone + 1) % count;
    let prev = (racer.zone + count - 1) % count;
    if track.in_zone(next, position) { racer.zone = next; }
    else if track.in_zone(prev, position) { racer.zone = prev; }
}

// the .pan step: node, distance to the finish, wrong way, lap fraction, finish line - on a crossing forwards: seconds since it, a lap or not
fn update_progress(track: &Track, racer: &mut Racer, position: Vector3<f32>, forward: Vector3<f32>, right: Vector3<f32>, dt: f32) -> Option<(f64, bool)>
{
    let node = &track.nodes[racer.pan_node];
    let distance = |index: usize| (track.nodes[index].position - position).norm_squared();
    let nearest = node.prev.iter().chain(node.next.iter()).copied()
        .fold((racer.pan_node, distance(racer.pan_node)), |best, index| { let d = distance(index); if d < best.1 { (index, d) } else { best } });
    racer.pan_node = nearest.0;
    let node = &track.nodes[racer.pan_node];

    // against the driving direction
    let back = node.next.iter().map(|&next| (node.position - track.nodes[next].position).normalize()).sum::<Vector3<f32>>();
    let back = if back.norm() > 0.0 { back.normalize() } else { Vector3::zeros() };
    racer.remaining = node.distance + back.dot(&(position - node.position));
    racer.wrong_way_flag = back.dot(&forward) > WRONG_WAY_DOT;

    if !racer.fraction_known
    {
        racer.fraction = (racer.remaining / track.length).rem_euclid(1.0);
        racer.fraction_known = true;
        // a grid just past the line: it counts as crossed, the lap clock runs from the start
        if racer.fraction > FINISH_HIGH
        {
            racer.backwards = false;
            racer.not_crossed = false;
        }
    }
    let old = racer.fraction;
    let mut step = racer.remaining / track.length - old;
    if step > 0.5 { step -= 1.0; } else if step < -0.5 { step += 1.0; }
    let new = (old + step).rem_euclid(1.0);
    racer.fraction = new;

    // the angle between the heading and the track direction, turned at most ARROW_RATE (the exe: a spline through the nodes)
    let direction = track.direction(racer.pan_node, position);
    let target = direction.dot(&right).atan2(direction.dot(&forward));
    let mut turn = target - racer.arrow;
    while turn > PI { turn -= 2.0 * PI; }
    while turn < -PI { turn += 2.0 * PI; }
    let max = ARROW_RATE * dt;
    racer.arrow += turn.clamp(-max, max);
    while racer.arrow > PI { racer.arrow -= 2.0 * PI; }
    while racer.arrow < -PI { racer.arrow += 2.0 * PI; }

    if old < FINISH_LOW && new > FINISH_HIGH
    {
        let since = ((1.0 - new) / (old + 1.0 - new) * dt) as f64;
        let lap = !racer.backwards;
        racer.backwards = false;
        return Some((since, lap));
    }
    if old > FINISH_HIGH && new < FINISH_LOW
    {
        racer.backwards = true;
    }
    None
}

// what a bot needs to know of its car
fn view(scene: &Scene, racer: &Racer) -> Option<BotView>
{
    let vehicle = vehicle(scene, racer.id)?;
    let transform = racer.node.read().unwrap().get_full_transform();
    let along: Vec<f32> = vehicle.wheels.iter().map(|wheel| wheel.center.z).collect();
    let wheelbase = along.iter().copied().fold(f32::MIN, f32::max) - along.iter().copied().fold(f32::MAX, f32::min);
    Some(BotView
    {
        position: transform.column(3).xyz(),
        forward: -transform.column(2).xyz().normalize(),
        right: transform.column(0).xyz().normalize(),
        up: transform.column(1).xyz().normalize(),
        speed: vehicle.telemetry().speed_kmh / 3.6,
        top_speed: racer.top_speed,
        wheelbase: if wheelbase.is_finite() { wheelbase.max(0.1) } else { 0.5 },
        max_steer: vehicle.steering.max_angle.to_radians(),
        catch_up: 1.0,
    })
}

fn vehicle<'a>(scene: &'a Scene, id: u32) -> Option<&'a VehicleController>
{
    scene.controller.iter()
        .filter_map(|controller| controller.as_any().downcast_ref::<VehicleController>())
        .find(|vehicle| vehicle.node.as_ref().is_some_and(|node| node.read().unwrap().id == id))
}

// the controller of a car with the scene beside it (place needs both)
pub fn with_vehicle<R>(scene: &mut Scene, id: u32, f: impl FnOnce(&mut VehicleController, &mut Scene) -> R) -> Option<R>
{
    let mut controllers = std::mem::take(&mut scene.controller);
    let mut result = None;
    for controller in controllers.iter_mut()
    {
        let Some(vehicle) = controller.as_any_mut().downcast_mut::<VehicleController>() else { continue; };
        if vehicle.node.as_ref().is_some_and(|node| node.read().unwrap().id == id)
        {
            result = Some(f(vehicle, scene));
            break;
        }
    }
    // controllers added meanwhile stay behind ours
    controllers.append(&mut scene.controller);
    scene.controller = controllers;
    result
}

// a copy of a camera (its data as json) with a viewport of its own - its id
pub fn add_camera(scene: &mut Scene, data: &serde_json::Value, name: &str, view: [f32; 4]) -> u32
{
    let resolution = resolution(data);
    let mut cam = Camera::new(name.to_string());
    if let Ok(copy) = serde_json::from_value(data.clone()) { cam.get_data_mut().set(copy); }
    cam.init(view[0], view[1], view[2], view[3], resolution[0], resolution[1]);
    let id = cam.id;
    scene.cameras.push(Box::new(cam));
    id
}

// the viewport of a camera (its data as json): x, y, width, height
pub fn viewport(data: &serde_json::Value) -> [f32; 4]
{
    let value = |key: &str, default: f64| data["viewport"][key].as_f64().unwrap_or(default) as f32;
    [value("x", 0.0), value("y", 0.0), value("width", 1.0), value("height", 1.0)]
}

fn resolution(data: &serde_json::Value) -> [u32; 2]
{
    [data["resolution_width"].as_u64().unwrap_or(1920) as u32, data["resolution_height"].as_u64().unwrap_or(1080) as u32]
}

// the cells of split screen: 2 players side by side, 3-4 in 2 x 2, 5-8 in 4 x 2 - x, y from the bottom left, width, height
pub fn split_cells(players: usize) -> Vec<[f32; 4]>
{
    let (columns, rows) = match players { 0 | 1 => (1, 1), 2 => (2, 1), 3 | 4 => (2, 2), _ => (4, 2) };
    let (w, h) = (1.0 / columns as f32, 1.0 / rows as f32);
    (0..columns * rows).map(|cell| { let (column, row) = (cell % columns, cell / columns); [column as f32 * w, 1.0 - (row + 1) as f32 * h, w, h] }).collect()
}

// drives the car instead of the input bindings, None gives them back
fn set_input(scene: &mut Scene, id: u32, drive: Option<VehicleDriveInput>)
{
    for controller in scene.controller.iter_mut()
    {
        let Some(vehicle) = controller.as_any_mut().downcast_mut::<VehicleController>() else { continue; };
        if vehicle.node.as_ref().is_some_and(|node| node.read().unwrap().id == id)
        {
            vehicle.input_override = drive;
        }
    }
}

// mm:ss:mmm like the HUD of the game
pub fn format_time(seconds: f64) -> String
{
    let ms = (seconds.max(0.0) * 1000.0) as u64;
    format!("{:02}:{:02}:{:03}", ms / 60000, ms / 1000 % 60, ms % 1000)
}

fn number(node: &Node, key: &str) -> Option<f32>
{
    node.extras.get::<f64>(key).map(|v| *v as f32).or_else(|| node.extras.get::<i64>(key).map(|v| *v as f32))
}

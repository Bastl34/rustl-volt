// the podium ceremony after a cup in the frontend (formats/README.md, Championship): the place of the player, then the first three cars
// drop onto the podium (3rd, 2nd, 1st, 5 s each) with fireworks after the winner - or, below 3rd, all three at once and the player's car next to them with a putty bomb

use rustl::egui::{self, Color32, TextureHandle, TextureOptions};
use rustl::prelude::*;
use rustl::resources::resources::load_binary;
use rustl::state::state::State;

use crate::field::{self, Opponent, Slot};
use crate::frontend::{Frontend, Target};
use crate::hud::{suffix, Anchor, Painter, WIDTH};
use crate::random::Random;

// the podium places 1st, 2nd, 3rd and the place beside it, game units (the cars are dropped from this height)
const PLACES: [[f32; 3]; 4] = [[-1628.0, -1000.0, 1763.0], [-1801.0, -1000.0, 1588.0], [-1448.0, -1000.0, 1950.0], [-1515.0, -1000.0, 1665.0]];
// the views of the ceremony: the overview, the close ups of 1st, 2nd, 3rd, the place beside the podium
const VIEW_OVERVIEW: usize = 19;
const VIEW_CLOSE_FIRST: usize = 20;
const VIEW_BESIDE: usize = 23;
// s: the place message, one car, the three at once, the end on / below the podium
const INTRO: f32 = 3.0;
const ONE_CAR: f32 = 5.0;
const ALL_CARS: f32 = 7.0;
const OUTRO_PODIUM: f32 = 3.0;
const OUTRO_BELOW: f32 = 5.5;
// fireworks: from a box above the podium (game units), one every 0.1-2.6 s, 1 s up, then the burst
const FIREWORK_MIN: [f32; 3] = [-1884.0, -700.0, 1507.0];
const FIREWORK_SIZE: [f32; 3] = [512.0, 100.0, 512.0];
const FIREWORK_RISE: f32 = 6.0;
const FIREWORK_FLIGHT: f32 = 1.0;
const BURST_TIME: f32 = 0.6;
const BURST_SIZE: f32 = 2.5;
const BURST_COLORS: [[f32; 3]; 4] = [[1.0, 0.3, 0.2], [0.3, 1.0, 0.3], [0.3, 0.5, 1.0], [1.0, 0.9, 0.3]];
// the putty bomb goes off after 8 s
// the cars on the podium hop like in the game: m/s up, a chance per check of a resting car
const HOP_SPEED: f32 = 2.5;
const HOP_CHECK: f32 = 0.25;
const HOP_CHANCE: f32 = 0.3;
const BOMB_TIME: f32 = 8.0;
const BOMB_LIFT: f32 = 6.0;
const YELLOW: Color32 = Color32::from_rgb(255, 255, 0);

#[derive(Clone, Copy, PartialEq, Debug)]
enum Step
{
    Intro,
    Present,
    Outro,
    Done,
}

struct Rocket
{
    node: NodeItem,
    start: Vector3<f32>,
    time: f32,
    burst: Option<NodeItem>,
}

pub struct Ceremony
{
    scene_id: u32,
    assets: String,
    rank: usize,
    // the first three: car id and name, the player's car
    podium: Vec<(String, String)>,
    player_car: String,
    // the message of the place, empty below 3rd
    lines: Vec<String>,
    step: Step,
    time: f32,
    // cars on the podium so far, the length of the current presentation
    presented: usize,
    length: f32,
    view: usize,
    cars: Vec<Opponent>,
    rockets: Vec<Rocket>,
    next_rocket: Option<f32>,
    bomb: Option<(NodeItem, NodeItem, f32)>,
    // seconds since the last hop check
    hop_time: f32,
    random: Random,
    font: Option<TextureHandle>,
}

impl Ceremony
{
    // rank 1 = won; new: the cup was won for the first time (only then the game names what it opened)
    pub fn new(scene_id: u32, assets: &str, cup: usize, rank: usize, new: bool, podium: Vec<(String, String)>, player_car: &str) -> Self
    {
        let mut lines = vec![];
        if rank <= 3 { lines.push(format!("YOU CAME {rank}{}!", suffix(rank as u32).to_uppercase())); }
        if rank == 1 && new
        {
            match cup
            {
                0..=2 => { lines.push(["SILVER CUP", "GOLD CUP", "SPECIAL CUP"][cup].to_string()); lines.push("IS UNLOCKED".to_string()); },
                _ => { lines.push("Well done, you have mastered".to_string()); lines.push("all Re-Volt Championships!".to_string()); },
            }
        }
        console_log!("podium: place {}, {}", rank, podium.iter().map(|(_, name)| name.as_str()).collect::<Vec<_>>().join(", "));
        Self
        {
            scene_id, assets: assets.to_string(), rank, podium, player_car: player_car.to_string(), lines, step: Step::Intro, time: 0.0,
            presented: 0, length: 0.0, view: VIEW_OVERVIEW, cars: vec![], rockets: vec![], next_rocket: None, bomb: None, hop_time: 0.0, random: Random::default(), font: None,
        }
    }

    pub fn done(&self) -> bool
    {
        self.step == Step::Done
    }

    pub fn update(&mut self, state: &mut State, frontend: &mut Frontend, dt: f32)
    {
        self.time += dt;
        match self.step
        {
            Step::Intro if self.time >= INTRO => { self.time = 0.0; self.step = Step::Present; self.present(state); },
            Step::Present =>
            {
                // on the podium: the close up of the car in the middle half of its time, the overview in the last quarter
                if self.rank <= 3
                {
                    if self.time > self.length * 0.75 { self.view = VIEW_OVERVIEW; }
                    else if self.time > self.length * 0.25 { self.view = VIEW_CLOSE_FIRST + 3 - self.presented; }
                }
                if self.time >= self.length
                {
                    self.time = 0.0;
                    let all = if self.rank <= 3 { 3 } else { 4 };
                    if self.presented >= all { self.step = Step::Outro; } else { self.present(state); }
                }
            },
            Step::Outro if self.time >= if self.rank <= 3 { OUTRO_PODIUM } else { OUTRO_BELOW } => self.step = Step::Done,
            _ => {},
        }
        self.fireworks(state, dt);
        self.hop(state, dt);
        self.update_bomb(state, dt);
        let target = Target { view: self.view, presented: None, landing: None, letter: 0, picture: None };
        frontend.update(state, &target, dt);
    }

    // the next car(s): one by one from 3rd on the podium, below the podium all three at once and then the player's car beside it
    fn present(&mut self, state: &mut State)
    {
        if self.rank > 3 && self.presented == 0
        {
            for place in 0..3 { self.drop_car(state, place); }
            self.presented = 3;
            self.length = ALL_CARS;
            return;
        }
        if self.rank > 3
        {
            let car = self.player_car.clone();
            if let Some(node) = self.spawn(state, &car, 3) { self.attach_bomb(state, node); }
            self.presented = 4;
            self.length = ONE_CAR;
            self.view = VIEW_BESIDE;
            return;
        }
        let place = 2 - self.presented.min(2);
        self.drop_car(state, place);
        self.presented += 1;
        self.length = ONE_CAR;
        // the fireworks start with the winner
        if place == 0 { self.next_rocket = Some(0.0); }
    }

    fn drop_car(&mut self, state: &mut State, place: usize)
    {
        let Some((car, _)) = self.podium.get(place).cloned() else { return; };
        self.spawn(state, &car, place);
    }

    // a car dropped at a place, facing the cameras
    fn spawn(&mut self, state: &mut State, car: &str, place: usize) -> Option<NodeItem>
    {
        let slot = Slot { floor: game(PLACES[place]), heading: Vector3::z() };
        match field::spawn_extra(state, self.scene_id, &self.assets, car, &format!("podium_{place}"), slot)
        {
            Ok(opponent) => { let node = opponent.node.clone(); self.cars.push(opponent); Some(node) },
            Err(error) => { console_warning!("podium: {}", error); None },
        }
    }

    fn fireworks(&mut self, state: &mut State, dt: f32)
    {
        if let Some(next) = self.next_rocket.as_mut()
        {
            *next -= dt;
            if *next <= 0.0
            {
                *next = 0.1 + self.random.next() * 2.5;
                let offset: [f32; 3] = std::array::from_fn(|a| FIREWORK_MIN[a] + self.random.next() * FIREWORK_SIZE[a]);
                let start = game(offset);
                if let Ok(node) = field::show_model(state, self.scene_id, &format!("{}/models/firework.glb", self.assets), "firework")
                {
                    place(&node, start, 1.0);
                    self.rockets.push(Rocket { node, start, time: 0.0, burst: None });
                    play(state, &self.assets, "firefire", 0.6);
                }
            }
        }
        let mut finished = vec![];
        for (index, rocket) in self.rockets.iter_mut().enumerate()
        {
            rocket.time += dt;
            if rocket.burst.is_none()
            {
                place(&rocket.node, rocket.start + Vector3::y() * FIREWORK_RISE * rocket.time.min(FIREWORK_FLIGHT), 1.0);
                if rocket.time >= FIREWORK_FLIGHT
                {
                    let position = rocket.start + Vector3::y() * FIREWORK_RISE * FIREWORK_FLIGHT;
                    rocket.node.write().unwrap().settings.visible = false;
                    let color = BURST_COLORS[((self.random.next() * BURST_COLORS.len() as f32) as usize).min(BURST_COLORS.len() - 1)];
                    rocket.burst = burst(state, self.scene_id, &self.assets, position, color);
                    play(state, &self.assets, "firebang", 0.8);
                }
            }
            else if let Some(burst) = &rocket.burst
            {
                let share = ((rocket.time - FIREWORK_FLIGHT) / BURST_TIME).min(1.0);
                place(burst, rocket.start + Vector3::y() * FIREWORK_RISE * FIREWORK_FLIGHT, (share * BURST_SIZE).max(0.01));
                if share >= 1.0 { finished.push(index); }
            }
        }
        for index in finished.into_iter().rev()
        {
            let rocket = self.rockets.remove(index);
            remove(state, self.scene_id, &rocket.node);
            if let Some(burst) = rocket.burst { remove(state, self.scene_id, &burst); }
        }
    }

    fn hop(&mut self, state: &mut State, dt: f32)
    {
        self.hop_time += dt;
        if self.hop_time < HOP_CHECK { return; }
        self.hop_time = 0.0;
        let Some(scene) = state.find_scene_by_id_mut(self.scene_id) else { return; };
        for car in self.cars.iter().take(3)
        {
            if self.random.next() > HOP_CHANCE { continue; }
            let id = car.node.read().unwrap().id;
            let Some((_, body)) = scene.physics.vehicle_mut(id) else { continue; };
            if body.linvel().y.abs() > 0.1 { continue; }
            let mass = body.mass();
            body.apply_impulse(Vector3::new(0.0, HOP_SPEED * mass, 0.0).into(), true);
        }
    }

    // the see through ball around the player's car, it goes off after a while
    fn attach_bomb(&mut self, state: &mut State, car: NodeItem)
    {
        let Ok(ball) = field::show_model(state, self.scene_id, &format!("{}/models/putty_bomb.glb", self.assets), "putty_bomb") else { return; };
        self.bomb = Some((ball, car, 0.0));
        play(state, &self.assets, "fuse", 0.8);
    }

    fn update_bomb(&mut self, state: &mut State, dt: f32)
    {
        let Some((ball, car, time)) = self.bomb.as_mut() else { return; };
        *time += dt;
        let position = car.read().unwrap().get_full_transform().column(3).xyz();
        // it pulses faster the closer it gets
        let pulse = 1.0 + 0.08 * (*time * (4.0 + *time * 2.0)).sin();
        place(ball, position, pulse);
        if *time < BOMB_TIME { return; }
        let (ball, car) = (ball.clone(), car.clone());
        self.bomb = None;
        remove(state, self.scene_id, &ball);
        play(state, &self.assets, "puttbang", 1.0);
        let id = car.read().unwrap().id;
        if let Some(scene) = state.find_scene_by_id_mut(self.scene_id)
        {
            if let Some((_, body)) = scene.physics.vehicle_mut(id)
            {
                let mass = body.mass();
                body.apply_impulse(Vector3::new(0.0, BOMB_LIFT * mass, 0.0).into(), true);
                body.apply_torque_impulse(Vector3::new(mass * 0.6, 0.0, mass * 0.3).into(), true);
            }
        }
        let flash = burst(state, self.scene_id, &self.assets, position, [1.0, 0.6, 0.2]);
        if let Some(flash) = flash
        {
            place(&flash, position, BURST_SIZE);
            self.rockets.push(Rocket { node: flash.clone(), start: position - Vector3::y() * FIREWORK_RISE * FIREWORK_FLIGHT, time: FIREWORK_FLIGHT, burst: Some(flash) });
        }
    }

    // the place in yellow 12 x 24 at y 198 and what it opened below, like the game
    pub fn ui(&mut self, ui: &mut egui::Ui)
    {
        if self.step != Step::Intro || self.lines.is_empty() { return; }
        if self.font.is_none()
        {
            let path = format!("{}/gfx/font.png", self.assets);
            let Ok(image) = load_binary(&path).map_err(|e| e.to_string()).and_then(|bytes| rustl::image::load_from_memory(&bytes).map_err(|e| e.to_string())) else { return; };
            let image = image.to_rgba8();
            let size = [image.width() as usize, image.height() as usize];
            self.font = Some(ui.ctx().load_texture("revolt_podium_font", egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()), TextureOptions::LINEAR));
        }
        let Some(font) = &self.font else { return; };
        let mut painter = Painter::new(ui.ctx().content_rect(), vec![font.id(), egui::TextureId::default()]);
        let width = self.lines.iter().map(|line| line.len()).max().unwrap_or(0) as f32 * 12.0;
        painter.rounded(Anchor::Center, [(WIDTH - width) * 0.5 - 20.0, 186.0, width + 40.0, self.lines.len() as f32 * 28.0 + 20.0], Color32::from_rgba_premultiplied(14, 14, 16, 0xa0), Color32::from_rgb(0xc0, 0x30, 0x28), 2.0, 8.0);
        for (i, line) in self.lines.iter().enumerate()
        {
            painter.text(Anchor::Center, (WIDTH - line.len() as f32 * 12.0) * 0.5, 198.0 + i as f32 * 28.0, 12.0, 24.0, YELLOW, line);
        }
        painter.finish(ui.ctx(), &[1, 0]);
    }

    pub fn stop(&mut self, state: &mut State)
    {
        if let Some(scene) = state.find_scene_by_id_mut(self.scene_id) { field::remove(scene, &self.cars); }
        self.cars.clear();
        for rocket in self.rockets.drain(..)
        {
            remove(state, self.scene_id, &rocket.node);
            if let Some(burst) = rocket.burst { remove(state, self.scene_id, &burst); }
        }
        if let Some((ball, _, _)) = self.bomb.take() { remove(state, self.scene_id, &ball); }
    }
}

fn game([x, y, z]: [f32; 3]) -> Vector3<f32>
{
    Vector3::new(x, -y, -z) * 0.01
}

fn place(node: &NodeItem, position: Vector3<f32>, scale: f32)
{
    let node = node.read().unwrap();
    if let Some(transform) = node.find_component::<Transformation>()
    {
        component_downcast_mut!(transform, Transformation);
        let data = transform.get_data_mut().get_mut();
        data.position = position;
        data.scale = Vector3::new(scale, scale, scale);
        transform.calc_transform();
    }
}

fn burst(state: &mut State, scene_id: u32, assets: &str, position: Vector3<f32>, color: [f32; 3]) -> Option<NodeItem>
{
    let node = field::show_model(state, scene_id, &format!("{assets}/models/burst.glb"), "burst").ok()?;
    place(&node, position, 0.01);
    crate::for_each_node(&vec![node.clone()], &mut |part| crate::change_material(part, |m| m.base_color = Vector3::new(color[0], color[1], color[2])));
    Some(node)
}

fn remove(state: &mut State, scene_id: u32, node: &NodeItem)
{
    let id = node.read().unwrap().id;
    if let Some(scene) = state.find_scene_by_id_mut(scene_id) { scene.delete_node_by_id(id, true, false, true, false); }
}

// a sound of assets/sounds, loaded once
fn play(state: &mut State, assets: &str, name: &str, volume: f32)
{
    let source = match state.resources.sound_sources.values().find(|source| source.read().unwrap().name == name).cloned()
    {
        Some(source) => source,
        None =>
        {
            let Ok(bytes) = load_binary(&format!("{assets}/sounds/{name}.wav")) else { return; };
            state.load_sound_source_byte_or_reuse(&bytes, name, Some("wav".to_string()))
        },
    };
    state.play_one_shot_sound_source(source, volume);
}

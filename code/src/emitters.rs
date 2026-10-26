// the particle sources of the levels and the cars (formats/README.md "Sparks"): spark generators, the smoke of the train, the sprinklers, the dragon fire,
// sparks where a car scrapes over a hard surface, bits and dust behind the wheels on soft ground, the splash of a water box

use std::collections::{HashMap, HashSet};

use rustl::nalgebra::Matrix4;
use rustl::prelude::*;
use rustl::state::scene::physics::contacts::ContactTarget;
use rustl::state::scene::scene_controller::vehicle_controller::VehicleController;

use crate::object_type::ObjectType;
use crate::particles::{Kind, Particles};
use crate::random::Random;
use crate::sounds::cars;

// game units per m
const UNITS: f32 = 100.0;

// spark generator: the speed params are in 10 units/s, one particle per 5 / frequency s
const GENERATOR_SPEED: f32 = 10.0 / UNITS;
const GENERATOR_PERIOD: f32 = 5.0;

// the train: a puff every 0.1 s above its chimney (game units, model space), rising
const TRAIN_SMOKE_EVERY: f32 = 0.1;
const TRAIN_CHIMNEY: [f32; 3] = [0.0, -550.0, -196.0];
const TRAIN_SMOKE_RISE: f32 = 3.5;

// sprinkler: drops per s (the small and the big kind by turns), from in front of the head, thrown up and out (head space, m/s) at 20..100 % of it
const SPRINKLER_RATE: f32 = 90.0;
const SPRINKLER_NOZZLE: [f32; 3] = [0.0, 8.0, -24.0];
const SPRINKLER_THROW: [f32; 3] = [0.0, -1300.0, -2000.0];

// dragon: fire per s while the head morph is at its end (mouth open), from around the mouth (body space, game units) along the jaw (units/s)
const DRAGON_FIRE_RATE: f32 = 100.0;
const DRAGON_MOUTH: [f32; 3] = [-494.2, 208.0, 96.0];
const DRAGON_MOUTH_SPREAD: f32 = 32.0;
const DRAGON_BREATH: [f32; 3] = [-32.4, 144.0, 432.0];
const DRAGON_OPEN: f32 = 0.999;
// own: smoke per s from the back of the flame (s of its way), rising (m/s)
const DRAGON_SMOKE_RATE: f32 = 30.0;
const DRAGON_SMOKE_AT: f32 = 0.3;
const DRAGON_SMOKE_SPREAD: f32 = 0.3;
const DRAGON_SMOKE_RISE: f32 = 0.8;

// the surfaces that spark (flag 1 of the material table): default, marble, stone, glass, ice 1-3, metal, bump metal, pebbles, gravel, paving
const SPARK_SURFACES: [i64; 12] = [0, 1, 2, 9, 10, 11, 13, 14, 15, 21, 22, 26];
// sliding faster than this (m/s) sparks, certainly at the second - at most this many sparks per contact and frame, over all cars per frame
const SCRAPE_SLOW: f32 = 1.0;
const SCRAPE_FAST: f32 = 16.0;
const SCRAPE_SPARKS: f32 = 3.0;
const SCRAPE_MAX: usize = 12;
// the spark flies off at this share of the slide (against it), spread by this share of its speed (at most 2 m/s)
const SCRAPE_BACK: f32 = 0.5;
const SCRAPE_SPREAD: f32 = 0.15;

// dust of the surface (its dust type): bits above DUST_FAST and a cloud above DUST_SLOW (m/s), each with a chance - per s and wheel × DUST_RATE
const DUST_FAST: f32 = 1.8;
const DUST_SLOW: f32 = 0.7;
const DUST_RATE: f32 = 40.0;
const DUST: [(Option<(Kind, f32)>, Option<(Kind, f32)>); 6] =
[
    (None, None),
    (Some((Kind::Gravel, 0.6)), Some((Kind::GravelDust, 0.3))),
    (Some((Kind::Sand, 0.2)), Some((Kind::SandDust, 0.1))),
    (Some((Kind::Grass, 0.8)), Some((Kind::GrassDust, 0.4))),
    (Some((Kind::Dirt, 0.4)), Some((Kind::SoilDust, 0.2))),
    (None, Some((Kind::RoadDust, 0.2))),
];
// the bits: thrown back (share of the car speed) and up, spread; the cloud: drifting up and a little back
const BITS_BACK: f32 = 0.25;
const BITS_UP: f32 = 1.2;
const BITS_SPREAD: f32 = 1.0;
const CLOUD_BACK: f32 = 0.05;
const CLOUD_UP: f32 = 0.2;
const CLOUD_SPREAD: f32 = 0.3;

// m a car moves in a frame at most - more is a reset
const MAX_STEP: f32 = 3.0;

// a car going into a water box: drops up and out
const SPLASH_DROPS: usize = 34;
const SPLASH_UP: f32 = 2.5;
const SPLASH_SPREAD: f32 = 2.5;

// the dust type of a surface of the material table
fn dust_type(surface: i64) -> usize
{
    match surface
    {
        14 | 15 => 1,
        4 => 2,
        12 => 3,
        18..=20 => 4,
        26 => 5,
        _ => 0,
    }
}

struct Generator
{
    node: NodeItem,
    kind: Kind,
    speed: f32,
    spread: f32,
    rate: f32,
}

// an object whose particles come from a part of its model (found once the model is there)
struct Part
{
    object: NodeItem,
    name: &'static str,
    part: Option<NodeItem>,
    due: f32,
}

struct Dragon
{
    body: Part,
    // the node with the morph weight of the head
    head: Option<NodeItem>,
}

struct WaterBox
{
    min: Vector3<f32>,
    max: Vector3<f32>,
}

struct SceneEmitters
{
    scene_id: u32,
    generators: Vec<Generator>,
    trains: Vec<Part>,
    sprinklers: Vec<Part>,
    dragons: Vec<Dragon>,
    water: Vec<WaterBox>,
}

#[derive(Default)]
pub struct Emitters
{
    scenes: Vec<SceneEmitters>,
    // per car: where it was last frame, whether it is in water
    last: HashMap<u32, Vector3<f32>>,
    wet: HashSet<u32>,
    big_drop: bool,
    random: Random,
}

impl Emitters
{
    pub fn add_scene(&mut self, scene: &Scene)
    {
        if self.scenes.iter().any(|emitters| emitters.scene_id == scene.id) { return; }
        let mut emitters = SceneEmitters { scene_id: scene.id, generators: vec![], trains: vec![], sprinklers: vec![], dragons: vec![], water: vec![] };
        crate::for_each_node(&scene.nodes, &mut |node|
        {
            let read = node.read().unwrap();
            if !read.tags.contains("revolt_object") { return; }
            let param = |key: &str| read.extras.get::<i64>(key).copied().unwrap_or(0);
            match read.extras.get::<i64>("revolt_object_type").and_then(|id| ObjectType::from_id(*id))
            {
                Some(ObjectType::SparkGenerator) =>
                {
                    let Some(kind) = Kind::from_id(param("revolt_param_type")) else { return; };
                    let frequency = param("revolt_param_frequency") as f32;
                    if frequency <= 0.0 { return; }
                    emitters.generators.push(Generator { node: node.clone(), kind, speed: param("revolt_param_av_speed") as f32 * GENERATOR_SPEED, spread: param("revolt_param_var_speed") as f32 * GENERATOR_SPEED, rate: frequency / GENERATOR_PERIOD });
                },
                Some(ObjectType::Train) => emitters.trains.push(Part { object: node.clone(), name: "train", part: None, due: 0.0 }),
                Some(ObjectType::Sprinkler) => emitters.sprinklers.push(Part { object: node.clone(), name: "head", part: None, due: 0.0 }),
                Some(ObjectType::Dragon) => emitters.dragons.push(Dragon { body: Part { object: node.clone(), name: "body", part: None, due: 0.0 }, head: None }),
                Some(ObjectType::WaterBox) =>
                {
                    let half = Vector3::new(param("revolt_param_x_range") as f32, param("revolt_param_y_range") as f32, param("revolt_param_z_range") as f32) / UNITS;
                    let center = read.get_full_transform().column(3).xyz();
                    emitters.water.push(WaterBox { min: center - half, max: center + half });
                },
                _ => {},
            }
        });
        console_log!("emitters: scene '{}': {} spark generators, {} trains, {} sprinklers, {} dragons, {} water boxes", scene.name, emitters.generators.len(), emitters.trains.len(), emitters.sprinklers.len(), emitters.dragons.len(), emitters.water.len());
        self.scenes.push(emitters);
    }

    pub fn remove_scene(&mut self, scene_id: u32)
    {
        self.scenes.retain(|emitters| emitters.scene_id != scene_id);
    }

    pub fn stop(&mut self)
    {
        self.scenes.clear();
        self.last.clear();
        self.wet.clear();
    }

    // the cars of every active scene, the objects of the scenes with emitters
    pub fn update(&mut self, state: &State, particles: &mut Particles, dt: f32)
    {
        if dt <= 0.0 { return; }
        let mut last = HashMap::new();
        let mut wet = HashSet::new();
        let mut scrapes = 0;
        for scene in state.scenes.iter().filter(|scene| scene.active)
        {
            let emitters = self.scenes.iter_mut().find(|emitters| emitters.scene_id == scene.id);
            if let Some(emitters) = emitters
            {
                emitters.update(scene, particles, &mut self.random, &mut self.big_drop, dt);
                for car in cars(scene)
                {
                    let (id, position) = { let car = car.read().unwrap(); (car.id, car.get_full_transform().column(3).xyz()) };
                    if !emitters.water.iter().any(|water| (0..3).all(|a| position[a] >= water.min[a] && position[a] <= water.max[a])) { continue; }
                    wet.insert(id);
                    if !self.wet.contains(&id) { particles.burst(scene.id, Kind::Water, SPLASH_DROPS, position, Vector3::y() * SPLASH_UP, SPLASH_SPREAD); }
                }
            }
            for car in cars(scene)
            {
                let (id, position) = { let car = car.read().unwrap(); (car.id, car.get_full_transform().column(3).xyz()) };
                // a jump (reset, teleport) is no speed
                let velocity = self.last.get(&id).map(|before| position - before).filter(|moved| moved.norm() < MAX_STEP).map_or(Vector3::zeros(), |moved| moved / dt);
                last.insert(id, position);
                dust(scene, &car, velocity, particles, &mut self.random, dt);
            }
            scrapes += self.scrape(scene, particles, SCRAPE_MAX.saturating_sub(scrapes));
        }
        self.last = last;
        self.wet = wet;
    }

    // a car sliding along a hard surface of the level
    fn scrape(&mut self, scene: &Scene, particles: &mut Particles, budget: usize) -> usize
    {
        let mut made = 0;
        for event in scene.physics.contact_events().iter().filter(|event| event.started() || event.touching())
        {
            if made >= budget { break; }
            // the car as target, the level as other
            let event = match (event.target, event.other)
            {
                (ContactTarget::Vehicle { .. }, ContactTarget::Object { .. }) => *event,
                (ContactTarget::Object { .. }, ContactTarget::Vehicle { .. }) => event.flipped(),
                _ => continue,
            };
            let (ContactTarget::Vehicle { node_id: car }, ContactTarget::Object { node_id: other, .. }) = (event.target, event.other) else { continue; };
            if !scene.find_node_by_id(car).is_some_and(|node| node.read().unwrap().tags.contains("revolt_car")) { continue; }
            let Some(other) = scene.find_node_by_id(other) else { continue; };
            let surface = { let read = other.read().unwrap(); if read.tags.contains("revolt_physics") { continue; } read.extras.get::<i64>("revolt_surface").copied().unwrap_or(0) };
            if !SPARK_SURFACES.contains(&surface) { continue; }

            // the slide: the other against the car along the surface
            let slide = event.relative_velocity - event.normal * event.relative_velocity.dot(&event.normal);
            let speed = slide.norm();
            if speed <= SCRAPE_SLOW { continue; }
            let chance = ((speed - SCRAPE_SLOW) / (SCRAPE_FAST - SCRAPE_SLOW)).min(1.0);
            let count = (chance * SCRAPE_SPARKS + self.random.next()) as usize;
            for _ in 0..count.min(budget - made)
            {
                particles.emit(scene.id, Kind::Spark, event.point, slide * SCRAPE_BACK, (speed * SCRAPE_SPREAD).min(2.0));
                made += 1;
            }
        }
        made
    }
}

impl SceneEmitters
{
    fn update(&mut self, scene: &Scene, particles: &mut Particles, random: &mut Random, big_drop: &mut bool, dt: f32)
    {
        for generator in &self.generators
        {
            let count = particles.due(generator.rate, dt);
            if count == 0 { continue; }
            let transform = generator.node.read().unwrap().get_full_transform();
            let look = (-transform.column(2).xyz()).try_normalize(1e-6).unwrap_or_else(Vector3::y);
            particles.burst(scene.id, generator.kind, count, transform.column(3).xyz(), look * generator.speed, generator.spread);
        }

        for train in &mut self.trains
        {
            let Some(transform) = train.transform() else { continue; };
            train.due += dt;
            while train.due >= TRAIN_SMOKE_EVERY
            {
                train.due -= TRAIN_SMOKE_EVERY;
                let chimney = transform.transform_point(&Point3::from(gltf(TRAIN_CHIMNEY))).coords;
                particles.emit(scene.id, Kind::Smoke2, chimney, Vector3::y() * TRAIN_SMOKE_RISE, 0.0);
            }
        }

        for sprinkler in &mut self.sprinklers
        {
            let Some(transform) = sprinkler.transform() else { continue; };
            let count = particles.due(SPRINKLER_RATE, dt);
            let turn = |v: Vector3<f32>| transform.transform_vector(&v).try_normalize(1e-6).unwrap_or_else(Vector3::zeros) * v.norm();
            let (nozzle, throw) = (turn(gltf(SPRINKLER_NOZZLE)), turn(gltf(SPRINKLER_THROW)));
            let head = transform.column(3).xyz();
            for _ in 0..count
            {
                // the pressure of the drop
                let share = 0.2 + 0.8 * random.next();
                let kind = if *big_drop { Kind::SprinklerBig } else { Kind::Sprinkler };
                *big_drop = !*big_drop;
                particles.emit(scene.id, kind, head + nozzle * share, throw * share, 0.0);
            }
        }

        for dragon in &mut self.dragons
        {
            if !dragon.open() { continue; }
            let Some(transform) = dragon.body.transform() else { continue; };
            let mouth = transform.transform_point(&Point3::from(gltf(DRAGON_MOUTH))).coords;
            let breath = transform.transform_vector(&gltf(DRAGON_BREATH));
            let count = particles.due(DRAGON_FIRE_RATE, dt);
            for _ in 0..count
            {
                particles.emit(scene.id, Kind::DragonFire, mouth + around(random, DRAGON_MOUTH_SPREAD / UNITS), breath, 0.0);
            }
            let count = particles.due(DRAGON_SMOKE_RATE, dt);
            for _ in 0..count
            {
                particles.emit(scene.id, Kind::DragonSmoke, mouth + breath * DRAGON_SMOKE_AT + around(random, DRAGON_SMOKE_SPREAD), breath * 0.5 + Vector3::y() * DRAGON_SMOKE_RISE, 0.0);
            }
        }
    }
}

impl Dragon
{
    // the head morph at its end - None until the model is there
    fn open(&mut self) -> bool
    {
        if self.head.is_none()
        {
            let Some(head) = find_child(&self.body.object, "head") else { return false; };
            crate::for_each_node(&vec![head], &mut |node| if self.head.is_none() && node.read().unwrap().has_morph_target_weights() { self.head = Some(node.clone()); });
        }
        self.head.as_ref().and_then(|head| head.read().unwrap().get_morph_target_weights_vec()).and_then(|weights| weights.first().copied()).is_some_and(|weight| weight >= DRAGON_OPEN)
    }
}

impl Part
{
    // the world transform of the part of the model, None until it is loaded
    fn transform(&mut self) -> Option<Matrix4<f32>>
    {
        if self.part.is_none()
        {
            self.part = find_child(&self.object, self.name);
        }
        self.part.as_ref().map(|part| part.read().unwrap().get_full_transform())
    }
}

// bits and dust behind the wheels on the ground of a soft (dusty) surface
fn dust(scene: &Scene, car: &NodeItem, velocity: Vector3<f32>, particles: &mut Particles, random: &mut Random, dt: f32)
{
    let speed = velocity.norm();
    if speed <= DUST_SLOW { return; }
    let id = car.read().unwrap().id;
    let Some(vehicle) = scene.controller.iter().filter_map(|controller| controller.as_any().downcast_ref::<VehicleController>()).find(|vehicle| vehicle.node.as_ref().is_some_and(|node| node.read().unwrap().id == id)) else { return; };
    let transform = car.read().unwrap().get_full_transform();
    let up = transform.column(1).xyz().try_normalize(1e-6).unwrap_or_else(Vector3::y);
    let grounds = scene.physics.vehicle_wheel_grounds(id);
    for (wheel, ground) in vehicle.wheels.iter().zip(grounds)
    {
        let Some(ground) = ground else { continue; };
        let Some(surface) = scene.find_node_by_id(ground).and_then(|node| node.read().unwrap().extras.get::<i64>("revolt_surface").copied()) else { continue; };
        let (bits, cloud) = DUST[dust_type(surface)];
        let contact = (transform * wheel.center.push(1.0)).xyz() - up * wheel.radius;
        if let Some((kind, chance)) = bits.filter(|_| speed > DUST_FAST)
        {
            let count = particles.due(chance * DUST_RATE, dt);
            particles.burst(scene.id, kind, count, contact, -velocity * BITS_BACK + up * BITS_UP, BITS_SPREAD * (0.5 + random.next()));
        }
        if let Some((kind, chance)) = cloud
        {
            let count = particles.due(chance * DUST_RATE, dt);
            particles.burst(scene.id, kind, count, contact, -velocity * CLOUD_BACK + up * CLOUD_UP, CLOUD_SPREAD);
        }
    }
}

// a random point in a ball of this radius
fn around(random: &mut Random, radius: f32) -> Vector3<f32>
{
    let direction = Vector3::new(random.next() * 2.0 - 1.0, random.next() * 2.0 - 1.0, random.next() * 2.0 - 1.0).try_normalize(1e-6).unwrap_or_else(Vector3::y);
    direction * random.next() * radius
}

// a point or direction of the game (y down, z away) in glTF axes and m
fn gltf(v: [f32; 3]) -> Vector3<f32>
{
    Vector3::new(v[0], -v[1], -v[2]) / UNITS
}

fn find_child(node: &NodeItem, name: &str) -> Option<NodeItem>
{
    for child in node.read().unwrap().nodes.clone()
    {
        if child.read().unwrap().name == name { return Some(child); }
        if let Some(found) = find_child(&child, name) { return Some(found); }
    }
    None
}

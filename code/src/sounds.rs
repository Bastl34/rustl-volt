// sounds of the game beside engines, hits and the menu (formats/README.md, Sounds and music): the hits of the physics objects, the splash of a water box,
// the skid on rough surfaces and the servo of the own car - sounds the scene sound list has not got come from assets/sounds

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

use rustl::prelude::*;
use rustl::resources::resources::load_binary;
use rustl::state::resources::sound_source::SoundSourceItem;
use rustl::state::scene::components::component::ComponentItem;
use rustl::state::scene::components::sound::{Sound, SoundType};
use rustl::state::scene::physics::contacts::ContactTarget;
use rustl::state::scene::scene_controller::vehicle_controller::VehicleController;
use rustl::state::state::State;

use crate::for_each_node;
use crate::object_type::ObjectType;

// game units per m
const UNITS: f32 = 100.0;
// the hit of a physics object (per object type, like the game): sound, volume offset, threshold, pitch base, pitch divisor, pitch max - units/s
// played above the threshold, volume (v + offset) / 5 of 127, pitch 1 + (min((v - threshold) / divisor, max) + base) semitones
// the sounds of the level block are the ones of the levels the objects are in (basketball nhood, cone nhood, carton market, block toy, tumbleweed ghost)
const OBJECT_HITS: [(ObjectType, &str, f32, f32, f32, f32, f32); 7] =
[
    (ObjectType::Beachball, "beachball", 0.0, 300.0, -1.0, 2000.0, 2.0),
    (ObjectType::BasketBall, "hood_basketball", 0.0, 300.0, 0.0, 3000.0, 1.0),
    (ObjectType::Bottle, "bottle", 200.0, 100.0, 0.0, 3000.0, 1.0),
    (ObjectType::Cone, "hood_roadcone", -150.0, 300.0, 0.0, 20000.0, 1.0),
    (ObjectType::AbcBlock, "toy_toybrick", 200.0, 100.0, -1.0, 500.0, 2.0),
    (ObjectType::Packet, "market_carton", 200.0, 100.0, 0.0, 2000.0, 1.0),
    (ObjectType::Tumbleweed, "ghost_tumbweed", 200.0, 100.0, 0.0, 10000.0, 0.0),
];
// 2^(1/12) - 1: one semitone
const SEMITONE: f32 = 0.059_463;
// full volume up to this many m (the range 1.0 of the cars: 6 m)
const SOUND_DISTANCE: f32 = 6.0;
// frames between two hit sounds of one object
const HIT_COOLDOWN: u64 = 6;

// the surfaces the game plays skid_rough on (its skid table): sand, carpet tile / shag, ice 1-3, grass, pebbles, gravel, dirt 1-3
const ROUGH_SURFACES: [i64; 12] = [4, 6, 7, 10, 12, 14, 15, 18, 19, 20, 21, 22];
const SQUEAL: &str = "Squeal";
const SKID_NORMAL: &str = "skid_normal";
const SKID_ROUGH: &str = "skid_rough";

// the servo of the own car: steering moving more than this a frame (at 60 fps) turns it up by SERVO_RATE per s, else down
const SERVO_STEP: f32 = 0.01;
const SERVO_RATE: f32 = 8.0;
// Hz at full level above 22050, and lower while steering back
const SERVO_PITCH: f32 = 10000.0;
const SERVO_BACK: f32 = 5000.0;
const SERVO_NAME: &str = "Servo";
const SERVO_VOLUME: f32 = 0.6;

struct HitObject
{
    sound: SoundSourceItem,
    volume: f32,
    threshold: f32,
    pitch: f32,
    divisor: f32,
    max: f32,
    last: u64,
}

// a water box (.fob 68): position ± the ranges - a car going in splashes
struct WaterBox
{
    scene_id: u32,
    min: Vector3<f32>,
    max: Vector3<f32>,
}

struct ServoState
{
    car: NodeItem,
    component: ComponentItem,
    steer: f32,
    level: f32,
}

#[derive(Default)]
pub struct Sounds
{
    // every node of a physics object -> its hit
    hits: HashMap<u32, usize>,
    objects: Vec<HitObject>,
    // nodes whose report_contacts this code turned on - off again at the end
    reporting: Vec<NodeItem>,
    scenes: HashSet<u32>,
    water: Vec<WaterBox>,
    splash: Option<SoundSourceItem>,
    // cars in a water box now
    wet: HashSet<u32>,
    // the cars on rough ground now (squeal on skid_rough)
    rough: HashSet<u32>,
    skid: Option<(SoundSourceItem, SoundSourceItem)>,
    servo: Option<ServoState>,
    frames: u64,
}

// a sound of the scene sound list, else the wav of assets/sounds
pub fn source(state: &mut State, assets: &str, name: &str) -> Option<SoundSourceItem>
{
    if let Some(source) = state.resources.sound_sources.values().find(|source| source.read().unwrap().name == name).cloned()
    {
        return Some(source);
    }
    match load_binary(&format!("{assets}/sounds/{name}.wav"))
    {
        Ok(bytes) => Some(state.load_sound_source_byte_or_reuse(&bytes, name, Some("wav".to_string()))),
        Err(error) => { console_warning!("sounds: {} - {:?}", name, error); None },
    }
}

impl Sounds
{
    // the physics objects with a hit sound report their contacts, the water boxes of the scene
    pub fn add_scene(&mut self, state: &mut State, scene_id: u32, assets: &str)
    {
        if !self.scenes.insert(scene_id) { return; }
        let Some(scene) = state.find_scene_by_id(scene_id) else { return; };
        let mut found: Vec<(NodeItem, usize)> = vec![];
        let mut boxes = vec![];
        for_each_node(&scene.nodes, &mut |node|
        {
            let read = node.read().unwrap();
            if !read.tags.contains("revolt_object") { return; }
            let Some(kind) = read.extras.get::<i64>("revolt_object_type").and_then(|id| ObjectType::from_id(*id)) else { return; };
            if kind == ObjectType::WaterBox
            {
                let range = |key: &str| read.extras.get::<i64>(key).map_or(0.0, |v| *v as f32) / UNITS;
                let half = Vector3::new(range("revolt_param_x_range"), range("revolt_param_y_range"), range("revolt_param_z_range"));
                let center = read.get_full_transform().column(3).xyz();
                boxes.push(WaterBox { scene_id, min: center - half, max: center + half });
                return;
            }
            if let Some(index) = OBJECT_HITS.iter().position(|(object, ..)| *object == kind).filter(|_| read.tags.contains("revolt_physics"))
            {
                found.push((node.clone(), index));
            }
        });

        let mut sources: HashMap<usize, SoundSourceItem> = HashMap::new();
        for index in found.iter().map(|(_, index)| *index).collect::<HashSet<usize>>()
        {
            if let Some(sound) = source(state, assets, OBJECT_HITS[index].1) { sources.insert(index, sound); }
        }
        if !boxes.is_empty() && self.splash.is_none() { self.splash = source(state, assets, "splash"); }
        if self.skid.is_none() { self.skid = source(state, assets, SKID_NORMAL).zip(source(state, assets, SKID_ROUGH)); }

        let Some(scene) = state.find_scene_by_id_mut(scene_id) else { return; };
        let count = found.len();
        for (node, index) in found
        {
            let Some(sound) = sources.get(&index).cloned() else { continue; };
            self.register(scene, &node, index, sound);
        }
        console_log!("sounds: {} physics objects with hit sounds, {} water boxes", count, boxes.len());
        self.water.extend(boxes);
    }

    // a physics object made later (thrown by an object thrower): its hit sound like the ones of the level
    pub fn add_object(&mut self, state: &mut State, scene_id: u32, assets: &str, node: &NodeItem)
    {
        let Some(kind) = node.read().unwrap().extras.get::<i64>("revolt_object_type").and_then(|id| ObjectType::from_id(*id)) else { return; };
        let Some(index) = OBJECT_HITS.iter().position(|(object, ..)| *object == kind) else { return; };
        let Some(sound) = source(state, assets, OBJECT_HITS[index].1) else { return; };
        let Some(scene) = state.find_scene_by_id_mut(scene_id) else { return; };
        self.register(scene, node, index, sound);
    }

    fn register(&mut self, scene: &mut Scene, node: &NodeItem, index: usize, sound: SoundSourceItem)
    {
        let (_, _, volume, threshold, pitch, divisor, max) = OBJECT_HITS[index];
        let object = self.objects.len();
        self.objects.push(HitObject { sound, volume, threshold, pitch, divisor, max, last: 0 });
        for_each_node(&vec![node.clone()], &mut |part|
        {
            let id = part.read().unwrap().id;
            self.hits.insert(id, object);
            scene.physics.set_report_contacts(id, true);
        });
        // the next build of the physics keeps it (the flag counts for everything below)
        if !node.read().unwrap().settings.physics.report_contacts
        {
            node.write().unwrap().settings.physics.report_contacts = true;
            self.reporting.push(node.clone());
        }
    }

    pub fn remove_scene(&mut self, scene_id: u32)
    {
        self.scenes.remove(&scene_id);
        self.water.retain(|water| water.scene_id != scene_id);
    }

    // own_car: the car of the single local player (the servo)
    pub fn update(&mut self, state: &mut State, assets: &str, own_car: Option<&NodeItem>, dt: f32)
    {
        self.frames += 1;
        if state.paused() { return; }
        self.play_hits(state);
        self.splashes(state);
        self.skids(state);
        self.update_servo(state, assets, own_car, dt);
    }

    // a started contact of a physics object fast enough: its sound at the contact, louder and higher the harder
    fn play_hits(&mut self, state: &mut State)
    {
        let mut plays = vec![];
        for scene in state.scenes.iter().filter(|scene| scene.active && self.scenes.contains(&scene.id))
        {
            for contact in scene.physics.contact_events().iter().filter(|contact| contact.started())
            {
                for target in [contact.target, contact.other]
                {
                    let ContactTarget::Object { node_id, .. } = target else { continue; };
                    let Some(&index) = self.hits.get(&node_id) else { continue; };
                    let object = &mut self.objects[index];
                    let speed = contact.impact_speed * UNITS;
                    if speed <= object.threshold || self.frames - object.last < HIT_COOLDOWN { continue; }
                    object.last = self.frames;
                    let volume = ((speed + object.volume) / 5.0).clamp(0.0, 127.0) / 127.0;
                    let pitch = 1.0 + (((speed - object.threshold) / object.divisor).min(object.max) + object.pitch) * SEMITONE;
                    plays.push((object.sound.clone(), contact.point, volume, pitch));
                }
            }
        }
        for (sound, point, volume, pitch) in plays
        {
            state.play_one_shot_sound_source_at(sound, Point3::from(point), volume, pitch, SOUND_DISTANCE);
        }
    }

    // a car going into a water box splashes once
    fn splashes(&mut self, state: &mut State)
    {
        let Some(splash) = self.splash.clone() else { return; };
        let mut plays = vec![];
        let mut wet = HashSet::new();
        for water in &self.water
        {
            let Some(scene) = state.find_scene_by_id(water.scene_id).filter(|scene| scene.active) else { continue; };
            for car in cars(scene)
            {
                let (id, position) = { let car = car.read().unwrap(); (car.id, car.get_full_transform().column(3).xyz()) };
                if (0..3).all(|a| position[a] >= water.min[a] && position[a] <= water.max[a])
                {
                    wet.insert(id);
                    if !self.wet.contains(&id) { plays.push(position); }
                }
            }
        }
        self.wet = wet;
        for position in plays
        {
            state.play_one_shot_sound_source_at(splash.clone(), Point3::from(position), 1.0, 1.0, SOUND_DISTANCE);
        }
    }

    // the skid of a car takes the sound of the surface most of its wheels are on (the game: of its skidding wheels)
    fn skids(&mut self, state: &mut State)
    {
        let Some((normal, rough)) = self.skid.clone() else { return; };
        let mut now = HashSet::new();
        for scene in state.scenes.iter().filter(|scene| scene.active)
        {
            for car in cars(scene)
            {
                let id = car.read().unwrap().id;
                let surfaces: Vec<i64> = scene.physics.vehicle_wheel_grounds(id).into_iter().flatten()
                    .filter_map(|ground| scene.find_node_by_id(ground).and_then(|node| node.read().unwrap().extras.get::<i64>("revolt_surface").copied())).collect();
                let rough_wheels = surfaces.iter().filter(|surface| ROUGH_SURFACES.contains(surface)).count();
                let is_rough = if surfaces.is_empty() { self.rough.contains(&id) } else { rough_wheels * 2 > surfaces.len() };
                if is_rough { now.insert(id); }
                if is_rough == self.rough.contains(&id) { continue; }
                let wanted = if is_rough { &rough } else { &normal };
                for component in car.read().unwrap().find_components::<Sound>()
                {
                    let mut item = component.write().unwrap();
                    if item.get_base().name != SQUEAL { continue; }
                    let Some(sound) = item.as_any_mut().downcast_mut::<Sound>() else { continue; };
                    let running = sound.running();
                    sound.set_sound_source(wanted.clone());
                    if running { sound.start(); }
                }
            }
        }
        self.rough = now;
    }

    // the servo whirrs while the steering of the own car moves - higher at full level, lower while it steers back
    fn update_servo(&mut self, state: &mut State, assets: &str, own_car: Option<&NodeItem>, dt: f32)
    {
        let same = match (&self.servo, own_car) { (Some(servo), Some(car)) => Arc::ptr_eq(&servo.car, car), (None, None) => true, _ => false };
        if !same
        {
            self.stop_servo();
            if let Some(car) = own_car
            {
                if let Some(source) = source(state, assets, "servo")
                {
                    let mut sound = Sound::new(SERVO_NAME, source, SoundType::Stereo, true);
                    sound.get_data_mut().get_mut().volume = 0.0;
                    let component: ComponentItem = Arc::new(RwLock::new(Box::new(sound)));
                    car.write().unwrap().add_component(component.clone());
                    self.servo = Some(ServoState { car: car.clone(), component, steer: 0.0, level: 0.0 });
                }
            }
        }
        let Some(servo) = self.servo.as_mut() else { return; };
        let id = servo.car.read().unwrap().id;
        let steer = state.scenes.iter().filter(|scene| scene.active).find_map(|scene| vehicle(scene, id).map(|vehicle| vehicle.telemetry().steer)).unwrap_or(servo.steer);
        let change = steer - servo.steer;
        servo.steer = steer;
        // per frame of 60 fps like the game
        let moving = change.abs() > SERVO_STEP * dt * 60.0;
        servo.level = if moving { (servo.level + SERVO_RATE * dt).min(1.0) } else { (servo.level - SERVO_RATE * dt).max(0.0) };

        let mut item = servo.component.write().unwrap();
        let Some(sound) = item.as_any_mut().downcast_mut::<Sound>() else { return; };
        {
            let data = sound.get_data_mut().get_mut();
            data.volume = servo.level * SERVO_VOLUME;
            data.speed = (22050.0 + SERVO_PITCH * servo.level - if change < 0.0 { SERVO_BACK } else { 0.0 }) / 22050.0;
        }
        match (servo.level > 0.0, sound.running())
        {
            (true, false) => sound.start(),
            (false, true) => sound.pause(),
            _ => {},
        }
    }

    fn stop_servo(&mut self)
    {
        let Some(servo) = self.servo.take() else { return; };
        let id = servo.component.read().unwrap().id();
        if let Some(sound) = servo.component.write().unwrap().as_any_mut().downcast_mut::<Sound>() { sound.stop(); }
        servo.car.write().unwrap().remove_component_by_id(id);
    }

    // Play stops: the servo goes, the objects report no contacts again
    pub fn stop(&mut self, state: &mut State)
    {
        self.stop_servo();
        for node in self.reporting.drain(..)
        {
            node.write().unwrap().settings.physics.report_contacts = false;
        }
        for scene in state.scenes.iter_mut()
        {
            for id in self.hits.keys()
            {
                scene.physics.set_report_contacts(*id, false);
            }
        }
        *self = Self::default();
    }
}

// the Re-Volt cars driving in a scene
pub fn cars(scene: &Scene) -> Vec<NodeItem>
{
    scene.controller.iter()
        .filter_map(|controller| controller.as_any().downcast_ref::<VehicleController>())
        .filter_map(|vehicle| vehicle.node.as_ref().cloned())
        .filter(|node| node.read().unwrap().tags.contains("revolt_car"))
        .collect()
}

fn vehicle<'a>(scene: &'a Scene, id: u32) -> Option<&'a VehicleController>
{
    scene.controller.iter()
        .filter_map(|controller| controller.as_any().downcast_ref::<VehicleController>())
        .find(|vehicle| vehicle.node.as_ref().is_some_and(|node| node.read().unwrap().id == id))
}

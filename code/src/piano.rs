// the piano of Toy World 1 (toylite, .tri trigger type 0, formats/README.md "Piano") - behaviour of the game, own code
// every wheel on the keyboard plays the key under it once (again after it left it and the light is out), the key lights up in its color and fades
// the converter writes the keyboard (revolt_piano, glTF m) and a glb with one light per key (white_NN, black_NN) onto the piano object

use std::sync::{Arc, RwLock};

use rustl::prelude::*;
use rustl::serde_json::{self, Value};
use rustl::state::scene::components::component::ComponentItem;
use rustl::state::scene::components::sound::{Sound, SoundType};
use rustl::state::scene::scene_controller::vehicle_controller::VehicleController;

// a hit key lights for 16 and goes down by 72 per s, its light alpha is 12 per step (0.22 s, at most 192 / 255)
const LIGHT: f32 = 16.0;
const LIGHT_FADE: f32 = 72.0;
const LIGHT_ALPHA: f32 = 12.0 / 255.0;
// the note of semitone n: 12000 + 800 n Hz, the sample has 22050
const NOTE_BASE: f32 = 12000.0;
const NOTE_STEP: f32 = 800.0;
const SAMPLE_RATE: f32 = 22050.0;
// a black key: within a quarter key of a key boundary
const BLACK_HALF: f32 = 0.25;
// the notes: full volume up to this many m
const SOUND_DISTANCE: f32 = 6.0;
// a wheel counts on the keys only this close to their height (m)
const HEIGHT_RANGE: f32 = 0.5;

struct Key
{
    node: Option<NodeItem>,
    sound: Option<ComponentItem>,
    timer: f32,
}

struct Trigger
{
    center: Vector3<f32>,
    axes: [Vector3<f32>; 3],
    half_size: Vector3<f32>,
}

pub struct Piano
{
    scene_id: u32,
    // the front left corner of the keyboard, x to the right, the keys go to -z
    x0: f32,
    z0: f32,
    y: f32,
    key_width: f32,
    depth: f32,
    black_from: f32,
    // the keys run to -x (mirrored level)
    mirrored: bool,
    // the key boundaries with a black key (1..31), in order
    black_ids: Vec<i64>,
    white: Vec<Key>,
    black: Vec<Key>,
    triggers: Vec<Trigger>,
}

impl Piano
{
    pub fn start(state: &State, scene: &Scene) -> Option<Self>
    {
        let node = crate::track::find_node(&scene.nodes, &|node| node.tags.contains("revolt_piano"))?;
        let (json, sound_name) = { let read = node.read().unwrap(); (read.extras.get::<String>("revolt_piano").cloned()?, read.extras.get::<String>("revolt_sound_piano").cloned()) };
        let data: Value = serde_json::from_str(&json).ok()?;
        let float = |key: &str| data[key].as_f64().map(|v| v as f32);
        let vector = |value: &Value| Vector3::new(value[0].as_f64().unwrap_or(0.0) as f32, value[1].as_f64().unwrap_or(0.0) as f32, value[2].as_f64().unwrap_or(0.0) as f32);

        let source = sound_name.and_then(|name| state.resources.sound_sources.values().find(|source| source.read().unwrap().name == name).cloned());
        if source.is_none() { console_warning!("piano: no piano sound in the scene sound list - re-convert the level"); }
        let key = |name: String| -> Key
        {
            let found = crate::track::find_node(&node.read().unwrap().nodes, &|n| n.name == name);
            let sound = match (&found, &source)
            {
                (Some(key), Some(source)) =>
                {
                    let mut sound = Sound::new("Piano", source.clone(), SoundType::Spatial, false);
                    sound.get_data_mut().get_mut().spatial_distance_scale = SOUND_DISTANCE;
                    let component: ComponentItem = Arc::new(RwLock::new(Box::new(sound)));
                    key.write().unwrap().add_component(component.clone());
                    Some(component)
                },
                _ => None,
            };
            if let Some(key) = &found
            {
                key.write().unwrap().settings.visible = false;
                crate::for_each_node(&vec![key.clone()], &mut |part| crate::change_material(part, |m| { m.cast_shadow = false; m.receive_shadow = false; }));
            }
            Key { node: found, sound, timer: 0.0 }
        };

        let black_ids: Vec<i64> = data["black"].as_array()?.iter().filter_map(Value::as_i64).collect();
        let whites = data["white_keys"].as_u64().unwrap_or(32) as usize;
        let white = (0..whites).map(|i| key(format!("white_{i:02}"))).collect();
        let black = (0..black_ids.len()).map(|i| key(format!("black_{i:02}"))).collect();
        let triggers = data["triggers"].as_array().map(|list| list.iter().map(|t| Trigger
        {
            center: vector(&t["center"]),
            axes: [vector(&t["axes"][0]), vector(&t["axes"][1]), vector(&t["axes"][2])],
            half_size: vector(&t["half_size"]),
        }).collect()).unwrap_or_default();

        let piano = Self { scene_id: scene.id, x0: float("x0")?, z0: float("z0")?, y: float("y")?, key_width: float("key_width")?, depth: float("depth")?, black_from: float("black_from")?, mirrored: data["mirrored"].as_bool().unwrap_or(false), black_ids, white, black, triggers };
        console_log!("piano: {} white and {} black keys, {} triggers", piano.white.len(), piano.black.len(), piano.triggers.len());
        Some(piano)
    }

    pub fn scene_id(&self) -> u32 { self.scene_id }

    // the wheels of every car in a trigger box play the keys under them, the lights fade
    pub fn update(&mut self, scene: &Scene, dt: f32)
    {
        let mut wheels = vec![];
        for vehicle in scene.controller.iter().filter_map(|controller| controller.as_any().downcast_ref::<VehicleController>())
        {
            let Some(node) = vehicle.node.as_ref() else { continue; };
            let (id, transform) = { let read = node.read().unwrap(); (read.id, read.get_full_transform()) };
            let position = Vector3::new(transform.m14, transform.m24, transform.m34);
            if !self.triggers.is_empty() && !self.triggers.iter().any(|trigger| trigger.contains(position)) { continue; }
            let Some((entry, _)) = scene.physics.vehicle(id) else { continue; };
            for wheel in entry.controller.wheels()
            {
                let info = wheel.raycast_info();
                if info.is_in_contact
                {
                    let p = info.contact_point_ws;
                    wheels.push(Vector3::new(p.x, p.y, p.z));
                }
            }
        }

        for wheel in wheels
        {
            if (wheel.y - self.y).abs() > HEIGHT_RANGE { continue; }
            let x = (if self.mirrored { self.x0 - wheel.x } else { wheel.x - self.x0 }) / self.key_width;
            let z = self.z0 - wheel.z;
            if x < 0.0 || x * self.key_width > self.key_width * self.white.len() as f32 || z < 0.0 || z > self.depth { continue; }

            // the back part: a black key on a key boundary, else the white key
            if z > self.black_from
            {
                let boundary = (x + 0.5).floor();
                if (x - boundary).abs() < BLACK_HALF
                {
                    if let Some(index) = self.black_ids.iter().position(|&id| id == boundary as i64)
                    {
                        let note = self.note(boundary as i64, true);
                        hit(&mut self.black[index], note);
                        continue;
                    }
                }
            }
            let index = (x.floor() as usize).min(self.white.len() - 1);
            let note = self.note(index as i64, false);
            hit(&mut self.white[index], note);
        }

        for key in self.white.iter_mut().chain(self.black.iter_mut())
        {
            if key.timer <= 0.0 { continue; }
            key.timer = (key.timer - dt * LIGHT_FADE).max(0.0);
            let alpha = key.timer.floor() * LIGHT_ALPHA;
            if let Some(node) = &key.node
            {
                node.write().unwrap().settings.visible = alpha > 0.0;
                crate::for_each_node(&vec![node.clone()], &mut |part| crate::change_material(part, |m| m.alpha = alpha));
            }
        }
    }

    // the speed of the sample for a key: semitones counted over the white and the black keys from the left
    fn note(&self, id: i64, black: bool) -> f32
    {
        let below = self.black_ids.iter().filter(|&&b| b <= id).count() as i64;
        let semitone = below + id - black as i64;
        (NOTE_BASE + NOTE_STEP * semitone as f32) / SAMPLE_RATE
    }

    pub fn stop(&mut self)
    {
        for key in self.white.iter().chain(self.black.iter())
        {
            let Some(node) = &key.node else { continue; };
            node.write().unwrap().settings.visible = false;
            if let Some(sound) = &key.sound
            {
                {
                    component_downcast_mut!(sound, Sound);
                    sound.stop();
                }
                let id = sound.read().unwrap().id();
                node.write().unwrap().remove_component_by_id(id);
            }
        }
    }
}

impl Trigger
{
    fn contains(&self, point: Vector3<f32>) -> bool
    {
        let d = point - self.center;
        (0..3).all(|a| d.dot(&self.axes[a]).abs() <= self.half_size[a])
    }
}

// a key not lit yet plays its note, a lit one stays lit while a wheel is on it
fn hit(key: &mut Key, speed: f32)
{
    if key.timer <= 0.0
    {
        if let Some(sound) = &key.sound
        {
            component_downcast_mut!(sound, Sound);
            sound.get_data_mut().get_mut().speed = speed;
            sound.stop();
            sound.start();
        }
    }
    key.timer = LIGHT;
}

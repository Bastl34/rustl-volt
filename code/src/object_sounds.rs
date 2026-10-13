// the sounds of the moving objects like the exe (formats/README.md, Object motion): loops at the object, one shots on events of its motion
// the converter writes the sound names as extras revolt_sound_<role>; the components go onto the moving node of the glb while playing and away again in exit

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use rustl::prelude::*;
use rustl::state::scene::components::component::ComponentItem;
use rustl::state::scene::components::sound::{Sound, SoundType};

use crate::object_type::ObjectType;

// Miles min distance 3 at range 1.0, one Miles unit = 200 game units
const MIN_DISTANCE_UNITS: f32 = 600.0;
// the horse creaks at 22050 Hz one way and 20000 Hz the other
const CREAK_LOW: f32 = 20000.0 / 22050.0;
// the whistle blows once a car comes into this box ahead of the train (game units, train minus car)
const WHISTLE_X: f32 = 400.0;
const WHISTLE_Z: (f32, f32) = (-400.0, 800.0);

pub struct ObjectSounds
{
    scene_id: u32,
    kind: ObjectType,
    // the moving node of the glb - the sounds follow it
    holder: NodeItem,
    cars: Vec<NodeItem>,
    scale: f32,
    looped: Option<ComponentItem>,
    looping: bool,
    events: HashMap<String, ComponentItem>,
    // seconds since the animations started - the events follow their phase
    time: f32,
    phase: f32,
    ratchet: f32,
    // horse: the swing threshold of the exe, its sign flips on every creak
    swing: f32,
    // sprinkler: the highest ratchet value since the last tick
    peak: f32,
    armed: bool,
}

impl ObjectSounds
{
    pub fn new(state: &State, scene: &Scene, node: &NodeItem, kind: ObjectType) -> Option<Self>
    {
        let holder_name = match kind
        {
            ObjectType::Plane => Some("plane"),
            ObjectType::Copter => None,
            ObjectType::Train => Some("train"),
            ObjectType::Dragon | ObjectType::Sprinkler => Some("head"),
            ObjectType::Horse => Some("rock"),
            ObjectType::Slider => Some("slide"),
            _ => return None,
        };
        let holder = match holder_name
        {
            Some(name) => find_child(node, name)?,
            None => node.clone(),
        };

        let (sounds, scale, phase, ratchet) =
        {
            let node = node.read().unwrap();
            let sounds: Vec<(String, String)> = ["loop", "roar", "creak", "whistle", "tick", "open", "close"].iter()
                .filter_map(|role| node.extras.get::<String>(&format!("revolt_sound_{role}")).map(|name| (role.to_string(), name.clone())))
                .collect();
            (sounds, number(&node, "revolt_scale").unwrap_or(0.01), number(&node, "revolt_phase").unwrap_or(0.0), number(&node, "revolt_ratchet").unwrap_or(0.0))
        };
        if sounds.is_empty() { return None; }

        let mut looped = None;
        let mut events = HashMap::new();
        for (role, name) in sounds
        {
            let Some(source) = state.resources.sound_sources.values().find(|source| source.read().unwrap().name == name).cloned() else
            {
                console_warning!("{}: sound {} is not in the scene sound list - re-convert the level", node.read().unwrap().name, name);
                continue;
            };
            let is_loop = role == "loop";
            let mut sound = Sound::new(&format!("Revolt {role}"), source, SoundType::Spatial, is_loop);
            sound.get_data_mut().get_mut().spatial_distance_scale = MIN_DISTANCE_UNITS * scale;
            let component: ComponentItem = Arc::new(RwLock::new(Box::new(sound)));
            holder.write().unwrap().add_component(component.clone());
            match is_loop
            {
                true => looped = Some(component),
                false => { events.insert(role, component); },
            }
        }

        let mut cars = vec![];
        if kind == ObjectType::Train
        {
            crate::for_each_node(&scene.nodes, &mut |n| if n.read().unwrap().tags.contains("revolt_car") { cars.push(n.clone()); });
        }

        Some(Self { scene_id: scene.id, kind, holder, cars, scale, looped, looping: false, events, time: 0.0, phase, ratchet, swing: 0.01, peak: f32::MIN, armed: true })
    }

    pub fn scene_id(&self) -> u32 { self.scene_id }

    // seconds of this frame - the time runs with the animations, the sounds only play in an active scene
    pub fn update(&mut self, seconds: f32, active: bool)
    {
        let before = self.time;
        self.time += seconds;
        let now = self.time;

        if let Some(looped) = &self.looped
        {
            if active != self.looping
            {
                component_downcast_mut!(looped, Sound);
                if active { looped.start(); } else { looped.stop(); }
                self.looping = active;
            }
        }

        let crossed = |period: f32, at: f32| (before - at).div_euclid(period) != (now - at).div_euclid(period);
        let mut play: Vec<(&str, f32)> = vec![];
        match self.kind
        {
            // the mouth opens at 2 s of its 8 s cycle
            ObjectType::Dragon => if crossed(8.0, 2.0) { play.push(("roar", 1.0)); },
            ObjectType::Horse =>
            {
                let swing = (4.0 * now + self.phase).sin();
                if (self.swing > 0.0 && swing > self.swing) || (self.swing < 0.0 && swing < self.swing)
                {
                    play.push(("creak", if self.swing > 0.0 { 1.0 } else { CREAK_LOW }));
                    self.swing = -self.swing;
                }
            },
            ObjectType::Sprinkler =>
            {
                let ratchet = (self.ratchet * now).sin();
                if ratchet < self.peak
                {
                    if self.armed { play.push(("tick", 1.0)); }
                    self.armed = false;
                }
                else
                {
                    self.armed = true;
                    self.peak = ratchet;
                }
            },
            // open at the far end (1.5 s), closed at the start of the 3 s cycle
            ObjectType::Slider =>
            {
                if crossed(3.0, 1.5) { play.push(("close", 1.0)); }
                if crossed(3.0, 0.0) { play.push(("open", 1.0)); }
            },
            ObjectType::Train =>
            {
                let near = self.car_ahead();
                if near && self.armed { play.push(("whistle", 1.0)); }
                self.armed = !near;
            },
            _ => {},
        }

        if !active { return; }
        for (role, speed) in play
        {
            if let Some(component) = self.events.get(role)
            {
                component_downcast_mut!(component, Sound);
                component.get_data_mut().get_mut().speed = speed;
                component.stop();
                component.start();
            }
        }
    }

    // a car in the whistle box of the exe
    fn car_ahead(&self) -> bool
    {
        let train = position(&self.holder);
        self.cars.iter().any(|car|
        {
            let car = position(car);
            let dx = (train.x - car.x) / self.scale;
            // glTF z is the negative game z
            let dz = (car.z - train.z) / self.scale;
            dx.abs() < WHISTLE_X && dz > WHISTLE_Z.0 && dz < WHISTLE_Z.1
        })
    }

    pub fn remove(&self)
    {
        let mut holder = self.holder.write().unwrap();
        for component in self.looped.iter().chain(self.events.values())
        {
            {
                component_downcast_mut!(component, Sound);
                component.stop();
            }
            let id = component.read().unwrap().id();
            holder.remove_component_by_id(id);
        }
    }
}

fn position(node: &NodeItem) -> Vector3<f32>
{
    let transform = node.read().unwrap().get_full_transform();
    Vector3::new(transform.m14, transform.m24, transform.m34)
}

fn find_child(node: &NodeItem, name: &str) -> Option<NodeItem>
{
    let mut found = None;
    crate::for_each_node(&node.read().unwrap().nodes.clone(), &mut |child| if found.is_none() && child.read().unwrap().name == name { found = Some(child.clone()); });
    found
}

fn number(node: &Node, key: &str) -> Option<f32>
{
    node.extras.get::<f64>(key).map(|v| *v as f32).or_else(|| node.extras.get::<i64>(key).map(|v| *v as f32))
}

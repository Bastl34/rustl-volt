// the laser beams of the museum (.fob type 33, formats/README.md "Laser") - behaviour of the game, own code
// a beam along the look axis of the object up to the level (the converter measures it one sided like the game: revolt_laser_length), with flag "object" also up to the cars and objects in it (ray per frame)
// it flickers in width (width + random × a 100 ms sawtooth), hums; something in the beam sets off the alarm for 5 s

use std::sync::{Arc, RwLock};

use rustl::prelude::*;
use rustl::rapier3d::prelude::{QueryFilter, Ray, Vector};
use rustl::state::scene::components::component::ComponentItem;
use rustl::state::scene::components::sound::{Sound, SoundType};
use rustl::state::scene::components::transformation::TransformationData;

use crate::object_type::ObjectType;

// the game casts 10000 units ahead
const REACH: f32 = 100.0;
// m of beam width per unit of the width flag (the game draws 1-3 pixels at 640 x 480)
const WIDTH_UNIT: f32 = 0.012;
// the sawtooth of the flicker, s
const FLICKER_PERIOD: f32 = 0.1;
// the alarm sounds this long after the last block
const ALARM_TIME: f32 = 5.0;
// the hum and the alarm: full volume up to this many m (range 1.0 of the game)
const SOUND_DISTANCE: f32 = 6.0;
// the beam is red and glows: tint of its texture and emission (HDR, bloom) - green / blue low or the bloom turns it pink-white
const TINT: Vector3<f32> = Vector3::new(1.0, 0.12, 0.08);
const GLOW: Vector3<f32> = Vector3::new(1.6, 0.02, 0.01);

struct Laser
{
    scene_id: u32,
    node: NodeItem,
    restore: Option<TransformationData>,
    // m along the look axis to the level, None until the physics of the scene is there
    length: Option<f32>,
    // tries of the level ray - a laser that hits nothing reaches REACH
    tries: u32,
    width: f32,
    random: f32,
    // the beam stops at cars and objects too
    blocks: bool,
    phase: f32,
    alarm: f32,
    hum: Option<ComponentItem>,
    siren: Option<ComponentItem>,
    playing: Option<bool>,
}

#[derive(Default)]
pub struct Lasers
{
    lasers: Vec<Laser>,
    time: f32,
}

impl Lasers
{
    pub fn add_scene(&mut self, state: &State, scene: &Scene)
    {
        let source = |name: Option<String>| name.and_then(|name| state.resources.sound_sources.values().find(|source| source.read().unwrap().name == name).cloned());
        let mut added = 0;
        crate::for_each_node(&scene.nodes, &mut |node|
        {
            let (is_laser, width, random, blocks, hum, siren, length) =
            {
                let read = node.read().unwrap();
                let is_laser = read.tags.contains("revolt_object") && read.extras.get::<i64>("revolt_object_type").and_then(|id| ObjectType::from_id(*id)) == Some(ObjectType::Laser);
                let int = |key: &str| read.extras.get::<i64>(key).copied().unwrap_or(0);
                let name = |key: &str| read.extras.get::<String>(key).cloned();
                (is_laser, int("revolt_param_width"), int("revolt_param_random"), int("revolt_param_object") != 0, name("revolt_sound_loop"), name("revolt_sound_alarm"), read.extras.get::<f64>("revolt_laser_length").map(|l| *l as f32))
            };
            if !is_laser || self.lasers.iter().any(|laser| Arc::ptr_eq(&laser.node, node)) { return; }

            let restore = node.read().unwrap().find_component::<Transformation>().map(|t| { component_downcast!(t, Transformation); *t.get_data() });
            crate::for_each_node(&vec![node.clone()], &mut |part| crate::change_material(part, |m| { m.base_color = TINT; m.ambient_color = GLOW; m.cast_shadow = false; m.receive_shadow = false; }));
            let hum = source(hum).map(|source| add_sound(node, "Laser Hum", source));
            let siren = source(siren).map(|source| add_sound(node, "Laser Alarm", source));
            let phase = (self.lasers.len() as f32 * 0.37).fract() * FLICKER_PERIOD;
            self.lasers.push(Laser { scene_id: scene.id, node: node.clone(), restore, length, tries: 0, width: width as f32, random: random as f32, blocks, phase, alarm: 0.0, hum, siren, playing: None });
            added += 1;
        });
        if added > 0
        {
            console_log!("scene '{}': {} lasers", scene.name, added);
        }
    }

    pub fn update(&mut self, state: &State, dt: f32)
    {
        self.time += dt;
        for laser in &mut self.lasers
        {
            let Some(scene) = state.find_scene_by_id(laser.scene_id) else { continue; };
            let transform = laser.node.read().unwrap().get_full_transform();
            let origin = Vector3::new(transform.m14, transform.m24, transform.m34);
            let Some(direction) = (-transform.column(2).xyz()).try_normalize(1e-6) else { continue; };

            // without the length of the converter: the level once, as soon as the physics has it (two sided - re-convert)
            if laser.length.is_none()
            {
                laser.tries += 1;
                let hit = cast(scene, origin, direction, REACH, QueryFilter::only_fixed());
                if hit.is_some() || laser.tries > 60
                {
                    laser.length = Some(hit.unwrap_or(REACH));
                    console_log!("laser '{}': {:.1} m", laser.node.read().unwrap().name, laser.length.unwrap_or(REACH));
                }
            }
            let length = laser.length.unwrap_or(REACH);
            let blocked = laser.blocks.then(|| cast(scene, origin, direction, length, QueryFilter::exclude_fixed())).flatten();
            if blocked.is_some()
            {
                laser.alarm = ALARM_TIME;
            }
            else
            {
                laser.alarm = (laser.alarm - dt).max(0.0);
            }

            let saw = ((self.time + laser.phase) / FLICKER_PERIOD).fract();
            let width = (laser.width + laser.random * saw).max(0.5) * WIDTH_UNIT;
            set_scale(&laser.node, laser.restore.as_ref(), Vector3::new(width, width, blocked.unwrap_or(length).max(0.01)));

            // the hum, the alarm while something is in the beam and 5 s after - only in the active scene
            let playing = scene.active.then_some(laser.alarm > 0.0);
            if playing != laser.playing
            {
                switch(&laser.hum, playing == Some(false));
                switch(&laser.siren, playing == Some(true));
                laser.playing = playing;
            }
        }
    }

    pub fn stop(&mut self)
    {
        for laser in self.lasers.drain(..)
        {
            let node = laser.node.read().unwrap();
            if let (Some(data), Some(transform)) = (laser.restore, node.find_component::<Transformation>())
            {
                component_downcast_mut!(transform, Transformation);
                transform.get_data_mut().set(data);
            }
            drop(node);
            for sound in laser.hum.iter().chain(laser.siren.iter())
            {
                {
                    component_downcast_mut!(sound, Sound);
                    sound.stop();
                }
                let id = sound.read().unwrap().id();
                laser.node.write().unwrap().remove_component_by_id(id);
            }
        }
    }

    // the scene was unloaded - its lasers go silent, the nodes are gone
    pub fn remove_scene(&mut self, scene_id: u32)
    {
        for laser in self.lasers.iter().filter(|laser| laser.scene_id == scene_id)
        {
            for sound in laser.hum.iter().chain(laser.siren.iter())
            {
                component_downcast_mut!(sound, Sound);
                sound.stop();
            }
        }
        self.lasers.retain(|laser| laser.scene_id != scene_id);
    }
}

// m to the first hit along the ray, sensors left out
fn cast(scene: &Scene, origin: Vector3<f32>, direction: Vector3<f32>, max: f32, filter: QueryFilter) -> Option<f32>
{
    let ray = Ray::new(Vector::new(origin.x, origin.y, origin.z), Vector::new(direction.x, direction.y, direction.z));
    scene.physics.query_pipeline(filter.exclude_sensors()).cast_ray(&ray, max, true).map(|(_, distance)| distance)
}

// the beam glb is 1 m long along -z and 1 m wide: scaled to the beam of this frame
fn set_scale(node: &NodeItem, restore: Option<&TransformationData>, scale: Vector3<f32>)
{
    let node = node.read().unwrap();
    let Some(transform) = node.find_component::<Transformation>() else { return; };
    component_downcast_mut!(transform, Transformation);
    let base = restore.map_or(Vector3::repeat(1.0), |data| data.scale);
    transform.get_data_mut().get_mut().scale = base.component_mul(&scale);
    transform.calc_transform();
}

fn add_sound(node: &NodeItem, name: &str, source: rustl::state::resources::sound_source::SoundSourceItem) -> ComponentItem
{
    let mut sound = Sound::new(name, source, SoundType::Spatial, true);
    sound.get_data_mut().get_mut().spatial_distance_scale = SOUND_DISTANCE;
    let component: ComponentItem = Arc::new(RwLock::new(Box::new(sound)));
    node.write().unwrap().add_component(component.clone());
    component
}

fn switch(sound: &Option<ComponentItem>, on: bool)
{
    let Some(component) = sound else { return; };
    component_downcast_mut!(component, Sound);
    if on { component.start(); } else { component.stop(); }
}

// the moving objects of the levels (formats/README.md, Object motion): the converter writes their motion as looping glb animations (tag revolt_animated)
// they start here; the copter flies here like in the game: random points in a box, its glb only turns the rotors

use rustl::nalgebra::{Quaternion, Unit, UnitQuaternion};
use rustl::prelude::*;
use rustl::state::scene::components::transformation::{Transformation, TransformationData};
use rustl::state::scene::node::Node;
use rustl::state::scene::scene::Scene;
use rustl::state::state::{State, REFERENCE_UPDATE_FRAMES};

use crate::object_sounds::ObjectSounds;
use crate::object_type::ObjectType;
use crate::random::Random;

// game units and seconds like the game
const COPTER_WAIT: f32 = 2.0;
const COPTER_TAKEOFF_BACK: f32 = 600.0;
const COPTER_MAX_SPEED: f32 = 300.0;
const COPTER_ACCELERATION: f32 = 100.0;
const COPTER_ARRIVED: f32 = 10.0;
const COPTER_MIN_HOP: f32 = 1000.0;
const COPTER_TURN_TIME: f32 = 5.0;
const COPTER_TILT: f32 = 0.15;
// share of the hops where it turns to face the next point
const COPTER_FACE_TARGET: f32 = 0.8;

struct Copter
{
    scene_id: u32,
    node: NodeItem,
    restore: Option<TransformationData>,
    scale: f32,
    // game units: x right, y down, z forward
    position: Vector3<f32>,
    box_min: Vector3<f32>,
    box_max: Vector3<f32>,
    wait: f32,
    target: Vector3<f32>,
    direction: Vector3<f32>,
    speed: f32,
    // glTF rotations: the heading slerps from -> to in 5 s
    turn_from: UnitQuaternion<f32>,
    turn_to: UnitQuaternion<f32>,
    turn_time: f32,
}

#[derive(Default)]
pub struct Animated
{
    // scene id and node of every started animation
    nodes: Vec<(u32, NodeItem)>,
    copters: Vec<Copter>,
    sounds: Vec<ObjectSounds>,
    random: Random,
}

impl Animated
{
    // starts the animations of the moving objects of a loaded scene
    pub fn add_scene(&mut self, state: &State, scene: &Scene)
    {
        let mut found = vec![];
        crate::for_each_node(&scene.nodes, &mut |node| if node.read().unwrap().tags.contains("revolt_animated") { found.push(node.clone()); });

        let (mut moving, mut copters) = (0, 0);
        for node in found
        {
            if self.nodes.iter().any(|(_, n)| std::sync::Arc::ptr_eq(n, &node)) { continue; }
            node.read().unwrap().start_all_animations();

            let kind = node.read().unwrap().extras.get::<i64>("revolt_object_type").and_then(|id| ObjectType::from_id(*id));
            if kind == Some(ObjectType::Copter)
            {
                if let Some(copter) = Copter::new(scene.id, &node)
                {
                    self.copters.push(copter);
                    copters += 1;
                }
            }
            if let Some(sounds) = kind.and_then(|kind| ObjectSounds::new(state, scene, &node, kind))
            {
                self.sounds.push(sounds);
            }
            self.nodes.push((scene.id, node));
            moving += 1;
        }

        if moving > 0
        {
            console_log!("scene '{}': {} moving objects, {} copters flying", scene.name, moving, copters);
        }
    }

    pub fn update(&mut self, state: &State)
    {
        let seconds = (state.stats.frame_scale / REFERENCE_UPDATE_FRAMES).min(0.1);
        for copter in &mut self.copters
        {
            if state.scenes.iter().any(|scene| scene.id == copter.scene_id && scene.active)
            {
                copter.update(seconds, &mut self.random);
            }
        }
        for sounds in &mut self.sounds
        {
            let active = state.scenes.iter().any(|scene| scene.id == sounds.scene_id() && scene.active);
            sounds.update(seconds, active);
        }
    }

    // Play stops - back to the start poses
    pub fn stop(&mut self)
    {
        for (_, node) in self.nodes.drain(..)
        {
            node.read().unwrap().stop_all_animations();
        }
        for copter in self.copters.drain(..)
        {
            copter.restore();
        }
        for sounds in self.sounds.drain(..)
        {
            sounds.remove();
        }
    }

    // the scene was unloaded - its nodes are gone, nothing to restore
    pub fn remove_scene(&mut self, scene_id: u32)
    {
        self.nodes.retain(|(id, _)| *id != scene_id);
        self.copters.retain(|copter| copter.scene_id != scene_id);
        for sounds in self.sounds.iter().filter(|sounds| sounds.scene_id() == scene_id)
        {
            sounds.remove();
        }
        self.sounds.retain(|sounds| sounds.scene_id() != scene_id);
    }
}

impl Copter
{
    fn new(scene_id: u32, node: &NodeItem) -> Option<Self>
    {
        let (restore, flags, scale) =
        {
            let node = node.read().unwrap();
            let transform = node.find_component::<Transformation>()?;
            component_downcast!(transform, Transformation);
            let flags: Vec<f32> = (0..4).map(|i| number(&node, &format!("revolt_flag_{i}")).unwrap_or(0.0)).collect();
            (*transform.get_data(), flags, number(&node, "revolt_scale").unwrap_or(0.01))
        };

        let p = restore.position;
        let position = Vector3::new(p.x, -p.y, -p.z) / scale;
        let rotation = restore.rotation_quat.map_or(UnitQuaternion::identity(), |q| UnitQuaternion::new_normalize(Quaternion::new(q.w, q.x, q.y, q.z)));
        let center = Vector3::new(position.x, position.y - flags[3] * 50.0, position.z);
        let half = Vector3::new(flags[0], flags[1], flags[2]) * 10.0;

        Some(Self
        {
            scene_id,
            node: node.clone(),
            restore: Some(restore),
            scale,
            position,
            box_min: center - half,
            box_max: center + half,
            wait: COPTER_WAIT,
            target: position,
            direction: Vector3::zeros(),
            speed: 0.0,
            turn_from: rotation,
            turn_to: rotation,
            turn_time: COPTER_TURN_TIME,
        })
    }

    fn update(&mut self, seconds: f32, random: &mut Random)
    {
        // on the ground for 2 s, then up to the top of the box and 6 m back
        if self.wait > 0.0
        {
            self.wait -= seconds;
            if self.wait <= 0.0
            {
                let look = game(self.turn_to * Vector3::new(0.0, 0.0, -1.0));
                let target = Vector3::new(self.position.x - look.x * COPTER_TAKEOFF_BACK, self.box_min.y, self.position.z - look.z * COPTER_TAKEOFF_BACK);
                self.head_for(target);
            }
            self.apply();
            return;
        }

        let distance = (self.target - self.position).norm();
        let mut arrived = distance < COPTER_ARRIVED;
        let cap = distance.min(COPTER_MAX_SPEED);
        if self.speed < cap { self.speed += seconds * COPTER_ACCELERATION; } else { self.speed = cap; }
        if self.speed < 0.0
        {
            self.speed = 0.0;
            arrived = true;
        }
        self.position += self.direction * self.speed * seconds;
        self.turn_time = (self.turn_time + seconds).min(COPTER_TURN_TIME);
        self.apply();

        if arrived
        {
            self.pick_target(random);
        }
    }

    // a random point in the box at least 1000 units away (10 tries), most of the time facing it
    fn pick_target(&mut self, random: &mut Random)
    {
        let mut target = self.position;
        for _ in 0..10
        {
            target = self.box_min + (self.box_max - self.box_min).component_mul(&Vector3::new(random.next(), random.next(), random.next()));
            if (target - self.position).norm() >= COPTER_MIN_HOP { break; }
        }
        self.head_for(target);

        let flat = gltf(Vector3::new(self.direction.x, 0.0, self.direction.z));
        if random.next() < COPTER_FACE_TARGET && flat.norm() > 1e-5
        {
            // the model looks along its game z = glTF -z
            self.turn_to = UnitQuaternion::face_towards(&-flat, &Vector3::y());
        }
    }

    fn head_for(&mut self, target: Vector3<f32>)
    {
        self.target = target;
        self.direction = (target - self.position).try_normalize(1e-6).unwrap_or(Vector3::zeros());
        self.speed = 0.0;
        self.turn_from = self.turn_to;
        self.turn_time = 0.0;
    }

    // the heading slerps in 5 s, the nose dips with the speed
    fn apply(&self)
    {
        let k = (((self.turn_time - COPTER_TURN_TIME / 2.0) * 0.2 * std::f32::consts::PI).sin() + 1.0) / 2.0;
        let heading = self.turn_from.slerp(&self.turn_to, k);
        let direction = gltf(self.direction);
        let tilt = match Vector3::y().cross(&direction).try_normalize(1e-6)
        {
            Some(axis) => UnitQuaternion::from_axis_angle(&Unit::new_unchecked(axis), 2.0 * COPTER_TILT * self.speed / COPTER_MAX_SPEED),
            None => UnitQuaternion::identity(),
        };
        let rotation = tilt * heading;
        let position = gltf(self.position) * self.scale;

        let node = self.node.read().unwrap();
        if let Some(transform) = node.find_component::<Transformation>()
        {
            component_downcast_mut!(transform, Transformation);
            let data = transform.get_data_mut().get_mut();
            data.position = position;
            data.rotation_quat = Some(Vector4::new(rotation.i, rotation.j, rotation.k, rotation.w));
            transform.calc_transform();
        }
    }

    fn restore(&self)
    {
        let node = self.node.read().unwrap();
        if let (Some(data), Some(transform)) = (self.restore, node.find_component::<Transformation>())
        {
            component_downcast_mut!(transform, Transformation);
            transform.get_data_mut().set(data);
        }
    }
}

// game space (y down, z forward) <-> glTF
fn gltf(v: Vector3<f32>) -> Vector3<f32> { Vector3::new(v.x, -v.y, -v.z) }
fn game(v: Vector3<f32>) -> Vector3<f32> { Vector3::new(v.x, -v.y, -v.z) }

fn number(node: &Node, key: &str) -> Option<f32>
{
    node.extras.get::<f64>(key).map(|v| *v as f32).or_else(|| node.extras.get::<i64>(key).map(|v| *v as f32))
}

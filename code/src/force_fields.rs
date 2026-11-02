// the force fields of the level (formats/README.md ".fld"): the water - boxes that lift and brake the cars and objects inside along their direction (linear)
// or turn them upright (orientation), values in units of the game like the ufo forces, per body mass of the game

use std::collections::HashMap;

use rustl::prelude::*;
use rustl::rapier3d::prelude::{RigidBody, RigidBodyHandle, Vector};
use rustl::serde_json::{self, Value};
use rustl::state::scene::scene_controller::vehicle_controller::VehicleController;

use crate::for_each_node;

const LINEAR: u64 = 0;
const ORIENTATION: u64 = 1;

struct Field
{
    orientation: bool,
    center: Vector,
    axes: [Vector; 3],
    half_size: [f32; 3],
    direction: Vector,
    magnitude: f32,
    damping: f32,
}

// a physics object of the level - the body is looked up again after a physics rebuild
struct Object
{
    node_id: u32,
    mass: f32,
    body: Option<RigidBodyHandle>,
    // force and torque this code put on the body
    applied: (Vector, Vector),
}

struct SceneFields
{
    scene_id: u32,
    scale: f32,
    fields: Vec<Field>,
    objects: Vec<Object>,
}

#[derive(Default)]
pub struct ForceFields
{
    scenes: Vec<SceneFields>,
    // scenes whose level glb has not brought its extras yet (they come after scene_loaded)
    pending: Vec<u32>,
    // per car node: the body mass of the game
    car_masses: HashMap<u32, f32>,
}

impl ForceFields
{
    pub fn add_scene(&mut self, scene: &Scene)
    {
        if self.pending.contains(&scene.id) || self.scenes.iter().any(|part| part.scene_id == scene.id) { return; }
        self.pending.push(scene.id);
    }

    pub fn remove_scene(&mut self, scene_id: u32)
    {
        self.scenes.retain(|part| part.scene_id != scene_id);
        self.pending.retain(|id| *id != scene_id);
    }

    pub fn stop(&mut self)
    {
        *self = Self::default();
    }

    // the waiting scenes whose level extras are there now
    fn resolve(&mut self, state: &State)
    {
        let scenes = &mut self.scenes;
        self.pending.retain(|scene_id|
        {
            let Some(scene) = state.find_scene_by_id(*scene_id) else { return false; };
            match crate::track::level_node(scene)
            {
                None => false,
                Some(None) => true,
                Some(Some(level)) => { scenes.extend(load(scene, &level)); false },
            }
        });
    }

    pub fn update(&mut self, state: &mut State, dt: f32)
    {
        if !self.pending.is_empty() { self.resolve(state); }
        if dt <= 0.0 { return; }
        for part in &mut self.scenes
        {
            let Some(scene) = state.find_scene_by_id_mut(part.scene_id).filter(|scene| scene.active) else { continue; };

            // the cars: body mass of the game from the model, the up of the vehicle frame
            let cars: Vec<(u32, Vector3<f32>)> = scene.controller.iter()
                .filter_map(|controller| controller.as_any().downcast_ref::<VehicleController>())
                .filter_map(|vehicle| Some((vehicle.node.as_ref()?.clone(), vehicle.frame?.up)))
                .filter(|(node, _)| node.read().unwrap().tags.contains("revolt_car"))
                .map(|(node, up)|
                {
                    let id = node.read().unwrap().id;
                    self.car_masses.entry(id).or_insert_with(|| body_mass(&node));
                    (id, up)
                }).collect();
            // as forces the physics integrates itself - at a low frame rate it simulates less time than the frame took
            // the cars: through the controller, it resets the body forces before every step
            let pushes: Vec<(u32, Vector, Vector)> = cars.iter().filter_map(|(id, up)|
            {
                let (_, body) = scene.physics.vehicle(*id)?;
                let (force, torque) = forces(&part.fields, part.scale, body, self.car_masses[id], Vector::new(up.x, up.y, up.z));
                Some((*id, force, torque))
            }).collect();
            for vehicle in scene.controller.iter_mut().filter_map(|controller| controller.as_any_mut().downcast_mut::<VehicleController>())
            {
                let Some(id) = vehicle.node.as_ref().map(|node| node.read().unwrap().id) else { continue; };
                let Some((_, force, torque)) = pushes.iter().find(|(car, ..)| *car == id) else { continue; };
                vehicle.external_force = Vector3::new(force.x, force.y, force.z);
                vehicle.external_torque = Vector3::new(torque.x, torque.y, torque.z);
            }

            // the objects: a force that stays on the body, changed by the difference - only while awake, a resting or waiting one stays where it is
            for object in &mut part.objects
            {
                if object.body.is_none_or(|handle| scene.physics.bodies.get(handle).is_none())
                {
                    object.body = scene.physics.body_of(object.node_id);
                    object.applied = (Vector::ZERO, Vector::ZERO);
                }
                let Some(body) = object.body.and_then(|handle| scene.physics.bodies.get_mut(handle)) else { continue; };
                if !body.is_dynamic() || body.is_sleeping() { continue; }
                let (force, torque) = forces(&part.fields, part.scale, body, object.mass, Vector::Y);
                if (force, torque) == object.applied { continue; }
                body.add_force(force - object.applied.0, true);
                body.add_torque(torque - object.applied.1, true);
                object.applied = (force, torque);
            }
        }
    }
}

// the fields of a level whose extras are loaded - None without any
fn load(scene: &Scene, level: &NodeItem) -> Option<SceneFields>
{
    let level = level.read().unwrap();
    let json: Value = level.extras.get::<String>("revolt_fields").and_then(|text| serde_json::from_str(text).ok()).unwrap_or_default();
    let entries = json.as_array().filter(|entries| !entries.is_empty())?;
    let transform = level.get_full_transform();
    let scale = level.extras.get::<f64>("revolt_scale").map_or(0.01, |scale| *scale as f32);
    let vector = |value: &Value| Vector3::new(value[0].as_f64().unwrap_or(0.0) as f32, value[1].as_f64().unwrap_or(0.0) as f32, value[2].as_f64().unwrap_or(0.0) as f32);
    let rapier = |v: Vector3<f32>| Vector::new(v.x, v.y, v.z);
    let fields: Vec<Field> = entries.iter().filter(|field| matches!(field["kind"].as_u64(), Some(LINEAR | ORIENTATION))).map(|field|
    {
        let half_size = vector(&field["half_size"]);
        Field
        {
            orientation: field["kind"].as_u64() == Some(ORIENTATION),
            center: rapier(transform.transform_point(&Point3::from(vector(&field["center"]))).coords),
            axes: [0, 1, 2].map(|row| rapier(transform.transform_vector(&vector(&field["axes"][row])).try_normalize(1e-6).unwrap_or_else(Vector3::zeros))),
            half_size: [half_size.x, half_size.y, half_size.z],
            direction: rapier(transform.transform_vector(&vector(&field["direction"])).try_normalize(1e-6).unwrap_or_else(Vector3::zeros)),
            magnitude: field["magnitude"].as_f64().unwrap_or(0.0) as f32,
            damping: field["damping"].as_f64().unwrap_or(0.0) as f32,
        }
    }).collect();
    drop(level);

    let mut objects = vec![];
    for_each_node(&scene.nodes, &mut |node|
    {
        let node = node.read().unwrap();
        if let Some(mass) = node.extras.get::<f64>("revolt_mass").filter(|mass| **mass > 0.0) { objects.push(Object { node_id: node.id, mass: *mass as f32, body: None, applied: (Vector::ZERO, Vector::ZERO) }); }
    });
    console_log!("force fields: scene '{}': {} linear, {} orientation, {} physics objects", scene.name, fields.iter().filter(|field| !field.orientation).count(), fields.iter().filter(|field| field.orientation).count(), objects.len());
    Some(SceneFields { scene_id: scene.id, scale, fields, objects })
}

// force and torque of every field the centre of the body is in: linear = (magnitude - damping × speed along the direction) along it,
// orientation = a turn of the body up towards the direction, braked by the spin about that axis - both per body mass of the game
fn forces(fields: &[Field], scale: f32, body: &RigidBody, mass: f32, local_up: Vector) -> (Vector, Vector)
{
    let center = body.center_of_mass();
    let (mut force, mut torque) = (Vector::ZERO, Vector::ZERO);
    for field in fields.iter().filter(|field| field.contains(center))
    {
        if field.orientation
        {
            let up = body.rotation() * local_up;
            let axis = up.cross(field.direction);
            let strength = (1.0 - field.damping * axis.dot(body.angvel())) * field.magnitude;
            // the inertia of the body is the one of the game × scale² × this (like the ufo)
            torque += axis * (strength * scale * scale * body.mass() / mass);
        }
        else
        {
            let push = (field.magnitude * scale - field.damping * body.linvel().dot(field.direction)) / mass;
            force += field.direction * (push * body.mass());
        }
    }
    (force, torque)
}

impl Field
{
    fn contains(&self, point: Vector) -> bool
    {
        let delta = point - self.center;
        (0..3).all(|axis| delta.dot(self.axes[axis]).abs() <= self.half_size[axis])
    }
}

// the BODY mass of Parameters.txt on the model of the car
fn body_mass(node: &NodeItem) -> f32
{
    let mut mass = None;
    for_each_node(&vec![node.clone()], &mut |part|
    {
        let part = part.read().unwrap();
        if mass.is_none() && part.name == "model" { mass = part.extras.get::<f64>("revolt_body_mass").map(|m| *m as f32).filter(|m| *m > 0.0); }
    });
    mass.unwrap_or(1.0)
}

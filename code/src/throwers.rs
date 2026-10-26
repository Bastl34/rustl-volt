// the object throwers of the levels (formats/README.md "Object thrower"; nhood1: two throw a basketball onto the road): the first car in a
// thrower trigger sets the thrower of that ID off, once per race - its object comes out of the thrower along its look axis

use std::sync::{Arc, RwLock};

use rustl::prelude::*;
use rustl::state::scene::loader::loader::{load_asset, LoaderOptions};
use rustl::state::scene::node::PhysicsBodyType;
use rustl::state::state::State;

use crate::race::Race;
use crate::sounds::Sounds;

// units/s per speed of the flag - × the scale of the level gives m/s
const SPEED_UNIT: f32 = 50.0;
const UNIT: f32 = 0.01;

struct Thrower
{
    id: i32,
    kind: i64,
    // the glb of the thrown object, relative to the project folder
    source: String,
    position: Vector3<f32>,
    look: Vector3<f32>,
    speed: f32,
}

pub struct Throwers
{
    scene_id: u32,
    throwers: Vec<Thrower>,
    // indices into the thrower triggers of the track that did not fire yet
    armed: Vec<usize>,
    thrown: Vec<NodeItem>,
}

impl Throwers
{
    pub fn start(scene: &Scene, race: &Race) -> Self
    {
        let mut throwers = vec![];
        crate::for_each_node(&scene.nodes, &mut |node|
        {
            let read = node.read().unwrap();
            if !read.tags.contains("object_thrower") { return; }
            let number = |key: &str| read.extras.get::<i64>(key).copied();
            let Some(source) = read.extras.get::<String>("revolt_thrown_source").cloned() else { return; };
            let transform = read.get_full_transform();
            throwers.push(Thrower
            {
                id: number("revolt_param_id").unwrap_or(-1) as i32,
                kind: number("revolt_param_object").unwrap_or(-1),
                source,
                position: transform.column(3).xyz(),
                look: (-transform.column(2).xyz()).try_normalize(1e-6).unwrap_or_else(Vector3::y),
                speed: number("revolt_param_speed").unwrap_or(0) as f32 * SPEED_UNIT * UNIT,
            });
        });
        let armed = if throwers.is_empty() { vec![] } else { (0..race.track().throws.len()).collect() };
        if !throwers.is_empty() { console_log!("throwers: {} throwers, {} triggers", throwers.len(), armed.len()); }
        Self { scene_id: scene.id, throwers, armed, thrown: vec![] }
    }

    pub fn scene_id(&self) -> u32
    {
        self.scene_id
    }

    pub fn update(&mut self, state: &mut State, race: &Race, sounds: &mut Sounds, assets: &str)
    {
        if self.armed.is_empty() { return; }
        let cars: Vec<Vector3<f32>> = race.racers.iter().map(|racer| racer.node.read().unwrap().get_full_transform().column(3).xyz()).collect();
        let throws = &race.track().throws;
        let (fired, armed): (Vec<usize>, Vec<usize>) = self.armed.iter().partition(|index| cars.iter().any(|car| throws[**index].contains(*car)));
        self.armed = armed;
        for id in fired.iter().map(|index| throws[*index].id)
        {
            for index in (0..self.throwers.len()).filter(|index| self.throwers[*index].id == id)
            {
                match self.throw(state, index)
                {
                    Ok(node) => { sounds.add_object(state, self.scene_id, assets, &node); self.thrown.push(node); },
                    Err(error) => { console_warning!("throwers: {}", error); },
                }
            }
        }
    }

    // the object at the thrower, flying off along its look axis - a physics object like the ones of the level, gone on Stop
    fn throw(&self, state: &mut State, index: usize) -> Result<NodeItem, String>
    {
        let thrower = &self.throwers[index];
        let folder = state.find_scene_by_id(self.scene_id).map(|scene| crate::project_folder(scene)).unwrap_or_default();
        let path = if folder.is_empty() { thrower.source.clone() } else { format!("{folder}/{}", thrower.source) };
        let options = LoaderOptions
        {
            path: path.clone(),
            extension: "glb".to_string(),
            parent_node_id: None,
            hide_root_nodes: false,
            reuse_materials: false,
            clear_unused_textures: true,
            object_only: true,
            create_mipmaps: true,
            max_texture_resolution: state.max_texture_resolution(),
            texture_cache: None,
            material_cache: None,
        };
        let mut container = load_asset(&options).map_err(|error| format!("{path}: {error}"))?;
        let result = container.apply_to_scene(state, self.scene_id);
        let scene = state.find_scene_by_id_mut(self.scene_id).ok_or("scene gone")?;
        let node = result.node_ids.iter().filter_map(|id| scene.find_node_by_id(*id)).find(|node| node.read().unwrap().root_node).ok_or("no root node")?;
        // the loader returns a container - the body is the glb node below it (rustl_body_type)
        let mut body = None;
        crate::for_each_node(&vec![node.clone()], &mut |part| if body.is_none() && part.read().unwrap().settings.physics.body_type == PhysicsBodyType::Dynamic { body = Some(part.clone()); });
        let body = body.ok_or("no dynamic body in the glb")?;
        {
            let mut body = body.write().unwrap();
            body.settings.physics.linear_velocity = thrower.look * thrower.speed;
            // the throwers stand behind the walls of the level - like in the game the walls only push from the front, the ball flies through from behind
            body.settings.physics.one_sided_meshes = true;
        }
        {
            let mut root = node.write().unwrap();
            root.name = format!("thrown_{}", thrower.id);
            root.settings.transient = true;
            root.settings.collision = true;
            root.tags.insert("revolt_object");
            root.tags.insert("revolt_physics");
            root.extras.insert("revolt_object_type", thrower.kind);
            match root.find_component::<Transformation>()
            {
                Some(transform) =>
                {
                    component_downcast_mut!(transform, Transformation);
                    transform.get_data_mut().get_mut().position = thrower.position;
                    transform.calc_transform();
                },
                None =>
                {
                    let transform = Transformation::new("Transform", thrower.position, Vector3::zeros(), Vector3::new(1.0, 1.0, 1.0));
                    root.add_component(Arc::new(RwLock::new(Box::new(transform))));
                },
            }
        }
        crate::no_self_shadow(&node);
        // the body at once, not with the next scan of the physics (a compound of the glb node's children - add_node takes only own meshes)
        let nodes = scene.nodes.clone();
        scene.physics.scan_nodes(&nodes);
        console_log!("throwers: thrower {} threw '{}' at {:.1} m/s", thrower.id, thrower.source, thrower.speed);
        Ok(node)
    }

    // the thrown objects go
    pub fn stop(&mut self, state: &mut State)
    {
        let Some(scene) = state.find_scene_by_id_mut(self.scene_id) else { return; };
        for node in self.thrown.drain(..)
        {
            let id = node.read().unwrap().id;
            scene.delete_node_by_id(id, true, false, true, false);
        }
    }
}

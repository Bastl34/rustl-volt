// the field of a race: the opponents of the player car, loaded from the converted car glbs at the start, and the start grid
// picked like the game (formats/README.md, Race setup): the rating of the player car and below, random within a rating, no special cars

use std::path::Path;
use std::sync::{Arc, RwLock};

use rustl::prelude::*;
use rustl::serde_json::{self, Value};
use rustl::state::scene::components::component::ComponentItem;
use rustl::state::scene::components::sound::{Sound, SoundType};
use rustl::state::scene::loader::loader::{load_asset, LoaderOptions};
use rustl::state::scene::node::NodeSettings;
use rustl::state::scene::scene_controller::scene_controller::SceneController;
use rustl::state::scene::scene_controller::vehicle_controller::VehicleController;
use rustl::state::state::State;

use crate::cars;
use crate::random::Random;
use crate::track::find_node;

// cars in a race, the player included - the game: 2-12, 8 by default
pub const CARS: usize = 8;
// the controllers of the opponents - removed again when Play stops
pub const BOT_CONTROLLER_NAME: &str = "Re-Volt bot";
pub const BOT_TAG: &str = "revolt_bot";
// the car of the menu when it is not the start car of the level
const PLAYER_CONTROLLER_NAME: &str = "Re-Volt player";

// the stock cars that can be opponents: id, rating (0 rookie .. 4 pro), class (0 electric, 1 glow)
const OPPONENTS: [(&str, u32, u32); 26] =
[
    ("rc", 0, 0), ("mite", 0, 0), ("phat", 0, 0), ("moss", 0, 0), ("mud", 0, 0), ("beatall", 0, 0), ("volken", 0, 0), ("tc6", 0, 0),
    ("dino", 1, 0), ("candy", 1, 0), ("gencar", 1, 0), ("tc4", 1, 1), ("mouse", 1, 0),
    ("flag", 2, 0), ("tc2", 2, 1), ("r5", 2, 1), ("tc5", 2, 1), ("sgt", 2, 0),
    ("tc3", 3, 1), ("adeon", 3, 1), ("fone", 3, 0), ("tc1", 3, 1),
    ("cougar", 4, 1), ("sugo", 4, 1), ("toyeca", 4, 1), ("amw", 4, 1),
];

// the sound components of a car, like the converter puts them on the start car: name, sound, volume
const ENGINE_SOUND: &str = "Engine";
const CAR_SOUNDS: [(&str, &str, f32); 2] = [("Squeal", "skid_normal", 0.6), ("Scrape", "scrape", 0.7)];
// one shots at the car when it hits something (app.rs) - spatial, the ones of the player car are stereo
pub const HIT_SOUNDS: [(&str, &str); 2] = [("Hit", "hit1"), ("Hit Hard", "hit2")];
const ENGINE_VOLUME: f32 = 0.7;
// full volume up to this many m - the cars of the game use range 1.0 (6 m), at 1 m they were barely heard
const SOUND_DISTANCE: f32 = 6.0;

// a slot of the start grid: floor point (m) and heading
#[derive(Clone, Copy)]
pub struct Slot
{
    pub floor: Vector3<f32>,
    pub heading: Vector3<f32>,
}

#[derive(Clone)]
pub struct Opponent
{
    pub node: NodeItem,
    pub car: String,
}

// the grid the converter wrote onto the start car: slot 0 is STARTPOS, then further back
pub fn grid(car: &Node) -> Vec<Slot>
{
    let Some(json) = car.extras.get::<String>("revolt_grid").and_then(|text| serde_json::from_str::<Value>(text).ok()) else { return vec![]; };
    let yaw = json["yaw"].as_f64().unwrap_or(0.0) as f32;
    // the car looks along -z, turned by yaw around the up axis
    let heading = Vector3::new(-yaw.to_radians().sin(), 0.0, -yaw.to_radians().cos());
    json["slots"].as_array().into_iter().flatten().filter_map(|slot|
    {
        let v = |i: usize| slot[i].as_f64().map(|v| v as f32);
        Some(Slot { floor: Vector3::new(v(0)?, v(1)?, v(2)?), heading })
    }).collect()
}

// the stock cars of one rating (0 rookie .. 4 pro) - the candidates of the championship
pub fn cars_of_rating(rating: u32) -> Vec<&'static str>
{
    OPPONENTS.iter().filter(|(_, r, _)| *r == rating).map(|(id, _, _)| *id).collect()
}

// the rating of a car glb (Parameters.txt on the model node), 0 if unknown
pub fn rating(car: &NodeItem) -> u32
{
    let model = find_node(&car.read().unwrap().nodes, &|node| node.name == "model");
    model.and_then(|model| { let model = model.read().unwrap(); model.extras.get::<i64>("revolt_rating").map(|r| *r as f64).or_else(|| model.extras.get::<f64>("revolt_rating").copied()) })
        .map_or(0, |r| r.max(0.0) as u32)
}

// the opponents in the order to try: the rating of the player and below, random within a rating
pub fn pick(player_car: &str, player_rating: u32, random: &mut Random) -> Vec<&'static str>
{
    let mut order = vec![];
    for rating in (0..=player_rating.min(4)).rev()
    {
        let mut bucket: Vec<&'static str> = OPPONENTS.iter().filter(|(id, r, _)| *r == rating && *id != player_car).map(|(id, _, _)| *id).collect();
        // shuffle
        for i in (1..bucket.len()).rev()
        {
            let j = ((random.next() * (i + 1) as f32) as usize).min(i);
            bucket.swap(i, j);
        }
        order.extend(bucket);
    }
    order
}

// loads `count` opponents onto the slots: the next candidate where a glb is missing, the picked ones again if all are used up
pub fn spawn(state: &mut State, scene_id: u32, assets: &str, candidates: &[&str], slots: &[Slot], count: usize) -> Vec<Opponent>
{
    let mut opponents: Vec<Opponent> = vec![];
    let mut tried = 0;
    while opponents.len() < count && tried < candidates.len() + count
    {
        let car = match candidates.get(tried)
        {
            Some(car) => car.to_string(),
            // a rookie with 11 opponents: the picked ones again
            None if !opponents.is_empty() => opponents[(tried - candidates.len()) % opponents.len()].car.clone(),
            None => break,
        };
        tried += 1;

        let Some(slot) = slots.get(opponents.len()).copied() else { break; };
        match spawn_car(state, scene_id, assets, &car, &format!("{car}_bot{}", opponents.len() + 1), slot, None)
        {
            Ok(opponent) => opponents.push(opponent),
            Err(error) => { console_warning!("field: {} - {}", car, error); },
        }
    }
    console_log!("field: {} opponents: {}", opponents.len(), opponents.iter().map(|o| o.car.as_str()).collect::<Vec<_>>().join(", "));
    opponents
}

// the car of the menu instead of the start car: driven by the player with the camera of the start car - split screen: player 1.. with a camera each
pub fn spawn_player(state: &mut State, scene_id: u32, assets: &str, car: &str, player: usize, slot: Slot, cam_name: &str) -> Result<NodeItem, String>
{
    let name = if player == 0 { format!("{car}_player") } else { format!("{car}_player{}", player + 1) };
    spawn_car(state, scene_id, assets, car, &name, slot, Some(cam_name)).map(|opponent| opponent.node)
}

// a car that drives by code without camera (the clockwork cars of the menu) - removed with `remove`
pub fn spawn_extra(state: &mut State, scene_id: u32, assets: &str, car: &str, name: &str, slot: Slot) -> Result<Opponent, String>
{
    spawn_car(state, scene_id, assets, car, name, slot, None)
}

// the car glb as a transient scene object with the sounds and a vehicle controller - a bot without camera, the player with one
fn spawn_car(state: &mut State, scene_id: u32, assets: &str, car: &str, name: &str, slot: Slot, player_camera: Option<&str>) -> Result<Opponent, String>
{
    let path = format!("{assets}/cars/{car}/{car}.glb");
    let options = LoaderOptions
    {
        path: path.clone(),
        extension: Path::new(&path).extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default(),
        parent_node_id: None,
        hide_root_nodes: true,
        reuse_materials: false,
        clear_unused_textures: true,
        object_only: true,
        create_mipmaps: true,
        max_texture_resolution: state.max_texture_resolution(),
        texture_cache: None,
        material_cache: None,
    };
    let mut container = load_asset(&options).map_err(|error| format!("{path}: {error} - convert the car"))?;
    let result = container.apply_to_scene(state, scene_id);

    let model_class = result.node_ids.iter().filter_map(|id| state.find_scene_by_id(scene_id)?.find_node_by_id(*id))
        .find_map(|node| { let node = node.read().unwrap(); (node.name == "model").then(|| node.extras.get::<i64>("revolt_class").copied()).flatten() });
    let class = model_class.map(|class| class as u32).or_else(|| OPPONENTS.iter().find(|(id, _, _)| *id == car).map(|(_, _, class)| *class)).unwrap_or(0);
    let engine = if car.starts_with("wincar") { "clockwrk" } else if class == 1 { "petrol" } else { "moto" };
    let source = |name: &str| state.resources.sound_sources.values().find(|source| source.read().unwrap().name == name).cloned();
    let sounds: Vec<(&str, Option<_>, f32, bool)> = [(ENGINE_SOUND, source(engine), ENGINE_VOLUME, true)].into_iter()
        .chain(CAR_SOUNDS.iter().map(|(component, sound, volume)| (*component, source(sound), *volume, true)))
        .chain(HIT_SOUNDS.iter().map(|(component, sound)| (*component, source(sound), 1.0, false))).collect();

    let scene = state.find_scene_by_id_mut(scene_id).ok_or("scene gone")?;
    let node = result.node_ids.iter().filter_map(|id| scene.find_node_by_id(*id)).find(|node| node.read().unwrap().root_node).ok_or("no root node")?;
    let name = name.to_string();
    let display = find_node(&node.read().unwrap().nodes, &|n| n.name == "model").and_then(|model| model.read().unwrap().extras.get::<String>("revolt_name").cloned()).unwrap_or_else(|| car.to_string());
    {
        let mut node = node.write().unwrap();
        node.name = name.clone();
        // like the start car of the converter - the loader hid the root node, the scene object settings make it visible again
        node.settings = NodeSettings { transient: true, collision: false, camera_collision: false, ..NodeSettings::default() };
        node.tags.insert("revolt_car");
        node.tags.insert(car);
        if player_camera.is_none() { node.tags.insert(BOT_TAG); }
        node.extras.insert("revolt_car", car.to_string());
        node.extras.insert("revolt_name", display.clone());

        let mut transform = Transformation::new("Transform", slot.floor, Vector3::zeros(), Vector3::new(1.0, 1.0, 1.0));
        let yaw = (-slot.heading.x).atan2(-slot.heading.z);
        transform.apply_rotation_quaternion(Vector4::new(0.0, (yaw * 0.5).sin(), 0.0, (yaw * 0.5).cos()), true);
        node.add_component(Arc::new(RwLock::new(Box::new(transform))));

        for (component, source, volume, looped) in sounds
        {
            let Some(source) = source else { console_warning!("field: sound for '{}' not in the scene sound list - re-convert the level", component); continue; };
            let mut sound = Sound::new(component, source, SoundType::Spatial, looped);
            sound.get_data_mut().get_mut().volume = volume;
            sound.get_data_mut().get_mut().spatial_distance_scale = SOUND_DISTANCE;
            let component: ComponentItem = Arc::new(RwLock::new(Box::new(sound)));
            node.add_component(component);
        }
    }

    let mut controller = VehicleController::default();
    controller.get_base_mut().name = if player_camera.is_some() { PLAYER_CONTROLLER_NAME } else { BOT_CONTROLLER_NAME }.to_string();
    controller.camera.enabled = player_camera.is_some();
    if let Some(error) = cars::setup(scene, &mut controller, &node, &name, player_camera.unwrap_or(""))
    {
        scene.delete_node_by_id(node.read().unwrap().id, true, false, true, false);
        return Err(error);
    }
    // on its wheels above the floor - the body is made where the node is
    set_position(&node, slot.floor + Vector3::y() * (cars::ground_height(&controller) + cars::LIFT));
    scene.controller.push(Box::new(controller));
    cars::apply_level_shade(scene, &node);
    cars::hide_pins(&node);
    cars::add_env_map(state, &node, assets);

    Ok(Opponent { node, car: car.to_string() })
}

// a car glb only to look at (the menu): no controller, no collision, transient
pub fn show_car(state: &mut State, scene_id: u32, assets: &str, car: &str) -> Result<NodeItem, String>
{
    let node = show_model(state, scene_id, &format!("{assets}/cars/{car}/{car}.glb"), &format!("{car}_shown")).map_err(|error| format!("{error} - convert the car"))?;
    cars::hide_pins(&node);
    cars::add_env_map(state, &node, assets);
    Ok(node)
}

// any glb to look at: no collision, transient, a transformation on its root
pub fn show_model(state: &mut State, scene_id: u32, path: &str, name: &str) -> Result<NodeItem, String>
{
    let path = path.to_string();
    let options = LoaderOptions
    {
        path: path.clone(),
        extension: "glb".to_string(),
        parent_node_id: None,
        hide_root_nodes: true,
        reuse_materials: false,
        clear_unused_textures: true,
        object_only: true,
        create_mipmaps: true,
        max_texture_resolution: state.max_texture_resolution(),
        texture_cache: None,
        material_cache: None,
    };
    let mut container = load_asset(&options).map_err(|error| format!("{path}: {error}"))?;
    let result = container.apply_to_scene(state, scene_id);
    let scene = state.find_scene_by_id_mut(scene_id).ok_or("scene gone")?;
    let node = result.node_ids.iter().filter_map(|id| scene.find_node_by_id(*id)).find(|node| node.read().unwrap().root_node).ok_or("no root node")?;
    {
        let mut root = node.write().unwrap();
        root.name = name.to_string();
        root.settings = NodeSettings { transient: true, collision: false, camera_collision: false, ..NodeSettings::default() };
    }
    let children = node.read().unwrap().nodes.clone();
    for_each(&children, &mut |child| { let mut child = child.write().unwrap(); child.settings.collision = false; child.settings.camera_collision = false; });
    // the loader gives the root no transformation - the menu moves the car with it
    if node.read().unwrap().find_component::<Transformation>().is_none()
    {
        let transform = Transformation::new("Transform", Vector3::zeros(), Vector3::zeros(), Vector3::new(1.0, 1.0, 1.0));
        node.write().unwrap().add_component(Arc::new(RwLock::new(Box::new(transform))));
    }
    Ok(node)
}

fn for_each(nodes: &Vec<NodeItem>, f: &mut dyn FnMut(&NodeItem))
{
    for node in nodes
    {
        f(node);
        let children = node.read().unwrap().nodes.clone();
        for_each(&children, f);
    }
}

fn set_position(node: &NodeItem, position: Vector3<f32>)
{
    let node = node.read().unwrap();
    if let Some(transform) = node.find_component::<Transformation>()
    {
        component_downcast_mut!(transform, Transformation);
        transform.get_data_mut().get_mut().position = position;
        transform.calc_transform();
    }
}

// the start car of the level steps aside for the car of the menu: its controller out (no body, no camera), hidden
pub fn bench_start_car(scene: &mut Scene, start_car: &NodeItem) -> Option<(Box<dyn SceneController>, String)>
{
    let id = start_car.read().unwrap().id;
    let controllers = std::mem::take(&mut scene.controller);
    let (mut benched, others): (Vec<_>, Vec<_>) = controllers.into_iter().partition(|c|
        c.as_any().downcast_ref::<VehicleController>().and_then(|vehicle| vehicle.node.as_ref()).is_some_and(|node| node.read().unwrap().id == id));
    scene.controller = others;
    let mut controller = benched.pop()?;
    let cam_name = controller.as_any().downcast_ref::<VehicleController>().map(|vehicle| vehicle.cam_name.clone()).unwrap_or_default();
    controller.on_remove(scene);
    start_car.write().unwrap().settings.visible = false;
    Some((controller, cam_name))
}

// back to the race of the start car: its controller makes the body again, the camera follows it again
pub fn unbench_start_car(scene: &mut Scene, start_car: &NodeItem, mut controller: Box<dyn SceneController>, cam_name: &str)
{
    start_car.write().unwrap().settings.visible = true;
    if let Some(vehicle) = controller.as_any_mut().downcast_mut::<VehicleController>()
    {
        if let Some(error) = vehicle.attach_camera(scene, cam_name) { console_warning!("field: start car camera - {}", error); }
    }
    scene.controller.push(controller);
}

// the cars of the menu go again
pub fn remove_players(scene: &mut Scene, players: &[NodeItem])
{
    remove_by_name(scene, PLAYER_CONTROLLER_NAME);
    for player in players
    {
        let id = player.read().unwrap().id;
        scene.delete_node_by_id(id, true, false, true, false);
    }
}

fn remove_by_name(scene: &mut Scene, name: &str)
{
    let controllers = std::mem::take(&mut scene.controller);
    let (mut removed, others): (Vec<_>, Vec<_>) = controllers.into_iter().partition(|c| c.get_base().name == name);
    scene.controller = others;
    for controller in &mut removed
    {
        controller.on_remove(scene);
    }
}

// Play stops: the opponents and their controllers go again
pub fn remove(scene: &mut Scene, opponents: &[Opponent])
{
    let controllers = std::mem::take(&mut scene.controller);
    let (mut bots, others): (Vec<_>, Vec<_>) = controllers.into_iter().partition(|c| c.get_base().name == BOT_CONTROLLER_NAME);
    scene.controller = others;
    for controller in &mut bots
    {
        controller.on_remove(scene);
    }
    for opponent in opponents
    {
        let id = opponent.node.read().unwrap().id;
        scene.delete_node_by_id(id, true, false, true, false);
    }
}

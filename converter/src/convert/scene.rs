// <level>.scene in the project folder: the level glb + the instances, added to the .project if it is not in there yet
// an existing scene keeps everything the editor added - only the objects of the converter (found by uuid) are replaced

use std::path::Path;

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use super::music;
use super::sounds;
use super::start_car::{self, StartCar};
use super::{Log, Settings};

// the gravity of the game: 2200 units/s² (Parameters.txt, Gravity) - 22 m/s² at 0.01 per unit, the world is about twice real size
const GRAVITY_UNITS: f32 = 2200.0;

// physics steps per second - the small cars spring fast at this gravity, mite shook at 60 (simulated)
const PHYSICS_HZ: f64 = 120.0;
// substeps per frame at most - 8 keeps the time down to 15 fps
const MAX_SUBSTEPS: u32 = 8;

// gravity and step of the game - the other physics settings of an existing scene stay, a new block gets the ground plane as asked
pub fn put_gravity(scene: &mut serde_json::Map<String, Value>, scale: f32, ground_plane: bool, log: &Log)
{
    let physics = scene.entry("physics").or_insert_with(|| json!({ "ground_plane": ground_plane, "ground_plane_y": 0.0 }));
    if let Some(physics) = physics.as_object_mut()
    {
        physics.insert("gravity".into(), json!([0.0, -(GRAVITY_UNITS * scale) as f64, 0.0]));
        physics.insert("fixed_timestep".into(), json!(1.0 / PHYSICS_HZ));
        physics.insert("max_substeps".into(), json!(MAX_SUBSTEPS));
        // the cars scrape along the .ncp walls - without, every triangle edge bumped them away
        physics.insert("smooth_mesh_edges".into(), json!(true));
        // bodies at rest only sleep: a frozen (fixed) body stops a car dead in the step it is hit, like a wall
        physics.insert("freeze_resting".into(), json!(false));
        physics.insert("settle_time".into(), json!(0.0));
    }
    log.info(format!("  physics: gravity {:.1} m/s² (2200 units/s² of the game), {PHYSICS_HZ} Hz, up to {MAX_SUBSTEPS} substeps, smooth mesh edges, resting bodies sleep (no freezing)", GRAVITY_UNITS * scale));
}

// project relative with forward slashes - absolute if the file is outside of the project
pub fn relative_path(project_dir: &Path, path: &Path) -> String
{
    path.strip_prefix(project_dir).unwrap_or(path).to_string_lossy().replace('\\', "/")
}

// scene_name: <level> or <level>_reversed - the uuids stay those of the level, they only have to be unique in one scene
pub fn write(level: &str, scene_name: &str, settings: &Settings, level_glb: &Path, collision_glb: Option<&Path>, instances: Vec<Value>, objects: Vec<Value>, sky: Vec<Value>, start_car: Option<StartCar>, level_extras: &Value, track: serde_json::Map<String, Value>, log: &Log) -> Result<()>
{
    let level = level.to_lowercase();
    let scene_name = scene_name.to_lowercase();
    let file_name = format!("{scene_name}.scene");
    let path = settings.project_dir.join(&file_name);
    let uuids = [format!("revolt-{level}-level"), format!("revolt-{level}-instances"), format!("revolt-{level}-objects"), format!("revolt-{level}-skybox"), format!("revolt-{level}-clouds"), format!("revolt-{level}-collision"), start_car::uuid(&level)];

    let level_object = json!({
        "source": relative_path(&settings.project_dir, level_glb),
        "uuid": uuids[0],
        "name": level,
        // the physics uses the .ncp, the camera the visible mesh
        "options": { "settings": { "collision": collision_glb.is_none(), "camera_collision": true } },
        "tags": ["revolt_level"],
        // zones, position nodes, AI nodes of this direction - the glb is shared by both directions
        "extras": track,
        "position": [0.0, 0.0, 0.0],
        "rotation": [0.0, 0.0, 0.0],
        "scale": [1.0, 1.0, 1.0],
    });
    let instance_count = instances.len();
    let object_count = objects.len();
    // the sounds of the 3D sound objects go into the scene sound list
    let ambient_names: Vec<String> = objects.iter()
        .flat_map(|o| o["extras"].as_object().into_iter().flatten().filter(|(key, _)| key.starts_with("revolt_sound")).filter_map(|(_, value)| value.as_str().map(str::to_string)).collect::<Vec<_>>())
        .collect();
    let objects_object = json!({
        "uuid": uuids[2],
        "name": "objects",
        "options": {},
        "position": [0.0, 0.0, 0.0],
        "rotation": [0.0, 0.0, 0.0],
        "scale": [1.0, 1.0, 1.0],
        "objects": objects,
    });
    let instances_object = json!({
        "uuid": uuids[1],
        "name": "instances",
        "options": {},
        "position": [0.0, 0.0, 0.0],
        "rotation": [0.0, 0.0, 0.0],
        "scale": [1.0, 1.0, 1.0],
        "objects": instances,
    });

    let mut scene = match path.exists()
    {
        true =>
        {
            log.read(&path);
            let text = std::fs::read_to_string(&path)?;
            // never overwrite a file we do not understand
            serde_json::from_str::<Value>(&text).with_context(|| format!("{} is no valid scene - not touched", path.display()))?
        },
        false => json!({ "name": scene_name }),
    };
    let Some(scene_object) = scene.as_object_mut() else { bail!("{} is no json object - not touched", path.display()); };

    put_gravity(scene_object, settings.scale, false, log);

    let old_objects = scene_object.remove("objects").and_then(|o| match o { Value::Array(a) => Some(a), _ => None }).unwrap_or_default();
    let ours = |object: &Value| object["uuid"].as_str().is_some_and(|uuid| uuids.iter().any(|u| u == uuid));
    let replaced = old_objects.iter().filter(|o| ours(o)).count();
    let kept: Vec<Value> = old_objects.into_iter().filter(|o| !ours(o)).collect();

    let mut objects = vec![level_object];
    if let Some(collision_glb) = collision_glb
    {
        // shown in the editor by making the object visible
        objects.push(json!({
            "source": relative_path(&settings.project_dir, collision_glb),
            "uuid": uuids[5],
            "name": "collision",
            "options": { "settings": { "visible": false, "pickable": false, "collision": true, "camera_collision": false } },
            "tags": ["revolt_collision"],
            "position": [0.0, 0.0, 0.0],
            "rotation": [0.0, 0.0, 0.0],
            "scale": [1.0, 1.0, 1.0],
        }));
    }
    objects.extend([instances_object, objects_object]);
    music::put_scene_music(scene_object, settings, level_extras, log);

    let car_engine = start_car.as_ref().and_then(|car| car.engine_sound);
    let has_car = start_car.is_some();
    let car_controller = start_car.map(|car| { objects.push(car.object); car.controller });
    let mut sound_names = match has_car
    {
        // the race of the app code: countdown beeps, pickups (levels without .pan never start one)
        // + the engines of the opponents (stock cars: moto / petrol)
        true => sounds::scene_sounds(&car_engine.into_iter().chain(sounds::OPPONENT_ENGINES).collect::<Vec<_>>()).into_iter().chain(["countdown", "pickup", "pickgen"].map(str::to_string)).collect(),
        false => vec![],
    };
    sound_names.extend(ambient_names.iter().cloned());
    if level == super::frontend::LEVEL
    {
        sound_names.extend(super::frontend::SOUNDS.map(str::to_string));
        log.info(format!("  sounds: the menu and the clockwork cars ({})", super::frontend::SOUNDS.join(", ")));
    }
    sound_names.sort();
    sound_names.dedup();
    sounds::put_scene_sounds(scene_object, settings, &sound_names, log);
    if has_car { log.info("  sounds: the loops of the start car and the opponents + skid, scrape, hit1 / hit2, countdown, pickup, pickgen in the scene sound list"); }
    if !ambient_names.is_empty() { log.info(format!("  sounds: {} of the 3D sound objects and moving objects in the scene sound list", ambient_names.iter().collect::<std::collections::BTreeSet<_>>().len())); }
    let sky_names: Vec<String> = sky.iter().filter_map(|o| o["name"].as_str().map(str::to_string)).collect();
    objects.extend(sky);
    let kept_count = kept.len();
    objects.extend(kept);
    scene_object.insert("objects".into(), Value::Array(objects));

    put_car_controller(scene_object, &start_car::uuid(&level), car_controller, log);

    // a camera at the start and a sun - only if the scene has no own camera / directional light
    let camera_uuid = format!("revolt-{level}-camera");
    let light_uuid = format!("revolt-{level}-sun");
    let own_camera = put_converter_entry(scene_object, "cameras", &camera_uuid, start_camera(&camera_uuid, level_extras), |_| true);
    let own_light = put_converter_entry(scene_object, "lights", &light_uuid, sun(&light_uuid), |light| light["light_type"] == "Directional");
    log.info(match own_camera
    {
        true => "  camera: the scene has its own - none added".to_string(),
        false => format!("  camera: fly camera behind the start position ({camera_uuid})"),
    });
    log.info(match own_light
    {
        true => "  light: the scene has its own directional light - none added".to_string(),
        false => format!("  light: directional sun with shadows ({light_uuid})"),
    });

    std::fs::write(&path, serde_json::to_string_pretty(&scene)?)?;
    let collision_text = if collision_glb.is_some() { "level (visible, camera collision) + collision (.ncp, hidden)" } else { "level (collision from the visible mesh)" };
    log.info(format!("  objects: {collision_text} + instances ({instance_count}) + objects ({object_count})"));
    if !sky_names.is_empty() { log.info(format!("  sky: {} (tags revolt_skybox / revolt_clouds)", sky_names.join(", "))); }
    log.info("  tags: revolt_level · revolt_instance + model name · revolt_object + type (pickup, water_ripples, 3d_sound, ...)");
    match replaced
    {
        0 => log.info("  new scene"),
        _ => log.info(format!("  replaced the {replaced} converter objects, kept {kept_count} other objects and everything else (cameras, lights, ...)")),
    }
    log.write(&path);

    match settings.add_to_project
    {
        true => add_to_project(&settings.project_dir, &file_name, log),
        false => { log.info("  not added to the .project (off)"); Ok(()) },
    }
}

// the controller of the start car - one set up and saved in the editor stays, one never set up is written again
fn put_car_controller(scene: &mut serde_json::Map<String, Value>, car_uuid: &str, controller: Option<Value>, log: &Log)
{
    let mut controllers = match scene.remove("controller") { Some(Value::Array(a)) => a, _ => vec![] };
    let of_car = |c: &Value| c["type"] == "VehicleController" && c["node"].as_str() == Some(car_uuid);
    let set_up = controllers.iter().any(|c| of_car(c) && !c["frame"].is_null());

    match (set_up, controller)
    {
        (true, _) => log.info("  vehicle controller: set up in the editor - kept"),
        (false, Some(controller)) =>
        {
            controllers.retain(|c| !of_car(c));
            controllers.push(controller);
        },
        (false, None) => controllers.retain(|c| !of_car(c)),
    }
    if !controllers.is_empty()
    {
        scene.insert("controller".into(), Value::Array(controllers));
    }
}

// replaces the converter entry (by uuid) - adds it only if no other entry matches `counts`; true = the scene has its own one
pub fn put_converter_entry(scene: &mut serde_json::Map<String, Value>, list: &str, uuid: &str, entry: Value, counts: impl Fn(&Value) -> bool) -> bool
{
    let mut entries = match scene.remove(list) { Some(Value::Array(a)) => a, _ => vec![] };
    entries.retain(|e| e["uuid"].as_str() != Some(uuid));
    let own = entries.iter().any(|e| counts(e));
    if !own
    {
        entries.push(entry);
    }
    if !entries.is_empty()
    {
        scene.insert(list.into(), Value::Array(entries));
    }
    own
}

// a fly camera 3 m behind and 1.5 m above the start, looking at it - the start values come from the .inf extras
pub fn start_camera(uuid: &str, level_extras: &Value) -> Value
{
    let get = |key: &str| level_extras[key].as_f64().unwrap_or(0.0) as f32;
    let start = [get("start_pos_x"), get("start_pos_y"), get("start_pos_z")];
    let yaw = get("start_rot_y").to_radians();
    // the car looks along -z at 0 degrees
    let forward = [-yaw.sin(), 0.0, -yaw.cos()];
    let eye = [start[0] - forward[0] * 3.0, start[1] + 1.5, start[2] - forward[2] * 3.0];
    let target = [start[0], start[1] + 0.2, start[2]];
    let dir = [target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]];

    json!({
        "id": 0,
        "uuid": uuid,
        "name": "Start camera",
        "enabled": true,
        "node": null,
        "tags": {},
        "data": {
            "projection_type": "Perspective",
            "viewport": { "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0 },
            "resolution_width": 0,
            "resolution_height": 0,
            "resolution_aspect_ratio": 1.0,
            "fovy": 0.7853982,
            "clipping_near": 0.05,
            "clipping_far": 1000.0,
            "left": -1.0, "right": 1.0, "top": 1.0, "bottom": -1.0,
            "eye_pos": eye,
            "dir": dir,
            "up": [0.0, 1.0, 0.0],
            "culling_mask": 1048575,
            "use_target_node_for_ears": true,
            "left_ear_pos": [-1.0, 0.0, 0.0],
            "right_ear_pos": [1.0, 0.0, 0.0],
        },
        "controller": {
            "type": "FlyController",
            "base": { "is_enabled": true, "name": "Fly Controller", "icon": "✈" },
            "keyboard_movement": true,
            "mouse_movement": true,
            "gamepad_movement": true,
            "move_speed": 0.05,
            "move_speed_shift": 0.2,
            "mouse_sensitivity": [0.0015, 0.0015],
            "gamepad_sensitivity": 0.03,
            "collision": false,
            "collision_distance": 0.5,
            "viewport_only": false,
        },
    })
}

// where the sun light travels - the sun of the cloud sky sits opposite
pub const SUN_DIRECTION: [f32; 3] = [0.35, -1.0, 0.25];

// a directional light from above at an angle, with shadows
pub fn sun(uuid: &str) -> Value
{
    json!({
        "id": 0,
        "uuid": uuid,
        "name": "Sun",
        "enabled": true,
        "tags": {},
        "light_type": "Directional",
        "pos": [0.0, 100.0, 0.0],
        "dir": SUN_DIRECTION,
        "color": [1.0, 0.97, 0.9],
        "ground_color": [0.0, 0.0, 0.0],
        "intensity": 1.0,
        "range": 0.0,
        "max_angle": 0.0,
        "distance_based_intensity": false,
        "cast_shadow": true,
        "shadow_strength": 0.7,
        "shadow_bias": 0.0001,
    })
}

// only if the scene is not listed yet
pub fn add_to_project(project_dir: &Path, scene_file: &str, log: &Log) -> Result<()>
{
    let project_path = std::fs::read_dir(project_dir)?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .find(|path| path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("project")))
        .context("no .project file in the project folder")?;

    log.read(&project_path);
    let text = std::fs::read_to_string(&project_path)?;
    let mut project: Value = serde_json::from_str(&text).with_context(|| format!("{} is no valid project - not touched", project_path.display()))?;
    let Some(scenes) = project.get_mut("scenes").and_then(|s| s.as_array_mut()) else { bail!("{} has no scenes list - not touched", project_path.display()); };

    let normalize = |path: &str| path.replace('\\', "/").trim_start_matches("./").to_lowercase();
    if scenes.iter().any(|scene| scene["path"].as_str().is_some_and(|path| normalize(path) == normalize(scene_file)))
    {
        log.info(format!("  {scene_file} is already in {} - not changed", project_path.file_name().unwrap_or_default().to_string_lossy()));
        return Ok(());
    }

    scenes.push(json!({ "path": scene_file, "active": false }));
    std::fs::write(&project_path, serde_json::to_string_pretty(&project)?)?;
    log.info(format!("  {scene_file} added to the scenes (not active)"));
    log.write(&project_path);
    Ok(())
}

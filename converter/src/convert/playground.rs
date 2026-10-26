// the test scenes (own, not in the game): powerup_test.scene = the markar arena once more (bots driving circles, number keys give a weapon that
// never runs out), physics_test.scene = the grid floor of cars.scene with the start car and the physics objects of every level (their glbs
// stay in their level folders) - driven alone like the stunt arena; the app code knows both by their name

use anyhow::{Context, Result};
use serde_json::{json, Value};

use super::{Log, Settings};

pub const LEVEL: &str = "markar";
pub const SCENE: &str = "powerup_test";
const PHYSICS_SCENE: &str = "physics_test";

// the physics object types, copies of each
const KINDS: [i64; 11] = [57, 58, 59, 66, 67, 7, 1, 15, 43, 52, 36];
const COPIES: usize = 4;
// a grid in front of the car (it faces -z), m - the objects drop from DROP onto the ground plane
const SPACING: f32 = 3.0;
const AHEAD: f32 = 8.0;
const DROP: f32 = 1.0;
// the start car stands this high on the floor, like on the levels
const CAR_LIFT: f32 = 0.05;

pub fn write_tests(settings: &Settings, levels: &[String], log: &Log)
{
    if let Err(err) = write_physics_test(settings, levels, log)
    {
        log.error(format!("{PHYSICS_SCENE}.scene: {err:#}"));
    }
}

fn write_physics_test(settings: &Settings, levels: &[String], log: &Log) -> Result<()>
{
    let read = |scene: &str| -> Option<Value> { serde_json::from_str(&std::fs::read_to_string(settings.project_dir.join(format!("{scene}.scene"))).ok()?).ok() };
    let mut scene = read(SCENE).context("powerup_test.scene missing - convert markar")?;
    let showroom = read("cars").context("cars.scene missing - convert the cars")?;
    scene["name"] = json!(PHYSICS_SCENE);
    // the endless ground plane and the gravity of the showroom
    scene["physics"] = showroom["physics"].clone();

    // the object group of the arena with all its fields, the copies go in instead of its objects
    let mut group = scene["objects"].as_array().and_then(|objects| objects.iter().find(|object| object["name"] == json!("objects"))).cloned().context("no object group in powerup_test.scene")?;

    // only the start car of the arena stays (its controller and camera come with the scene), on the floor of the showroom
    let mut objects: Vec<Value> = scene["objects"].as_array().cloned().unwrap_or_default().into_iter().filter(|object| object["tags"].as_array().is_some_and(|tags| tags.contains(&json!("revolt_start_car")))).collect();
    for car in &mut objects { car["position"] = json!([0.0, CAR_LIFT, 0.0]); }
    objects.extend(showroom["objects"].as_array().into_iter().flatten().filter(|object| object["uuid"] == json!("revolt-cars-floor")).cloned());

    // the first copies of every type, from the levels in their order, a row per type
    let mut placed = vec![];
    for (row, kind) in KINDS.iter().enumerate()
    {
        let mut found: Vec<Value> = vec![];
        for level in levels.iter().map(|level| level.to_lowercase()).filter(|level| level != LEVEL)
        {
            let Some(source) = read(&level) else { continue; };
            let group = source["objects"].as_array().and_then(|objects| objects.iter().find(|object| object["name"] == json!("objects"))).and_then(|objects| objects["objects"].as_array()).cloned().unwrap_or_default();
            found.extend(group.into_iter().filter(|object| object["extras"]["revolt_object_type"] == json!(kind)).take(COPIES - found.len()));
            if found.len() >= COPIES { break; }
        }
        for (column, mut object) in found.into_iter().enumerate()
        {
            let x = (column as f32 - (COPIES - 1) as f32 * 0.5) * SPACING;
            object["position"] = json!([x, DROP, -(AHEAD + row as f32 * SPACING)]);
            object["uuid"] = json!(format!("revolt-{PHYSICS_SCENE}-{kind}-{column}"));
            object["name"] = json!(format!("{}_{row}_{column}", object["name"].as_str().unwrap_or("object")));
            placed.push(object);
        }
    }
    let count = placed.len();
    group["uuid"] = json!(format!("revolt-{PHYSICS_SCENE}-objects"));
    group["objects"] = json!(placed);
    objects.push(group);
    scene["objects"] = json!(objects);

    let path = settings.project_dir.join(format!("{PHYSICS_SCENE}.scene"));
    std::fs::write(&path, serde_json::to_string_pretty(&scene)?)?;
    log.info(format!("  {PHYSICS_SCENE}.scene: the showroom floor, the start car and {count} physics objects of the levels"));
    log.write(&path);
    Ok(())
}

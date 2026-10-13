// the car of the settings on the start position of a level: scene object + vehicle controller
// the controller only names its node - the app code sets it up with the values of Parameters.txt at Play

use serde_json::{json, Value};

use super::geometry::to_gltf;
use super::scene::relative_path;
use super::sounds;
use super::{Log, Settings};
use crate::revolt::find_file;
use crate::revolt::ncp::{Ncp, NCP_CAMERA_ONLY};
use crate::revolt::parameters;

// m above the floor - the car drops onto its wheels at the start
const LIFT: f32 = 0.05;
// the start grid of the app code (own layout): slots behind STARTPOS, staggered left / right - units
const GRID_SLOTS: usize = 12;
const GRID_STEP: f32 = 120.0;
const GRID_SIDE: f32 = 100.0;
// units the floor may lie above STARTPOS (y down) and still count as the floor under it
const FLOOR_ABOVE: f32 = 20.0;

pub struct StartCar
{
    pub object: Value,
    pub controller: Value,
    // the loop of its engine in the scene sound list
    pub engine_sound: Option<&'static str>,
}

pub fn uuid(level: &str) -> String
{
    format!("revolt-{}-car", level.to_lowercase())
}

pub fn build(settings: &Settings, level: &str, level_extras: &Value, ncp: Option<&Ncp>, log: &Log) -> Option<StartCar>
{
    let car = settings.start_car.as_str();
    if car.is_empty()
    {
        log.info("  start car: none set");
        return None;
    }
    let id = car.to_lowercase();
    let params = find_file(&settings.game_dir.join("cars").join(car), "parameters.txt").and_then(|path| parameters::load(&path).ok());
    let Some(params) = params else
    {
        log.warn(format!("  start car: cars/{car}/Parameters.txt missing - no car"));
        return None;
    };
    let name = params.top.text("Name").unwrap_or_default().to_string();
    let engine_sound = sounds::engine_sound(car, params.top.float("Class").map(|c| c as i64));

    let get = |key: &str| level_extras[key].as_f64().map(|v| v as f32);
    let (Some(x), Some(y), Some(z)) = (get("start_pos_x"), get("start_pos_y"), get("start_pos_z")) else
    {
        log.warn("  start car: the .inf has no STARTPOS - no car");
        return None;
    };
    let yaw = get("start_rot_y").unwrap_or(0.0);

    // y down: the lowest wheel point below the origin
    let scale = settings.scale;
    let ground = params.blocks("WHEEL")
        .filter(|wheel| wheel.flag("IsPresent") != Some(false))
        .map(|wheel| (wheel.vec3("Offset1").unwrap_or_default()[1] + wheel.float("Radius").unwrap_or(0.0)) * scale)
        .fold(0.0_f32, f32::max);

    let floor = ncp.and_then(|ncp| floor_below(ncp, [x, y, z], scale));
    let height = match floor
    {
        Some(floor) =>
        {
            log.info(format!("  start car: floor {:.3} m {} STARTPOS", (floor - y).abs(), if floor <= y { "below" } else { "above" }));
            floor
        },
        None =>
        {
            log.warn("  start car: no floor under STARTPOS in the .ncp - placed at STARTPOS");
            y
        },
    };

    // the car looks along -z, the start turns it around the up axis
    let half = yaw.to_radians() * 0.5;
    let glb = settings.out_dir.join("cars").join(&id).join(format!("{id}.glb"));
    if !glb.exists()
    {
        log.warn(format!("  start car: {} is not converted yet - convert the car too", glb.display()));
    }

    let grid = grid(ncp, [x, y, z], yaw, scale, log);
    let uuid = uuid(level);
    let object = json!({
        "source": relative_path(&settings.project_dir, &glb),
        "uuid": uuid,
        "name": id,
        // the vehicle controller builds the chassis from the hull
        "options": { "settings": { "collision": false, "camera_collision": false } },
        "tags": ["revolt_car", id, "revolt_start_car"],
        "extras": { "revolt_car": id, "revolt_name": name, "revolt_grid": grid.to_string() },
        "position": [x, height + ground + LIFT, z],
        "rotation": [0.0, 0.0, 0.0],
        "rotation_quat": [0.0, half.sin(), 0.0, half.cos()],
        "scale": [1.0, 1.0, 1.0],
        // engine, squeal and scrape loops - the app code puts them onto the controller slots
        "components": sounds::car_components(&uuid, engine_sound),
    });

    // everything else takes the defaults of the engine
    let controller = json!({
        "type": "VehicleController",
        "base": { "is_enabled": true, "name": "Vehicle Controller", "phase": "Pre" },
        "node": uuid,
        "node_name": id,
        // empty: the first camera of the scene
        "cam_name": "",
    });

    log.info(format!("  start car: {name} ({id}) at {x:.2} {:.2} {z:.2}, {yaw:.0}° · vehicle controller set up at Play by the app code", height + ground + LIFT));
    Some(StartCar { object, controller, engine_sound })
}

// the floor points of the grid slots (glTF meters, the car origin goes its wheel height above) and the heading
// slot 0 is STARTPOS, then every slot GRID_STEP further back and on the other side
fn grid(ncp: Option<&Ncp>, start: [f32; 3], yaw: f32, scale: f32, log: &Log) -> Value
{
    // glTF: the car looks along -z, turned by yaw around the up axis
    let (sin, cos) = yaw.to_radians().sin_cos();
    let back = [sin, 0.0, cos];
    let right = [cos, 0.0, -sin];

    let mut missing = 0;
    let slots: Vec<[f64; 3]> = (0..GRID_SLOTS).map(|i|
    {
        let side = if i % 2 == 0 { -GRID_SIDE } else { GRID_SIDE } * scale;
        let along = i as f32 * GRID_STEP * scale;
        let p = [0, 1, 2].map(|a| start[a] + back[a] * along + right[a] * side);
        let floor = ncp.and_then(|ncp| floor_below(ncp, p, scale)).unwrap_or_else(|| { missing += 1; p[1] });
        [p[0], floor, p[2]].map(|v| (v as f64 * 1e4).round() / 1e4)
    }).collect();

    log.info(format!("  start grid: {GRID_SLOTS} slots, {:.1} m apart, ±{:.2} m to the side{}", GRID_STEP * scale, GRID_SIDE * scale,
        if missing > 0 { format!(" - {missing} without a floor below (at STARTPOS height)") } else { String::new() }));
    json!({ "yaw": yaw, "slots": slots })
}

// the highest upward floor polygon under the start (glTF meters in and out)
fn floor_below(ncp: &Ncp, start: [f32; 3], scale: f32) -> Option<f32>
{
    // back to the game: y down, units
    let p = to_gltf(start).map(|v| v / scale);

    ncp.polygons.iter()
        .filter(|polygon| !polygon.has(NCP_CAMERA_ONLY))
        // y down: an upward face has a negative y normal
        .filter(|polygon| polygon.plane.normal[1] < -0.5)
        .filter_map(|polygon|
        {
            let n = polygon.plane.normal;
            let y = -(n[0] * p[0] + n[2] * p[2] + polygon.plane.distance) / n[1];
            let point = [p[0], y, p[2]];
            let count = if polygon.has(crate::revolt::ncp::NCP_QUAD) { 4 } else { 3 };
            let inside = polygon.edges[..count].iter().all(|edge| edge.normal.iter().zip(point).map(|(a, b)| a * b).sum::<f32>() + edge.distance <= 0.5);
            (inside && y >= p[1] - FLOOR_ABOVE).then_some(y)
        })
        // the smallest y down is the highest floor
        .min_by(|a, b| a.total_cmp(b))
        .map(|y| -y * scale)
}

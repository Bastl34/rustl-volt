// cars/<car>/Parameters.txt + models + .hul -> assets/cars/<car>/<car>.glb with all parameters as extras
// cars.scene in the project folder shows the converted cars side by side

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::{json, Map, Value};

use super::bodies;
use super::sounds;
use super::geometry::{to_gltf, MeshWriter};
use super::scene::{self, relative_path};
use super::texture::{load_extra_pages, load_page, Page};
use super::{Log, Settings};
use crate::glb::Glb;
use crate::revolt::find_file;
use crate::revolt::hul;
use crate::revolt::mesh::{Mesh, FACE_ENV, FACE_NOENV, FACE_TEXANIM};
use crate::revolt::parameters::{self, Block, Parameters};

// WHEEL 0..3 - names the vehicle controller of the engine recognizes
const WHEEL_NAMES: [&str; 4] = ["wheel_front_left", "wheel_front_right", "wheel_back_left", "wheel_back_right"];
const SIDE_NAMES: [&str; 4] = ["front_left", "front_right", "back_left", "back_right"];

pub struct ConvertedCar
{
    pub id: String,
    pub name: String,
    pub glb: PathBuf,
    // m from the origin down to the lowest wheel
    pub ground: f32,
    // Class of Parameters.txt: 0 electric, 1 glow, 2 other
    pub class: Option<i64>,
}

// a car is a folder in cars/ with a Parameters.txt - (folder, name in the game)
pub fn list(game_dir: &Path) -> Vec<(String, String)>
{
    let Ok(entries) = std::fs::read_dir(game_dir.join("cars")) else { return vec![]; };

    let mut cars: Vec<(String, String)> = entries
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.is_dir())
        .filter_map(|path|
        {
            let file = find_file(&path, "parameters.txt")?;
            let name = parameters::load(&file).ok().and_then(|p| p.top.text("Name").map(str::to_string)).unwrap_or_default();
            Some((path.file_name()?.to_string_lossy().to_string(), name))
        })
        .collect();

    cars.sort_by_key(|(id, _)| id.to_lowercase());
    cars
}

pub fn convert(settings: &Settings, car: &str, log: &Log) -> Result<ConvertedCar>
{
    let id = car.to_lowercase();
    let dir = settings.game_dir.join("cars").join(car);
    let params_path = find_file(&dir, "parameters.txt").context("Parameters.txt missing")?;
    let params = parameters::load(&params_path)?;
    let name = params.top.text("Name").unwrap_or_default().to_string();
    let scale = settings.scale;
    log.heading(format!("car {id}  ·  {name}"));

    log.step("1  Read the car files");
    log.read(&params_path);
    let mut models: BTreeMap<i32, Mesh> = BTreeMap::new();
    for (n, path) in model_list(&params)
    {
        match game_file(&settings.game_dir, &path)
        {
            Some(file) =>
            {
                log.read(&file);
                match Mesh::load(&file)
                {
                    Ok((mesh, lods)) =>
                    {
                        if lods > 1 { log.info(format!("  model {n}: {lods} levels of detail - the first one is used")); }
                        models.insert(n, mesh);
                    },
                    Err(err) => log.warn(format!("  model {n} {path}: {err:#}")),
                }
            },
            None => log.warn(format!("  model {n} {path} missing")),
        }
    }

    log.step("2  Texture (TPAGE --> png in the glb)");
    let mut pages: Vec<Option<Page>> = vec![None];
    match params.top.text("TPAGE").filter(|t| !t.eq_ignore_ascii_case("none"))
    {
        Some(tpage) => match game_file(&settings.game_dir, tpage)
        {
            Some(file) =>
            {
                log.read(&file);
                match load_page(&file)
                {
                    Ok(page) => { log.info(format!("  0  {tpage}  {}×{}{}", page.width, page.height, if page.has_alpha { "  color key" } else { "" })); pages[0] = Some(page); },
                    Err(err) => log.warn(format!("  {tpage}: {err:#}")),
                }
            },
            None => log.warn(format!("  {tpage} missing - untextured")),
        },
        None => log.info("  TPAGE NONE"),
    }
    // the page index in the models is arbitrary (amw 22, cougar 8, ufo 15, ...) - the game draws them with the TPAGE
    if pages[0].is_some()
    {
        let remapped: std::collections::BTreeSet<i16> = models.values().flat_map(|m| &m.polygons).filter(|p| p.texture > 0).map(|p| p.texture).collect();
        for polygon in models.values_mut().flat_map(|m| m.polygons.iter_mut()).filter(|p| p.texture >= 0 && !p.has(FACE_TEXANIM))
        {
            polygon.texture = 0;
        }
        if !remapped.is_empty() { log.info(format!("  pages {remapped:?} in the models --> the TPAGE")); }
    }
    let used: std::collections::BTreeSet<usize> = models.values().flat_map(|m| &m.polygons)
        .filter(|p| !p.has(FACE_TEXANIM)).filter_map(|p| usize::try_from(p.texture).ok()).collect();
    let extra: Vec<usize> = used.iter().copied().filter(|&p| p >= 10).collect();
    load_extra_pages(&settings.game_dir, &dir, "", &extra, &mut pages, log);
    for page in used.iter().filter(|&&p| (1..10).contains(&p))
    {
        log.warn(format!("  page {page} used - a car has only its TPAGE (0), untextured"));
    }

    log.step("3  Parts (Parameters.txt --> nodes at their offsets)");
    let mut glb = Glb::default();
    let mut writer = MeshWriter::new(&pages, &[], settings);
    let mut meshes: BTreeMap<(i32, bool), usize> = BTreeMap::new();
    // env: the body like the game - every polygon but the NOENV ones gets the env map (own material, the wheels share the page)
    let mut mesh_of = |n: i32, env: bool, glb: &mut Glb, writer: &mut MeshWriter| -> Option<usize>
    {
        if let Some(&mesh) = meshes.get(&(n, env)) { return Some(mesh); }
        let model = models.get(&n)?;
        let mut primitives = BTreeMap::new();
        for polygon in &model.polygons
        {
            let mut polygon = polygon.clone();
            if env && !polygon.has(FACE_NOENV) { polygon.flags |= FACE_ENV; }
            writer.add(&mut primitives, model, &polygon, None, -1);
        }
        let mesh = writer.write(glb, &format!("model_{n}"), primitives)?.mesh;
        meshes.insert((n, env), mesh);
        Some(mesh)
    };
    let model_num = |block: &Block, key: &str| block.float(key).map(|n| n as i32).filter(|&n| n >= 0);
    let place = |p: [f32; 3]| to_gltf(p).map(|v| v * scale);

    let mut children = vec![];
    let mut lines = vec![];

    // body
    let body = params.block("BODY", None);
    let body_offset = body.and_then(|b| b.vec3("Offset")).unwrap_or_default();
    let body_mesh = body.and_then(|b| model_num(b, "ModelNum")).or(Some(0)).and_then(|n| mesh_of(n, true, &mut glb, &mut writer));
    // only the hull and the wheels collide - the vehicle controller of the engine takes its chassis from them too
    let mut node = json!({ "name": "body", "translation": place(body_offset), "extras": { "rustl_collision": false } });
    if let Some(mesh) = body_mesh { node["mesh"] = json!(mesh); } else { log.warn("  body: no model"); }
    children.push(glb.node(node));
    lines.push(format!("  body        model {}", if body_mesh.is_some() { "yes" } else { "no" }));

    // wheels at Offset1 - the tread is modelled Offset2 aside
    let mut wheel_centers: BTreeMap<i32, [f32; 3]> = BTreeMap::new();
    let mut ground: f32 = 0.0;
    // wheel masses and cylinder volumes - the car as a rigid body needs mass / volume
    let mut wheel_mass = 0.0;
    let mut wheel_volume = 0.0;
    for wheel in params.blocks("WHEEL")
    {
        let index = wheel.index.unwrap_or(0);
        let offset = wheel.vec3("Offset1").unwrap_or_default();
        wheel_centers.insert(index, offset);
        if wheel.flag("IsPresent") == Some(false)
        {
            lines.push(format!("  wheel {index}     not present"));
            continue;
        }
        let radius = wheel.float("Radius").unwrap_or(0.0);
        // y down: the lowest point of the wheel
        ground = ground.max((offset[1] + radius) * scale);
        wheel_mass += wheel.float("Mass").unwrap_or(0.0);
        let width = model_num(wheel, "ModelNum").and_then(|n| models.get(&n)).map_or(0.0, |m| { let xs = m.vertices.iter().map(|v| v.position[0]); (xs.clone().fold(f32::MIN, f32::max) - xs.fold(f32::MAX, f32::min)) * scale });
        wheel_volume += std::f32::consts::PI * (radius * scale).powi(2) * width;

        let name = WHEEL_NAMES.get(index as usize).map_or(format!("wheel_{index}"), |n| n.to_string());
        let mesh = model_num(wheel, "ModelNum").and_then(|n| mesh_of(n, false, &mut glb, &mut writer));
        let mut extras = block_extras(wheel, "revolt_");
        extras.insert("revolt_wheel".into(), json!(index));
        let mut node = json!({ "name": name, "translation": place(offset), "extras": extras });
        if let Some(mesh) = mesh { node["mesh"] = json!(mesh); }
        children.push(glb.node(node));
        lines.push(format!("  {name:<18} ({:.1}, {:.1}, {:.1}) r {radius} {}{}", offset[0], offset[1], offset[2],
            if wheel.flag("IsPowered") == Some(true) { "powered " } else { "" }, if wheel.flag("IsTurnable") == Some(true) { "turnable" } else { "" }));
    }

    // springs (model along +y) and axles (along +z) point from their offset to the wheel - the game turns them every frame
    for (kind, axis) in [("SPRING", [0.0, 1.0, 0.0]), ("AXLE", [0.0, 0.0, 1.0])]
    {
        for block in params.blocks(kind)
        {
            let index = block.index.unwrap_or(0);
            let offset = block.vec3("Offset").unwrap_or_default();
            let Some(model) = model_num(block, "ModelNum") else { continue; };
            let mesh = mesh_of(model, false, &mut glb, &mut writer);
            let side = SIDE_NAMES.get(index as usize).map_or(index.to_string(), |s| s.to_string());
            let name = format!("{}_{side}", kind.to_lowercase());
            let mut extras = block_extras(block, "revolt_");
            extras.insert("rustl_collision".into(), json!(false));
            let mut node = json!({ "name": name, "translation": place(offset), "extras": extras });
            if let Some(wheel) = wheel_centers.get(&index)
            {
                let direction = to_gltf([0, 1, 2].map(|a| wheel[a] - offset[a]));
                node["rotation"] = json!(rotation_between(to_gltf(axis), direction));
                // stretched from the offset to the wheel center like the game - the app follows the sprung wheel
                let length = block.float("Length").unwrap_or(0.0);
                if length > 0.0
                {
                    let stretch = direction.iter().map(|v| v * v).sum::<f32>().sqrt() / length;
                    node["scale"] = json!(axis.map(|a| if a > 0.0 { stretch } else { 1.0 }));
                    node["extras"]["revolt_length_m"] = json!(length * scale);
                }
            }
            if let Some(mesh) = mesh { node["mesh"] = json!(mesh); }
            children.push(glb.node(node));
        }
    }

    // pins sit on the wheel center - not checked against the game
    for block in params.blocks("PIN")
    {
        let index = block.index.unwrap_or(0);
        let Some(model) = model_num(block, "ModelNum") else { continue; };
        let wheel = wheel_centers.get(&index).copied().unwrap_or_default();
        let offset = block.vec3("Offset").unwrap_or_default();
        let side = SIDE_NAMES.get(index as usize).map_or(index.to_string(), |s| s.to_string());
        let mut extras = block_extras(block, "revolt_");
        extras.insert("rustl_collision".into(), json!(false));
        // the app keeps it at this offset from the sprung wheel
        let pin_offset = to_gltf(offset).map(|v| v * scale);
        for (axis, v) in ["x", "y", "z"].iter().zip(pin_offset) { extras.insert(format!("revolt_pin_offset_{axis}"), json!(v)); }
        let mut node = json!({ "name": format!("pin_{side}"), "translation": place([0, 1, 2].map(|a| wheel[a] + offset[a])), "extras": extras });
        if let Some(mesh) = mesh_of(model, false, &mut glb, &mut writer) { node["mesh"] = json!(mesh); }
        children.push(glb.node(node));
    }

    // spinner (wind-up key of the wincars, radar of sgt, head of panga) - turns around Axis with AngVel in code
    if let Some(spinner) = params.block("SPINNER", None)
    {
        if let Some(model) = model_num(spinner, "ModelNum")
        {
            let axis = to_gltf(spinner.vec3("Axis").unwrap_or([0.0, 1.0, 0.0]));
            let speed = spinner.float("AngVel").unwrap_or(0.0);
            let mut extras = block_extras(spinner, "revolt_");
            // the axis in glTF space, the speed as it is (unit not checked)
            for (name, v) in ["x", "y", "z"].iter().zip(axis) { extras.insert(format!("revolt_spin_axis_{name}"), json!(v as f64)); }
            extras.insert("revolt_spin_speed".into(), json!(speed as f64));
            extras.insert("rustl_collision".into(), json!(false));
            let mut node = json!({ "name": "spinner", "translation": place(spinner.vec3("Offset").unwrap_or_default()), "extras": extras });
            if let Some(mesh) = mesh_of(model, false, &mut glb, &mut writer) { node["mesh"] = json!(mesh); }
            children.push(glb.node(node));
            lines.push(format!("  spinner     model {model}, axis ({:.1}, {:.1}, {:.1}), speed {speed}", axis[0], axis[1], axis[2]));
        }
    }

    // aerial: the section model is 1 unit long along -y, stretched to Length, the top on its end - the game bends it in sections
    if let Some(aerial) = params.block("AERIAL", None)
    {
        let length = aerial.float("Length").unwrap_or(0.0);
        let mut parts = vec![];
        // the aerial uvs are the same on every car page - on some (mite) they hit the black color key, the mask would cut it away
        writer.solid = true;
        if let Some(mesh) = model_num(aerial, "SecModelNum").and_then(|n| mesh_of(n, false, &mut glb, &mut writer))
        {
            parts.push(glb.node(json!({ "name": "aerial_section", "mesh": mesh, "scale": [1.0, length, 1.0], "extras": { "rustl_collision": false } })));
        }
        if let Some(mesh) = model_num(aerial, "TopModelNum").and_then(|n| mesh_of(n, false, &mut glb, &mut writer))
        {
            parts.push(glb.node(json!({ "name": "aerial_top", "mesh": mesh, "translation": [0.0, length * scale, 0.0], "extras": { "rustl_collision": false } })));
        }
        writer.solid = false;
        if !parts.is_empty()
        {
            let direction = to_gltf(aerial.vec3("Direction").unwrap_or([0.0, -1.0, 0.0]));
            children.push(glb.node(json!({
                "name": "aerial",
                "translation": place(aerial.vec3("Offset").unwrap_or_default()),
                "rotation": rotation_between([0.0, 1.0, 0.0], direction),
                "children": parts,
            })));
            lines.push(format!("  aerial      length {length}"));
        }
    }
    lines.iter().for_each(|line| log.info(line.as_str()));

    log.step("4  Collision (.hul --> hidden convex hulls + sphere nodes)");
    let mut sphere_count = 0;
    let mut hull_volume = 0.0;
    match params.top.text("COLL").and_then(|coll| game_file(&settings.game_dir, coll).map(|file| (coll, file)))
    {
        Some((coll, file)) =>
        {
            log.read(&file);
            match hul::load(&file)
            {
                Ok(hull) =>
                {
                    let (mut nodes, volume, skipped) = bodies::hull_nodes(&mut glb, &hull, scale);
                    hull_volume = volume;
                    for (i, sphere) in hull.spheres.iter().enumerate()
                    {
                        let mut extras = bodies::hidden();
                        extras["revolt_sphere_radius"] = json!((sphere.radius * scale) as f64);
                        nodes.push(glb.node(json!({ "name": format!("collision_sphere_{i}"), "translation": place(sphere.center), "extras": extras })));
                    }
                    sphere_count = hull.spheres.len();
                    log.info(format!("  {coll}: {} hulls ({volume:.5} m³), {sphere_count} spheres (empty nodes, radius in the extras)", hull.hulls.len() - skipped));
                    if skipped > 0 { log.warn(format!("  {skipped} hulls with broken values left out")); }
                    children.push(glb.node(json!({ "name": "collision", "children": nodes })));
                },
                Err(err) => log.warn(format!("  {coll}: {err:#} - no collision")),
            }
        },
        None => log.warn("  no COLL file - no collision"),
    }

    log.step("5  Write the car");
    let mut extras = Map::new();
    extras.insert("revolt_car".into(), json!(id));
    extras.insert("revolt_scale".into(), json!(scale as f64));
    extras.insert("revolt_hull_spheres".into(), json!(sphere_count));
    let body_mass = body.and_then(|b| b.float("Mass")).unwrap_or(0.0);
    // CoM is relative to the center of the body bounds, not to the origin - only then it is above the ground for every car
    let body_model = body.and_then(|b| model_num(b, "ModelNum")).unwrap_or(0);
    if let Some(mesh) = models.get(&body_model).filter(|m| !m.vertices.is_empty())
    {
        let (min, max) = bodies::bounds(mesh.vertices.iter().map(|v| v.position));
        for (a, axis) in ["x", "y", "z"].iter().enumerate()
        {
            extras.insert(format!("revolt_body_center_{axis}"), json!(((min[a] + max[a]) / 2.0) as f64));
        }
    }
    extras.insert("revolt_total_mass".into(), json!((body_mass + wheel_mass) as f64));
    // m³ of the colliding parts: hulls + wheel cylinders
    extras.insert("revolt_collision_volume".into(), json!((hull_volume + wheel_volume) as f64));
    for (key, values) in &params.top.entries
    {
        // MODEL n "path" -> revolt_model_n
        if key == "MODEL"
        {
            if let [n, path, ..] = values.as_slice() { extras.insert(format!("revolt_model_{n}"), json!(path.replace('\\', "/"))); }
            continue;
        }
        insert(&mut extras, &format!("revolt_{}", key.to_lowercase()), values);
    }
    for (block, prefix) in [("BODY", "revolt_body_"), ("SPINNER", "revolt_spinner_"), ("AERIAL", "revolt_aerial_"), ("AI", "revolt_ai_")]
    {
        if let Some(block) = params.block(block, None) { extras.extend(block_extras(block, prefix)); }
    }

    // not the car id - the scene object carries that name, a vehicle setup looks the node up by it
    let root = glb.node(json!({ "name": "model", "children": children, "extras": Value::Object(extras.clone()) }));
    let out = settings.out_dir.join("cars").join(&id).join(format!("{id}.glb"));
    glb.save(&out, &[root])?;
    log.info(format!("  {} parts · {} materials · {} extras on the root (parameters in re-volt units, flat: vectors as _x _y _z)", children.len(), writer.material_count(), extras.len()));
    log.write(&out);

    let class = params.top.float("Class").map(|c| c as i64);
    Ok(ConvertedCar { id, name, glb: out, ground, class })
}

// cars.scene: the converted cars in rows - cars of earlier runs stay, the ones of this run are replaced
pub fn write_showroom(settings: &Settings, cars: &[ConvertedCar], log: &Log) -> Result<()>
{
    const PER_ROW: usize = 6;
    const SPACING_X: f32 = 1.6;
    const SPACING_Z: f32 = 2.4;

    log.heading("Cars scene");
    let path = settings.project_dir.join("cars.scene");
    let mut scene = match path.exists()
    {
        true =>
        {
            log.read(&path);
            serde_json::from_str::<Value>(&std::fs::read_to_string(&path)?).with_context(|| format!("{} is no valid scene - not touched", path.display()))?
        },
        false => json!({ "name": "cars" }),
    };
    let scene_object = scene.as_object_mut().context("cars.scene is no json object - not touched")?;

    // the gravity of the game, the cars stand on the endless physics floor of the engine (only set in a new physics block)
    scene::put_gravity(scene_object, settings.scale, true, log);

    let old = match scene_object.remove("objects") { Some(Value::Array(a)) => a, _ => vec![] };
    let ours = |o: &Value| o["uuid"].as_str().is_some_and(|u| u.starts_with("revolt-car-"));
    let mut cars_by_id: BTreeMap<String, Value> = old.iter().filter(|o| ours(o)).filter_map(|o| Some((o["name"].as_str()?.to_string(), o.clone()))).collect();
    let kept: Vec<Value> = old.into_iter().filter(|o| !ours(o)).collect();

    for car in cars
    {
        let uuid = format!("revolt-car-{}", car.id);
        cars_by_id.insert(car.id.clone(), json!({
            "source": relative_path(&settings.project_dir, &car.glb),
            "uuid": format!("revolt-car-{}", car.id),
            "name": car.id,
            "options": { "settings": { "collision": false, "camera_collision": false } },
            "tags": ["revolt_car", car.id],
            "extras": { "revolt_car": car.id, "revolt_name": car.name },
            "position": [0.0, car.ground, 0.0],
            "rotation": [0.0, 0.0, 0.0],
            "scale": [1.0, 1.0, 1.0],
            "components": sounds::car_components(&uuid, sounds::engine_sound(&car.id, car.class)),
        }));
    }

    // the sounds the components of all cars use, also of cars from earlier runs
    let used: Vec<String> = cars_by_id.values()
        .flat_map(|object| object["components"].as_array().cloned().unwrap_or_default())
        .filter_map(|component| component["sound_source"].as_str().and_then(|s| s.strip_prefix("revolt-sound-")).map(str::to_string))
        .collect();
    let used: Vec<&str> = used.iter().map(String::as_str).collect();
    sounds::put_scene_sounds(scene_object, settings, &sounds::scene_sounds(&used), log);

    // rows along x, facing -z
    let count = cars_by_id.len();
    let mut objects = vec![];
    for (i, (_, mut object)) in cars_by_id.into_iter().enumerate()
    {
        let ground = object["position"][1].as_f64().unwrap_or(0.0);
        object["position"] = json!([(i % PER_ROW) as f32 * SPACING_X, ground, (i / PER_ROW) as f32 * SPACING_Z]);
        objects.push(object);
    }
    let kept_count = kept.len();
    objects.extend(kept);
    scene_object.insert("objects".into(), Value::Array(objects));

    // in front of the first row, looking along +z
    let center = (PER_ROW.min(count).max(1) - 1) as f32 * SPACING_X / 2.0;
    let start = json!({ "start_pos_x": center, "start_pos_y": 0.0, "start_pos_z": 0.0, "start_rot_y": 180.0 });
    scene::put_converter_entry(scene_object, "cameras", "revolt-cars-camera", scene::start_camera("revolt-cars-camera", &start), |_| true);
    scene::put_converter_entry(scene_object, "lights", "revolt-cars-sun", scene::sun("revolt-cars-sun"), |light| light["light_type"] == "Directional");

    std::fs::write(&path, serde_json::to_string_pretty(&scene)?)?;
    log.info(format!("  {count} cars in rows of {PER_ROW} (this run: {}), {kept_count} other objects kept · tags revolt_car + id", cars.len()));
    log.write(&path);

    match settings.add_to_project
    {
        true => scene::add_to_project(&settings.project_dir, "cars.scene", log),
        false => Ok(()),
    }
}

fn model_list(params: &Parameters) -> Vec<(i32, String)>
{
    (0..32).filter_map(|n| params.model(n).map(|path| (n, path))).collect()
}

// a path of the game ("cars\Adeon\car.bmp") - every part case-insensitive (the folder is adeon)
fn game_file(game_dir: &Path, path: &str) -> Option<PathBuf>
{
    path.split(['\\', '/']).filter(|part| !part.is_empty()).try_fold(game_dir.to_path_buf(), |dir, part| find_file(&dir, part))
}

// the entries of a block as flat extras: one value as it is, 3 as _x _y _z, more as _0 _1 ...
fn block_extras(block: &Block, prefix: &str) -> Map<String, Value>
{
    let mut extras = Map::new();
    for (key, values) in &block.entries
    {
        insert(&mut extras, &format!("{prefix}{}", key.to_lowercase()), values);
    }
    extras
}

fn insert(extras: &mut Map<String, Value>, key: &str, values: &[String])
{
    match values.len()
    {
        0 => {},
        1 => { extras.insert(key.to_string(), value(&values[0])); },
        3 => for (axis, v) in ["x", "y", "z"].iter().zip(values) { extras.insert(format!("{key}_{axis}"), value(v)); },
        _ => for (i, v) in values.iter().enumerate() { extras.insert(format!("{key}_{i}"), value(v)); },
    }
}

fn value(text: &str) -> Value
{
    if text.eq_ignore_ascii_case("true") { return json!(true); }
    if text.eq_ignore_ascii_case("false") { return json!(false); }
    if let Ok(i) = text.parse::<i64>() { return json!(i); }
    if let Ok(f) = text.parse::<f64>() { return json!(f); }
    json!(text)
}

// the shortest rotation that turns a onto b, [x, y, z, w]
fn rotation_between(a: [f32; 3], b: [f32; 3]) -> [f32; 4]
{
    let normalize = |v: [f32; 3]| { let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt(); if l < 1e-9 { v } else { v.map(|c| c / l) } };
    let (a, b) = (normalize(a), normalize(b));
    let dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    if dot < -0.9999
    {
        // opposite: half a turn around any axis perpendicular to a
        let axis = if a[0].abs() < 0.9 { [0.0, a[2], -a[1]] } else { [-a[2], 0.0, a[0]] };
        let axis = normalize(axis);
        return [axis[0], axis[1], axis[2], 0.0];
    }
    let cross = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let q = [cross[0], cross[1], cross[2], 1.0 + dot];
    let l = q.iter().map(|v| v * v).sum::<f32>().sqrt();
    q.map(|v| v / l)
}

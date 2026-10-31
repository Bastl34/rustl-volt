// level .w -> glb (one node per cell of big cubes, one primitive per material), .ncp -> collision glb, .fin -> model glbs, all into <level>.scene
// <level>/reversed: the same world, the .fin / .taz / .pan / .fan of the reversed folder and STARTPOSREV - into <level>_reversed.scene
// <level>[/reversed]/mirrored: everything mirrored left / right (mirror.rs), own asset folder <level>_mirrored - into <level>[_reversed]_mirrored.scene

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde_json::{json, Value};

use super::geometry::{linear_rgba, to_gltf, MeshWriter};
use super::texture::{load_extra_pages, load_pages};
use super::collision::{self, CollisionStats};
use super::{bodies, clouds, effects, frontend, instances, lights, mirror, objects, playground, scene, skybox, sounds, start_car, track, Log, Settings};
use crate::glb::Glb;
use crate::revolt::{find_file, game_path, MIRRORED, REVERSED};

// the engine makes a node per primitive - per big cube muse2 had 460 for 12k triangles, the cpu paid for every one
const LEVEL_CELL_M: f32 = 80.0;
use crate::revolt::inf::Inf;
use crate::revolt::lit;
use crate::revolt::mesh::*;
use crate::revolt::ncp;
use crate::revolt::rim::{self, MirrorPlane};
use crate::revolt::world::World;

pub fn convert(settings: &Settings, id: &str, log: &Log, progress: &dyn Fn(f32)) -> Result<PathBuf>
{
    let (id, mirrored) = match id.strip_suffix(&format!("/{MIRRORED}")) { Some(id) => (id, true), None => (id, false) };
    let (level, reversed) = match id.strip_suffix(&format!("/{REVERSED}")) { Some(level) => (level, true), None => (id, false) };
    let mut scene_name = if reversed { format!("{level}_{REVERSED}") } else { level.to_string() };
    if mirrored { scene_name += &format!("_{MIRRORED}"); }
    let settings = &Settings { mirrored, ..settings.clone() };
    let dir = game_path(&settings.game_dir, &format!("levels/{level}"));
    // .fin, .taz, .pan, .fan of this direction
    let track_dir = if reversed { game_path(&dir, REVERSED) } else { dir.clone() };
    let inf_path = find_file(&dir, &format!("{level}.inf"));
    let inf = inf_path.as_ref().map(|path| Inf::load(path));
    let variant = match (reversed, mirrored) { (true, true) => " (reversed, mirrored)", (true, false) => " (reversed)", (false, true) => " (mirrored)", _ => "" };
    log.heading(format!("{level}{variant}  ·  {}", inf.as_ref().and_then(|inf| inf.name()).unwrap_or_default()));

    log.step("1  Read the level files");
    match &inf_path { Some(path) => log.read(path), None => log.warn("  no .inf file") }
    let w_path = find_file(&dir, &format!("{level}.w")).context(".w file missing")?;
    log.read(&w_path);
    let mut world = World::load(&w_path)?;
    let rim_path = find_file(&dir, &format!("{level}.rim"));
    let mut mirror_planes = match &rim_path
    {
        Some(path) => { log.read(path); rim::load(path)? },
        None => { log.info("  no .rim file - no mirror planes"); vec![] },
    };
    let ncp_path = find_file(&dir, &format!("{level}.ncp"));
    let mut level_ncp = match &ncp_path
    {
        Some(path) => { log.read(path); Some(ncp::load(path)?) },
        None => { log.warn("  no .ncp file - the level collides with its visible mesh"); None },
    };
    let mut level_instances = instances::load(&dir, &track_dir, level, log)?;
    let mut level_objects = objects::load(&settings.game_dir, &dir, level, log)?;
    let mut level_lights = match find_file(&dir, &format!("{level}.lit"))
    {
        Some(path) => { log.read(&path); lit::load(&path).unwrap_or_else(|err| { log.warn(format!("  {}: {err:#} - no lights", path.display())); vec![] }) },
        None => vec![],
    };
    shade_models(inf.as_ref(), &mut level_instances, &mut level_objects, log);
    if mirrored
    {
        log.step("1b  Mirror (left / right like the game's mirror mode)");
        mirror::world(&mut world);
        mirror::mirror_planes(&mut mirror_planes);
        if let Some(level_ncp) = level_ncp.as_mut() { mirror::ncp(level_ncp); }
        mirror::instances(&mut level_instances);
        mirror::objects(&mut level_objects);
        mirror::lights(&mut level_lights);
        let polygons = world.meshes.iter().map(|m| &m.mesh).chain(level_instances.models.iter().map(|m| &m.mesh)).chain(level_objects.models.iter().map(|m| &m.mesh)).flat_map(|mesh| &mesh.polygons);
        mirror::tex_anims(&mut world.tex_anims, polygons, log);
        log.info(format!("  world, collision, {} instances, {} objects, {} lights, {} mirror planes, start positions and track data mirrored", level_instances.instances.len(), level_objects.objects.len(), level_lights.len(), mirror_planes.len()));
    }

    progress(0.05);
    log.step("2  World (.w)");
    log_world(&world, settings.scale, log);

    log.step("2b  Lights (.lit --> added to the vertex colors like the game does at load)");
    lights::bake(&mut world, &mut level_instances, &mut level_objects, &level_lights, log);

    log.step("3  Textures (.bmp --> png in the glbs)");
    let polygon_pages = world.meshes.iter().flat_map(|m| &m.mesh.polygons).filter(|p| !p.has(FACE_TEXANIM)).map(|p| p.texture as i32);
    let mut polygons_per_page: Vec<usize> = vec![];
    // a level has the pages a..j - higher indices (fx pages) are not in the level folder
    let all_pages = polygon_pages.chain(level_instances.texture_pages()).chain(level_objects.texture_pages());
    for page in all_pages.filter_map(|page| usize::try_from(page).ok()).filter(|&page| page < 10)
    {
        if polygons_per_page.len() <= page { polygons_per_page.resize(page + 1, 0); }
        polygons_per_page[page] += 1;
    }
    if let Some(max) = world.tex_anims.iter().flatten().filter_map(|frame| usize::try_from(frame.texture).ok()).max()
    {
        if polygons_per_page.len() <= max { polygons_per_page.resize(max + 1, 0); }
    }
    let mut pages = load_pages(&dir, level, &polygons_per_page, log, &|f| progress(0.08 + 0.17 * f));
    let extra_pages: std::collections::BTreeSet<usize> = level_instances.texture_pages().chain(level_objects.texture_pages())
        .filter_map(|page| usize::try_from(page).ok()).filter(|&page| page >= 10).collect();
    load_extra_pages(&settings.game_dir, &dir, level, &extra_pages.into_iter().collect::<Vec<_>>(), &mut pages, log);
    if frontend::is_frontend(level) { frontend::replace_wood_logo(&mut pages, &settings.files_dir, log); }

    let mut glb = Glb::default();
    let mut writer = MeshWriter::new(&pages, &world.tex_anims, settings);
    let mut node_lines = vec![];

    // the env colors are in file order - the start index of every mesh
    let env_starts: Vec<usize> = world.meshes.iter()
        .scan(0, |count, m| { let start = *count; *count += m.mesh.polygons.iter().filter(|p| p.has(FACE_ENV)).count(); Some(start) })
        .collect();

    // polygons per mirror plane, the last entry: mirror polygons on no plane
    let mut mirror_counts = vec![0usize; mirror_planes.len() + 1];

    let mut children = vec![];
    let mut triangles = 0;

    let groups = group_meshes(&world, settings.scale);
    let group_count = groups.len();
    for (g, (group_name, mesh_ids)) in groups.into_iter().enumerate()
    {
        progress(0.25 + 0.4 * g as f32 / group_count as f32);
        let mut primitives = BTreeMap::new();

        for &mesh_id in &mesh_ids
        {
            let mesh = &world.meshes[mesh_id].mesh;
            let mut env_index = env_starts[mesh_id];

            for polygon in &mesh.polygons
            {
                let env_color = polygon.has(FACE_ENV).then(|| { env_index += 1; world.env_colors[env_index - 1] });
                let mirror_plane = if polygon.has(FACE_MIRROR) { mirror_plane_of(mesh, polygon, &mirror_planes) } else { -1 };
                if polygon.has(FACE_MIRROR) && !polygon.has(FACE_SKIP)
                {
                    mirror_counts[usize::try_from(mirror_plane).unwrap_or(mirror_planes.len())] += 1;
                }
                writer.add(&mut primitives, mesh, polygon, env_color, mirror_plane);
            }
        }

        if let Some(written) = writer.write(&mut glb, &group_name, primitives)
        {
            node_lines.push(format!("  {group_name:<14} {:>4} meshes {:>3} primitives {:>6} vertices {:>6} triangles",
                mesh_ids.len(), written.primitives, written.vertices, written.triangles));
            triangles += written.triangles;
            children.push(glb.node(json!({ "name": group_name, "mesh": written.mesh })));
        }
    }

    progress(0.65);
    log.step("4  Materials");
    writer.log_materials(log);

    log.step(format!("5  Nodes (big cubes in {LEVEL_CELL_M} m cells)"));
    node_lines.iter().for_each(|line| log.info(line.as_str()));

    log.step("6  Mirror planes (.rim --> empty nodes with extras)");
    if let Some(node) = mirror_nodes(&mut glb, &mirror_planes, &mirror_counts, settings.scale, log)
    {
        children.push(node);
    }

    log.step("7  Level info (.inf --> glb extras on the root node)");
    let mut extras = level_extras(level, settings.scale, inf.as_ref(), log);
    if mirrored { mirror::level_extras(&mut extras); }
    // the lights the game keeps while racing - they light the cars (app car_light.rs)
    extras["revolt_lights"] = json!(lights::runtime_json(&level_lights, settings.scale).to_string());
    log.info(format!("  {} of {} lights kept for the cars (extras revolt_lights)", lights::runtime(&level_lights).len(), level_lights.len()));
    // start car and camera of the reversed scene: STARTPOSREV / STARTROTREV
    let start_extras = if reversed { reversed_start(&extras, log) } else { extras.clone() };

    log.step(format!("8  Track ({}.taz zones, .pan position nodes, .fan AI nodes, .tri signs --> JSON strings on the level object of the scene)", if reversed { "reversed/" } else { "" }));
    let track = track::extras(&track_dir, level, settings.scale, mirrored, log);

    log.step("9  Write the level");
    let root = glb.node(json!({ "name": level.to_lowercase(), "children": children, "extras": extras.clone() }));
    let out = settings.level_dir(level).join(format!("{}.glb", level.to_lowercase()));
    glb.save(&out, &[root])?;
    progress(0.7);
    log.info(format!("  {} nodes · {} materials · {} textures · {triangles} triangles", children.len(), writer.material_count(), writer.texture_count()));
    log.write(&out);

    log.step("10  Instances (.fin + .prm --> one glb per model)");
    let instance_objects = instances::write(&level_instances, level, &pages, &world.tex_anims, settings, log, &|f| progress(0.7 + 0.15 * f))?;

    progress(0.85);
    log.step("11  Game objects (.fob --> one glb per model, markers for the rest)");
    let mut game_objects = objects::write(&level_objects, level, &pages, &world.tex_anims, settings, log, &|f| progress(0.85 + 0.1 * f))?;
    // the lasers: their length against the level like the game (one scene object per .fob object, same order)
    for (object, scene_object) in level_objects.objects.iter().zip(game_objects.iter_mut()).filter(|(object, _)| object.kind == effects::LASER)
    {
        let length = effects::laser_length(object, level_ncp.as_ref(), settings.scale);
        scene_object["extras"]["revolt_laser_length"] = json!(length as f64);
        log.info(format!("  {}: beam {length:.2} m", scene_object["name"].as_str().unwrap_or("laser")));
    }
    // the piano of toylite (.tri type 0): its keyboard and a light per key
    match effects::piano(&track_dir, level, sounds::level_sound(level, effects::PIANO_SOUND), settings, log)
    {
        Ok(piano) => game_objects.extend(piano),
        Err(error) => log.error(format!("  piano: {error:#}")),
    }

    progress(0.95);
    log.step("12  Sky (sky_*.bmp or Clouds object --> box around the level)");
    let (center, half_size) = sky_box_size(&world, settings.scale);
    let sky_object = match skybox::sky_folder(&settings.game_dir, &dir, &level_objects.objects)
    {
        Some(folder) => Some(skybox::write(folder, level, center, half_size, settings, log)?),
        None => Some(clouds::write_sky(&level_objects.objects, level, center, half_size, &extras, settings, log)?),
    };

    log.step("13  Collision (.ncp --> one invisible mesh per surface material)");
    let collision_glb = match &level_ncp
    {
        Some(level_ncp) => Some(write_collision(level_ncp, level, settings, log)?),
        None => { log.info("  none"); None },
    };

    log.step("14  Start car (STARTPOS of the .inf, on the .ncp floor)");
    // the frontend is the menu - no car
    let car = match level.eq_ignore_ascii_case(frontend::LEVEL)
    {
        true => { log.info("  none in the frontend (the menu)"); None },
        false => start_car::build(settings, level, &start_extras, level_ncp.as_ref(), log),
    };

    // the arena once more for the power up test (own)
    let playground_parts = (level.eq_ignore_ascii_case(playground::LEVEL) && !reversed && !mirrored).then(|| (instance_objects.clone(), game_objects.clone(), sky_object.clone(), car.clone(), track.clone()));

    log.step("15  Scene (.scene in the project folder)");
    scene::write(level, &scene_name, settings, &out, collision_glb.as_deref(), instance_objects, game_objects, sky_object.into_iter().collect(), car, &start_extras, track, log)?;

    if let Some((instances, objects, sky, car, track)) = playground_parts
    {
        log.step(format!("16  Power up test ({}.scene: this arena once more)", playground::SCENE));
        scene::write(level, playground::SCENE, settings, &out, collision_glb.as_deref(), instances, objects, sky.into_iter().collect(), car, &start_extras, track, log)?;
    }
    Ok(out)
}

// <level>_collision.glb next to the level glb - the nodes are visible, the scene object hides them
fn write_collision(level_ncp: &ncp::Ncp, level: &str, settings: &Settings, log: &Log) -> Result<PathBuf>
{
    collision::log_ncp(level_ncp, log);

    let mut glb = Glb::default();
    let mut stats = CollisionStats::default();
    let children = collision::add_nodes(&mut glb, level_ncp, settings.scale, false, &mut stats);
    let root = glb.node(json!({ "name": format!("{}_collision", level.to_lowercase()), "children": children }));

    let out = settings.level_dir(level).join(format!("{}_collision.glb", level.to_lowercase()));
    glb.save(&out, &[root])?;
    stats.log(log);
    log.info(format!("  {} nodes (collision_<surface>) · friction = grip × 0.7 (rustl_friction) · surface values in the extras", children.len()));
    log.write(&out);
    Ok(out)
}

// center of the level and half the size of a sky box clearly around it (meters, glTF) - within the 1000 m of the camera
fn sky_box_size(world: &World, scale: f32) -> ([f32; 3], f32)
{
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for vertex in world.meshes.iter().flat_map(|m| &m.mesh.vertices)
    {
        let p = to_gltf(vertex.position).map(|v| v * scale);
        for a in 0..3
        {
            min[a] = min[a].min(p[a]);
            max[a] = max[a].max(p[a]);
        }
    }
    let center = [0, 1, 2].map(|a| (min[a] + max[a]) / 2.0);
    let extent = (0..3).map(|a| max[a] - min[a]).fold(0.0, f32::max);
    (center, (extent * 0.75).clamp(100.0, 450.0))
}

// one empty node per mirror plane under "mirror_planes" - position = center, extras = plane, bounds and corners in level space
fn mirror_nodes(glb: &mut Glb, planes: &[MirrorPlane], counts: &[usize], scale: f32, log: &Log) -> Option<usize>
{
    let unassigned = counts[planes.len()];
    if unassigned > 0
    {
        log.warn(format!("  {unassigned} polygons with the MIRROR flag lie on no mirror plane - material ends in _mirror without a number"));
    }
    if planes.is_empty()
    {
        log.info("  none");
        return None;
    }

    let mut nodes = vec![];
    for (i, plane) in planes.iter().enumerate()
    {
        let corners = plane.corners.map(|c| to_gltf(c).map(|v| v * scale));
        let normal = to_gltf(plane.normal);
        let distance = plane.distance * scale;
        let mut min = [f32::MAX; 3];
        let mut max = [f32::MIN; 3];
        for corner in &corners
        {
            for a in 0..3
            {
                min[a] = min[a].min(corner[a]);
                max[a] = max[a].max(corner[a]);
            }
        }
        let center = [0, 1, 2].map(|a| (min[a] + max[a]) / 2.0);

        let mut extras = serde_json::Map::new();
        extras.insert("revolt_mirror_plane_node".into(), json!(true));
        extras.insert("revolt_mirror_plane".into(), json!(i));
        extras.insert("mirror_flag".into(), json!(plane.flag));
        extras.insert("mirror_plane_d".into(), json!(distance));
        extras.insert("mirror_polygons".into(), json!(counts[i]));
        for (a, axis) in ["x", "y", "z"].iter().enumerate()
        {
            extras.insert(format!("mirror_normal_{axis}"), json!(normal[a]));
            extras.insert(format!("mirror_min_{axis}"), json!(min[a]));
            extras.insert(format!("mirror_max_{axis}"), json!(max[a]));
            for (c, corner) in corners.iter().enumerate()
            {
                extras.insert(format!("mirror_corner_{c}_{axis}"), json!(corner[a]));
            }
        }

        let name = format!("mirror_{i}");
        let [nx, ny, nz] = plane.normal;
        log.info(format!("  {name:<10} normal ({nx}, {ny}, {nz}) d {:<10} --> normal ({:.2}, {:.2}, {:.2}) d {distance:.4}",
            plane.distance, normal[0], normal[1], normal[2]));
        log.info(format!("  {:<10} size {:.2} × {:.2} m at ({:.2}, {:.2}, {:.2}), flag {}, {} mirror polygons",
            "", max[0] - min[0], max[2] - min[2], center[0], center[1], center[2], plane.flag, counts[i]));
        nodes.push(glb.node(json!({ "name": name, "translation": center, "extras": extras })));
    }

    log.info(format!("  {} mirror planes · the reflective polygons are the materials ending in _mirror<plane>", nodes.len()));
    Some(glb.node(json!({ "name": "mirror_planes", "children": nodes })))
}

// the plane whose rectangle contains the polygon center (re-volt coordinates), the closest one if several
fn mirror_plane_of(mesh: &Mesh, polygon: &Polygon, planes: &[MirrorPlane]) -> i16
{
    const TOLERANCE: f32 = 2.0;

    let corners = &polygon.indices[..polygon.corners()];
    let Some(points) = corners.iter().map(|&i| mesh.vertices.get(i as usize).map(|v| v.position)).collect::<Option<Vec<_>>>() else { return -1; };
    let center = [0, 1, 2].map(|a| points.iter().map(|p| p[a]).sum::<f32>() / points.len() as f32);

    planes.iter().enumerate()
        .filter(|(_, plane)| (0..3).all(|a|
        {
            let min = plane.corners.iter().map(|c| c[a]).fold(f32::MAX, f32::min);
            let max = plane.corners.iter().map(|c| c[a]).fold(f32::MIN, f32::max);
            center[a] >= min - TOLERANCE && center[a] <= max + TOLERANCE
        }))
        .map(|(i, plane)| (i, (plane.normal.iter().zip(center).map(|(n, c)| n * c).sum::<f32>() + plane.distance).abs()))
        .filter(|&(_, distance)| distance <= TOLERANCE)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map_or(-1, |(i, _)| i as i16)
}

// root node extras from the .inf, converted: meters, glTF coordinates, degrees, linear colors
// flat values only - the engine reads no arrays or objects from extras
fn level_extras(level: &str, scale: f32, inf: Option<&Inf>, log: &Log) -> Value
{
    let scale = scale as f64;
    let mut extras = serde_json::Map::new();
    extras.insert("revolt_level".into(), json!(level.to_lowercase()));
    extras.insert("revolt_scale".into(), json!(scale));

    for (key, value) in inf.iter().flat_map(|inf| inf.entries())
    {
        let mut converted: Vec<(String, Value)> = vec![];
        let mut put = |key: &str, value: Value| converted.push((key.to_string(), value));
        let mut kept = false;
        // bot_bat writes STARTPOS with commas
        let n: Vec<f64> = value.split(|c: char| c.is_whitespace() || c == ',').filter_map(|v| v.parse().ok()).collect();
        let position = |prefix: &str, put: &mut dyn FnMut(&str, Value)|
        {
            let p = to_gltf([n[0] as f32, n[1] as f32, n[2] as f32]);
            for (axis, v) in ["x", "y", "z"].iter().zip(p)
            {
                put(&format!("{prefix}_{axis}"), json!(v as f64 * scale));
            }
        };

        match (key.as_str(), n.len())
        {
            ("NAME", _) => put("name", json!(value.trim_matches('\''))),
            ("STARTPOS", 3) => position("start_pos", &mut put),
            ("STARTPOSREV", 3) => position("start_pos_rev", &mut put),
            // 0..1 turns around the down axis -> degrees around the up axis
            ("STARTROT", 1) => put("start_rot_y", json!(-n[0] * 360.0)),
            ("STARTROTREV", 1) => put("start_rot_rev_y", json!(-n[0] * 360.0)),
            ("FARCLIP", 1) =>
            {
                put("far_clip", json!(n[0] * scale));
                // exponential fog of the engine at 95 % on the far clip
                put("fog_density", json!(3.0 / (n[0] * scale)));
            },
            ("FOGSTART", 1) => put("fog_start", json!(n[0] * scale)),
            ("FOGCOLOR", 3) =>
            {
                let color = linear_rgba(u32::from_be_bytes([0, n[0] as u8, n[1] as u8, n[2] as u8]), Some(1.0));
                for (channel, v) in ["r", "g", "b"].iter().zip(color) { put(&format!("fog_color_{channel}"), json!(v)); }
            },
            // heights: y down -> y up
            ("VERTFOGSTART", 1) => put("vertical_fog_start", json!(-n[0] * scale)),
            ("VERTFOGEND", 1) => put("vertical_fog_end", json!(-n[0] * scale)),
            ("WORLDRGBPER", 1) => put("world_rgb", json!(n[0] / 100.0)),
            ("MODELRGBPER", 1) => put("model_rgb", json!(n[0] / 100.0)),
            ("INSTANCERGBPER", 1) => put("instance_rgb", json!(n[0] / 100.0)),
            ("STARTGRID", 1) => put("start_grid", json!(n[0] as i64)),
            ("STARTGRIDREV", 1) => put("start_grid_rev", json!(n[0] as i64)),
            // 'gfx\envfront.bmp' -> gfx/envfront.png, relative to the assets folder
            ("ENVSTILL", _) => put("env_map", json!(value.trim_matches('\'').replace('\\', "/").to_lowercase().replace(".bmp", ".png"))),
            // type, mix, intensity, distance
            ("MIRRORS", 4) =>
            {
                put("mirror_type", json!(n[0] as i64));
                put("mirror_mix", json!(n[1]));
                put("mirror_intensity", json!(n[2]));
                put("mirror_distance", json!(n[3] * scale));
            },
            // cd audio track range
            ("REDBOOK", 2) =>
            {
                put("music_track_start", json!(n[0] as i64));
                put("music_track_end", json!(n[1] as i64));
            },
            ("MP3", _) => put("music_file", json!(value.trim_matches('\''))),
            _ =>
            {
                kept = true;
                put(&format!("inf_{}", key.to_lowercase()), json!(value.trim_matches('\'')));
            },
        }

        let text = converted.iter().map(|(k, v)| format!("{k} {}", show(v))).collect::<Vec<_>>().join(", ");
        let line = format!("  {key:<14} {value:<22} --> {text}");
        if kept { log.warn(format!("{line}   (unknown, not converted)")); } else { log.info(line); }
        extras.extend(converted);
    }

    if !extras.contains_key("env_map")
    {
        log.info(format!("  {:<14} {:<22} --> env_map \"gfx/envstill.png\"", "(no ENVSTILL)", ""));
        extras.insert("env_map".into(), json!("gfx/envstill.png"));
    }
    // the game plays no music without REDBOOK (only the Market Arena) - the range of the supermarket tracks
    if !extras.contains_key("music_track_start")
    {
        log.info(format!("  {:<14} {:<22} --> music_track_start 4, music_track_end 9", "(no REDBOOK)", ""));
        extras.insert("music_track_start".into(), json!(4));
        extras.insert("music_track_end".into(), json!(9));
    }
    log.info(format!("  {} values on the root node", extras.len()));
    Value::Object(extras)
}

// the start values of the reversed direction take the place of the forward ones
fn reversed_start(extras: &Value, log: &Log) -> Value
{
    let mut start = extras.clone();
    let Some(map) = start.as_object_mut() else { return start; };
    for (forward, backward) in [("start_pos_x", "start_pos_rev_x"), ("start_pos_y", "start_pos_rev_y"), ("start_pos_z", "start_pos_rev_z"), ("start_rot_y", "start_rot_rev_y"), ("start_grid", "start_grid_rev")]
    {
        match extras.get(backward)
        {
            Some(value) => { map.insert(forward.into(), value.clone()); },
            None => log.warn(format!("  reversed: no {backward} in the .inf - the forward {forward} is used")),
        }
    }
    log.info("  reversed: start car and camera on STARTPOSREV / STARTROTREV");
    start
}

fn show(value: &Value) -> String
{
    match value.as_f64()
    {
        Some(number) => format!("{number:.4}"),
        None => value.to_string(),
    }
}

// everything the .w file contains, before the conversion
fn log_world(world: &World, scale: f32, log: &Log)
{
    let polygons: Vec<&Polygon> = world.meshes.iter().flat_map(|m| &m.mesh.polygons).collect();
    let quads = polygons.iter().filter(|p| p.has(FACE_QUAD)).count();
    let vertices: usize = world.meshes.iter().map(|m| m.mesh.vertices.len()).sum();
    let untextured = polygons.iter().filter(|p| p.texture < 0).count();

    log.info(format!("  .w        {} meshes · {} polygons ({quads} quads, {} triangles) · {vertices} vertices",
        world.meshes.len(), polygons.len(), polygons.len() - quads));
    log.info(format!("  .w        {} big cubes · {} env colors · {untextured} untextured polygons", world.big_cubes.len(), world.env_colors.len()));

    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for vertex in world.meshes.iter().flat_map(|m| &m.mesh.vertices)
    {
        for a in 0..3
        {
            min[a] = min[a].min(vertex.position[a]);
            max[a] = max[a].max(vertex.position[a]);
        }
    }
    let size = [0, 1, 2].map(|a| (max[a] - min[a]) * scale);
    log.info(format!("  size      {:.1} × {:.1} × {:.1} m (x × y × z)", size[0], size[1], size[2]));

    let flags: Vec<String> = FLAG_NAMES.iter()
        .map(|&(flag, name)| (name, polygons.iter().filter(|p| p.has(flag)).count()))
        .filter(|&(_, count)| count > 0)
        .map(|(name, count)| format!("{name} {count}"))
        .collect();
    log.info(format!("  flags     {}", flags.join(" · ")));

    for (i, frames) in world.tex_anims.iter().enumerate()
    {
        let delay: f32 = frames.iter().map(|frame| frame.delay).sum();
        let mut pages: Vec<i32> = frames.iter().map(|frame| frame.texture).collect();
        pages.sort();
        pages.dedup();
        log.info(format!("  texanim   {i}: {} frames, total delay {delay:.2}, pages {pages:?}", frames.len()));
    }
}

// big cubes merged into square cells on the ground (by their center), meshes in no cube go into "rest"
fn group_meshes(world: &World, scale: f32) -> Vec<(String, Vec<usize>)>
{
    let mut used = vec![false; world.meshes.len()];
    let cell = LEVEL_CELL_M / scale;
    let mut cells: BTreeMap<(i32, i32), Vec<usize>> = BTreeMap::new();

    for cube in &world.big_cubes
    {
        let key = ((cube.center[0] / cell).floor() as i32, (cube.center[2] / cell).floor() as i32);
        let ids = cells.entry(key).or_default();
        for &id in &cube.meshes
        {
            if id < used.len() && !used[id]
            {
                used[id] = true;
                ids.push(id);
            }
        }
    }

    let mut groups: Vec<(String, Vec<usize>)> = cells.into_values().enumerate().map(|(i, ids)| (format!("cell_{i:03}"), ids)).collect();
    let rest: Vec<usize> = (0..used.len()).filter(|&id| !used[id]).collect();
    groups.push(("rest".to_string(), rest));
    groups
}


// INSTANCERGBPER / MODELRGBPER of the .inf: the game scales the corner colors of every model it loads (ship2 50 %) - objects only for the physics ones here
fn shade_models(inf: Option<&Inf>, instances: &mut instances::LevelInstances, objects: &mut objects::LevelObjects, log: &Log)
{
    let percent = |key: &str| inf.and_then(|inf| inf.get(key)).and_then(|v| v.split_whitespace().next()?.parse::<f64>().ok()).map(|v| v as i64).filter(|v| *v != 100);
    if let Some(per) = percent("INSTANCERGBPER")
    {
        for model in &mut instances.models { scale_colors(&mut model.mesh, per); }
        log.info(format!("  instance colors × {per} % (INSTANCERGBPER), {} models", instances.models.len()));
    }
    if let Some(per) = percent("MODELRGBPER")
    {
        let mut models: Vec<usize> = objects.objects.iter().zip(&objects.model_of).filter(|(object, _)| bodies::body(object.kind).is_some()).filter_map(|(_, m)| *m).collect();
        models.sort();
        models.dedup();
        for m in &models { scale_colors(&mut objects.models[*m].mesh, per); }
        log.info(format!("  physics object colors × {per} % (MODELRGBPER), {} models - the cars get it in the game", models.len()));
    }
}

// per channel c * per / 100 like the game, alpha kept (0xAARRGGBB)
fn scale_colors(mesh: &mut Mesh, per: i64)
{
    for polygon in &mut mesh.polygons
    {
        for color in &mut polygon.colors
        {
            let channel = |shift: u32| ((((*color >> shift) & 0xff) as i64 * per / 100).clamp(0, 255) as u32) << shift;
            *color = (*color & 0xff00_0000) | channel(16) | channel(8) | channel(0);
        }
    }
}

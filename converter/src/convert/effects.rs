// effects the game draws in code: the laser beams of the museum (.fob type 33) and the lit keys of the toylite piano (.tri type 0)
// formats/README.md "Laser" and "Piano" - the app code moves and lights them

use std::io::Cursor;
use std::path::Path;

use anyhow::Result;
use image::{ImageFormat, RgbaImage};
use serde_json::{json, Value};

use super::geometry::linear_rgba;
use super::scene::relative_path;
use super::texture::additive_png;
use super::{Log, Settings};
use crate::glb::Glb;
use crate::revolt::{find_file, game_path, tri};
use crate::revolt::ncp::{Ncp, NCP_CAMERA_ONLY, NCP_DOUBLE};
use crate::revolt::fob::Object;

pub const LASER: i32 = 33;
// the sounds of a laser in the level block: hum, alarm while something is in the beam
pub const LASER_SOUNDS: [(&str, usize); 2] = [("loop", 0x2C), ("alarm", 0x2D)];

// the game tests the beam 10000 units ahead
const LASER_REACH: f32 = 10000.0;

// the laser band of fxpage1: across the beam, texels (230, 32..48)
const LASER_TEXELS: (u32, u32, u32) = (230, 32, 16);
// the beam is drawn as blades crossing in its axis - from every side some width
const LASER_BLADES: usize = 3;

// the piano of the game (hard coded in the game, units): front left corner, keyboard size, the black keys from this depth, height of the keys
const PIANO_X0: f32 = 1040.0;
const PIANO_Z0: f32 = -6525.0;
const PIANO_WIDTH: f32 = 2285.0;
const PIANO_DEPTH: f32 = 250.0;
const PIANO_BLACK_FROM: f32 = 112.5;
const PIANO_Y: f32 = -342.5;
const PIANO_WHITE_KEYS: usize = 32;
// the key boundaries that have a black key
const PIANO_BLACK: [i64; 23] = [1, 2, 3, 5, 6, 8, 9, 10, 12, 13, 15, 16, 17, 19, 20, 22, 23, 24, 26, 27, 29, 30, 31];
// the light colors: white keys by key % 7 (a rainbow), black keys by index % 3
const PIANO_WHITE_COLORS: [u32; 7] = [0xff0c07, 0xff4700, 0xffe300, 0x09c31f, 0x0282ff, 0x1000b9, 0xa000e1];
const PIANO_BLACK_COLORS: [u32; 3] = [0xff0000, 0x00ff00, 0x0000ff];
// the lights lie this far above the keys (m)
const PIANO_LIFT: f32 = 0.005;
// the toy block sound of the piano (id 0x2B)
pub const PIANO_SOUND: usize = 0x2B;

// a beam 1 m long along -z and 1 m wide: the app code scales it to its length and flickering width
pub fn write_laser(path: &Path, game_dir: &Path, log: &Log) -> Result<()>
{
    let png = laser_texture(game_dir).unwrap_or_else(|error|
    {
        log.warn(format!("  laser: gfx/fxpage1.bmp: {error:#} - a plain red beam"));
        plain_red()
    });
    write_beam(path, "laser", &png, [1.0, 0.2, 0.15])?;
    log.write(path);
    Ok(())
}

// the lightning of the game (battle tag arc: green, electro pulse: blue): a strip of fxpage1, across it texels v 33..47, along it 7 texels from u
// a piece of 1 m like the laser beam - the app chains many short pieces between the cars
pub const LIGHTNING_GREEN: u32 = 209;
pub const LIGHTNING_BLUE: u32 = 216;
const LIGHTNING_ACROSS: (u32, u32) = (33, 15);
const LIGHTNING_ALONG: u32 = 7;

// a square of fxpage1 (x, y, width, height in texels) as a quad of 1 m in the xy plane, facing +z, additive look
pub const SHOCK_TILE: [u32; 4] = [0, 64, 64, 64];

pub fn write_sprite(path: &Path, game_dir: &Path, tile: [u32; 4], emissive: [f32; 3], log: &Log) -> Result<()>
{
    let png = match image::open(game_path(game_dir, "gfx/fxpage1.bmp"))
    {
        Ok(page) =>
        {
            let image = image::imageops::crop_imm(&page.to_rgba8(), tile[0], tile[1], tile[2], tile[3]).to_image();
            let mut png = vec![];
            image.write_to(&mut Cursor::new(&mut png), ImageFormat::Png)?;
            png
        },
        Err(error) => { log.warn(format!("  sprite: gfx/fxpage1.bmp: {error:#} - a plain one")); plain_red() },
    };
    let name = path.file_stem().map_or("sprite".to_string(), |stem| stem.to_string_lossy().to_string());
    let mut glb = Glb::default();
    let texture = glb.png_texture(&additive_png(&png)?, &format!("{name}_additive.png"));
    glb.use_extension("KHR_materials_unlit");
    let material = glb.material(json!({
        "name": name,
        "doubleSided": true,
        "alphaMode": "BLEND",
        "emissiveFactor": emissive,
        "pbrMetallicRoughness": { "baseColorTexture": { "index": texture }, "metallicFactor": 0.0, "roughnessFactor": 1.0 },
        "extensions": { "KHR_materials_unlit": {} },
        "extras": { "revolt_blend": "additive" },
    }));
    let positions = [-0.5f32, -0.5, 0.0, 0.5, -0.5, 0.0, 0.5, 0.5, 0.0, -0.5, 0.5, 0.0];
    let normals = [0.0f32, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0];
    let uvs = [0.0f32, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0];
    let primitive = json!({
        "attributes": { "POSITION": glb.floats(&positions, "VEC3", true), "NORMAL": glb.floats(&normals, "VEC3", false), "TEXCOORD_0": glb.floats(&uvs, "VEC2", false) },
        "indices": glb.indices(&[0, 1, 2, 0, 2, 3]),
        "material": material,
    });
    let mesh = glb.mesh(&name, vec![primitive]);
    let root = glb.node(json!({ "name": name, "mesh": mesh, "extras": { "rustl_collision": false, "rustl_camera_collision": false } }));
    glb.save(path, &[root])?;
    log.write(path);
    Ok(())
}

pub fn write_lightning(path: &Path, game_dir: &Path, u: u32, emissive: [f32; 3], log: &Log) -> Result<()>
{
    let png = match image::open(game_path(game_dir, "gfx/fxpage1.bmp"))
    {
        Ok(page) =>
        {
            let page = page.to_rgba8();
            let (v, across) = LIGHTNING_ACROSS;
            let image = RgbaImage::from_fn(across, LIGHTNING_ALONG, |x, y| { let mut pixel = *page.get_pixel(u + y, v + x); pixel[3] = 255; pixel });
            let mut png = vec![];
            image.write_to(&mut Cursor::new(&mut png), ImageFormat::Png)?;
            png
        },
        Err(error) => { log.warn(format!("  lightning: gfx/fxpage1.bmp: {error:#} - a plain beam")); plain_red() },
    };
    let name = path.file_stem().map_or("lightning".to_string(), |stem| stem.to_string_lossy().to_string());
    write_beam(path, &name, &png, emissive)?;
    log.write(path);
    Ok(())
}

// blades crossing along -z, 1 m long, 1 m wide, additive look
fn write_beam(path: &Path, name: &str, png: &[u8], emissive: [f32; 3]) -> Result<()>
{
    let mut glb = Glb::default();
    let texture = glb.png_texture(&additive_png(png)?, &format!("{name}_additive.png"));
    glb.use_extension("KHR_materials_unlit");
    let material = glb.material(json!({
        "name": name,
        "doubleSided": true,
        "alphaMode": "BLEND",
        "emissiveFactor": emissive,
        "pbrMetallicRoughness": { "baseColorTexture": { "index": texture }, "metallicFactor": 0.0, "roughnessFactor": 1.0 },
        "extensions": { "KHR_materials_unlit": {} },
        "extras": { "revolt_blend": "additive" },
    }));

    let mut positions = vec![];
    let mut normals = vec![];
    let mut uvs = vec![];
    let mut indices = vec![];
    for blade in 0..LASER_BLADES
    {
        let angle = blade as f32 * std::f32::consts::PI / LASER_BLADES as f32;
        let (side, normal) = ([angle.cos() * 0.5, angle.sin() * 0.5], [-angle.sin(), angle.cos(), 0.0]);
        let base = (positions.len() / 3) as u32;
        for (s, z, u, v) in [(-1.0, 0.0, 0.0, 0.0), (1.0, 0.0, 1.0, 0.0), (1.0, -1.0, 1.0, 1.0), (-1.0, -1.0, 0.0, 1.0)]
        {
            positions.extend([side[0] * s, side[1] * s, z]);
            normals.extend(normal);
            uvs.extend([u, v]);
        }
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    let primitive = json!({
        "attributes": { "POSITION": glb.floats(&positions, "VEC3", true), "NORMAL": glb.floats(&normals, "VEC3", false), "TEXCOORD_0": glb.floats(&uvs, "VEC2", false) },
        "indices": glb.indices(&indices),
        "material": material,
    });
    let mesh = glb.mesh(name, vec![primitive]);
    let root = glb.node(json!({ "name": name, "mesh": mesh, "extras": { "rustl_collision": false, "rustl_camera_collision": false } }));
    glb.save(path, &[root])?;
    Ok(())
}

// the beam length like the game at load: a line along the look axis against the level - one sided, the emitters sit behind the wall surface
// (the engine collision is two sided: its ray hit that wall from behind after 0.2-0.7 m) - m, the reach if nothing is hit
pub fn laser_length(object: &Object, ncp: Option<&Ncp>, scale: f32) -> f32
{
    let hit = ncp.and_then(|ncp| ray_one_sided(ncp, object.position, object.look, LASER_REACH));
    hit.unwrap_or(LASER_REACH) * scale
}

// units to the first front face along the ray (double sided polygons from both sides)
fn ray_one_sided(ncp: &Ncp, origin: [f32; 3], direction: [f32; 3], reach: f32) -> Option<f32>
{
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let mut best: Option<f32> = None;
    for polygon in &ncp.polygons
    {
        if polygon.has(NCP_CAMERA_ONLY) { continue; }
        let facing = dot(polygon.plane.normal, direction);
        if facing >= -1e-6 && !(polygon.has(NCP_DOUBLE) && facing.abs() > 1e-6) { continue; }
        let t = -(dot(polygon.plane.normal, origin) + polygon.plane.distance) / facing;
        if t <= 0.0 || t > reach || best.is_some_and(|best| t >= best) { continue; }
        let point = [0, 1, 2].map(|a| origin[a] + direction[a] * t);
        let sides = if polygon.has(crate::revolt::ncp::NCP_QUAD) { 4 } else { 3 };
        if polygon.edges[..sides].iter().all(|edge| dot(edge.normal, point) + edge.distance <= 1.0)
        {
            best = Some(t);
        }
    }
    best
}

// the band across the beam as one row: the first 16 texels down from (230, 32) of fxpage1
fn laser_texture(game_dir: &Path) -> Result<Vec<u8>>
{
    let page = image::open(game_path(game_dir, "gfx/fxpage1.bmp"))?.to_rgba8();
    let (u, v, count) = LASER_TEXELS;
    let image = RgbaImage::from_fn(count, 2, |x, _| { let mut pixel = *page.get_pixel(u, v + x); pixel[3] = 255; pixel });
    let mut png = vec![];
    image.write_to(&mut Cursor::new(&mut png), ImageFormat::Png)?;
    Ok(png)
}

fn plain_red() -> Vec<u8>
{
    let image = RgbaImage::from_fn(16, 2, |x, _| { let c = (1.0 - ((x as f32 - 7.5) / 8.0).abs()).max(0.0); image::Rgba([255, (160.0 * c * c) as u8, (128.0 * c * c) as u8, (255.0 * c) as u8]) });
    let mut png = vec![];
    let _ = image.write_to(&mut Cursor::new(&mut png), ImageFormat::Png);
    png
}

// the piano of the level: its keyboard and triggers as extras, one light per key in a glb - None without a piano trigger (type 0)
pub fn piano(track_dir: &Path, level: &str, sound: Option<&'static str>, settings: &Settings, log: &Log) -> Result<Option<Value>>
{
    let Some(path) = find_file(track_dir, &format!("{level}.tri")) else { return Ok(None); };
    let mut triggers: Vec<tri::Trigger> = tri::load(&path)?.into_iter().filter(|t| t.kind == tri::TRIGGER_PIANO).collect();
    if settings.mirrored { super::mirror::triggers(&mut triggers); }
    if triggers.is_empty() { return Ok(None); }

    let scale = settings.scale;
    let key_width = PIANO_WIDTH / PIANO_WHITE_KEYS as f32 * scale;
    let (depth, black_from) = (PIANO_DEPTH * scale, PIANO_BLACK_FROM * scale);
    // mirrored: the keys run to -x from the mirrored corner
    let mirrored = settings.mirrored;
    let origin = [PIANO_X0 * scale * if mirrored { -1.0 } else { 1.0 }, -PIANO_Y * scale, -PIANO_Z0 * scale];

    let mut glb = Glb::default();
    glb.use_extension("KHR_materials_unlit");
    let mut children = vec![];
    let quarter = key_width * 0.25;
    for i in 0..PIANO_WHITE_KEYS
    {
        let (left, right) = (i as f32 * key_width, (i + 1) as f32 * key_width);
        // the back part leaves room for the black keys on its boundaries
        let notch = |boundary: usize| if PIANO_BLACK.contains(&(boundary as i64)) { quarter } else { 0.0 };
        let rects = [[left, right, 0.0, black_from], [left + notch(i), right - notch(i + 1), black_from, depth]];
        children.push(key_node(&mut glb, &format!("white_{i:02}"), &rects, PIANO_WHITE_COLORS[i % 7], mirrored));
    }
    for (index, &boundary) in PIANO_BLACK.iter().enumerate()
    {
        let center = boundary as f32 * key_width;
        children.push(key_node(&mut glb, &format!("black_{index:02}"), &[[center - quarter, center + quarter, black_from, depth]], PIANO_BLACK_COLORS[index % 3], mirrored));
    }
    let root = glb.node(json!({ "name": "piano", "children": children, "translation": [0.0, PIANO_LIFT, 0.0] }));
    let out = settings.level_dir(level).join("objects").join("piano.glb");
    glb.save(&out, &[root])?;
    log.write(&out);

    let round = |v: f32| (v as f64 * 1e5).round() / 1e5;
    let trigger_values: Vec<Value> = triggers.iter().map(|t| json!({
        "center": super::geometry::to_gltf(t.center).map(|v| round(v * scale)),
        "axes": t.axes.map(|axis| super::geometry::to_gltf(axis).map(round)),
        "half_size": t.half_size.map(|v| round(v * scale)),
    })).collect();
    let data = json!({
        "x0": round(origin[0]), "z0": round(origin[2]), "y": round(origin[1]),
        "key_width": round(key_width), "depth": round(depth), "black_from": round(black_from),
        "white_keys": PIANO_WHITE_KEYS, "black": PIANO_BLACK, "triggers": trigger_values, "mirrored": mirrored,
    });

    let mut extras = serde_json::Map::new();
    extras.insert("revolt_piano".into(), json!(data.to_string()));
    match sound
    {
        Some(sound) => { extras.insert("revolt_sound_piano".into(), json!(sound)); },
        None => log.warn("  piano: no piano sound in the sound block of the level"),
    }
    log.info(format!("  piano: {} triggers, {PIANO_WHITE_KEYS} white + {} black keys, {:.2} m wide at ({:.2}, {:.2}, {:.2}) m, sound {}", triggers.len(), PIANO_BLACK.len(), PIANO_WIDTH * scale, origin[0], origin[1], origin[2], sound.unwrap_or("-")));

    Ok(Some(json!({
        "name": "piano",
        "options": { "settings": { "collision": false, "camera_collision": false } },
        "position": origin,
        "rotation": [0.0, 0.0, 0.0],
        "rotation_quat": [0.0, 0.0, 0.0, 1.0],
        "scale": [1.0, 1.0, 1.0],
        "extras": extras,
        "tags": ["revolt_piano"],
        "source": relative_path(&settings.project_dir, &out),
    })))
}

// a flat light over a key: rects [left, right, front, back] in m from the front left corner, the keys go to -z - the node sits in the middle of the key (its sound plays there)
fn key_node(glb: &mut Glb, name: &str, rects: &[[f32; 4]], color: u32, mirrored: bool) -> usize
{
    let [r, g, b, _] = linear_rgba(0xff000000 | color, Some(1.0));
    let material = glb.material(json!({
        "name": name,
        "alphaMode": "BLEND",
        "emissiveFactor": [r, g, b],
        "pbrMetallicRoughness": { "baseColorFactor": [r, g, b, 0.75], "metallicFactor": 0.0, "roughnessFactor": 1.0 },
        "extensions": { "KHR_materials_unlit": {} },
    }));
    let x = (rects.iter().map(|r| r[0]).fold(f32::MAX, f32::min) + rects.iter().map(|r| r[1]).fold(f32::MIN, f32::max)) * 0.5;
    let z = -(rects.iter().map(|r| r[2]).fold(f32::MAX, f32::min) + rects.iter().map(|r| r[3]).fold(f32::MIN, f32::max)) * 0.5;
    let mut positions = vec![];
    let mut indices = vec![];
    for [left, right, front, back] in rects
    {
        let base = (positions.len() / 3) as u32;
        positions.extend([left - x, 0.0, -front - z, right - x, 0.0, -front - z, right - x, 0.0, -back - z, left - x, 0.0, -back - z]);
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    // mirrored: the key at -x, its quads mirrored and the other way round
    let x = if mirrored
    {
        positions.iter_mut().step_by(3).for_each(|v| *v = -*v);
        indices.chunks_mut(3).for_each(|triangle| triangle.swap(1, 2));
        -x
    }
    else { x };
    let normals: Vec<f32> = (0..positions.len() / 3).flat_map(|_| [0.0, 1.0, 0.0]).collect();
    let primitive = json!({
        "attributes": { "POSITION": glb.floats(&positions, "VEC3", true), "NORMAL": glb.floats(&normals, "VEC3", false) },
        "indices": glb.indices(&indices),
        "material": material,
    });
    let mesh = glb.mesh(name, vec![primitive]);
    glb.node(json!({ "name": name, "mesh": mesh, "translation": [x, 0.0, z], "extras": { "rustl_collision": false, "rustl_camera_collision": false, "rustl_visible": false } }))
}

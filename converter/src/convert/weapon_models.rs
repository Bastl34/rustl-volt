// the models of the weapon objects the app code needs (weapons, the podium ceremony after a cup): models/firework.m, models/ball.m (the putty bomb around a car,
// the ball bearing, the shockwave), models/wbomb.m (the water balloon)
// and a spark star for the bursts (the game draws sparks there - no particles yet) - assets/models/*.glb, vertex colors, unlit

use std::path::Path;

use anyhow::{Context, Result};
use serde_json::{json, Value};

use super::geometry::{linear_rgba, to_gltf};
use super::{Log, Settings};
use crate::glb::Glb;
use crate::revolt::find_file;
use crate::revolt::mesh::Mesh;

// the putty bomb ball is see through
const BOMB_ALPHA: f32 = 0.45;
const BALL_GRAY: [f32; 4] = [0.45, 0.45, 0.47, 1.0];
// the spark star: spikes of 1 m, this wide, white in the middle and orange at the tips
const SPARKS: usize = 24;
const SPARK_WIDTH: f32 = 0.05;
// the sprite tiles of fxpage1 the particles use (x, y, size in texels) - faded out round, the glows fill their tiles up to the edge (a square on screen)
const SPRITE_TILES: [(u32, u32, u32); 11] = [(0, 0, 64), (64, 0, 64), (192, 0, 16), (208, 0, 16), (224, 0, 16), (240, 0, 16), (192, 16, 16), (208, 16, 16), (224, 16, 16), (192, 32, 16), (240, 32, 16)];
// share of the tile size from its center: full up to the first, nothing from the second on
const SPRITE_FADE: [f32; 2] = [0.34, 0.5];

pub fn convert(settings: &Settings, log: &Log) -> Result<()>
{
    let dir = settings.game_dir.join("models");
    let out = settings.out_dir.join("models");
    std::fs::create_dir_all(&out)?;
    // the ball bearing is ball.m in one gray (the game draws it chrome, the app adds the env map) - its own colors made it a beach ball
    for (model, name, alpha, color) in [("firework.m", "firework", None, None), ("ball.m", "putty_bomb", Some(BOMB_ALPHA), None), ("wbomb.m", "water_balloon", None, None), ("ball.m", "ball_bearing", None, Some(BALL_GRAY))]
    {
        let path = find_file(&dir, model).with_context(|| format!("models/{model} missing"))?;
        log.read(&path);
        let (mesh, _) = Mesh::load(&path)?;
        let file = out.join(format!("{name}.glb"));
        write_model(&file, name, &mesh, settings.scale, alpha, color)?;
        log.info(format!("  {model}: {} polygons --> {name}.glb", mesh.polygons.len()));
        log.write(&file);
    }
    let file = out.join("burst.glb");
    write_burst(&file)?;
    log.info(format!("  burst.glb: {SPARKS} sparks of 1 m (the app scales and tints it)"));
    log.write(&file);
    // the lightning of the game: the arc of battle tag green, the electro pulse blue
    super::effects::write_lightning(&out.join("lightning.glb"), &settings.game_dir, super::effects::LIGHTNING_GREEN, [0.3, 1.0, 0.2], log)?;
    super::effects::write_lightning(&out.join("lightning_blue.glb"), &settings.game_dir, super::effects::LIGHTNING_BLUE, [0.3, 0.4, 1.0], log)?;
    // the shockwave of the game: the blue web of fxpage1 as a sprite, the app turns it to the camera
    super::effects::write_sprite(&out.join("shock_sprite.glb"), &settings.game_dir, super::effects::SHOCK_TILE, [0.3, 0.5, 1.4], log)?;
    // the skins of the game (turbo, zap, bomb): fxpage1 as it is, the app moves the uvs of a copy of the car over its tiles
    match image::open(settings.game_dir.join("gfx").join("fxpage1.bmp"))
    {
        Ok(image) =>
        {
            let file = out.join("fxpage1.png");
            image.to_rgb8().save(&file)?;
            log.write(&file);
            // the particles of the app: the same page with black see through (the color key of the game)
            let mut sparks = image.to_rgba8();
            for pixel in sparks.pixels_mut() { if pixel[0] == 0 && pixel[1] == 0 && pixel[2] == 0 { pixel[3] = 0; } }
            for (x0, y0, size) in SPRITE_TILES
            {
                for y in y0..(y0 + size).min(sparks.height())
                {
                    for x in x0..(x0 + size).min(sparks.width())
                    {
                        let center = size as f32 * 0.5;
                        let distance = ((x - x0) as f32 + 0.5 - center).hypot((y - y0) as f32 + 0.5 - center) / size as f32;
                        let t = ((SPRITE_FADE[1] - distance) / (SPRITE_FADE[1] - SPRITE_FADE[0])).clamp(0.0, 1.0);
                        let fade = t * t * (3.0 - 2.0 * t);
                        // brightness for the additive kinds, alpha for the blended ones
                        let pixel = sparks.get_pixel_mut(x, y);
                        for channel in 0..4 { pixel[channel] = (pixel[channel] as f32 * fade).round() as u8; }
                    }
                }
            }
            let file = out.join("sparks.png");
            sparks.save(&file)?;
            log.write(&file);
        },
        Err(error) => log.warn(format!("  skins: gfx/fxpage1.bmp: {error:#} - plain skins")),
    }
    Ok(())
}

fn unlit_material(glb: &mut Glb, name: &str, alpha: Option<f32>, emissive: Option<[f32; 3]>) -> usize
{
    glb.use_extension("KHR_materials_unlit");
    let mut material = json!({
        "name": name,
        "pbrMetallicRoughness": { "baseColorFactor": [1.0, 1.0, 1.0, alpha.unwrap_or(1.0)], "metallicFactor": 0.0, "roughnessFactor": 1.0 },
        "extensions": { "KHR_materials_unlit": {} },
    });
    if alpha.is_some() { material["alphaMode"] = json!("BLEND"); material["doubleSided"] = json!(true); }
    if let Some(emissive) = emissive { material["emissiveFactor"] = json!(emissive); }
    glb.material(material)
}

// every corner its own vertex with the polygon color, clockwise of the game -> counter clockwise
fn write_model(path: &Path, name: &str, mesh: &Mesh, scale: f32, alpha: Option<f32>, color: Option<[f32; 4]>) -> Result<()>
{
    let mut glb = Glb::default();
    let material = unlit_material(&mut glb, name, alpha, None);
    let (mut positions, mut normals, mut colors, mut indices) = (vec![], vec![], vec![], vec![]);
    for polygon in &mesh.polygons
    {
        let corners = polygon.corners();
        if polygon.indices[..corners].iter().any(|&i| i as usize >= mesh.vertices.len()) { continue; }
        let base = (positions.len() / 3) as u32;
        for c in 0..corners
        {
            let vertex = &mesh.vertices[polygon.indices[c] as usize];
            positions.extend(to_gltf(vertex.position).map(|v| v * scale));
            normals.extend(to_gltf(vertex.normal));
            colors.extend(color.unwrap_or_else(|| linear_rgba(polygon.colors[c], Some(1.0))));
        }
        indices.extend([base, base + 2, base + 1]);
        if corners == 4 { indices.extend([base, base + 3, base + 2]); }
    }
    let primitive = json!({
        "attributes": {
            "POSITION": glb.floats(&positions, "VEC3", true),
            "NORMAL": glb.floats(&normals, "VEC3", false),
            "COLOR_0": glb.floats(&colors, "VEC4", false),
        },
        "indices": glb.indices(&indices),
        "material": material,
    });
    let mesh = glb.mesh(name, vec![primitive]);
    let root = glb.node(json!({ "name": name, "mesh": mesh, "extras": { "rustl_collision": false, "rustl_camera_collision": false } }));
    glb.save(path, &[root])
}

// spikes in all directions (a golden spiral), each two crossed quads - additive and emissive for the bloom
fn write_burst(path: &Path) -> Result<()>
{
    let mut glb = Glb::default();
    let material = unlit_material(&mut glb, "burst", Some(1.0), Some([1.0, 0.6, 0.25]));
    let (mut positions, mut normals, mut colors, mut indices): (Vec<f32>, Vec<f32>, Vec<f32>, Vec<u32>) = (vec![], vec![], vec![], vec![]);
    let golden = std::f32::consts::PI * (3.0 - 5f32.sqrt());
    for i in 0..SPARKS
    {
        let y = 1.0 - 2.0 * (i as f32 + 0.5) / SPARKS as f32;
        let ring = (1.0 - y * y).sqrt();
        let angle = golden * i as f32;
        let direction = [ring * angle.cos(), y, ring * angle.sin()];
        // two sides across the spike
        let any = if direction[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
        let side = normalize(cross(direction, any));
        let other = cross(direction, side);
        for across in [side, other]
        {
            let base = (positions.len() / 3) as u32;
            let half = across.map(|v| v * SPARK_WIDTH * 0.5);
            for (point, white) in [([-half[0], -half[1], -half[2]], true), (half, true), ([direction[0] + half[0], direction[1] + half[1], direction[2] + half[2]], false), ([direction[0] - half[0], direction[1] - half[1], direction[2] - half[2]], false)]
            {
                positions.extend(point);
                normals.extend(cross(direction, across));
                colors.extend(if white { [1.0, 1.0, 0.9, 1.0] } else { [1.0, 0.45, 0.1, 0.0] });
            }
            indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
    let primitive = json!({
        "attributes": {
            "POSITION": glb.floats(&positions, "VEC3", true),
            "NORMAL": glb.floats(&normals, "VEC3", false),
            "COLOR_0": glb.floats(&colors, "VEC4", false),
        },
        "indices": glb.indices(&indices),
        "material": material,
    });
    let mesh = glb.mesh("burst", vec![primitive]);
    let mut extras = serde_json::Map::new();
    extras.insert("rustl_collision".into(), Value::Bool(false));
    extras.insert("rustl_camera_collision".into(), Value::Bool(false));
    let root = glb.node(json!({ "name": "burst", "mesh": mesh, "extras": extras }));
    glb.save(path, &[root])
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3]
{
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn normalize(v: [f32; 3]) -> [f32; 3]
{
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
    v.map(|c| c / length)
}

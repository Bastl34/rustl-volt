// the frontend level is the 3D menu (formats/README.md, Frontend): car boxes with the picture of their car, the name wheel, the track screen

use std::collections::BTreeMap;
use std::path::Path;

use image::RgbaImage;
use serde_json::{json, Value};

use super::geometry::{to_gltf, MeshWriter};
use super::texture::{load_plain, Page};
use super::Log;
use crate::glb::Glb;
use crate::revolt::{find_file, game_path};
use crate::revolt::mesh::{Mesh, FACE_ENV};

pub const LEVEL: &str = "frontend";
pub const CAR_BOX: i32 = 46;
const NAME_ENTRY: i32 = 39;
const TRACK_SCREEN: i32 = 44;

// sounds of the menu and of the clockwork cars on the table - into the scene sound list of the frontend
pub const SOUNDS: [&str; 11] = ["menunext", "menuprev", "menuupdown", "menuleftright", "boxslide", "tvstatic", "clockwrk", "skid_normal", "scrape", "hit1", "hit2"];

// the picture of a car box: 3 x 3 cells per page, cars 0-26 on pages 27-29 (carbox1-3.bmp), panga (27) on page 4, the plain box (-1) on page 6
const CELL_STEP: f32 = 85.0 / 256.0;
const CELL_SIZE: f32 = 84.0 / 256.0;
const CELL_INSET: f32 = 1.0 / 256.0;

// the name wheel sits this far from the stand origin (game units, stand space) and turns about its z axis
const WHEEL_OFFSET: [f32; 3] = [0.0, -490.0, -80.0];

// the corners of the picture on the big screen (game units, screen space): top left, top right, bottom right, bottom left
const SCREEN_CORNERS: [[f32; 3]; 4] = [[-299.0, -726.4, -75.2], [299.0, -726.4, -75.2], [299.0, -291.6, 5.0], [-299.0, -291.6, 5.0]];
// game units the picture lies in front of the screen model
const SCREEN_LIFT: f32 = 3.0;
// noise frames of the tv static
const STATIC_FRAMES: u32 = 4;

pub fn is_frontend(level: &str) -> bool
{
    level.eq_ignore_ascii_case(LEVEL)
}

// page and top left uv of the picture of a car id
fn picture(car: i32) -> (i16, f32, f32)
{
    match car
    {
        0..=26 => (27 + (car / 9) as i16, (car % 3) as f32 * CELL_STEP + CELL_INSET, ((car % 9) / 3) as f32 * CELL_STEP + CELL_INSET),
        27 => (4, 171.0 / 256.0, 73.0 / 256.0),
        _ => (6, 0.0, 171.0 / 256.0),
    }
}

// the car box model with the picture of this car on its picture polygon (the first one with flag 0x800)
pub fn car_box(mesh: &Mesh, car: i32) -> Option<Mesh>
{
    let mut mesh = mesh.clone();
    let polygon = mesh.polygons.iter_mut().find(|p| p.has(FACE_ENV))?;
    let (page, u, v) = picture(car);
    polygon.flags &= !FACE_ENV;
    polygon.texture = page;
    polygon.uvs = [[u + CELL_SIZE, v], [u + CELL_SIZE, v + CELL_SIZE], [u, v + CELL_SIZE], [u, v]];
    Some(mesh)
}

pub fn car_box_stem(car: i32) -> String
{
    if car < 0 { "carbox_plain".to_string() } else { format!("carbox_{car:02}") }
}

// child nodes of a frontend model glb: the name wheel of the stand, the track pictures of the big screen
pub fn children(kind: i32, glb: &mut Glb, writer: &mut MeshWriter, parts: &[&Mesh], game_dir: &Path, scale: f32, log: &Log) -> Vec<usize>
{
    match kind
    {
        NAME_ENTRY => name_wheel(glb, writer, parts, scale, log).into_iter().collect(),
        TRACK_SCREEN => track_pictures(glb, game_dir, scale, log),
        _ => vec![],
    }
}

fn name_wheel(glb: &mut Glb, writer: &mut MeshWriter, parts: &[&Mesh], scale: f32, log: &Log) -> Option<usize>
{
    let Some(wheel) = parts.first() else
    {
        log.warn("  name stand: namewheel.m missing - no wheel");
        return None;
    };
    let mut primitives = BTreeMap::new();
    for polygon in &wheel.polygons
    {
        writer.add(&mut primitives, wheel, polygon, None, -1);
    }
    let written = writer.write(glb, "namewheel", primitives)?;
    let translation = to_gltf(WHEEL_OFFSET).map(|v| v * scale);
    log.info(format!("  name stand: child node 'namewheel' at ({:.2}, {:.2}, {:.2}) m, turns about its z axis (30 letters)", translation[0], translation[1], translation[2]));
    Some(glb.node(json!({ "name": "namewheel", "mesh": written.mesh, "translation": translation })))
}

// one hidden quad per gfx/<level>.bmp (the preview images of the track menu): screen_<level> - the app code shows the selected one
fn track_pictures(glb: &mut Glb, game_dir: &Path, scale: f32, log: &Log) -> Vec<usize>
{
    let gfx = game_path(game_dir, "gfx");
    let mut levels: Vec<String> = std::fs::read_dir(game_path(game_dir, "levels")).into_iter().flatten().flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().to_lowercase())
        .filter(|level| !is_frontend(level))
        .collect();
    levels.sort();

    let corners: Vec<[f32; 3]> = SCREEN_CORNERS.iter().map(|&[x, y, z]| to_gltf([x, y, z - SCREEN_LIFT]).map(|v| v * scale)).collect();
    let positions: Vec<f32> = corners.concat();
    let edge = |a: usize, b: usize| [0, 1, 2].map(|i| corners[b][i] - corners[a][i]);
    let (right, down) = (edge(0, 1), edge(0, 3));
    let normal = [right[1] * down[2] - right[2] * down[1], right[2] * down[0] - right[0] * down[2], right[0] * down[1] - right[1] * down[0]];
    let length = normal.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-6);
    let normals: Vec<f32> = [0; 4].iter().flat_map(|_| normal.map(|v| v / length)).collect();
    let uvs = [0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0];

    let mut nodes = vec![];
    for level in levels
    {
        let Some(path) = find_file(&gfx, &format!("{level}.bmp")) else { continue; };
        let image = match load_plain(&path)
        {
            Ok(image) => image,
            Err(err) => { log.warn(format!("  track screen: {err:#}")); continue; },
        };
        log.read(&path);
        let texture = glb.png_texture_clamped(&image.png, &format!("{level}.png"));
        let material = glb.material(json!({
            "name": format!("screen_{level}"),
            "doubleSided": true,
            "pbrMetallicRoughness": { "baseColorTexture": { "index": texture }, "metallicFactor": 0.0, "roughnessFactor": 1.0 },
            "extensions": { "KHR_materials_unlit": {} },
        }));
        glb.use_extension("KHR_materials_unlit");
        let primitive = json!({
            "attributes": {
                "POSITION": glb.floats(&positions, "VEC3", true),
                "NORMAL": glb.floats(&normals, "VEC3", false),
                "TEXCOORD_0": glb.floats(&uvs, "VEC2", false),
            },
            "indices": glb.indices(&[0, 1, 2, 0, 2, 3]),
            "material": material,
        });
        let mesh = glb.mesh(&format!("screen_{level}"), vec![primitive]);
        let extras: Value = json!({ "rustl_visible": false, "rustl_collision": false, "revolt_track_picture": level });
        nodes.push(glb.node(json!({ "name": format!("screen_{level}"), "mesh": mesh, "extras": extras })));
    }
    log.info(format!("  track screen: {} hidden pictures screen_<level> (gfx/<level>.bmp)", nodes.len()));

    // tv static between two pictures: a few noise frames the app code flips through
    for frame in 0..STATIC_FRAMES
    {
        let mut png = vec![];
        if static_frame(frame).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).is_err() { continue; }
        let name = format!("screen_static_{frame}");
        let texture = glb.png_texture_clamped(&png, &format!("{name}.png"));
        let material = glb.material(json!({
            "name": name,
            "doubleSided": true,
            "pbrMetallicRoughness": { "baseColorTexture": { "index": texture }, "metallicFactor": 0.0, "roughnessFactor": 1.0 },
            "extensions": { "KHR_materials_unlit": {} },
        }));
        let primitive = json!({
            "attributes": {
                "POSITION": glb.floats(&positions, "VEC3", true),
                "NORMAL": glb.floats(&normals, "VEC3", false),
                "TEXCOORD_0": glb.floats(&uvs, "VEC2", false),
            },
            "indices": glb.indices(&[0, 1, 2, 0, 2, 3]),
            "material": material,
        });
        let mesh = glb.mesh(&name, vec![primitive]);
        let extras: Value = json!({ "rustl_visible": false, "rustl_collision": false, "revolt_track_static": frame });
        nodes.push(glb.node(json!({ "name": name, "mesh": mesh, "extras": extras })));
    }
    log.info(format!("  track screen: {STATIC_FRAMES} hidden static frames screen_static_N (own noise, shown on a change of track)"));
    nodes
}

// gray noise with brighter / darker scan lines and a rolling bright band - the same every conversion
fn static_frame(frame: u32) -> RgbaImage
{
    let (width, height) = (160, 120);
    let mut seed = 0x9e37_79b9u32 ^ frame.wrapping_mul(0x85eb_ca6b);
    let mut next = move || { seed ^= seed << 13; seed ^= seed >> 17; seed ^= seed << 5; seed };
    let band = (frame * height / STATIC_FRAMES) as f32;
    let mut image = RgbaImage::new(width, height);
    for y in 0..height
    {
        let line = 0.75 + (next() % 50) as f32 / 100.0;
        let roll = 1.0 + 0.5 * (-((y as f32 - band).powi(2)) / 60.0).exp();
        for x in 0..width
        {
            let value = ((next() % 256) as f32 * line * roll * if y % 2 == 0 { 1.0 } else { 0.8 }).min(255.0) as u8;
            image.put_pixel(x, y, image::Rgba([value, value, value, 255]));
        }
    }
    image
}

// frontendf.bmp: the wooden RE-VOLT logo on the counter (texels 0..128, 128..256) - replaced by the logo of the project, carved into the same wood
const WOOD_PAGE: usize = 5;
const WOOD_RECT: [u32; 4] = [0, 128, 128, 128];
// rows of plain wood above / below the old logo, mirrored to fill the square
const WOOD_CLEAN: [(u32, u32); 2] = [(0, 18), (106, 128)];
// the project logo in its own files (the checkerboard cut out)
const PROJECT_LOGO: &str = "rustl-volt_alpha.png";
// supersampling of the carving
const CARVE_SCALE: u32 = 4;

pub fn replace_wood_logo(pages: &mut [Option<Page>], files_dir: &Path, log: &Log)
{
    let path = files_dir.join(PROJECT_LOGO);
    let Some(page) = pages.get_mut(WOOD_PAGE).and_then(Option::as_mut) else
    {
        log.warn("  wood logo: page f missing - the old logo stays");
        return;
    };
    let logo = match image::open(&path)
    {
        Ok(logo) => logo.to_rgba8(),
        Err(err) => { log.warn(format!("  wood logo: {}: {err} - the old logo stays", path.display())); return; },
    };
    log.read(&path);
    let mut image = match image::load_from_memory(&page.png)
    {
        Ok(image) => image.to_rgba8(),
        Err(err) => { log.warn(format!("  wood logo: page f: {err}")); return; },
    };
    carve_logo(&mut image, &logo);
    let mut png = vec![];
    if image.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).is_ok()
    {
        page.png = png;
        log.info(format!("  wood logo: {} carved into {} at ({}, {}) {}×{} (the RE-VOLT logo of the counter)", PROJECT_LOGO, page.name, WOOD_RECT[0], WOOD_RECT[1], WOOD_RECT[2], WOOD_RECT[3]));
    }
}

// plain wood from the clean rows, then a height map (ring, disc, the logo sunk in, its letters raised) lit from the top left
fn carve_logo(page: &mut RgbaImage, logo: &RgbaImage)
{
    let [x0, y0, size, _] = WOOD_RECT;
    let rows: Vec<u32> = WOOD_CLEAN.iter().flat_map(|&(a, b)| a..b).collect();
    let mut wood = RgbaImage::new(size, size);
    let (mut index, mut forward) = (0i32, true);
    for y in 0..size
    {
        let source = rows[index as usize];
        for x in 0..size { wood.put_pixel(x, y, *page.get_pixel(x0 + x, y0 + source)); }
        index += if forward { 1 } else { -1 };
        if index >= rows.len() as i32 { index = rows.len() as i32 - 2; forward = false; }
        if index < 0 { index = 1; forward = true; }
    }

    let n = size * CARVE_SCALE;
    let s = CARVE_SCALE as f32;
    let width = 124.0 * s;
    let height = (width * logo.height() as f32 / logo.width() as f32).round();
    let logo = image::imageops::resize(logo, width as u32, height as u32, image::imageops::FilterType::Lanczos3);
    let (lx, ly) = (((n as f32 - width) * 0.5) as i32, ((n as f32 - height) * 0.5) as i32);
    let center = n as f32 * 0.5;

    let mut heights = vec![0.0f32; (n * n) as usize];
    let mut carved = vec![0.0f32; (n * n) as usize];
    for y in 0..n
    {
        for x in 0..n
        {
            let r = ((x as f32 - center).powi(2) + (y as f32 - center).powi(2)).sqrt() / s;
            let mut h = 0.0;
            if (50.0..58.0).contains(&r) { h = -0.5; }
            if r < 44.0 { h = -0.25; }
            let (u, v) = (x as i32 - lx, y as i32 - ly);
            if u >= 0 && v >= 0 && (u as u32) < logo.width() && (v as u32) < logo.height()
            {
                let p = logo.get_pixel(u as u32, v as u32);
                let alpha = p[3] as f32 / 255.0;
                let (max, min) = (p[0].max(p[1]).max(p[2]) as f32 / 255.0, p[0].min(p[1]).min(p[2]) as f32 / 255.0);
                let light = (p[0] as f32 + p[1] as f32 + p[2] as f32) / 765.0;
                let letter = alpha * ((light - 0.3) / 0.2).clamp(0.0, 1.0) * ((0.4 - (max - min)) / 0.2).clamp(0.0, 1.0);
                if alpha > 0.5 { h = -1.0; }
                h += 0.8 * letter;
            }
            heights[(y * n + x) as usize] = h;
            carved[(y * n + x) as usize] = (-h).clamp(0.0, 1.0);
        }
    }
    let heights = blur(&heights, n, (1.2 * s) as usize);
    let carved = blur(&carved, n, (0.6 * s) as usize);

    // lit: the slope towards the top left light brightens, a carved floor darkens and browns the wood
    let mut big = RgbaImage::new(n, n);
    let wood_big = image::imageops::resize(&wood, n, n, image::imageops::FilterType::CatmullRom);
    for y in 0..n
    {
        for x in 0..n
        {
            let at = |x: u32, y: u32| heights[(y.min(n - 1) * n + x.min(n - 1)) as usize];
            let gx = (at(x + 1, y) - at(x.saturating_sub(1), y)) * 0.5;
            let gy = (at(x, y + 1) - at(x, y.saturating_sub(1))) * 0.5;
            let shade = (-gx - gy) * 0.7 * s * 1.6 * 255.0;
            let depth = carved[(y * n + x) as usize];
            let p = wood_big.get_pixel(x, y);
            let dark = 1.0 - 0.28 * depth;
            let channel = |c: u8, tint: f32| (c as f32 * dark * tint + shade).clamp(0.0, 255.0) as u8;
            big.put_pixel(x, y, image::Rgba([channel(p[0], 1.0), channel(p[1], 1.0), channel(p[2], 1.0 - 0.15 * depth), 255]));
        }
    }
    let small = image::imageops::resize(&big, size, size, image::imageops::FilterType::Lanczos3);
    image::imageops::replace(page, &small, x0 as i64, y0 as i64);
}

// three box blurs ~ a gaussian of this radius
fn blur(values: &[f32], n: u32, radius: usize) -> Vec<f32>
{
    let n = n as usize;
    let mut a = values.to_vec();
    let mut b = vec![0.0; a.len()];
    let r = (radius / 2).max(1);
    for _ in 0..3
    {
        for horizontal in [true, false]
        {
            for line in 0..n
            {
                let index = |i: usize| if horizontal { line * n + i } else { i * n + line };
                let mut sum = 0.0;
                let mut count = 0.0;
                for i in 0..=r.min(n - 1) { sum += a[index(i)]; count += 1.0; }
                for i in 0..n
                {
                    b[index(i)] = sum / count;
                    if i + r + 1 < n { sum += a[index(i + r + 1)]; count += 1.0; }
                    if i >= r { sum -= a[index(i - r)]; count -= 1.0; }
                }
            }
            std::mem::swap(&mut a, &mut b);
        }
    }
    a
}

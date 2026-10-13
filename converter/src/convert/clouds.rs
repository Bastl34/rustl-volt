// the sky of levels without sky images - the game makes it in code: a box in the background color (FOGCOLOR),
// with a Clouds object (type 38) the clouds and the sun of gfx/clouds.bmp on a dome inside it
// indoor levels get the box alone: market1 has no ceiling, above the lamps the game shows its dark FOGCOLOR

use std::f32::consts::TAU;

use anyhow::{Context, Result};
use serde_json::{json, Value};

use super::scene::{relative_path, SUN_DIRECTION};
use super::texture::load_soft_sprites;
use super::{Log, Settings};
use crate::glb::Glb;
use crate::revolt::find_file;
use crate::revolt::fob::Object;

const CLOUDS: i32 = 38;

// pixel rectangles in clouds.bmp (x0, y0, x1, y1 inclusive)
const SPRITES: [[u32; 4]; 6] = [[16, 17, 88, 69], [128, 6, 165, 52], [168, 1, 253, 127], [22, 80, 44, 110], [67, 78, 122, 117], [5, 138, 127, 238]];
const SUN: [u32; 4] = [128, 130, 254, 254];

// by eye (screenshot of garden1): count, size per sprite pixel relative to the dome, elevation range in degrees
const CLOUDS_PER_SPRITE: usize = 5;
const SIZE_PER_PIXEL: f32 = 0.0011;
const ELEVATION: (f32, f32) = (5.0, 50.0);
// soft edge of the clouds in pixels of the 256 image - the image itself has hard edges
const SOFT_EDGE: i32 = 3;

// center and half size in meters (glTF) - returns the scene object
pub fn write_sky(objects: &[Object], level: &str, center: [f32; 3], half_size: f32, level_extras: &Value, settings: &Settings, log: &Log) -> Result<Value>
{
    let mut glb = Glb::default();

    // the box in the background color - the fog color is written linear into the level extras
    let channel = |key: &str| level_extras[key].as_f64().unwrap_or(0.5);
    let color = [channel("fog_color_r"), channel("fog_color_g"), channel("fog_color_b"), 1.0];
    let background = glb.material(json!({
        "name": "background",
        "pbrMetallicRoughness": { "baseColorFactor": color, "metallicFactor": 0.0, "roughnessFactor": 1.0 },
        "extensions": { "KHR_materials_unlit": {} },
    }));
    glb.use_extension("KHR_materials_unlit");
    let box_primitive = inward_box(&mut glb, center, half_size, background);
    let mut primitives = vec![box_primitive];
    log.info(format!("  background ({:.3}, {:.3}, {:.3}) linear = FOGCOLOR · box {:.0} m", color[0], color[1], color[2], half_size * 2.0));

    if objects.iter().any(|o| o.kind == CLOUDS)
    {
        primitives.push(cloud_primitive(&mut glb, level, center, half_size, settings, log)?);
    }
    else
    {
        log.info("  no Clouds object - indoor level, only the background color");
    }

    let mesh = glb.mesh("sky", primitives);
    let root = glb.node(json!({ "name": "skybox", "mesh": mesh }));
    let path = settings.level_dir(level).join("skybox.glb");
    glb.save(&path, &[root])?;
    log.write(&path);

    Ok(json!({
        "source": relative_path(&settings.project_dir, &path),
        "uuid": format!("revolt-{}-skybox", level.to_lowercase()),
        "name": "skybox",
        "tags": ["revolt_skybox"],
        "options": { "settings": { "collision": false, "camera_collision": false, "frustum_culling": false, "occlusion_culling": false } },
        "position": [0.0, 0.0, 0.0],
        "rotation": [0.0, 0.0, 0.0],
        "scale": [1.0, 1.0, 1.0],
    }))
}

// the clouds and the sun on a dome inside the box, one primitive
fn cloud_primitive(glb: &mut Glb, level: &str, center: [f32; 3], half_size: f32, settings: &Settings, log: &Log) -> Result<Value>
{
    let source = find_file(&settings.game_dir.join("gfx"), "clouds.bmp").context("gfx/clouds.bmp missing")?;
    log.read(&source);
    let image = load_soft_sprites(&source, SOFT_EDGE)?;

    let texture = glb.png_texture_clamped(&image.png, "clouds.png");
    let cloud_material = glb.material(json!({
        "name": "clouds",
        "pbrMetallicRoughness": { "baseColorTexture": { "index": texture }, "metallicFactor": 0.0, "roughnessFactor": 1.0 },
        "alphaMode": "BLEND",
        "doubleSided": true,
        "extensions": { "KHR_materials_unlit": {} },
    }));

    // the same clouds for the same level every time
    let mut random = Random(level.bytes().fold(1469598103934665603u64, |hash, b| (hash ^ b as u64).wrapping_mul(1099511628211)) | 1);
    let dome = half_size * 0.95;
    let mut quads = Quads::new(center, image.width as f32, image.height as f32);

    // the sun first so it is drawn behind the clouds - where the light comes from
    let sun_length = SUN_DIRECTION.iter().map(|v| v * v).sum::<f32>().sqrt();
    let sun_direction = SUN_DIRECTION.map(|v| -v / sun_length);
    let sun_size = (SUN[2] - SUN[0] + 1) as f32 * dome * SIZE_PER_PIXEL / 2.0;
    quads.add(sun_direction, dome * 0.99, sun_size, sun_size, SUN);

    for sprite in SPRITES
    {
        for _ in 0..CLOUDS_PER_SPRITE
        {
            // more clouds low at the horizon than high up
            let azimuth = random.next() * TAU;
            let elevation = (ELEVATION.0 + random.next().powf(1.5) * (ELEVATION.1 - ELEVATION.0)).to_radians();
            let direction = [elevation.cos() * azimuth.cos(), elevation.sin(), elevation.cos() * azimuth.sin()];
            let size = 1.0 + random.next();
            // stretched sideways like in the game
            let half_x = (sprite[2] - sprite[0] + 1) as f32 * dome * SIZE_PER_PIXEL / 2.0 * size * 1.6;
            let half_y = (sprite[3] - sprite[1] + 1) as f32 * dome * SIZE_PER_PIXEL / 2.0 * size;
            quads.add(direction, dome * 0.95, half_x, half_y, sprite);
        }
    }

    log.info(format!("  {} clouds and the sun on a dome of {dome:.0} m, clouds {}-{} degrees high, sun where the light comes from (by eye)",
        SPRITES.len() * CLOUDS_PER_SPRITE, ELEVATION.0, ELEVATION.1));

    Ok(json!({
        "attributes": {
            "POSITION": glb.floats(&quads.positions, "VEC3", true),
            "NORMAL": glb.floats(&quads.normals, "VEC3", false),
            "TEXCOORD_0": glb.floats(&quads.uvs, "VEC2", false),
        },
        "indices": glb.indices(&quads.indices),
        "material": cloud_material,
    }))
}

// quads on a dome around the center, each facing the center
struct Quads
{
    center: [f32; 3],
    image_size: (f32, f32),
    positions: Vec<f32>,
    normals: Vec<f32>,
    uvs: Vec<f32>,
    indices: Vec<u32>,
}

impl Quads
{
    fn new(center: [f32; 3], width: f32, height: f32) -> Self
    {
        Self { center, image_size: (width, height), positions: vec![], normals: vec![], uvs: vec![], indices: vec![] }
    }

    // direction from the center (unit), distance, half width and height, pixel rectangle in the image
    fn add(&mut self, direction: [f32; 3], distance: f32, half_x: f32, half_y: f32, [x0, y0, x1, y1]: [u32; 4])
    {
        let right = normalize(cross([0.0, 1.0, 0.0], direction));
        let up = cross(direction, right);
        let middle = [0, 1, 2].map(|a| self.center[a] + direction[a] * distance);

        let base = (self.positions.len() / 3) as u32;
        for (x, y) in [(-1.0, 1.0), (1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)]
        {
            self.positions.extend([0, 1, 2].map(|a| middle[a] + right[a] * x * half_x + up[a] * y * half_y));
            self.normals.extend(direction.map(|v| -v));
        }
        let (w, h) = self.image_size;
        let (u0, v0, u1, v1) = (x0 as f32 / w, y0 as f32 / h, (x1 + 1) as f32 / w, (y1 + 1) as f32 / h);
        self.uvs.extend([u0, v0, u1, v0, u1, v1, u0, v1]);
        self.indices.extend([base, base + 3, base + 2, base, base + 2, base + 1]);
    }
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3]
{
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn normalize(v: [f32; 3]) -> [f32; 3]
{
    let length = v.iter().map(|c| c * c).sum::<f32>().sqrt().max(1e-6);
    v.map(|c| c / length)
}

// the 6 faces of a box seen from inside, one primitive
fn inward_box(glb: &mut Glb, center: [f32; 3], half: f32, material: usize) -> Value
{
    let faces: [([f32; 3], [f32; 3]); 6] = [
        ([0.0, 0.0, -1.0], [0.0, 1.0, 0.0]), ([0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
        ([-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]), ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([0.0, 1.0, 0.0], [0.0, 0.0, 1.0]), ([0.0, -1.0, 0.0], [0.0, 0.0, -1.0]),
    ];
    let (mut positions, mut normals, mut indices) = (vec![], vec![], vec![]);
    for (forward, up) in faces
    {
        let right = cross(forward, up);
        let base = (positions.len() / 3) as u32;
        for (u, v) in [(-1.0, 1.0), (1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)]
        {
            positions.extend([0, 1, 2].map(|a| center[a] + (forward[a] + up[a] * v + right[a] * u) * half));
            normals.extend(forward.map(|n| -n));
        }
        // counter clockwise from inside
        indices.extend([base, base + 3, base + 2, base, base + 2, base + 1]);
    }

    json!({
        "attributes": {
            "POSITION": glb.floats(&positions, "VEC3", true),
            "NORMAL": glb.floats(&normals, "VEC3", false),
        },
        "indices": glb.indices(&indices),
        "material": material,
    })
}

// xorshift - deterministic
struct Random(u64);

impl Random
{
    fn next(&mut self) -> f32
    {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
}

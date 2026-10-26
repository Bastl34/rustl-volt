// sky_ft/bk/lt/rt/tp/bt.bmp -> a box of 6 inward facing quads around the level, unlit, one glb per level
// the sky set: the own sky_*.bmp of the level, else the "level" flag of the Skybox object (type 55)

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::{json, Value};

use super::scene::relative_path;
use super::texture::load_plain;
use super::{Log, Settings};
use crate::glb::Glb;
use crate::revolt::find_file;
use crate::revolt::fob::Object;

const SKYBOX: i32 = 55;
// the "level" flag of the Skybox object: Toytanic day, Toytanic night, Wild West, Neighborhood (names from the game)
const SKY_SETS: [&str; 4] = ["ship1", "ship2", "wild_west1", "nhood1"];

// file, direction the face is seen in (glTF), up of the image
const FACES: [(&str, [f32; 3], [f32; 3]); 6] = [
    ("sky_ft", [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
    ("sky_bk", [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
    ("sky_lt", [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    ("sky_rt", [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    ("sky_tp", [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
    ("sky_bt", [0.0, -1.0, 0.0], [0.0, 0.0, -1.0]),
];

// the folder with the sky images of the level, None = the level has no sky (indoor)
pub fn sky_folder(game_dir: &Path, dir: &Path, objects: &[Object]) -> Option<PathBuf>
{
    if find_file(dir, "sky_ft.bmp").is_some()
    {
        return Some(dir.to_path_buf());
    }

    let set = objects.iter().find(|o| o.kind == SKYBOX).and_then(|o| SKY_SETS.get(usize::try_from(o.flags[0]).ok()?))?;
    let folder = game_dir.join("levels").join(set);
    find_file(&folder, "sky_ft.bmp").map(|_| folder)
}

// center and half size in meters (glTF space) - returns the scene object
pub fn write(folder: PathBuf, level: &str, center: [f32; 3], half_size: f32, settings: &Settings, log: &Log) -> Result<Value>
{
    let mut glb = Glb::default();
    let mut primitives = vec![];
    for (name, forward, up) in FACES
    {
        let Some(path) = find_file(&folder, &format!("{name}.bmp")) else
        {
            log.warn(format!("  {name}.bmp missing in {}", folder.display()));
            continue;
        };
        log.read(&path);
        let image = load_plain(&path)?;
        let texture = glb.png_texture_clamped(&image.png, &format!("{name}.png"));

        // seen from inside: right = forward × up
        let right = [forward[1] * up[2] - forward[2] * up[1], forward[2] * up[0] - forward[0] * up[2], forward[0] * up[1] - forward[1] * up[0]];
        // mirrored: the box at x mirrored, the triangles the other way round
        let sign = if settings.mirrored { [-1.0, 1.0, 1.0] } else { [1.0; 3] };
        let corner = |u: f32, v: f32| [0, 1, 2].map(|a| center[a] + (forward[a] + up[a] * v + right[a] * u) * sign[a] * half_size);
        let positions: Vec<f32> = [corner(-1.0, 1.0), corner(1.0, 1.0), corner(1.0, -1.0), corner(-1.0, -1.0)].concat();
        let normals: Vec<f32> = [0; 4].iter().flat_map(|_| [0, 1, 2].map(|a| -forward[a] * sign[a])).collect();
        let uvs = [0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0];

        let material = glb.material(json!({
            "name": name,
            "pbrMetallicRoughness": { "baseColorTexture": { "index": texture }, "metallicFactor": 0.0, "roughnessFactor": 1.0 },
            "extensions": { "KHR_materials_unlit": {} },
        }));
        glb.use_extension("KHR_materials_unlit");

        primitives.push(json!({
            "attributes": {
                "POSITION": glb.floats(&positions, "VEC3", true),
                "NORMAL": glb.floats(&normals, "VEC3", false),
                "TEXCOORD_0": glb.floats(&uvs, "VEC2", false),
            },
            // top left, bottom left, bottom right / top left, bottom right, top right - counter clockwise from inside
            "indices": glb.indices(if settings.mirrored { &[0, 2, 3, 0, 1, 2] } else { &[0, 3, 2, 0, 2, 1] }),
            "material": material,
        }));
        log.info(format!("  {name:<7} {}×{}  seen along ({}, {}, {})", image.width, image.height, forward[0], forward[1], forward[2]));
    }

    let mesh = glb.mesh("skybox", primitives);
    let root = glb.node(json!({ "name": "skybox", "mesh": mesh }));
    let path = settings.level_dir(level).join("skybox.glb");
    glb.save(&path, &[root])?;
    log.info(format!("  sky set {} · box {:.0} m around the level center ({:.1}, {:.1}, {:.1})",
        folder.file_name().unwrap_or_default().to_string_lossy(), half_size * 2.0, center[0], center[1], center[2]));
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

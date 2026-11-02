// the .fob objects the original simulates as rigid bodies -> a dynamic glb: the visible mesh without collision,
// hidden collision nodes from the .hul (a sphere for the balls), the trolley rolls on its wheels from Parameters.txt

use std::path::Path;

use anyhow::{Context, Result};
use serde_json::{json, Value};

use super::geometry::{to_gltf, MeshWriter};
use super::{mirror, Log};
use crate::glb::Glb;
use crate::revolt::{find_file, game_file, game_path};
use crate::revolt::hul::{self, Hull};
use crate::revolt::mesh::Mesh;
use crate::revolt::parameters;

pub const TROLLEY: i32 = 7;

// kg per mass unit of the game - the app drives the cars with the same factor (1.8 kg cars spun, 18 kg is the engine preset for their size)
pub const MASS_UNIT: f32 = 10.0;

pub struct Body
{
    // re-volt units, about kg - from the object inits of the game, the trolley from its Parameters.txt
    pub mass: f32,
    pub friction: f32,
    pub restitution: f32,
    pub sphere: bool,
    // stands still until something hits it - off: the waiting body is fixed in the step it is hit, a car hits it like a wall
    pub wait_for_hit: bool,
}

// the types the original moves when hit - mass from the game, friction and restitution guessed
pub fn body(kind: i32) -> Option<Body>
{
    let body = |mass, friction, restitution, sphere| Some(Body { mass, friction, restitution, sphere, wait_for_hit: false });
    match kind
    {
        1 => body(0.1, 0.6, 0.7, true),
        // low friction: the wheels roll, not in the simulation - the mass comes from Parameters.txt
        // placed up to 45 units above the floor - it drops at the start like in the original
        TROLLEY => body(4.6, 0.1, 0.0, false),
        15 => body(0.2, 0.6, 0.6, true),
        // the wobbly cones on the table of the menu: mass 0.5 like the game, the clockwork cars knock them over
        36 => body(0.5, 0.6, 0.1, false),
        43 => body(0.3, 0.6, 0.7, true),
        // moved by code in the game (wind, not decoded) - here a very light ball the cars push (user wish)
        52 => body(0.03, 0.6, 0.3, true),
        57 => body(1.2, 0.5, 0.1, false),
        58 => body(1.5, 0.5, 0.1, false),
        59 => body(1.6, 0.6, 0.1, false),
        66 => body(1.6, 0.6, 0.0, false),
        // the ABC blocks of toylite: 0.8 in the game - they stopped the cars like walls (user), a tenth of it
        67 => body(0.08, 0.6, 0.1, false),
        _ => None,
    }
}

pub struct Written
{
    pub root: usize,
    pub text: String,
}

// the nodes of a dynamic object below one root - the visible mesh is written already
pub fn write(glb: &mut Glb, writer: &mut MeshWriter, kind: i32, body: &Body, model_file: &str, stem: &str, mesh: &Mesh, visible_mesh: usize, mut extras: serde_json::Map<String, Value>, game_dir: &Path, scale: f32, mirrored: bool, log: &Log) -> Result<Written>
{
    let mut children = vec![glb.node(json!({ "name": format!("{stem}_mesh"), "mesh": visible_mesh, "extras": { "rustl_collision": false } }))];
    let mut mass = body.mass;
    let mut volume = 0.0;
    let mut text;

    if body.sphere
    {
        // a box around the model - the engine fits the sphere to its bounds
        let (min, max) = bounds(mesh.vertices.iter().map(|v| to_gltf(v.position).map(|c| c * scale)));
        let center = [0, 1, 2].map(|a| (min[a] + max[a]) / 2.0);
        let radius = (0..3).map(|a| (max[a] - min[a]) / 2.0).fold(0.0, f32::max);
        let material = collision_material(glb);
        let (positions, indices) = cube(center, radius);
        let shape = mesh_of(glb, "sphere", &positions, &indices, material);
        children.push(glb.node(json!({ "name": "collision_sphere", "mesh": shape, "extras": hidden() })));
        volume = 4.0 / 3.0 * std::f32::consts::PI * radius.powi(3);
        text = format!("sphere r {radius:.3} m");
    }
    else
    {
        let (hul_path, wheels) = match kind
        {
            TROLLEY => trolley(game_dir, glb, writer, scale, mirrored, &mut mass, &mut volume, &mut children, log)?,
            _ =>
            {
                // models/bottle.m -> models/Bottle.hul
                let relative = Path::new(model_file);
                let dir = game_dir.join(relative.parent().unwrap_or(Path::new("")));
                let name = format!("{}.hul", relative.file_stem().unwrap_or_default().to_string_lossy());
                (find_file(&dir, &name).with_context(|| format!("{model_file}: {name} missing"))?, 0)
            },
        };
        log.read(&hul_path);
        let mut hull = hul::load(&hul_path)?;
        if mirrored { mirror::hull(&mut hull); }
        let (nodes, hull_volume, skipped) = hull_nodes(glb, &hull, scale);
        volume += hull_volume;
        children.extend(nodes.iter().copied());
        text = format!("{} hulls ({} spheres not used)", nodes.len(), hull.spheres.len());
        if wheels > 0 { text += &format!(" + {wheels} wheels"); }
        if skipped > 0 { log.warn(format!("  {stem}: {skipped} hulls with broken values left out")); }
    }

    // the engine knows density only - mass / volume of the collision parts
    let density = mass * MASS_UNIT / volume.max(1e-6);
    extras.insert("revolt_mass".into(), json!(mass as f64));
    extras.insert("rustl_body_type".into(), json!("dynamic"));
    extras.insert("rustl_shape".into(), json!(if body.sphere { "sphere" } else { "convex_hull" }));
    extras.insert("rustl_combine_children".into(), json!(true));
    extras.insert("rustl_density".into(), json!(density as f64));
    extras.insert("rustl_friction".into(), json!(body.friction as f64));
    extras.insert("rustl_restitution".into(), json!(body.restitution as f64));
    extras.insert("rustl_react_on_first_hit".into(), json!(body.wait_for_hit));

    let start = if body.wait_for_hit { "waits for the first hit" } else { "simulated from the start" };
    text = format!("dynamic ({start}), {text}, mass {mass:.2} × {MASS_UNIT} kg, volume {:.5} m³ --> density {density:.0}, friction {}, restitution {}", volume, body.friction, body.restitution);
    let root = glb.node(json!({ "name": stem, "children": children, "extras": Value::Object(extras) }));
    Ok(Written { root, text })
}

// the wheels from Parameters.txt as colliding children, returns the hull file
fn trolley(game_dir: &Path, glb: &mut Glb, writer: &mut MeshWriter, scale: f32, mirrored: bool, mass: &mut f32, volume: &mut f32, children: &mut Vec<usize>, log: &Log) -> Result<(std::path::PathBuf, usize)>
{
    let dir = game_path(game_dir, "cars/trolley");
    let path = find_file(&dir, "parameters.txt").context("cars/trolley/Parameters.txt missing")?;
    log.read(&path);
    let params = parameters::load(&path)?;

    if let Some(body_mass) = params.block("BODY", None).and_then(|b| b.float("Mass"))
    {
        *mass = body_mass;
    }

    let mut wheels = 0;
    let mut written: Vec<(String, usize, f32)> = vec![];
    for wheel in params.blocks("WHEEL")
    {
        if wheel.flag("IsPresent") == Some(false) { continue; }
        let Some(model) = wheel.float("ModelNum").and_then(|n| params.model(n as i32)) else { continue; };
        let offset1 = wheel.vec3("Offset1").map(|o| if mirrored { mirror::point(o) } else { o }).unwrap_or_default();
        *mass += wheel.float("Mass").unwrap_or(0.0);

        // the same model for several wheels is written once
        let mesh = match written.iter().find(|(file, _, _)| *file == model)
        {
            Some((_, mesh, _)) => *mesh,
            None =>
            {
                let relative = Path::new(&model);
                let file = game_file(game_dir, &relative.to_string_lossy())
                    .with_context(|| format!("{model} missing"))?;
                log.read(&file);
                let (mut wheel_mesh, lods) = Mesh::load(&file)?;
                if mirrored { mirror::mesh(&mut wheel_mesh); }
                if lods > 1 { log.info(format!("  {model}: {lods} levels of detail - the first one is used")); }
                let mut primitives = Default::default();
                for polygon in &wheel_mesh.polygons { writer.add(&mut primitives, &wheel_mesh, polygon, None, -1); }
                let name = file.file_stem().unwrap_or_default().to_string_lossy().to_lowercase();
                let mesh = writer.write(glb, &name, primitives).with_context(|| format!("{model}: no polygons"))?.mesh;
                let (min, max) = bounds(wheel_mesh.vertices.iter().map(|v| v.position.map(|c| c * scale)));
                // a cylinder around the x axis
                let wheel_volume = std::f32::consts::PI * ((max[1] - min[1]) / 2.0).powi(2) * (max[0] - min[0]);
                written.push((model.clone(), mesh, wheel_volume));
                mesh
            },
        };
        *volume += written.iter().find(|(file, _, _)| *file == model).map_or(0.0, |w| w.2);

        // the model sits at Offset1, its tread is already modelled Offset2 aside (rc, cougar, trolley)
        let position = to_gltf(offset1).map(|v| v * scale);
        let index = wheel.index.unwrap_or(wheels as i32);
        children.push(glb.node(json!({
            "name": format!("wheel_{index}"),
            "mesh": mesh,
            "translation": position,
            "extras": {
                "revolt_wheel": index,
                "revolt_wheel_radius": wheel.float("Radius").unwrap_or(0.0) as f64 * scale as f64,
                "revolt_wheel_turnable": wheel.flag("IsTurnable").unwrap_or(false),
            },
        })));
        wheels += 1;
    }

    let coll = params.top.text("COLL").map(|c| c.replace('\\', "/")).unwrap_or_else(|| "cars/trolley/trolleyhull.hul".to_string());
    let relative = Path::new(&coll);
    let hul = game_file(game_dir, &relative.to_string_lossy())
        .with_context(|| format!("{coll} missing"))?;
    Ok((hul, wheels))
}

// one hidden mesh per convex hull, the volume of all in m³, the number of broken hulls
pub fn hull_nodes(glb: &mut Glb, hull: &Hull, scale: f32) -> (Vec<usize>, f32, usize)
{
    let material = collision_material(glb);
    let mut nodes = vec![];
    let mut volume = 0.0;
    let mut skipped = 0;

    for (i, convex) in hull.hulls.iter().enumerate()
    {
        let faces = convex.face_polygons();
        if !convex.is_finite() || faces.len() < 4
        {
            skipped += 1;
            continue;
        }

        let positions: Vec<[f32; 3]> = convex.vertices.iter().map(|&v| to_gltf(v).map(|c| c * scale)).collect();
        let center = [0, 1, 2].map(|a| positions.iter().map(|p| p[a]).sum::<f32>() / positions.len() as f32);
        let mut indices = vec![];
        for face in &faces
        {
            for k in 1..face.len() - 1
            {
                let (a, b, c) = (face[0], face[k], face[k + 1]);
                indices.extend_from_slice(&[a as u32, b as u32, c as u32]);
                // tetrahedron from the center
                volume += tetrahedron(center, positions[a], positions[b], positions[c]);
            }
        }

        let mesh = mesh_of(glb, &format!("hull_{i}"), &positions, &indices, material);
        nodes.push(glb.node(json!({ "name": format!("collision_hull_{i}"), "mesh": mesh, "extras": hidden() })));
    }

    (nodes, volume, skipped)
}

pub fn hidden() -> Value
{
    json!({ "rustl_visible": false, "rustl_camera_collision": false, "revolt_collision": true })
}

pub fn collision_material(glb: &mut Glb) -> usize
{
    glb.use_extension("KHR_materials_unlit");
    glb.material(json!({
        "name": "collision_hull",
        "pbrMetallicRoughness": { "baseColorFactor": [1.0, 0.0, 1.0, 1.0], "metallicFactor": 0.0, "roughnessFactor": 1.0 },
        "doubleSided": true,
        "extensions": { "KHR_materials_unlit": {} },
    }))
}

pub fn mesh_of(glb: &mut Glb, name: &str, positions: &[[f32; 3]], indices: &[u32], material: usize) -> usize
{
    let flat: Vec<f32> = positions.iter().flatten().copied().collect();
    let position = glb.floats(&flat, "VEC3", true);
    let indices = glb.indices(indices);
    glb.mesh(name, vec![json!({ "attributes": { "POSITION": position }, "indices": indices, "material": material })])
}

fn cube(center: [f32; 3], half: f32) -> (Vec<[f32; 3]>, Vec<u32>)
{
    let positions = (0..8).map(|i| [0, 1, 2].map(|a| center[a] + if i & (1 << a) != 0 { half } else { -half })).collect();
    let indices = vec![0, 2, 1, 1, 2, 3, 4, 5, 6, 5, 7, 6, 0, 1, 4, 1, 5, 4, 2, 6, 3, 3, 6, 7, 0, 4, 2, 2, 4, 6, 1, 3, 5, 3, 7, 5];
    (positions, indices)
}

pub fn bounds(points: impl Iterator<Item = [f32; 3]>) -> ([f32; 3], [f32; 3])
{
    points.fold(([f32::MAX; 3], [f32::MIN; 3]), |(min, max), p| ([0, 1, 2].map(|a| min[a].min(p[a])), [0, 1, 2].map(|a| max[a].max(p[a]))))
}

fn tetrahedron(o: [f32; 3], a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> f32
{
    let (a, b, c) = ([0, 1, 2].map(|i| a[i] - o[i]), [0, 1, 2].map(|i| b[i] - o[i]), [0, 1, 2].map(|i| c[i] - o[i]));
    (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0])).abs() / 6.0
}

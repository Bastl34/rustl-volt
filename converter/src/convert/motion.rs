// the scripted motion of the .fob objects (formats/README.md, Object motion) as glTF animations - one glb per moving object
// the motion is computed like the exe in game space (rows = local axes, turns) and written as looping keyframes; the copter flies in the app code, its glb only turns the rotors

use std::collections::BTreeMap;

use serde_json::{json, Value};

use super::geometry::{rotation_quat, to_gltf, MeshWriter};
use super::mirror;
use super::objects::ObjectModel;
use crate::glb::Glb;
use crate::revolt::fob::Object;
use crate::revolt::mesh::Mesh;

type Mat = [[f32; 3]; 3];
const IDENTITY: Mat = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
const TAU: f32 = std::f32::consts::TAU;

// keys per loop of a turning part - less than half a turn between two keys
const TURN_KEYS: usize = 64;
// keys per second of the motions that are no plain turn
const KEYS_PER_SECOND: f32 = 30.0;

pub const PLANET: i32 = 2;
pub const COPTER: i32 = 4;

// the types with a scripted motion
pub fn moves(kind: i32) -> bool
{
    matches!(kind, 0 | 2 | 3 | 4 | 5 | 8 | 10 | 11 | 12 | 13 | 17 | 40 | 56)
}

// ********** rotations in turns, row major (rows = local axes) **********

fn rot_x(turns: f32) -> Mat
{
    let (s, c) = (turns * TAU).sin_cos();
    [[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]]
}

fn rot_y(turns: f32) -> Mat
{
    let (s, c) = (turns * TAU).sin_cos();
    [[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]]
}

fn rot_z(turns: f32) -> Mat
{
    let (s, c) = (turns * TAU).sin_cos();
    [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]]
}

fn rot_xyz(a: f32, b: f32, c: f32) -> Mat
{
    mul(&mul(&rot_x(a), &rot_y(b)), &rot_z(c))
}

fn mul(a: &Mat, b: &Mat) -> Mat
{
    let mut out = [[0.0; 3]; 3];
    for i in 0..3
    {
        for j in 0..3
        {
            out[i][j] = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    out
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] { [a[0] - b[0], a[1] - b[1], a[2] - b[2]] }
fn length(v: [f32; 3]) -> f32 { (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt() }
fn scaled(v: [f32; 3], s: f32) -> [f32; 3] { v.map(|c| c * s) }
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] { [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]] }

// look from a to b, the right axis horizontal
fn look_at(from: [f32; 3], to: [f32; 3]) -> Mat
{
    let d = sub(to, from);
    let look = scaled(d, 1.0 / length(d).max(1e-6));
    let flat = (look[2] * look[2] + look[0] * look[0]).sqrt().max(1e-6);
    let right = [look[2] / flat, 0.0, -look[0] / flat];
    [right, cross(look, right), look]
}

// the local axes of a .fob object
pub fn object_axes(object: &Object) -> Mat
{
    let (up, look) = (object.up, object.look);
    [cross(up, look), up, look]
}

// ********** the node tree of a glb **********

struct Track
{
    times: Vec<f32>,
    translations: Vec<[f32; 3]>,
    // glTF quaternions [x, y, z, w]
    rotations: Vec<[f32; 4]>,
    weights: Vec<f32>,
}

struct Part
{
    name: String,
    parent: Option<usize>,
    // game space, in the frame of the parent
    translation: [f32; 3],
    rotation: Mat,
    mesh: Option<usize>,
    track: Option<Track>,
}

struct Tree
{
    parts: Vec<Part>,
}

impl Tree
{
    fn new(name: &str) -> Self
    {
        Self { parts: vec![Part { name: name.to_string(), parent: None, translation: [0.0; 3], rotation: IDENTITY, mesh: None, track: None }] }
    }

    fn add(&mut self, parent: usize, name: &str, translation: [f32; 3], rotation: Mat, mesh: Option<usize>, track: Option<Track>) -> usize
    {
        self.parts.push(Part { name: name.to_string(), parent: Some(parent), translation, rotation, mesh, track });
        self.parts.len() - 1
    }

    // mirrored: translations at x mirrored, rotations S·R·S - a quaternion (x, -y, -z, w)
    fn write(&self, glb: &mut Glb, scale: f32, mirrored: bool) -> usize
    {
        let position = |t: [f32; 3]| { let mut p = to_gltf(t).map(|v| v * scale); if mirrored { p[0] = -p[0]; } p };
        let turn = |q: [f32; 4]| if mirrored { [q[0], -q[1], -q[2], q[3]] } else { q };
        let mut nodes = vec![0; self.parts.len()];
        // the mesh leaf of each part - morph weights animate the node that has the mesh
        let mut leaves = vec![None; self.parts.len()];
        for i in (0..self.parts.len()).rev()
        {
            let part = &self.parts[i];
            let children: Vec<usize> = (0..self.parts.len()).filter(|&c| self.parts[c].parent == Some(i)).map(|c| nodes[c]).collect();
            let mut children = children;
            // the mesh on a leaf of its own - the engine hangs the children of a mesh with several materials onto the parent
            if let Some(mesh) = part.mesh
            {
                let mut leaf = json!({ "name": format!("{}_mesh", part.name), "mesh": mesh });
                // the small turning parts do not collide
                if !collides(&part.name) { leaf["extras"] = json!({ "rustl_collision": false }); }
                let leaf = glb.node(leaf);
                leaves[i] = Some(leaf);
                children.push(leaf);
            }
            let mut node = json!({
                "name": part.name,
                "translation": position(part.translation),
                "rotation": turn(rotation_quat(&part.rotation)),
            });
            if !children.is_empty() { node["children"] = json!(children); }
            // the meshes below follow the animation as kinematic convex hulls
            if i == 0 { node["extras"] = json!({ "rustl_body_type": "kinematic" }); }
            nodes[i] = glb.node(node);
        }

        for (i, part) in self.parts.iter().enumerate()
        {
            let Some(track) = &part.track else { continue; };
            let input = glb.keyframes(&track.times, "SCALAR");
            let mut channels = vec![];
            if !track.translations.is_empty()
            {
                let values: Vec<f32> = track.translations.iter().flat_map(|t| position(*t)).collect();
                channels.push((nodes[i], "translation", input, glb.keyframes(&values, "VEC3")));
            }
            if !track.rotations.is_empty()
            {
                let values: Vec<f32> = track.rotations.iter().flat_map(|q| turn(*q)).collect();
                channels.push((nodes[i], "rotation", input, glb.keyframes(&values, "VEC4")));
            }
            if !track.weights.is_empty()
            {
                channels.push((leaves[i].unwrap_or(nodes[i]), "weights", input, glb.keyframes(&track.weights, "SCALAR")));
            }
            glb.animation(&part.name, channels);
        }
        nodes[0]
    }
}

// propellers, rotors and heads turn inside their object - the wheels of the train collide (in the exe train.ncp covers them);
// the spinning barrel is a tunnel without a hull in the game - its convex hull was an invisible wall
fn collides(name: &str) -> bool
{
    !matches!(name, "propeller" | "rotor" | "tail_rotor" | "head" | "barrel")
}

// times 0..=duration, the last key equals the first - the loop closes
fn times(duration: f32, keys: usize) -> Vec<f32>
{
    (0..=keys).map(|k| duration * k as f32 / keys as f32).collect()
}

// a rotation over time, the quaternions kept on one side (no flip between two keys)
fn rotation_track(duration: f32, keys: usize, rotation: impl Fn(f32) -> Mat) -> Track
{
    let times = times(duration, keys);
    let mut rotations: Vec<[f32; 4]> = vec![];
    for &t in &times
    {
        let mut q = rotation_quat(&rotation(t));
        if let Some(previous) = rotations.last()
        {
            if (0..4).map(|i| q[i] * previous[i]).sum::<f32>() < 0.0 { q = q.map(|c| -c); }
        }
        rotations.push(q);
    }
    Track { times, translations: vec![], rotations, weights: vec![] }
}

fn translation_track(times: Vec<f32>, translation: impl Fn(f32) -> [f32; 3]) -> Track
{
    let translations = times.iter().map(|&t| translation(t)).collect();
    Track { times, translations, rotations: vec![], weights: vec![] }
}

// a constant turn: turns per second about one local axis - None if it stands still
fn spin(turns_per_second: f32, axis: fn(f32) -> Mat) -> Option<Track>
{
    if turns_per_second.abs() < 1e-4 { return None; }
    let duration = 1.0 / turns_per_second.abs();
    Some(rotation_track(duration, TURN_KEYS, move |t| axis(turns_per_second * t)))
}

// a phase from the object index - the exe takes rand(), a glb loop needs a fixed one
fn phase(index: usize) -> f32
{
    (index as f32 * 0.618034).fract()
}

// ********** the motions **********

pub struct Motion
{
    pub root: usize,
    // where the scene object goes (game units) - the start, the planet a moon orbits, ...
    pub anchor: [f32; 3],
    // false: the glb carries the rotation of the object, the scene object gets none
    pub keeps_rotation: bool,
    pub text: String,
    // what the app code needs for the sounds: phases, rates
    pub extras: serde_json::Map<String, Value>,
}

pub struct Models<'a>
{
    pub models: &'a [ObjectModel],
    // the main model and the parts of the object (object_types::parts), indices into models
    pub model: Option<usize>,
    pub parts: &'a [Option<usize>],
}

// writes the moving object into the glb - None if the type has no motion
// mirrored: the motion is built from the objects as the game has them and mirrored as a whole when written (the models are mirrored already)
pub fn write(glb: &mut Glb, writer: &mut MeshWriter, objects: &[Object], index: usize, models: Models, scale: f32, mirrored: bool) -> Option<Motion>
{
    let original: Vec<Object>;
    let objects = match mirrored
    {
        true => { original = objects.iter().map(|o| { let mut o = o.clone(); mirror::object(&mut o); o }).collect(); &original[..] },
        false => objects,
    };
    let object = &objects[index];
    let flags = object.flags;
    let w0 = object_axes(object);
    // one glb mesh per model, the wheels share theirs
    let mut written: BTreeMap<(usize, Option<usize>), Option<usize>> = BTreeMap::new();
    let mut mesh_of = |model: Option<usize>, target: Option<usize>| -> Option<usize>
    {
        let key = (model?, target);
        if let Some(mesh) = written.get(&key) { return *mesh; }
        let model = &models.models[key.0];
        let target: Option<&Mesh> = target.map(|t| &models.models[t].mesh);
        let mut primitives = BTreeMap::new();
        for polygon in &model.mesh.polygons
        {
            writer.add_with_target(&mut primitives, &model.mesh, polygon, None, -1, target);
        }
        let mesh = writer.write(glb, &model.stem, primitives).map(|written| written.mesh);
        written.insert(key, mesh);
        mesh
    };
    let part = |i: usize| models.parts.get(i).copied().flatten();

    let mut tree = Tree::new("motion");
    let mut anchor = object.position;
    let mut keeps_rotation = false;
    let mut extras = serde_json::Map::new();
    let text;

    match object.kind
    {
        0 =>
        {
            let speed = flags[0] as f32 / 32768.0 * 72.0;
            let base = tree.add(0, "rotation", [0.0; 3], w0, None, None);
            tree.add(base, "barrel", [0.0; 3], IDENTITY, mesh_of(models.model, None), spin(speed, rot_x));
            text = format!("spins about X, {speed:.2} turns/s, no collision (the level .ncp carries the cars through it)");
        },
        PLANET =>
        {
            // the chain of planets it orbits, the outermost first
            let find = |name: i32| objects.iter().position(|o| o.kind == PLANET && o.flags[0] == name);
            let mut chain = vec![index];
            while chain.len() < 8
            {
                let current = &objects[*chain.last().unwrap()];
                match find(current.flags[1]).filter(|&p| current.flags[1] != current.flags[0] && !chain.contains(&p))
                {
                    Some(parent) => chain.push(parent),
                    None => break,
                }
            }
            chain.reverse();
            anchor = objects[chain[0]].position;

            let mut parent_node = 0;
            for pair in chain.windows(2)
            {
                let (parent, child) = (&objects[pair[0]], &objects[pair[1]]);
                let offset = sub(child.position, parent.position);
                let distance = length(offset);
                let frame = mul(&rot_z(0.1), &look_at(parent.position, child.position));
                let speed = child.flags[2].abs() as f32 / 32768.0 * 72.0;
                let track = (speed > 1e-4).then(||
                {
                    // frame turned about its local X: the offset (0, 0, distance) runs along up / look
                    let duration = 1.0 / speed;
                    translation_track(times(duration, TURN_KEYS), |t|
                    {
                        let (s, c) = (speed * t * TAU).sin_cos();
                        [0, 1, 2].map(|i| distance * (s * frame[1][i] + c * frame[2][i]))
                    })
                });
                parent_node = tree.add(parent_node, &format!("orbit_{}", child.flags[0]), offset, IDENTITY, None, track);
            }

            let spin_speed = flags[3] as f32 / 4096.0 * 72.0;
            let base = tree.add(parent_node, "rotation", [0.0; 3], w0, None, None);
            tree.add(base, "spin", [0.0; 3], IDENTITY, mesh_of(models.model, None), spin(spin_speed, rot_y));
            text = format!("planet {}: orbits {} (chain of {}), spins {spin_speed:.3} turns/s", flags[0], flags[1], chain.len() - 1);
        },
        3 =>
        {
            let speed = flags[0] as f32 / 16384.0 * 72.0;
            let radius = flags[1] as f32;
            let bank = mul(&rot_z(flags[2] as f32 / 512.0), &rot_y(if speed > 0.0 { 0.25 } else { 0.75 }));
            let orbit = tree.add(0, "orbit", [0.0; 3], IDENTITY, None, spin(speed, rot_y));
            let body = tree.add(orbit, "plane", [0.0, 0.0, radius], bank, mesh_of(models.model, None), None);
            tree.add(body, "propeller", [0.0, -120.0, 110.0], IDENTITY, mesh_of(part(0), None), spin(2.0, rot_z));
            text = format!("circles {:.1} m around the start in {:.1} s, propeller 2 turns/s", radius * scale, 1.0 / speed.abs().max(1e-6));
        },
        COPTER =>
        {
            // the app code flies the scene object - the glb has the body and turns the rotors
            keeps_rotation = true;
            let body = tree.add(0, "copter", [0.0; 3], IDENTITY, mesh_of(models.model, None), None);
            tree.add(body, "rotor", [0.0, -340.0, 66.0], IDENTITY, mesh_of(part(0), None), spin(2.0, rot_y));
            tree.add(body, "tail_rotor", [28.0, -252.0, -200.0], IDENTITY, mesh_of(part(1), None), spin(2.5, rot_x));
            text = "rotors 2 / 2.5 turns/s, the flight is app code".to_string();
        },
        5 =>
        {
            let base = tree.add(0, "rotation", [0.0; 3], w0, None, None);
            tree.add(base, "body", [0.0; 3], IDENTITY, mesh_of(models.model, None), None);
            // the head morphs to dragon3 and back every 8 s
            let pulse = |t: f32| 0.03 * (t * 5.0 * std::f32::consts::PI).sin();
            let times = times(8.0, (8.0 * KEYS_PER_SECOND) as usize);
            let weights = times.iter().map(|&t| match t
            {
                t if t < 2.0 => t / 2.0,
                t if t < 2.2 => 1.0 + pulse(t - 2.0),
                t if t <= 4.0 => 1.0,
                t if t <= 6.0 => (6.0 - t) / 2.0,
                t if t < 6.2 => -pulse(t - 6.0),
                _ => 0.0,
            }).collect();
            let head = mesh_of(part(0), part(1));
            tree.add(base, "head", [0.0; 3], IDENTITY, head, Some(Track { times, translations: vec![], rotations: vec![], weights }));
            text = "head opens and closes every 8 s (morph dragon2 -> dragon3)".to_string();
        },
        8 =>
        {
            // the exe takes random periods (rock 4-6 s, bob 2-3 s) - here ones that fit into one 24 s loop
            let duration = 24.0;
            let (t1, t3, t2) = (duration / [4.0, 5.0, 6.0][index % 3], duration / [5.0, 6.0, 4.0][index % 3], duration / [8.0, 9.0, 10.0, 11.0, 12.0][index % 5]);
            let keys = times(duration, (duration * KEYS_PER_SECOND) as usize);
            let bob = tree.add(0, "bob", [0.0; 3], IDENTITY, None, Some(translation_track(keys.clone(), |t| [0.0, 15.0 * (TAU * t / t2).sin(), 0.0])));
            let base = tree.add(bob, "rotation", [0.0; 3], w0, None, None);
            let rock = rotation_track(duration, keys.len() - 1, |t| rot_xyz((TAU * t / t1).sin() / 90.0, (TAU * t / t3).sin() / 90.0, 0.0));
            tree.add(base, "rock", [0.0; 3], IDENTITY, mesh_of(models.model, None), Some(rock));
            text = format!("bobs ({t2:.2} s) and rocks ({t1:.1} / {t3:.1} s)");
        },
        10 =>
        {
            tree.add(0, "spin", [0.0; 3], IDENTITY, mesh_of(models.model, None), spin(0.75, rot_y));
            text = "turns 0.75 turns/s".to_string();
        },
        11 =>
        {
            let offset = phase(index) * TAU;
            let track = translation_track(times(TAU, (TAU * KEYS_PER_SECOND) as usize), |t| [0.0, 16.0 * (t + offset).sin(), 0.0]);
            let bob = tree.add(0, "bob", [0.0; 3], IDENTITY, None, Some(track));
            tree.add(bob, "balloon", [0.0; 3], w0, mesh_of(models.model, None), None);
            text = "floats up and down (6.3 s)".to_string();
        },
        12 =>
        {
            let offset = phase(index) * TAU;
            extras.insert("revolt_phase".into(), json!(offset));
            let duration = TAU / 4.0;
            let base = tree.add(0, "rotation", [0.0; 3], w0, None, None);
            let rock = rotation_track(duration, (duration * KEYS_PER_SECOND) as usize, |t| rot_x(0.02 * (4.0 * t + offset).sin()));
            tree.add(base, "rock", [0.0; 3], IDENTITY, mesh_of(models.model, None), Some(rock));
            text = "rocks ±7.2° (1.57 s)".to_string();
        },
        13 =>
        {
            // along world -z to -11500, then back to -400 (the track of toy2)
            let (start, end, restart) = (object.position[2], -11500.0, -400.0);
            let speed = 200.0;
            let duration = (restart - end) / speed;
            let reach = ((start - end) / speed).clamp(0.0, duration - 0.002);
            let track = Track
            {
                times: vec![0.0, reach, reach + 0.001, duration],
                translations: vec![[0.0; 3], [0.0, 0.0, end - start], [0.0, 0.0, restart - start], [0.0; 3]],
                rotations: vec![],
                weights: vec![],
            };
            let drive = tree.add(0, "drive", [0.0; 3], IDENTITY, None, Some(track));
            tree.add(drive, "train", [0.0; 3], w0, mesh_of(models.model, None), None);
            let wheels = [([98.0, -136.0, 330.0], 0, -0.3), ([-98.0, -136.0, 330.0], 0, -0.3), ([98.0, -92.0, -204.0], 1, -0.5), ([-98.0, -92.0, -204.0], 1, -0.5)];
            for (i, (offset, model, speed)) in wheels.into_iter().enumerate()
            {
                let base = tree.add(drive, &format!("wheel_{i}"), offset, w0, None, None);
                tree.add(base, &format!("wheel_{i}_spin"), [0.0; 3], IDENTITY, mesh_of(part(model), None), spin(speed, rot_x));
            }
            text = format!("drives along -z at 2 m/s, {duration:.1} s loop");
        },
        17 =>
        {
            // a small fixed turn every frame - at 60 fps a turn about a fixed local axis
            let step = rotation_quat(&rot_xyz(0.001, 0.001, 0.0));
            let angle = 2.0 * step[3].clamp(-1.0, 1.0).acos();
            let axis = scaled([step[0], step[1], step[2]], 1.0 / (angle / 2.0).sin().max(1e-9));
            let speed = angle * 60.0;
            let duration = TAU / speed;
            let base = tree.add(0, "rotation", [0.0; 3], w0, None, None);
            let times = times(duration, TURN_KEYS);
            let mut rotations: Vec<[f32; 4]> = vec![];
            for &t in &times
            {
                let (s, c) = (speed * t / 2.0).sin_cos();
                let mut q = [axis[0] * s, axis[1] * s, axis[2] * s, c];
                if let Some(previous) = rotations.last()
                {
                    if (0..4).map(|i| q[i] * previous[i]).sum::<f32>() < 0.0 { q = q.map(|c| -c); }
                }
                rotations.push(q);
            }
            tree.add(base, "tumble", [0.0; 3], IDENTITY, mesh_of(models.model, None), Some(Track { times, translations: vec![], rotations, weights: vec![] }));
            text = format!("tumbles, {duration:.1} s per turn (60 fps)");
        },
        40 =>
        {
            // -0.2 turns/s and a ratchet - 11 ratchet cycles in the 5 s loop (the exe: 11.4)
            let duration = 5.0;
            let ratchet = 11.0 * TAU / duration;
            extras.insert("revolt_ratchet".into(), json!(ratchet));
            let base = tree.add(0, "rotation", [0.0; 3], w0, mesh_of(models.model, None), None);
            let head = rotation_track(duration, (duration * KEYS_PER_SECOND) as usize, |t| rot_y(-0.2 * t + 0.04 * (ratchet * t).sin()));
            tree.add(base, "head", [0.0, -38.0, 0.0], IDENTITY, mesh_of(part(0), None), Some(head));
            text = "head turns -0.2 turns/s with a ratchet".to_string();
        },
        56 =>
        {
            let sign = if flags[0] == 0 { -1.0 } else { 1.0 };
            let base = tree.add(0, "rotation", [0.0; 3], w0, None, None);
            let track = Track { times: vec![0.0, 1.5, 3.0], translations: vec![[0.0; 3], [400.0 * sign, 0.0, 0.0], [0.0; 3]], rotations: vec![], weights: vec![] };
            tree.add(base, "slide", [0.0; 3], IDENTITY, mesh_of(models.model, None), Some(track));
            text = format!("slides {} units along X and back in 3 s", 400.0 * sign);
        },
        _ => return None,
    }

    let root = tree.write(glb, scale, mirrored);
    if mirrored { anchor = mirror::point(anchor); }
    Some(Motion { root, anchor, keeps_rotation, text, extras })
}

// the scene object of a moving object: its own glb, placed at the anchor
pub fn place(scene_object: &mut Value, motion: &Motion, source: String, scale: f32)
{
    scene_object["source"] = json!(source);
    scene_object["position"] = json!(to_gltf(motion.anchor).map(|v| v * scale));
    if !motion.keeps_rotation
    {
        scene_object["rotation_quat"] = json!([0.0, 0.0, 0.0, 1.0]);
    }
    if let Some(tags) = scene_object["tags"].as_array_mut()
    {
        tags.push(json!("revolt_animated"));
    }
    // meters per game unit - the copter flies in the app code with the units of the game
    scene_object["extras"]["revolt_scale"] = json!(scale);
    for (key, value) in &motion.extras
    {
        scene_object["extras"][key] = value.clone();
    }
    scene_object["options"]["settings"]["collision"] = json!(true);
}

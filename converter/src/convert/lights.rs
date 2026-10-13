// the lights of the .lit baked into the vertex colors like the game does it at load (formats/README.md, .lit)
// world polygons, instances (a lit instance gets a model of its own) and the objects that do not move

use super::instances::{LevelInstances, Model};
use super::objects::{LevelObjects, ObjectModel};
use super::{bodies, motion, Log};
use crate::revolt::fin::FIN_HIDE;
use crate::revolt::lit::{Light, LIGHT_NO_MODELS, LIGHT_OMNI, LIGHT_OMNI_NORMAL, LIGHT_SPOT, LIGHT_SPOT_NORMAL, LIGHT_WORLD};
use crate::revolt::mesh::Mesh;
use crate::revolt::world::World;

// instance flag: not lit by the lights
const FIN_NO_LIGHTS: u8 = 8;

// a mesh in the level: its origin and its axes (rows right, up, look) - the world has none
#[derive(Clone, Copy)]
pub struct Placement
{
    pub position: [f32; 3],
    pub axes: [[f32; 3]; 3],
}

impl Placement
{
    pub const IDENTITY: Placement = Placement { position: [0.0; 3], axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] };

    fn point(&self, p: [f32; 3]) -> [f32; 3]
    {
        let d = self.direction(p);
        [d[0] + self.position[0], d[1] + self.position[1], d[2] + self.position[2]]
    }

    fn direction(&self, v: [f32; 3]) -> [f32; 3]
    {
        let a = &self.axes;
        [0, 1, 2].map(|i| v[0] * a[0][i] + v[1] * a[1][i] + v[2] * a[2][i])
    }
}

pub fn world_lights(lights: &[Light]) -> Vec<&Light>
{
    lights.iter().filter(|light| light.flag & LIGHT_WORLD != 0).collect()
}

// the lights that reach a mesh placed there (bounding sphere test)
pub fn reaching<'a>(lights: &[&'a Light], mesh: &Mesh, placement: &Placement) -> Vec<&'a Light>
{
    let points: Vec<[f32; 3]> = mesh.vertices.iter().map(|v| placement.point(v.position)).collect();
    if points.is_empty() { return vec![]; }
    let n = points.len() as f32;
    let center = [0, 1, 2].map(|i| points.iter().map(|p| p[i]).sum::<f32>() / n);
    let radius = points.iter().map(|p| distance(*p, center)).fold(0.0, f32::max);
    lights.iter().copied().filter(|light| distance(light.position, center) < light.reach + radius).collect()
}

// adds the lights to the corner colors; true if a color changed
pub fn apply(mesh: &mut Mesh, placement: &Placement, lights: &[&Light]) -> bool
{
    let mut changed = false;
    let positions: Vec<[f32; 3]> = mesh.vertices.iter().map(|v| placement.point(v.position)).collect();
    let normals: Vec<[f32; 3]> = mesh.vertices.iter().map(|v| placement.direction(v.normal)).collect();
    for polygon in &mut mesh.polygons
    {
        for corner in 0..polygon.corners()
        {
            let index = polygon.indices[corner] as usize;
            let (Some(&position), Some(&normal)) = (positions.get(index), normals.get(index)) else { continue; };
            for light in lights
            {
                let amount = amount(light, position, normal);
                if amount <= 0.0 { continue; }
                polygon.colors[corner] = add(polygon.colors[corner], light.color, amount);
                changed = true;
            }
        }
    }
    changed
}

// 0..1: fall off with the squared distance, turned by the surface normal, inside the cone of a spot
fn amount(light: &Light, position: [f32; 3], normal: [f32; 3]) -> f32
{
    let delta = [0, 1, 2].map(|i| light.position[i] - position[i]);
    let squared = dot(delta, delta);
    let reach_squared = light.reach * light.reach;
    if squared >= reach_squared || squared <= 0.0 { return 0.0; }
    let length = squared.sqrt();
    let mut amount = 1.0 - squared / reach_squared;

    if light.kind == LIGHT_OMNI_NORMAL || light.kind == LIGHT_SPOT_NORMAL
    {
        let facing = dot(delta, normal);
        if facing <= 0.0 { return 0.0; }
        amount *= facing / length;
    }
    if (light.kind == LIGHT_SPOT || light.kind == LIGHT_SPOT_NORMAL) && light.cone > 0.0
    {
        let along = -dot(delta, light.axes[2]) / length;
        let cone = (along - 1.0) * (180.0 / light.cone) + 1.0;
        if cone <= 0.0 { return 0.0; }
        amount *= cone;
    }
    amount
}

// the game adds color * round(amount * 256) / 256 per channel and clamps to 0..255 (0xAARRGGBB)
fn add(color: u32, light: [f32; 3], amount: f32) -> u32
{
    let scale = (amount * 256.0).round() as i32;
    let channel = |shift: u32, value: f32| -> u32
    {
        let old = ((color >> shift) & 0xff) as i32;
        (((value as i32 * scale) >> 8) + old).clamp(0, 255) as u32
    };
    (color & 0xff00_0000) | (channel(16, light[0]) << 16) | (channel(8, light[1]) << 8) | channel(0, light[2])
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32
{
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32
{
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    dot(d, d).sqrt()
}

// bakes the lights of the .lit into the world, the instances and the objects that stand still
pub fn bake(world: &mut World, instances: &mut LevelInstances, objects: &mut LevelObjects, lights: &[Light], log: &Log)
{
    if lights.is_empty()
    {
        log.info("  no lights");
        return;
    }
    for (i, light) in lights.iter().enumerate()
    {
        let kind = match light.kind { LIGHT_OMNI => "omni", LIGHT_OMNI_NORMAL => "omni (normal)", LIGHT_SPOT => "spot", LIGHT_SPOT_NORMAL => "spot (normal)", _ => "unknown" };
        log.info(format!("  light {i}: {kind} at ({:.0}, {:.0}, {:.0}), reach {:.0}, cone {:.0}°, color ({:.0}, {:.0}, {:.0}), flag {}, speed {}",
            light.position[0], light.position[1], light.position[2], light.reach, light.cone, light.color[0], light.color[1], light.color[2], light.flag, light.speed));
    }

    // the world: the lights with the world flag
    let world_lights = world_lights(lights);
    let mut world_meshes = 0;
    for world_mesh in &mut world.meshes
    {
        let near = reaching(&world_lights, &world_mesh.mesh, &Placement::IDENTITY);
        if !near.is_empty() && apply(&mut world_mesh.mesh, &Placement::IDENTITY, &near) { world_meshes += 1; }
    }

    // instances and objects: every light without the "no models" flag - a lit one gets a model of its own
    let model_lights: Vec<&Light> = lights.iter().filter(|light| light.flag & LIGHT_NO_MODELS == 0).collect();
    let mut lit_instances = 0;
    for (index, instance) in instances.instances.iter().enumerate()
    {
        if instance.flag & (FIN_HIDE | FIN_NO_LIGHTS) != 0 { continue; }
        let Some(m) = instances.model_of[index] else { continue; };
        let placement = Placement { position: instance.position, axes: instance.axes };
        let near = reaching(&model_lights, &instances.models[m].mesh, &placement);
        if near.is_empty() { continue; }
        let mut mesh = instances.models[m].mesh.clone();
        if !apply(&mut mesh, &placement, &near) { continue; }
        let model = &instances.models[m];
        let lit = Model { stem: format!("{}_lit{index:03}", model.stem), mesh, ncp: model.ncp.clone() };
        instances.models.push(lit);
        instances.model_of[index] = Some(instances.models.len() - 1);
        lit_instances += 1;
    }

    let mut lit_objects = 0;
    for (index, object) in objects.objects.iter().enumerate()
    {
        if motion::moves(object.kind) || bodies::body(object.kind).is_some() { continue; }
        let Some(m) = objects.model_of[index] else { continue; };
        let (up, look) = (object.up, object.look);
        let right = [up[1] * look[2] - up[2] * look[1], up[2] * look[0] - up[0] * look[2], up[0] * look[1] - up[1] * look[0]];
        let placement = Placement { position: object.position, axes: [right, up, look] };
        let near = reaching(&model_lights, &objects.models[m].mesh, &placement);
        if near.is_empty() { continue; }
        let mut mesh = objects.models[m].mesh.clone();
        if !apply(&mut mesh, &placement, &near) { continue; }
        let model = &objects.models[m];
        let lit = ObjectModel { file: model.file.clone(), stem: format!("{}_lit{index:03}", model.stem), mesh };
        objects.models.push(lit);
        objects.model_of[index] = Some(objects.models.len() - 1);
        lit_objects += 1;
    }
    log.info(format!("  baked into {world_meshes} world meshes, {lit_instances} instances and {lit_objects} objects (each lit one with a model of its own)"));
}

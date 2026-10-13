// .ncp -> collision nodes: one mesh per surface material, the friction from the grip of the surface

use std::collections::{BTreeMap, HashMap};

use serde_json::{json, Value};

use super::geometry::{linear_rgba, to_gltf};
use super::Log;
use crate::glb::Glb;
use crate::revolt::ncp::*;

// a normal surface in the engine - the vehicle grip scales with friction / 0.7
const ENGINE_FRICTION: f32 = 0.7;

#[derive(Default)]
pub struct CollisionStats
{
    pub polygons: usize,
    pub triangles: usize,
    pub camera_only: usize,
    pub degenerate: usize,
    pub clamped: usize,
    pub unknown_surface: usize,
}

impl CollisionStats
{
    pub fn add(&mut self, other: &CollisionStats)
    {
        self.polygons += other.polygons;
        self.triangles += other.triangles;
        self.camera_only += other.camera_only;
        self.degenerate += other.degenerate;
        self.clamped += other.clamped;
        self.unknown_surface += other.unknown_surface;
    }

    pub fn log(&self, log: &Log)
    {
        log.info(format!("  {} polygons --> {} triangles", self.polygons, self.triangles));
        if self.camera_only > 0 { log.info(format!("  {} camera only polygons left out (the camera collides with the visible mesh)", self.camera_only)); }
        if self.clamped > 0 { log.info(format!("  {} polygons with a corner pulled into their bounding box (non planar quads)", self.clamped)); }
        if self.degenerate > 0 { log.warn(format!("  {} polygons left out - two of their planes are parallel", self.degenerate)); }
        if self.unknown_surface > 0 { log.warn(format!("  {} polygons with an unknown surface - treated as default", self.unknown_surface)); }
    }
}

#[derive(Default)]
struct Part
{
    positions: Vec<f32>,
    indices: Vec<u32>,
    welded: HashMap<[i64; 3], u32>,
    polygons: usize,
    flags: i32,
}

impl Part
{
    // shared corners become one vertex - the trimesh then knows its neighbours
    fn vertex(&mut self, p: [f32; 3]) -> u32
    {
        let key = p.map(|v| (v as f64 * 1e4).round() as i64);
        *self.welded.entry(key).or_insert_with(||
        {
            self.positions.extend_from_slice(&p);
            (self.positions.len() / 3 - 1) as u32
        })
    }
}

// the nodes go into the glb, returns their indices - hidden: invisible nodes inside of a visible model
pub fn add_nodes(glb: &mut Glb, ncp: &Ncp, scale: f32, hidden: bool, stats: &mut CollisionStats) -> Vec<usize>
{
    // per surface, no skid polygons apart
    let mut parts: BTreeMap<(usize, bool), Part> = BTreeMap::new();

    for polygon in &ncp.polygons
    {
        stats.polygons += 1;
        if polygon.has(NCP_CAMERA_ONLY)
        {
            stats.camera_only += 1;
            continue;
        }
        let surface = match usize::try_from(polygon.surface).ok().filter(|&s| s < SURFACES.len())
        {
            Some(surface) => surface,
            None => { stats.unknown_surface += 1; 0 },
        };
        let Some((corners, clamped)) = polygon.corners() else
        {
            stats.degenerate += 1;
            continue;
        };
        if clamped { stats.clamped += 1; }

        let points: Vec<[f32; 3]> = corners.iter().map(|&c| to_gltf(c).map(|v| v * scale)).collect();
        let normal = to_gltf(polygon.plane.normal);
        let part = parts.entry((surface, polygon.has(NCP_NO_SKID))).or_default();
        part.polygons += 1;
        part.flags |= polygon.flags;

        for i in 1..points.len() - 1
        {
            let (a, mut b, mut c) = (points[0], points[i], points[i + 1]);
            let cross = cross(sub(b, a), sub(c, a));
            if dot(cross, cross) < 1e-12
            {
                continue;
            }
            // counter clockwise around the plane normal
            if dot(cross, normal) < 0.0
            {
                std::mem::swap(&mut b, &mut c);
            }
            let indices = [part.vertex(a), part.vertex(b), part.vertex(c)];
            part.indices.extend_from_slice(&indices);
            stats.triangles += 1;
        }
    }

    let mut nodes = vec![];
    for ((surface_index, no_skid), part) in parts
    {
        if part.indices.is_empty()
        {
            continue;
        }
        let surface = &SURFACES[surface_index];
        let name = if no_skid { format!("collision_{}_noskid", surface.name) } else { format!("collision_{}", surface.name) };

        let [r, g, b] = surface.color;
        let material = glb.material(json!({
            "name": name,
            "pbrMetallicRoughness": { "baseColorFactor": linear_rgba(u32::from_be_bytes([0, r, g, b]), Some(1.0)), "metallicFactor": 0.0, "roughnessFactor": 1.0 },
            "doubleSided": true,
            "extensions": { "KHR_materials_unlit": {} },
        }));
        glb.use_extension("KHR_materials_unlit");

        let position = glb.floats(&part.positions, "VEC3", true);
        let indices = glb.indices(&part.indices);
        let mesh = glb.mesh(&name, vec![json!({ "attributes": { "POSITION": position }, "indices": indices, "material": material })]);

        let mut extras = serde_json::Map::new();
        extras.insert("revolt_collision".into(), json!(true));
        extras.insert("revolt_surface".into(), json!(surface_index));
        extras.insert("revolt_surface_name".into(), json!(surface.name));
        extras.insert("revolt_grip".into(), json!(surface.grip as f64));
        extras.insert("revolt_roughness".into(), json!(surface.roughness as f64));
        extras.insert("revolt_hardness".into(), json!(surface.hardness as f64));
        extras.insert("revolt_surface_flags".into(), json!(surface.flags));
        extras.insert("revolt_no_skid".into(), json!(no_skid));
        extras.insert("revolt_dusty".into(), json!(surface.flags & SURFACE_DUSTY != 0));
        extras.insert("revolt_corrugated".into(), json!(surface.flags & SURFACE_CORRUGATED != 0));
        extras.insert("revolt_out_of_bounds".into(), json!(surface.flags & SURFACE_OUT_OF_BOUNDS != 0));
        extras.insert("revolt_corrugation".into(), json!(surface.corrugation));
        extras.insert("revolt_dust".into(), json!(surface.dust));
        extras.insert("revolt_ncp_flags".into(), json!(part.flags));
        extras.insert("revolt_polygons".into(), json!(part.polygons));
        if surface.flags & SURFACE_MOVES != 0
        {
            // m/s in glTF space - the engine has no moving surfaces yet
            for (axis, v) in ["x", "y", "z"].iter().zip(to_gltf(surface.velocity))
            {
                extras.insert(format!("revolt_conveyor_{axis}"), json!((v * scale) as f64));
            }
        }
        extras.insert("rustl_friction".into(), json!((surface.grip * ENGINE_FRICTION) as f64));
        extras.insert("rustl_camera_collision".into(), json!(false));
        if hidden
        {
            extras.insert("rustl_visible".into(), json!(false));
        }

        nodes.push(glb.node(json!({ "name": name, "mesh": mesh, "extras": Value::Object(extras) })));
    }

    nodes
}

// what the file contains, before the conversion
pub fn log_ncp(ncp: &Ncp, log: &Log)
{
    let quads = ncp.polygons.iter().filter(|p| p.has(NCP_QUAD)).count();
    log.info(format!("  .ncp      {} polygons ({quads} quads, {} triangles)", ncp.polygons.len(), ncp.polygons.len() - quads));

    let flags: Vec<String> = NCP_FLAG_NAMES.iter().skip(1)
        .map(|&(flag, name)| (name, ncp.polygons.iter().filter(|p| p.has(flag)).count()))
        .filter(|&(_, count)| count > 0)
        .map(|(name, count)| format!("{name} {count}"))
        .collect();
    if !flags.is_empty() { log.info(format!("  flags     {}", flags.join(" · "))); }

    let mut surfaces: BTreeMap<i32, usize> = BTreeMap::new();
    for polygon in &ncp.polygons { *surfaces.entry(polygon.surface).or_default() += 1; }
    for (surface, count) in surfaces
    {
        match usize::try_from(surface).ok().and_then(|s| SURFACES.get(s))
        {
            Some(s) => log.info(format!("  surface   {surface:>2} {:<17} {count:>6} polygons  grip {:.2} --> friction {:.3}", s.name, s.grip, s.grip * ENGINE_FRICTION)),
            None => log.warn(format!("  surface   {surface:>2} {:<17} {count:>6} polygons", "unknown")),
        }
    }

    if let Some(grid) = &ncp.grid
    {
        log.info(format!("  grid      {} × {} cells of {} at ({}, {}), {} entries - not needed, rapier has its own",
            grid.x_count, grid.z_count, grid.raster, grid.x0, grid.z0, grid.entries));
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] { [a[0] - b[0], a[1] - b[1], a[2] - b[2]] }
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 { a[0] * b[0] + a[1] * b[1] + a[2] * b[2] }
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] { [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]] }

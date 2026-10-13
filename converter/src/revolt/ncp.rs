// .ncp - collision: convex polygons stored as planes with a surface material, the level one adds a lookup grid

use std::path::Path;

use anyhow::{bail, Result};

use super::reader::Reader;

// polygon flags (from the re-volt blender add-on) - 1024 and 2048 are unknown
pub const NCP_QUAD: i32 = 1;
pub const NCP_DOUBLE: i32 = 2;
pub const NCP_OBJECT_ONLY: i32 = 4;
pub const NCP_CAMERA_ONLY: i32 = 8;
pub const NCP_NO_SKID: i32 = 32;

pub const NCP_FLAG_NAMES: [(i32, &str); 9] = [
    (NCP_QUAD, "quad"), (NCP_DOUBLE, "double"), (NCP_OBJECT_ONLY, "object only"), (NCP_CAMERA_ONLY, "camera only"), (16, "non planar"),
    (NCP_NO_SKID, "no skid"), (64, "oil"), (1024, "unknown 1024"), (2048, "unknown 2048"),
];

// surface flags in revolt.exe - 1 and 2 are not clear (spark / skid?)
pub const SURFACE_OUT_OF_BOUNDS: u32 = 4;
pub const SURFACE_CORRUGATED: u32 = 8;
pub const SURFACE_MOVES: u32 = 16;
pub const SURFACE_DUSTY: u32 = 32;

// the surface material table of revolt.exe (27 entries)
pub struct Surface
{
    pub name: &'static str,
    pub flags: u32,
    pub roughness: f32,
    pub grip: f32,
    pub hardness: f32,
    pub corrugation: u32,
    pub dust: u32,
    // conveyors, re-volt units per second
    pub velocity: [f32; 3],
    // the debug color of the blender add-on
    pub color: [u8; 3],
}

const fn surface(name: &'static str, flags: u32, roughness: f32, grip: f32, hardness: f32, corrugation: u32, dust: u32, velocity: [f32; 3], color: [u8; 3]) -> Surface
{
    Surface { name, flags, roughness, grip, hardness, corrugation, dust, velocity, color }
}

pub const SURFACES: [Surface; 27] = [
    surface("default", 3, 1.0, 1.0, 1.0, 0, 0, [0.0; 3], [153, 153, 153]),
    surface("marble", 3, 0.9, 0.9, 0.5, 0, 0, [0.0; 3], [130, 91, 91]),
    surface("stone", 3, 0.9, 0.9, 0.5, 0, 0, [0.0; 3], [56, 56, 56]),
    surface("wood", 2, 0.8, 0.8, 0.3, 0, 0, [0.0; 3], [119, 76, 35]),
    surface("sand", 34, 0.5, 0.6, 0.0, 0, 2, [0.0; 3], [244, 193, 127]),
    surface("plastic", 2, 0.7, 0.9, 0.2, 0, 0, [0.0; 3], [22, 22, 22]),
    surface("carpettile", 2, 1.0, 0.7, 0.0, 0, 0, [0.0; 3], [170, 20, 0]),
    surface("carpetshag", 2, 1.0, 0.7, 0.0, 0, 0, [0.0; 3], [135, 45, 33]),
    surface("boundary", 4, 1.0, 1.0, 0.0, 0, 0, [0.0; 3], [255, 0, 255]),
    surface("glass", 3, 0.7, 0.8, 0.3, 0, 0, [0.0; 3], [255, 255, 255]),
    surface("ice1", 3, 0.4, 0.4, 0.2, 0, 0, [0.0; 3], [184, 255, 242]),
    surface("metal", 3, 0.8, 0.8, 0.4, 0, 0, [0.0; 3], [135, 153, 163]),
    surface("grass", 42, 0.7, 0.5, 0.0, 3, 3, [0.0; 3], [45, 91, 12]),
    surface("bumpmetal", 11, 0.8, 0.8, 0.4, 3, 0, [0.0; 3], [56, 61, 51]),
    surface("pebbles", 43, 0.7, 0.7, 0.2, 1, 1, [0.0; 3], [140, 140, 124]),
    surface("gravel", 43, 0.9, 0.8, 0.2, 2, 1, [0.0; 3], [201, 196, 193]),
    surface("conveyor1", 24, 1.0, 1.0, 0.0, 3, 0, [-287.4, 128.7, 388.4], [56, 0, 128]),
    surface("conveyor2", 24, 1.0, 1.0, 0.0, 3, 0, [287.4, 128.7, -388.4], [51, 38, 61]),
    surface("dirt1", 42, 1.0, 1.0, 0.0, 5, 4, [0.0; 3], [135, 99, 73]),
    surface("dirt2", 42, 1.0, 1.0, 0.0, 6, 4, [0.0; 3], [91, 66, 48]),
    surface("dirt3", 42, 1.0, 1.0, 0.0, 7, 4, [0.0; 3], [66, 40, 25]),
    surface("ice2", 3, 0.45, 0.45, 0.2, 0, 0, [0.0; 3], [132, 181, 178]),
    surface("ice3", 3, 0.5, 0.5, 0.2, 0, 0, [0.0; 3], [102, 136, 134]),
    surface("wood2", 2, 0.8, 0.8, 0.4, 0, 0, [0.0; 3], [119, 76, 43]),
    surface("conveyor_market1", 24, 1.0, 1.0, 0.0, 3, 0, [0.0, 0.0, 357.8], [0, 20, 56]),
    surface("conveyor_market2", 24, 1.0, 1.0, 0.0, 3, 0, [-357.8, 0.0, 0.0], [25, 33, 51]),
    surface("paving", 3, 0.9, 0.9, 0.5, 0, 5, [0.0; 3], [142, 127, 114]),
];

// normal . p + distance = 0
#[derive(Clone, Copy)]
pub struct Plane
{
    pub normal: [f32; 3],
    pub distance: f32,
}

#[derive(Clone)]
pub struct CollisionPolygon
{
    pub flags: i32,
    pub surface: i32,
    pub plane: Plane,
    // one per side, pointing outward - a triangle uses the first 3
    pub edges: [Plane; 4],
    pub min: [f32; 3],
    pub max: [f32; 3],
}

#[derive(Clone)]
pub struct LookupGrid
{
    pub x0: f32,
    pub z0: f32,
    pub x_count: usize,
    pub z_count: usize,
    pub raster: f32,
    // polygon references over all cells
    pub entries: usize,
}

#[derive(Default, Clone)]
pub struct Ncp
{
    pub polygons: Vec<CollisionPolygon>,
    pub grid: Option<LookupGrid>,
}

impl CollisionPolygon
{
    pub fn has(&self, flag: i32) -> bool
    {
        self.flags & flag != 0
    }

    // corner i = main plane x side i x side i+1, pulled into the bounding box (non planar quads leave it a bit)
    // None if two planes are parallel; the bool says a corner was pulled in by more than 1 unit
    pub fn corners(&self) -> Option<(Vec<[f32; 3]>, bool)>
    {
        let count = if self.has(NCP_QUAD) { 4 } else { 3 };
        let mut corners = vec![];
        let mut clamped = false;

        for i in 0..count
        {
            let corner = intersect(&self.plane, &self.edges[i], &self.edges[(i + 1) % count])?;
            let inside = [0, 1, 2].map(|a| corner[a].clamp(self.min[a], self.max[a]));
            clamped |= (0..3).any(|a| (inside[a] - corner[a]).abs() > 1.0);
            corners.push(inside);
        }

        Some((corners, clamped))
    }
}

// the point on all three planes - Cramer's rule in f64
fn intersect(a: &Plane, b: &Plane, c: &Plane) -> Option<[f32; 3]>
{
    let rows = [a, b, c].map(|p| p.normal.map(|v| v as f64));
    let rhs = [a, b, c].map(|p| -p.distance as f64);
    let det = |m: &[[f64; 3]; 3]| m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0]) + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);

    let d = det(&rows);
    if d.abs() < 1e-9
    {
        return None;
    }

    Some([0, 1, 2].map(|column|
    {
        let mut m = rows;
        for row in 0..3 { m[row][column] = rhs[row]; }
        (det(&m) / d) as f32
    }))
}

fn plane(reader: &mut Reader) -> Result<Plane>
{
    Ok(Plane { normal: reader.vec3()?, distance: reader.f32()? })
}

pub fn load(path: &Path) -> Result<Ncp>
{
    let data = std::fs::read(path)?;
    // a few instance models have an empty file - no collision
    if data.is_empty()
    {
        return Ok(Ncp::default());
    }
    let mut reader = Reader::new(data);

    let mut polygons = vec![];
    for _ in 0..reader.u16()?
    {
        let flags = reader.i32()?;
        let surface = reader.i32()?;
        let main = plane(&mut reader)?;
        let edges = [plane(&mut reader)?, plane(&mut reader)?, plane(&mut reader)?, plane(&mut reader)?];
        // xlo xhi ylo yhi zlo zhi
        let b = [reader.f32()?, reader.f32()?, reader.f32()?, reader.f32()?, reader.f32()?, reader.f32()?];
        polygons.push(CollisionPolygon { flags, surface, plane: main, edges, min: [b[0], b[2], b[4]], max: [b[1], b[3], b[5]] });
    }

    // only the level file has the grid: x0, z0, x count, z count, raster size, then per cell a count + polygon indices
    let mut grid = None;
    if reader.remaining() > 0
    {
        let (x0, z0, x_count, z_count, raster) = (reader.f32()?, reader.f32()?, reader.f32()? as usize, reader.f32()? as usize, reader.f32()?);
        let mut entries = 0;
        for _ in 0..x_count * z_count
        {
            let count = reader.count()?;
            for _ in 0..count { reader.i32()?; }
            entries += count;
        }
        grid = Some(LookupGrid { x0, z0, x_count, z_count, raster, entries });
    }

    if reader.remaining() != 0
    {
        bail!("{} bytes left after parsing - format not fully understood", reader.remaining());
    }

    Ok(Ncp { polygons, grid })
}

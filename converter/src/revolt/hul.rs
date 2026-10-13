// .hul - collision of cars and physics objects: convex hulls (vertices, edges, face planes) + spheres

use std::path::Path;

use anyhow::{bail, Result};

use super::ncp::Plane;
use super::reader::Reader;

// edges, offset and the spheres are for the cars later
#[allow(dead_code)]
pub struct ConvexHull
{
    // model space
    pub vertices: Vec<[f32; 3]>,
    pub edges: Vec<[u16; 2]>,
    // normals point outward: n . p + d <= 0 inside
    pub faces: Vec<Plane>,
    // the hull center, the bounding box is stored around it
    pub offset: [f32; 3],
}

#[allow(dead_code)]
pub struct Sphere
{
    pub center: [f32; 3],
    pub radius: f32,
}

#[derive(Default)]
pub struct Hull
{
    pub hulls: Vec<ConvexHull>,
    pub spheres: Vec<Sphere>,
}

impl ConvexHull
{
    // the corners of every face, counter clockwise seen from outside (re-volt space) - faces with less than 3 corners are left out
    pub fn face_polygons(&self) -> Vec<Vec<usize>>
    {
        const ON_PLANE: f32 = 0.5;

        self.faces.iter().filter_map(|face|
        {
            let n = face.normal;
            let on: Vec<usize> = (0..self.vertices.len())
                .filter(|&i| (dot(n, self.vertices[i]) + face.distance).abs() <= ON_PLANE)
                .collect();
            if on.len() < 3
            {
                return None;
            }

            // sorted by the angle around the face center
            let center = [0, 1, 2].map(|a| on.iter().map(|&i| self.vertices[i][a]).sum::<f32>() / on.len() as f32);
            let u = normalize(sub(self.vertices[on[0]], center));
            let v = cross(n, u);
            let mut sorted = on.clone();
            sorted.sort_by(|&a, &b|
            {
                let angle = |i: usize| { let d = sub(self.vertices[i], center); dot(d, v).atan2(dot(d, u)) };
                angle(a).total_cmp(&angle(b))
            });
            Some(sorted)
        }).collect()
    }

    pub fn is_finite(&self) -> bool
    {
        self.vertices.iter().flatten().all(|v| v.is_finite()) && self.faces.iter().all(|f| f.normal.iter().all(|v| v.is_finite()) && f.distance.is_finite())
    }
}

pub fn load(path: &Path) -> Result<Hull>
{
    let data = std::fs::read(path)?;
    if data.is_empty()
    {
        bail!("empty file");
    }
    let mut reader = Reader::new(data);

    let mut hulls = vec![];
    for _ in 0..reader.i16()?
    {
        let (vertex_count, edge_count, face_count) = (reader.i16()?, reader.i16()?, reader.i16()?);
        // xlo xhi ylo yhi zlo zhi around the offset
        for _ in 0..6 { reader.f32()?; }
        let offset = reader.vec3()?;
        let vertices = (0..vertex_count).map(|_| reader.vec3()).collect::<Result<Vec<_>>>()?;
        let edges = (0..edge_count).map(|_| Ok([reader.u16()?, reader.u16()?])).collect::<Result<Vec<_>>>()?;
        let faces = (0..face_count).map(|_| Ok(Plane { normal: reader.vec3()?, distance: reader.f32()? })).collect::<Result<Vec<_>>>()?;
        hulls.push(ConvexHull { vertices, edges, faces, offset });
    }

    // Bucket.hul ends after the hulls
    let mut spheres = vec![];
    if reader.remaining() > 0
    {
        for _ in 0..reader.i16()?
        {
            spheres.push(Sphere { center: reader.vec3()?, radius: reader.f32()? });
        }
    }

    Ok(Hull { hulls, spheres })
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 { a[0] * b[0] + a[1] * b[1] + a[2] * b[2] }
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] { [a[0] - b[0], a[1] - b[1], a[2] - b[2]] }
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] { [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]] }
fn normalize(a: [f32; 3]) -> [f32; 3] { let l = dot(a, a).sqrt().max(1e-9); a.map(|v| v / l) }

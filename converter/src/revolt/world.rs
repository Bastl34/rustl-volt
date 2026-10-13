// .w - the track world: meshes (cubes), big cubes (groups of meshes), texture animations, env colors

use std::path::Path;

use anyhow::{bail, Result};

use super::mesh::{Mesh, FACE_ENV};
use super::reader::Reader;

#[allow(dead_code)] // center + radius: not used yet
pub struct WorldMesh
{
    pub center: [f32; 3],
    pub radius: f32,
    pub mesh: Mesh,
}

// a bounding sphere around several meshes - the original culls with them
#[allow(dead_code)]
pub struct BigCube
{
    pub center: [f32; 3],
    pub radius: f32,
    pub meshes: Vec<usize>,
}

pub struct TexFrame
{
    pub texture: i32,
    pub delay: f32,
    pub uvs: [[f32; 2]; 4],
}

pub struct World
{
    pub meshes: Vec<WorldMesh>,
    pub big_cubes: Vec<BigCube>,
    pub tex_anims: Vec<Vec<TexFrame>>,
    // one per polygon with FACE_ENV, in file order
    pub env_colors: Vec<u32>,
}

impl World
{
    pub fn load(path: &Path) -> Result<Self>
    {
        let mut reader = Reader::new(std::fs::read(path)?);

        let mut meshes = vec![];
        for _ in 0..reader.count()?
        {
            let center = reader.vec3()?;
            let radius = reader.f32()?;
            // bounding box xlo xhi ylo yhi zlo zhi - not needed
            for _ in 0..6 { reader.f32()?; }
            meshes.push(WorldMesh { center, radius, mesh: Mesh::read(&mut reader)? });
        }

        let mut big_cubes = vec![];
        for _ in 0..reader.count()?
        {
            let center = reader.vec3()?;
            let radius = reader.f32()?;
            let mut indices = vec![];
            for _ in 0..reader.count()? { indices.push(reader.count()?); }
            big_cubes.push(BigCube { center, radius, meshes: indices });
        }

        let mut tex_anims = vec![];
        for _ in 0..reader.count()?
        {
            let mut frames = vec![];
            for _ in 0..reader.count()?
            {
                let texture = reader.i32()?;
                let delay = reader.f32()?;
                let uvs = [reader.vec2()?, reader.vec2()?, reader.vec2()?, reader.vec2()?];
                frames.push(TexFrame { texture, delay, uvs });
            }
            tex_anims.push(frames);
        }

        let env_count = meshes.iter().flat_map(|m| &m.mesh.polygons).filter(|p| p.has(FACE_ENV)).count();
        let mut env_colors = Vec::with_capacity(env_count);
        for _ in 0..env_count { env_colors.push(reader.u32()?); }

        if reader.remaining() != 0
        {
            bail!("{} bytes left after parsing - format not fully understood", reader.remaining());
        }

        Ok(Self { meshes, big_cubes, tex_anims, env_colors })
    }
}

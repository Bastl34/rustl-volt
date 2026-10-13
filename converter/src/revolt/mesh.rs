// polygon mesh - the body of .prm/.m files and of every mesh in a .w file

use std::path::Path;

use anyhow::Result;

use super::reader::Reader;

// polygon flags
pub const FACE_QUAD: u16 = 1;
pub const FACE_DOUBLE: u16 = 2;
pub const FACE_TRANSLUCENT: u16 = 4;
pub const FACE_MIRROR: u16 = 128;
pub const FACE_ADDITIVE: u16 = 256;
pub const FACE_TEXANIM: u16 = 512;
pub const FACE_NOENV: u16 = 1024;
pub const FACE_ENV: u16 = 2048;
pub const FACE_SKIP: u16 = 8192;

pub const FLAG_NAMES: [(u16, &str); 9] = [
    (FACE_QUAD, "quad"), (FACE_DOUBLE, "double"), (FACE_TRANSLUCENT, "translucent"), (FACE_MIRROR, "mirror"), (FACE_ADDITIVE, "additive"),
    (FACE_TEXANIM, "texanim"), (FACE_NOENV, "noenv"), (FACE_ENV, "env"), (FACE_SKIP, "skip"),
];

#[derive(Clone)]
pub struct Polygon
{
    pub flags: u16,
    // texture page (level a..j) - with FACE_TEXANIM the index of the texture animation, -1 = none
    pub texture: i16,
    pub indices: [u16; 4],
    // 0xAARRGGBB
    pub colors: [u32; 4],
    pub uvs: [[f32; 2]; 4],
}

impl Polygon
{
    pub fn has(&self, flag: u16) -> bool
    {
        self.flags & flag != 0
    }

    pub fn corners(&self) -> usize
    {
        if self.has(FACE_QUAD) { 4 } else { 3 }
    }
}

#[derive(Clone)]
pub struct Vertex
{
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

#[derive(Clone)]
pub struct Mesh
{
    pub polygons: Vec<Polygon>,
    pub vertices: Vec<Vertex>,
}

impl Mesh
{
    // a .prm / .m file: only the most detailed level of detail, plus how many levels the file holds
    pub fn load(path: &Path) -> Result<(Self, usize)>
    {
        let lods = Self::load_lods(path)?;
        let count = lods.len();
        Ok((lods.into_iter().next().unwrap_or(Self { polygons: vec![], vertices: vec![] }), count))
    }

    // .prm files can hold several meshes in a row, from detailed to coarse (levels of detail)
    pub fn load_lods(path: &Path) -> Result<Vec<Self>>
    {
        let mut reader = Reader::new(std::fs::read(path)?);
        let mut lods = vec![Self::read(&mut reader)?];
        while reader.remaining() > 0
        {
            lods.push(Self::read(&mut reader)?);
        }
        Ok(lods)
    }

    pub fn read(reader: &mut Reader) -> Result<Self>
    {
        let polygon_count = reader.u16()? as usize;
        let vertex_count = reader.u16()? as usize;

        let mut polygons = Vec::with_capacity(polygon_count);
        for _ in 0..polygon_count
        {
            let flags = reader.u16()?;
            let texture = reader.i16()?;
            let indices = [reader.u16()?, reader.u16()?, reader.u16()?, reader.u16()?];
            let colors = [reader.u32()?, reader.u32()?, reader.u32()?, reader.u32()?];
            let uvs = [reader.vec2()?, reader.vec2()?, reader.vec2()?, reader.vec2()?];
            polygons.push(Polygon { flags, texture, indices, colors, uvs });
        }

        let mut vertices = Vec::with_capacity(vertex_count);
        for _ in 0..vertex_count
        {
            vertices.push(Vertex { position: reader.vec3()?, normal: reader.vec3()? });
        }

        Ok(Self { polygons, vertices })
    }
}

pub fn color_rgba(color: u32) -> [u8; 4]
{
    let [b, g, r, a] = color.to_le_bytes();
    [r, g, b, a]
}

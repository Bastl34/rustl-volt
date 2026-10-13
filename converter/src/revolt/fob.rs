// .fob - game objects: pickups, sounds, water, planets, ... 56 bytes each, the types are in object_types.rs

use std::path::Path;

use anyhow::{bail, Result};

use super::reader::Reader;

#[derive(Clone)]
pub struct Object
{
    pub kind: i32,
    // meaning depends on the type
    pub flags: [i32; 4],
    pub position: [f32; 3],
    // local axes in world space like the .fin rows, right = up × look
    pub up: [f32; 3],
    pub look: [f32; 3],
}

pub fn load(path: &Path) -> Result<Vec<Object>>
{
    let mut reader = Reader::new(std::fs::read(path)?);

    let mut objects = vec![];
    for _ in 0..reader.count()?
    {
        let kind = reader.i32()?;
        let flags = [reader.i32()?, reader.i32()?, reader.i32()?, reader.i32()?];
        let position = reader.vec3()?;
        let up = reader.vec3()?;
        let look = reader.vec3()?;
        objects.push(Object { kind, flags, position, up, look });
    }

    if reader.remaining() != 0
    {
        bail!("{} bytes left after parsing - format not fully understood", reader.remaining());
    }

    Ok(objects)
}

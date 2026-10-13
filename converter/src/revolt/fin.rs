// .fin - instances: .prm models placed in the level (plants, shelves, props), 72 bytes each

use std::path::Path;

use anyhow::{bail, Result};

use super::reader::Reader;

// instance flags (from the re-volt blender add-on)
pub const FIN_HIDE: u8 = 2;
pub const FIN_NO_OBJECT_COLLISION: u8 = 32;
pub const FIN_NO_CAMERA_COLLISION: u8 = 64;

pub const FIN_FLAG_NAMES: [(u8, &str); 7] = [
    (1, "env"), (FIN_HIDE, "hide"), (4, "no mirror"), (8, "no lights"), (16, "set model rgb"),
    (FIN_NO_OBJECT_COLLISION, "no object collision"), (FIN_NO_CAMERA_COLLISION, "no camera collision"),
];

pub struct Instance
{
    // the model file name without extension, upper case, cut to 8 characters
    pub name: String,
    // signed rgb offset to the model colors
    pub color: [i8; 3],
    pub env_color: u32,
    pub priority: u8,
    pub flag: u8,
    pub lod_bias: f32,
    pub position: [f32; 3],
    // the local axes right, up, look in world space (rows)
    pub axes: [[f32; 3]; 3],
}

pub fn load(path: &Path) -> Result<Vec<Instance>>
{
    let mut reader = Reader::new(std::fs::read(path)?);

    let mut instances = vec![];
    for _ in 0..reader.count()?
    {
        let name_bytes: Vec<u8> = (0..9).map(|_| reader.u8()).collect::<Result<_>>()?;
        let name = String::from_utf8_lossy(name_bytes.split(|&b| b == 0).next().unwrap_or(&[])).to_string();
        let color = [reader.u8()? as i8, reader.u8()? as i8, reader.u8()? as i8];
        let env_color = reader.u32()?;
        let priority = reader.u8()?;
        let flag = reader.u8()?;
        reader.u16()?;
        let lod_bias = reader.f32()?;
        let position = reader.vec3()?;
        let axes = [reader.vec3()?, reader.vec3()?, reader.vec3()?];
        instances.push(Instance { name, color, env_color, priority, flag, lod_bias, position, axes });
    }

    if reader.remaining() != 0
    {
        bail!("{} bytes left after parsing - format not fully understood", reader.remaining());
    }

    Ok(instances)
}

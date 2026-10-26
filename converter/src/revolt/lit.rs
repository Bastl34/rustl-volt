// .lit - the lights of a level: count, then 72 bytes per light

use std::path::Path;

use anyhow::{bail, Result};

use super::reader::Reader;

// light flags: 1 baked into the world at load, 2 kept as a light while racing, 4 lights moving models (cars, objects - unchecked), 16 not baked into the models
pub const LIGHT_WORLD: u8 = 1;
pub const LIGHT_KEPT: u8 = 2;
pub const LIGHT_NO_MODELS: u8 = 16;

// light types: omni, omni turned by the surface normal, spot, spot turned by the surface normal
pub const LIGHT_OMNI: u8 = 0;
pub const LIGHT_OMNI_NORMAL: u8 = 1;
pub const LIGHT_SPOT: u8 = 2;
pub const LIGHT_SPOT_NORMAL: u8 = 3;
// not a light: a box (light axes, the color fields are its half size) - moving models inside it get the SHADOWBOX value of the .inf added (default -96)
pub const LIGHT_SQUARE_SHADOW: u8 = 4;

pub struct Light
{
    pub position: [f32; 3],
    pub reach: f32,
    // rows right, up, look - a spot shines along look
    pub axes: [[f32; 3]; 3],
    // degrees
    pub cone: f32,
    // 0..255 and more (the game clamps the lit color)
    pub color: [f32; 3],
    pub flag: u8,
    pub kind: u8,
    pub speed: u8,
}

pub fn load(path: &Path) -> Result<Vec<Light>>
{
    let mut reader = Reader::new(std::fs::read(path)?);

    let mut lights = vec![];
    for _ in 0..reader.count()?
    {
        let position = reader.vec3()?;
        let reach = reader.f32()?;
        let axes = [reader.vec3()?, reader.vec3()?, reader.vec3()?];
        let cone = reader.f32()?;
        let color = reader.vec3()?;
        let flag = reader.u8()?;
        let kind = reader.u8()?;
        let speed = reader.u8()?;
        reader.u8()?;
        lights.push(Light { position, reach, axes, cone, color, flag, kind, speed });
    }

    if reader.remaining() != 0
    {
        bail!("{} bytes left after parsing - format not fully understood", reader.remaining());
    }

    Ok(lights)
}

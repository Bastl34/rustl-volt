// .fld - force fields: oriented boxes that push the cars and objects inside them (the water of the levels)

use std::path::Path;

use anyhow::{bail, Result};

use super::reader::Reader;

// field types: linear (push along the direction, damped along it), orientation (turns the body up against the direction), velocity, spherical, a field over the whole world
pub const FIELD_LINEAR: i32 = 0;
pub const FIELD_ORIENTATION: i32 = 1;

pub struct Field
{
    pub kind: i32,
    pub center: [f32; 3],
    // rows = the local axes of the box
    pub axes: [[f32; 3]; 3],
    pub half_size: [f32; 3],
    pub direction: [f32; 3],
    pub magnitude: f32,
    pub damping: f32,
}

pub fn load(path: &Path) -> Result<Vec<Field>>
{
    let mut reader = Reader::new(std::fs::read(path)?);

    let mut fields = vec![];
    for _ in 0..reader.count()?
    {
        let kind = reader.i32()?;
        let center = reader.vec3()?;
        let axes = [reader.vec3()?, reader.vec3()?, reader.vec3()?];
        let half_size = reader.vec3()?;
        let direction = reader.vec3()?;
        let magnitude = reader.f32()?;
        let damping = reader.f32()?;
        // editor values (64, 128, 0, 0 in every file) - the game does not read them
        for _ in 0..4 { reader.f32()?; }
        fields.push(Field { kind, center, axes, half_size, direction, magnitude, damping });
    }

    if reader.remaining() != 0
    {
        bail!("{} bytes left after parsing - format not fully understood", reader.remaining());
    }

    Ok(fields)
}

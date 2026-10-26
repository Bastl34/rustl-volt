// .tri - triggers: oriented boxes with a type and a flag (piano, reposition, camera, track direction signs, ...)

use std::path::Path;

use anyhow::{bail, Result};

use super::reader::Reader;

// type 0: the piano of toylite (the keyboard is hard coded in the game)
pub const TRIGGER_PIANO: i32 = 0;
// type 2: a track direction sign, flag = order << 16 | sign
pub const TRIGGER_TRACK_DIR: i32 = 2;
// type 6: the object thrower whose ID is the flag throws its object - once, when the first car enters the box
pub const TRIGGER_THROW: i32 = 6;

pub struct Trigger
{
    pub kind: i32,
    pub flag: i32,
    pub center: [f32; 3],
    // rows = the local axes of the box
    pub axes: [[f32; 3]; 3],
    pub half_size: [f32; 3],
}

pub fn load(path: &Path) -> Result<Vec<Trigger>>
{
    let mut reader = Reader::new(std::fs::read(path)?);

    let mut triggers = vec![];
    for _ in 0..reader.count()?
    {
        let kind = reader.i32()?;
        let flag = reader.i32()?;
        let center = reader.vec3()?;
        let axes = [reader.vec3()?, reader.vec3()?, reader.vec3()?];
        let half_size = reader.vec3()?;
        triggers.push(Trigger { kind, flag, center, axes, half_size });
    }

    if reader.remaining() != 0
    {
        bail!("{} bytes left after parsing - format not fully understood", reader.remaining());
    }

    Ok(triggers)
}

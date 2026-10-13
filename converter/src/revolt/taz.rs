// .taz - track zones: oriented boxes a car has to pass in id order (anti shortcut, gates the lap progress)

use std::path::Path;

use anyhow::{bail, Result};

use super::reader::Reader;

pub struct Zone
{
    // the order on the track - several boxes can share one id
    pub id: i32,
    pub center: [f32; 3],
    // rows = the local axes of the box
    pub axes: [[f32; 3]; 3],
    pub half_size: [f32; 3],
}

pub fn load(path: &Path) -> Result<Vec<Zone>>
{
    let mut reader = Reader::new(std::fs::read(path)?);

    let mut zones = vec![];
    for _ in 0..reader.count()?
    {
        let id = reader.i32()?;
        let center = reader.vec3()?;
        let axes = [reader.vec3()?, reader.vec3()?, reader.vec3()?];
        let half_size = reader.vec3()?;
        zones.push(Zone { id, center, axes, half_size });
    }

    if reader.remaining() != 0
    {
        bail!("{} bytes left after parsing - format not fully understood", reader.remaining());
    }

    // the exe sorts by id (stable bubble sort) after loading
    zones.sort_by_key(|zone| zone.id);
    Ok(zones)
}

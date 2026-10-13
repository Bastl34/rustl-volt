// .rim - mirror planes: where the reflective floors are (the polygons have FACE_MIRROR)

use std::path::Path;

use anyhow::{bail, Result};

use super::reader::Reader;

pub struct MirrorPlane
{
    pub flag: i32,
    // plane: normal . p + distance = 0
    pub normal: [f32; 3],
    pub distance: f32,
    pub corners: [[f32; 3]; 4],
}

pub fn load(path: &Path) -> Result<Vec<MirrorPlane>>
{
    let mut reader = Reader::new(std::fs::read(path)?);

    let mut planes = vec![];
    for _ in 0..reader.u16()?
    {
        let flag = reader.i32()?;
        let normal = reader.vec3()?;
        let distance = reader.f32()?;
        // bounding box xlo xhi ylo yhi zlo zhi - the corners say the same
        for _ in 0..6 { reader.f32()?; }
        let corners = [reader.vec3()?, reader.vec3()?, reader.vec3()?, reader.vec3()?];
        planes.push(MirrorPlane { flag, normal, distance, corners });
    }

    if reader.remaining() != 0
    {
        bail!("{} bytes left after parsing - format not fully understood", reader.remaining());
    }

    Ok(planes)
}

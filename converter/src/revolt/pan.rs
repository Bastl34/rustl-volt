// .pan - position nodes: a graph along the track with the distance to the finish line (lap progress, race position, wrong way)

use std::path::Path;

use anyhow::{bail, Result};

use super::reader::Reader;

pub struct PathNode
{
    pub position: [f32; 3],
    // distance along the track to the finish line, 0 at the start node
    pub distance: f32,
    // links, -1 = none
    pub prev: [i32; 4],
    pub next: [i32; 4],
}

pub struct TrackPath
{
    pub start: i32,
    // length of one lap
    pub length: f32,
    pub nodes: Vec<PathNode>,
}

pub fn load(path: &Path) -> Result<TrackPath>
{
    let mut reader = Reader::new(std::fs::read(path)?);

    // battle levels: only a count of 0
    let count = reader.count()?;
    if count == 0
    {
        return Ok(TrackPath { start: 0, length: 0.0, nodes: vec![] });
    }

    let start = reader.i32()?;
    let length = reader.f32()?;

    let mut nodes = vec![];
    for _ in 0..count
    {
        let position = reader.vec3()?;
        let distance = reader.f32()?;
        let prev = [reader.i32()?, reader.i32()?, reader.i32()?, reader.i32()?];
        let next = [reader.i32()?, reader.i32()?, reader.i32()?, reader.i32()?];
        nodes.push(PathNode { position, distance, prev, next });
    }

    if reader.remaining() != 0
    {
        bail!("{} bytes left after parsing - format not fully understood", reader.remaining());
    }

    Ok(TrackPath { start, length, nodes })
}

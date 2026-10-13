// .fan - AI nodes: track cross sections (left / right edge) with the racing line, linked in driving direction

use std::path::Path;

use anyhow::{bail, Result};

use super::reader::Reader;

pub struct AiNode
{
    pub priority: u8,
    pub start_node: u8,
    pub flag: u8,
    // 0 = left edge, 1 = right edge
    pub racing_line: f32,
    pub finish_distance: f32,
    // unknown 0..1, maybe the overtaking line
    pub line2: f32,
    pub unknown: i32,
    pub racing_line_speed: i32,
    pub centre_speed: i32,
    // links, -1 = none
    pub prev: [i32; 2],
    pub next: [i32; 2],
    pub right_speed: i32,
    pub right: [f32; 3],
    pub left_speed: i32,
    pub left: [f32; 3],
}

pub struct AiNodes
{
    // unknown: the exe allocates count + this many nodes
    pub extra: i16,
    pub nodes: Vec<AiNode>,
    // unknown tail: a node index and a length (about one lap)
    pub tail_node: i32,
    pub tail_length: f32,
}

pub fn load(path: &Path) -> Result<AiNodes>
{
    let mut reader = Reader::new(std::fs::read(path)?);

    let count = reader.i16()?;
    let extra = reader.i16()?;
    if count < 0
    {
        bail!("negative count {count}");
    }

    let mut nodes = vec![];
    for _ in 0..count
    {
        let priority = reader.u8()?;
        let start_node = reader.u8()?;
        let flag = reader.u8()?;
        reader.u8()?;
        let racing_line = reader.f32()?;
        let finish_distance = reader.f32()?;
        let line2 = reader.f32()?;
        let unknown = reader.i32()?;
        let racing_line_speed = reader.i32()?;
        let centre_speed = reader.i32()?;
        let prev = [reader.i32()?, reader.i32()?];
        let next = [reader.i32()?, reader.i32()?];
        let right_speed = reader.i32()?;
        let right = reader.vec3()?;
        let left_speed = reader.i32()?;
        let left = reader.vec3()?;
        nodes.push(AiNode { priority, start_node, flag, racing_line, finish_distance, line2, unknown, racing_line_speed, centre_speed, prev, next, right_speed, right, left_speed, left });
    }

    // empty files (battle levels) end after the counts
    let (tail_node, tail_length) = if count > 0 { (reader.i32()?, reader.f32()?) } else { (0, 0.0) };

    if reader.remaining() != 0
    {
        bail!("{} bytes left after parsing - format not fully understood", reader.remaining());
    }

    Ok(AiNodes { extra, nodes, tail_node, tail_length })
}

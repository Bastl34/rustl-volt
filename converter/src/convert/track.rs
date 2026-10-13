// .taz zones, .pan position nodes, .fan AI nodes, .tri track direction signs -> JSON strings in the extras of the level object (meters, glTF coordinates)
// the race logic of the app code reads them - formats/README.md, Race progress

use std::path::Path;

use anyhow::Result;
use serde_json::{json, Map, Value};

use super::geometry::to_gltf;
use super::{mirror, Log};
use crate::revolt::{fan, find_file, pan, taz, tri};

pub fn extras(dir: &Path, level: &str, scale: f32, mirrored: bool, log: &Log) -> Map<String, Value>
{
    let mut extras = Map::new();

    let mut put = |key: &str, ext: &str, convert: &dyn Fn(&Path) -> Result<Option<Value>>|
    {
        let Some(path) = find_file(dir, &format!("{level}.{ext}")) else
        {
            log.warn(format!("  no .{ext} file"));
            return;
        };
        log.read(&path);
        match convert(&path)
        {
            Ok(Some(value)) => { extras.insert(key.into(), json!(value.to_string())); },
            Ok(None) => {},
            Err(error) => log.error(format!("  {}: {error:#}", path.display())),
        }
    };

    put("revolt_track_zones", "taz", &|path| zones(path, scale, mirrored, log));
    put("revolt_track_path", "pan", &|path| track_path(path, scale, mirrored, log));
    put("revolt_ai_nodes", "fan", &|path| ai_nodes(path, scale, mirrored, log));
    put("revolt_track_signs", "tri", &|path| signs(path, scale, mirrored, log));

    extras
}

fn zones(path: &Path, scale: f32, mirrored: bool, log: &Log) -> Result<Option<Value>>
{
    let mut zones = taz::load(path)?;
    if mirrored { mirror::zones(&mut zones); }
    let ids = zones.last().map_or(0, |zone| zone.id + 1);
    let shared = zones.len() as i32 - ids;
    log.info(format!("  {} zones, ids 0..{}{}", zones.len(), ids - 1, if shared > 0 { format!(", {shared} more boxes on shared ids") } else { String::new() }));

    let zones: Vec<Value> = zones.iter().map(|zone| json!(
    {
        "id": zone.id,
        "center": position(zone.center, scale),
        "axes": zone.axes.map(|axis| round(to_gltf(axis), 1.0, 1e6)),
        "half_size": round(zone.half_size, scale, 1e4),
    })).collect();
    Ok(Some(json!(zones)))
}

fn track_path(path: &Path, scale: f32, mirrored: bool, log: &Log) -> Result<Option<Value>>
{
    let mut track = pan::load(path)?;
    if mirrored { mirror::track_path(&mut track); }
    if track.nodes.is_empty()
    {
        log.info("  no position nodes (battle / stunt level)");
        return Ok(None);
    }

    let branches = track.nodes.iter().filter(|node| links(&node.next).len() > 1).count();
    log.info(format!("  {} position nodes, start node {}, lap {:.1} m, {branches} branches", track.nodes.len(), track.start, track.length * scale));

    let nodes: Vec<Value> = track.nodes.iter().map(|node| json!(
    {
        "position": position(node.position, scale),
        "distance": round_value(node.distance * scale),
        "prev": links(&node.prev),
        "next": links(&node.next),
    })).collect();
    Ok(Some(json!({ "start": track.start, "length": round_value(track.length * scale), "nodes": nodes })))
}

fn ai_nodes(path: &Path, scale: f32, mirrored: bool, log: &Log) -> Result<Option<Value>>
{
    let mut ai = fan::load(path)?;
    if mirrored { mirror::ai_nodes(&mut ai); }
    if ai.nodes.is_empty()
    {
        log.info("  no AI nodes (battle / stunt level)");
        return Ok(None);
    }

    let starts = ai.nodes.iter().filter(|node| node.start_node != 0).count();
    log.info(format!("  {} AI nodes, {starts} start nodes, header {} / tail {} {:.1} m (unknown)", ai.nodes.len(), ai.extra, ai.tail_node, ai.tail_length * scale));

    let nodes: Vec<Value> = ai.nodes.iter().map(|node| json!(
    {
        "left": position(node.left, scale),
        "right": position(node.right, scale),
        "racing_line": round_value(node.racing_line),
        "finish_distance": round_value(node.finish_distance * scale),
        "priority": node.priority,
        "start_node": node.start_node,
        "prev": links(&node.prev),
        "next": links(&node.next),
        // raw values, meaning not decoded
        "racing_line_speed": node.racing_line_speed,
        "centre_speed": node.centre_speed,
        "left_speed": node.left_speed,
        "right_speed": node.right_speed,
        "line2": round_value(node.line2),
        "flag": node.flag,
        "unknown": node.unknown,
    })).collect();
    Ok(Some(json!({ "nodes": nodes })))
}

// the track direction triggers (type 2): order (the car counts them per lap), sign (the arrow, 0..10) and their box
fn signs(path: &Path, scale: f32, mirrored: bool, log: &Log) -> Result<Option<Value>>
{
    let mut triggers = tri::load(path)?;
    if mirrored { mirror::triggers(&mut triggers); }
    let signs: Vec<Value> = triggers.iter().filter(|t| t.kind == tri::TRIGGER_TRACK_DIR).map(|t| json!(
    {
        "order": t.flag >> 16,
        "sign": t.flag & 0xffff,
        "center": position(t.center, scale),
        "axes": t.axes.map(|axis| round(to_gltf(axis), 1.0, 1e6)),
        "half_size": round(t.half_size, scale, 1e4),
    })).collect();
    log.info(format!("  {} triggers, {} track direction signs", triggers.len(), signs.len()));
    if signs.is_empty() { return Ok(None); }
    Ok(Some(json!(signs)))
}

fn position(p: [f32; 3], scale: f32) -> [f64; 3]
{
    round(to_gltf(p), scale, 1e4)
}

fn round(v: [f32; 3], scale: f32, steps: f64) -> [f64; 3]
{
    v.map(|v| (v as f64 * scale as f64 * steps).round() / steps)
}

fn round_value(v: f32) -> f64
{
    (v as f64 * 1e4).round() / 1e4
}

fn links(links: &[i32]) -> Vec<i32>
{
    links.iter().copied().filter(|&link| link >= 0).collect()
}


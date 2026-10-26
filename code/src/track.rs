// the track data of a level: zones (.taz), position nodes (.pan), direction signs (.tri) and AI nodes (.fan)
// the converter writes them as JSON strings into the extras of the level object (meters, glTF coordinates) - formats/README.md

use rustl::prelude::*;
use rustl::serde_json::{self, Value};
use rustl::state::project::project::SceneObject;

// links followed from the last AI segment before the whole track is searched
const LOCAL_SEARCH_DEPTH: usize = 4;
// m from the racing line segment that still counts as on it for the local search
const LOCAL_SEARCH_DISTANCE: f32 = 6.0;
// a height difference counts this much more than a distance on the ground - bridges and crossings lie above each other
const HEIGHT_WEIGHT: f32 = 3.0;
// for "on the track" height differences up to this many m count nothing - the AI nodes of the nhood1 tunnel float 3 m above its floor
// (picking the segment still weighs every height difference: toy2 / wild_west1 have crossings 2.9 / 3.5 m apart)
const HEIGHT_TOLERANCE: f32 = 3.5;

pub struct Zone
{
    pub id: i32,
    pub center: Vector3<f32>,
    pub axes: [Vector3<f32>; 3],
    pub half_size: Vector3<f32>,
}

// a track direction trigger of the .tri: shown when the car is in its box and `order` is the next one of the lap
pub struct Sign
{
    pub order: i32,
    pub sign: usize,
    pub zone: Zone,
}

pub struct PathNode
{
    pub position: Vector3<f32>,
    pub distance: f32,
    pub prev: Vec<usize>,
    pub next: Vec<usize>,
}

// a cross section of the track: left and right edge, the racing line between them (0 left .. 1 right)
pub struct AiNode
{
    pub left: Vector3<f32>,
    pub right: Vector3<f32>,
    pub line: f32,
    pub prev: Vec<usize>,
    pub next: Vec<usize>,
}

// where a car is on the AI nodes: the segment from `node` to `next` and the share along it
#[derive(Clone, Copy, Debug)]
pub struct AiPlace
{
    pub node: usize,
    pub next: usize,
    pub t: f32,
    // m from the centre line of the segment, height differences up to HEIGHT_TOLERANCE left out
    pub distance: f32,
    // the same with every height difference weighted - picks the segment
    pub rank: f32,
}

pub struct Track
{
    pub zones: Vec<Zone>,
    pub zone_ids: i32,
    pub nodes: Vec<PathNode>,
    pub start: usize,
    pub length: f32,
    pub signs: Vec<Sign>,
    // the object thrower triggers, id = the ID of the thrower they set off
    pub throws: Vec<Zone>,
    pub ai: Vec<AiNode>,
}

impl Track
{
    // the level object of a scene with track data - None for levels without (battle, stunts, frontend)
    pub fn from_scene(scene: &Scene) -> Option<Self>
    {
        let level = find_node(&scene.nodes, &|node| node.tags.contains("revolt_level") && node.extras.contains("revolt_track_zones"))?;
        let level = level.read().unwrap();
        let json = |key: &str| level.extras.get::<String>(key).and_then(|text| serde_json::from_str::<Value>(text).ok());
        Self::parse(&json("revolt_track_zones")?, &json("revolt_track_path")?, json("revolt_track_signs").as_ref(), json("revolt_ai_nodes").as_ref(), json("revolt_track_throws").as_ref())
    }

    // a battle arena: no zones, no path, no AI nodes
    pub fn arena() -> Self
    {
        Self { zones: vec![], zone_ids: 1, nodes: vec![], start: 0, length: 1.0, signs: vec![], ai: vec![], throws: vec![] }
    }

    // NAME of the .inf of a scene that is not loaded, from the root extras of its level glb
    pub fn level_name_from_file(scene: &Scene) -> Option<String>
    {
        let path = scene.source.as_ref()?.origin_path.clone();
        let file = rustl::state::project::loader::load_editor_scene(&path)?;
        let level = find_object(&file.objects, &|object| object.tags.iter().any(|tag| tag == "revolt_level"))?;
        level.source.as_ref().and_then(|source| glb_root_name(&rustl::helper::file::resolve_relative_path(&path, source)))
    }

    // the same for a scene that is not loaded: from its file - with the level name (NAME of the .inf) from the root extras of its glb
    pub fn from_scene_file(scene: &Scene) -> Option<(Self, Option<String>)>
    {
        let path = scene.source.as_ref()?.origin_path.clone();
        let file = rustl::state::project::loader::load_editor_scene(&path)?;
        let level = find_object(&file.objects, &|object| object.tags.iter().any(|tag| tag == "revolt_level") && object.extras.contains_key("revolt_track_zones"))?;
        let json = |key: &str| level.extras.get(key).and_then(|value| value.as_str()).and_then(|text| serde_json::from_str::<Value>(text).ok());
        let track = Self::parse(&json("revolt_track_zones")?, &json("revolt_track_path")?, json("revolt_track_signs").as_ref(), json("revolt_ai_nodes").as_ref(), json("revolt_track_throws").as_ref())?;
        let name = level.source.as_ref().and_then(|source| glb_root_name(&rustl::helper::file::resolve_relative_path(&path, source)));
        Some((track, name))
    }

    fn parse(zones: &Value, path: &Value, signs: Option<&Value>, ai: Option<&Value>, throws: Option<&Value>) -> Option<Self>
    {
        let zones: Vec<Zone> = zones.as_array()?.iter().filter_map(zone).collect();
        let throws: Vec<Zone> = throws.and_then(|throws| throws.as_array()).into_iter().flatten().filter_map(zone).collect();
        let signs: Vec<Sign> = signs.and_then(|signs| signs.as_array()).into_iter().flatten().filter_map(|sign| Some(Sign
        {
            order: sign["order"].as_i64()? as i32,
            sign: sign["sign"].as_u64()? as usize,
            zone: zone(sign)?,
        })).collect();
        let nodes: Vec<PathNode> = path["nodes"].as_array()?.iter().filter_map(|node| Some(PathNode
        {
            position: vector(&node["position"])?,
            distance: node["distance"].as_f64()? as f32,
            prev: links(&node["prev"]),
            next: links(&node["next"]),
        })).collect();
        let ai: Vec<AiNode> = ai.and_then(|ai| ai["nodes"].as_array()).into_iter().flatten().filter_map(|node| Some(AiNode
        {
            left: vector(&node["left"])?,
            right: vector(&node["right"])?,
            line: node["racing_line"].as_f64()? as f32,
            prev: links(&node["prev"]),
            next: links(&node["next"]),
        })).collect();
        if zones.is_empty() || nodes.is_empty()
        {
            return None;
        }

        let zone_ids = zones.iter().map(|zone| zone.id).max().unwrap_or(0) + 1;
        let start = (path["start"].as_u64()? as usize).min(nodes.len() - 1);
        Some(Self { zones, zone_ids, nodes, start, length: path["length"].as_f64()? as f32, signs, ai, throws })
    }

    // in one of the boxes of the zone
    pub fn in_zone(&self, id: i32, position: Vector3<f32>) -> bool
    {
        self.zones.iter().filter(|zone| zone.id == id).any(|zone| zone.contains(position))
    }

    // in no zone at all - off the track
    pub fn in_any_zone(&self, position: Vector3<f32>) -> bool
    {
        self.zones.iter().any(|zone| zone.contains(position))
    }

    // driving direction at a .pan node: from its prev to its next nodes, blended towards the neighbour the car is closer to
    pub fn direction(&self, index: usize, position: Vector3<f32>) -> Vector3<f32>
    {
        let tangent = |index: usize|
        {
            let node = &self.nodes[index];
            let average = |links: &Vec<usize>| if links.is_empty() { node.position } else { links.iter().map(|&link| self.nodes[link].position).sum::<Vector3<f32>>() / links.len() as f32 };
            let tangent = average(&node.next) - average(&node.prev);
            if tangent.norm() > 0.0 { tangent.normalize() } else { Vector3::zeros() }
        };

        let node = &self.nodes[index];
        let here = tangent(index);
        let ahead = (position - node.position).dot(&here) >= 0.0;
        let neighbour = if ahead { node.next.first() } else { node.prev.first() };
        let Some(&neighbour) = neighbour else { return here; };

        let span = (self.nodes[neighbour].position - node.position).norm().max(0.01);
        let t = ((position - node.position).norm() / span).clamp(0.0, 1.0) * 0.5;
        let blended = here * (1.0 - t) + tangent(neighbour) * t;
        if blended.norm() > 0.0 { blended.normalize() } else { here }
    }

    // ********** AI nodes **********

    pub fn has_ai(&self) -> bool
    {
        !self.ai.is_empty()
    }

    // the point of a node at `line` (0 left edge .. 1 right edge)
    pub fn ai_point(&self, node: usize, line: f32) -> Vector3<f32>
    {
        let node = &self.ai[node];
        node.left + (node.right - node.left) * line
    }

    // the racing line of a node, moved sideways by `offset` (share of the track width), kept off the edges
    pub fn racing_point(&self, node: usize, offset: f32) -> Vector3<f32>
    {
        self.ai_point(node, (self.ai[node].line + offset).clamp(0.1, 0.9))
    }

    // the segment the position is on: first around the last one, then the whole track
    pub fn ai_place(&self, position: Vector3<f32>, last: Option<AiPlace>) -> Option<AiPlace>
    {
        if self.ai.is_empty()
        {
            return None;
        }

        if let Some(last) = last
        {
            let mut best: Option<AiPlace> = None;
            let mut visited = vec![last.node];
            let mut frontier = vec![last.node];
            for _ in 0..LOCAL_SEARCH_DEPTH
            {
                let mut next_frontier = vec![];
                for &node in &frontier
                {
                    for place in self.segments_of(node).map(|next| self.segment_place(node, next, position))
                    {
                        if best.is_none_or(|best| place.rank < best.rank) { best = Some(place); }
                    }
                    for &link in self.ai[node].next.iter().chain(self.ai[node].prev.iter())
                    {
                        if !visited.contains(&link) { visited.push(link); next_frontier.push(link); }
                    }
                }
                frontier = next_frontier;
            }
            if let Some(best) = best.filter(|best| best.distance < LOCAL_SEARCH_DISTANCE + self.half_width(best.node))
            {
                return Some(best);
            }
        }

        (0..self.ai.len())
            .flat_map(|node| self.segments_of(node).map(move |next| (node, next)))
            .map(|(node, next)| self.segment_place(node, next, position))
            .min_by(|a, b| a.rank.total_cmp(&b.rank))
    }

    // half of the track width at a node
    pub fn half_width(&self, node: usize) -> f32
    {
        (self.ai[node].right - self.ai[node].left).norm() * 0.5
    }

    // the track direction of a segment, flat
    pub fn segment_direction(&self, node: usize, next: usize) -> Vector3<f32>
    {
        let d = self.ai_point(next, 0.5) - self.ai_point(node, 0.5);
        let d = Vector3::new(d.x, 0.0, d.z);
        if d.norm() > 0.001 { d.normalize() } else { -Vector3::z() }
    }

    // the next nodes of a node, the node itself if it has none (open end)
    fn segments_of(&self, node: usize) -> impl Iterator<Item = usize> + '_
    {
        let next = &self.ai[node].next;
        let own = if next.is_empty() { Some(node) } else { None };
        next.iter().copied().chain(own)
    }

    // the position on the centre line from node to next: share along it and the distance (height counts more)
    fn segment_place(&self, node: usize, next: usize, position: Vector3<f32>) -> AiPlace
    {
        let a = self.ai_point(node, 0.5);
        let b = self.ai_point(next, 0.5);
        let ab = b - a;
        let t = if ab.norm_squared() > 1e-6 { ((position - a).dot(&ab) / ab.norm_squared()).clamp(0.0, 1.0) } else { 0.0 };
        let offset = position - (a + ab * t);
        let ground = offset.x * offset.x + offset.z * offset.z;
        let height = (offset.y.abs() - HEIGHT_TOLERANCE).max(0.0) * HEIGHT_WEIGHT;
        let distance = (ground + height * height).sqrt();
        let rank = (ground + offset.y * offset.y * HEIGHT_WEIGHT * HEIGHT_WEIGHT).sqrt();
        AiPlace { node, next, t, distance, rank }
    }
}

impl Zone
{
    pub fn contains(&self, position: Vector3<f32>) -> bool
    {
        let offset = position - self.center;
        (0..3).all(|axis| self.axes[axis].dot(&offset).abs() <= self.half_size[axis])
    }
}

fn vector(value: &Value) -> Option<Vector3<f32>>
{
    Some(Vector3::new(value[0].as_f64()? as f32, value[1].as_f64()? as f32, value[2].as_f64()? as f32))
}

fn links(value: &Value) -> Vec<usize>
{
    value.as_array().map(|links| links.iter().filter_map(|link| link.as_u64().map(|link| link as usize)).collect()).unwrap_or_default()
}

fn zone(zone: &Value) -> Option<Zone>
{
    Some(Zone
    {
        id: zone["id"].as_i64().unwrap_or(0) as i32,
        center: vector(&zone["center"])?,
        axes: [vector(&zone["axes"][0])?, vector(&zone["axes"][1])?, vector(&zone["axes"][2])?],
        half_size: vector(&zone["half_size"])?,
    })
}

fn find_object<'a>(objects: &'a [SceneObject], test: &dyn Fn(&SceneObject) -> bool) -> Option<&'a SceneObject>
{
    objects.iter().find_map(|object| if test(object) { Some(object) } else { find_object(&object.objects, test) })
}

// the "name" extra of the root node of a glb - only its json chunk is read (no file access: None)
fn glb_root_name(path: &str) -> Option<String>
{
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    let mut header = [0u8; 20];
    file.read_exact(&mut header).ok()?;
    if &header[0..4] != b"glTF" || &header[16..20] != b"JSON" { return None; }
    let length = u32::from_le_bytes([header[12], header[13], header[14], header[15]]) as usize;
    let mut json = vec![0u8; length];
    file.read_exact(&mut json).ok()?;
    let gltf: Value = serde_json::from_slice(&json).ok()?;
    let scene = gltf["scene"].as_u64().unwrap_or(0) as usize;
    let root = gltf["scenes"][scene]["nodes"][0].as_u64()? as usize;
    gltf["nodes"][root]["extras"]["name"].as_str().map(|name| name.to_string())
}

pub fn find_node(nodes: &Vec<NodeItem>, test: &dyn Fn(&Node) -> bool) -> Option<NodeItem>
{
    for node in nodes
    {
        if test(&node.read().unwrap())
        {
            return Some(node.clone());
        }
        let children = node.read().unwrap().nodes.clone();
        if let Some(found) = find_node(&children, test)
        {
            return Some(found);
        }
    }
    None
}

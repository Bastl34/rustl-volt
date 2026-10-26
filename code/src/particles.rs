// sprite particles like the sparks of the game (formats/README.md "Sparks"): 31 kinds with own values after the spark table of the game, plus the dragon fire and its smoke
// and the trails of the game (a ribbon behind a rocket or a spark)
// the effects only call emit / burst with a kind - the drawing (one node per kind and scene, its instances camera facing quads like the tire marks)
// stays in here, so a particle system of the engine can take over later without touching the effects

use std::sync::{Arc, RwLock};

use rustl::helper::option_or_id::OptionOrId;
use rustl::nalgebra::Matrix4;
use rustl::prelude::*;
use rustl::rapier3d::prelude::{QueryFilter, Ray, Vector};
use rustl::state::resources::mesh_resource::{MeshResource, MeshResourceItem};
use rustl::state::resources::texture::TextureItem;
use rustl::state::scene::components::material::{BlendMode, Material, MaterialItem, TextureType};
use rustl::state::scene::components::mesh::Mesh;
use rustl::state::scene::instance::Instance;
use rustl::state::scene::node::InstanceItemArc;
use rustl::state::state::ENGINE_INTERNAL_TAG_PREFX;

use crate::random::Random;

// fxpage1 with black see through (the converter writes it since ASSET_VERSION 23)
pub const TEXTURE_FILE: &str = "models/sparks.png";
// at most this many at once over all scenes like the game, new ones are dropped beyond it
const MAX_PARTICLES: usize = 1024;
// instances made at once the first time per kind, after that the amount doubles - a new count rebuilds buffers
const FIRST_BLOCK: usize = 32;
// the game steps 120 times a second - its drag is a share per step
const GAME_STEP_RATE: f32 = 120.0;
// m off the surface after a bounce
const BOUNCE_LIFT: f32 = 0.005;
// the stretched kinds are drawn as long as the way of this time (own - the game draws dots)
const STRETCH_TIME: f32 = 0.025;
// the additive kinds this much brighter (above 1 blooms)
const GLOW: f32 = 1.4;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind
{
    Spark, Spark2, Snow, Popcorn, Gravel, Sand, Grass, Electric, Water, Dirt,
    Smoke1, Smoke2, Smoke3, Blue, BigBlue, SmallOrange, SmallRed, Explosion1, Explosion2, Star,
    ProbeSmoke, Sprinkler, SprinklerBig, Dolphin, DolphinBig, Spark3, RoadDust, GrassDust, SoilDust, GravelDust,
    SandDust, DragonFire, DragonSmoke,
}

pub const KINDS: usize = 33;
// the spark types of the game - the kinds after them are own
const SPARK_TYPES: usize = 31;

const ALL: [Kind; KINDS] =
[
    Kind::Spark, Kind::Spark2, Kind::Snow, Kind::Popcorn, Kind::Gravel, Kind::Sand, Kind::Grass, Kind::Electric, Kind::Water, Kind::Dirt,
    Kind::Smoke1, Kind::Smoke2, Kind::Smoke3, Kind::Blue, Kind::BigBlue, Kind::SmallOrange, Kind::SmallRed, Kind::Explosion1, Kind::Explosion2, Kind::Star,
    Kind::ProbeSmoke, Kind::Sprinkler, Kind::SprinklerBig, Kind::Dolphin, Kind::DolphinBig, Kind::Spark3, Kind::RoadDust, Kind::GrassDust, Kind::SoilDust, Kind::GravelDust,
    Kind::SandDust, Kind::DragonFire, Kind::DragonSmoke,
];

impl Kind
{
    // the spark type of the game (the spark generator objects)
    pub fn from_id(id: i64) -> Option<Kind>
    {
        usize::try_from(id).ok().and_then(|id| ALL[..SPARK_TYPES].get(id).copied())
    }
}

const ADD: u32 = 1; // additive, fades by brightness - else alpha blended, fades by alpha
const FALL: u32 = 2; // gravity of the scene
const HIT: u32 = 4; // bounces off the level
const SPIN: u32 = 8;
const GROW: u32 = 16;
const STRETCH: u32 = 32; // own: drawn along its way

struct Info
{
    size: [f32; 2], // m, half width and height
    tile: [f32; 4], // u, v, width, height on the page
    color: [f32; 3],
    flags: u32,
    drag: f32, // share of the speed lost per game step
    friction: f32, // share of the speed along the surface lost at a bounce
    bounce: f32, // share of the speed into the surface kept
    life: f32, // s - minus random × life_var
    life_var: f32,
    spin_var: f32, // rad/s, ± half of it
    size_var: f32, // m added at random
    grow: [f32; 2], // m/s: base + random × the second
}

// texels of the 256 px page, half a texel inside - the linear filter would take in the neighbour tile at the edge
const fn tile(x: f32, y: f32, size: f32) -> [f32; 4] { [(x + 0.5) / 256.0, (y + 0.5) / 256.0, (size - 1.0) / 256.0, (size - 1.0) / 256.0] }
const fn gray(value: f32) -> [f32; 3] { [value, value, value] }
const WHITE: [f32; 3] = [1.0, 1.0, 1.0];
const SMOKE_TILE: [f32; 4] = tile(0.0, 0.0, 64.0);
const BLAST_TILE: [f32; 4] = tile(64.0, 0.0, 64.0);

const fn info(size: f32, tile: [f32; 4], color: [f32; 3], flags: u32, drag: f32, friction: f32, bounce: f32, life: f32, life_var: f32, spin_var: f32, size_var: f32, grow: [f32; 2]) -> Info
{
    Info { size: [size, size], tile, color, flags, drag, friction, bounce, life, life_var, spin_var, size_var, grow }
}

// in the order of the game's spark types
const INFO: [Info; KINDS] =
[
    info(0.012, tile(240.0, 0.0, 16.0), WHITE, ADD | FALL | HIT | STRETCH, 0.02, 0.1, 0.5, 0.5, 0.05, 0.0, 0.0, [0.0; 2]),
    info(0.012, tile(240.0, 0.0, 16.0), WHITE, ADD | STRETCH, 0.02, 0.1, 0.5, 0.3, 0.02, 0.0, 0.0, [0.0; 2]),
    info(0.05, tile(224.0, 0.0, 16.0), WHITE, ADD | FALL | HIT, 0.1, 0.6, 0.2, 10.0, 5.0, 0.0, 0.0, [0.0; 2]),
    info(0.1, tile(208.0, 0.0, 16.0), [1.0, 0.2, 1.0], ADD | FALL | HIT, 0.01, 0.7, 0.8, 10.0, 0.0, 0.0, 0.0, [0.0; 2]),
    info(0.035, tile(192.0, 0.0, 16.0), gray(0.5), FALL | HIT, 0.02, 0.7, 0.5, 1.5, 0.5, 0.0, 0.0, [0.0; 2]),
    info(0.02, tile(240.0, 0.0, 16.0), WHITE, ADD | FALL | HIT, 0.02, 0.7, 0.5, 0.5, 0.1, 0.0, 0.0, [0.0; 2]),
    info(0.04, tile(208.0, 0.0, 16.0), WHITE, FALL | HIT | SPIN, 0.02, 1.0, 0.5, 1.0, 0.3, 15.0, 0.0, [0.0; 2]),
    info(0.008, tile(224.0, 16.0, 16.0), [0.75, 0.75, 1.0], ADD | FALL | STRETCH, 0.02, 0.1, 0.5, 0.15, 0.0, 0.0, 0.0, [0.0; 2]),
    info(0.1, tile(192.0, 16.0, 16.0), [0.27, 0.2, 0.87], ADD | FALL | HIT, 0.001, 0.8, 0.2, 2.0, 1.0, 0.0, 0.0, [0.0; 2]),
    info(0.035, tile(208.0, 16.0, 16.0), WHITE, FALL | HIT, 0.001, 0.4, 0.2, 1.5, 0.5, 0.0, 0.0, [0.0; 2]),
    info(0.06, SMOKE_TILE, gray(0.25), ADD | HIT | SPIN | GROW, 0.002, 0.0, 0.0, 0.5, 0.1, 6.0, 0.02, [0.0, 0.36]),
    info(0.64, SMOKE_TILE, gray(0.375), ADD | HIT | SPIN | GROW, 0.02, 0.0, 0.0, 2.6, 0.0, 6.0, 0.0, [0.4, 0.2]),
    info(0.04, SMOKE_TILE, gray(0.25), ADD | SPIN | GROW, 0.0, 0.0, 0.0, 1.5, 0.1, 6.0, 0.02, [0.0, 0.36]),
    info(0.04, tile(224.0, 16.0, 16.0), gray(0.5), ADD, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, [0.0; 2]),
    info(0.04, tile(224.0, 16.0, 16.0), gray(0.5), ADD | FALL, 0.02, 0.1, 0.5, 0.5, 0.5, 0.0, 0.08, [0.0; 2]),
    info(0.18, tile(192.0, 32.0, 16.0), WHITE, ADD | FALL | HIT, 0.016, 0.0, 0.4, 1.2, 0.5, 0.0, 0.02, [0.0; 2]),
    info(0.2, tile(240.0, 32.0, 16.0), WHITE, ADD | FALL | HIT, 0.012, 0.0, 0.4, 1.5, 0.5, 0.0, 0.05, [0.0; 2]),
    info(0.12, BLAST_TILE, WHITE, ADD | SPIN | GROW, 0.015, 0.0, 0.0, 0.5, 0.0, 6.0, 0.0, [0.0, 1.28]),
    info(0.08, BLAST_TILE, WHITE, ADD | SPIN | GROW, 0.015, 0.0, 0.0, 0.5, 0.3, 32.0, 0.0, [0.12, 0.05]),
    info(0.08, [241.5 / 256.0, 32.5 / 256.0, 14.0 / 256.0, 15.0 / 256.0], WHITE, ADD | GROW, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, [0.3, 0.0]),
    // a long band on a page of the level in the game - never emitted here
    Info { size: [7.0, 1.5], tile: SMOKE_TILE, color: WHITE, flags: ADD | GROW, drag: 0.0, friction: 0.0, bounce: 0.0, life: 0.5, life_var: 0.0, spin_var: 0.0, size_var: 0.0, grow: [1.0, 0.0] },
    info(0.03, tile(192.0, 16.0, 16.0), [0.125, 0.125, 0.16], ADD | FALL | GROW, 0.01, 0.2, 0.2, 0.9, 0.0, 0.0, 0.0, [0.08, 0.08]),
    info(0.03, tile(192.0, 16.0, 16.0), [0.03, 0.03, 0.04], ADD | FALL | SPIN | GROW, 0.01, 0.2, 0.2, 1.0, 0.0, 6.0, 0.0, [1.28, 0.64]),
    info(0.03, tile(192.0, 16.0, 16.0), [0.25, 0.25, 0.31], ADD | FALL | GROW, 0.005, 0.2, 0.2, 1.3, 0.0, 0.0, 0.0, [0.08, 0.08]),
    info(0.03, tile(192.0, 16.0, 16.0), [0.06, 0.06, 0.09], ADD | FALL | SPIN | GROW, 0.005, 0.2, 0.2, 1.3, 0.0, 6.0, 0.0, [0.96, 0.48]),
    info(0.03, tile(240.0, 0.0, 16.0), WHITE, ADD | FALL | HIT | STRETCH, 0.002, 0.0, 0.8, 4.0, 0.05, 0.0, 0.0, [0.0; 2]),
    info(0.06, SMOKE_TILE, gray(0.25), ADD | HIT | SPIN | GROW, 0.002, 0.0, 0.0, 0.5, 0.1, 6.0, 0.02, [0.0, 0.36]),
    info(0.06, SMOKE_TILE, [0.08, 0.1, 0.06], ADD | HIT | SPIN | GROW, 0.002, 0.0, 0.0, 0.5, 0.1, 6.0, 0.02, [0.0, 0.36]),
    info(0.06, SMOKE_TILE, [0.11, 0.1, 0.06], ADD | HIT | SPIN | GROW, 0.002, 0.0, 0.0, 0.5, 0.1, 6.0, 0.02, [1.2, 0.2]),
    info(0.06, SMOKE_TILE, gray(0.25), ADD | HIT | SPIN | GROW, 0.002, 0.0, 0.0, 0.5, 0.1, 6.0, 0.02, [0.0, 0.36]),
    info(0.06, SMOKE_TILE, [0.06, 0.03, 0.0], ADD | HIT | SPIN | GROW, 0.002, 0.0, 0.0, 0.5, 0.1, 6.0, 0.02, [0.6, 0.2]),
    // the fire of the dragon (its own sprites in the game, the blast tile)
    info(0.16, BLAST_TILE, WHITE, ADD | SPIN | GROW, 0.0, 0.0, 0.0, 0.5, 0.0, 9.0, 0.08, [0.32, 0.0]),
    // own: dark smoke out of the dragon fire, alpha blended
    info(0.2, SMOKE_TILE, gray(0.1), SPIN | GROW, 0.004, 0.0, 0.0, 1.8, 0.5, 3.0, 0.1, [0.5, 0.3]),
];

// the trails of the game: a point every life / points s, the oldest goes after life s, narrower and darker towards the end
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TrailKind
{
    Spark,
    Firework,
}

const TRAIL_KINDS: usize = 2;
const TRAIL_ALL: [TrailKind; TRAIL_KINDS] = [TrailKind::Spark, TrailKind::Firework];

struct TrailInfo
{
    head: [f32; 4], // the tile of the newest piece
    tile: [f32; 4], // of the others
    color: [f32; 3],
    life: f32,
    half_width: f32, // m at the newest point
    points: usize,
}

// the game's trail types 0 (small red sparks) and 2 (firework)
const TRAIL_INFO: [TrailInfo; TRAIL_KINDS] =
[
    TrailInfo { head: tile(225.0, 33.0, 14.0), tile: tile(225.0, 33.0, 14.0), color: WHITE, life: 0.3, half_width: 0.05, points: 12 },
    TrailInfo { head: tile(224.0, 48.0, 16.0), tile: tile(240.0, 48.0, 16.0), color: [0.53, 0.53, 0.67], life: 0.4, half_width: 0.1, points: 12 },
];
// the kinds that pull a trail behind them
const TRAILED: [(Kind, TrailKind); 1] = [(Kind::SmallRed, TrailKind::Spark)];
// a trail nobody moved for this long ends by itself
const TRAIL_ORPHAN: f32 = 0.5;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TrailId(u64);

struct Trail
{
    kind: TrailKind,
    // newest first: position, age
    points: Vec<(Vector3<f32>, f32)>,
    head: Vector3<f32>,
    due: f32,
    idle: f32,
    ended: bool,
}

impl Trail
{
    fn new(kind: TrailKind, head: Vector3<f32>) -> Self
    {
        Self { kind, points: vec![], head, due: 0.0, idle: 0.0, ended: false }
    }

    fn step(&mut self, dt: f32)
    {
        let info = &TRAIL_INFO[self.kind as usize];
        for point in &mut self.points { point.1 += dt; }
        self.points.retain(|point| point.1 < info.life);
        if self.ended { return; }
        self.due -= dt;
        if self.due <= 0.0
        {
            self.due += info.life / info.points as f32;
            self.points.insert(0, (self.head, 0.0));
            self.points.truncate(info.points);
        }
    }

    // the line from the head (while it moves) to the oldest point, with the share of life left at each point
    fn line(&self) -> Vec<(Vector3<f32>, f32)>
    {
        let life = TRAIL_INFO[self.kind as usize].life;
        let head = (!self.ended).then_some((self.head, 1.0));
        head.into_iter().chain(self.points.iter().map(|(point, age)| (*point, 1.0 - age / life))).collect()
    }
}

struct Particle
{
    position: Vector3<f32>,
    velocity: Vector3<f32>,
    age: f32,
    life: f32,
    angle: f32,
    spin: f32,
    size: f32, // m added to the size of the kind
    grow: f32,
    trail: Option<Trail>,
}

// the ribbons of one trail kind in one scene: one mesh rebuilt every frame
struct TrailPool
{
    node: NodeItem,
    mesh: MeshResourceItem,
    instance: InstanceItemArc,
    shown: bool,
}

// the node of one kind in one scene, its instances in use first
struct Pool
{
    node: NodeItem,
    instances: Vec<InstanceItemArc>,
    shown: usize,
}

struct SceneParticles
{
    scene_id: u32,
    particles: Vec<Vec<Particle>>,
    pools: Vec<Option<Pool>>,
    trails: Vec<(TrailId, Trail)>,
    trail_pools: Vec<Option<TrailPool>>,
    // loaded with the first pool
    texture: Option<Option<TextureItem>>,
}

#[derive(Default)]
pub struct Particles
{
    scenes: Vec<SceneParticles>,
    random: Random,
    next_trail: u64,
}

impl Particles
{
    // one particle at a point - spread: m/s at random on each axis (the speed variation of the game)
    pub fn emit(&mut self, scene_id: u32, kind: Kind, position: Vector3<f32>, velocity: Vector3<f32>, spread: f32)
    {
        if self.count() >= MAX_PARTICLES { return; }
        let info = &INFO[kind as usize];
        let random = &mut self.random;
        let jitter = Vector3::new(random.next() * 2.0 - 1.0, random.next() * 2.0 - 1.0, random.next() * 2.0 - 1.0) * spread;
        let particle = Particle
        {
            position,
            velocity: velocity + jitter,
            age: 0.0,
            life: (info.life - random.next() * info.life_var).max(0.02),
            angle: random.next() * std::f32::consts::TAU,
            spin: (random.next() - 0.5) * info.spin_var,
            size: random.next() * info.size_var,
            grow: info.grow[0] + random.next() * info.grow[1],
            trail: TRAILED.iter().find(|(trailed, _)| *trailed == kind).map(|(_, trail)| Trail::new(*trail, position)),
        };
        self.scene(scene_id).particles[kind as usize].push(particle);
    }

    pub fn burst(&mut self, scene_id: u32, kind: Kind, count: usize, position: Vector3<f32>, velocity: Vector3<f32>, spread: f32)
    {
        for _ in 0..count
        {
            self.emit(scene_id, kind, position, velocity, spread);
        }
    }

    // a trail behind something - move its head every frame, end it when the thing goes (its points fade out)
    pub fn trail_start(&mut self, scene_id: u32, kind: TrailKind, position: Vector3<f32>) -> TrailId
    {
        self.next_trail += 1;
        let id = TrailId(self.next_trail);
        self.scene(scene_id).trails.push((id, Trail::new(kind, position)));
        id
    }

    pub fn trail_move(&mut self, id: TrailId, position: Vector3<f32>)
    {
        if let Some(trail) = self.trail(id) { trail.head = position; trail.idle = 0.0; }
    }

    pub fn trail_end(&mut self, id: TrailId)
    {
        if let Some(trail) = self.trail(id) { trail.ended = true; }
    }

    fn trail(&mut self, id: TrailId) -> Option<&mut Trail>
    {
        self.scenes.iter_mut().flat_map(|part| part.trails.iter_mut()).find(|(trail, _)| *trail == id).map(|(_, trail)| trail)
    }

    // a number of particles per s as whole ones this frame (the rest carried over by chance)
    pub fn due(&mut self, rate: f32, dt: f32) -> usize
    {
        let amount = rate * dt;
        amount as usize + usize::from(self.random.next() < amount.fract())
    }

    // move, bounce and draw all - dt 0 while paused (they still turn to the camera)
    pub fn update(&mut self, state: &mut State, dt: f32)
    {
        for part in &mut self.scenes
        {
            let wanted = (0..KINDS).any(|kind| !part.particles[kind].is_empty() && part.pools[kind].is_none()) || !part.trails.is_empty();
            if wanted && part.texture.is_none()
            {
                let path = state.find_scene_by_id(part.scene_id).map(|scene| format!("{}/{TEXTURE_FILE}", crate::assets_folder(scene)));
                let max_resolution = state.max_texture_resolution();
                part.texture = Some(path.and_then(|path| state.load_texture_or_reuse(&path, Some("png".to_string()), max_resolution).map_err(|error| { console_warning!("particles: {}: {} - plain quads", path, error); }).ok()));
            }
            let Some(scene) = state.find_scene_by_id_mut(part.scene_id) else
            {
                part.particles.clear();
                continue;
            };
            part.step(scene, dt);
            part.draw(scene);
        }
        self.scenes.retain(|part| !part.particles.is_empty());
    }

    // the scene is gone, its nodes with it
    pub fn remove_scene(&mut self, scene_id: u32)
    {
        self.scenes.retain(|part| part.scene_id != scene_id);
    }

    // all particles and their nodes go (back in the editor)
    pub fn stop(&mut self, state: &mut State)
    {
        for part in self.scenes.drain(..)
        {
            let Some(scene) = state.find_scene_by_id_mut(part.scene_id) else { continue; };
            let nodes = part.pools.into_iter().flatten().map(|pool| pool.node).chain(part.trail_pools.into_iter().flatten().map(|pool| pool.node));
            for node in nodes
            {
                let id = node.read().unwrap().id;
                scene.delete_node_by_id(id, true, false, true, false);
            }
        }
    }

    fn count(&self) -> usize
    {
        self.scenes.iter().map(|part| part.particles.iter().map(Vec::len).sum::<usize>()).sum()
    }

    fn scene(&mut self, scene_id: u32) -> &mut SceneParticles
    {
        let index = match self.scenes.iter().position(|part| part.scene_id == scene_id)
        {
            Some(index) => index,
            None =>
            {
                self.scenes.push(SceneParticles { scene_id, particles: (0..KINDS).map(|_| vec![]).collect(), pools: (0..KINDS).map(|_| None).collect(), trails: vec![], trail_pools: (0..TRAIL_KINDS).map(|_| None).collect(), texture: None });
                self.scenes.len() - 1
            },
        };
        &mut self.scenes[index]
    }
}

impl SceneParticles
{
    fn step(&mut self, scene: &Scene, dt: f32)
    {
        if dt <= 0.0 { return; }
        let gravity = scene.physics.settings.gravity;
        for (kind, particles) in self.particles.iter_mut().enumerate()
        {
            let info = &INFO[kind];
            particles.retain_mut(|particle|
            {
                particle.age += dt;
                if particle.age >= particle.life { return false; }
                if info.flags & FALL != 0 { particle.velocity += gravity * dt; }
                if info.drag > 0.0 { particle.velocity *= (1.0 - info.drag * GAME_STEP_RATE * dt).max(0.0); }
                let from = particle.position;
                let way = particle.velocity * dt;
                particle.position += way;
                if info.flags & HIT != 0
                {
                    if let Some((distance, normal)) = cast(scene, from, way)
                    {
                        particle.position = from + way.normalize() * distance + normal * BOUNCE_LIFT;
                        let into = particle.velocity.dot(&normal);
                        if into < 0.0
                        {
                            let along = particle.velocity - normal * into;
                            particle.velocity = along * (1.0 - info.friction) - normal * into * info.bounce;
                        }
                    }
                }
                particle.angle += particle.spin * dt;
                particle.size += particle.grow * dt;
                if let Some(trail) = particle.trail.as_mut() { trail.head = particle.position; trail.step(dt); }
                true
            });
        }
        for (_, trail) in &mut self.trails
        {
            trail.idle += dt;
            if trail.idle > TRAIL_ORPHAN { trail.ended = true; }
            trail.step(dt);
        }
        self.trails.retain(|(_, trail)| !(trail.ended && trail.points.is_empty()));
    }

    // every particle an instance turned to the nearest camera, the ones left over hidden
    fn draw(&mut self, scene: &mut Scene)
    {
        let eyes: Vec<Vector3<f32>> = scene.cameras.iter().filter(|camera| camera.enabled && !camera.tags.contains_starts_with(ENGINE_INTERNAL_TAG_PREFX)).map(|camera| camera.get_data().eye_pos.coords).collect();
        for kind in 0..KINDS
        {
            let particles = &self.particles[kind];
            if particles.is_empty() && self.pools[kind].as_ref().is_none_or(|pool| pool.shown == 0) { continue; }
            let texture = self.texture.clone().flatten();
            let pool = self.pools[kind].get_or_insert_with(|| Pool::new(scene, kind, texture));
            pool.reserve(particles.len());

            let info = &INFO[kind];
            for (particle, instance) in particles.iter().zip(&pool.instances)
            {
                let left = 1.0 - particle.age / particle.life;
                let [r, g, b] = info.color;
                let color = if info.flags & ADD != 0 { let glow = left * GLOW; Vector4::new(r * glow, g * glow, b * glow, 1.0) } else { Vector4::new(r, g, b, left) };
                let eye = eyes.iter().min_by(|x, y| (*x - particle.position).norm_squared().total_cmp(&(*y - particle.position).norm_squared())).copied().unwrap_or(particle.position + Vector3::z());
                let transform = billboard(info, particle, eye);

                let mut instance = instance.write().unwrap();
                let data = instance.get_data_mut().get_mut();
                data.visible = true;
                data.color = color;
                data.computed.world_matrix = transform;
                data.computed.alpha = color.w;
            }
            for instance in pool.instances.iter().take(pool.shown).skip(particles.len())
            {
                instance.write().unwrap().get_data_mut().get_mut().visible = false;
            }
            pool.shown = particles.len();
            // one upload of the whole buffer instead of one per instance
            pool.node.write().unwrap().instances.force_change();
        }
        self.draw_trails(scene, &eyes);
    }

    // every trail of a kind as camera facing quads in one mesh, in world space
    fn draw_trails(&mut self, scene: &mut Scene, eyes: &[Vector3<f32>])
    {
        for kind in 0..TRAIL_KINDS
        {
            let lines: Vec<Vec<(Vector3<f32>, f32)>> = self.trails.iter().map(|(_, trail)| trail).chain(self.particles.iter().flatten().filter_map(|particle| particle.trail.as_ref()))
                .filter(|trail| trail.kind as usize == kind).map(Trail::line).filter(|line| line.len() >= 2).collect();
            if lines.is_empty() && self.trail_pools[kind].as_ref().is_none_or(|pool| !pool.shown) { continue; }
            let texture = self.texture.clone().flatten();
            let pool = self.trail_pools[kind].get_or_insert_with(|| TrailPool::new(scene, kind, texture));
            pool.shown = !lines.is_empty();
            pool.instance.write().unwrap().get_data_mut().get_mut().visible = pool.shown;
            if !pool.shown { continue; }

            let info = &TRAIL_INFO[kind];
            let (mut vertices, mut normals, mut uvs, mut colors, mut indices) = (vec![], vec![], vec![], vec![], vec![]);
            for line in &lines
            {
                let eye = eyes.iter().min_by(|x, y| (*x - line[0].0).norm_squared().total_cmp(&(*y - line[0].0).norm_squared())).copied().unwrap_or(line[0].0 + Vector3::z());
                let count = line.len() - 1;
                let sides: Vec<Vector3<f32>> = (0..count).map(|i| (line[i + 1].0 - line[i].0).cross(&((line[i].0 + line[i + 1].0) * 0.5 - eye)).try_normalize(1e-6).unwrap_or_else(Vector3::zeros)).collect();
                let offsets: Vec<Vector3<f32>> = (0..=count).map(|i|
                {
                    let side = if i == 0 { sides[0] } else if i == count { sides[count - 1] } else { (sides[i - 1] + sides[i]).try_normalize(1e-6).unwrap_or(sides[i]) };
                    side * info.half_width * line[i].1
                }).collect();
                for i in 0..count
                {
                    let [u, v, width, height] = if i == 0 { info.head } else { info.tile };
                    let base = vertices.len() as u32;
                    // the streak of the tile runs along u (newest at the left), across v
                    for (point, side, x, y) in [(i, -1.0, 0.0, 0.0), (i + 1, -1.0, 1.0, 0.0), (i + 1, 1.0, 1.0, 1.0), (i, 1.0, 0.0, 1.0)]
                    {
                        let (position, left) = line[point];
                        vertices.push(Point3::from(position + offsets[point] * side));
                        normals.push((eye - position).try_normalize(1e-6).unwrap_or_else(Vector3::y));
                        // the vertex buffer flips v (glTF convention) - stored as 1 - v
                        uvs.push(Point2::new(u + x * width, 1.0 - (v + y * height)));
                        let [r, g, b] = info.color;
                        let glow = left * GLOW;
                        colors.push([r * glow, g * glow, b * glow, 1.0]);
                    }
                    indices.extend([[base, base + 1, base + 2], [base, base + 2, base + 3]]);
                }
            }
            let mut mesh = pool.mesh.write().unwrap();
            let data = mesh.get_data_mut().get_mut();
            data.vertices = vertices;
            data.normals = normals;
            data.uvs_0 = uvs;
            data.colors = colors;
            data.uv_indices = indices.clone();
            data.normals_indices = indices.clone();
            data.indices = indices;
        }
    }
}

impl TrailPool
{
    // one mesh in world space, rebuilt every frame - its bounds would be stale, so no culling
    fn new(scene: &mut Scene, kind: usize, texture: Option<TextureItem>) -> Self
    {
        let name = format!("trails {:?}", TRAIL_ALL[kind]);
        let resource: MeshResourceItem = Arc::new(RwLock::new(Box::new(MeshResource::new_plane(&name, Point3::new(-0.5, 0.0, 0.0), Point3::new(0.5, 0.0, 0.0), Point3::new(0.5, 0.0, -1.0), Point3::new(-0.5, 0.0, -1.0)))));
        let mut mesh = Mesh::new(&name);
        mesh.mesh_resource = OptionOrId::Some(resource.clone());
        let material = material(scene, &name, texture, true, true);

        let node = Node::new(&name);
        {
            let mut write = node.write().unwrap();
            write.add_component(Arc::new(RwLock::new(Box::new(mesh))));
            write.add_component(material);
            settings(&mut write.settings);
        }
        let transform = Transformation::new("Transform", Vector3::zeros(), Vector3::zeros(), Vector3::new(1.0, 1.0, 1.0));
        let mut instance = Instance::new_with_transform(name, node.clone(), transform);
        instance.pickable = false;
        {
            let data = instance.get_data_mut().get_mut();
            data.collision = false;
            data.visible = false;
        }
        let instance = node.write().unwrap().add_instance(Box::new(instance));
        scene.add_node(node.clone());
        Self { node, mesh: resource, instance, shown: false }
    }
}

impl Pool
{
    // a quad of the tile of the kind, unlit, additive or alpha blended, no shadow, no depth write
    fn new(scene: &mut Scene, kind: usize, texture: Option<TextureItem>) -> Self
    {
        let info = &INFO[kind];
        let name = format!("particles {:?}", ALL[kind]);
        let [u, v, width, height] = info.tile;
        // the vertex buffer flips v (glTF convention) - stored as 1 - v
        let uv = |x: f32, y: f32| Point2::new(u + x * width, 1.0 - (v + y * height));
        let vertices = vec![Point3::new(-0.5, -0.5, 0.0), Point3::new(0.5, -0.5, 0.0), Point3::new(0.5, 0.5, 0.0), Point3::new(-0.5, 0.5, 0.0)];
        let uvs = vec![uv(0.0, 1.0), uv(1.0, 1.0), uv(1.0, 0.0), uv(0.0, 0.0)];
        let indices = vec![[0, 1, 2], [0, 2, 3]];
        let resource = MeshResource::new_with_data(&name, vertices, indices.clone(), uvs, indices.clone(), vec![Vector3::z(); 4], indices);
        let mut mesh = Mesh::new(&name);
        mesh.mesh_resource = OptionOrId::Some(Arc::new(RwLock::new(Box::new(resource))));

        let material = material(scene, &name, texture, info.flags & ADD != 0, false);

        let node = Node::new(&name);
        {
            let mut write = node.write().unwrap();
            write.add_component(Arc::new(RwLock::new(Box::new(mesh))));
            write.add_component(material);
            // update writes where each instance is drawn
            write.set_skip_instance_update(true);
            settings(&mut write.settings);
        }
        scene.add_node(node.clone());
        Self { node, instances: vec![], shown: 0 }
    }

    // at least this many instances - in blocks that double
    fn reserve(&mut self, count: usize)
    {
        while self.instances.len() < count
        {
            let amount = self.instances.len().max(FIRST_BLOCK).min(MAX_PARTICLES.saturating_sub(self.instances.len()).max(1));
            for _ in 0..amount
            {
                let transform = Transformation::new("Transform", Vector3::zeros(), Vector3::zeros(), Vector3::new(1.0, 1.0, 1.0));
                let mut instance = Instance::new_with_transform(format!("particle_{}", self.instances.len()), self.node.clone(), transform);
                instance.pickable = false;
                let data = instance.get_data_mut().get_mut();
                data.collision = false;
                data.visible = false;
                self.instances.push(self.node.write().unwrap().add_instance(Box::new(instance)));
            }
        }
    }
}

// unlit with the tile page, additive or alpha blended, no shadow
pub fn material(scene: &mut Scene, name: &str, texture: Option<TextureItem>, additive: bool, vertex_colors: bool) -> MaterialItem
{
    let mut material = Material::new(name);
    if let Some(texture) = texture { material.set_texture(texture, TextureType::Base); }
    {
        let data = material.get_data_mut().get_mut();
        data.base_color = Vector3::new(1.0, 1.0, 1.0);
        data.ambient_color = Vector3::zeros();
        data.specular_color = Vector3::zeros();
        data.unlit_shading = true;
        data.reflectivity = 0.0;
        data.vertex_colors = vertex_colors;
        data.cast_shadow = false;
        data.receive_shadow = false;
        data.backface_culling = false;
        data.blend_mode = if additive { BlendMode::Additive } else { BlendMode::Blend };
        // the transparent pass - the instances carry their own opacity
        data.alpha = if additive { 1.0 } else { 0.99 };
        // a tile next to other tiles: a mip level would bleed them in
        if let Some(texture) = data.texture_base.as_mut() { texture.sampler.no_mipmaps = true; }
    }
    let material: MaterialItem = Arc::new(RwLock::new(Box::new(material)));
    scene.add_material(&material);
    material
}

// not saved, not picked, no collision, no depth write, never culled (the instances spread over the level)
pub fn settings(settings: &mut rustl::state::scene::node::NodeSettings)
{
    settings.transient = true;
    settings.pickable = false;
    settings.collision = false;
    settings.camera_collision = false;
    settings.depth_write = false;
    settings.occlusion_culling = false;
    settings.frustum_culling = false;
}

// the quad facing the eye: turned by its angle, or stretched along its way across the view
fn billboard(info: &Info, particle: &Particle, eye: Vector3<f32>) -> Matrix4<f32>
{
    let facing = (eye - particle.position).try_normalize(1e-6).unwrap_or_else(Vector3::z);
    let mut right = Vector3::y().cross(&facing).try_normalize(1e-6).unwrap_or_else(Vector3::x);
    let mut up = facing.cross(&right);
    let width = info.size[0] + particle.size;
    let mut height = info.size[1] + particle.size;

    let across = particle.velocity - facing * particle.velocity.dot(&facing);
    let speed = across.norm();
    if info.flags & STRETCH != 0 && speed > 0.1
    {
        up = across / speed;
        right = up.cross(&facing);
        height += speed * STRETCH_TIME * 0.5;
    }
    else if info.flags & SPIN != 0
    {
        let (sin, cos) = particle.angle.sin_cos();
        (right, up) = (right * cos + up * sin, up * cos - right * sin);
    }

    let mut transform = Matrix4::identity();
    transform.fixed_view_mut::<3, 1>(0, 0).copy_from(&(right * width * 2.0));
    transform.fixed_view_mut::<3, 1>(0, 1).copy_from(&(up * height * 2.0));
    transform.fixed_view_mut::<3, 1>(0, 2).copy_from(&facing);
    transform.fixed_view_mut::<3, 1>(0, 3).copy_from(&particle.position);
    transform
}

// distance and normal of the level along this frame's way of a particle
fn cast(scene: &Scene, from: Vector3<f32>, way: Vector3<f32>) -> Option<(f32, Vector3<f32>)>
{
    let length = way.norm();
    if length < 1e-5 { return None; }
    let direction = way / length;
    let ray = Ray::new(Vector::new(from.x, from.y, from.z), Vector::new(direction.x, direction.y, direction.z));
    scene.physics.query_pipeline(QueryFilter::only_fixed().exclude_sensors()).cast_ray_and_get_normal(&ray, length, true)
        .map(|(_, hit)| (hit.time_of_impact, Vector3::new(hit.normal.x, hit.normal.y, hit.normal.z)))
        .filter(|(_, normal)| normal.norm_squared() > 0.25)
}

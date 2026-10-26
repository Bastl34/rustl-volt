// the weapons in action (formats/README.md "Weapons"): what is fired flies, rolls or lies on the track, hits the cars and goes
// own code after the behaviour of the game, the values varied a bit - the fireworks steer better than the game's but miss now and then

use std::sync::{Arc, RwLock};

use rustl::nalgebra::{Matrix4, UnitQuaternion};
use rustl::prelude::*;
use rustl::rapier3d::prelude::{QueryFilter, Ray, Vector};
use rustl::state::scene::components::component::ComponentItem;
use rustl::state::scene::components::material::{BlendMode, Material};
use rustl::state::scene::components::sound::{Sound, SoundType};
use rustl::state::scene::node::{PhysicsBodyType, PhysicsSettings, PhysicsShape};
use rustl::state::scene::scene_controller::vehicle_controller::VehicleController;

use crate::lightning::{self, Lightning};
use crate::particles::{Kind, Particles, TrailId, TrailKind};
use crate::race::{Race, Racer};
use crate::random::Random;
use crate::weapons::car_box;

// the weapons by their index in weapons::WEAPONS
const SHOCKWAVE: usize = 0;
const FIREWORK: usize = 1;
const FIREWORK_PACK: usize = 2;
pub const PUTTY_BOMB: usize = 3;
const WATER_BALLOONS: usize = 4;
const ELECTRO_PULSE: usize = 5;
const OIL_SLICK: usize = 6;
const BALL_BEARING: usize = 7;
const TURBO: usize = 8;
const CLONE: usize = 9;
const GLOBAL_PULSE: usize = 10;

// the skin over the body like the game: an additive copy of the body (every vertex this many m out along its normal) whose uvs circle over a spot of fxpage1 - texels: centre x, y, radius
const SKIN_OFFSET: f32 = 0.02;
const SKIN_FILE: &str = "models/fxpage1.png";
const ZAP_SPOT: [f32; 3] = [96.0, 96.0, 24.0];
const TURBO_SPOT: [f32; 3] = [96.0, 160.0, 10.0];
// rad/s the spot turns (the game: every vertex 1-6 / 1-3 rad/s on its own)
const ZAP_SPIN: f32 = 3.5;
const TURBO_SPIN: f32 = 2.0;
// brightness: zapped per s left (the game 80 / 255), pulse carrier, turbo (192 / 255) fading in and out; all × the glow (above 1 blooms)
const ZAP_BRIGHT: f32 = 0.31;
const PULSE_BRIGHT: f32 = 0.45;
const TURBO_BRIGHT: f32 = 0.75;
const SKIN_FADE: f32 = 0.5;
const SKIN_GLOW: f32 = 1.6;
// the bomb carrier: black with a red glow of the turbo spot, pulsing (1/s) faster as the fuse burns - own, the game throws sparks off the body
const BOMB_SKIN: Vector3<f32> = Vector3::new(1.0, 0.3, 0.08);
const BOMB_BRIGHT: [f32; 2] = [0.15, 0.7];
const BOMB_PULSE: [f32; 2] = [1.5, 7.0];
// the car with a bomb goes this black, this fast (1/s), back at half the speed
const SOOT: f32 = 0.93;
const SOOT_RATE: f32 = 4.0;

// m up to which a shot is heard at full volume
const SOUND_DISTANCE: f32 = 8.0;

// shockwave: a spinning blue sprite with sparks behind it, flies straight ahead hovering over the ground, bounces off walls,
// lifts and rolls the cars it passes (they fly around it), bursts at a car or after its time
const SHOCK_SPEED: f32 = 50.0;
const SHOCK_SPRITE: f32 = 0.8;
const SHOCK_SPIN: f32 = 3.0;
// the blue sparks it leaves like the game: per s, drifting along at this share of its speed, spread m/s
const SHOCK_TRAIL_RATE: f32 = 90.0;
const SHOCK_TRAIL_DRIFT: f32 = 0.1;
const SHOCK_TRAIL_SPREAD: f32 = 0.5;
// its burst: blue sparks flying off in all directions (m/s)
const SHOCK_BURST_SPARKS: usize = 128;
const SHOCK_BURST_SPEED: [f32; 2] = [5.0, 10.0];
const SHOCK_TIME: f32 = 2.0;
const SHOCK_HOVER: f32 = 0.3;
const SHOCK_RADIUS: f32 = 0.35;
// a face this flat (normal y) is a slope it rides up, a steeper one a wall
const SHOCK_WALL: f32 = 0.6;
// cars this far away (horizontally, vertically) feel it, the lift at the center in gravities and the roll in rad/s²
const SHOCK_REACH: f32 = 9.0;
const SHOCK_REACH_UP: f32 = 1.2;
const SHOCK_LIFT: f32 = 2.8;
const SHOCK_ROLL: f32 = 70.0;
const SHOCK_HIT_CAR: f32 = 0.6;
const SHOCK_BURST_TIME: f32 = 0.3;
const SHOCK_BURST_SIZE: f32 = 1.6;

// firework: a rocket that steers to the car ahead (in a cone, up to this far), the sloppy ones aim beside it
const ROCKET_START_SPEED: f32 = 15.0;
const ROCKET_TOP_SPEED: f32 = 55.0;
const ROCKET_ACCELERATION: f32 = 90.0;
// launched up out of the car, the steeper the farther the target (per m), then it curves down onto it
const ROCKET_CLIMB: f32 = 0.3;
const ROCKET_CLIMB_PER_M: f32 = 0.012;
const ROCKET_TIME: f32 = 1.6;
const ROCKET_RANGE: f32 = 50.0;
const ROCKET_CONE: f32 = 0.5;
const ROCKET_PRECISE: f32 = 0.72;
const ROCKET_TURN: [f32; 2] = [9.0, 3.5];
const ROCKET_MISS_SIDE: [f32; 2] = [1.0, 2.4];
const ROCKET_MISS_UP: [f32; 2] = [0.3, 0.9];
const ROCKET_HIT_CAR: f32 = 0.8;
// rad/s a rocket without a target turns level
const ROCKET_LEVEL: f32 = 1.5;
const ROCKET_SCALE: f32 = 0.45;
// the trail of the game comes out of the back of the rocket, this far behind its center
const ROCKET_NOZZLE: f32 = 0.15;
// the blast: cars this far away, the push away and up (m/s at 1 m, falls with the distance), the nearest distance counted, a random spin
const BLAST_REACH: f32 = 5.0;
const BLAST_PUSH: [f32; 2] = [8.0, 11.0];
const BLAST_UP: [f32; 2] = [2.4, 3.5];
const BLAST_NEAR: f32 = 0.75;
// m beside the center of mass the push hits (random) - the car is thrown and tumbles
const BLAST_OFFSET: f32 = 0.5;
// the particles of the blast like the game: a flash, puffs of smoke, orange and red sparks (kind, count, spread m/s)
pub const BLAST_PARTICLES: [(Kind, usize, f32); 4] = [(Kind::Explosion1, 1, 0.0), (Kind::Smoke2, 3, 0.6), (Kind::SmallOrange, 30, 12.0), (Kind::SmallRed, 30, 14.0)];

// water balloon: thrown ahead and up, it bounces until the bounces used up its life (m/s of impact), bursts at a car or after its time
const BALLOON_THROW: f32 = 24.0;
const BALLOON_THROW_UP: f32 = 4.5;
const BALLOON_LIFE: [f32; 2] = [4.0, 12.0];
const BALLOON_TIME: f32 = 8.0;
const BALLOON_RADIUS: f32 = 0.12;
const BALLOON_BOUNCE: f32 = 0.5;
const BALLOON_HIT_CAR: f32 = 0.15;
// rad/s it tumbles forward
const BALLOON_TUMBLE: f32 = 15.0;
const SPLASH_REACH: f32 = 6.0;
const SPLASH_PUSH: [f32; 2] = [5.0, 7.5];
const SPLASH_UP: [f32; 2] = [0.55, 2.4];
const SPLASH_NEAR: f32 = 0.5;
const SPLASH_OFFSET: f32 = 0.25;
// drops of the burst (own - the game shows none): count, up m/s, spread
const SPLASH_DROPS: (usize, f32, f32) = (34, 2.0, 3.0);

// putty bomb: on the car at once - the fuse is its aerial burning down (it grows back at this rate, 1/s), the time the giver is safe from
// getting it back, the blast on the car (up, spin) and around it
const BOMB_FUSE: f32 = 10.0;
const AERIAL_REGROW: f32 = 1.0;
const BOMB_IMMUNE: f32 = 3.0;
// the blast throws the car up from the ground, hit beside its center (m, horizontal) so it turns over
const BOMB_UP: f32 = 7.5;
const BOMB_OFFSET: [f32; 2] = [0.3, 0.9];
const BOMB_REACH: f32 = 2.5;
const BOMB_PUSH: [f32; 2] = [4.0, 6.0];
// the burning fuse sparks at the top of the aerial (per s, spread m/s) like the game, every 0.1 s smoke rises off 8 points of the body (m/s, spread)
const FUSE_SPARK_RATE: f32 = 120.0;
const FUSE_SPARK_SPREAD: f32 = 0.5;
const BOMB_SMOKE_EVERY: f32 = 0.1;
const BOMB_SMOKE_RISE: f32 = 0.8;
const BOMB_SMOKE_SPREAD: f32 = 0.16;

// electro pulse: carried this long, it zaps the cars this close with a free line, they have no drive for a while and are safe for a while after
const PULSE_TIME: f32 = 9.0;
const PULSE_REACH: f32 = 3.6;
const ZAP_TIME: f32 = 3.0;
const ZAP_SAFE: f32 = 2.5;
// 1/s a zapped car loses its speed over the ground
const ZAP_DRAG: f32 = 1.5;
// share of the steering speed a zapped car keeps (the game: a third)
const ZAP_STEER: f32 = 0.33;
const PULSE_GLOW: Vector3<f32> = Vector3::new(0.3, 0.5, 1.4);
// the arcs spark at both ends: per s, spread m/s
const ARC_SPARK_RATE: f32 = 50.0;
const ARC_SPARK_SPREAD: f32 = 2.0;
// the global pulse flashes every screen white for this long
const FLASH_TIME: f32 = 0.3;

// turbo: the boost of power at the start, down to none at the end, and the grip on top
const TURBO_TIME: f32 = 8.0;
const TURBO_POWER: f32 = 0.9;
const TURBO_GRIP: f32 = 1.2;

// oil slick: dropped for this long, a blob every this far, they grow to their size and stay a while - a car on one slides
const OIL_DROP_TIME: f32 = 1.0;
// one puddle, no trail: at most this many slicks, the next one after this far, each spreads from the start size to its own (m, 1/s)
const OIL_DROPS: usize = 3;
const OIL_SPACING: f32 = 0.62;
const OIL_SIZES: [f32; OIL_DROPS] = [1.25, 1.1, 0.95];
const OIL_START_SIZE: f32 = 0.3;
const OIL_SPREAD: f32 = 1.0;
const OIL_TIME: f32 = 28.0;
const OIL_FADE: f32 = 1.0;
const OIL_LIFT: f32 = 0.015;
// m around a blob that still counts for a tire (its width)
const OIL_TOUCH: f32 = 0.08;
// the grip on oil, the oily tires get it back up to this share over this many s, then all of it - and leave marks all the while (fading)
const OIL_GRIP: f32 = 0.06;
const OIL_RECOVER_GRIP: f32 = 0.55;
const OIL_RECOVER: f32 = 2.2;
// the oily tires leave black marks at full strength (the game: full color the whole time), fading out only in the last s of it
const OIL_MARK_FADE: f32 = 0.4;
const OIL_BLOBS: usize = 48;
// s the dropper does not slide on what it just dropped
const OIL_OWNER_SAFE: f32 = 1.5;
const OIL_COLOR: Vector3<f32> = Vector3::new(0.02, 0.02, 0.025);

// ball bearing: a heavy chrome ball (3 × the mass of a car of mass 1.5) rolled out behind the car, gone after a while
const BALL_RADIUS: f32 = 0.5;
const BALL_SHINE: f32 = 0.15;
// plain gold (on the gray of the model), lit with a highlight
const BALL_COLOR: Vector3<f32> = Vector3::new(1.6, 1.15, 0.3);
const BALL_MASS: f32 = 45.0;
const BALL_BACK_SPEED: f32 = 5.0;
const BALL_TIME: f32 = 25.0;
const BALL_LOUD_SPEED: f32 = 8.0;

// clone pickup: a fake pickup behind the car - whoever takes it gets the bomb at once (the dropper only after a while)
const CLONE_BACK: f32 = 0.6;
const CLONE_HALF: f32 = 0.3;
const CLONE_OWNER_SAFE: f32 = 2.0;
const CLONE_TIME: f32 = 60.0;

// what flies or lies around: the node, its looping sound
struct Thing
{
    node: NodeItem,
    sound: Option<ComponentItem>,
}

struct Shockwave
{
    thing: Thing,
    second: NodeItem,
    trail_due: f32,
    owner: usize,
    position: Vector3<f32>,
    direction: Vector3<f32>,
    age: f32,
}

struct Rocket
{
    thing: Thing,
    owner: usize,
    target: Option<usize>,
    position: Vector3<f32>,
    velocity: Vector3<f32>,
    offset: Vector3<f32>,
    turn: f32,
    age: f32,
    trail: Option<TrailId>,
}

struct Balloon
{
    thing: Thing,
    // it tumbles forward about the right axis of the car that threw it
    axis: Vector3<f32>,
    owner: usize,
    position: Vector3<f32>,
    velocity: Vector3<f32>,
    life: f32,
    age: f32,
}

struct Ball
{
    thing: Thing,
    // the mesh node with the body
    body: NodeItem,
    previous: Option<Vector3<f32>>,
    age: f32,
}

struct Clone
{
    node: NodeItem,
    owner: usize,
    position: Vector3<f32>,
    age: f32,
}

struct Blob
{
    position: Vector3<f32>,
    normal: Vector3<f32>,
    // the size it spreads to, turned about its normal
    size: f32,
    yaw: f32,
    owner: usize,
    age: f32,
}

// a burst that grows and fades, then goes
struct Burst
{
    node: NodeItem,
    age: f32,
    time: f32,
    start: f32,
    size: f32,
}

#[derive(Default)]
struct CarState
{
    // s without drive, then s safe from the next zap
    zap: f32,
    zap_safe: f32,
    // glowing copies of the body on the car, both additive at once like the objects of the game: [the lightning (zapped, pulse), the glow (turbo, bomb)] - (the copy, the body of the car), their spot turns, the bomb pulse
    skins: [Option<(NodeItem, NodeItem)>; 2],
    skin_spins: [f32; 2],
    bomb_phase: f32,
    // 0..1 how black the car is from the bomb (car_light.rs puts it on its parts)
    soot: f32,
    pulse: f32,
    pulse_sound: Option<ComponentItem>,
    turbo: f32,
    turbo_sound: Option<ComponentItem>,
    // the fuse left and its sound on the car (None right after it was passed on)
    bomb: Option<(f32, Option<ComponentItem>)>,
    bomb_safe: f32,
    // the aerial as the converter made it, how much of it burnt down (0 = all there)
    aerial: Option<(NodeItem, Vector3<f32>)>,
    aerial_burnt: f32,
    aerial_shown: f32,
    oil_drop: f32,
    oil_last: Option<Vector3<f32>>,
    oil_dropped: usize,
    // per wheel: s the tire stays oily after the oil
    oily: Vec<f32>,
    // the top of the aerial (the fuse sparks there), s to the next smoke of the bomb
    aerial_top: Option<Option<NodeItem>>,
    bomb_smoke: f32,
}

// a car this frame
#[derive(Clone, Copy)]
struct Car
{
    id: u32,
    position: Vector3<f32>,
    forward: Vector3<f32>,
    up: Vector3<f32>,
    velocity: Vector3<f32>,
    min: Vector3<f32>,
    max: Vector3<f32>,
    half_length: f32,
    active: bool,
}

// what the second half of an update does with the state: sounds and bursts
#[derive(Default)]
struct Out
{
    sounds: Vec<(&'static str, Vector3<f32>, f32)>,
    // particles: kind, count, position, velocity, spread
    emits: Vec<(Kind, usize, Vector3<f32>, Vector3<f32>, f32)>,
    // the trails of the rockets gone
    trail_ends: Vec<TrailId>,
}

pub struct Shots
{
    scene_id: u32,
    assets: String,
    pickup_model: Option<String>,
    random: Random,
    cars: Vec<CarState>,
    shockwaves: Vec<Shockwave>,
    rockets: Vec<Rocket>,
    balloons: Vec<Balloon>,
    balls: Vec<Ball>,
    clones: Vec<Clone>,
    bursts: Vec<Burst>,
    blobs: Vec<Blob>,
    oil: Option<(NodeItem, Vec<rustl::state::scene::node::InstanceItemArc>)>,
    lightning: Option<Option<Lightning>>,
    // fxpage1 for the skins, once
    skin_texture: Option<Option<rustl::state::resources::texture::TextureItem>>,
    // the arcs and beams of this frame (start, end, shape: 1 arc, 0 beam)
    arcs: Vec<(Vector3<f32>, Vector3<f32>, f32)>,
    // per racer: the car its firework would go for (only while it holds one)
    targets: Vec<Option<usize>>,
    // s left of the flash of the global pulse
    flash: f32,
}

impl Shots
{
    pub fn new(scene: &Scene, assets: &str, racers: usize) -> Self
    {
        let mut pickup_model = None;
        crate::for_each_node(&scene.nodes, &mut |node|
        {
            let read = node.read().unwrap();
            if pickup_model.is_none() && read.tags.contains("pickup") { pickup_model = read.source.as_ref().map(|source| source.origin_path.clone()); }
        });
        Self
        {
            scene_id: scene.id, assets: assets.to_string(), pickup_model, random: Random::default(), cars: (0..racers).map(|_| CarState::default()).collect(),
            shockwaves: vec![], rockets: vec![], balloons: vec![], balls: vec![], clones: vec![], bursts: vec![], blobs: vec![], oil: None, lightning: None, skin_texture: None,
            arcs: vec![], targets: vec![], flash: 0.0,
        }
    }

    // the cars racing - finished cars still get hit, the clone and the fireworks look for the active ones
    fn cars(scene: &Scene, race: &Race) -> Vec<Car>
    {
        race.racers.iter().map(|racer| car(scene, racer)).collect()
    }

    // a weapon goes off for a car
    pub fn fire(&mut self, state: &mut State, race: &Race, index: usize, weapon: usize)
    {
        self.cars.resize_with(race.racers.len(), CarState::default);
        let cars =
        {
            let Some(scene) = state.find_scene_by_id(self.scene_id) else { return; };
            Self::cars(scene, race)
        };
        let Some(car) = cars.get(index).copied() else { return; };
        match weapon
        {
            SHOCKWAVE =>
            {
                let direction = horizontal(car.forward);
                let position = car.position + direction * (car.half_length + SHOCK_RADIUS) + car.up * 0.15;
                let Some(thing) = self.spawn(state, "shock_sprite", "shockwave", Some("shock")) else { return; };
                let Some(second) = self.spawn(state, "shock_sprite", "shockwave", None) else { if let Some(scene) = state.find_scene_by_id_mut(self.scene_id) { remove(scene, thing); } return; };
                place(&thing.node, position, UnitQuaternion::identity(), SHOCK_SPRITE);
                place(&second.node, position, UnitQuaternion::identity(), SHOCK_SPRITE);
                self.shockwaves.push(Shockwave { thing, second: second.node, trail_due: 0.0, owner: index, position, direction, age: 0.0 });
                self.sound(state, "shockfire", position, 1.0);
            },
            FIREWORK | FIREWORK_PACK =>
            {
                let position = car.position + car.forward * 0.1 + car.up * 0.3;
                let target = self.targets.get(index).copied().flatten().or_else(|| target(&cars, index));
                let precise = self.random.next() < ROCKET_PRECISE;
                let offset = if precise { Vector3::zeros() } else
                {
                    let side = between(&mut self.random, ROCKET_MISS_SIDE) * if self.random.next() < 0.5 { -1.0 } else { 1.0 };
                    car.forward.cross(&Vector3::y()).try_normalize(1e-4).unwrap_or_else(Vector3::x) * side + Vector3::y() * between(&mut self.random, ROCKET_MISS_UP)
                };
                let (ahead, climb) = match target.and_then(|target| cars.get(target))
                {
                    Some(other) => (horizontal(other.position - car.position), ROCKET_CLIMB + (other.position - car.position).norm() * ROCKET_CLIMB_PER_M),
                    None => (car.forward, ROCKET_CLIMB),
                };
                let velocity = car.velocity + (ahead + Vector3::y() * climb).normalize() * ROCKET_START_SPEED;
                let Some(thing) = self.spawn(state, "firework", "firework", Some("firefire")) else { return; };
                place(&thing.node, position, along(velocity), ROCKET_SCALE);
                let turn = if precise { ROCKET_TURN[0] } else { ROCKET_TURN[1] };
                self.rockets.push(Rocket { thing, owner: index, target, position, velocity, offset, turn, age: 0.0, trail: None });
            },
            PUTTY_BOMB => self.give_bomb(state, race, index, BOMB_FUSE),
            WATER_BALLOONS =>
            {
                let position = car.position + car.forward * (car.half_length + 0.1) + car.up * 0.2;
                let velocity = car.velocity + car.forward * BALLOON_THROW + Vector3::y() * BALLOON_THROW_UP;
                let Some(thing) = self.spawn(state, "water_balloon", "water_balloon", Some("wbomb")) else { return; };
                place(&thing.node, position, UnitQuaternion::identity(), 1.0);
                let life = between(&mut self.random, BALLOON_LIFE);
                let axis = car.forward.cross(&car.up).try_normalize(1e-4).unwrap_or_else(Vector3::x);
                self.balloons.push(Balloon { thing, axis, owner: index, position, velocity, life, age: 0.0 });
                self.sound(state, "wbombfire", position, 1.0);
            },
            ELECTRO_PULSE =>
            {
                self.load_lightning(state);
                self.cars[index].pulse = PULSE_TIME;
                if self.cars[index].pulse_sound.is_none()
                {
                    let node = race.racers[index].node.clone();
                    self.cars[index].pulse_sound = loop_sound(state, &self.assets, &node, "electro");
                }
            },
            OIL_SLICK =>
            {
                self.make_oil(state);
                let car_state = &mut self.cars[index];
                car_state.oil_drop = OIL_DROP_TIME;
                car_state.oil_last = None;
                car_state.oil_dropped = 0;
                self.sound(state, "oildrop", car.position, 1.0);
            },
            BALL_BEARING => self.roll_ball(state, race, &car),
            TURBO =>
            {
                let car_state = &mut self.cars[index];
                if car_state.zap > 0.0
                {
                    car_state.zap = 0.0;
                    car_state.zap_safe = ZAP_SAFE;
                    self.sound(state, "shock", car.position, 1.0);
                    return;
                }
                car_state.turbo = TURBO_TIME;
                if car_state.turbo_sound.is_none()
                {
                    let node = race.racers[index].node.clone();
                    self.cars[index].turbo_sound = loop_sound(state, &self.assets, &node, "turbo");
                }
            },
            CLONE =>
            {
                let Some(path) = self.pickup_model.clone() else { return; };
                let Ok(node) = crate::field::show_model(state, self.scene_id, &path, "clone_pickup") else { return; };
                let position = car.position - horizontal(car.forward) * (car.half_length + CLONE_BACK);
                let position = self.ground(state, position).map(|(point, _)| point + Vector3::y() * 0.35).unwrap_or(position);
                place(&node, position, UnitQuaternion::identity(), 1.0);
                // like the real ones: no shadow of itself on itself
                crate::for_each_node(&vec![node.clone()], &mut |part| crate::change_material(part, |m| m.receive_shadow = false));
                self.clones.push(Clone { node, owner: index, position, age: 0.0 });
            },
            GLOBAL_PULSE =>
            {
                self.flash = FLASH_TIME;
                for (other, other_car) in cars.iter().enumerate().filter(|(other, _)| *other != index)
                {
                    self.cars[other].zap = ZAP_TIME;
                    self.sound(state, "electrozap", other_car.position, 1.0);
                }
            },
            _ => {},
        }
    }

    // free_hands: per racer, could take a pickup - held: per racer, the weapon in the hand
    pub fn update(&mut self, state: &mut State, race: &Race, free_hands: &[bool], held: &[Option<usize>], particles: &mut Particles, dt: f32)
    {
        self.cars.resize_with(race.racers.len(), CarState::default);
        self.flash = (self.flash - dt).max(0.0);
        let mut out = Out::default();
        {
            let Some(scene) = state.find_scene_by_id_mut(self.scene_id) else { return; };
            let cars = Self::cars(scene, race);
            self.targets = (0..cars.len()).map(|index| matches!(held.get(index), Some(Some(FIREWORK | FIREWORK_PACK))).then(|| target(&cars, index)).flatten()).collect();
            self.arcs.clear();
            let gravity = scene.physics.settings.gravity.norm().max(0.1);
            if dt > 0.0
            {
                self.update_shockwaves(scene, &cars, gravity, dt, &mut out);
                self.update_rockets(scene, &cars, dt, &mut out);
                self.update_balloons(scene, &cars, gravity, dt, &mut out);
                self.update_balls(scene, dt);
                self.update_clones(scene, &cars, free_hands, dt, &mut out);
                self.update_cars(scene, &cars, dt, &mut out);
                self.update_oil(scene, &cars, dt, &mut out);
                self.update_bursts(scene, dt);
            }
            let arcs = std::mem::take(&mut self.arcs);
            if let Some(Some(lightning)) = self.lightning.as_mut() { lightning.draw_shaped(scene, &arcs, dt); }
            self.apply(scene, &cars);
        }
        for (name, position, volume) in out.sounds
        {
            self.sound(state, name, position, volume);
        }
        // a zapped car, one with the pulse, the turbo or the bomb gets its skin (once per car, hidden after); a bomb passed on gets its fuse sound
        for index in 0..self.cars.len().min(race.racers.len())
        {
            let node = race.racers[index].node.clone();
            if self.cars[index].bomb.as_ref().is_some_and(|(_, sound)| sound.is_none())
            {
                let sound = loop_sound(state, &self.assets, &node, "fuse");
                if let Some((_, slot)) = self.cars[index].bomb.as_mut() { *slot = sound; }
            }
            let car_state = &self.cars[index];
            let wanted = [car_state.zap > 0.0 || car_state.pulse > 0.0, car_state.turbo > 0.0 || car_state.bomb.is_some()];
            let Some(layer) = (0..2).find(|&layer| wanted[layer] && car_state.skins[layer].is_none()) else { continue; };
            if self.skin_texture.is_none()
            {
                let path = format!("{}/{SKIN_FILE}", self.assets);
                let max_resolution = state.max_texture_resolution();
                self.skin_texture = Some(state.load_texture_or_reuse(&path, Some("png".to_string()), max_resolution).map_err(|error| { console_warning!("shots: {}: {} - plain skins", path, error); }).ok());
            }
            let texture = self.skin_texture.clone().flatten();
            self.cars[index].skins[layer] = skin(state, self.scene_id, &node, texture, &mut self.random);
        }
        for (kind, count, position, velocity, spread) in out.emits
        {
            particles.burst(self.scene_id, kind, count, position, velocity, spread);
        }
        for rocket in &mut self.rockets
        {
            let nozzle = rocket.position - rocket.velocity.try_normalize(1e-4).unwrap_or_else(Vector3::y) * ROCKET_NOZZLE;
            match rocket.trail
            {
                Some(trail) => particles.trail_move(trail, nozzle),
                None => rocket.trail = Some(particles.trail_start(self.scene_id, TrailKind::Firework, nozzle)),
            }
        }
        for trail in out.trail_ends
        {
            particles.trail_end(trail);
        }
    }

    // Play stops or the race restarts: everything goes, the cars drive normally again
    // how dark each car is from the bomb: its node id and the shade (1 = its own colors)
    pub fn shades(&self, race: &Race) -> Vec<(u32, f32)>
    {
        self.cars.iter().zip(&race.racers).map(|(car, racer)| (racer.node.read().unwrap().id, 1.0 - SOOT * car.soot)).collect()
    }

    pub fn stop(&mut self, state: &mut State, race: &Race)
    {
        let Some(scene) = state.find_scene_by_id_mut(self.scene_id) else { return; };
        let mut things: Vec<Thing> = vec![];
        for shot in self.shockwaves.drain(..)
        {
            delete(scene, &shot.second);
            things.push(shot.thing);
        }
        things.extend(self.rockets.drain(..).map(|shot| shot.thing));
        things.extend(self.balloons.drain(..).map(|shot| shot.thing));
        for ball in self.balls.drain(..)
        {
            scene.physics.remove_node(ball.body.read().unwrap().id);
            things.push(ball.thing);
        }
        for (index, car_state) in self.cars.drain(..).enumerate()
        {
            for (copy, _) in car_state.skins.into_iter().flatten() { delete_kept(scene, &copy); }
            if let Some((aerial, scale)) = car_state.aerial { set_scale(&aerial, scale); }
            let Some(racer) = race.racers.get(index) else { continue; };
            for sound in [car_state.turbo_sound, car_state.pulse_sound, car_state.bomb.and_then(|(_, sound)| sound)].into_iter().flatten() { stop_sound(&racer.node, &sound); }
        }
        for thing in things { remove(scene, thing); }
        for clone in self.clones.drain(..) { delete(scene, &clone.node); }
        for burst in self.bursts.drain(..) { delete(scene, &burst.node); }
        if let Some((node, _)) = self.oil.take() { delete(scene, &node); }
        self.blobs.clear();
        if let Some(Some(lightning)) = self.lightning.take() { scene.delete_node_by_id(lightning.root_id(), true, false, true, false); }
        for racer in &race.racers
        {
            let id = racer.node.read().unwrap().id;
            with_vehicle(scene, id, |vehicle| { vehicle.throttle_limit = 1.0; vehicle.power_scale = 1.0; vehicle.grip_scale = 1.0; vehicle.steer_rate_scale = 1.0; vehicle.wheel_grip_scale.clear(); vehicle.wheel_mark_force.clear(); });
        }
    }

    // ********** the shots **********

    fn update_shockwaves(&mut self, scene: &mut Scene, cars: &[Car], gravity: f32, dt: f32, out: &mut Out)
    {
        let mut index = 0;
        while index < self.shockwaves.len()
        {
            let shot = &mut self.shockwaves[index];
            shot.age += dt;
            let step = SHOCK_SPEED * dt;
            // a wall turns it away like a ball, a slope it rides up
            if let Some((_, normal)) = cast(scene, shot.position, shot.direction, step + SHOCK_RADIUS).filter(|(_, normal)| normal.y < SHOCK_WALL)
            {
                let across = Vector3::new(normal.x, 0.0, normal.z).try_normalize(1e-4).unwrap_or(-shot.direction);
                shot.direction = horizontal(shot.direction - across * 2.0 * shot.direction.dot(&across));
            }
            shot.position += shot.direction * step;
            // hovering over the ground, falling where there is none
            match cast(scene, shot.position + Vector3::y() * 0.5, -Vector3::y(), 0.5 + SHOCK_HOVER * 4.0)
            {
                Some((distance, _)) => shot.position.y += (0.5 + SHOCK_HOVER - distance) * (1.0 - (-20.0 * dt).exp()),
                None => shot.position.y -= gravity * 0.5 * dt,
            }

            // blue sparks behind it
            shot.trail_due += SHOCK_TRAIL_RATE * dt;
            let trail = shot.trail_due as usize;
            shot.trail_due -= trail as f32;
            out.emits.push((Kind::Blue, trail, shot.position, shot.direction * SHOCK_SPEED * SHOCK_TRAIL_DRIFT, SHOCK_TRAIL_SPREAD));

            // the cars it passes go up and roll
            let mut hit_car = false;
            for (other, car) in cars.iter().enumerate().filter(|(other, _)| *other != shot.owner)
            {
                let offset = car.position - shot.position;
                let across = Vector3::new(offset.x, 0.0, offset.z).norm();
                if across < SHOCK_HIT_CAR && offset.y.abs() < SHOCK_HIT_CAR { hit_car = true; }
                if across >= SHOCK_REACH || offset.y.abs() >= SHOCK_REACH_UP { continue; }
                let strength = 1.0 - (across / SHOCK_REACH).powi(2);
                // like the game: always about the own look axis, wherever the ball is
                // push(scene, car.id, Vector3::y() * strength * SHOCK_LIFT * gravity * dt, car.forward * strength * SHOCK_ROLL * dt);
                // own (user wish): tipped away from the ball - right over it: along its way
                let away = Vector3::new(offset.x, 0.0, offset.z).try_normalize(0.05).unwrap_or_else(|| horizontal(shot.direction));
                push(scene, car.id, Vector3::y() * strength * SHOCK_LIFT * gravity * dt, Vector3::y().cross(&away) * strength * SHOCK_ROLL * dt);
                let _ = other;
            }

            // two sprites turned to the camera, spinning against each other
            let facing = UnitQuaternion::face_towards(&(eye(scene, shot.position) - shot.position), &Vector3::y());
            let angle = shot.age * SHOCK_SPIN;
            place(&shot.thing.node, shot.position, facing * UnitQuaternion::from_axis_angle(&Vector3::z_axis(), angle), SHOCK_SPRITE);
            place(&shot.second, shot.position, facing * UnitQuaternion::from_axis_angle(&Vector3::z_axis(), -angle * 1.3), SHOCK_SPRITE);
            if !(hit_car || shot.age >= SHOCK_TIME) { index += 1; continue; }

            // the burst: the sprite grows and fades
            let shot = self.shockwaves.remove(index);
            if let Some(sound) = &shot.thing.sound { stop_sound(&shot.thing.node, sound); }
            delete(scene, &shot.second);
            self.bursts.push(Burst { node: shot.thing.node, age: 0.0, time: SHOCK_BURST_TIME, start: SHOCK_SPRITE, size: SHOCK_BURST_SIZE * 1.5 });
            out.sounds.push(("shock", shot.position, 0.7));
            for _ in 0..SHOCK_BURST_SPARKS
            {
                let direction = random_direction(&mut self.random);
                out.emits.push((Kind::BigBlue, 1, shot.position, direction * between(&mut self.random, SHOCK_BURST_SPEED), 0.0));
            }
        }
    }

    fn update_rockets(&mut self, scene: &mut Scene, cars: &[Car], dt: f32, out: &mut Out)
    {
        let mut index = 0;
        while index < self.rockets.len()
        {
            let rocket = &mut self.rockets[index];
            rocket.age += dt;
            let speed = (ROCKET_START_SPEED + ROCKET_ACCELERATION * rocket.age).min(ROCKET_TOP_SPEED).max(rocket.velocity.norm().min(ROCKET_TOP_SPEED));
            let direction = rocket.velocity.try_normalize(1e-4).unwrap_or_else(Vector3::y);

            // to where the target will be, the sloppy ones beside it
            let direction = match rocket.target.and_then(|target| cars.get(target))
            {
                Some(car) =>
                {
                    let lead = ((car.position - rocket.position).norm() / speed).min(0.6);
                    let aim = car.position + car.velocity * lead + rocket.offset;
                    turn_towards(direction, (aim - rocket.position).try_normalize(1e-4).unwrap_or(direction), rocket.turn * dt)
                },
                None => turn_towards(direction, horizontal(direction), ROCKET_LEVEL * dt),
            };
            rocket.velocity = direction * speed;
            let step = speed * dt;
            let wall = cast(scene, rocket.position, direction, step).map(|(distance, _)| distance);
            rocket.position += direction * wall.unwrap_or(step);
            let near_car = cars.iter().enumerate().any(|(other, car)| other != rocket.owner && (car.position - rocket.position).norm() < ROCKET_HIT_CAR);
            place(&rocket.thing.node, rocket.position, along(rocket.velocity), ROCKET_SCALE);
            if !(wall.is_some() || near_car || rocket.age >= ROCKET_TIME) { index += 1; continue; }

            let rocket = self.rockets.remove(index);
            out.trail_ends.extend(rocket.trail);
            remove(scene, rocket.thing);
            blast(scene, cars, rocket.position, BLAST_REACH, BLAST_PUSH, BLAST_UP, BLAST_NEAR, BLAST_OFFSET, &mut self.random);
            out.sounds.push(("firebang", rocket.position, 1.0));
            blast_particles(out, rocket.position);
        }
    }

    fn update_balloons(&mut self, scene: &mut Scene, cars: &[Car], gravity: f32, dt: f32, out: &mut Out)
    {
        let mut index = 0;
        while index < self.balloons.len()
        {
            let balloon = &mut self.balloons[index];
            balloon.age += dt;
            balloon.velocity.y -= gravity * dt;
            let step = balloon.velocity * dt;
            let length = step.norm();
            let mut burst = balloon.age >= BALLOON_TIME;
            match (length > 1e-5).then(|| cast(scene, balloon.position, step / length, length + BALLOON_RADIUS)).flatten()
            {
                Some((distance, normal)) =>
                {
                    let impact = -balloon.velocity.dot(&normal);
                    balloon.position += step / length * (distance - BALLOON_RADIUS).max(0.0);
                    balloon.life -= impact.max(0.0);
                    if balloon.life < 0.0 { burst = true; }
                    else
                    {
                        let along_surface = balloon.velocity + normal * impact;
                        balloon.velocity = along_surface * 0.85 + normal * impact * BALLOON_BOUNCE;
                        if impact > 1.0 { out.sounds.push(("wbombbounce", balloon.position, (impact / 8.0).min(1.0))); }
                    }
                },
                None => balloon.position += step,
            }
            // a car it touches - its thrower only once it is away
            let hit = cars.iter().enumerate().any(|(other, car)| (other != balloon.owner || balloon.age > 0.4) && inside(car, balloon.position, BALLOON_HIT_CAR));
            let wobble = 1.0 + 0.1 * (balloon.age * 9.0).sin();
            let tumble = UnitQuaternion::from_axis_angle(&rustl::nalgebra::Unit::new_normalize(balloon.axis), -BALLOON_TUMBLE * balloon.age);
            place_scaled(&balloon.thing.node, balloon.position, tumble, Vector3::new(wobble, 2.0 - wobble, wobble));
            if !(burst || hit) { index += 1; continue; }

            let balloon = self.balloons.remove(index);
            remove(scene, balloon.thing);
            blast(scene, cars, balloon.position, SPLASH_REACH, SPLASH_PUSH, SPLASH_UP, SPLASH_NEAR, SPLASH_OFFSET, &mut self.random);
            out.sounds.push(("wbombhit", balloon.position, 1.0));
            let (drops, up, spread) = SPLASH_DROPS;
            out.emits.push((Kind::Water, drops, balloon.position, Vector3::y() * up, spread));
        }
    }

    fn update_balls(&mut self, scene: &mut Scene, dt: f32)
    {
        let mut index = 0;
        while index < self.balls.len()
        {
            let ball = &mut self.balls[index];
            ball.age += dt;
            let position = ball.body.read().unwrap().instances.get_ref().first().map(|instance| instance.read().unwrap().calculate_transform().column(3).xyz());
            if let (Some(position), Some(sound)) = (position, ball.thing.sound.as_ref())
            {
                let speed = ball.previous.map_or(0.0, |previous| (position - previous).norm() / dt);
                let level = (speed / BALL_LOUD_SPEED).min(1.0);
                component_downcast_mut!(sound, Sound);
                sound.set_fixed_position(Some(Point3::from(position)));
                sound.set_runtime_modulation(level, 0.8 + 0.4 * level);
            }
            ball.previous = position;
            if ball.age < BALL_TIME { index += 1; continue; }
            let ball = self.balls.remove(index);
            scene.physics.remove_node(ball.body.read().unwrap().id);
            remove(scene, ball.thing);
        }
    }

    fn update_clones(&mut self, scene: &mut Scene, cars: &[Car], free_hands: &[bool], dt: f32, out: &mut Out)
    {
        let mut index = 0;
        while index < self.clones.len()
        {
            let clone = &mut self.clones[index];
            clone.age += dt;
            let spin = UnitQuaternion::from_axis_angle(&Vector3::y_axis(), clone.age * std::f32::consts::PI);
            place(&clone.node, clone.position, spin, 1.0);
            let taker = cars.iter().enumerate().position(|(other, car)| (other != clone.owner || clone.age > CLONE_OWNER_SAFE) && free_hands.get(other).copied().unwrap_or(false) && inside(car, clone.position, CLONE_HALF));
            if taker.is_none() && clone.age < CLONE_TIME { index += 1; continue; }
            let clone = self.clones.remove(index);
            delete(scene, &clone.node);
            let Some(taker) = taker else { continue; };
            out.sounds.push(("pickup", clone.position, 1.0));
            self.explode(scene, cars, taker, out);
        }
    }

    // ********** what the cars carry **********

    fn update_cars(&mut self, scene: &mut Scene, cars: &[Car], dt: f32, out: &mut Out)
    {
        for index in 0..self.cars.len().min(cars.len())
        {
            let car = cars[index];
            let car_state = &mut self.cars[index];
            let was_zapped = car_state.zap > 0.0;
            car_state.zap = (car_state.zap - dt).max(0.0);
            car_state.zap_safe = (car_state.zap_safe - dt).max(0.0);
            if was_zapped && car_state.zap <= 0.0 { car_state.zap_safe = ZAP_SAFE; }
            if car_state.zap > 0.0
            {
                let keep = (-ZAP_DRAG * dt).exp();
                if let Some((_, body)) = scene.physics.vehicle_mut(car.id)
                {
                    let velocity = body.linvel();
                    body.set_linvel(Vector::new(velocity.x * keep, velocity.y, velocity.z * keep), true);
                }
            }
            car_state.turbo = (car_state.turbo - dt).max(0.0);
            for oily in &mut car_state.oily { *oily = (*oily - dt).max(0.0); }
            car_state.bomb_safe = (car_state.bomb_safe - dt).max(0.0);
            if car_state.turbo <= 0.0 { if let Some(sound) = car_state.turbo_sound.take() { stop_sound(&car_node(scene, car.id), &sound); } }

            // the pulse: arcs to the cars close by, a zap for the ones not safe
            if car_state.pulse > 0.0
            {
                car_state.pulse -= dt;
                for (other, other_car) in cars.iter().enumerate().filter(|(other, _)| *other != index)
                {
                    if (other_car.position - car.position).norm() >= PULSE_REACH || !sight(scene, car.position, other_car.position) { continue; }
                    let (from, to) = crate::battle::arc_ends(car.position, other_car.position);
                    self.arcs.push((from, to, 1.0));
                    for end in [from, to]
                    {
                        let count = due(&mut self.random, ARC_SPARK_RATE, dt);
                        out.emits.push((Kind::Electric, count, end, Vector3::zeros(), ARC_SPARK_SPREAD));
                    }
                    if self.cars[other].zap <= 0.0 && self.cars[other].zap_safe <= 0.0
                    {
                        self.cars[other].zap = ZAP_TIME;
                        out.sounds.push(("electrozap", other_car.position, 1.0));
                    }
                }
                let car_state = &mut self.cars[index];
                if car_state.pulse <= 0.0
                {
                    car_state.pulse = 0.0;
                    if let Some(sound) = car_state.pulse_sound.take() { stop_sound(&car_node(scene, car.id), &sound); }
                }
            }

            // the bomb: the fuse burns down (the aerial with it), a touch passes it on - with the time left
            let fuse = self.cars[index].bomb.as_mut().map(|(fuse, _)| { *fuse -= dt; *fuse });
            if let Some(fuse) = fuse
            {
                self.cars[index].aerial_burnt = (1.0 - fuse / BOMB_FUSE).clamp(0.0, 1.0);
                if fuse <= 0.0
                {
                    self.explode(scene, cars, index, out);
                    continue;
                }
                let car_state = &mut self.cars[index];
                let top = car_state.aerial_top.get_or_insert_with(|| find_child(&car_node(scene, car.id), "aerial_top")).as_ref().map(|top| top.read().unwrap().get_full_transform().column(3).xyz());
                let count = due(&mut self.random, FUSE_SPARK_RATE, dt);
                out.emits.push((Kind::Spark2, count, top.unwrap_or(car.position + car.up * 0.3), car.velocity, FUSE_SPARK_SPREAD));
                let car_state = &mut self.cars[index];
                car_state.bomb_smoke -= dt;
                if car_state.bomb_smoke <= 0.0
                {
                    car_state.bomb_smoke += BOMB_SMOKE_EVERY;
                    for point in body_points(&car)
                    {
                        out.emits.push((Kind::Smoke3, 1, point, Vector3::y() * BOMB_SMOKE_RISE, BOMB_SMOKE_SPREAD));
                    }
                }
                let receiver = cars.iter().enumerate().position(|(other, other_car)| other != index && self.cars[other].bomb.is_none() && self.cars[other].bomb_safe <= 0.0 && overlap(&car, other_car, 0.05));
                if let Some(receiver) = receiver
                {
                    if let Some((fuse, sound)) = self.cars[index].bomb.take()
                    {
                        if let Some(sound) = sound { stop_sound(&car_node(scene, car.id), &sound); }
                        self.cars[receiver].bomb = Some((fuse, None));
                    }
                    self.cars[index].bomb_safe = BOMB_IMMUNE;
                }
            }
            else
            {
                let car_state = &mut self.cars[index];
                car_state.aerial_burnt = (car_state.aerial_burnt - AERIAL_REGROW * dt).max(0.0);
            }
        }

        for index in 0..self.cars.len().min(cars.len())
        {
            let flicker = self.random.next();
            let node = car_node(scene, cars[index].id);
            let car_state = &mut self.cars[index];

            // the bomb makes the car black
            let rate = if car_state.bomb.is_some() { SOOT_RATE } else { -SOOT_RATE * 0.5 };
            car_state.soot = (car_state.soot + rate * dt).clamp(0.0, 1.0);

            // the aerial burns down with the fuse and grows back after
            if (car_state.aerial_burnt - car_state.aerial_shown).abs() > 0.005
            {
                if car_state.aerial.is_none() { car_state.aerial = find_child(&node, "aerial").map(|aerial| { let scale = scale_of(&aerial); (aerial, scale) }); }
                if let Some((aerial, scale)) = &car_state.aerial
                {
                    set_scale(aerial, Vector3::new(scale.x, scale.y * (1.0 - car_state.aerial_burnt).max(0.02), scale.z));
                }
                car_state.aerial_shown = car_state.aerial_burnt;
            }

            // the lightning: zapped (dimming with the zap) or carrying the pulse - the glow: the bomb pulsing red, the turbo fading in and out
            let white = Vector3::new(1.0, 1.0, 1.0);
            let turbo_left = car_state.turbo;
            let lightning = if car_state.zap > 0.0 { Some((ZAP_SPOT, ZAP_SPIN, white, ZAP_BRIGHT * car_state.zap * (0.85 + 0.15 * flicker))) }
                else if car_state.pulse > 0.0 { Some((ZAP_SPOT, ZAP_SPIN, white, PULSE_BRIGHT * (0.8 + 0.2 * flicker))) }
                else { None };
            let glow = if let Some((left, _)) = &car_state.bomb
                {
                    let burnt = (1.0 - left / BOMB_FUSE).clamp(0.0, 1.0);
                    car_state.bomb_phase += (BOMB_PULSE[0] + (BOMB_PULSE[1] - BOMB_PULSE[0]) * burnt) * std::f32::consts::TAU * dt;
                    Some((TURBO_SPOT, TURBO_SPIN, BOMB_SKIN, BOMB_BRIGHT[0] + (BOMB_BRIGHT[1] - BOMB_BRIGHT[0]) * (0.5 + 0.5 * car_state.bomb_phase.sin())))
                }
                else if turbo_left > 0.0 { Some((TURBO_SPOT, TURBO_SPIN, white, TURBO_BRIGHT * ((TURBO_TIME - turbo_left) / SKIN_FADE).min(turbo_left / SKIN_FADE).min(1.0))) }
                else { None };
            for (layer, look) in [lightning, glow].into_iter().enumerate()
            {
                let Some((copy, car_body)) = car_state.skins[layer].as_ref() else { continue; };
                copy.write().unwrap().settings.visible = look.is_some();
                let Some((spot, spin, color, brightness)) = look else { continue; };
                car_state.skin_spins[layer] += spin * dt;
                let turn = car_state.skin_spins[layer];
                follow_body(copy, car_body);
                crate::for_each_node(&vec![copy.clone()], &mut |part| crate::change_material(part, |m|
                {
                    m.base_color = color * brightness * SKIN_GLOW;
                    if let Some(texture) = m.texture_base.as_mut()
                    {
                        texture.transform.offset = Vector2::new(spot[0], spot[1]) / 256.0;
                        texture.transform.scale = Vector2::new(spot[2], spot[2]) / 256.0;
                        texture.transform.rotation = turn;
                    }
                }));
            }
        }
    }

    // the oil a car drops, and the cars sliding on it
    fn update_oil(&mut self, scene: &mut Scene, cars: &[Car], dt: f32, out: &mut Out)
    {
        for index in 0..self.cars.len().min(cars.len())
        {
            let car = cars[index];
            let car_state = &mut self.cars[index];
            if car_state.oil_drop <= 0.0 || car_state.oil_dropped >= OIL_DROPS { continue; }
            car_state.oil_drop -= dt;
            let back = car.position - horizontal(car.forward) * car.half_length * 0.8;
            if car_state.oil_last.is_some_and(|last| (last - back).norm() < OIL_SPACING) { continue; }
            car_state.oil_last = Some(back);
            let size = OIL_SIZES[car_state.oil_dropped];
            car_state.oil_dropped += 1;
            if let Some((point, normal)) = cast(scene, back + Vector3::y() * 0.5, -Vector3::y(), 1.5).map(|(distance, normal)| (back + Vector3::y() * (0.5 - distance), normal))
            {
                if self.blobs.len() >= OIL_BLOBS { self.blobs.remove(0); }
                let yaw = self.random.next() * std::f32::consts::TAU;
                self.blobs.push(Blob { position: point, normal, size, yaw, owner: index, age: 0.0 });
            }
        }
        let _ = out;

        for blob in &mut self.blobs { blob.age += dt; }
        self.blobs.retain(|blob| blob.age < OIL_TIME);
        for (index, car) in cars.iter().enumerate()
        {
            // each tire on its own: only the wheels that roll through it get oily
            let tires = tires(scene, car.id);
            let Some(car_state) = self.cars.get_mut(index) else { continue; };
            car_state.oily.resize(tires.len(), 0.0);
            for (wheel, tire) in tires.iter().enumerate()
            {
                let on_oil = self.blobs.iter().filter(|blob| blob.owner != index || blob.age > OIL_OWNER_SAFE).any(|blob| { let offset = tire - blob.position; Vector3::new(offset.x, 0.0, offset.z).norm() < blob_size(blob) + OIL_TOUCH && offset.y.abs() < 0.4 });
                if on_oil { car_state.oily[wheel] = OIL_RECOVER; }
            }
        }

        // the blobs on their instances, the rest hidden
        let Some((_, instances)) = self.oil.as_ref() else { return; };
        for (index, instance) in instances.iter().enumerate()
        {
            let mut instance = instance.write().unwrap();
            let Some(blob) = self.blobs.get(index) else { instance.get_data_mut().get_mut().visible = false; continue; };
            let fade = ((OIL_TIME - blob.age) / OIL_FADE).clamp(0.0, 1.0);
            {
                let data = instance.get_data_mut().get_mut();
                data.visible = true;
                data.color = Vector4::new(1.0, 1.0, 1.0, fade);
            }
            let rotation = UnitQuaternion::rotation_between(&Vector3::y(), &blob.normal).unwrap_or_else(UnitQuaternion::identity) * UnitQuaternion::from_axis_angle(&Vector3::y_axis(), blob.yaw);
            let local = Matrix4::new_translation(&(blob.position + blob.normal * OIL_LIFT)) * rotation.to_homogeneous() * Matrix4::new_nonuniform_scaling(&Vector3::new(blob_size(blob), 1.0, blob_size(blob)));
            if let Some(transform) = instance.find_component::<Transformation>()
            {
                component_downcast_mut!(transform, Transformation);
                transform.set_local_transform(local);
            }
        }
    }

    fn update_bursts(&mut self, scene: &mut Scene, dt: f32)
    {
        let mut index = 0;
        while index < self.bursts.len()
        {
            let burst = &mut self.bursts[index];
            burst.age += dt;
            let share = (burst.age / burst.time).min(1.0);
            let position = burst.node.read().unwrap().find_component::<Transformation>().map(|transform| { component_downcast!(transform, Transformation); transform.get_data().position }).unwrap_or_else(Vector3::zeros);
            place(&burst.node, position, UnitQuaternion::identity(), (burst.start + (burst.size - burst.start) * share.sqrt()).max(0.01));
            set_alpha(&burst.node, 1.0 - share);
            if burst.age < burst.time { index += 1; continue; }
            let burst = self.bursts.remove(index);
            delete(scene, &burst.node);
        }
    }

    // the effects on the vehicle controllers: no drive and slow steering while zapped, the turbo, sliding on oil
    fn apply(&self, scene: &mut Scene, cars: &[Car])
    {
        for (index, car) in cars.iter().enumerate()
        {
            let Some(car_state) = self.cars.get(index) else { continue; };
            let (throttle, steer_rate) = if car_state.zap > 0.0 { (0.0, ZAP_STEER) } else { (1.0, 1.0) };
            let turbo = car_state.turbo / TURBO_TIME;
            let power = 1.0 + TURBO_POWER * turbo;
            let grip = if turbo > 0.0 { TURBO_GRIP } else { 1.0 };
            // per wheel: the oily ones slide and leave marks, fading while they get their grip back
            let wheel_grip: Vec<f32> = car_state.oily.iter().map(|oily| if *oily > 0.0 { OIL_GRIP + (OIL_RECOVER_GRIP - OIL_GRIP) * (1.0 - oily / OIL_RECOVER) } else { 1.0 }).collect();
            let marks: Vec<f32> = car_state.oily.iter().map(|oily| (oily / OIL_MARK_FADE).min(1.0)).collect();
            with_vehicle(scene, car.id, |vehicle| { vehicle.throttle_limit = throttle; vehicle.steer_rate_scale = steer_rate; vehicle.power_scale = power; vehicle.grip_scale = grip; vehicle.wheel_grip_scale = wheel_grip; vehicle.wheel_mark_force = marks; });
        }
    }

    // ********** helpers **********

    // a model of assets/models with a looping sound on it
    fn spawn(&mut self, state: &mut State, model: &str, name: &str, sound: Option<&str>) -> Option<Thing>
    {
        let node = crate::field::show_model(state, self.scene_id, &format!("{}/models/{model}.glb", self.assets), name).ok()?;
        crate::for_each_node(&vec![node.clone()], &mut |part| crate::change_material(part, |m| { m.cast_shadow = false; }));
        let sound = sound.and_then(|sound| loop_sound(state, &self.assets, &node, sound));
        Some(Thing { node, sound })
    }

    // the bomb on a car: no halo, the car goes black and the aerial burns down (the fuse sound on the car)
    fn give_bomb(&mut self, state: &mut State, race: &Race, index: usize, fuse: f32)
    {
        if let Some((left, _)) = self.cars[index].bomb.as_mut() { *left = fuse; return; }
        let node = race.racers[index].node.clone();
        let sound = loop_sound(state, &self.assets, &node, "fuse");
        self.cars[index].bomb = Some((fuse, sound));
    }

    // the flash of the global pulse, 1 at its start
    pub fn flash(&self) -> f32
    {
        self.flash / FLASH_TIME
    }

    // the blue strip model of the pulse arcs and the shockwave beam, once
    fn load_lightning(&mut self, state: &mut State)
    {
        if self.lightning.is_some() { return; }
        let model = crate::field::show_model(state, self.scene_id, &format!("{}/{}", self.assets, lightning::BLUE_FILE), "blue_arcs").ok();
        self.lightning = Some(model.and_then(|root| Lightning::new(root, PULSE_GLOW)));
    }

    // per local player: green over the car its firework would go for, red over its own car while someone aims at it - normalized screen position (y down), red
    pub fn reticles(&self, scene: &Scene, race: &Race) -> Vec<([f32; 2], bool)>
    {
        let mut reticles = vec![];
        for (index, racer) in race.racers.iter().enumerate()
        {
            if !racer.is_local() { continue; }
            let aimed_at = self.targets.iter().enumerate().any(|(other, target)| other != index && *target == Some(index));
            let marks = self.targets.get(index).copied().flatten().map(|target| (target, false)).into_iter().chain(aimed_at.then_some((index, true)));
            let camera = match race.multi()
            {
                true => race.split_views().iter().find(|(view, _, _)| *view == index).and_then(|(_, id, _)| scene.cameras.iter().find(|camera| camera.id == *id)),
                false => scene.cameras.iter().find(|camera| camera.enabled && !camera.tags.contains_starts_with(rustl::state::state::ENGINE_INTERNAL_TAG_PREFX)),
            };
            let Some(camera) = camera else { continue; };
            for (car, red) in marks
            {
                let position = race.racers[car].node.read().unwrap().get_full_transform().column(3).xyz() + Vector3::y() * 0.2;
                let Some(point) = camera.world_to_screen(&Point3::from(position)) else { continue; };
                let data = camera.get_data();
                let (width, height) = (data.resolution_width.max(1) as f32, data.resolution_height.max(1) as f32);
                // split screen: only inside the own cell (the projection does not clip, it ran into the neighbour)
                if !camera.is_point_in_viewport(&point) { continue; }
                reticles.push(([point.x / width, 1.0 - point.y / height], red));
            }
        }
        reticles
    }

    // the putty bomb goes off on a car: up and spinning, the cars next to it pushed away
    fn explode(&mut self, scene: &mut Scene, cars: &[Car], index: usize, out: &mut Out)
    {
        let Some(car) = cars.get(index).copied() else { return; };
        if let Some((_, Some(sound))) = self.cars.get_mut(index).and_then(|car_state| car_state.bomb.take()) { stop_sound(&car_node(scene, car.id), &sound); }
        let angle = self.random.next() * std::f32::consts::TAU;
        let beside = Vector3::new(angle.cos(), 0.0, angle.sin()) * between(&mut self.random, BOMB_OFFSET);
        push_at(scene, car.id, Vector3::y() * BOMB_UP, beside);
        for (other, other_car) in cars.iter().enumerate().filter(|(other, _)| *other != index)
        {
            let offset = other_car.position - car.position;
            let distance = offset.norm();
            if distance >= BOMB_REACH { continue; }
            let away = horizontal(offset);
            push_at(scene, other_car.id, away * (BOMB_PUSH[0] / distance.max(0.5)).min(BOMB_PUSH[1]) + Vector3::y() * 1.5, Vector3::zeros());
            let _ = other;
        }
        out.sounds.push(("puttbang", car.position, 1.0));
        blast_particles(out, car.position);
    }

    // the mesh of the oil blobs: a flat disc of radius 1, one instance per blob
    fn make_oil(&mut self, state: &mut State)
    {
        if self.oil.is_some() { return; }
        let Some(scene) = state.find_scene_by_id_mut(self.scene_id) else { return; };
        self.oil = Some(oil_node(scene));
    }

    fn roll_ball(&mut self, state: &mut State, race: &Race, car: &Car)
    {
        let Some(thing) = self.spawn(state, "ball_bearing", "ball_bearing", Some("ball")) else { return; };
        // chrome like the game: the env map of the cars, strong
        crate::for_each_node(&vec![thing.node.clone()], &mut |part| crate::change_material(part, |m| { m.cast_shadow = true; m.receive_shadow = false; }));
        let _ = race;
        if let Ok(texture) = state.load_texture_or_reuse(&format!("{}/gfx/envroll.png", self.assets), Some("png".to_string()), state.max_texture_resolution())
        {
            crate::for_each_node(&vec![thing.node.clone()], &mut |part|
            {
                let part = part.read().unwrap();
                let Some(material) = part.find_component::<rustl::state::scene::components::material::Material>() else { return; };
                component_downcast_mut!(material, rustl::state::scene::components::material::Material);
                material.set_texture(texture.clone(), rustl::state::scene::components::material::TextureType::Environment);
                material.get_data_mut().get_mut().reflectivity = BALL_SHINE;
            });
        }
        let back = horizontal(car.forward);
        let position = car.position - back * (car.half_length + BALL_RADIUS + 0.15) + Vector3::y() * BALL_RADIUS * 0.6;
        place(&thing.node, position, UnitQuaternion::identity(), BALL_RADIUS);
        tint(&thing.node, BALL_COLOR, Vector3::zeros(), None);
        crate::for_each_node(&vec![thing.node.clone()], &mut |part| crate::change_material(part, |m| { m.unlit_shading = false; m.specular_color = Vector3::new(1.0, 0.9, 0.6); m.shininess = 60.0; }));
        let volume = 4.0 / 3.0 * std::f32::consts::PI * BALL_RADIUS.powi(3);
        let mut body = None;
        crate::for_each_node(&vec![thing.node.clone()], &mut |part| if body.is_none() && part.read().unwrap().find_component::<rustl::state::scene::components::mesh::Mesh>().is_some() { body = Some(part.clone()); });
        let Some(body) = body else { return; };
        // the collision of the root too - a node only collides when all above it do
        thing.node.write().unwrap().settings.collision = true;
        {
            let mut node = body.write().unwrap();
            node.settings.collision = true;
            node.settings.physics = PhysicsSettings
            {
                body_type: PhysicsBodyType::Dynamic,
                shape: PhysicsShape::Sphere,
                density: BALL_MASS / volume,
                friction: 0.4,
                restitution: 0.25,
                linear_damping: 0.02,
                angular_damping: 0.2,
                linear_velocity: car.velocity - back * BALL_BACK_SPEED,
                ..PhysicsSettings::default()
            };
        }
        let Some(scene) = state.find_scene_by_id_mut(self.scene_id) else { return; };
        scene.physics.add_node(body.clone());
        self.balls.push(Ball { thing, body, previous: None, age: 0.0 });
        self.sound(state, "balldrop", position, 1.0);
    }

    fn ground(&self, state: &State, position: Vector3<f32>) -> Option<(Vector3<f32>, Vector3<f32>)>
    {
        let scene = state.find_scene_by_id(self.scene_id)?;
        cast(scene, position + Vector3::y() * 0.5, -Vector3::y(), 2.0).map(|(distance, normal)| (position + Vector3::y() * (0.5 - distance), normal))
    }

    fn sound(&self, state: &mut State, name: &str, position: Vector3<f32>, volume: f32)
    {
        let Some(source) = crate::sounds::source(state, &self.assets, name) else { return; };
        state.play_one_shot_sound_source_at(source, Point3::from(position), volume, 1.0, SOUND_DISTANCE);
    }
}

// ********** free helpers **********

fn car(scene: &Scene, racer: &Racer) -> Car
{
    let (min, max, _) = car_box(scene, racer);
    let (id, transform) = { let node = racer.node.read().unwrap(); (node.id, node.get_full_transform()) };
    let position = transform.column(3).xyz();
    let forward = (-transform.column(2).xyz()).try_normalize(1e-6).unwrap_or_else(Vector3::z);
    let up = transform.column(1).xyz().try_normalize(1e-6).unwrap_or_else(Vector3::y);
    let velocity = scene.physics.vehicle(id).map(|(_, body)| { let v = body.linvel(); Vector3::new(v.x, v.y, v.z) }).unwrap_or_else(|| forward * racer.speed);
    let size = max - min;
    Car { id, position, forward, up, velocity, min, max, half_length: (size.x.max(size.z) * 0.5).clamp(0.2, 1.5), active: !racer.finished() }
}

// the car ahead the firework goes for: in a cone ahead, up to ROCKET_RANGE, near and straight ahead first
fn target(cars: &[Car], index: usize) -> Option<usize>
{
    let car = cars.get(index)?;
    cars.iter().enumerate().filter(|(other, other_car)| *other != index && other_car.active).filter_map(|(other, other_car)|
    {
        let offset = other_car.position - car.position;
        let distance = offset.norm();
        let ahead = offset.dot(&car.forward) / distance.max(1e-4);
        (distance > 1.0 && distance < ROCKET_RANGE && ahead > ROCKET_CONE).then_some((other, distance * (2.0 - ahead)))
    }).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(other, _)| other)
}

// cars within reach pushed away from a point, more the closer they are, and spun a bit
fn blast(scene: &mut Scene, cars: &[Car], center: Vector3<f32>, reach: f32, push_at_1m: [f32; 2], up_at_1m: [f32; 2], near: f32, beside: f32, random: &mut Random)
{
    for car in cars
    {
        let offset = car.position - center;
        let distance = offset.norm();
        if distance >= reach { continue; }
        let counted = distance.max(near);
        let away = horizontal(offset);
        let velocity = away * (push_at_1m[0] / counted).min(push_at_1m[1]) + Vector3::y() * (up_at_1m[0] / counted).min(up_at_1m[1]);
        let point = Vector3::new(random.next() - 0.5, random.next() - 0.5, random.next() - 0.5) * 2.0 * beside;
        push_at(scene, car.id, velocity, point);
    }
}

// a change of the velocity of a car by a push beside its center of mass (m, world) - it also turns
fn push_at(scene: &mut Scene, id: u32, velocity: Vector3<f32>, beside: Vector3<f32>)
{
    let Some((_, body)) = scene.physics.vehicle_mut(id) else { return; };
    let impulse = Vector::new(velocity.x, velocity.y, velocity.z) * body.mass();
    let point = body.center_of_mass() + Vector::new(beside.x, beside.y, beside.z);
    body.apply_impulse_at_point(impulse, point, true);
}

// a change of the velocity and the spin of a car
fn push(scene: &mut Scene, id: u32, velocity: Vector3<f32>, spin: Vector3<f32>)
{
    let Some((_, body)) = scene.physics.vehicle_mut(id) else { return; };
    let linear = body.linvel() + Vector::new(velocity.x, velocity.y, velocity.z);
    body.set_linvel(linear, true);
    let angular = body.angvel() + Vector::new(spin.x, spin.y, spin.z);
    body.set_angvel(angular, true);
}

fn with_vehicle(scene: &mut Scene, id: u32, f: impl FnOnce(&mut VehicleController))
{
    for controller in scene.controller.iter_mut()
    {
        let Some(vehicle) = controller.as_any_mut().downcast_mut::<VehicleController>() else { continue; };
        if vehicle.node.as_ref().is_some_and(|node| node.read().unwrap().id == id)
        {
            f(vehicle);
            return;
        }
    }
}

fn car_node(scene: &Scene, id: u32) -> NodeItem
{
    scene.find_node_by_id(id).unwrap_or_else(|| Node::new("gone"))
}

// where the tires of a car touch the ground (below the wheel centers), in the order of its wheels
fn tires(scene: &Scene, id: u32) -> Vec<Vector3<f32>>
{
    let Some(vehicle) = scene.controller.iter().filter_map(|controller| controller.as_any().downcast_ref::<VehicleController>()).find(|vehicle| vehicle.node.as_ref().is_some_and(|node| node.read().unwrap().id == id)) else { return vec![]; };
    let Some(node) = vehicle.node.as_ref() else { return vec![]; };
    let transform = node.read().unwrap().get_full_transform();
    let down = -transform.column(1).xyz().try_normalize(1e-6).unwrap_or_else(Vector3::y);
    vehicle.wheels.iter().map(|wheel| (transform * wheel.center.push(1.0)).xyz() + down * wheel.radius).collect()
}


// a second copy of the car body hung next to the body of the car (so it moves with it in the same frame), additive - (the copy, the body of the car)
// every vertex gets a uv on the unit circle at a random angle, the texture transform puts the circle on the spot of fxpage1 and turns it
fn skin(state: &mut State, scene_id: u32, car: &NodeItem, texture: Option<rustl::state::resources::texture::TextureItem>, random: &mut Random) -> Option<(NodeItem, NodeItem)>
{
    let path = car.read().unwrap().source.as_ref()?.origin_path.clone();
    let car_body = find_child(car, "model")?;
    let parent = car_body.read().unwrap().parent.as_ref().cloned()?;
    let root = crate::field::show_model(state, scene_id, &path, "skin").ok()?;
    let body = find_child(&root, "model");
    if let Some(body) = &body
    {
        // out of the loaded model, next to the body of the car
        if let Some(old) = body.read().unwrap().parent.as_ref().cloned() { old.write().unwrap().nodes.retain(|node| !Arc::ptr_eq(node, body)); }
        Node::add_node(parent, body.clone());
    }
    if let Some(scene) = state.find_scene_by_id_mut(scene_id) { delete_kept(scene, &root); }
    let body = body?;
    // only the body like the game - the wheels, springs, axles, pins, aerial and hull of the copy would stand still at their rest pose
    for part in body.read().unwrap().nodes.iter()
    {
        let mut write = part.write().unwrap();
        // a body of several materials is one node per primitive ("body primitive_N")
        if write.name != "body" && !write.name.starts_with("body primitive_") { write.settings.visible = false; }
    }
    // no car for the game code (tags), never saved, no shadows or shine
    crate::for_each_node(&vec![body.clone()], &mut |part|
    {
        {
            let mut write = part.write().unwrap();
            write.tags = rustl::state::scene::utilities::tags::Tags::new();
            write.settings.pickable = false;
            write.settings.transient = true;
            write.settings.collision = false;
            write.settings.camera_collision = false;
        }
        let read = part.read().unwrap();
        if let Some(mesh) = read.find_component::<rustl::state::scene::components::mesh::Mesh>()
        {
            component_downcast_mut!(mesh, rustl::state::scene::components::mesh::Mesh);
            if let Some(mut resource) = mesh.mesh_resource.as_ref().map(|resource| resource.read().unwrap().duplicate())
            {
                {
                    let data = resource.get_data_mut().get_mut();
                    // out along the averaged normal of each vertex - a scale about the car origin left the body copy at the depth of the body
                    let mut normals = vec![Vector3::<f32>::zeros(); data.vertices.len()];
                    for (triangle, normal_triangle) in data.indices.iter().zip(data.normals_indices.iter())
                    {
                        for corner in 0..3
                        {
                            if let (Some(sum), Some(normal)) = (normals.get_mut(triangle[corner] as usize), data.normals.get(normal_triangle[corner] as usize)) { *sum += normal; }
                        }
                    }
                    // corners at the same place move together (the polygons keep their edges)
                    let key = |vertex: &Point3<f32>| (vertex * 10000.0).map(|v| v.round() as i64);
                    let mut shared: std::collections::HashMap<Point3<i64>, Vector3<f32>> = std::collections::HashMap::new();
                    for (vertex, normal) in data.vertices.iter().zip(&normals) { *shared.entry(key(vertex)).or_insert_with(Vector3::zeros) += normal; }
                    for vertex in data.vertices.iter_mut()
                    {
                        if let Some(normal) = shared.get(&key(vertex)).and_then(|normal| normal.try_normalize(1e-6)) { *vertex += normal * SKIN_OFFSET; }
                    }
                    // the vertex buffer flips v (glTF convention) - stored as 1 - v, the gpu gets the unit circle around 0
                    data.uvs_0 = (0..data.vertices.len()).map(|_| { let angle = random.next() * std::f32::consts::TAU; Point2::new(angle.sin(), 1.0 - angle.cos()) }).collect();
                    data.uv_indices = data.indices.clone();
                }
                mesh.mesh_resource = rustl::helper::option_or_id::OptionOrId::Some(Arc::new(RwLock::new(Box::new(resource))));
            }
        }
        if let Some(material) = read.find_component::<Material>()
        {
            component_downcast_mut!(material, Material);
            material.remove_all_textures();
            if let Some(texture) = &texture { material.set_texture(texture.clone(), rustl::state::scene::components::material::TextureType::Base); }
            let m = material.get_data_mut().get_mut();
            m.unlit_shading = true;
            m.cast_shadow = false;
            m.receive_shadow = false;
            m.reflectivity = 0.0;
            m.vertex_colors = false;
            m.ambient_color = Vector3::zeros();
            m.alpha = 1.0;
            m.blend_mode = BlendMode::Additive;
            // every triangle spans the whole spot - a mip level would average the thin lines to black, the game samples the full page
            if let Some(texture) = m.texture_base.as_mut() { texture.sampler.no_mipmaps = true; }
        }
    });
    {
        let mut write = body.write().unwrap();
        write.name = "skin".to_string();
        write.settings.visible = false;
    }
    Some((body, car_body))
}

// the skin like the body of the car (suspension, roll)
fn follow_body(copy: &NodeItem, car_body: &NodeItem)
{
    let local = car_body.read().unwrap().find_component::<Transformation>().map(|component| { component_downcast!(component, Transformation); *component.get_data() });
    if let (Some(local), Some(component)) = (local, copy.read().unwrap().find_component::<Transformation>())
    {
        component_downcast_mut!(component, Transformation);
        component.get_data_mut().set(local);
    }
}

fn scale_of(node: &NodeItem) -> Vector3<f32>
{
    node.read().unwrap().find_component::<Transformation>().map(|component| { component_downcast!(component, Transformation); component.get_data().scale }).unwrap_or_else(|| Vector3::new(1.0, 1.0, 1.0))
}

fn set_scale(node: &NodeItem, scale: Vector3<f32>)
{
    if let Some(component) = node.read().unwrap().find_component::<Transformation>()
    {
        component_downcast_mut!(component, Transformation);
        // with the matrix - the field alone left the aerial at its length
        component.set_scale(scale);
    }
}

// the camera nearest to a point (split screen: one of them)
fn eye(scene: &Scene, point: Vector3<f32>) -> Vector3<f32>
{
    scene.cameras.iter().filter(|camera| camera.enabled && !camera.tags.contains_starts_with(rustl::state::state::ENGINE_INTERNAL_TAG_PREFX)).map(|camera| camera.get_data().eye_pos.coords)
        .min_by(|x, y| (x - point).norm_squared().total_cmp(&(y - point).norm_squared())).unwrap_or(point + Vector3::new(0.0, 5.0, 5.0))
}

fn find_child(node: &NodeItem, name: &str) -> Option<NodeItem>
{
    for child in node.read().unwrap().nodes.clone()
    {
        if child.read().unwrap().name == name { return Some(child); }
        if let Some(found) = find_child(&child, name) { return Some(found); }
    }
    None
}

// distance and normal of the level along a ray
fn cast(scene: &Scene, from: Vector3<f32>, direction: Vector3<f32>, length: f32) -> Option<(f32, Vector3<f32>)>
{
    let ray = Ray::new(Vector::new(from.x, from.y, from.z), Vector::new(direction.x, direction.y, direction.z));
    scene.physics.query_pipeline(QueryFilter::only_fixed().exclude_sensors()).cast_ray_and_get_normal(&ray, length, true)
        .map(|(_, hit)| (hit.time_of_impact, Vector3::new(hit.normal.x, hit.normal.y, hit.normal.z)))
}

fn sight(scene: &Scene, from: Vector3<f32>, to: Vector3<f32>) -> bool
{
    let (from, to) = (from + Vector3::y() * 0.2, to + Vector3::y() * 0.2);
    let distance = (to - from).norm();
    distance < 0.01 || cast(scene, from, (to - from) / distance, distance).is_none()
}

fn inside(car: &Car, point: Vector3<f32>, margin: f32) -> bool
{
    (0..3).all(|a| point[a] >= car.min[a] - margin && point[a] <= car.max[a] + margin)
}

fn overlap(a: &Car, b: &Car, margin: f32) -> bool
{
    (0..3).all(|axis| a.min[axis] - margin <= b.max[axis] && a.max[axis] + margin >= b.min[axis])
}

fn horizontal(direction: Vector3<f32>) -> Vector3<f32>
{
    Vector3::new(direction.x, 0.0, direction.z).try_normalize(1e-4).unwrap_or_else(Vector3::x)
}

// the model's long axis (y) along a direction
fn along(direction: Vector3<f32>) -> UnitQuaternion<f32>
{
    UnitQuaternion::rotation_between(&Vector3::y(), &direction).unwrap_or_else(UnitQuaternion::identity)
}

fn turn_towards(from: Vector3<f32>, to: Vector3<f32>, max_angle: f32) -> Vector3<f32>
{
    let angle = from.angle(&to);
    if angle <= max_angle || angle < 1e-4 { return to; }
    let axis = from.cross(&to).try_normalize(1e-6).unwrap_or_else(Vector3::y);
    UnitQuaternion::from_axis_angle(&rustl::nalgebra::Unit::new_unchecked(axis), max_angle) * from
}

fn blob_size(blob: &Blob) -> f32
{
    blob.size - (blob.size - OIL_START_SIZE) * (-OIL_SPREAD * blob.age).exp()
}

// the flash, smoke and sparks of a firework or bomb blast
fn blast_particles(out: &mut Out, position: Vector3<f32>)
{
    for (kind, count, spread) in BLAST_PARTICLES
    {
        out.emits.push((kind, count, position, Vector3::zeros(), spread));
    }
}

// whole particles of a rate this frame, the rest by chance
fn due(random: &mut Random, rate: f32, dt: f32) -> usize
{
    let amount = rate * dt;
    amount as usize + usize::from(random.next() < amount.fract())
}

// 8 points on top of the box of a car: its corners and the middles of its edges
fn body_points(car: &Car) -> Vec<Vector3<f32>>
{
    let middle = (car.min + car.max) * 0.5;
    let mut points = vec![];
    for x in [car.min.x, middle.x, car.max.x]
    {
        for z in [car.min.z, middle.z, car.max.z]
        {
            if x == middle.x && z == middle.z { continue; }
            points.push(Vector3::new(x, car.max.y, z));
        }
    }
    points
}

fn random_direction(random: &mut Random) -> Vector3<f32>
{
    let (y, angle) = (random.next() * 2.0 - 1.0, random.next() * std::f32::consts::TAU);
    let across = (1.0 - y * y).max(0.0).sqrt();
    Vector3::new(across * angle.cos(), y, across * angle.sin())
}

fn between(random: &mut Random, [low, high]: [f32; 2]) -> f32
{
    low + random.next() * (high - low)
}

fn place(node: &NodeItem, position: Vector3<f32>, rotation: UnitQuaternion<f32>, scale: f32)
{
    place_scaled(node, position, rotation, Vector3::repeat(scale));
}

fn place_scaled(node: &NodeItem, position: Vector3<f32>, rotation: UnitQuaternion<f32>, scale: Vector3<f32>)
{
    let node = node.read().unwrap();
    if let Some(transform) = node.find_component::<Transformation>()
    {
        component_downcast_mut!(transform, Transformation);
        let data = transform.get_data_mut().get_mut();
        data.position = position;
        data.rotation_quat = Some(rotation.into_inner().coords);
        data.scale = scale;
        transform.calc_transform();
    }
}

// base color, glow and alpha of every part
fn tint(node: &NodeItem, base: Vector3<f32>, glow: Vector3<f32>, alpha: Option<f32>)
{
    crate::for_each_node(&vec![node.clone()], &mut |part| crate::change_material(part, |m|
    {
        m.base_color = base;
        m.ambient_color = glow;
        if let Some(alpha) = alpha { m.alpha = alpha; m.blend_mode = if alpha < 1.0 { BlendMode::Blend } else { m.blend_mode }; }
    }));
}

fn set_alpha(node: &NodeItem, alpha: f32)
{
    crate::for_each_node(&vec![node.clone()], &mut |part| crate::change_material(part, |m| { m.alpha = alpha; if m.blend_mode != BlendMode::Additive { m.blend_mode = BlendMode::Blend; } }));
}

fn loop_sound(state: &mut State, assets: &str, node: &NodeItem, name: &str) -> Option<ComponentItem>
{
    let source = crate::sounds::source(state, assets, name)?;
    let mut sound = Sound::new(name, source, SoundType::Spatial, true);
    sound.get_data_mut().get_mut().spatial_distance_scale = SOUND_DISTANCE;
    let component: ComponentItem = Arc::new(RwLock::new(Box::new(sound)));
    node.write().unwrap().add_component(component.clone());
    {
        component_downcast_mut!(component, Sound);
        component.start();
    }
    Some(component)
}

fn stop_sound(node: &NodeItem, sound: &ComponentItem)
{
    let id = sound.read().unwrap().id();
    {
        component_downcast_mut!(sound, Sound);
        sound.stop();
    }
    node.write().unwrap().remove_component_by_id(id);
}

fn remove(scene: &mut Scene, thing: Thing)
{
    if let Some(sound) = thing.sound.as_ref() { stop_sound(&thing.node, sound); }
    delete(scene, &thing.node);
}

fn delete(scene: &mut Scene, node: &NodeItem)
{
    let id = node.read().unwrap().id;
    scene.delete_node_by_id(id, true, false, true, false);
}

// a node of a car model: its meshes and materials may be shared with the car, they stay
fn delete_kept(scene: &mut Scene, node: &NodeItem)
{
    let id = node.read().unwrap().id;
    scene.delete_node_by_id(id, false, false, false, false);
}

// the oil: a dark glossy disc, one instance per blob (hidden until used)
fn oil_node(scene: &mut Scene) -> (NodeItem, Vec<rustl::state::scene::node::InstanceItemArc>)
{
    use rustl::helper::option_or_id::OptionOrId;
    use rustl::state::resources::mesh_resource::MeshResource;
    use rustl::state::scene::components::material::{Material, MaterialItem};
    use rustl::state::scene::components::mesh::Mesh;
    use rustl::state::scene::instance::Instance;

    const SIDES: u32 = 20;
    let mut vertices = vec![Point3::new(0.0, 0.0, 0.0)];
    let mut uvs = vec![Point2::new(0.5, 0.5)];
    for side in 0..SIDES
    {
        let angle = side as f32 / SIDES as f32 * std::f32::consts::TAU;
        // a little ragged at the edge
        let radius = 1.0 - 0.12 * ((side * 7 % 5) as f32 / 4.0);
        vertices.push(Point3::new(angle.cos() * radius, 0.0, angle.sin() * radius));
        uvs.push(Point2::new(0.5 + 0.5 * angle.cos(), 0.5 + 0.5 * angle.sin()));
    }
    let indices: Vec<[u32; 3]> = (0..SIDES).map(|side| [0, 1 + (side + 1) % SIDES, 1 + side]).collect();
    let normals = vec![Vector3::y(); vertices.len()];
    let resource = MeshResource::new_with_data("oil", vertices, indices.clone(), uvs, indices.clone(), normals, indices);
    let mut mesh = Mesh::new("oil");
    mesh.mesh_resource = OptionOrId::Some(Arc::new(RwLock::new(Box::new(resource))));

    let mut material = Material::new("oil");
    {
        let data = material.get_data_mut().get_mut();
        data.base_color = OIL_COLOR;
        data.specular_color = Vector3::new(0.7, 0.7, 0.75);
        data.shininess = 90.0;
        data.alpha = 0.92;
        data.blend_mode = BlendMode::Blend;
        data.cast_shadow = false;
    }
    let material: MaterialItem = Arc::new(RwLock::new(Box::new(material)));
    scene.add_material(&material);

    let node = Node::new("oil slicks");
    {
        let mut write = node.write().unwrap();
        write.add_component(Arc::new(RwLock::new(Box::new(mesh))));
        write.add_component(material);
        let settings = &mut write.settings;
        settings.transient = true;
        settings.pickable = false;
        settings.collision = false;
        settings.camera_collision = false;
        settings.depth_write = false;
        settings.occlusion_culling = false;
        settings.frustum_culling = false;
    }
    let instances = (0..OIL_BLOBS).map(|index|
    {
        let transform = Transformation::new("Transform", Vector3::zeros(), Vector3::zeros(), Vector3::new(1.0, 1.0, 1.0));
        let mut instance = Instance::new_with_transform(format!("oil_{index}"), node.clone(), transform);
        instance.get_data_mut().get_mut().visible = false;
        node.write().unwrap().add_instance(Box::new(instance))
    }).collect();
    scene.add_node(node.clone());
    (node, instances)
}

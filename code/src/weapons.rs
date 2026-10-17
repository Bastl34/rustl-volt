// pickups and the weapons of every car like revolt.exe (formats/README.md, Pickups): a few random pickups are out, the collected one goes and another one comes
// collecting picks a weapon by the race position, the icon spins 4 s and stops on it; Ctrl fires for the player, bots fire after a while - without an effect yet
// the global star (star type 0) gives the Global Pulse at once, it is gone for the rest of the race

use std::sync::{Arc, RwLock};

use rustl::prelude::*;
use rustl::state::resources::sound_source::SoundSourceItem;
use rustl::state::scene::components::component::ComponentItem;
use rustl::state::scene::components::sound::{Sound, SoundType};
use rustl::state::scene::components::transformation::{Transformation, TransformationData};
use rustl::state::scene::scene_controller::vehicle_controller::VehicleController;

use crate::object_type::ObjectType;
use crate::race::{Driver, Race, Racer};
use crate::random::Random;

pub const WEAPON_COUNT: usize = 11;

// name, ammo per pickup, icon texels on fxpage2 (32 x 32) - the order of the game
pub const WEAPONS: [(&str, u32, [f32; 2]); WEAPON_COUNT] =
[
    ("Shockwave", 1, [0.0, 224.0]),
    ("Firework", 1, [64.0, 225.0]),
    ("Firework Pack", 3, [96.0, 224.0]),
    ("Putty Bomb", 1, [32.0, 224.0]),
    ("Water Balloons", 3, [128.0, 224.0]),
    ("Electro Pulse", 1, [224.0, 224.0]),
    ("Oil Slick", 1, [192.0, 224.0]),
    ("Ball Bearing", 1, [0.0, 192.0]),
    ("Turbo Battery", 1, [32.0, 192.0]),
    ("Clone Pickup", 1, [160.0, 224.0]),
    ("Global Pulse", 1, [96.0, 192.0]),
];

// weights of the weapons in percent for the first place, the middle and the last place, interpolated by the position
const WEIGHTS: [[f32; WEAPON_COUNT]; 3] =
[
    [5.0, 18.0, 5.0, 10.0, 5.0, 18.0, 18.0, 4.0, 8.0, 9.0, 0.0],
    [10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 0.0],
    [20.0, 3.0, 15.0, 2.0, 15.0, 2.0, 2.0, 21.0, 13.0, 2.0, 5.0],
];
// the computer cars pick from one fixed row, whatever their place
const BOT_WEIGHTS: [f32; WEAPON_COUNT] = [10.0, 10.0, 5.0, 10.0, 10.0, 5.0, 15.0, 15.0, 10.0, 10.0, 0.0];

// a pickup waits this long before it comes out (s)
const APPEAR_DELAY: f32 = 5.0;
// collected: it rises 64 units/s for 1 s and grows to 3 times, then it is gone
const COLLECT_TIME: f32 = 1.0;
const COLLECT_RISE: f32 = 0.64;
// the icon spins this long and slows down: shown = chosen + 22 - t² * 0.34375 (mod 11)
const SPIN_TIME: f32 = 4.0;
const SPIN_OFFSET: f32 = 22.0;
const SPIN_SLOW: f32 = 0.34375;
// pickup box around its origin, m (models/pickup.m: x ±24, y -29..42 units, it spins)
const PICKUP_HALF_WIDTH: f32 = 0.24;
const PICKUP_BELOW: f32 = 0.42;
const PICKUP_ABOVE: f32 = 0.29;
// a bot fires its weapon after this many seconds (random in between), the next shot of a pack sooner
const BOT_FIRE_DELAY: [f32; 2] = [2.0, 8.0];
const BOT_NEXT_SHOT: [f32; 2] = [0.5, 2.0];
// the sounds at a pickup: full volume up to this many m (range 1.0 of the game: 6 m)
const PICKUP_SOUND_DISTANCE: f32 = 6.0;
const COLLECT_SOUND: &str = "pickup";
// the global star: its box is ±30 units around it, it gives this weapon
const APPEAR_SOUND: &str = "pickgen";
const STAR_HALF_SIZE: f32 = 0.3;
const GLOBAL_PULSE: usize = 10;

#[derive(Clone, Copy, PartialEq)]
enum PickupState
{
    // comes out after the timer
    Waiting(f32),
    // out, seconds since it came (it grows in the first 1.6 s)
    Out(f32),
    Collected(f32),
    Gone,
}

struct Pickup
{
    node: NodeItem,
    restore: Option<TransformationData>,
    visible: bool,
    position: Vector3<f32>,
    scale: Vector3<f32>,
    state: PickupState,
    // half of the car speed when collected, m/s
    velocity: Vector3<f32>,
    // spatial one shots on the pickup: collected (by a bot), came out
    collect_sound: Option<ComponentItem>,
    appear_sound: Option<ComponentItem>,
}

// the weapon of one car
#[derive(Default)]
pub struct Hand
{
    pub weapon: Option<usize>,
    pub ammo: u32,
    // the pick while the icon spins
    chosen: usize,
    spin: f32,
    // a bot: seconds until it fires
    fire_in: f32,
}

pub struct Weapons
{
    pickups: Vec<Pickup>,
    // the global stars: out (spinning) until a car takes one, then collected and gone
    stars: Vec<Pickup>,
    random: Random,
    // one per racer, the order of race.racers
    hands: Vec<Hand>,
}

// sounds of this frame for the local player: (name, pitch)
pub type Sounds = Vec<(&'static str, f32)>;

impl Hand
{
    // the icon shown while spinning: index and the fraction towards the next
    pub fn spinning_icon(&self) -> Option<(usize, f32)>
    {
        if self.spin <= 0.0 { return None; }
        let shown = (self.chosen as f32 + SPIN_OFFSET - self.spin * self.spin * SPIN_SLOW).rem_euclid(WEAPON_COUNT as f32);
        Some((shown as usize % WEAPON_COUNT, shown.fract()))
    }

    // empty hands and no spin - only then a pickup can be collected
    fn free(&self) -> bool
    {
        self.weapon.is_none() && self.spin <= 0.0
    }

    // one shot of the weapon, the hand is empty after the last one
    fn fire(&mut self, who: &str)
    {
        let Some(weapon) = self.weapon else { return; };
        self.ammo = self.ammo.saturating_sub(1);
        console_log!("weapons: {} fires {} ({} left)", who, WEAPONS[weapon].0, self.ammo);
        if self.ammo == 0 { self.weapon = None; }
    }
}

impl Weapons
{
    // every pickup of the scene goes in, 2 per car + 5 of them come out after APPEAR_DELAY
    pub fn start(scene: &Scene, racers: usize, sounds: &[SoundSourceItem]) -> Self
    {
        let source = |name: &str| sounds.iter().find(|source| source.read().unwrap().name == name).cloned();
        let (collect, appear) = (source(COLLECT_SOUND), source(APPEAR_SOUND));

        let mut pickups = vec![];
        collect_pickups(&scene.nodes, &mut pickups);
        for pickup in &mut pickups
        {
            pickup.collect_sound = collect.clone().map(|source| add_sound(&pickup.node, "Pickup Collect", source));
            pickup.appear_sound = appear.clone().map(|source| add_sound(&pickup.node, "Pickup Appear", source));
        }

        let mut stars = vec![];
        collect_stars(&scene.nodes, &mut stars);
        for star in &mut stars
        {
            star.state = PickupState::Out(0.0);
            star.collect_sound = collect.clone().map(|source| add_sound(&star.node, "Pickup Collect", source));
        }

        let mut weapons = Self { pickups, stars, random: Random::default(), hands: (0..racers).map(|_| Hand::default()).collect() };
        for pickup in &weapons.pickups
        {
            pickup.show(false, 1.0, pickup.position);
        }
        let out = (racers * 2 + 5).min(weapons.pickups.len());
        for _ in 0..out
        {
            weapons.activate_random();
        }
        console_log!("weapons: {} pickups, {} out, {} global stars", weapons.pickups.len(), out, weapons.stars.len());
        weapons
    }

    pub fn hand(&self, racer: usize) -> Option<&Hand>
    {
        self.hands.get(racer)
    }

    // fire: per local player
    pub fn update(&mut self, scene: &Scene, race: &Race, fire: &[bool], dt: f32) -> Sounds
    {
        let mut sounds = vec![];
        let boxes: Vec<(Vector3<f32>, Vector3<f32>, Vector3<f32>)> = race.racers.iter().map(|racer| car_box(scene, racer)).collect();
        let running = race.countdown <= 0.0 && !race.over;

        for i in 0..self.pickups.len()
        {
            match self.pickups[i].state
            {
                PickupState::Waiting(left) =>
                {
                    let pickup = &mut self.pickups[i];
                    let left = left - dt;
                    if left > 0.0 { pickup.state = PickupState::Waiting(left); continue; }
                    pickup.state = PickupState::Out(0.0);
                    pickup.show(true, 0.0, pickup.position);
                    play(&pickup.appear_sound);
                },
                PickupState::Out(time) =>
                {
                    let time = time + dt;
                    let pickup = &mut self.pickups[i];
                    pickup.state = PickupState::Out(time);
                    pickup.show(true, grow(time), pickup.position);
                    if time <= 0.5 { continue; }

                    // the box of the pickup against the box of a car with empty hands
                    let min = pickup.position - Vector3::new(PICKUP_HALF_WIDTH, PICKUP_BELOW, PICKUP_HALF_WIDTH);
                    let max = pickup.position + Vector3::new(PICKUP_HALF_WIDTH, PICKUP_ABOVE, PICKUP_HALF_WIDTH);
                    let collector = boxes.iter().enumerate().find(|(index, (car_min, car_max, _))|
                        self.hands.get(*index).is_some_and(Hand::free) && (0..3).all(|a| min[a] <= car_max[a] && max[a] >= car_min[a])).map(|(index, _)| index);
                    let Some(index) = collector else { continue; };

                    let racer = &race.racers[index];
                    let velocity = boxes[index].2;
                    pickup.state = PickupState::Collected(0.0);
                    pickup.velocity = Vector3::new(velocity.x * 0.5, COLLECT_RISE, velocity.z * 0.5);
                    match racer.is_local()
                    {
                        true => sounds.push((COLLECT_SOUND, 1.0)),
                        false => play(&pickup.collect_sound),
                    }

                    let chosen = self.pick(racer, race.racers.len());
                    let hand = &mut self.hands[index];
                    hand.chosen = chosen;
                    hand.spin = SPIN_TIME;
                    if racer.is_local() { console_log!("weapons: pickup collected - {} coming", WEAPONS[chosen].0); }
                },
                PickupState::Collected(time) =>
                {
                    let pickup = &mut self.pickups[i];
                    let time = time + dt;
                    if time < COLLECT_TIME
                    {
                        pickup.state = PickupState::Collected(time);
                        pickup.show(true, 1.0 + 2.0 * time, pickup.position + pickup.velocity * time);
                        continue;
                    }
                    pickup.state = PickupState::Gone;
                    pickup.show(false, 1.0, pickup.position);
                    self.activate_random();
                },
                PickupState::Gone => {},
            }
        }

        self.update_stars(race, &boxes, dt, &mut sounds);

        // the spin ends on the chosen weapon
        for (index, hand) in self.hands.iter_mut().enumerate()
        {
            if hand.spin <= 0.0 { continue; }
            hand.spin = (hand.spin - dt).max(0.0);
            if hand.spin == 0.0
            {
                hand.weapon = Some(hand.chosen);
                hand.ammo = WEAPONS[hand.chosen].1;
                hand.fire_in = between(&mut self.random, BOT_FIRE_DELAY);
                if race.racers.get(index).is_some_and(Racer::is_local) { console_log!("weapons: {} x {}", WEAPONS[hand.chosen].0, hand.ammo); }
            }
        }

        if !running
        {
            return sounds;
        }

        // Ctrl (pad X / RB) fires for the player, the bots after a while - no effect yet
        for (index, racer) in race.racers.iter().enumerate()
        {
            let Some(hand) = self.hands.get_mut(index) else { continue; };
            if hand.weapon.is_none() { continue; }
            match &racer.driver
            {
                Driver::Local(player) => if fire.get(*player as usize).copied().unwrap_or(false) { hand.fire(&racer.name); },
                Driver::Bot(_) =>
                {
                    hand.fire_in -= dt;
                    if hand.fire_in <= 0.0
                    {
                        hand.fire(&racer.name);
                        hand.fire_in = between(&mut self.random, BOT_NEXT_SHOT);
                    }
                },
            }
        }

        sounds
    }

    // Play stops: the pickups as they were, without the sounds of this code
    pub fn stop(&self)
    {
        for pickup in self.pickups.iter().chain(&self.stars)
        {
            {
                let node = pickup.node.read().unwrap();
                if let (Some(data), Some(transform)) = (pickup.restore, node.find_component::<Transformation>())
                {
                    component_downcast_mut!(transform, Transformation);
                    transform.get_data_mut().set(data);
                }
            }
            let mut node = pickup.node.write().unwrap();
            node.settings.visible = pickup.visible;
            for sound in pickup.collect_sound.iter().chain(pickup.appear_sound.iter())
            {
                let id = sound.read().unwrap().id();
                node.remove_component_by_id(id);
            }
        }
    }

    // a car with free hands in the box of a global star takes it: the Global Pulse at once (no spin), the star rises, grows and is gone for the race
    fn update_stars(&mut self, race: &Race, boxes: &[(Vector3<f32>, Vector3<f32>, Vector3<f32>)], dt: f32, sounds: &mut Sounds)
    {
        for star in &mut self.stars
        {
            match star.state
            {
                PickupState::Out(_) =>
                {
                    let half = Vector3::repeat(STAR_HALF_SIZE);
                    let (min, max) = (star.position - half, star.position + half);
                    let collector = boxes.iter().enumerate().find(|(index, (car_min, car_max, _))|
                        self.hands.get(*index).is_some_and(Hand::free) && (0..3).all(|a| min[a] <= car_max[a] && max[a] >= car_min[a])).map(|(index, _)| index);
                    let Some(index) = collector else { continue; };

                    let racer = &race.racers[index];
                    let velocity = boxes[index].2;
                    star.state = PickupState::Collected(0.0);
                    star.velocity = Vector3::new(velocity.x * 0.5, COLLECT_RISE, velocity.z * 0.5);
                    match racer.is_local()
                    {
                        true => sounds.push((COLLECT_SOUND, 1.0)),
                        false => play(&star.collect_sound),
                    }
                    let hand = &mut self.hands[index];
                    hand.weapon = Some(GLOBAL_PULSE);
                    hand.ammo = WEAPONS[GLOBAL_PULSE].1;
                    hand.fire_in = between(&mut self.random, BOT_FIRE_DELAY);
                    console_log!("weapons: {} takes the star - {}", racer.name, WEAPONS[GLOBAL_PULSE].0);
                },
                PickupState::Collected(time) =>
                {
                    let time = time + dt;
                    if time < COLLECT_TIME
                    {
                        star.state = PickupState::Collected(time);
                        star.show(true, 1.0 + 2.0 * time, star.position + star.velocity * time);
                        continue;
                    }
                    star.state = PickupState::Gone;
                    star.show(false, 1.0, star.position);
                },
                _ => {},
            }
        }
    }

    // one of the gone pickups comes out after APPEAR_DELAY
    fn activate_random(&mut self)
    {
        let gone: Vec<usize> = (0..self.pickups.len()).filter(|&i| self.pickups[i].state == PickupState::Gone).collect();
        if gone.is_empty() { return; }
        let i = gone[((self.random.next() * gone.len() as f32) as usize).min(gone.len() - 1)];
        self.pickups[i].state = PickupState::Waiting(APPEAR_DELAY);
    }

    // a random weapon: players by the weights of their place, bots by their fixed row
    fn pick(&mut self, racer: &Racer, racers: usize) -> usize
    {
        let weights = match racer.is_local()
        {
            true => weights(racer.position as usize - 1, racers),
            false => BOT_WEIGHTS,
        };
        let total: f32 = weights.iter().sum();
        let mut roll = self.random.next() * total;
        for (i, weight) in weights.iter().enumerate()
        {
            if *weight > 0.0 && roll < *weight { return i; }
            roll -= weight;
        }
        WEAPON_COUNT - 1
    }
}

impl Pickup
{
    fn show(&self, visible: bool, scale: f32, position: Vector3<f32>)
    {
        self.node.write().unwrap().settings.visible = visible;
        let node = self.node.read().unwrap();
        if let Some(transform) = node.find_component::<Transformation>()
        {
            component_downcast_mut!(transform, Transformation);
            let data = transform.get_data_mut().get_mut();
            data.scale = self.scale * scale.max(0.001);
            data.position = position;
            transform.calc_transform();
        }
    }
}

// the weights of the game: the first half of the field between the first place and the middle, the second half between the middle and the last
// one player alone: the first place
fn weights(position: usize, players: usize) -> [f32; WEAPON_COUNT]
{
    if players <= 1 { return WEIGHTS[0]; }
    let t = position as f32 / (players - 1) as f32;
    let (from, to) = if position < players / 2 { (WEIGHTS[0], WEIGHTS[1]) } else { (WEIGHTS[1], WEIGHTS[2]) };
    std::array::from_fn(|i| from[i] + (to[i] - from[i]) * t)
}

// the grow of a pickup coming out (the game: 0 for 0.5 s, then 2 (t - 0.5) + 0.667 sin(2π (t - 0.5)) up to 1 s, two smaller swings up to 1.6 s)
fn grow(time: f32) -> f32
{
    let tau = std::f32::consts::TAU;
    match time
    {
        t if t >= 1.6 => 1.0,
        t if t < 0.5 => 0.0,
        t if t < 1.0 => 2.0 * (t - 0.5) + 0.6667 * (tau * (t - 0.5)).sin(),
        t if t < 1.35 => 1.0 - 0.1667 * ((t - 1.0) * 8.97597).sin(),
        t => 1.0 + 0.08333 * ((t - 1.35) * 12.56637).sin(),
    }
}

// the world box of a car from its chassis bounds, and its velocity
fn car_box(scene: &Scene, racer: &Racer) -> (Vector3<f32>, Vector3<f32>, Vector3<f32>)
{
    let car = &racer.node;
    let transform = car.read().unwrap().get_full_transform();
    let id = car.read().unwrap().id;
    let bounds = scene.controller.iter()
        .filter_map(|controller| controller.as_any().downcast_ref::<VehicleController>())
        .find(|vehicle| vehicle.node.as_ref().is_some_and(|node| node.read().unwrap().id == id))
        .and_then(|vehicle| vehicle.chassis_bounds())
        .unwrap_or((Vector3::new(-0.3, -0.1, -0.3), Vector3::new(0.3, 0.2, 0.3)));

    let mut min = Vector3::repeat(f32::MAX);
    let mut max = Vector3::repeat(f32::MIN);
    for corner in 0..8
    {
        let local = Vector3::new(if corner & 1 == 0 { bounds.0.x } else { bounds.1.x }, if corner & 2 == 0 { bounds.0.y } else { bounds.1.y }, if corner & 4 == 0 { bounds.0.z } else { bounds.1.z });
        let world = (transform * local.push(1.0)).xyz();
        min = min.inf(&world);
        max = max.sup(&world);
    }
    let forward = -transform.column(2).xyz().normalize();
    (min, max, forward * racer.speed)
}

// a spatial one shot on the pickup node
fn add_sound(node: &NodeItem, name: &str, source: SoundSourceItem) -> ComponentItem
{
    let mut sound = Sound::new(name, source, SoundType::Spatial, false);
    sound.get_data_mut().get_mut().spatial_distance_scale = PICKUP_SOUND_DISTANCE;
    let component: ComponentItem = Arc::new(RwLock::new(Box::new(sound)));
    node.write().unwrap().add_component(component.clone());
    component
}

// from the start again
fn play(sound: &Option<ComponentItem>)
{
    let Some(component) = sound else { return; };
    component_downcast_mut!(component, Sound);
    component.stop();
    component.start();
}

fn between(random: &mut Random, [low, high]: [f32; 2]) -> f32
{
    low + random.next() * (high - low)
}

// the global stars (star type 0) - only the objects of the scene, not the star node inside their glb
fn collect_stars(nodes: &Vec<NodeItem>, stars: &mut Vec<Pickup>)
{
    for node in nodes
    {
        let is_star =
        {
            let read = node.read().unwrap();
            read.tags.contains("revolt_object") && read.extras.get::<i64>("revolt_object_type").and_then(|id| ObjectType::from_id(*id)) == Some(ObjectType::Star) && read.extras.get::<i64>("revolt_param_type") == Some(&0)
        };
        if is_star
        {
            let read = node.read().unwrap();
            if let Some(data) = read.find_component::<Transformation>().map(|transform| { component_downcast!(transform, Transformation); *transform.get_data() })
            {
                stars.push(Pickup { node: node.clone(), restore: Some(data), visible: read.settings.visible, position: data.position, scale: data.scale, state: PickupState::Gone, velocity: Vector3::zeros(), collect_sound: None, appear_sound: None });
            }
            continue;
        }
        let children = node.read().unwrap().nodes.clone();
        collect_stars(&children, stars);
    }
}

fn collect_pickups(nodes: &Vec<NodeItem>, pickups: &mut Vec<Pickup>)
{
    for node in nodes
    {
        let is_pickup = node.read().unwrap().extras.get::<i64>("revolt_object_type").and_then(|id| ObjectType::from_id(*id)) == Some(ObjectType::Pickup);
        if is_pickup
        {
            let read = node.read().unwrap();
            let data = read.find_component::<Transformation>().map(|transform| { component_downcast!(transform, Transformation); *transform.get_data() });
            if let Some(data) = data
            {
                pickups.push(Pickup { node: node.clone(), restore: Some(data), visible: read.settings.visible, position: data.position, scale: data.scale, state: PickupState::Gone, velocity: Vector3::zeros(), collect_sound: None, appear_sound: None });
            }
            continue;
        }
        let children = node.read().unwrap().nodes.clone();
        collect_pickups(&children, pickups);
    }
}

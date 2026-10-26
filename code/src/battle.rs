// battle tag like the game (game mode 6, formats/README.md "Battle Tag"): every car has a clock of 2 minutes, only the clock of the car with the star runs
// the stars of the arena give it to the car that touches them, a car that gets close to the holder with a free line of sight takes it over
// the first clock at 0 wins - the game has battle tag only over the network, the bots here are own code
// Battle Settings: one random star instead of all of them (like the videos of RVGL), the star taken over by ramming instead of getting close

use rustl::prelude::*;
use rustl::rapier3d::prelude::{QueryFilter, Ray, Vector};
use rustl::state::scene::physics::contacts::ContactTarget;
use rustl::state::scene::components::transformation::{Transformation, TransformationData};

use crate::lightning::Lightning;
use crate::object_type::ObjectType;
use crate::race::Racer;
use crate::random::Random;
use crate::weapons::car_box;

// s on every clock at the start (the game: 120000 ms)
pub const BATTLE_TIME: f64 = 120.0;
// m between the cars (center to center) to take the star over, with a free line of sight (the game: 384 units)
const TAG_DISTANCE: f32 = 3.84;
// s the car that lost the star can not take it back (the game: 2 s)
const IMMUNE_TIME: f32 = 2.0;
// the box of a star, m around it (like the game) - downwards it reaches the floor: the stars float ~0.45 m up, a flat car passed under the box
const STAR_HALF_SIZE: f32 = 0.3;
const STAR_REACH_BELOW: f32 = 0.7;
// collected: it rises 64 units/s, grows to 3 times in 1 s, then it is gone
const COLLECT_TIME: f32 = 1.0;
const COLLECT_RISE: f32 = 0.64;
// the first collected star floats over the holder: m above the car, size
const MARKER_HEIGHT: f32 = 0.6;
const MARKER_SCALE: f32 = 0.6;
// the line of sight starts this high above the car centers (the floor between two cars is no wall)
const SIGHT_HEIGHT: f32 = 0.2;
// s a message stays on the HUD
const MESSAGE_TIME: f32 = 2.0;
// the arc between the holder and a car close by (crate::lightning), up to this far
pub const ARC_FILE: &str = crate::lightning::GREEN_FILE;
const ARC_DISTANCE: f32 = 5.12;
// the game takes the vertex of each car nearest to the other one - from the car centers: this far towards the other car and up
const ARC_INSET: f32 = 0.2;
const ARC_LIFT: f32 = 0.1;

// the arc sparks at both ends like the game (electric sparks): per s, spread m/s
pub const ARC_SPARK_RATE: f32 = 50.0;
pub const ARC_SPARK_SPREAD: f32 = 2.0;

// the ends of an arc between two car centers
pub fn arc_ends(from: Vector3<f32>, to: Vector3<f32>) -> (Vector3<f32>, Vector3<f32>)
{
    let towards = (to - from).try_normalize(1e-6).unwrap_or_else(Vector3::x);
    (from + towards * ARC_INSET + Vector3::y() * ARC_LIFT, to - towards * ARC_INSET + Vector3::y() * ARC_LIFT)
}

// the bots: the holder flees to the arena point farthest from the others (a new one when reached or after a while), the others go for the holder ahead of it
const FLEE_REACHED: f32 = 4.0;
const FLEE_TIME: [f32; 2] = [3.0, 6.0];
const LEAD_TIME: f32 = 0.4;

#[derive(Clone, Copy, PartialEq)]
enum StarState
{
    Out,
    Collected(f32),
    Gone,
}

struct Star
{
    node: NodeItem,
    restore: Option<TransformationData>,
    visible: bool,
    position: Vector3<f32>,
    scale: Vector3<f32>,
    state: StarState,
    velocity: Vector3<f32>,
}

pub struct Battle
{
    // s left per racer (the order of race.racers)
    pub clocks: Vec<f64>,
    pub holder: Option<usize>,
    immune: Vec<f32>,
    stars: Vec<Star>,
    // the star that floats over the holder
    marker: Option<usize>,
    // where the bots can flee to: the stars, pickups and grid slots of the arena
    points: Vec<Vector3<f32>>,
    flee: Vec<Option<(usize, f32)>>,
    positions: Vec<Vector3<f32>>,
    velocities: Vec<Vector3<f32>>,
    random: Random,
    // for the HUD: a message (the star changed hands) and its seconds left
    pub message: Option<(String, f32)>,
    // sounds of this frame: name and where (None: the own car, stereo)
    pub sounds: Vec<(&'static str, Option<Vector3<f32>>)>,
    // Battle Settings: only one random star is out, the star goes over by contact
    one_star: bool,
    contact: bool,
    // the arc between the holder and a car close by (None until the app loaded its model)
    arc: Option<Option<Lightning>>,
    // the ends of the arc this frame (the app makes the sparks)
    pub arc_now: Option<(Vector3<f32>, Vector3<f32>)>,
}

impl Battle
{
    pub fn new(scene: &Scene, grid: &[Vector3<f32>], one_star: bool, contact: bool) -> Self
    {
        let mut stars = vec![];
        let mut points: Vec<Vector3<f32>> = grid.to_vec();
        collect(&scene.nodes, &mut stars, &mut points);
        console_log!("battle: {} stars{}, {} arena points, taken over {}", stars.len(), if one_star { " (one out)" } else { "" }, points.len(), if contact { "by contact" } else { "when close" });
        let mut battle = Self
        {
            clocks: vec![], holder: None, immune: vec![], stars, marker: None, points, flee: vec![], positions: vec![], velocities: vec![],
            random: Random::default(), message: None, sounds: vec![], one_star, contact, arc: None, arc_now: None,
        };
        battle.lay_out();
        battle
    }

    // every star out - or one random one, the others hidden
    fn lay_out(&mut self)
    {
        let chosen = ((self.random.next() * self.stars.len() as f32) as usize).min(self.stars.len().saturating_sub(1));
        for (index, star) in self.stars.iter_mut().enumerate()
        {
            let out = !self.one_star || index == chosen;
            star.state = if out { StarState::Out } else { StarState::Gone };
            show(star, out, 1.0, star.position);
        }
    }

    // one clock per racer - called whenever racers come
    pub fn resize(&mut self, racers: usize)
    {
        self.clocks.resize(racers, BATTLE_TIME);
        self.immune.resize(racers, 0.0);
        self.flee.resize(racers, None);
        self.positions.resize(racers, Vector3::zeros());
        self.velocities.resize(racers, Vector3::zeros());
    }

    // the stars out again, every clock full
    pub fn restart(&mut self)
    {
        self.clocks.iter_mut().for_each(|clock| *clock = BATTLE_TIME);
        self.immune.iter_mut().for_each(|time| *time = 0.0);
        self.flee.iter_mut().for_each(|flee| *flee = None);
        self.holder = None;
        self.marker = None;
        self.message = None;
        self.lay_out();
    }

    // the app loads the model of the arc once - None when it is missing (old assets)
    pub fn wants_arc(&self) -> bool
    {
        self.arc.is_none()
    }

    pub fn set_arc(&mut self, root: Option<NodeItem>)
    {
        self.arc = Some(root.and_then(|root| Lightning::new(root, ARC_GLOW)));
    }

    // Play stops: the stars as they were, the arc goes
    pub fn stop(&self, scene: &mut Scene)
    {
        if let Some(Some(arc)) = &self.arc
        {
            scene.delete_node_by_id(arc.root_id(), true, false, true, false);
        }
        for star in &self.stars
        {
            {
                let node = star.node.read().unwrap();
                if let (Some(data), Some(transform)) = (star.restore, node.find_component::<Transformation>())
                {
                    component_downcast_mut!(transform, Transformation);
                    transform.get_data_mut().set(data);
                }
            }
            star.node.write().unwrap().settings.visible = star.visible;
        }
    }

    // running: after GO and not over - the clock of the holder runs; the winner gets its finish time
    pub fn update(&mut self, scene: &Scene, racers: &mut [Racer], running: bool, race_time: f64, dt: f32)
    {
        self.resize(racers.len());
        self.sounds.clear();
        if let Some((_, left)) = self.message.as_mut() { *left -= dt; }
        self.message = self.message.take().filter(|(_, left)| *left > 0.0);

        for (index, racer) in racers.iter().enumerate()
        {
            let position = racer.node.read().unwrap().get_full_transform().column(3).xyz();
            if dt > 0.0 { self.velocities[index] = (position - self.positions[index]) / dt; }
            self.positions[index] = position;
        }
        for time in &mut self.immune { *time = (*time - dt).max(0.0); }

        self.update_stars(scene, racers, running, dt);
        if running { self.take_over(scene, racers); }

        // the clock of the holder
        if let Some(holder) = self.holder.filter(|_| running)
        {
            self.clocks[holder] = (self.clocks[holder] - dt as f64).max(0.0);
            if self.clocks[holder] <= 0.0 && !racers[holder].finished()
            {
                racers[holder].finish_time = Some(race_time);
                console_log!("battle: {} wins", racers[holder].name);
            }
        }
        self.update_marker();
        self.update_arc(scene, racers, dt);
    }

    // from the holder to the nearest car within ARC_DISTANCE (also a finished one, like the game) - hidden without
    fn update_arc(&mut self, scene: &Scene, racers: &[Racer], dt: f32)
    {
        let ends = self.holder.and_then(|holder|
        {
            let from = self.positions[holder];
            (0..racers.len()).filter(|&i| i != holder).map(|i| (i, (self.positions[i] - from).norm())).filter(|(_, d)| *d < ARC_DISTANCE)
                .min_by(|a, b| a.1.total_cmp(&b.1)).map(|(i, _)| (from, self.positions[i]))
        });
        self.arc_now = ends.map(|(from, to)| arc_ends(from, to));
        let Some(Some(arc)) = self.arc.as_mut() else { return; };
        match ends
        {
            Some((from, to)) => arc.draw(scene, &[arc_ends(from, to)], dt),
            None => arc.draw(scene, &[], dt),
        }
    }

    // a car in the box of a star that is out takes it - and the star with it (the game: any car, also with a weapon)
    fn update_stars(&mut self, scene: &Scene, racers: &[Racer], running: bool, dt: f32)
    {
        let boxes: Vec<(Vector3<f32>, Vector3<f32>, Vector3<f32>)> = racers.iter().map(|racer| car_box(scene, racer)).collect();
        for index in 0..self.stars.len()
        {
            match self.stars[index].state
            {
                StarState::Out if running =>
                {
                    let star = &self.stars[index];
                    let half = Vector3::repeat(STAR_HALF_SIZE);
                    let (min, max) = (star.position - Vector3::new(STAR_HALF_SIZE, STAR_REACH_BELOW, STAR_HALF_SIZE), star.position + half);
                    let taker = boxes.iter().enumerate().find(|(i, (car_min, car_max, _))| !racers[*i].finished() && (0..3).all(|a| min[a] <= car_max[a] && max[a] >= car_min[a])).map(|(i, _)| i);
                    let Some(taker) = taker else { continue; };
                    let velocity = boxes[taker].2;
                    let star = &mut self.stars[index];
                    star.state = StarState::Collected(0.0);
                    star.velocity = Vector3::new(velocity.x * 0.5, COLLECT_RISE, velocity.z * 0.5);
                    self.sounds.push(("pickup", (!racers[taker].is_local()).then_some(star.position)));
                    self.give(racers, taker, None);
                },
                StarState::Collected(time) =>
                {
                    let time = time + dt;
                    let star = &mut self.stars[index];
                    if time < COLLECT_TIME
                    {
                        star.state = StarState::Collected(time);
                        show(star, true, 1.0 + 2.0 * time, star.position + star.velocity * time);
                        continue;
                    }
                    star.state = StarState::Gone;
                    show(star, false, 1.0, star.position);
                    // the first one floats over the holder from now on
                    if self.marker.is_none() { self.marker = Some(index); }
                },
                _ => {},
            }
        }
    }

    // the nearest car close to the holder with a free line of sight takes the star over - not the one that just lost it
    // contact: a car touching the holder
    fn take_over(&mut self, scene: &Scene, racers: &[Racer])
    {
        let Some(holder) = self.holder else { return; };
        if self.contact
        {
            let holder_id = racers[holder].node.read().unwrap().id;
            let ids: Vec<u32> = racers.iter().map(|racer| racer.node.read().unwrap().id).collect();
            let taker = scene.physics.contacts_of(holder_id)
                .filter(|contact| !contact.stopped())
                .filter_map(|contact| match contact.other { ContactTarget::Vehicle { node_id } => ids.iter().position(|id| *id == node_id), _ => None })
                .find(|&i| i != holder && !racers[i].finished() && self.immune[i] <= 0.0);
            if let Some(taker) = taker { self.give(racers, taker, Some(holder)); }
            return;
        }
        let from = self.positions[holder];
        let taker = (0..racers.len())
            .filter(|&i| i != holder && !racers[i].finished() && self.immune[i] <= 0.0)
            .map(|i| (i, (self.positions[i] - from).norm()))
            .filter(|(i, distance)| *distance < TAG_DISTANCE && sight(scene, from, self.positions[*i]))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i);
        if let Some(taker) = taker
        {
            self.give(racers, taker, Some(holder));
        }
    }

    // the star to a car: from the car that had it when it was taken over (it may not take it back for a while)
    fn give(&mut self, racers: &[Racer], to: usize, from: Option<usize>)
    {
        if self.holder == Some(to) { return; }
        if let Some(from) = from { self.immune[from] = IMMUNE_TIME; }
        self.holder = Some(to);
        self.flee[to] = None;
        let alone = racers.iter().filter(|racer| racer.is_local()).count() == 1;
        let text = if racers[to].is_local() && alone { "You have the star!".to_string() } else { format!("{} has the star!", racers[to].name) };
        console_log!("battle: {} has the star{}", racers[to].name, from.map_or(String::new(), |from| format!(" (from {})", racers[from].name)));
        self.message = Some((text, MESSAGE_TIME));
    }

    // the marker star over the holder, hidden without one
    fn update_marker(&self)
    {
        let Some(star) = self.marker.and_then(|index| self.stars.get(index)) else { return; };
        match self.holder
        {
            Some(holder) => show(star, true, MARKER_SCALE, self.positions[holder] + Vector3::y() * MARKER_HEIGHT),
            None => show(star, false, 1.0, star.position),
        }
    }

    // where a bot drives: the holder flees, the others go for the holder (or the nearest star while nobody has one)
    pub fn target(&mut self, index: usize, dt: f32) -> Option<Vector3<f32>>
    {
        let position = *self.positions.get(index)?;
        match self.holder
        {
            Some(holder) if holder == index => Some(self.flee_point(index, dt)),
            Some(holder) => Some(self.positions[holder] + self.velocities[holder] * LEAD_TIME),
            None =>
            {
                let star = self.stars.iter().filter(|star| star.state == StarState::Out).map(|star| star.position)
                    .min_by(|a, b| (a - position).norm().total_cmp(&(b - position).norm()));
                star.or_else(|| Some(self.flee_point(index, dt)))
            },
        }
    }

    // the arena point farthest from the others - kept until it is reached or its time is up
    fn flee_point(&mut self, index: usize, dt: f32) -> Vector3<f32>
    {
        let position = self.positions[index];
        if let Some((point, time)) = self.flee[index]
        {
            let left = time - dt;
            if left > 0.0 && (self.points[point] - position).norm() > FLEE_REACHED
            {
                self.flee[index] = Some((point, left));
                return self.points[point];
            }
        }
        if self.points.is_empty() { return position; }
        let others: Vec<Vector3<f32>> = (0..self.positions.len()).filter(|&i| i != index).map(|i| self.positions[i]).collect();
        let score = |point: &Vector3<f32>| others.iter().map(|other| (point - other).norm()).fold(f32::MAX, f32::min) - (point - position).norm() * 0.3;
        let best = (0..self.points.len())
            .filter(|&i| (self.points[i] - position).norm() > FLEE_REACHED)
            .max_by(|&a, &b| score(&self.points[a]).total_cmp(&score(&self.points[b])))
            .unwrap_or(0);
        let time = FLEE_TIME[0] + self.random.next() * (FLEE_TIME[1] - FLEE_TIME[0]);
        self.flee[index] = Some((best, time));
        self.points[best]
    }

    // where a car has to go: the holder, or the nearest star while nobody has one - None for the holder itself
    // the turn from the heading (rad, positive = to the right) and the m on the ground
    pub fn pointer(&self, index: usize, racers: &[Racer]) -> Option<(f32, f32)>
    {
        let position = *self.positions.get(index)?;
        let target = match self.holder
        {
            Some(holder) if holder == index => return None,
            Some(holder) => self.positions[holder],
            None => self.stars.iter().filter(|star| star.state == StarState::Out).map(|star| star.position)
                .min_by(|a, b| (a - position).norm().total_cmp(&(b - position).norm()))?,
        };
        let transform = racers.get(index)?.node.read().unwrap().get_full_transform();
        let forward = -transform.column(2).xyz().normalize();
        let right = transform.column(0).xyz().normalize();
        let to = target - position;
        Some((to.dot(&right).atan2(to.dot(&forward)), Vector3::new(to.x, 0.0, to.z).norm()))
    }

    // the ranking: the winner first, then by the clock left (less is better)
    pub fn order(&self, racers: &[Racer]) -> Vec<usize>
    {
        let mut order: Vec<usize> = (0..racers.len()).collect();
        order.sort_by(|&a, &b| match (racers[a].finish_time, racers[b].finish_time)
        {
            (Some(ta), Some(tb)) => ta.total_cmp(&tb),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => self.clock(a).total_cmp(&self.clock(b)),
        });
        order
    }

    pub fn clock(&self, index: usize) -> f64
    {
        self.clocks.get(index).copied().unwrap_or(BATTLE_TIME)
    }
}

// the green of the strip glows for the bloom
const ARC_GLOW: Vector3<f32> = Vector3::new(0.3, 1.2, 0.2);

// no level geometry between two cars
fn sight(scene: &Scene, from: Vector3<f32>, to: Vector3<f32>) -> bool
{
    let (from, to) = (from + Vector3::y() * SIGHT_HEIGHT, to + Vector3::y() * SIGHT_HEIGHT);
    let direction = to - from;
    let distance = direction.norm();
    if distance < 0.01 { return true; }
    let direction = direction / distance;
    let ray = Ray::new(Vector::new(from.x, from.y, from.z), Vector::new(direction.x, direction.y, direction.z));
    scene.physics.query_pipeline(QueryFilter::only_fixed().exclude_sensors()).cast_ray(&ray, distance, true).is_none()
}

fn show(star: &Star, visible: bool, scale: f32, position: Vector3<f32>)
{
    star.node.write().unwrap().settings.visible = visible;
    let node = star.node.read().unwrap();
    if let Some(transform) = node.find_component::<Transformation>()
    {
        component_downcast_mut!(transform, Transformation);
        let data = transform.get_data_mut().get_mut();
        data.scale = star.scale * scale.max(0.001);
        data.position = position;
        transform.calc_transform();
    }
}

// the stars (both types - the game puts all of them out in battle tag) and the pickups as flee points
fn collect(nodes: &Vec<NodeItem>, stars: &mut Vec<Star>, points: &mut Vec<Vector3<f32>>)
{
    for node in nodes
    {
        let kind =
        {
            let read = node.read().unwrap();
            read.tags.contains("revolt_object").then(|| read.extras.get::<i64>("revolt_object_type").and_then(|id| ObjectType::from_id(*id))).flatten()
        };
        if matches!(kind, Some(ObjectType::Star | ObjectType::Pickup))
        {
            let read = node.read().unwrap();
            if let Some(data) = read.find_component::<Transformation>().map(|transform| { component_downcast!(transform, Transformation); *transform.get_data() })
            {
                points.push(data.position);
                if kind == Some(ObjectType::Star)
                {
                    stars.push(Star { node: node.clone(), restore: Some(data), visible: read.settings.visible, position: data.position, scale: data.scale, state: StarState::Out, velocity: Vector3::zeros() });
                }
            }
            continue;
        }
        let children = node.read().unwrap().nodes.clone();
        collect(&children, stars, points);
    }
}

// the battle arenas of the game, in its order
pub const ARENAS: [&str; 4] = ["nhood1_battle", "bot_bat", "muse_bat", "markar"];

pub fn is_arena(scene_name: &str) -> bool
{
    ARENAS.contains(&scene_name.to_lowercase().as_str())
}

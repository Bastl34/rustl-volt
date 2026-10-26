// a computer driver: follows the racing line of the AI nodes (.fan), the game is only the inspiration
// steers towards a point ahead on the line (pure pursuit), takes its speed from the curves ahead, frees itself, and is put back onto the line when lost

use rustl::prelude::*;
use rustl::state::scene::scene_controller::vehicle_controller::VehicleDriveInput;

use crate::random::Random;
use crate::track::{AiPlace, Track};

// look ahead for steering: m at a standstill plus s of the speed
const LOOK_MIN: f32 = 2.5;
const LOOK_TIME: f32 = 0.35;
const LOOK_MAX: f32 = 14.0;
// cornering and braking the bots plan with at Hard, m/s² (the world has 22 m/s² gravity; 13 / 14 were too careful)
const LATERAL_ACCEL: f32 = 18.0;
const BRAKE_DECEL: f32 = 16.0;
// the target speed may lie this much above the top speed - full throttle up to it, the engine caps the speed itself
const TOP_SPEED_MARGIN: f32 = 1.1;

// how hard the bots drive - the game has Easy / Medium / Hard / Extreme in the menu (its effect on the AI is not decoded, this is an own scale)
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Difficulty
{
    Easy,
    Medium,
    Hard,
    Extreme,
}

impl Difficulty
{
    pub const ALL: [Difficulty; 4] = [Difficulty::Easy, Difficulty::Medium, Difficulty::Hard, Difficulty::Extreme];

    // the label of the game menu
    pub fn label(self) -> &'static str
    {
        match self
        {
            Difficulty::Easy => "Easy",
            Difficulty::Medium => "Medium",
            Difficulty::Hard => "Hard",
            Difficulty::Extreme => "Extreme",
        }
    }

    // share of the grip and the top speed the bots use: lowest and highest of the field
    fn pace(self) -> [f32; 2]
    {
        match self
        {
            Difficulty::Easy => [0.80, 0.88],
            Difficulty::Medium => [0.88, 0.95],
            Difficulty::Hard => [0.95, 1.0],
            Difficulty::Extreme => [1.0, 1.08],
        }
    }
}
// the speed plan samples the line every CURVE_STEP m and measures the heading change over ±CURVE_SPAN m
const CURVE_STEP: f32 = 2.0;
const CURVE_SPAN: f32 = 6.0;
// m of the line looked at for the speed beyond the braking distance
const SPEED_LOOK_EXTRA: f32 = 6.0;
// m/s around the target speed without throttle or brake
const SPEED_BAND: f32 = 0.5;
// stuck: below this speed with throttle this long -> reverse this long
const STUCK_SPEED: f32 = 0.6;
const STUCK_TIME: f32 = 1.2;
const REVERSE_TIME: f32 = 1.0;
// put back onto the line: after this many reverses in a row, upside down / off the track this long
const STUCK_RESETS: u32 = 3;
const LOST_TIME: f32 = 2.0;
// m beyond the track edge that counts as off the track
const OFF_TRACK: f32 = 3.0;
// up.y below this is on the side or the roof
const UPRIGHT: f32 = 0.3;
// the bot wanders around the racing line: share of the track width, a new aim every few s, share per s it moves
const WANDER: f32 = 0.18;
const WANDER_TIME: [f32; 2] = [1.5, 5.0];
const WANDER_RATE: f32 = 0.08;
// and changes its pace a bit now and then
const PACE_JITTER: f32 = 0.04;
// battle tag: share of the speed taken away for a point straight behind (half of it at 90°)
const ARENA_TURN_SLOWDOWN: f32 = 0.6;

// what a bot sees of its car
pub struct BotView
{
    pub position: Vector3<f32>,
    pub forward: Vector3<f32>,
    pub right: Vector3<f32>,
    pub up: Vector3<f32>,
    // m/s along forward, negative backwards
    pub speed: f32,
    pub top_speed: f32,
    pub wheelbase: f32,
    // radians of full lock
    pub max_steer: f32,
    // share of its speed it drives at - Junior RC: a bot far ahead of the best Junior RC player waits for it (1 = full)
    pub catch_up: f32,
}

pub enum BotAction
{
    Drive(VehicleDriveInput),
    // put the car there, facing along heading - the reason for the log
    Reset { position: Vector3<f32>, heading: Vector3<f32>, reason: &'static str },
}

pub struct Bot
{
    place: Option<AiPlace>,
    random: Random,
    // share of the track width the bot drives beside the racing line: its own one, plus the wander towards the aim
    offset: f32,
    wander: f32,
    wander_aim: f32,
    wander_time: f32,
    // share of the grip and the top speed it uses: its own one, plus a jitter
    pace: f32,
    pace_jitter: f32,
    // picks the branch at forks
    fork: usize,
    stuck_time: f32,
    reverse_time: f32,
    reverses: u32,
    lost_time: f32,
}

impl Bot
{
    pub fn new(random: &mut Random, difficulty: Difficulty) -> Self
    {
        Self
        {
            place: None,
            random: Random::seeded((random.next() * u32::MAX as f32) as u32),
            offset: (random.next() - 0.5) * 0.16,
            wander: 0.0,
            wander_aim: 0.0,
            wander_time: 0.0,
            pace: { let [low, high] = difficulty.pace(); low + random.next() * (high - low) },
            pace_jitter: 0.0,
            fork: (random.next() * 1000.0) as usize,
            stuck_time: 0.0,
            reverse_time: 0.0,
            reverses: 0,
            lost_time: 0.0,
        }
    }

    pub fn drive(&mut self, track: &Track, view: &BotView, dt: f32) -> BotAction
    {
        self.place = track.ai_place(view.position, self.place);
        let Some(place) = self.place else { return BotAction::Drive(VehicleDriveInput::default()); };

        // lost: upside down, far off the line or out of every zone - or stuck again and again
        let reason = if view.up.y < UPRIGHT { Some("upside down") }
            else if place.distance > track.half_width(place.node) + OFF_TRACK { Some("off the track") }
            else if !track.in_any_zone(view.position) { Some("out of every zone") }
            else { None };
        self.lost_time = if reason.is_some() { self.lost_time + dt } else { 0.0 };
        if self.lost_time > LOST_TIME
        {
            return self.reset(track, place, reason.unwrap_or("lost"));
        }
        if self.reverses >= STUCK_RESETS
        {
            return self.reset(track, place, "stuck after 3 reverses");
        }

        self.update_wander(dt);
        let line = self.line_ahead(track, place, view.position, LOOK_MAX.max(view.speed * view.speed / (2.0 * BRAKE_DECEL) + SPEED_LOOK_EXTRA));

        let mut steer = steer_along(view, &line);

        // speed: the slowest the curves ahead allow, minus what braking until there takes away
        let pace = self.pace + self.pace_jitter;
        let mut target_speed = view.top_speed * pace * TOP_SPEED_MARGIN;
        // from the point on the line on (the way from the car to it is no curve), sampled evenly: the heading change over ±CURVE_SPAN
        // node to node the small kinks of the line looked like sharp curves - the bots braked all the time
        let samples = resample(&line[1..], CURVE_STEP);
        let span = (CURVE_SPAN / CURVE_STEP).round().max(1.0) as usize;
        for i in span..samples.len().saturating_sub(span)
        {
            let (a, b, c) = (samples[i - span].0, samples[i].0, samples[i + span].0);
            let (ab, bc) = (flat(b - a), flat(c - b));
            if ab.norm() < 0.01 || bc.norm() < 0.01 { continue; }
            let curvature = ab.angle(&bc) / ((ab.norm() + bc.norm()) * 0.5);
            if curvature < 1e-4 { continue; }
            let corner = (LATERAL_ACCEL * pace / curvature).sqrt();
            let distance = samples[i].1.max(0.0);
            target_speed = target_speed.min((corner * corner + 2.0 * BRAKE_DECEL * distance).sqrt());
        }

        target_speed *= view.catch_up;

        let mut drive = VehicleDriveInput::default();
        let error = target_speed - view.speed;
        if error > SPEED_BAND { drive.throttle = 1.0; }
        else if error < -SPEED_BAND { drive.brake = (-error / 4.0).clamp(0.2, 1.0); }

        // stuck at a wall: back off with the steering the other way
        if self.reverse_time > 0.0
        {
            self.reverse_time -= dt;
            drive.throttle = 0.0;
            drive.brake = 1.0;
            steer = -steer;
        }
        else if drive.throttle > 0.0 && view.speed.abs() < STUCK_SPEED
        {
            self.stuck_time += dt;
            if self.stuck_time > STUCK_TIME
            {
                self.stuck_time = 0.0;
                self.reverse_time = REVERSE_TIME;
                self.reverses += 1;
            }
        }
        else if view.speed > STUCK_SPEED * 3.0
        {
            self.stuck_time = 0.0;
            self.reverses = 0;
        }

        drive.steer = steer;
        BotAction::Drive(drive)
    }

    // battle tag (no track): straight at a point - slower the sharper the turn, full lock for a point behind, the reverse when stuck, the recover of the engine on the roof
    pub fn drive_to(&mut self, view: &BotView, target: Vector3<f32>, dt: f32) -> BotAction
    {
        self.update_wander(dt);
        let to_target = target - view.position;
        let distance = flat(to_target).norm();
        let alpha = to_target.dot(&-view.right).atan2(to_target.dot(&view.forward));
        let look = distance.max(LOOK_MIN);
        let wheel_angle = (2.0 * view.wheelbase * alpha.sin() / look).atan();
        let mut steer = if alpha.abs() > std::f32::consts::FRAC_PI_2 { alpha.signum() } else { (wheel_angle / view.max_steer.max(0.1)).clamp(-1.0, 1.0) };

        let pace = self.pace + self.pace_jitter;
        let turn = (alpha.abs() / std::f32::consts::PI).min(1.0);
        let target_speed = view.top_speed * pace * TOP_SPEED_MARGIN * (1.0 - ARENA_TURN_SLOWDOWN * turn);
        let mut drive = VehicleDriveInput::default();
        let error = target_speed - view.speed;
        if error > SPEED_BAND { drive.throttle = 1.0; }
        else if error < -SPEED_BAND { drive.brake = (-error / 4.0).clamp(0.2, 1.0); }

        // on the side or the roof for a while: the recover of the engine puts it back on its wheels
        self.lost_time = if view.up.y < UPRIGHT { self.lost_time + dt } else { 0.0 };
        if self.lost_time > LOST_TIME
        {
            self.lost_time = 0.0;
            drive.recover = true;
        }

        if self.reverse_time > 0.0
        {
            self.reverse_time -= dt;
            drive.throttle = 0.0;
            drive.brake = 1.0;
            steer = -steer;
        }
        else if drive.throttle > 0.0 && view.speed.abs() < STUCK_SPEED
        {
            self.stuck_time += dt;
            if self.stuck_time > STUCK_TIME
            {
                self.stuck_time = 0.0;
                self.reverse_time = REVERSE_TIME;
            }
        }
        else if view.speed > STUCK_SPEED * 3.0
        {
            self.stuck_time = 0.0;
        }

        drive.steer = steer;
        BotAction::Drive(drive)
    }

    // a new aim beside the line now and then, moved to slowly - no two laps the same
    fn update_wander(&mut self, dt: f32)
    {
        self.wander_time -= dt;
        if self.wander_time <= 0.0
        {
            self.wander_time = WANDER_TIME[0] + self.random.next() * (WANDER_TIME[1] - WANDER_TIME[0]);
            self.wander_aim = (self.random.next() * 2.0 - 1.0) * WANDER;
            self.pace_jitter = (self.random.next() * 2.0 - 1.0) * PACE_JITTER;
        }
        let step = WANDER_RATE * dt;
        self.wander += (self.wander_aim - self.wander).clamp(-step, step);
    }

    // on the racing line of the next node, facing along the track
    fn reset(&mut self, track: &Track, place: AiPlace, reason: &'static str) -> BotAction
    {
        self.lost_time = 0.0;
        self.stuck_time = 0.0;
        self.reverse_time = 0.0;
        self.reverses = 0;
        reset_action(track, place, self.fork, reason)
    }

    // no progress along the track for a long time (water, a gap the reverse does not get out of): put back `skip` nodes further ahead
    pub fn reset_ahead(&mut self, track: &Track, skip: usize) -> Option<BotAction>
    {
        let mut place = self.place?;
        for _ in 0..skip
        {
            let next = &track.ai[place.next].next;
            if next.is_empty() { break; }
            place = AiPlace { node: place.next, next: next[(self.fork + place.next) % next.len()], ..place };
        }
        Some(self.reset(track, place, "no progress"))
    }

    fn line_ahead(&self, track: &Track, place: AiPlace, position: Vector3<f32>, length: f32) -> Vec<(Vector3<f32>, f32)>
    {
        line_ahead(track, place, position, length, self.offset + self.wander, self.fork)
    }
}

// the racing line (moved sideways by offset) from the car on: points and their distance along it, forks picked by fork
pub fn line_ahead(track: &Track, place: AiPlace, position: Vector3<f32>, length: f32, offset: f32, fork: usize) -> Vec<(Vector3<f32>, f32)>
{
    let start = track.racing_point(place.node, offset).lerp(&track.racing_point(place.next, offset), place.t);
    let mut points = vec![(position, 0.0), (start, 0.0)];
    let mut distance = (start - position).norm();
    points[1].1 = distance;

    let mut node = place.next;
    let mut last = start;
    for _ in 0..64
    {
        let point = track.racing_point(node, offset);
        distance += (point - last).norm();
        points.push((point, distance));
        last = point;
        if distance > length { break; }
        let next = &track.ai[node].next;
        if next.is_empty() { break; }
        node = next[(fork + node) % next.len()];
    }
    points
}

// the steering (-1..1) towards the point at the look ahead distance on the line: the wheel angle of a circle through it
pub fn steer_along(view: &BotView, line: &[(Vector3<f32>, f32)]) -> f32
{
    let look = (LOOK_MIN + view.speed.max(0.0) * LOOK_TIME).min(LOOK_MAX);
    let target = point_at(line, look);
    let to_target = target - view.position;
    let alpha = to_target.dot(&-view.right).atan2(to_target.dot(&view.forward));
    let wheel_angle = (2.0 * view.wheelbase * alpha.sin() / look.max(0.1)).atan();
    (wheel_angle / view.max_steer.max(0.1)).clamp(-1.0, 1.0)
}

// a car put back onto the track: the racing line of the node ahead, facing to the one after it
pub fn reset_action(track: &Track, place: AiPlace, fork: usize, reason: &'static str) -> BotAction
{
    let node = place.next;
    let next = track.ai[node].next.get(fork % track.ai[node].next.len().max(1)).copied().unwrap_or(node);
    let heading = if next != node { track.segment_direction(node, next) } else { track.segment_direction(place.node, node) };
    BotAction::Reset { position: track.racing_point(node, 0.0), heading, reason }
}

// points every `step` m along the line, with their distance
fn resample(line: &[(Vector3<f32>, f32)], step: f32) -> Vec<(Vector3<f32>, f32)>
{
    let Some(&(_, first)) = line.first() else { return vec![]; };
    let last = line.last().map_or(first, |p| p.1);
    let count = ((last - first) / step).floor().max(0.0) as usize;
    (0..=count).map(|i| { let d = first + i as f32 * step; (point_at(line, d), d) }).collect()
}

// the point `distance` m along the line
fn point_at(line: &[(Vector3<f32>, f32)], distance: f32) -> Vector3<f32>
{
    for pair in line.windows(2)
    {
        let ((a, da), (b, db)) = (pair[0], pair[1]);
        if db >= distance && db > da
        {
            return a.lerp(&b, ((distance - da) / (db - da)).clamp(0.0, 1.0));
        }
    }
    line.last().map_or(Vector3::zeros(), |p| p.0)
}

fn flat(v: Vector3<f32>) -> Vector3<f32>
{
    Vector3::new(v.x, 0.0, v.z)
}

// the driving modes of the game (Select Mode: Simulation, Arcade, Console, Junior RC - formats/README.md "Driving modes") with own assists for the player car
// the exe: from Console on, ground hits give the car no spin; Junior RC changes car values too - here: stability by damping roll and pitch,
// Junior RC also steers back onto the track near its edges (like the smart steering of kart games), turns a car around that drives the wrong way
// and puts it back onto the track when it is lost (upside down, the wrong way, far off the track, stuck while the driver tries to drive)

use rustl::prelude::*;
use rustl::rapier3d::prelude::Vector;

use crate::track::{AiPlace, Track};

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum DriveMode
{
    Simulation,
    #[default]
    Arcade,
    Console,
    JuniorRc,
}

// 1/s roll and pitch damping of the stable modes
const STABILITY: [f32; 2] = [4.0, 8.0];
// the car counts as upright from this up.y on - on its side or roof the recover is the way out
const UPRIGHT: f32 = 0.3;
// steering help: the car looks this far ahead, the edge zone is this share of the width on each side
const LOOK_AHEAD: f32 = 0.4;
const EDGE_ZONE: f32 = 0.2;
// m ahead along the track the help aims for, steering per radian off that aim
const AIM_AHEAD: f32 = 6.0;
const AIM_GAIN: f32 = 2.5;
// m/s below which the edge help does nothing, the turning around below the lower one
const MIN_SPEED: f32 = 3.0;
const MIN_TURN_SPEED: f32 = 1.0;
// share of the steering the turning around takes
const TURN_BACK_WEIGHT: f32 = 0.85;
// Junior RC puts the car back onto the track after this many s: upside down, the wrong way (after the 1 s of the HUD), far off the track
const RESPAWN_UPSIDE_DOWN: f32 = 1.5;
const RESPAWN_WRONG_WAY: f32 = 3.0;
const RESPAWN_OFF_TRACK: f32 = 2.0;
const RESPAWN_STUCK: f32 = 3.0;
// stuck: the driver gives throttle or brake (reverse) this far down, but the car gets no further than this (m) from where it was - jiggling in a corner too
const STUCK_INPUT: f32 = 0.3;
const STUCK_DISTANCE: f32 = 1.5;
// s without input the watch tolerates (switching between forward and reverse) - longer and standing still is the driver's choice
const STUCK_GAP: f32 = 1.0;
// m beyond the half width that count as far off the track
const OFF_TRACK: f32 = 3.0;

impl DriveMode
{
    pub const ALL: [DriveMode; 4] = [DriveMode::Simulation, DriveMode::Arcade, DriveMode::Console, DriveMode::JuniorRc];

    // the labels of the game menu
    pub fn label(self) -> &'static str
    {
        match self
        {
            DriveMode::Simulation => "Simulation",
            DriveMode::Arcade => "Arcade",
            DriveMode::Console => "Console",
            DriveMode::JuniorRc => "Junior RC",
        }
    }

    pub fn hint(self) -> &'static str
    {
        match self
        {
            DriveMode::Simulation => "No driving aids, no sliding along walls",
            DriveMode::Arcade => "Like the game: slides along walls",
            DriveMode::Console => "Arcade + a steadier car, bumps do not flip it",
            DriveMode::JuniorRc => "Console + steers back onto the track at its edges",
        }
    }

    // sliding along walls (the engine wall slide) - off only in Simulation
    pub fn wall_slide(self) -> bool
    {
        self != DriveMode::Simulation
    }

    fn stability(self) -> Option<f32>
    {
        match self
        {
            DriveMode::Console => Some(STABILITY[0]),
            DriveMode::JuniorRc => Some(STABILITY[1]),
            _ => None,
        }
    }
}

// roll and pitch of an upright car die down - its yaw stays (the driver steers)
pub fn stabilize(scene: &mut Scene, id: u32, mode: DriveMode, dt: f32)
{
    let Some(rate) = mode.stability() else { return; };
    let Some((_, body)) = scene.physics.vehicle_mut(id) else { return; };
    let up = body.rotation() * Vector::Y;
    if up.y < UPRIGHT { return; }
    let spin = body.angvel();
    let yaw = up * spin.dot(up);
    let keep = (-rate * dt).exp();
    body.set_angvel(yaw + (spin - yaw) * keep, true);
}

// Junior RC: (steer, weight) for the vehicle steer assist - near an edge, or heading out of it soon, the car is steered back along the track
// speed: m/s along the car, negative backwards - backwards the steering turns the car the other way
pub fn steer_back(track: &Track, place: Option<AiPlace>, position: Vector3<f32>, forward: Vector3<f32>, right: Vector3<f32>, speed: f32) -> Option<(f32, f32)>
{
    let place = place?;
    if speed.abs() < MIN_TURN_SPEED || !track.has_ai() { return None; }
    let along = track.segment_direction(place.node, place.next);
    // the wrong way: the nose turned towards the track direction the shorter way round
    if Vector3::new(forward.x, 0.0, forward.z).dot(&along) < 0.0
    {
        let steer = if along.dot(&right) > 0.0 { -1.0 } else { 1.0 };
        return Some((steer * speed.signum(), TURN_BACK_WEIGHT));
    }
    // the edge help looks ahead - not while backing up
    if speed < MIN_SPEED { return None; }

    // the share across the track (0 left edge, 1 right edge) now and a moment ahead
    let left = track.ai_point(place.node, 0.0).lerp(&track.ai_point(place.next, 0.0), place.t);
    let edge = track.ai_point(place.node, 1.0).lerp(&track.ai_point(place.next, 1.0), place.t) - left;
    let width = Vector3::new(edge.x, 0.0, edge.z);
    if width.norm_squared() < 0.01 { return None; }
    let across = |p: Vector3<f32>| { let d = p - left; Vector3::new(d.x, 0.0, d.z).dot(&width) / width.norm_squared() };
    let ahead = position + forward * speed * LOOK_AHEAD;
    let share = across(position).min(across(ahead)).min(1.0 - across(position).max(across(ahead)));
    let weight = ((EDGE_ZONE - share) / EDGE_ZONE).clamp(0.0, 1.0);
    if weight <= 0.0 { return None; }

    // aim at the middle of the track a bit ahead
    let aim = left + edge * 0.5 + along * AIM_AHEAD;
    let to_aim = aim - position;
    let angle = to_aim.dot(&right).atan2(to_aim.dot(&forward));
    // positive steering is left, the angle is positive to the right
    Some(((-angle * AIM_GAIN).clamp(-1.0, 1.0), weight))
}

// Junior RC: stuck while the driver tries to drive - by the way made, not the speed
#[derive(Default)]
pub struct StuckWatch
{
    anchor: Option<Vector3<f32>>,
    time: f32,
    idle: f32,
}

impl StuckWatch
{
    // throttle, brake: what the driver gives (vehicle telemetry) - true once stuck for RESPAWN_STUCK
    pub fn update(&mut self, position: Vector3<f32>, throttle: f32, brake: f32, dt: f32) -> bool
    {
        let trying = throttle.abs() > STUCK_INPUT || brake > STUCK_INPUT;
        self.idle = if trying { 0.0 } else { self.idle + dt };
        if self.idle > STUCK_GAP
        {
            self.clear();
            return false;
        }
        let anchor = *self.anchor.get_or_insert(position);
        if (position - anchor).norm() > STUCK_DISTANCE
        {
            self.anchor = Some(position);
            self.time = 0.0;
            return false;
        }
        self.time += dt;
        self.time > RESPAWN_STUCK
    }

    pub fn clear(&mut self)
    {
        *self = Self::default();
    }
}

// Junior RC: why the car should be put back onto the track and after how many s - None while it drives fine (stuck: the watch waited already)
pub fn lost(track: &Track, place: Option<AiPlace>, up_y: f32, wrong_way: bool, stuck: bool) -> Option<(&'static str, f32)>
{
    let off_track = place.is_some_and(|place| place.distance > track.half_width(place.node) + OFF_TRACK);
    if up_y < UPRIGHT { Some(("upside down", RESPAWN_UPSIDE_DOWN)) }
    else if wrong_way { Some(("the wrong way", RESPAWN_WRONG_WAY)) }
    else if off_track { Some(("off the track", RESPAWN_OFF_TRACK)) }
    else if stuck { Some(("stuck", 0.0)) }
    else { None }
}

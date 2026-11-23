// the driving modes of the game (Select Mode: Simulation, Arcade, Console, Junior RC - formats/README.md "Driving modes") with own assists for the player car
// the game: from Console on, ground hits give the car no spin; Junior RC changes car values too - here: stability by damping roll and pitch,
// Junior RC also steers back onto the track near its edges (like the smart steering of kart games), turns a car round on the track that drives the wrong way
// and puts it back onto the track when it is lost (upside down, the wrong way, far off the track, stuck while the driver tries to drive)

use rustl::prelude::*;
use rustl::rapier3d::prelude::Vector;

use crate::bot::{self, BotView};
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
// steering help: s the car looks ahead for the edges, the edge zone is this share of the width on each side
const LOOK_AHEAD: f32 = 0.4;
const EDGE_ZONE: f32 = 0.2;
// m of racing line the help follows
const LINE_LENGTH: f32 = 16.0;
// m/s below which the help does nothing
const MIN_TURN_SPEED: f32 = 1.0;
// 1/s the weight of the help changes by - in and out without a jolt of the wheels
pub const BLEND_RATE: f32 = 5.0;
// driving the wrong way: nose against the track direction beyond this dot, for this many s - then turned round on the racing line
const TURN_ROUND_DOT: f32 = 0.5;
const TURN_ROUND_AFTER: f32 = 1.0;
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
            DriveMode::Simulation => "No driving aids, no sliding along walls, bumps from cars spin",
            DriveMode::Arcade => "Like the game: slides along walls, bumps from cars do not spin",
            DriveMode::Console => "Arcade + a steadier car, bumps do not flip it",
            DriveMode::JuniorRc => "Console + keeps you on the track, puts you back when lost, bots wait",
        }
    }

    // sliding along walls (the engine wall slide) and car hits through the centers (center hits, the game from Arcade on) - off only in Simulation
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

// Junior RC: (steer, weight) for the vehicle steer assist - near an edge, now or a moment ahead, the car is steered along the racing line
// like a bot, the driver drives everywhere else; view.speed negative backwards
pub fn steer_back(track: &Track, place: Option<AiPlace>, view: &BotView) -> Option<(f32, f32)>
{
    let place = place?;
    if view.speed.abs() < MIN_TURN_SPEED || !track.has_ai() { return None; }
    // the wrong way: no help (it steered fast cars into the walls) - turn_round puts the car round; backing up is the driver's way out
    let along = track.segment_direction(place.node, place.next);
    if Vector3::new(view.forward.x, 0.0, view.forward.z).dot(&along) < 0.0 || view.speed < 0.0 { return None; }

    let ahead = view.position + view.forward * view.speed * LOOK_AHEAD;
    let share = [view.position, ahead].iter().filter_map(|p| across(track, track.ai_place(*p, Some(place))?, *p)).map(|a| a.min(1.0 - a)).fold(f32::MAX, f32::min);
    let weight = ((EDGE_ZONE - share) / EDGE_ZONE).clamp(0.0, 1.0);
    if weight <= 0.0 { return None; }

    let line = bot::line_ahead(track, place, view.position, LINE_LENGTH, 0.0, 0);
    Some((bot::steer_along(view, &line), weight))
}

// Junior RC: driving forwards against the track direction (the .pan wrong way too) - the car is turned round after TURN_ROUND_AFTER
pub fn driving_against(track: &Track, place: Option<AiPlace>, view: &BotView, wrong_way: bool) -> bool
{
    let Some(place) = place else { return false; };
    let forward = Vector3::new(view.forward.x, 0.0, view.forward.z);
    wrong_way && view.up.y >= UPRIGHT && view.speed > MIN_TURN_SPEED && track.has_ai() && forward.dot(&track.segment_direction(place.node, place.next)) < -TURN_ROUND_DOT * forward.norm()
}

// the racing line where the car is, facing along the track
pub fn turn_round_spot(track: &Track, place: AiPlace) -> (Vector3<f32>, Vector3<f32>)
{
    let position = track.racing_point(place.node, 0.0).lerp(&track.racing_point(place.next, 0.0), place.t.clamp(0.0, 1.0));
    (position, track.segment_direction(place.node, place.next))
}

// the share across the track at a position (0 left edge, 1 right edge, beyond outside)
fn across(track: &Track, place: AiPlace, position: Vector3<f32>) -> Option<f32>
{
    let left = track.ai_point(place.node, 0.0).lerp(&track.ai_point(place.next, 0.0), place.t);
    let edge = track.ai_point(place.node, 1.0).lerp(&track.ai_point(place.next, 1.0), place.t) - left;
    let width = Vector3::new(edge.x, 0.0, edge.z);
    if width.norm_squared() < 0.01 { return None; }
    let d = position - left;
    Some(Vector3::new(d.x, 0.0, d.z).dot(&width) / width.norm_squared())
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

pub const TURNED_ROUND: &str = "turned round";

// Junior RC: why the car should be put back onto the track and after how many s - None while it drives fine (stuck: the watch waited already)
// against: driving_against - TURNED_ROUND puts it round where it is, the others onto the next node
pub fn lost(track: &Track, place: Option<AiPlace>, up_y: f32, wrong_way: bool, against: bool, stuck: bool) -> Option<(&'static str, f32)>
{
    let off_track = place.is_some_and(|place| place.distance > track.half_width(place.node) + OFF_TRACK);
    if up_y < UPRIGHT { Some(("upside down", RESPAWN_UPSIDE_DOWN)) }
    else if against && !off_track { Some((TURNED_ROUND, TURN_ROUND_AFTER)) }
    else if wrong_way { Some(("the wrong way", RESPAWN_WRONG_WAY)) }
    else if off_track { Some(("off the track", RESPAWN_OFF_TRACK)) }
    else if stuck { Some(("stuck", 0.0)) }
    else { None }
}

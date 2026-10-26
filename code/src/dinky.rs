// the three clockwork cars on the table of the options view (formats/README.md, Frontend): full lock and full throttle,
// both turned round every 3 s or at once after a hard hit - they knock the wobbly cones over

use rustl::prelude::*;
use rustl::state::scene::physics::contacts::ContactTarget;
use rustl::state::scene::scene_controller::vehicle_controller::VehicleDriveInput;
use rustl::state::state::State;

use crate::cars;
use crate::field::{self, Opponent, Slot};
use crate::random::Random;
use crate::race::with_vehicle;

// the cars and their start on the table (grid 2 of the game, game units)
const DINKIES: [(&str, [f32; 3]); 3] = [("wincar", [-1600.0, -250.0, -1100.0]), ("wincar2", [-1700.0, -250.0, -1200.0]), ("wincar3", [-1500.0, -250.0, -1000.0])];
// the top of the table (m) - the grid points are the car centers
const TABLE_TOP: f32 = 2.36;
// s between two turns, m/s of a hit that turns at once (400 units/s in the game)
const TURN_TIME: f32 = 3.0;
const HIT_SPEED: f32 = 4.0;
// own: a car off the table or on its roof this long goes back to its start
const LOST_TIME: f32 = 3.0;
const LOST_HEIGHT: f32 = 1.5;

struct Dinky
{
    opponent: Opponent,
    slot: Slot,
    // -1, 0 (not chosen yet), 1
    steer: f32,
    throttle: f32,
    timer: f32,
    lost: f32,
}

pub struct Dinkies
{
    scene_id: u32,
    cars: Vec<Dinky>,
    random: Random,
}

impl Dinkies
{
    pub fn start(state: &mut State, scene_id: u32, assets: &str) -> Self
    {
        let mut cars = vec![];
        for (i, (car, [x, _, z])) in DINKIES.iter().enumerate()
        {
            let slot = Slot { floor: Vector3::new(x * 0.01, TABLE_TOP, -z * 0.01), heading: -Vector3::z() };
            match field::spawn_extra(state, scene_id, assets, car, &format!("dinky{}", i + 1), slot, Some("clockwrk"))
            {
                Ok(opponent) => cars.push(Dinky { opponent, slot, steer: 0.0, throttle: 0.0, timer: 0.0, lost: 0.0 }),
                Err(error) => { console_warning!("dinky: {} - {}", car, error); },
            }
        }
        console_log!("dinky: {} clockwork cars on the table", cars.len());
        Self { scene_id, cars, random: Random::default() }
    }

    pub fn update(&mut self, state: &mut State, dt: f32)
    {
        let Some(scene) = state.find_scene_by_id_mut(self.scene_id) else { return; };
        let hits: Vec<u32> = scene.physics.contact_events().iter().filter(|contact| contact.started() && contact.impact_speed >= HIT_SPEED)
            .flat_map(|contact| [contact.target, contact.other]).filter_map(|target| match target { ContactTarget::Vehicle { node_id } => Some(node_id), _ => None }).collect();

        for dinky in &mut self.cars
        {
            let id = dinky.opponent.node.read().unwrap().id;
            if hits.contains(&id) && dinky.throttle != 0.0
            {
                dinky.steer = -dinky.steer;
                dinky.throttle = -dinky.throttle;
                dinky.timer = TURN_TIME;
            }
            else if dinky.timer <= 0.0
            {
                dinky.steer = if dinky.steer == 0.0 { if self.random.next() > 0.5 { 1.0 } else { -1.0 } } else { -dinky.steer };
                dinky.throttle = if dinky.throttle == 0.0 { if self.random.next() > 0.5 { 1.0 } else { -1.0 } } else { -dinky.throttle };
                dinky.timer = TURN_TIME;
            }
            dinky.timer -= dt;

            let (position, up) = pose(&dinky.opponent.node);
            dinky.lost = if position.y < LOST_HEIGHT || up.y < 0.3 { dinky.lost + dt } else { 0.0 };
            let drive = VehicleDriveInput { throttle: dinky.throttle.max(0.0), brake: (-dinky.throttle).max(0.0), steer: dinky.steer, handbrake: false, recover: false };
            let slot = dinky.slot;
            let back = dinky.lost > LOST_TIME;
            with_vehicle(scene, id, |vehicle, scene|
            {
                vehicle.input_override = Some(drive);
                if back { vehicle.place(scene, slot.floor + Vector3::y() * (cars::ground_height(vehicle) + cars::LIFT), slot.heading); }
            });
            if back { dinky.lost = 0.0; }
        }
    }

    // Play stops: the cars and their controllers go
    pub fn stop(self, state: &mut State)
    {
        let Some(scene) = state.find_scene_by_id_mut(self.scene_id) else { return; };
        let opponents: Vec<Opponent> = self.cars.into_iter().map(|dinky| dinky.opponent).collect();
        field::remove(scene, &opponents);
    }
}

// position and up axis of a node
fn pose(node: &NodeItem) -> (Vector3<f32>, Vector3<f32>)
{
    let node = node.read().unwrap();
    let Some(transform) = node.find_component::<Transformation>() else { return (Vector3::zeros(), Vector3::y()); };
    component_downcast!(transform, Transformation);
    let data = transform.get_data();
    let up = data.rotation_quat.map_or(Vector3::y(), |q| rustl::nalgebra::UnitQuaternion::new_normalize(rustl::nalgebra::Quaternion::new(q.w, q.x, q.y, q.z)) * Vector3::y());
    (data.position, up)
}

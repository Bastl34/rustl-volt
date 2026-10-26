// cars.scene of the converter: Enter / Backspace switch the driven car, the other cars are rigid bodies to ram
// the editor keeps the scene after Play - everything changed here is put back in stop()

use std::collections::{HashMap, HashSet};

use rustl::prelude::*;
use rustl::helper::option_or_id::OptionOrId;
use rustl::rapier3d::prelude::Vector;
use rustl::serde_json;
use rustl::nalgebra::{Matrix4, Unit, UnitQuaternion};
use rustl::state::scene::components::transformation::TransformationData;
use rustl::state::scene::node::{NodeSettings, PhysicsBodyType, PhysicsSettings, PhysicsShape};
use rustl::state::scene::scene_controller::scene_controller::SceneController;
use rustl::state::scene::components::material::{Material, TextureType};
use rustl::state::scene::components::sound::{Sound, SoundType};
use crate::for_each_node;
use rustl::state::scene::scene_controller::vehicle::engine::EngineType;
use rustl::state::scene::scene_controller::vehicle::engine_sound::EngineSoundLayer;
use rustl::state::scene::scene_controller::vehicle_controller::{VehicleController, VehicleDrive, VehicleType, VehicleWheel};

// marks the controller of this code - a run that ended without stop() left it in the scene
const CONTROLLER_NAME: &str = "Re-Volt car";

// m per unit of the speedometer: TopEnd / TopSpeed is about 88 units/s per mph
const SPEEDO_UNIT: f32 = 0.00508;
// Acc of the hand made cars is 50 - the median of the stock cars instead
const DEFAULT_ACC: f32 = 6.6;
// the motor: one gear, full force up to half of TopEnd, then constant power
const MOTOR_RPM: f32 = 15000.0;
// rpm room above TopEnd: in the air an electric motor of the engine revs to 85 % - with 1.1 it ran at 91 % at TopEnd already and the free rev was not heard
// the force at the wheels does not change with it, torque and gearing follow
const MOTOR_TOP_MARGIN: f32 = 1.5;
const MOTOR_BASE_SHARE: f32 = 0.5;
// the engine loop plays this much faster at TopEnd (the rpm it was recorded at, in motor rpm) - free in the air about 2.3
const ENGINE_SOUND_TOP_PITCH: f32 = 1.8;
const ENGINE_SOUND_RPM: f32 = MOTOR_RPM / MOTOR_TOP_MARGIN / ENGINE_SOUND_TOP_PITCH;
// sound components the converter puts on a car
const ENGINE_SOUND: &str = "Engine";
const SQUEAL_SOUND: &str = "Squeal";
const SCRAPE_SOUND: &str = "Scrape";
// kg per mass unit of the game: the rc becomes 18 kg like the engine preset for its size (1.8 kg made every car spin)
const MASS_UNIT: f32 = 10.0;
const EARTH_GRAVITY: f32 = 9.81;
// m a wheel may stick out of the hull sideways without bumpers
const BUMPER_SIDE_MARGIN: f32 = 0.01;
// wheel radii the wheel rays start above their spring mount (engine: suspension.ray_headroom)
const RAY_HEADROOM: f32 = 1.0;
// highest center of mass above the axles, in wheel radii
const COM_ABOVE_AXLE: f32 = 0.3;
const BODY_FRICTION: f32 = 0.3;
const WALL_SLIDE: f32 = 1.0;
// m above the floor a car is put - it drops onto its wheels
pub const LIFT: f32 = 0.05;

struct Car
{
    node: NodeItem,
    name: String,
    settings: NodeSettings,
    transform: Option<TransformationData>,
}

pub struct Showroom
{
    scene_id: u32,
    cars: Vec<Car>,
    current: Option<usize>,
    // the first game camera as it was: data, controller, node
    camera: Option<(serde_json::Value, Option<serde_json::Value>, OptionOrId<NodeItem>)>,
}

impl Showroom
{
    // None if the scene has no converted cars
    pub fn start(scene: &mut Scene) -> Option<Self>
    {
        remove_controllers(scene);
        // a level scene: its start car has a controller of its own
        if scene.controller.iter().any(|c| c.as_any().is::<VehicleController>())
        {
            return None;
        }

        let mut cars = vec![];
        let mut found = vec![];
        for node in &scene.nodes
        {
            collect_tagged(node, "revolt_car", &mut found);
        }
        for node in found
        {
            let (name, mut settings, transform) =
            {
                let node = node.read().unwrap();
                let transform = node.find_component::<Transformation>().map(|t| { component_downcast!(t, Transformation); *t.get_data() });
                (node.name.clone(), node.settings.clone(), transform)
            };
            // left dynamic by a run without stop() - back to the way the converter writes them
            if settings.physics.body_type != PhysicsBodyType::Static
            {
                console_warning!("cars: '{}' was still a rigid body from an earlier run - reset", name);
                settings.physics = PhysicsSettings::default();
                settings.collision = false;
            }
            cars.push(Car { node, name, settings, transform });
        }
        if cars.is_empty()
        {
            return None;
        }
        cars.sort_by(|a, b| a.name.cmp(&b.name));

        let camera = scene.get_game_camera_mut("").map(|cam|
        {
            let data = serde_json::to_value(cam.get_data()).unwrap_or_default();
            let controller = cam.controller.as_ref().and_then(|c| serde_json::to_value(c).ok());
            (data, controller, cam.node.clone())
        });

        let mut showroom = Self { scene_id: scene.id, cars, current: None, camera };
        for index in 0..showroom.cars.len()
        {
            showroom.make_rigid_body(index);
        }
        showroom.drive(scene, 0);
        console_log!("cars: {} cars - Enter the next one, Backspace the one before", showroom.cars.len());
        Some(showroom)
    }

    pub fn scene_id(&self) -> u32
    {
        self.scene_id
    }

    // the name of the car driven (Parameters.txt), None before the first one
    pub fn current_name(&self) -> Option<String>
    {
        let car = &self.cars[self.current?];
        let node = car.node.read().unwrap();
        Some(node.extras.get::<String>("revolt_name").cloned().unwrap_or_else(|| car.name.clone()))
    }

    // step: 1 the next car, -1 the one before
    pub fn update(&mut self, scene: &mut Scene, step: i32)
    {
        if step != 0
        {
            let count = self.cars.len() as i32;
            let index = self.current.map_or(0, |i| (i as i32 + step).rem_euclid(count) as usize);
            self.drive(scene, index);
        }
    }

    // everything back as it was before Play
    pub fn stop(&mut self, scene: &mut Scene)
    {
        remove_controllers(scene);

        for car in &self.cars
        {
            let mut node = car.node.write().unwrap();
            node.settings = car.settings.clone();
            if let (Some(data), Some(transform)) = (car.transform, node.find_component::<Transformation>())
            {
                component_downcast_mut!(transform, Transformation);
                transform.get_data_mut().set(data);
            }
        }

        if let (Some((data, controller, node)), Some(cam)) = (self.camera.take(), scene.get_game_camera_mut(""))
        {
            // the resolution stays the one of now - saved right after loading (the menu) it was still 0
            if let Ok(mut data) = serde_json::from_value::<rustl::state::scene::camera::CameraData>(data)
            {
                let current = cam.get_data();
                (data.resolution_width, data.resolution_height) = (current.resolution_width, current.resolution_height);
                cam.get_data_mut().set(data);
            }
            cam.controller = controller.and_then(|c| serde_json::from_value(c).ok());
            cam.node = node;
        }

        self.current = None;
        console_log!("cars: stopped, scene restored");
    }

    // the car before becomes a rigid body where it stands, this one gets the vehicle controller
    fn drive(&mut self, scene: &mut Scene, index: usize)
    {
        remove_controllers(scene);
        if let Some(previous) = self.current
        {
            self.make_rigid_body(previous);
        }

        let car = &self.cars[index];
        {
            let mut node = car.node.write().unwrap();
            node.settings.physics = PhysicsSettings::default();
            node.settings.collision = false;
        }

        let mut controller = VehicleController::default();
        controller.get_base_mut().name = CONTROLLER_NAME.to_string();
        if let Some(error) = setup(scene, &mut controller, &car.node, &car.name, "")
        {
            console_error!("cars: '{}' - {}", car.name, error);
            return;
        }

        scene.controller.push(Box::new(controller));
        self.current = Some(index);
    }

    // mass from Parameters.txt over the volume of hull + wheels
    fn make_rigid_body(&mut self, index: usize)
    {
        let car = &self.cars[index];
        let model = find_child(&car.node, "model");
        let mass = model.as_ref().and_then(|m| number(&m.read().unwrap(), "revolt_total_mass")).unwrap_or(1.0) * MASS_UNIT;
        let volume = model.as_ref().and_then(|m| number(&m.read().unwrap(), "revolt_collision_volume")).unwrap_or(0.0);

        let mut node = car.node.write().unwrap();
        node.settings.collision = true;
        node.settings.camera_collision = false;
        node.settings.physics = PhysicsSettings
        {
            body_type: PhysicsBodyType::Dynamic,
            shape: PhysicsShape::ConvexHull,
            combine_children: true,
            density: if volume > 0.0 { mass / volume } else { 1.0 },
            friction: 0.6,
            ..PhysicsSettings::default()
        };
    }
}

// the vehicle controllers of the level scenes the converter wrote for the start car - set up once, saving the scene keeps them
// the own car without direction: its sounds (engine layers, squeal, scrape) stereo - spatial they wandered from ear to ear in every turn behind the chase camera
// the volume is scaled, a spatial sound behind the car was quieter (guessed, OWN_CAR_VOLUME); stereo = false puts it back
const OWN_CAR_VOLUME: f32 = 0.6;

pub fn set_own_car_sounds(car: &NodeItem, stereo: bool)
{
    let (wanted, factor) = if stereo { (SoundType::Stereo, OWN_CAR_VOLUME) } else { (SoundType::Spatial, 1.0 / OWN_CAR_VOLUME) };
    for component in car.read().unwrap().find_components::<Sound>()
    {
        let mut item = component.write().unwrap();
        let Some(sound) = item.as_any_mut().downcast_mut::<Sound>() else { continue; };
        if sound.get_data().sound_type == wanted { continue; }
        {
            let data = sound.get_data_mut().get_mut();
            data.sound_type = wanted;
            data.volume *= factor;
        }
        // the player is made for its type - a new one, playing on if it played
        let Some(source) = sound.sound_source.as_ref().cloned() else { continue; };
        let running = sound.running();
        sound.set_sound_source(source);
        if running { sound.start(); }
    }
}

pub fn setup_start_cars(scene: &mut Scene)
{
    let mut controllers = std::mem::take(&mut scene.controller);
    for controller in &mut controllers
    {
        let Some(vehicle) = controller.as_any_mut().downcast_mut::<VehicleController>() else { continue; };
        let Some(node) = vehicle.node.as_ref().cloned() else { continue; };
        if !node.read().unwrap().tags.contains("revolt_car") { continue; }
        // set up and saved before the sounds came - only the empty slots; the camera of the game unless it was set by hand
        if vehicle.frame.is_some()
        {
            assign_sounds(vehicle, &node);
            if vehicle.camera.distance_auto
            {
                let scale = find_child(&node, "model").and_then(|model| number(&model.read().unwrap(), "revolt_scale")).unwrap_or(0.01);
                apply_camera(vehicle, scale);
            }
            // the file keeps no camera link - without it the camera stayed at the start (toylite saved in the editor)
            let cam_name = vehicle.cam_name.clone();
            if let Some(error) = vehicle.attach_camera(scene, &cam_name) { console_warning!("cars: start car camera - {}", error); }
            // saved before the center hits came (see apply_parameters)
            vehicle.chassis.center_hits = true;
            continue;
        }

        let name = node.read().unwrap().name.clone();
        let cam_name = vehicle.cam_name.clone();
        if let Some(error) = setup(scene, vehicle, &node, &name, &cam_name)
        {
            console_error!("cars: start car '{}' - {}", name, error);
        }
    }
    // controllers added meanwhile stay behind ours
    controllers.append(&mut scene.controller);
    scene.controller = controllers;
}

// auto setup, preset of the engine for the size, then the values of Parameters.txt
pub fn setup(scene: &mut Scene, controller: &mut VehicleController, car: &NodeItem, name: &str, cam_name: &str) -> Option<String>
{
    let model = find_child(car, "model");
    let extra = |key: &str| model.as_ref().and_then(|m| number(&m.read().unwrap(), key));
    controller.vehicle_type = if extra("revolt_class") == Some(0.0) { VehicleType::ElectricCar } else { VehicleType::Car };

    // the setup takes up from how the car lies - one on its roof (pushed over in the showroom) got an upside down frame; the glb up is +y
    let pose = car.read().unwrap().find_component::<Transformation>().map(|t| { component_downcast!(t, Transformation); *t.get_data() });
    if let Some(local) = local_transform(car)
    {
        let scale = Vector3::new(local.column(0).xyz().norm(), local.column(1).xyz().norm(), local.column(2).xyz().norm());
        set_local_transform(car, Matrix4::new_translation(&local.column(3).xyz()) * Matrix4::new_nonuniform_scaling(&scale));
    }
    let result = controller.auto_setup(scene, name, cam_name);
    if let (Some(data), Some(transform)) = (pose, car.read().unwrap().find_component::<Transformation>())
    {
        component_downcast_mut!(transform, Transformation);
        transform.get_data_mut().set(data);
    }
    if let Some(error) = result
    {
        return Some(error);
    }
    add_modelless_wheels(controller, car);
    controller.apply_preset_for_size();
    apply_parameters(controller, car, model.as_ref(), scene_gravity(scene));
    apply_camera(controller, extra("revolt_scale").unwrap_or(0.01));
    controller.reset_chase_camera(scene);
    assign_sounds(controller, car);
    controller.mark_physics_dirty();

    console_log!("cars: driving '{}' ({})", name, model.as_ref().and_then(|m| m.read().unwrap().extras.get::<String>("revolt_name").cloned()).unwrap_or_default());
    None
}

const CAMERA_FULL_LENGTH: f32 = 2.0;

// the ufo flies on its own on top of the car physics (formats/README.md, ufo) - values in units of the game, mass 1
const UFO_RIGHTING: f32 = 20000.0;
const UFO_YAW: f32 = 15000.0;
const UFO_THRUST: f32 = 2000.0;
const UFO_SIDE_SLIP: f32 = 5.0;
const UFO_LIFT: f32 = 0.8;
// the angular resistance scale of the ufo in the air (ResMod) - spin × (1 - AngRes × it × 120 × dt)
const UFO_AIR_RES_MOD: f32 = 40.0;
const RES_STEPS: f32 = 120.0;
struct Ufo
{
    id: u32,
    forward: Vector3<f32>,
    up: Vector3<f32>,
    target: f32,
    steer: f32,
    scale: f32,
    mass: f32,
    engine_rate: f32,
    ang_res: f32,
}

// the voltage of the motor: it follows the input at EngineRate, a change of direction starts from 0
#[derive(Default)]
pub struct Hover
{
    voltage: HashMap<u32, f32>,
}

impl Hover
{
    pub fn update(&mut self, state: &mut State, dt: f32)
    {
        if dt <= 0.0 { return; }
        let mut seen = HashSet::new();
        for scene in state.scenes.iter_mut().filter(|scene| scene.active)
        {
            let gravity = scene_gravity(scene);
            let ufos: Vec<Ufo> = scene.controller.iter().filter_map(|controller| controller.as_any().downcast_ref::<VehicleController>()).filter_map(|vehicle|
            {
                let node = vehicle.node.as_ref()?;
                if !node.read().unwrap().extras.get::<String>("revolt_car").is_some_and(|car| car == "ufo") { return None; }
                let frame = vehicle.frame?;
                let model = find_child(node, "model")?;
                let model = model.read().unwrap();
                let telemetry = vehicle.telemetry();
                // forward input minus backward input - while reversing the engine calls the backward input throttle
                let target = if telemetry.gear < 0 { telemetry.brake - telemetry.throttle } else { telemetry.throttle - telemetry.brake };
                Some(Ufo
                {
                    id: node.read().unwrap().id,
                    forward: frame.forward,
                    up: frame.up,
                    target,
                    steer: telemetry.steer,
                    scale: number(&model, "revolt_scale").unwrap_or(0.01),
                    mass: number(&model, "revolt_body_mass").filter(|m| *m > 0.0).unwrap_or(1.0),
                    engine_rate: number(&model, "revolt_enginerate").unwrap_or(4.5),
                    ang_res: number(&model, "revolt_body_angres").unwrap_or(0.001),
                })
            }).collect();

            for ufo in ufos
            {
                seen.insert(ufo.id);
                let voltage = self.voltage.entry(ufo.id).or_insert(0.0);
                if ufo.target * *voltage < 0.0 { *voltage = 0.0; }
                let step = ufo.engine_rate * dt;
                *voltage += (ufo.target - *voltage).clamp(-step, step);
                let voltage = *voltage;

                let Some((entry, body)) = scene.physics.vehicle_mut(ufo.id) else { continue; };
                let air = !entry.controller.wheels().iter().any(|wheel| wheel.raycast_info().is_in_contact);
                let rotation = *body.rotation();
                let forward = rotation * Vector::new(ufo.forward.x, ufo.forward.y, ufo.forward.z);
                let up = rotation * Vector::new(ufo.up.x, ufo.up.y, ufo.up.z);
                let right = forward.cross(up);
                let velocity = body.linvel();

                // thrust along the look axis (less when tilted), side slip damped, lift of 0.8 g while not rising and upright
                // the lift only in the air - on the ground it took the weight off the tires and the wheels hardly steered
                let mut change = forward * (up.y.abs() * voltage * UFO_THRUST * ufo.scale / ufo.mass * dt) - right * (velocity.dot(right) * UFO_SIDE_SLIP / ufo.mass * dt);
                if air && velocity.y <= 0.0 && up.y > 0.0 { change += up * (UFO_LIFT * gravity * dt); }
                body.set_linvel(velocity + change, true);

                // yaw by the steering and a pull back upright, as angular momentum - the inertia of the body is the one of the game × scale² × this
                let momentum = ufo.scale * ufo.scale * body.mass() / ufo.mass;
                // the yaw only in the air - on the ground the wheels steer, both together turned too much
                let yaw = if air { UFO_YAW } else { 0.0 };
                let torque = up * (ufo.steer * yaw) + up.cross(Vector::Y) * UFO_RIGHTING;
                body.apply_torque_impulse(torque * (momentum * dt), true);

                if air
                {
                    let keep = (1.0 - ufo.ang_res * UFO_AIR_RES_MOD * RES_STEPS * dt).max(0.0);
                    let spin = body.angvel();
                    body.set_angvel(spin * keep, true);
                }
            }
        }
        self.voltage.retain(|id, _| seen.contains(id));
    }
}

// springs and axles reach from their offset to the sprung wheel (stretched) - like the game every frame
pub fn update_suspension(state: &mut State)
{
    for scene in state.scenes.iter()
    {
        let cars: Vec<NodeItem> = scene.controller.iter().filter_map(|controller| controller.as_any().downcast_ref::<VehicleController>()).filter_map(|vehicle| vehicle.node.as_ref().cloned()).collect();
        for car in cars
        {
            let Some(model) = find_child(&car, "model") else { continue; };
            for (side, wheel_name) in SIDE_NAMES.iter().zip(WHEEL_NODES)
            {
                let Some(hub) = find_child(&model, wheel_name).and_then(|wheel| local_transform(&wheel)).map(|t| t.column(3).xyz()) else { continue; };
                for (kind, axis) in [("spring", Vector3::new(0.0, -1.0, 0.0)), ("axle", Vector3::new(0.0, 0.0, -1.0))]
                {
                    if let Some(part) = find_child(&model, &format!("{kind}_{side}")) { aim_part(&part, &hub, &axis); }
                }
            }
        }
    }
}

const SIDE_NAMES: [&str; 4] = ["front_left", "front_right", "back_left", "back_right"];

// a car only to look at (the menu) - its parts move too
pub const SHOWN_CAR_TAG: &str = "revolt_shown_car";
// the wind-up keys of the wincars turn this fast (rad/s, own value - the game turns them at 0.5, one turn in 12.6 s looked like standing still)
const WIND_UP_SPEED: f32 = 4.0;
// panga's head follows the steering (the game: the wheel angle × 1.5) instead of spinning
const HEAD_TURN: f32 = 1.5;
// the aerial: the tip lags behind the acceleration of its foot, the springs of Parameters.txt (Stiffness 1/s², Damping 1/s) pull it back
// own model (the game swings 3 spring points) - the sway is an own value, with the plain values a hard launch bent it about 1°
const AERIAL_SWAY: f32 = 12.0;
const AERIAL_MAX_BEND: f32 = 1.2;
const AERIAL_MAX_ACCEL: f32 = 300.0;
const AERIAL_SUBSTEPS: usize = 4;

struct Spinner
{
    parts: Vec<(NodeItem, Matrix4<f32>)>,
    axis: Vector3<f32>,
    speed: f32,
    angle: f32,
    head: bool,
}

struct Aerial
{
    joints: Vec<(NodeItem, Matrix4<f32>)>,
    // the aerial node: its world frame
    node: NodeItem,
    length: f32,
    stiffness: f32,
    damping: f32,
    // rad about the local x and z axis of the aerial, and their rates
    bend: Vector3<f32>,
    rate: Vector3<f32>,
    // world position and velocity of the foot last frame
    last: Option<(Vector3<f32>, Option<Vector3<f32>>)>,
}

// the moving parts of the driven cars: the spinner turns (wind-up key of the wincars, radar of sgt), the head of panga looks into the turn, the aerial sways
#[derive(Default)]
pub struct Parts
{
    spinners: HashMap<u32, Option<Spinner>>,
    aerials: HashMap<u32, Option<Aerial>>,
}

impl Parts
{
    pub fn update(&mut self, state: &State, dt: f32)
    {
        let mut seen = HashSet::new();
        for scene in state.scenes.iter().filter(|scene| scene.active)
        {
            // the steering of the driven ones - cars only to look at (the menu) steer nothing
            let steering: HashMap<u32, f32> = scene.controller.iter().filter_map(|controller| controller.as_any().downcast_ref::<VehicleController>())
                .filter_map(|vehicle| vehicle.node.as_ref().map(|node| (node.read().unwrap().id, vehicle.telemetry().steer * vehicle.steering.max_angle.to_radians())))
                .collect();
            // every car glb at the top of the scene: its model node carries the car id
            for car in scene.nodes.iter().filter(|node| { let tags = &node.read().unwrap().tags; tags.contains("revolt_car") || tags.contains(SHOWN_CAR_TAG) })
            {
                let Some(model) = find_child(car, "model").filter(|model| model.read().unwrap().extras.get::<String>("revolt_car").is_some()) else { continue; };
                let car = car.clone();
                let id = car.read().unwrap().id;
                seen.insert(id);
                let steer = steering.get(&id).copied().unwrap_or(0.0);

                if let Some(spinner) = self.spinners.entry(id).or_insert_with(|| Spinner::find(&car, &model)).as_mut()
                {
                    spinner.angle = if spinner.head { steer * HEAD_TURN } else { (spinner.angle + spinner.speed * dt) % std::f32::consts::TAU };
                    let rotation = UnitQuaternion::from_axis_angle(&Unit::new_normalize(spinner.axis), spinner.angle).to_homogeneous();
                    for (part, base) in &spinner.parts { set_local_transform(part, base * rotation); }
                }
                if let Some(aerial) = self.aerials.entry(id).or_insert_with(|| Aerial::find(&model)).as_mut()
                {
                    aerial.update(dt);
                }
            }
        }
        self.spinners.retain(|id, _| seen.contains(id));
        self.aerials.retain(|id, _| seen.contains(id));
    }

    // Play stops: the parts as the converter placed them (the editor keeps the start car)
    pub fn stop(&mut self)
    {
        for spinner in self.spinners.drain().filter_map(|(_, spinner)| spinner)
        {
            for (part, base) in spinner.parts { set_local_transform(&part, base); }
        }
        for aerial in self.aerials.drain().filter_map(|(_, aerial)| aerial)
        {
            for (joint, base) in aerial.joints { set_local_transform(&joint, base); }
        }
    }
}

impl Spinner
{
    // the axis and speed of Parameters.txt on the model root (game units: y down, z forward)
    fn find(car: &NodeItem, model: &NodeItem) -> Option<Spinner>
    {
        let mut nodes = vec![];
        collect_named(model, "spinner", &mut nodes);
        let parts: Vec<(NodeItem, Matrix4<f32>)> = nodes.into_iter().filter_map(|node| local_transform(&node).map(|base| (node, base))).collect();
        if parts.is_empty() { return None; }
        let model = model.read().unwrap();
        let get = |key: &str| number(&model, key).unwrap_or(0.0);
        let axis = Vector3::new(get("revolt_spinner_axis_x"), -get("revolt_spinner_axis_y"), -get("revolt_spinner_axis_z"));
        // the model root has the car id also on cars only to look at
        let car_id = model.extras.get::<String>("revolt_car").cloned().or_else(|| car.read().unwrap().extras.get::<String>("revolt_car").cloned()).unwrap_or_default();
        let head = car_id == "panga";
        // the head turns about the up axis, a spinner about its own one
        let axis = if head { Vector3::y() } else { axis };
        if axis.norm() < 1e-4 { return None; }
        let speed = get("revolt_spinner_angvel");
        let speed = if car_id.starts_with("wincar") { WIND_UP_SPEED * speed.signum() } else { speed };
        Some(Spinner { parts, axis, speed, angle: 0.0, head })
    }
}

impl Aerial
{
    fn find(model: &NodeItem) -> Option<Aerial>
    {
        let node = find_child(model, "aerial")?;
        let (length, stiffness, damping, sections) =
        {
            let read = node.read().unwrap();
            let scale = number(&model.read().unwrap(), "revolt_scale").unwrap_or(0.01);
            (number(&read, "revolt_aerial_length")? * scale, number(&read, "revolt_aerial_stiffness")?, number(&read, "revolt_aerial_damping").unwrap_or(0.0), number(&read, "revolt_aerial_sections")? as usize)
        };
        let joints: Vec<(NodeItem, Matrix4<f32>)> = (0..sections).filter_map(|i| find_child(&node, &format!("aerial_joint_{i}"))).filter_map(|joint| local_transform(&joint).map(|base| (joint, base))).collect();
        if joints.is_empty() || length <= 0.0 || stiffness <= 0.0 { return None; }
        Some(Aerial { joints, node, length, stiffness, damping, bend: Vector3::zeros(), rate: Vector3::zeros(), last: None })
    }

    fn update(&mut self, dt: f32)
    {
        if dt <= 0.0 { return; }
        let transform = self.node.read().unwrap().get_full_transform();
        let position = transform.column(3).xyz();
        let velocity = self.last.map(|(last, _)| (position - last) / dt);
        let accel = match (velocity, self.last.and_then(|(_, v)| v)) { (Some(v), Some(before)) => (v - before) / dt, _ => Vector3::zeros() };
        self.last = Some((position, velocity));
        let accel = if accel.norm() > AERIAL_MAX_ACCEL { Vector3::zeros() } else { accel };

        // the acceleration in the frame of the aerial (it points along its +y)
        let axes = [0, 1, 2].map(|i| transform.column(i).xyz().normalize());
        let local = Vector3::new(axes[0].dot(&accel), axes[1].dot(&accel), axes[2].dot(&accel));
        // the tip lags: speeding up along +x bends it about +z towards -x, along +z about -x
        let push = Vector3::new(-local.z, 0.0, local.x) * AERIAL_SWAY / self.length;
        let step = dt / AERIAL_SUBSTEPS as f32;
        for _ in 0..AERIAL_SUBSTEPS
        {
            self.rate += (push - self.bend * self.stiffness - self.rate * self.damping) * step;
            self.bend += self.rate * step;
        }
        if self.bend.norm() > AERIAL_MAX_BEND { self.bend *= AERIAL_MAX_BEND / self.bend.norm(); }

        // the same share of the bend at every joint - a smooth curve to the tip
        let rotation = UnitQuaternion::from_scaled_axis(self.bend / self.joints.len() as f32).to_homogeneous();
        for (joint, base) in &self.joints { set_local_transform(joint, base * rotation); }
    }
}

fn local_transform(node: &NodeItem) -> Option<Matrix4<f32>>
{
    let transformation = node.read().unwrap().find_component::<Transformation>()?;
    component_downcast!(transformation, Transformation);
    Some(*transformation.get_transform())
}

fn set_local_transform(node: &NodeItem, local: Matrix4<f32>)
{
    let Some(transformation) = node.read().unwrap().find_component::<Transformation>() else { return; };
    component_downcast_mut!(transformation, Transformation);
    transformation.set_local_transform(local);
}

// the model axis (glTF) turned towards the hub and stretched along it
fn aim_part(part: &NodeItem, hub: &Vector3<f32>, axis: &Vector3<f32>)
{
    let Some(length) = number(&part.read().unwrap(), "revolt_length_m").filter(|l| *l > 0.0) else { return; };
    let Some(local) = local_transform(part) else { return; };
    let position = local.column(3).xyz();
    let direction = hub - position;
    let distance = direction.norm();
    if distance < 1e-5 { return; }
    let rotation = UnitQuaternion::rotation_between(axis, &direction).unwrap_or_else(|| UnitQuaternion::from_axis_angle(&Vector3::x_axis(), std::f32::consts::PI));
    let stretch = distance / length;
    let scale = axis.map(|a| if a.abs() > 0.5 { stretch } else { 1.0 });
    set_local_transform(part, Matrix4::new_translation(&position) * rotation.to_homogeneous() * Matrix4::new_nonuniform_scaling(&scale));
}

// the pins of rc and flag stood 15 units above the hub - not seen in the game, what PIN Length (-1.35) does is not decoded
pub fn hide_pins(car: &NodeItem)
{
    for side in SIDE_NAMES
    {
        let name = format!("pin_{side}");
        let mut found = vec![];
        collect_named(car, &name, &mut found);
        for pin in found { hide_tree(&pin); }
    }
}

// a mesh of several primitives becomes sibling nodes "<name> primitive_<n>" without a node "<name>"
fn collect_named(node: &NodeItem, name: &str, found: &mut Vec<NodeItem>)
{
    for child in node.read().unwrap().nodes.clone()
    {
        let matches = { let child_name = &child.read().unwrap().name; child_name == name || child_name.strip_prefix(name).is_some_and(|rest| rest.starts_with(" primitive_")) };
        if matches { found.push(child.clone()); }
        collect_named(&child, name, found);
    }
}

// the renderer checks only the node itself - the primitives of a mesh are child nodes
fn hide_tree(node: &NodeItem)
{
    node.write().unwrap().settings.visible = false;
    for child in node.read().unwrap().nodes.clone() { hide_tree(&child); }
}

// ufo and q have wheels without a model - auto setup finds only meshes and then took +z as forward (Re-Volt cars face -z): the wheel nodes become the wheels
fn add_modelless_wheels(controller: &mut VehicleController, car: &NodeItem)
{
    let nodes: Vec<NodeItem> = WHEEL_NODES.iter().filter_map(|name| find_child(car, name)).collect();
    if nodes.len() < WHEEL_NODES.len() || nodes.len() <= controller.wheels.len() { return; }
    let to_car = car.read().unwrap().get_full_transform().try_inverse().unwrap_or_default();
    controller.wheels = nodes.iter().map(|node|
    {
        let node = node.read().unwrap();
        let center = (to_car * node.get_full_transform().column(3)).xyz();
        let mut wheel = VehicleWheel::new(&node.name, center, 0.1);
        wheel.node_uuid = node.uuid.clone();
        wheel.width = 0.05;
        wheel
    }).collect();
    if let Some(frame) = controller.frame.as_mut()
    {
        let front = (controller.wheels[0].center + controller.wheels[1].center) * 0.5;
        let back = controller.wheels[2..].iter().map(|wheel| wheel.center).sum::<Vector3<f32>>() / (controller.wheels.len() - 2).max(1) as f32;
        let forward = (front - back) - frame.up * (front - back).dot(&frame.up);
        if forward.norm() > 0.001 { frame.forward = forward.normalize(); }
    }
    controller.assign_wheel_roles();
    console_log!("cars: {} wheels without a model", controller.wheels.len());
}

const WHEEL_NODES: [&str; 4] = ["wheel_front_left", "wheel_front_right", "wheel_back_left", "wheel_back_right"];

// m from the floor to the origin of a set up car: its lowest wheel point
pub fn ground_height(controller: &VehicleController) -> f32
{
    let up = controller.frame.map_or(Vector3::y(), |frame| frame.up);
    controller.wheels.iter().map(|wheel| wheel.radius - wheel.center.dot(&up)).fold(0.0, f32::max)
}

// the follow camera of the game (the default of its camera table): 460 units behind, 150 up, looking at 60 above the car - the same for every car
fn apply_camera(controller: &mut VehicleController, scale: f32)
{
    // cars longer than 2 m (trolley 3.2 m) get the camera further out, the others keep the values of the game
    let forward = controller.frame.map_or(Vector3::z(), |frame| frame.forward);
    let length = controller.chassis_bounds().map_or(0.0, |(min, max)| (max - min).dot(&forward.abs()));
    let scale = scale * (length / CAMERA_FULL_LENGTH).max(1.0);
    let (back, up, look) = (460.0 * scale, 150.0 * scale, 60.0 * scale);
    let camera = &mut controller.camera;
    camera.distance_auto = false;
    camera.distance = (back * back + (up - look) * (up - look)).sqrt();
    camera.height = look;
    camera.pitch = ((up - look) / back).atan().to_degrees();
}

// the sound components the converter put on the car, by name, onto the empty slots - the motor of the game stays audible at speed
fn assign_sounds(controller: &mut VehicleController, car: &NodeItem)
{
    let components = car.read().unwrap().find_components::<Sound>();
    let by_name = |name: &str| components.iter().find(|component| component.read().unwrap().get_base().name == name).cloned();

    let sound = &mut controller.sound;
    if sound.engine_layers.is_empty()
    {
        if let Some(engine) = by_name(ENGINE_SOUND)
        {
            sound.engine_layers.push(EngineSoundLayer { sound: OptionOrId::Some(engine), rpm: ENGINE_SOUND_RPM, load: 1.0 });
        }
    }
    for (slot, name) in [(&mut sound.squeal, SQUEAL_SOUND), (&mut sound.scrape, SCRAPE_SOUND)]
    {
        if slot.is_none()
        {
            if let Some(component) = by_name(name) { *slot = OptionOrId::Some(component); }
        }
    }
    sound.electric_fade = false;
}

fn scene_gravity(scene: &Scene) -> f32
{
    let gravity = scene.physics.settings.gravity.norm();
    if gravity > 0.01 { gravity } else { EARTH_GRAVITY }
}

// the values of Parameters.txt on top of the preset of the engine for this size
// simulated for all cars (22 m/s², 120 Hz, launch, slopes, full lock at full speed): the preset carries the suspension and tires, the game the drive
// left out on purpose: the masses as they are (1.8 kg spun every car), StaticFriction as grip, MaxPos as travel (3 cm sat on the ground)
fn apply_parameters(controller: &mut VehicleController, car: &NodeItem, model: Option<&NodeItem>, gravity: f32)
{
    let Some(model) = model else { return; };
    let model = model.read().unwrap();
    let scale = number(&model, "revolt_scale").unwrap_or(0.01);
    let to_gltf = |x: f32, y: f32, z: f32| Vector3::new(x, -y, -z) * scale;

    // the preset springs carry 3 times the weight at 9.81 - at 22 only 1.3, a 1 m drop pushed 5 cars through the floor (simulated, 120 Hz needed: mite shook at 60)
    let ratio = gravity / EARTH_GRAVITY;
    controller.suspension.stiffness *= ratio;
    controller.suspension.compression *= ratio.sqrt();
    controller.suspension.relaxation *= ratio.sqrt();

    // wheels: radius, center of the tread (Offset1 + Offset2), powered, turnable
    let ratios: Vec<f32> = controller.wheels.iter().filter_map(|w| find_child(car, &w.node_name))
        .filter(|w| flag(&w.read().unwrap(), "revolt_isturnable") == Some(true))
        .filter_map(|w| number(&w.read().unwrap(), "revolt_steerratio")).collect();
    let max_ratio = ratios.iter().fold(0.0_f32, |m, r| m.max(r.abs())).max(0.001);
    // a wheel turns by SteerRatio × the steering in radians - the preset's 35° gave the ufo (0.25 = 14°) 2.4 times its lock
    if !ratios.is_empty() { controller.steering.max_angle = max_ratio.to_degrees(); }
    for wheel in &mut controller.wheels
    {
        let Some(node) = find_child(car, &wheel.node_name) else { continue; };
        let node = node.read().unwrap();
        let get = |key: &str| number(&node, key).unwrap_or(0.0);

        wheel.radius = get("revolt_radius") * scale;
        // the tire marks - the setup measures the whole model with hub and axle stub (rc 0.29 m, rotor 0.7 m)
        if get("revolt_skidwidth") > 0.0 { wheel.width = get("revolt_skidwidth") * scale; }
        wheel.center = to_gltf(get("revolt_offset1_x") + get("revolt_offset2_x"), get("revolt_offset1_y") + get("revolt_offset2_y"), get("revolt_offset1_z") + get("revolt_offset2_z"));
        wheel.driven = flag(&node, "revolt_ispowered") == Some(true);
        // front -0.5 steers normally, a turnable rear wheel with + steers against it
        wheel.steer = if flag(&node, "revolt_isturnable") == Some(true) { -get("revolt_steerratio") / max_ratio } else { 0.0 };
        wheel.handbrake = !wheel.driven || wheel.steer == 0.0;
    }
    let driven: Vec<bool> = controller.wheels.iter().map(|w| w.driven).collect();
    controller.drive = match (driven.iter().all(|d| *d), controller.wheels.iter().any(|w| w.driven && w.steer != 0.0))
    {
        (true, _) => VehicleDrive::All,
        (false, true) => VehicleDrive::Front,
        _ => VehicleDrive::Rear,
    };

    // the masses of the game × 10 - about the preset mass of this size, the cars to each other and to the bottles as in the game
    if let Some(mass) = number(&model, "revolt_total_mass").filter(|m| *m > 0.0).map(|m| m * MASS_UNIT)
    {
        let factor = mass / controller.chassis.mass.max(0.001);
        controller.chassis.mass = mass;
        controller.brakes.brake_force *= factor;
        controller.brakes.handbrake_force *= factor;
        controller.brakes.rolling_resistance *= factor;
        controller.brakes.air_drag *= factor;
        controller.engine.max_torque *= factor;
    }

    // wheels outside the hull collide as balls, like the wheel spheres of the game - as rays only they sank up to 35 cm into walls and pinned 4 cars there (simulated)
    // above the hull (rotor): with rays only it rolls over the other cars instead of pushing them
    let up = controller.frame.map_or(Vector3::y(), |frame| frame.up);
    let right = controller.frame.map_or(Vector3::x(), |frame| frame.right());
    let hull = controller.chassis_bounds().map(|(min, max)| (min.dot(&up).max(max.dot(&up)), min.dot(&right).abs().max(max.dot(&right).abs())));
    controller.chassis.wheel_bumpers = hull.is_some_and(|(top, side)| controller.wheels.iter().any(|wheel|
        wheel.center.dot(&up) + wheel.radius > top || wheel.center.dot(&right).abs() + wheel.width * 0.5 > side + BUMPER_SIDE_MARGIN));

    // the wheel rays start inside the body: a hard landing (toyeca after the big jump of toylite, 19 m/s) sank the hull 6 cm into the floor, the rays then started below it and found nothing - the car hung in the floor
    controller.suspension.ray_headroom = controller.wheels.iter().map(|wheel| wheel.radius).fold(0.0, f32::max) * RAY_HEADROOM;

    // the inertia of the body (BODY Inertia, mass × units²) - the box of the engine is far too small for wide wheels (rotor jumps around)
    let inertia = [0, 4, 8].map(|i| number(&model, &format!("revolt_body_inertia_{i}")));
    if let ([Some(x), Some(y), Some(z)], Some(body_mass)) = (inertia, number(&model, "revolt_body_mass").filter(|m| *m > 0.0))
    {
        // the wheels add their mass, spread like the body
        let ratio = controller.chassis.mass / body_mass;
        controller.chassis.inertia_auto = false;
        controller.chassis.inertia = Vector3::new(x, y, z) * scale * scale * ratio;
    }

    // CoM counts from the center of the body bounds - from the origin it would lie below the ground for 10 cars
    if let (Some(x), Some(y), Some(z)) = (number(&model, "revolt_com_x"), number(&model, "revolt_com_y"), number(&model, "revolt_com_z"))
    {
        let center = |axis: &str| number(&model, &format!("revolt_body_center_{axis}")).unwrap_or(0.0);
        controller.chassis.center_of_mass_auto = false;
        controller.chassis.center_of_mass = to_gltf(x + center("x"), y + center("y"), z + center("z"));
    }

    // at most a bit above the axles - a wall hits the wheel balls there, from far below the center of mass it rolled the car over (23 of 96 wall hits, then 3 - simulated)
    let axle = controller.wheels.iter().map(|wheel| wheel.center.dot(&up)).sum::<f32>() / controller.wheels.len().max(1) as f32;
    let wheel_radius = controller.wheels.iter().map(|wheel| wheel.radius).fold(0.0, f32::max);
    let above = controller.chassis.center_of_mass.dot(&up) - (axle + wheel_radius * COM_ABOVE_AXLE);
    if above > 0.0
    {
        controller.chassis.center_of_mass -= up * above;
    }

    // the body scrapes along walls - with the default it stuck to them (simulated: 19 instead of 31 of 96 wall hits below 3 m/s)
    controller.chassis.friction = BODY_FRICTION;
    // a curve scraped along an inner wall: the bounce of every touch pushed the car away (simulated with the wall slide: 22 -> 6 push-offs)
    controller.chassis.restitution = 0.0;
    // no roll at walls, the heading turns along them (simulated, 96 wall hits: 8 roll-overs -> 1, 2.2 m -> 0.6 m off the wall 1 s later)
    controller.chassis.wall_slide = WALL_SLIDE;
    // car against car the game pushes through the centers of mass (two balls) from Arcade on - a bump at a rear corner turned the car round
    controller.chassis.center_hits = true;

    // the drive of the game: TopEnd (units/s) as top speed, Acc (m/s² of the speedometer) as launch - the preset made 2-5 m/s² and 0-10° slopes (simulated)
    if let Some(top) = top_end(&model).map(|t| t * scale)
    {
        let acc = number(&model, "revolt_acc").filter(|a| *a > 0.0 && *a < 30.0).unwrap_or(DEFAULT_ACC) * scale / SPEEDO_UNIT;
        let driven: Vec<f32> = controller.wheels.iter().filter(|w| w.driven).map(|w| w.radius).collect();
        let radius = (driven.iter().sum::<f32>() / driven.len().max(1) as f32).max(0.01);
        let top_rpm = top / radius * 60.0 / std::f32::consts::TAU;

        let engine = &mut controller.engine;
        engine.engine_type = EngineType::Electric;
        engine.idle_rpm = 0.0;
        engine.max_rpm = MOTOR_RPM;
        engine.gear_ratios = vec![1.0];
        engine.reverse_ratio = 1.0;
        engine.final_drive = MOTOR_RPM / (top_rpm * MOTOR_TOP_MARGIN);
        engine.peak_torque_rpm = top_rpm * engine.final_drive * MOTOR_BASE_SHARE;
        engine.max_torque = controller.chassis.mass * acc * radius / (engine.final_drive * engine.efficiency);
        engine.top_speed = top * 3.6;
    }
    // the tires grip with the weight - the brakes follow the gravity like the springs
    controller.brakes.brake_force *= ratio;
    controller.brakes.handbrake_force *= ratio;

    // Re-Volt steers fully at top speed - the preset takes 65 % of the lock away at 120 km/h
    controller.steering.high_speed_factor = 1.0;
    // half of SteerRate away from the middle, 4 times back to it or across
    if let Some(rate) = number(&model, "revolt_steerrate")
    {
        controller.steering.speed = rate * 0.5;
        controller.steering.return_speed = rate * 4.0;
    }
    // the ufo slides all the time on 20 % of its weight - the counter steer turned its wheels against every turn
    if car.read().unwrap().extras.get::<String>("revolt_car").is_some_and(|id| id == "ufo") { controller.drift.counter_steer = 0.0; }

    console_log!("cars: mass {:.1} kg, inertia {:?} kg m², top speed {:.0} km/h, steer rate {:.1}, drive {:?}, wheel bumpers {}",
        controller.chassis.mass, controller.chassis.inertia, controller.engine.top_speed, controller.steering.speed, controller.drive, controller.chassis.wheel_bumpers);
}

// takes out the controllers of this code - also the ones a run without stop() left behind
fn remove_controllers(scene: &mut Scene)
{
    let controllers = std::mem::take(&mut scene.controller);
    let (mut ours, others): (Vec<_>, Vec<_>) = controllers.into_iter().partition(|c| c.get_base().name == CONTROLLER_NAME);
    scene.controller = others;
    for controller in &mut ours
    {
        controller.on_remove(scene);
    }
}

fn collect_tagged(node: &NodeItem, tag: &str, found: &mut Vec<NodeItem>)
{
    if node.read().unwrap().tags.contains(tag)
    {
        found.push(node.clone());
        return;
    }
    for child in node.read().unwrap().nodes.clone()
    {
        collect_tagged(&child, tag, found);
    }
}

// the shine of the game: envroll on the body polygons (the `_env` material of the converter), as strong as EnvRGB of Parameters.txt
pub fn add_env_map(state: &mut State, car: &NodeItem, assets: &str)
{
    let Some(model) = find_child(car, "model") else { return; };
    let rgb: f32 = ["x", "y", "z"].iter().filter_map(|axis| number(&model.read().unwrap(), &format!("revolt_envrgb_{axis}"))).sum();
    // added in gamma space by the game, the shader adds linear
    let strength = (rgb / (3.0 * 255.0)).powf(2.2);
    let mut parts = vec![];
    collect_env_parts(car, &mut parts);
    if strength <= 0.0 || parts.is_empty() { return; }

    let path = format!("{assets}/gfx/envroll.png");
    let max_resolution = state.max_texture_resolution();
    let texture = match state.load_texture_or_reuse(&path, Some("png".to_string()), max_resolution)
    {
        Ok(texture) => texture,
        Err(error) => { console_warning!("cars: env map {}: {}", path, error); return; },
    };
    for part in parts
    {
        let part = part.read().unwrap();
        let Some(material) = part.find_component::<Material>() else { continue; };
        component_downcast_mut!(material, Material);
        material.set_texture(texture.clone(), TextureType::Environment);
        material.get_data_mut().get_mut().reflectivity = strength;
    }
}

// the cups of the frontend shine in the game too - their models have no ENV polygons, a fixed env color
const CUP_ENV_COLOR: u32 = 0xc0c0c0;

// the level models with the ENV flag of the .fin (frontend: ufo stand, poster stands, sale bin) and the frontend cups get the env map of the level, strength by their env color
pub fn add_instance_env_maps(state: &mut State, scene_id: u32, assets: &str)
{
    let Some(scene) = state.find_scene_by_id(scene_id) else { return; };
    let level = crate::track::find_node(&scene.nodes, &|node| node.tags.contains("revolt_level") && node.extras.contains("env_map"));
    let map = level.and_then(|level| level.read().unwrap().extras.get::<String>("env_map").cloned()).unwrap_or_else(|| "gfx/envstill.png".to_string());
    let mut instances = vec![];
    for_each_node(&scene.nodes, &mut |node|
    {
        let node_ref = node.read().unwrap();
        let env = number(&node_ref, "revolt_fin_flags").is_some_and(|flags| flags as u32 & 1 != 0);
        let color = node_ref.extras.get::<String>("revolt_env_color").and_then(|hex| u32::from_str_radix(hex, 16).ok());
        if let (true, Some(color)) = (env, color) { instances.push((node.clone(), color)); }
        else if node_ref.tags.contains("revolt_object") && node_ref.tags.contains("cup") { instances.push((node.clone(), CUP_ENV_COLOR)); }
    });
    if instances.is_empty() { return; }

    let path = format!("{assets}/{map}");
    let max_resolution = state.max_texture_resolution();
    let texture = match state.load_texture_or_reuse(&path, Some("png".to_string()), max_resolution)
    {
        Ok(texture) => texture,
        Err(error) => { console_warning!("cars: env map {}: {}", path, error); return; },
    };
    for (node, color) in &instances
    {
        let rgb = ((color >> 16) & 255) + ((color >> 8) & 255) + (color & 255);
        // added in gamma space by the game, the shader adds linear
        let strength = (rgb as f32 / (3.0 * 255.0)).powf(2.2);
        for_each_node(&vec![node.clone()], &mut |part|
        {
            let part = part.read().unwrap();
            let Some(material) = part.find_component::<Material>() else { return; };
            component_downcast_mut!(material, Material);
            material.set_texture(texture.clone(), TextureType::Environment);
            material.get_data_mut().get_mut().reflectivity = strength;
        });
    }
    console_log!("cars: {} level models with the env map {}", instances.len(), map);
}

// MODELRGBPER of the .inf: the game scales the corner colors of every car model by it at load (ship2: 50 %)
pub fn apply_level_shade(scene: &Scene, car: &NodeItem)
{
    let Some(level) = crate::track::find_node(&scene.nodes, &|node| node.tags.contains("revolt_level") && node.extras.contains("model_rgb")) else { return; };
    let Some(share) = number(&level.read().unwrap(), "model_rgb").filter(|share| (share - 1.0).abs() > 0.001) else { return; };
    // scaled in gamma space by the game, the shader works linear
    let factor = share.max(0.0).powf(2.2);
    shade_parts(car, factor);
}

fn shade_parts(node: &NodeItem, factor: f32)
{
    for child in node.read().unwrap().nodes.clone()
    {
        if let Some(material) = child.read().unwrap().find_component::<Material>()
        {
            component_downcast_mut!(material, Material);
            material.get_data_mut().get_mut().base_color *= factor;
        }
        shade_parts(&child, factor);
    }
}

fn collect_env_parts(node: &NodeItem, found: &mut Vec<NodeItem>)
{
    for child in node.read().unwrap().nodes.clone()
    {
        let env = { let child = child.read().unwrap(); child.extras.get::<bool>("revolt_env").copied().unwrap_or(false) && child.find_component::<Material>().is_some() };
        if env { found.push(child.clone()); }
        collect_env_parts(&child, found);
    }
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

// extras of the converter: ints and floats
// TopEnd of Parameters.txt (units/s) - the trolley has 50 (a joke for the frontend bars), every other car about 85-105 × TopSpeed: then from TopSpeed
const TOPEND_PER_TOPSPEED: f32 = 95.0;
pub fn top_end(model: &Node) -> Option<f32>
{
    let top_speed = number(model, "revolt_topspeed").unwrap_or(0.0);
    number(model, "revolt_topend").filter(|t| *t > 0.0).map(|t| if t < top_speed * 10.0 { top_speed * TOPEND_PER_TOPSPEED } else { t })
}

fn number(node: &Node, key: &str) -> Option<f32>
{
    node.extras.get::<f64>(key).map(|v| *v as f32).or_else(|| node.extras.get::<i64>(key).map(|v| *v as f32))
}

fn flag(node: &Node, key: &str) -> Option<bool>
{
    node.extras.get::<bool>(key).copied()
}

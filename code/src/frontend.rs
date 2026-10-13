// the 3D side of the menu in the frontend level (formats/README.md, Frontend): the camera flies between the fixed views of the game,
// the car boxes come out of the stack, the name wheel turns, the big screen shows the track

use std::f32::consts::{FRAC_PI_2, TAU};

use rustl::nalgebra::{Quaternion, Unit, UnitQuaternion};
use rustl::prelude::*;
use rustl::state::scene::camera_controller::camera_controller::CameraControllerBox;
use rustl::state::state::{State, ENGINE_INTERNAL_TAG_PREFX};

use crate::field;
use crate::track::find_node;

pub const SCENE: &str = "frontend";

// game units -> m
const SCALE: f32 = 0.01;

// the views of the menu: camera position and the point it looks at, game units
const VIEWS: [([f32; 3], [f32; 3]); 24] =
[
    ([76.5, -79.3, -4462.6], [109.5, -155.3, -3466.1]),
    ([93.5, -102.9, -1926.7], [110.9, -296.6, -945.7]),
    ([863.5, -1092.5, 512.5], [1491.2, -561.9, 1082.2]),
    ([1355.5, -166.2, 864.4], [2204.6, 360.0, 911.5]),
    ([1589.6, -81.8, -1743.8], [2445.7, -365.5, -1311.8]),
    ([-212.5, -664.6, -706.8], [-212.5, -901.3, 134.4]),
    ([-1773.9, -503.9, -477.2], [-2657.6, -41.1, -546.7]),
    ([-1637.2, -764.6, 206.4], [-2420.9, -224.8, -101.0]),
    ([-1855.2, -733.4, 669.9], [-2503.0, -83.4, 271.3]),
    ([-2007.7, -713.5, 1026.9], [-2575.5, -29.3, 569.3]),
    ([-1171.6, -137.8, 834.4], [-2080.1, -287.1, 444.1]),
    ([-816.2, -534.5, -500.0], [-1591.0, -392.4, -1115.9]),
    ([527.1, -930.5, 1322.3], [1105.5, -375.5, 1922.1]),
    ([-521.2, -2870.0, -325.4], [-306.2, -1894.6, -276.3]),
    ([146.4, -919.2, -206.2], [35.6, 33.0, -78.3]),
    ([-696.8, -493.2, -1728.4], [-1611.2, -510.2, -2133.0]),
    ([2233.9, -507.6, -1393.0], [3125.8, -664.1, -968.6]),
    ([2026.2, -407.2, -1501.5], [2914.3, -579.5, -1075.5]),
    ([-1047.1, -746.2, 1165.8], [-1692.1, -377.2, 1835.1]),
    ([-1047.1, -746.2, 1165.8], [-1692.1, -377.2, 1835.1]),
    ([-1436.8, -612.4, 1353.8], [-1906.3, -354.4, 2198.2]),
    ([-1676.0, -534.9, 1268.1], [-1950.7, -21.6, 2081.1]),
    ([-1052.3, -437.1, 1770.1], [-1926.2, -44.5, 2056.7]),
    ([-1047.1, -746.2, 1165.8], [-1692.1, 0.0, 1835.1]),
];

// the views the menu uses
pub const VIEW_TITLE: usize = 0;
pub const VIEW_MAIN: usize = 1;
pub const VIEW_CAR_BOXES: usize = 2;
pub const VIEW_CAR_FLOOR: usize = 3;
pub const VIEW_TRACK_SCREEN: usize = 4;
pub const VIEW_OPTIONS: usize = 11;
pub const VIEW_NAME: usize = 12;
pub const VIEW_SELECT_RACE: usize = 14;

// s of a flight between two views, the first one from the title longer; eased with sin(pi/2 t)
pub const FLY_TIME: f32 = 0.85;
pub const TITLE_FLY_TIME: f32 = 1.7;
// the lens of the game: 640 on a 640 x 480 screen
const FOVY: f32 = 0.7297;

// the car boxes view moved back along its direction and its look point raised (m) - all shelves in the picture
const CAR_BOXES_BACK: f32 = 2.4;
const CAR_BOXES_LOOK_UP: f32 = 0.9;
// the floor view (the chosen box lands, the car comes out) moved back along its direction (m) - the landing point stays
const CAR_FLOOR_BACK: f32 = 1.2;

// the presented box: this far in front of the car box view, to the right and up (m)
const PRESENT_AHEAD: f32 = 7.5;
const PRESENT_RIGHT: f32 = 1.5;
const PRESENT_UP: f32 = 0.5;
// the chosen box lands this far along the floor view, on the floor (its half height above it)
const LAND_AHEAD: f32 = 2.3;
const BOX_HALF_HEIGHT: f32 = 0.69;
// s on the floor until the car comes out, then until the box flies back
const LAND_TIME: f32 = 1.0;
const BACK_TIME: f32 = 0.3;
// the car comes out where the box landed (the box flies home), turning
const CAR_AHEAD: f32 = 0.0;
const CAR_TURN: f32 = 0.6;
// box movement per axis at 50 frames/s like the game: home (x, y, z), out, rotation share per 1/30 s
const HOME_RATE: [f32; 3] = [0.2, 0.3, 0.2];
const OUT_RATE: [f32; 3] = [0.2, 0.1, 0.2];
const BACK_RATE: [f32; 3] = [0.1, 0.2, 0.1];
const HOME_TURN: f32 = 0.15;
const OUT_TURN: f32 = 0.1;
// a box returning home snaps when this close (m) and turned this close (rad) - the turn is slower, a snap on the distance alone jumped - or after this long (s)
const SNAP_DISTANCE: f32 = 0.002;
const SNAP_ANGLE: f32 = 0.002;
const SNAP_TIME: f32 = 4.0;
// two flying boxes push each other apart below this distance (m)
const PUSH_DISTANCE: f32 = 2.5;
// the select race view sits inside the hanging tv tower: the old renderer looked through its back faces, here it is hidden meanwhile
const TOWER_RADIUS: f32 = 3.0;

// tv static on the big screen when the track changes: s, noise frames per s
const STATIC_TIME: f32 = 0.35;
const STATIC_RATE: f32 = 24.0;

// the name wheel: 30 letters, it turns 1/6 of the way per frame at 60 frames/s
pub const WHEEL_LETTERS: usize = 30;
const WHEEL_SHARE: f32 = 1.0 / 6.0;

// what the menu wants to see this frame
#[derive(Default)]
pub struct Target
{
    pub view: usize,
    // the car id of the box in front of the camera (-1 the plain box)
    pub presented: Option<i32>,
    // the chosen box on the floor and its car folder
    pub landing: Option<(i32, String)>,
    pub letter: usize,
    // the level whose picture the big screen shows
    pub picture: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum BoxState
{
    Home,
    Out,
    Landing,
    Landed,
}

struct CarBox
{
    node: NodeItem,
    car: i32,
    home: (Vector3<f32>, UnitQuaternion<f32>),
    position: Vector3<f32>,
    rotation: UnitQuaternion<f32>,
    state: BoxState,
    // s in the state
    time: f32,
    settled: bool,
}

pub struct Frontend
{
    pub scene_id: u32,
    assets: String,
    camera_id: u32,
    // the scene camera as it was: position, direction, fovy, controller
    camera_restore: Option<(Point3<f32>, Vector3<f32>, f32, Option<CameraControllerBox>)>,
    view: usize,
    from: (Vector3<f32>, Vector3<f32>),
    position: Vector3<f32>,
    direction: Vector3<f32>,
    fly_time: f32,
    fly_duration: f32,
    boxes: Vec<CarBox>,
    wheel: Option<NodeItem>,
    // turns
    wheel_angle: f32,
    screens: Vec<(String, NodeItem)>,
    picture: Option<String>,
    // the noise frames of the big screen, the time left of the static, what the screen shows now (picture, static frame)
    statics: Vec<NodeItem>,
    static_time: f32,
    on_screen: (Option<String>, Option<usize>),
    // the chosen car on the floor
    shown: Option<(String, NodeItem)>,
    shown_turn: f32,
    // the tv tower around the select race view and whether it is hidden
    tower: Vec<NodeItem>,
    tower_hidden: bool,
}

impl Frontend
{
    // the frontend scene: its game camera becomes the menu camera, at the title view
    pub fn start(state: &mut State, scene_id: u32, assets: &str) -> Option<Self>
    {
        let scene = state.find_scene_by_id_mut(scene_id)?;
        let (from_position, from_direction) = view(VIEW_TITLE);
        // the game camera - the editor keeps its own cameras in the list too (internal tag)
        let camera = scene.cameras.iter_mut().find(|camera| !camera.tags.contains_starts_with(ENGINE_INTERNAL_TAG_PREFX))?;
        let (eye, dir, fovy) = { let data = camera.get_data(); (data.eye_pos, data.dir, data.fovy) };
        let restore = (eye, dir, fovy, camera.controller.take());
        camera.get_data_mut().get_mut().fovy = FOVY;
        camera.set_cam_position(Point3::from(from_position), from_direction);
        let camera_id = camera.id;

        let mut boxes = vec![];
        let mut wheel = None;
        let mut screens = vec![];
        let mut statics = vec![];
        let mut tower = vec![];
        let (race_view, _) = view(VIEW_SELECT_RACE);
        walk(&scene.nodes, &mut |node|
        {
            let read = node.read().unwrap();
            if read.name.starts_with("tvstand") || read.name.starts_with("small_screen")
            {
                let near = transform_of(&read).is_some_and(|(position, _)| (position - race_view).xz().norm() < TOWER_RADIUS);
                if near { tower.push(node.clone()); }
            }
            if read.tags.contains("carbox")
            {
                let car = read.extras.get::<i64>("revolt_param_car_id").map_or(-1, |id| *id as i32);
                if let Some((position, rotation)) = transform_of(&read)
                {
                    boxes.push(CarBox { node: node.clone(), car, home: (position, rotation), position, rotation, state: BoxState::Home, time: 0.0, settled: true });
                }
            }
            if read.name == "namewheel" { wheel = Some(node.clone()); }
            if read.name.starts_with("screen_static_") { statics.push(node.clone()); }
            else if let Some(level) = read.name.strip_prefix("screen_") { screens.push((level.to_string(), node.clone())); }
        });
        console_log!("frontend: {} car boxes, name wheel {}, {} track pictures, {} static frames", boxes.len(), if wheel.is_some() { "found" } else { "missing - re-convert the frontend" }, screens.len(), statics.len());

        Some(Self
        {
            scene_id, assets: assets.to_string(), camera_id, camera_restore: Some(restore),
            view: VIEW_TITLE, from: (from_position, from_direction), position: from_position, direction: from_direction, fly_time: 0.0, fly_duration: FLY_TIME,
            boxes, wheel, wheel_angle: 0.0, screens, picture: None, statics, static_time: 0.0, on_screen: (None, None), shown: None, shown_turn: 0.0, tower, tower_hidden: false,
        })
    }

    pub fn update(&mut self, state: &mut State, target: &Target, dt: f32)
    {
        if target.view != self.view
        {
            self.fly_duration = if self.view == VIEW_TITLE { TITLE_FLY_TIME } else { FLY_TIME };
            self.from = (self.position, self.direction);
            self.view = target.view;
            self.fly_time = 0.0;
        }

        // the chosen car comes out of its landed box; the one on the floor goes while the boxes are chosen from and while a box flies down
        let landing_done = self.boxes.iter().any(|b| b.state == BoxState::Landed);
        match &target.landing
        {
            Some((_, car)) if landing_done => self.show_car(state, car),
            Some(_) => self.remove_shown(state),
            None if target.presented.is_some() => self.remove_shown(state),
            None => {},
        }

        let Some(scene) = state.find_scene_by_id_mut(self.scene_id) else { return; };
        self.update_camera(scene, dt);
        self.update_boxes(target, dt);
        self.update_wheel(target.letter, dt);
        self.update_screen(target.picture.as_deref(), dt);
        self.update_shown(dt);
        self.update_tower();
    }

    fn update_tower(&mut self)
    {
        let hide = self.view == VIEW_SELECT_RACE;
        if hide == self.tower_hidden { return; }
        self.tower_hidden = hide;
        for node in &self.tower
        {
            set_visible(node, !hide);
        }
    }

    fn update_camera(&mut self, scene: &mut Scene, dt: f32)
    {
        self.fly_time = (self.fly_time + dt).min(self.fly_duration);
        let share = (FRAC_PI_2 * self.fly_time / self.fly_duration).sin().clamp(0.0, 1.0);
        let (to_position, to_direction) = view(self.view);
        self.position = self.from.0.lerp(&to_position, share);
        let from = Unit::new_normalize(self.from.1);
        let to = Unit::new_normalize(to_direction);
        self.direction = from.try_slerp(&to, share, 1e-6).map_or(to_direction, |d| d.into_inner());

        if let Some(camera) = scene.cameras.iter_mut().find(|camera| camera.id == self.camera_id)
        {
            camera.set_cam_position(Point3::from(self.position), self.direction);
        }
    }

    fn update_boxes(&mut self, target: &Target, dt: f32)
    {
        let (present_position, present_rotation) = present_pose();
        let (land_position, land_rotation) = land_pose();
        let landing = target.landing.as_ref().map(|(car, _)| *car);
        // the box of a car without one: the plain box
        let has_box = |car: i32| self.boxes.iter().any(|b| b.car == car);
        let present = target.presented.map(|car| if has_box(car) { car } else { -1 });
        let land = landing.map(|car| if has_box(car) { car } else { -1 });

        for b in &mut self.boxes
        {
            let next = if Some(b.car) == land
            {
                match b.state { BoxState::Landing | BoxState::Landed => b.state, _ => BoxState::Landing }
            }
            else if Some(b.car) == present { BoxState::Out } else { BoxState::Home };
            if next != b.state
            {
                b.state = next;
                b.time = 0.0;
                b.settled = false;
            }
            b.time += dt;
            if b.state == BoxState::Landing && b.time >= LAND_TIME
            {
                b.state = BoxState::Landed;
                b.time = 0.0;
            }

            match b.state
            {
                BoxState::Home => b.go_home(HOME_RATE, HOME_TURN, dt),
                BoxState::Out => b.move_to(present_position, present_rotation, OUT_RATE, OUT_TURN, dt),
                BoxState::Landing => b.move_to(land_position, land_rotation, OUT_RATE, OUT_TURN, dt),
                BoxState::Landed if b.time > BACK_TIME => b.go_home(BACK_RATE, OUT_TURN, dt),
                BoxState::Landed => {},
            }
        }

        // the box coming out pushes the ones flying home aside - only it, two boxes landing side by side in the stack would fight each other
        if let Some(out) = self.boxes.iter().position(|b| b.state == BoxState::Out)
        {
            let pusher = self.boxes[out].position;
            for (j, other) in self.boxes.iter_mut().enumerate()
            {
                if j == out || other.settled { continue; }
                let delta = other.position - pusher;
                let distance = delta.norm();
                if distance < 1e-3 || distance >= PUSH_DISTANCE { continue; }
                other.position += delta * ((PUSH_DISTANCE - distance) * 0.5 / distance * (dt * 50.0).min(1.0) * 0.1);
            }
        }

        for b in &self.boxes
        {
            set_transform(&b.node, b.position, b.rotation);
        }
    }

    fn update_wheel(&mut self, letter: usize, dt: f32)
    {
        let target = letter as f32 / WHEEL_LETTERS as f32;
        let delta = (target - self.wheel_angle + 0.5).rem_euclid(1.0) - 0.5;
        let share = 1.0 - (1.0 - WHEEL_SHARE).powf(dt * 60.0);
        self.wheel_angle = (self.wheel_angle + delta * share).rem_euclid(1.0);
        if let Some(wheel) = &self.wheel
        {
            let rotation = UnitQuaternion::from_axis_angle(&Vector3::z_axis(), self.wheel_angle * TAU);
            if let Some((position, _)) = transform_of(&wheel.read().unwrap())
            {
                set_transform(wheel, position, rotation);
            }
        }
    }

    // a change of track: static first (not for the first picture), then the new picture
    fn update_screen(&mut self, picture: Option<&str>, dt: f32)
    {
        if self.picture.as_deref() != picture
        {
            if self.picture.is_some() && !self.statics.is_empty() { self.static_time = STATIC_TIME; }
            self.picture = picture.map(str::to_string);
        }
        self.static_time = (self.static_time - dt).max(0.0);
        let wanted = match self.static_time > 0.0
        {
            true => (None, Some(((STATIC_TIME - self.static_time) * STATIC_RATE) as usize % self.statics.len().max(1))),
            false => (self.picture.clone(), None),
        };
        if wanted == self.on_screen { return; }
        if wanted.0 != self.on_screen.0
        {
            for (level, node) in &self.screens
            {
                set_visible(node, wanted.0.as_deref() == Some(level.as_str()));
            }
        }
        for (i, node) in self.statics.iter().enumerate()
        {
            set_visible(node, wanted.1 == Some(i));
        }
        self.on_screen = wanted;
    }

    fn update_shown(&mut self, dt: f32)
    {
        let Some((_, node)) = &self.shown else { return; };
        self.shown_turn = (self.shown_turn + CAR_TURN * dt).rem_euclid(TAU);
        let (land_position, _) = land_pose();
        let (camera, _) = view(VIEW_CAR_FLOOR);
        let toward = horizontal(camera - land_position);
        let position = land_position + toward * CAR_AHEAD - Vector3::y() * BOX_HALF_HEIGHT + Vector3::y() * ground_height(node);
        set_transform(node, position, UnitQuaternion::from_axis_angle(&Vector3::y_axis(), self.shown_turn));
    }

    // the chosen car on the floor - another one replaces it
    fn show_car(&mut self, state: &mut State, car: &str)
    {
        if self.shown.as_ref().is_some_and(|(shown, _)| shown == car) { return; }
        self.remove_shown(state);
        match field::show_car(state, self.scene_id, &self.assets, car)
        {
            Ok(node) =>
            {
                crate::for_each_node(&vec![node.clone()], &mut |part| crate::change_material(part, |m| m.cast_shadow = false));
                self.shown = Some((car.to_string(), node));
            },
            Err(error) => { console_warning!("frontend: {}", error); },
        }
    }

    fn remove_shown(&mut self, state: &mut State)
    {
        if let Some((_, node)) = self.shown.take()
        {
            if let Some(scene) = state.find_scene_by_id_mut(self.scene_id)
            {
                let id = node.read().unwrap().id;
                scene.delete_node_by_id(id, true, false, true, false);
            }
        }
    }

    // back from a race: the unpacked car on the floor goes - the next choice unpacks one again
    pub fn clear_car(&mut self, state: &mut State)
    {
        self.remove_shown(state);
    }

    // straight to a view, no flight (back from a race)
    pub fn jump(&mut self, view_index: usize)
    {
        self.view = view_index;
        let (position, direction) = view(view_index);
        self.from = (position, direction);
        self.fly_time = self.fly_duration;
    }

    // Play stops: the boxes home, the camera as it was
    pub fn stop(&mut self, state: &mut State)
    {
        self.remove_shown(state);
        let Some(scene) = state.find_scene_by_id_mut(self.scene_id) else { return; };
        for b in &self.boxes
        {
            set_transform(&b.node, b.home.0, b.home.1);
        }
        for node in self.screens.iter().map(|(_, node)| node).chain(&self.statics)
        {
            set_visible(node, false);
        }
        for node in &self.tower
        {
            set_visible(node, true);
        }
        if let Some(wheel) = &self.wheel
        {
            if let Some((position, _)) = transform_of(&wheel.read().unwrap())
            {
                set_transform(wheel, position, UnitQuaternion::identity());
            }
        }
        if let (Some((eye, dir, fovy, controller)), Some(camera)) = (self.camera_restore.take(), scene.cameras.iter_mut().find(|camera| camera.id == self.camera_id))
        {
            camera.get_data_mut().get_mut().fovy = fovy;
            camera.set_cam_position(eye, dir);
            camera.controller = controller;
        }
    }
}

impl CarBox
{
    fn move_to(&mut self, position: Vector3<f32>, rotation: UnitQuaternion<f32>, rate: [f32; 3], turn: f32, dt: f32)
    {
        for axis in 0..3
        {
            let share = (dt * 50.0 * rate[axis]).min(1.0);
            self.position[axis] += (position[axis] - self.position[axis]) * share;
        }
        let share = (dt * 30.0 * turn).min(1.0);
        self.rotation = self.rotation.try_slerp(&rotation, share, 1e-6).unwrap_or(rotation);
    }

    fn go_home(&mut self, rate: [f32; 3], turn: f32, dt: f32)
    {
        if self.settled { return; }
        let (position, rotation) = self.home;
        let close = (self.position - position).norm() < SNAP_DISTANCE && self.rotation.angle_to(&rotation) < SNAP_ANGLE;
        if close || self.time > SNAP_TIME
        {
            self.position = position;
            self.rotation = rotation;
            self.settled = true;
            return;
        }
        self.move_to(position, rotation, rate, turn, dt);
    }
}

// position and direction of a view in m, glTF axes - the car boxes view of the game cut the top shelf: own, further back and up
fn view(index: usize) -> (Vector3<f32>, Vector3<f32>)
{
    let (mut position, mut look) = game_view(index);
    let back = (position - look).normalize();
    if index == VIEW_CAR_BOXES
    {
        position += back * CAR_BOXES_BACK;
        look += Vector3::y() * CAR_BOXES_LOOK_UP;
    }
    if index == VIEW_CAR_FLOOR
    {
        position += back * CAR_FLOOR_BACK;
    }
    (position, look - position)
}

// position and look point of a view of the game in m
fn game_view(index: usize) -> (Vector3<f32>, Vector3<f32>)
{
    let (position, look) = VIEWS[index.min(VIEWS.len() - 1)];
    (game(position), game(look))
}

fn game([x, y, z]: [f32; 3]) -> Vector3<f32>
{
    Vector3::new(x, -y, -z) * SCALE
}

fn horizontal(v: Vector3<f32>) -> Vector3<f32>
{
    Vector3::new(v.x, 0.0, v.z).try_normalize(1e-6).unwrap_or(Vector3::z())
}

// the box in front of the car box view, its picture to the camera: local -z along the view, local y its up
fn present_pose() -> (Vector3<f32>, UnitQuaternion<f32>)
{
    let (position, direction) = view(VIEW_CAR_BOXES);
    let forward = direction.normalize();
    let right = forward.cross(&Vector3::y()).normalize();
    let up = right.cross(&forward);
    (position + forward * PRESENT_AHEAD + right * PRESENT_RIGHT + up * PRESENT_UP, UnitQuaternion::face_towards(&-forward, &up))
}

// the box on the floor in front of the floor view, upright, its picture to the camera
fn land_pose() -> (Vector3<f32>, UnitQuaternion<f32>)
{
    let (position, look) = game_view(VIEW_CAR_FLOOR);
    let forward = (look - position).normalize();
    let floor = position + forward * LAND_AHEAD;
    (Vector3::new(floor.x, BOX_HALF_HEIGHT, floor.z), UnitQuaternion::face_towards(&-horizontal(forward), &Vector3::y()))
}

// m from the origin of a car glb down to the bottom of its lowest wheel
fn ground_height(car: &NodeItem) -> f32
{
    let Some(model) = find_node(&car.read().unwrap().nodes, &|node| node.name == "model").or_else(|| Some(car.clone())) else { return 0.0; };
    let model = model.read().unwrap();
    let scale = model.extras.get::<f64>("revolt_scale").map_or(SCALE, |s| *s as f32);
    model.nodes.iter().filter_map(|node|
    {
        let node = node.read().unwrap();
        if !node.name.starts_with("wheel_") { return None; }
        let radius = node.extras.get::<f64>("revolt_radius").map(|r| *r as f32).or_else(|| node.extras.get::<i64>("revolt_radius").map(|r| *r as f32))?;
        let (position, _) = transform_of(&node)?;
        Some(radius * scale - position.y)
    }).fold(0.0, f32::max)
}

fn transform_of(node: &Node) -> Option<(Vector3<f32>, UnitQuaternion<f32>)>
{
    let transform = node.find_component::<Transformation>()?;
    component_downcast!(transform, Transformation);
    let data = transform.get_data();
    let rotation = data.rotation_quat.map_or(UnitQuaternion::identity(), |q| UnitQuaternion::new_normalize(Quaternion::new(q.w, q.x, q.y, q.z)));
    Some((data.position, rotation))
}

fn set_transform(node: &NodeItem, position: Vector3<f32>, rotation: UnitQuaternion<f32>)
{
    let node = node.read().unwrap();
    let Some(transform) = node.find_component::<Transformation>() else { return; };
    component_downcast_mut!(transform, Transformation);
    let data = transform.get_data_mut().get_mut();
    data.position = position;
    data.rotation = Vector3::zeros();
    let q = rotation.quaternion();
    data.rotation_quat = Some(Vector4::new(q.i, q.j, q.k, q.w));
    transform.calc_transform();
}

fn set_visible(node: &NodeItem, visible: bool)
{
    walk(&vec![node.clone()], &mut |part| part.write().unwrap().settings.visible = visible);
}

fn walk(nodes: &Vec<NodeItem>, f: &mut dyn FnMut(&NodeItem))
{
    for node in nodes
    {
        f(node);
        let children = node.read().unwrap().nodes.clone();
        walk(&children, f);
    }
}

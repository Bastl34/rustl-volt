// the weather of the levels (formats/README.md "Weather"): the distance fog of the .inf, the fog box (the mine of wild_west2),
// the rain of ship2 (streaks that fall in front of each camera, a splash where they land) and its lightning (a flash from above)

use std::sync::{Arc, RwLock};

use rustl::helper::option_or_id::OptionOrId;
use rustl::prelude::*;
use rustl::rapier3d::prelude::{QueryFilter, Ray, Vector};
use rustl::state::resources::mesh_resource::{MeshResource, MeshResourceItem};
use rustl::state::resources::texture::TextureItem;
use rustl::state::scene::components::mesh::Mesh;
use rustl::state::scene::instance::Instance;
use rustl::state::scene::node::InstanceItemArc;
use rustl::state::state::ENGINE_INTERNAL_TAG_PREFX;

use crate::object_type::ObjectType;
use crate::random::Random;

// game units per m
const UNITS: f32 = 100.0;

// drops over all cameras of a scene, m/s straight down, the streak is the way of this many s behind the drop
const DROPS: usize = 380;
const DROP_SPEED: f32 = 15.0;
const STREAK_TIME: f32 = 0.05;
// linear gray of the head, the tail fades to nothing
const DROP_GRAY: f32 = 0.12;
// m in front of the camera a drop starts (at the top edge of the view), across twice the width of the view
const DROP_DEPTH: [f32; 2] = [0.5, 30.0];
const DROP_WIDE: f32 = 2.0;
// half width of a streak per m of distance (about a pixel at 480 lines)
const STREAK_WIDTH: f32 = 1.0 / 480.0;
// m above and below the start the floor is looked for - without a floor the drop ends this far below the camera
const FLOOR_SEARCH: f32 = 75.0;
const NO_FLOOR: f32 = 10.0;
// s a drop waits when the floor above it is over the camera (a roof - no rain under it)
const COVERED_WAIT: f32 = 1.0;
// the splash: s, frames of 16 px on the page from this texel, half size in m, additive gray
const SPLASH_TIME: f32 = 0.25;
const SPLASH_FRAMES: usize = 4;
const SPLASH_TILE: [f32; 2] = [144.0, 240.0];
const SPLASH_SIZE: f32 = 0.08;
const SPLASH_GRAY: f32 = 0.35;
// m above the floor
const SPLASH_LIFT: f32 = 0.01;

// lightning: s between flashes, s of a flash, its pulses (half a sine each) - the HUD lays it over the screen
const FLASH_WAIT: [f32; 2] = [10.0, 40.0];
const FLASH_TIME: [f32; 2] = [0.2, 0.5];
const FLASH_PULSE: f32 = 0.15;

enum RainDrop
{
    Waiting(f32),
    Falling { camera: u32, position: Vector3<f32>, floor: f32, splash: bool },
    Splash { position: Vector3<f32>, age: f32 },
}

// one mesh in world space rebuilt every frame
struct Strip
{
    node: NodeItem,
    mesh: MeshResourceItem,
    instance: InstanceItemArc,
}

struct Rain
{
    drops: Vec<RainDrop>,
    streaks: Option<Strip>,
    splashes: Option<Strip>,
    texture: Option<Option<TextureItem>>,
}

struct Storm
{
    // s left of the wait or the flash
    left: f32,
    flash: Option<f32>,
}

struct FogBox
{
    min: Vector3<f32>,
    max: Vector3<f32>,
}

struct SceneWeather
{
    scene_id: u32,
    rain: Option<Rain>,
    storm: Option<Storm>,
    boxes: Vec<FogBox>,
    // the sky box and the clouds - hidden in a fog box
    sky: Vec<NodeItem>,
    sky_hidden: bool,
}

// the fog settings of the project before the levels took over
struct Saved
{
    fog: bool,
    color: Vector3<f32>,
    start: f32,
    end: f32,
}

#[derive(Default)]
pub struct Weather
{
    scenes: Vec<SceneWeather>,
    saved: Option<Saved>,
    random: Random,
    // the lightning of this frame 0..1
    flash: f32,
}

impl Weather
{
    pub fn add_scene(&mut self, scene: &Scene)
    {
        if self.scenes.iter().any(|weather| weather.scene_id == scene.id) { return; }
        let mut weather = SceneWeather { scene_id: scene.id, rain: None, storm: None, boxes: vec![], sky: vec![], sky_hidden: false };
        crate::for_each_node(&scene.nodes, &mut |node|
        {
            let read = node.read().unwrap();
            if read.tags.contains("revolt_skybox") { weather.sky.push(node.clone()); }
            if !read.tags.contains("revolt_object") { return; }
            let param = |key: &str| read.extras.get::<i64>(key).copied().unwrap_or(0) as f32;
            match read.extras.get::<i64>("revolt_object_type").and_then(|id| ObjectType::from_id(*id))
            {
                Some(ObjectType::Rain) if weather.rain.is_none() =>
                {
                    // spread over the first half s like the game
                    let drops = (0..DROPS).map(|i| RainDrop::Waiting(i as f32 * 0.5 / DROPS as f32)).collect();
                    weather.rain = Some(Rain { drops, streaks: None, splashes: None, texture: None });
                },
                Some(ObjectType::Lightning) if weather.storm.is_none() =>
                {
                    let wait = FLASH_WAIT[0] + self.random.next() * (FLASH_WAIT[1] - FLASH_WAIT[0]);
                    weather.storm = Some(Storm { left: wait, flash: None });
                },
                Some(ObjectType::FogBox) =>
                {
                    let half = Vector3::new(param("revolt_param_x_range"), param("revolt_param_y_range"), param("revolt_param_z_range")) / UNITS;
                    let center = read.get_full_transform().column(3).xyz();
                    weather.boxes.push(FogBox { min: center - half, max: center + half });
                },
                _ => {},
            }
        });
        console_log!("weather: scene '{}': rain {}, lightning {}, {} fog boxes", scene.name, weather.rain.is_some(), weather.storm.is_some(), weather.boxes.len());
        self.scenes.push(weather);
    }

    // the scene is gone, its nodes with it
    pub fn remove_scene(&mut self, scene_id: u32)
    {
        self.scenes.retain(|weather| weather.scene_id != scene_id);
    }

    // back in the editor: the nodes go, the sky comes back, the fog of the project again
    pub fn stop(&mut self, state: &mut State)
    {
        for weather in self.scenes.drain(..)
        {
            for node in &weather.sky { node.write().unwrap().settings.visible = true; }
            let Some(scene) = state.find_scene_by_id_mut(weather.scene_id) else { continue; };
            let Some(rain) = weather.rain else { continue; };
            for strip in [rain.streaks, rain.splashes].into_iter().flatten()
            {
                let id = strip.node.read().unwrap().id;
                scene.delete_node_by_id(id, true, false, true, false);
            }
        }
        self.flash = 0.0;
        self.restore_fog(state);
    }

    // the lightning of this frame 0..1
    pub fn flash(&self) -> f32
    {
        self.flash
    }

    // dt 0 while paused (the streaks still turn to the camera)
    pub fn update(&mut self, state: &mut State, dt: f32)
    {
        self.update_fog(state);

        let mut flash = 0.0f32;
        for weather in &mut self.scenes
        {
            let Some(active) = state.find_scene_by_id(weather.scene_id).map(|scene| scene.active) else { continue; };
            if !active { continue; }
            if let Some(storm) = weather.storm.as_mut() { flash = flash.max(storm.update(&mut self.random, dt)); }
            if let Some(rain) = weather.rain.as_mut()
            {
                if rain.texture.is_none()
                {
                    let path = state.find_scene_by_id(weather.scene_id).map(|scene| format!("{}/{}", crate::assets_folder(scene), crate::particles::TEXTURE_FILE));
                    let max_resolution = state.max_texture_resolution();
                    rain.texture = Some(path.and_then(|path| state.load_texture_or_reuse(&path, Some("png".to_string()), max_resolution).map_err(|error| { console_warning!("weather: {}: {} - plain splashes", path, error); }).ok()));
                }
                let Some(scene) = state.find_scene_by_id_mut(weather.scene_id) else { continue; };
                rain.update(scene, &mut self.random, dt);
                rain.draw(scene);
            }
        }
        self.flash = flash;
    }

    // the fog of the first active level from its .inf, black in a fog box - the sky goes there too
    // own: only in the levels with weather (rain, lightning, fog box) - the game has it everywhere (user wish)
    fn update_fog(&mut self, state: &mut State)
    {
        let level = state.scenes.iter().filter(|scene| scene.active).find_map(|scene|
        {
            let special = self.scenes.iter().any(|weather| weather.scene_id == scene.id && (weather.rain.is_some() || weather.storm.is_some() || !weather.boxes.is_empty()));
            if !special { return None; }
            let node = crate::track::find_node(&scene.nodes, &|node| node.tags.contains("revolt_level") && node.extras.contains("far_clip"))?;
            let read = node.read().unwrap();
            let number = |key: &str| read.extras.get::<f64>(key).map(|v| *v as f32).unwrap_or(0.0);
            let color = Vector3::new(number("fog_color_r"), number("fog_color_g"), number("fog_color_b"));
            Some((scene.id, color, number("fog_start"), number("far_clip"), eyes(scene)))
        });
        let Some((scene_id, mut color, start, end, eyes)) = level else
        {
            self.restore_fog(state);
            return;
        };

        // every camera in a box: the game does it per camera
        for weather in &mut self.scenes
        {
            let inside = weather.scene_id == scene_id && !eyes.is_empty() && eyes.iter().all(|(_, eye)| weather.boxes.iter().any(|b| (0..3).all(|a| eye[a] >= b.min[a] && eye[a] <= b.max[a])));
            if inside { color = Vector3::zeros(); }
            if inside != weather.sky_hidden
            {
                weather.sky_hidden = inside;
                for node in &weather.sky { node.write().unwrap().settings.visible = !inside; }
            }
        }

        let rendering = &mut state.rendering;
        if self.saved.is_none()
        {
            self.saved = Some(Saved { fog: rendering.fog, color: rendering.fog_color, start: rendering.fog_start, end: rendering.fog_end });
        }
        rendering.fog = true;
        rendering.fog_color = color;
        rendering.fog_start = start;
        rendering.fog_end = end;
    }

    fn restore_fog(&mut self, state: &mut State)
    {
        let Some(saved) = self.saved.take() else { return; };
        let rendering = &mut state.rendering;
        rendering.fog = saved.fog;
        rendering.fog_color = saved.color;
        rendering.fog_start = saved.start;
        rendering.fog_end = saved.end;
    }
}

impl Storm
{
    // the flash of this frame 0..1
    fn update(&mut self, random: &mut Random, dt: f32) -> f32
    {
        self.left -= dt;
        match self.flash
        {
            None =>
            {
                if self.left <= 0.0
                {
                    let time = FLASH_TIME[0] + random.next() * (FLASH_TIME[1] - FLASH_TIME[0]);
                    self.left = time;
                    self.flash = Some(time);
                }
                0.0
            },
            Some(time) =>
            {
                if self.left <= 0.0
                {
                    self.left = FLASH_WAIT[0] + random.next() * (FLASH_WAIT[1] - FLASH_WAIT[0]);
                    self.flash = None;
                    return 0.0;
                }
                let into = (time - self.left).rem_euclid(FLASH_PULSE);
                (into / FLASH_PULSE * std::f32::consts::PI).sin()
            },
        }
    }
}

// a camera of the players: its id and eye
fn eyes(scene: &Scene) -> Vec<(u32, Vector3<f32>)>
{
    scene.cameras.iter().filter(|camera| camera.enabled && !camera.tags.contains_starts_with(ENGINE_INTERNAL_TAG_PREFX)).map(|camera| (camera.id, camera.get_data().eye_pos.coords)).collect()
}

// eye, look, right, up and the half view (tan) across and up of a camera
struct View
{
    eye: Vector3<f32>,
    look: Vector3<f32>,
    right: Vector3<f32>,
    up: Vector3<f32>,
    tan: [f32; 2],
}

impl View
{
    fn of(scene: &Scene, id: u32) -> Option<View>
    {
        let camera = scene.cameras.iter().find(|camera| camera.id == id && camera.enabled)?;
        let data = camera.get_data();
        let look = data.dir.try_normalize(1e-6)?;
        let right = look.cross(&data.up).try_normalize(1e-6)?;
        let up = right.cross(&look);
        let tan = [1.0 / data.projection[(0, 0)].abs().max(1e-3), 1.0 / data.projection[(1, 1)].abs().max(1e-3)];
        Some(View { eye: data.eye_pos.coords, look, right, up, tan })
    }

    // in front, not too far, not below the view yet
    fn sees(&self, point: Vector3<f32>) -> bool
    {
        let d = point - self.eye;
        let depth = d.dot(&self.look);
        depth >= DROP_DEPTH[0] * 0.5 && depth <= DROP_DEPTH[1] * 1.1 && d.dot(&self.up) >= -self.tan[1] * depth * 1.2
    }
}

impl Rain
{
    fn update(&mut self, scene: &Scene, random: &mut Random, dt: f32)
    {
        if dt <= 0.0 { return; }
        let cameras: Vec<u32> = eyes(scene).into_iter().map(|(id, _)| id).collect();
        if cameras.is_empty() { return; }
        let views: Vec<(u32, View)> = cameras.iter().filter_map(|id| View::of(scene, *id).map(|view| (*id, view))).collect();
        for (number, drop) in self.drops.iter_mut().enumerate()
        {
            match drop
            {
                RainDrop::Waiting(left) =>
                {
                    *left -= dt;
                    if *left > 0.0 { continue; }
                    // split screen: the drops shared out over the cameras
                    let camera = cameras[number % cameras.len()];
                    let Some((_, view)) = views.iter().find(|(id, _)| *id == camera) else { continue; };
                    *drop = spawn(scene, view, camera, random);
                },
                RainDrop::Falling { camera, position, floor, splash } =>
                {
                    position.y -= DROP_SPEED * dt;
                    let seen = views.iter().find(|(id, _)| *id == *camera).is_some_and(|(_, view)| view.sees(*position));
                    if position.y <= *floor
                    {
                        *drop = if *splash { RainDrop::Splash { position: Vector3::new(position.x, *floor + SPLASH_LIFT, position.z), age: 0.0 } } else { RainDrop::Waiting(0.0) };
                    }
                    else if !seen
                    {
                        *drop = RainDrop::Waiting(0.0);
                    }
                },
                RainDrop::Splash { age, .. } =>
                {
                    *age += dt;
                    if *age >= SPLASH_TIME { *drop = RainDrop::Waiting(0.0); }
                },
            }
        }
    }

    // the streaks as quads facing their camera (alpha blended, fading to the tail), the splashes as quads with the frames of the page
    fn draw(&mut self, scene: &mut Scene)
    {
        let eyes = eyes(scene);
        let nearest = |point: Vector3<f32>| eyes.iter().map(|(_, eye)| *eye).min_by(|a, b| (a - point).norm_squared().total_cmp(&(b - point).norm_squared())).unwrap_or(point + Vector3::z());

        let mut streaks = Geometry::default();
        let mut splashes = Geometry::default();
        for drop in &self.drops
        {
            match drop
            {
                RainDrop::Falling { camera, position, .. } =>
                {
                    let eye = eyes.iter().find(|(id, _)| id == camera).map_or_else(|| nearest(*position), |(_, eye)| *eye);
                    let tail = position + Vector3::y() * DROP_SPEED * STREAK_TIME;
                    let side = Vector3::y().cross(&(position - eye)).try_normalize(1e-6).unwrap_or_else(Vector3::x) * (position - eye).norm() * STREAK_WIDTH;
                    let corners = [(tail - side, 0.0), (*position - side, 1.0), (*position + side, 1.0), (tail + side, 0.0)];
                    streaks.quad(eye, corners.map(|(point, alpha)| (point, [0.0, 0.0], [DROP_GRAY, DROP_GRAY, DROP_GRAY, alpha])));
                },
                RainDrop::Splash { position, age } =>
                {
                    let eye = nearest(*position);
                    let facing = (eye - position).try_normalize(1e-6).unwrap_or_else(Vector3::z);
                    let right = Vector3::y().cross(&facing).try_normalize(1e-6).unwrap_or_else(Vector3::x) * SPLASH_SIZE;
                    let up = facing.cross(&right);
                    let frame = ((age / SPLASH_TIME * SPLASH_FRAMES as f32) as usize).min(SPLASH_FRAMES - 1);
                    let u = (SPLASH_TILE[0] + frame as f32 * 16.0 + 0.5) / 256.0;
                    let v = (SPLASH_TILE[1] + 0.5) / 256.0;
                    let size = 15.0 / 256.0;
                    // the vertex buffer flips v (glTF convention) - stored as 1 - v
                    let corners = [(-1.0, -1.0, 0.0, 1.0), (1.0, -1.0, 1.0, 1.0), (1.0, 1.0, 1.0, 0.0), (-1.0, 1.0, 0.0, 0.0)];
                    let color = [SPLASH_GRAY, SPLASH_GRAY, SPLASH_GRAY, 1.0];
                    splashes.quad(eye, corners.map(|(x, y, s, t)| (position + right * x + up * y, [u + s * size, 1.0 - (v + t * size)], color)));
                },
                RainDrop::Waiting(_) => {},
            }
        }

        let texture = self.texture.clone().flatten();
        for (strip, geometry, name, additive, texture) in [(&mut self.streaks, streaks, "rain", false, None), (&mut self.splashes, splashes, "rain splashes", true, texture)]
        {
            if geometry.indices.is_empty() && strip.is_none() { continue; }
            let strip = strip.get_or_insert_with(|| Strip::new(scene, name, texture, additive));
            strip.instance.write().unwrap().get_data_mut().get_mut().visible = !geometry.indices.is_empty();
            if geometry.indices.is_empty() { continue; }
            let mut mesh = strip.mesh.write().unwrap();
            let data = mesh.get_data_mut().get_mut();
            data.vertices = geometry.vertices;
            data.normals = geometry.normals;
            data.uvs_0 = geometry.uvs;
            data.colors = geometry.colors;
            data.uv_indices = geometry.indices.clone();
            data.normals_indices = geometry.indices.clone();
            data.indices = geometry.indices;
        }
    }
}

// a new drop at the top edge of the view at a random depth - waiting when a roof is above it
fn spawn(scene: &Scene, view: &View, camera: u32, random: &mut Random) -> RainDrop
{
    let depth = DROP_DEPTH[0] + random.next() * (DROP_DEPTH[1] - DROP_DEPTH[0]);
    let across = (random.next() * 2.0 - 1.0) * view.tan[0] * DROP_WIDE * depth;
    let position = view.eye + view.look * depth + view.right * across + view.up * view.tan[1] * depth;

    // the first surface from high above
    let top = position + Vector3::y() * FLOOR_SEARCH;
    let ray = Ray::new(Vector::new(top.x, top.y, top.z), Vector::new(0.0, -1.0, 0.0));
    let hit = scene.physics.query_pipeline(QueryFilter::only_fixed().exclude_sensors()).cast_ray(&ray, FLOOR_SEARCH * 2.0, true).map(|(_, toi)| top.y - toi);
    match hit
    {
        Some(floor) if floor > view.eye.y => RainDrop::Waiting(COVERED_WAIT),
        Some(floor) => RainDrop::Falling { camera, position, floor, splash: true },
        None => RainDrop::Falling { camera, position, floor: view.eye.y - NO_FLOOR, splash: false },
    }
}

#[derive(Default)]
struct Geometry
{
    vertices: Vec<Point3<f32>>,
    normals: Vec<Vector3<f32>>,
    uvs: Vec<Point2<f32>>,
    colors: Vec<[f32; 4]>,
    indices: Vec<[u32; 3]>,
}

impl Geometry
{
    fn quad(&mut self, eye: Vector3<f32>, corners: [(Vector3<f32>, [f32; 2], [f32; 4]); 4])
    {
        let base = self.vertices.len() as u32;
        for (position, uv, color) in corners
        {
            self.vertices.push(Point3::from(position));
            self.normals.push((eye - position).try_normalize(1e-6).unwrap_or_else(Vector3::y));
            self.uvs.push(Point2::new(uv[0], uv[1]));
            self.colors.push(color);
        }
        self.indices.extend([[base, base + 1, base + 2], [base, base + 2, base + 3]]);
    }
}

impl Strip
{
    // like the trails of the particles: its bounds would be stale, so no culling
    fn new(scene: &mut Scene, name: &str, texture: Option<TextureItem>, additive: bool) -> Self
    {
        let resource: MeshResourceItem = Arc::new(RwLock::new(Box::new(MeshResource::new_plane(name, Point3::new(-0.5, 0.0, 0.0), Point3::new(0.5, 0.0, 0.0), Point3::new(0.5, 0.0, -1.0), Point3::new(-0.5, 0.0, -1.0)))));
        let mut mesh = Mesh::new(name);
        mesh.mesh_resource = OptionOrId::Some(resource.clone());
        let material = crate::particles::material(scene, name, texture, additive, true);

        let node = Node::new(name);
        {
            let mut write = node.write().unwrap();
            write.add_component(Arc::new(RwLock::new(Box::new(mesh))));
            write.add_component(material);
            crate::particles::settings(&mut write.settings);
        }
        let transform = Transformation::new("Transform", Vector3::zeros(), Vector3::zeros(), Vector3::new(1.0, 1.0, 1.0));
        let mut instance = Instance::new_with_transform(name.to_string(), node.clone(), transform);
        instance.pickable = false;
        {
            let data = instance.get_data_mut().get_mut();
            data.collision = false;
            data.visible = false;
        }
        let instance = node.write().unwrap().add_instance(Box::new(instance));
        scene.add_node(node.clone());
        Self { node, mesh: resource, instance }
    }
}

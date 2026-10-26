// the lightning of the game (formats/README.md "Lightning") - the arc of battle tag and the electro pulse
// a bezier bowed upwards, its points pushed along random directions (the pattern changes now and then, the push every frame),
// one strip facing the camera with the texture of a lightning model; several arcs share one mesh

use std::sync::{Arc, RwLock};

use rustl::helper::option_or_id::OptionOrId;
use rustl::nalgebra::Matrix4;
use rustl::prelude::*;
use rustl::state::resources::mesh_resource::{MeshResource, MeshResourceItem};
use rustl::state::scene::components::mesh::Mesh;
use rustl::state::scene::instance::Instance;
use rustl::state::scene::node::InstanceItemArc;
use rustl::state::state::ENGINE_INTERNAL_TAG_PREFX;

use crate::random::Random;

pub const GREEN_FILE: &str = "models/lightning.glb";
pub const BLUE_FILE: &str = "models/lightning_blue.glb";

const SEGMENTS: usize = 64;
const STEP: f32 = 0.075;
const BOW: f32 = 0.45;
const PUSH_SMALL: [f32; 2] = [0.025, 0.045];
const PUSH: [f32; 2] = [0.055, 0.095];
// directions to pick from, the step between two neighbouring points (no common divisor with the count) and s until the pattern changes
const DIRECTIONS: usize = 24;
const DIRECTION_STRIDE: usize = 7;
const REFRESH: f32 = 0.025;
// chance per frame for a short flash: the small push and the wide strip
const FLASH_CHANCE: f32 = 0.1;
// m to each side of the strip, new every frame - the ends narrower
const HALF_WIDTH: [f32; 2] = [0.022, 0.055];
const FLASH_HALF_WIDTH: [f32; 2] = [0.09, 0.14];
const END_HALF_WIDTH: f32 = 0.025;
// the brightness flickers between these every frame, the points next to the ends get this share of it
const BRIGHTNESS: [f32; 2] = [0.15, 1.0];
const NEAR_END: f32 = 0.6;

pub struct Lightning
{
    pub root: NodeItem,
    mesh: NodeItem,
    strip: MeshResourceItem,
    instance: InstanceItemArc,
    random: Random,
    directions: Vec<Vector3<f32>>,
    direction_start: usize,
    direction_time: f32,
}

impl Lightning
{
    // the material and texture of the model on a strip mesh of its own, one instance (the ones of the model hidden) - glow: the color for the bloom
    pub fn new(root: NodeItem, glow: Vector3<f32>) -> Option<Self>
    {
        let mut mesh = None;
        crate::for_each_node(&vec![root.clone()], &mut |node| if mesh.is_none() && node.read().unwrap().find_component::<Mesh>().is_some() { mesh = Some(node.clone()); });
        let mesh = mesh?;
        crate::change_material(&mesh, |m| { m.base_color = Vector3::new(1.0, 1.0, 1.0); m.ambient_color = glow; m.cast_shadow = false; m.receive_shadow = false; });

        // the blades of the model make way for the strip, rebuilt every frame - its bounds would be stale, so no culling and no picking
        let strip: MeshResourceItem = Arc::new(RwLock::new(Box::new(MeshResource::new_plane("lightning", Point3::new(-0.5, 0.0, 0.0), Point3::new(0.5, 0.0, 0.0), Point3::new(0.5, 0.0, -1.0), Point3::new(-0.5, 0.0, -1.0)))));
        if let Some(component) = mesh.read().unwrap().find_component::<Mesh>()
        {
            component_downcast_mut!(component, Mesh);
            component.mesh_resource = OptionOrId::Some(strip.clone());
        }
        {
            let mut node = mesh.write().unwrap();
            node.settings.frustum_culling = false;
            node.settings.occlusion_culling = false;
            node.settings.pickable = false;
        }
        for instance in mesh.read().unwrap().instances.get_ref().iter() { instance.write().unwrap().get_data_mut().get_mut().visible = false; }
        let transform = Transformation::new("Transform", Vector3::zeros(), Vector3::zeros(), Vector3::new(1.0, 1.0, 1.0));
        let mut instance = Instance::new_with_transform("lightning".to_string(), mesh.clone(), transform);
        instance.get_data_mut().get_mut().visible = false;
        let instance = mesh.write().unwrap().add_instance(Box::new(instance));

        let mut random = Random::default();
        let directions = (0..DIRECTIONS).map(|_| Vector3::new(random.next() * 2.0 - 1.0, random.next() * 2.0 - 1.0, random.next() * 2.0 - 1.0).try_normalize(1e-4).unwrap_or(Vector3::y())).collect();
        Some(Self { root, mesh, strip, instance, random, directions, direction_start: 0, direction_time: 0.0 })
    }

    pub fn root_id(&self) -> u32
    {
        self.root.read().unwrap().id
    }

    // the arcs of this frame, each from its start to its end - none hides the strip
    pub fn draw(&mut self, scene: &Scene, arcs: &[(Vector3<f32>, Vector3<f32>)], dt: f32)
    {
        let shaped: Vec<(Vector3<f32>, Vector3<f32>, f32)> = arcs.iter().map(|(a, d)| (*a, *d, 1.0)).collect();
        self.draw_shaped(scene, &shaped, dt);
    }

    // with a shape per arc: 1 = bowed up and jagged, 0 = a straight beam, only a little jagged
    pub fn draw_shaped(&mut self, scene: &Scene, arcs: &[(Vector3<f32>, Vector3<f32>, f32)], dt: f32)
    {
        if arcs.is_empty()
        {
            self.instance.write().unwrap().get_data_mut().get_mut().visible = false;
            return;
        }

        self.direction_time -= dt;
        if self.direction_time <= 0.0
        {
            self.direction_time = REFRESH;
            self.direction_start = (self.random.next() * DIRECTIONS as f32) as usize % DIRECTIONS;
        }
        // the brightness as alpha - the blend stands in for an additive one, so darker means fainter
        let [low, high] = BRIGHTNESS;
        let brightness = low + self.random.next() * (high - low);

        // across the view of the camera nearest to the arcs (split screen: one of them)
        let middle = arcs.iter().map(|(a, d, _)| (a + d) * 0.5).sum::<Vector3<f32>>() / arcs.len() as f32;
        let eye = scene.cameras.iter().filter(|camera| camera.enabled && !camera.tags.contains_starts_with(ENGINE_INTERNAL_TAG_PREFX)).map(|camera| camera.get_data().eye_pos.coords)
            .min_by(|x, y| (x - middle).norm_squared().total_cmp(&(y - middle).norm_squared())).unwrap_or(middle + Vector3::y() * 10.0);

        let inverse = self.mesh.read().unwrap().get_full_transform().try_inverse().unwrap_or_else(Matrix4::identity);
        let (mut vertices, mut normals, mut uvs, mut colors, mut indices) = (vec![], vec![], vec![], vec![], vec![]);
        for (number, (a, d, shape)) in arcs.iter().enumerate()
        {
            let (points, flash) = self.points(*a, *d, *shape, number);
            let count = points.len() - 1;
            let [low, high] = if flash { FLASH_HALF_WIDTH } else { HALF_WIDTH };
            let half_width = low + self.random.next() * (high - low);

            // at inner points along both segments
            let sides: Vec<Vector3<f32>> = (0..count).map(|i| (points[i + 1] - points[i]).cross(&((points[i] + points[i + 1]) * 0.5 - eye)).try_normalize(1e-6).unwrap_or_else(Vector3::zeros)).collect();
            let offsets: Vec<Vector3<f32>> = (0..=count).map(|i|
            {
                if i == 0 { sides[0] * END_HALF_WIDTH }
                else if i == count { sides[count - 1] * END_HALF_WIDTH }
                else { (sides[i - 1] + sides[i]).try_normalize(1e-6).unwrap_or(sides[i]) * half_width }
            }).collect();

            // one quad per segment, the texture along each one
            for i in 0..count
            {
                let base = vertices.len() as u32;
                for (point, side, u, v) in [(i, -1.0, 0.0, 0.0), (i + 1, -1.0, 0.0, 1.0), (i + 1, 1.0, 1.0, 1.0), (i, 1.0, 1.0, 0.0)]
                {
                    let position = points[point] + offsets[point] * side;
                    vertices.push(inverse.transform_point(&Point3::from(position)));
                    normals.push(inverse.transform_vector(&(eye - position)).try_normalize(1e-6).unwrap_or_else(Vector3::y));
                    uvs.push(Point2::new(u, v));
                    let alpha = if point == 0 || point == count { 0.0 } else if point == 1 || point + 1 == count { brightness * NEAR_END } else { brightness };
                    colors.push([1.0, 1.0, 1.0, alpha]);
                }
                indices.extend([[base, base + 1, base + 2], [base, base + 2, base + 3]]);
            }
        }
        {
            let mut strip = self.strip.write().unwrap();
            let data = strip.get_data_mut().get_mut();
            data.vertices = vertices;
            data.normals = normals;
            data.uvs_0 = uvs;
            data.colors = colors;
            data.uv_indices = indices.clone();
            data.normals_indices = indices.clone();
            data.indices = indices;
        }
        self.instance.write().unwrap().get_data_mut().get_mut().visible = true;
    }

    // the points of one arc and whether it flashes this frame (the small push and the wide strip) - each arc starts elsewhere in the directions
    fn points(&mut self, a: Vector3<f32>, d: Vector3<f32>, shape: f32, number: usize) -> (Vec<Vector3<f32>>, bool)
    {
        let (b, c) = (a + Vector3::y() * BOW * shape, d + Vector3::y() * BOW * shape);
        let count = (((d - a).norm() / STEP) as usize).clamp(1, SEGMENTS);
        let flash = self.random.next() < FLASH_CHANCE;
        let [low, high] = if flash { PUSH_SMALL } else { PUSH };
        let push = (low + self.random.next() * (high - low)) * (0.4 + 0.6 * shape);
        let start = self.direction_start + number * 5;
        let points = (0..=count).map(|i|
        {
            let t = i as f32 / count as f32;
            let u = 1.0 - t;
            a * (u * u * u) + b * (3.0 * u * u * t) + c * (3.0 * u * t * t) + d * (t * t * t) + self.directions[(start + i * DIRECTION_STRIDE) % DIRECTIONS] * push
        }).collect();
        (points, flash)
    }
}

// the lights of the level on the cars (formats/README.md ".lit"): the game adds the kept lights to the corner colors of the moving models every frame,
// the square shadows darken them - here once per car at its centre, as the color of all its mesh instances (with the soot of the bomb)

use std::collections::HashMap;

use rustl::prelude::*;
use rustl::serde_json::{self, Value};

use crate::sounds::cars;

const OMNI_NORMAL: u8 = 1;
const SPOT: u8 = 2;
const SPOT_NORMAL: u8 = 3;
const SQUARE_SHADOW: u8 = 4;

// the darkening of the square shadows without SHADOWBOX in the .inf
const SHADOW_BOX: f32 = -96.0;
// the normal kinds: the share of the light at the centre (no surface normal there - half of the car faces the light)
const NORMAL_SHARE: f32 = 0.5;
// at most this bright, the tint follows its target at this rate per s (a light reached at the centre would pop)
const MAX_TINT: f32 = 2.5;
const FOLLOW: f32 = 8.0;
// a change smaller than this is not written
const MIN_CHANGE: f32 = 0.005;

struct Light
{
    kind: u8,
    position: Vector3<f32>,
    reach: f32,
    axes: [Vector3<f32>; 3],
    cone: f32,
    // the color 0..255 or the half size of a square shadow (m)
    value: Vector3<f32>,
}

struct SceneLights
{
    scene_id: u32,
    lights: Vec<Light>,
    // MODELRGBPER of the .inf (the cars are scaled by it at load) and SHADOWBOX
    shade: f32,
    shadow_box: f32,
}

struct Tint
{
    scene_id: u32,
    current: Vector3<f32>,
    applied: Vector3<f32>,
}

#[derive(Default)]
pub struct CarLights
{
    scenes: Vec<SceneLights>,
    cars: HashMap<u32, Tint>,
}

impl CarLights
{
    pub fn add_scene(&mut self, scene: &Scene)
    {
        if self.scenes.iter().any(|part| part.scene_id == scene.id) { return; }
        let Some(level) = crate::track::find_node(&scene.nodes, &|node| node.tags.contains("revolt_level") && node.extras.contains("revolt_lights")) else { return; };
        let level = level.read().unwrap();
        let transform = level.get_full_transform();
        let json: Value = level.extras.get::<String>("revolt_lights").and_then(|text| serde_json::from_str(text).ok()).unwrap_or_default();
        let vector = |value: &Value| Vector3::new(value[0].as_f64().unwrap_or(0.0) as f32, value[1].as_f64().unwrap_or(0.0) as f32, value[2].as_f64().unwrap_or(0.0) as f32);
        let lights: Vec<Light> = json.as_array().into_iter().flatten().map(|light|
        {
            let kind = light["kind"].as_u64().unwrap_or(0) as u8;
            let axes = [0, 1, 2].map(|row| transform.transform_vector(&vector(&light["axes"][row])).try_normalize(1e-6).unwrap_or_else(Vector3::zeros));
            let value = if kind == SQUARE_SHADOW { vector(&light["half_size"]) } else { vector(&light["color"]) };
            Light { kind, position: transform.transform_point(&Point3::from(vector(&light["position"]))).coords, reach: light["reach"].as_f64().unwrap_or(0.0) as f32, axes, cone: light["cone"].as_f64().unwrap_or(0.0) as f32, value }
        }).collect();
        let number = |key: &str| level.extras.get::<f64>(key).map(|v| *v as f32);
        let shade = number("model_rgb").unwrap_or(1.0).max(0.05);
        let shadow_box = level.extras.get::<String>("inf_shadowbox").and_then(|text| text.trim().parse::<f32>().ok()).unwrap_or(SHADOW_BOX);
        console_log!("car lights: scene '{}': {} lights, {} square shadows (SHADOWBOX {})", scene.name, lights.iter().filter(|light| light.kind != SQUARE_SHADOW).count(), lights.iter().filter(|light| light.kind == SQUARE_SHADOW).count(), shadow_box);
        self.scenes.push(SceneLights { scene_id: scene.id, lights, shade, shadow_box });
    }

    pub fn remove_scene(&mut self, scene_id: u32)
    {
        self.scenes.retain(|part| part.scene_id != scene_id);
        self.cars.retain(|_, tint| tint.scene_id != scene_id);
    }

    // back in the editor: every car its own colors again
    pub fn stop(&mut self, state: &mut State)
    {
        for (id, tint) in self.cars.drain()
        {
            let Some(node) = state.find_scene_by_id(tint.scene_id).and_then(|scene| scene.find_node_by_id(id)) else { continue; };
            paint(&node, Vector3::new(1.0, 1.0, 1.0));
        }
        self.scenes.clear();
    }

    // shades: the soot of the bomb per car node (1 = none)
    pub fn update(&mut self, state: &State, shades: &[(u32, f32)], dt: f32)
    {
        let follow = if dt > 0.0 { (FOLLOW * dt).min(1.0) } else { 0.0 };
        for scene in state.scenes.iter().filter(|scene| scene.active)
        {
            let lights = self.scenes.iter().find(|part| part.scene_id == scene.id);
            for car in cars(scene)
            {
                let (id, center) = { let read = car.read().unwrap(); (read.id, read.get_full_transform().column(3).xyz()) };
                let soot = shades.iter().find(|(car, _)| *car == id).map_or(1.0, |(_, shade)| *shade);
                let target = lights.map_or(Vector3::new(1.0, 1.0, 1.0), |lights| lights.tint(center)) * soot;
                let tint = self.cars.entry(id).or_insert(Tint { scene_id: scene.id, current: target, applied: Vector3::new(1.0, 1.0, 1.0) });
                tint.current += (target - tint.current) * follow;
                // soot comes and goes at its own pace
                if soot < 1.0 || follow == 0.0 { tint.current = target; }
                if (tint.current - tint.applied).amax() < MIN_CHANGE { continue; }
                tint.applied = tint.current;
                paint(&car, tint.current);
            }
        }
    }
}

impl SceneLights
{
    // the factor on the car colors at a point: the lights added to the shaded corner color of the game, back into linear
    fn tint(&self, point: Vector3<f32>) -> Vector3<f32>
    {
        let mut added = Vector3::zeros();
        let mut shadowed = false;
        for light in &self.lights
        {
            let delta = light.position - point;
            if light.kind == SQUARE_SHADOW
            {
                // inside the box along its axes - once, like the game per corner
                shadowed = shadowed || (0..3).all(|axis| delta.dot(&light.axes[axis]).abs() < light.value[axis]);
                continue;
            }
            let squared = delta.norm_squared();
            let reach = light.reach * light.reach;
            if squared >= reach || squared <= 0.0 { continue; }
            let mut amount = 1.0 - squared / reach;
            if light.kind == OMNI_NORMAL || light.kind == SPOT_NORMAL { amount *= NORMAL_SHARE; }
            if (light.kind == SPOT || light.kind == SPOT_NORMAL) && light.cone > 0.0
            {
                let along = -delta.dot(&light.axes[2]) / squared.sqrt();
                let cone = (along - 1.0) * (180.0 / light.cone) + 1.0;
                if cone <= 0.0 { continue; }
                amount *= cone;
            }
            added += light.value * amount;
        }
        if shadowed { added += Vector3::new(1.0, 1.0, 1.0) * self.shadow_box; }
        added.map(|channel| (((self.shade + channel / 255.0).max(0.0) / self.shade).powf(2.2)).min(MAX_TINT))
    }
}

// all mesh instances of a car in this color - the skin hangs in the car and keeps its glow
fn paint(node: &NodeItem, tint: Vector3<f32>)
{
    if node.read().unwrap().name == "skin" { return; }
    for instance in node.read().unwrap().instances.get_ref().iter()
    {
        instance.write().unwrap().get_data_mut().get_mut().color = Vector4::new(tint.x, tint.y, tint.z, 1.0);
    }
    let children = node.read().unwrap().nodes.clone();
    for child in &children { paint(child, tint); }
}

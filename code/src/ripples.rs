// rings on the water of the levels where drops fall in (the dolphin fountain, sprinklers, splashes): the water objects get the water shader,
// the drops that end on their surface (particles.rs) are gathered into a few zones that the shader fills with rings at random points

use rustl::prelude::*;
use rustl::rendering::custom_shader::parse_params;
use rustl::state::scene::components::material::{Material, MaterialItem};

use crate::object_type::ObjectType;
use crate::particles::{Particles, WaterSurface};

// compiled into the game - the path is for "Load" in the material panel
const SHADER_PATH: &str = "data/projects/rustl-volt/code/shaders/water.wgsl";
const SHADER: &str = include_str!("../shaders/water.wgsl");
// the zones: one vec4 param each, one after the other from the first
const ZONES: usize = 4;
const FIRST_ZONE: &str = "zone0";
// thinner than this (m) counts as flat - only there drops end on the surface
const FLAT: f32 = 0.05;
// a drop further than this (m, or 2 × the radius) from every zone starts a new one
const JOIN: f32 = 1.5;
// each drop pulls its zone this share towards it, the radius towards the drop distance × RADIUS_SCALE
const FOLLOW: f32 = 0.03;
const RADIUS_SCALE: f32 = 1.5;
const START_RADIUS: f32 = 0.3;
const MIN_RADIUS: f32 = 0.1;
// drops per s for a zone full of rings, s to follow a change of the rate, below this share the zone goes
const FULL_RATE: f32 = 60.0;
const RATE_TIME: f32 = 0.5;
const GONE: f32 = 0.01;

struct Zone
{
    center: Vector2<f32>,
    radius: f32,
    rate: f32,
    hits: usize,
}

struct SceneRipples
{
    scene_id: u32,
    materials: Vec<MaterialItem>,
    // fixed slots - a zone keeps its shader param (and its rings) while it lives
    zones: [Option<Zone>; ZONES],
    // first float of zone0 in the custom params
    slot: usize,
}

#[derive(Default)]
pub struct Ripples
{
    scenes: Vec<SceneRipples>,
    // the Video Settings option - off: plain water, the drops still end on the surface
    enabled: bool,
}

impl Ripples
{
    pub fn add_scene(&mut self, scene: &Scene, particles: &mut Particles)
    {
        if self.scenes.iter().any(|ripples| ripples.scene_id == scene.id) { return; }
        let mut materials: Vec<MaterialItem> = vec![];
        let mut surfaces = vec![];
        crate::for_each_node(&scene.nodes, &mut |node|
        {
            {
                let read = node.read().unwrap();
                if !read.tags.contains("revolt_object") { return; }
                if !read.extras.get::<i64>("revolt_object_type").and_then(|id| ObjectType::from_id(*id)).is_some_and(|kind| kind.is_water()) { return; }
                if let Some((min, max)) = read.get_world_bounding_info(None, true, None).filter(|(min, max)| max.y - min.y < FLAT)
                {
                    surfaces.push(WaterSurface { min: Vector2::new(min.x, min.z), max: Vector2::new(max.x, max.z), y: max.y });
                }
            }
            crate::for_each_node(&vec![node.clone()], &mut |part|
            {
                let Some(material) = part.read().unwrap().find_component::<Material>() else { return; };
                if !materials.iter().any(|known| std::sync::Arc::ptr_eq(known, &material)) { materials.push(material); }
            });
        });
        if materials.is_empty() { return; }
        let Some(slot) = parse_params(SHADER).into_iter().find(|param| param.name == FIRST_ZONE).map(|param| param.slot) else { console_warning!("ripples: no param {} in {}", FIRST_ZONE, SHADER_PATH); return; };

        if self.enabled { shade(&materials, true); }
        console_log!("ripples: scene '{}': {} water materials, {} flat surfaces", scene.name, materials.len(), surfaces.len());
        particles.set_surfaces(scene.id, surfaces);
        self.scenes.push(SceneRipples { scene_id: scene.id, materials, zones: Default::default(), slot });
    }

    // the shader on or off on the water of every scene - the zones start over
    pub fn set_enabled(&mut self, enabled: bool)
    {
        if self.enabled == enabled { return; }
        self.enabled = enabled;
        for ripples in &mut self.scenes
        {
            shade(&ripples.materials, enabled);
            ripples.zones = Default::default();
        }
    }

    pub fn remove_scene(&mut self, scene_id: u32)
    {
        self.scenes.retain(|ripples| ripples.scene_id != scene_id);
    }

    pub fn stop(&mut self)
    {
        self.scenes.clear();
    }

    // after Particles::update - the drops of this frame
    pub fn update(&mut self, particles: &mut Particles, dt: f32)
    {
        if dt <= 0.0 { return; }
        let splashes = particles.take_splashes();
        if !self.enabled { return; }
        for ripples in &mut self.scenes
        {
            let scene_id = ripples.scene_id;
            for (_, point) in splashes.iter().filter(|(id, _)| *id == scene_id)
            {
                ripples.hit(Vector2::new(point.x, point.z));
            }
            ripples.step(dt);
            ripples.write();
        }
    }
}

// the water shader on, or off like the remove button of the material panel
fn shade(materials: &[MaterialItem], on: bool)
{
    for material in materials
    {
        component_downcast_mut!(material, Material);
        if on
        {
            material.set_shader(SHADER_PATH, SHADER);
            continue;
        }
        let data = material.get_data_mut().get_mut();
        data.shader_path.clear();
        data.shader_source_code.clear();
        data.shader_features.clear();
        data.custom_params = Default::default();
    }
}

impl SceneRipples
{
    fn hit(&mut self, point: Vector2<f32>)
    {
        let nearest = self.zones.iter_mut().flatten()
            .map(|zone| ((point - zone.center).norm(), zone))
            .filter(|(distance, zone)| *distance < JOIN.max(zone.radius * 2.0))
            .min_by(|a, b| a.0.total_cmp(&b.0));
        match nearest
        {
            Some((distance, zone)) =>
            {
                zone.center += (point - zone.center) * FOLLOW;
                zone.radius += ((distance * RADIUS_SCALE).max(MIN_RADIUS) - zone.radius) * FOLLOW;
                zone.hits += 1;
            },
            // all slots taken - the drop makes no rings
            None => if let Some(free) = self.zones.iter_mut().find(|zone| zone.is_none())
            {
                *free = Some(Zone { center: point, radius: START_RADIUS, rate: 0.0, hits: 1 });
            },
        }
    }

    fn step(&mut self, dt: f32)
    {
        let follow = 1.0 - (-dt / RATE_TIME).exp();
        for slot in &mut self.zones
        {
            let Some(zone) = slot else { continue; };
            zone.rate += (zone.hits as f32 / dt - zone.rate) * follow;
            zone.hits = 0;
            if zone.rate < FULL_RATE * GONE { *slot = None; }
        }
    }

    fn write(&self)
    {
        let values = self.zones.each_ref().map(|zone| zone.as_ref().map_or([0.0; 4], |zone| [zone.center.x, zone.center.y, zone.radius, (zone.rate / FULL_RATE).min(1.0)]));
        let values = values.as_flattened();
        // straight into the params - set_shader_param parses the shader every call; unmarked: uploaded every frame anyway, no rebuild
        for material in &self.materials
        {
            component_downcast_mut!(material, Material);
            material.get_data_mut().get_unmarked_mut().custom_params[self.slot..self.slot + values.len()].copy_from_slice(values);
        }
    }
}

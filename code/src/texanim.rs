// the texture animations of the levels (.w TexAnim: the escalator steps of muse2, screens, water, ...) as an offset of the base texture
// the converter writes the frames onto the material (revolt_texanim_data, the primitive nodes get the material extras), the mesh has the uvs of frame 0
// a frame that moves the uvs is played as their shift (centre of its first 3 corners minus the one of frame 0) - frames on another texture page are left out

use rustl::prelude::*;
use rustl::serde_json::{self, Value};

struct Animated
{
    node: NodeItem,
    // offset and seconds per frame
    frames: Vec<(Vector2<f32>, f32)>,
    duration: f32,
    shown: usize,
}

#[derive(Default)]
pub struct TexAnims
{
    // scene id and its animated primitives
    scenes: Vec<(u32, Vec<Animated>)>,
    time: f32,
}

impl TexAnims
{
    pub fn add_scene(&mut self, scene: &Scene)
    {
        if self.scenes.iter().any(|(id, _)| *id == scene.id) { return; }
        let mut animated = vec![];
        let mut skipped = 0;
        crate::for_each_node(&scene.nodes, &mut |node|
        {
            let Some(json) = node.read().unwrap().extras.get::<String>("revolt_texanim_data").cloned() else { return; };
            // only the node that holds the material - the parents carry no extras of it
            if node.read().unwrap().find_component::<rustl::state::scene::components::material::Material>().is_none() { return; }
            match frames(&json)
            {
                Some(frames) =>
                {
                    let duration = frames.iter().map(|(_, delay)| delay).sum::<f32>();
                    if duration > 0.0 { animated.push(Animated { node: node.clone(), frames, duration, shown: 0 }); }
                },
                None => skipped += 1,
            }
        });
        if !animated.is_empty() || skipped > 0
        {
            console_log!("scene '{}': {} texture animated parts, {} left out (frames on several pages)", scene.name, animated.len(), skipped);
        }
        self.scenes.push((scene.id, animated));
    }

    // only the active scenes - their frame by the clock of the game
    pub fn update(&mut self, state: &State, dt: f32)
    {
        self.time += dt;
        for (scene_id, animated) in &mut self.scenes
        {
            if !state.find_scene_by_id(*scene_id).is_some_and(|scene| scene.active) { continue; }
            for part in animated.iter_mut()
            {
                let mut t = self.time % part.duration;
                let mut frame = 0;
                while frame + 1 < part.frames.len() && t >= part.frames[frame].1
                {
                    t -= part.frames[frame].1;
                    frame += 1;
                }
                if frame == part.shown { continue; }
                part.shown = frame;
                set_offset(&part.node, part.frames[frame].0);
            }
        }
    }

    // the scene was unloaded - its parts are gone
    pub fn remove_scene(&mut self, scene_id: u32)
    {
        self.scenes.retain(|(id, _)| *id != scene_id);
    }

    pub fn stop(&mut self)
    {
        for (_, animated) in self.scenes.drain(..)
        {
            for part in animated.iter().filter(|part| part.shown != 0)
            {
                set_offset(&part.node, Vector2::zeros());
            }
        }
    }
}

// the uv shift of every frame against frame 0 - None if a frame uses another texture page
fn frames(json: &str) -> Option<Vec<(Vector2<f32>, f32)>>
{
    let data: Value = serde_json::from_str(json).ok()?;
    let list = data.as_array()?;
    // the first 3 corners - a triangle leaves the 4th unused (0 or junk)
    let centre = |frame: &Value| -> Option<Vector2<f32>>
    {
        let uvs = frame["uvs"].as_array()?;
        let used = &uvs[..uvs.len().min(3)];
        let sum = used.iter().fold(Vector2::zeros(), |sum, uv| sum + Vector2::new(uv[0].as_f64().unwrap_or(0.0) as f32, uv[1].as_f64().unwrap_or(0.0) as f32));
        Some(sum / used.len().max(1) as f32)
    };
    let first = list.first()?;
    let (page, origin) = (first["texture"].as_i64(), centre(first)?);
    list.iter().map(|frame|
    {
        if frame["texture"].as_i64() != page { return None; }
        Some((centre(frame)? - origin, frame["delay"].as_f64().unwrap_or(0.0) as f32))
    }).collect()
}

fn set_offset(node: &NodeItem, offset: Vector2<f32>)
{
    crate::change_material(node, |m| if let Some(texture) = m.texture_base.as_mut() { texture.transform.offset = offset; });
}

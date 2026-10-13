// the 3D sound objects of a level (.fob 49) like the game: loops play all the time, random play ones now and then
// only in active scenes - an inactive scene is not updated, its spatial sounds get no position and play at full volume
// the converter puts a spatial Sound component 'Ambient' on every 3D sound object (looped or not, distance scale = full volume radius)

use std::sync::Arc;

use rustl::prelude::*;
use rustl::state::scene::components::component::ComponentItem;
use rustl::state::scene::components::sound::Sound;
use rustl::state::scene::scene::Scene;
use rustl::state::state::{State, REFERENCE_UPDATE_FRAMES};

use crate::object_type::ObjectType;
use crate::random::Random;

const COMPONENT_NAME: &str = "Ambient";
// random play: the first play after 0-20 s, after each play a pause of 10-30 s
const FIRST_WAIT: f32 = 20.0;
const PAUSE_MIN: f32 = 10.0;
const PAUSE_RANDOM: f32 = 20.0;

struct AmbientSound
{
    scene_id: u32,
    component: ComponentItem,
    random: bool,
    // seconds until the next play of a random one
    wait: f32,
    playing: bool,
}

#[derive(Default)]
pub struct Ambient
{
    sounds: Vec<AmbientSound>,
    random: Random,
}

impl Ambient
{
    // the 3D sound objects of a loaded scene - they start in update while the scene is active
    pub fn add_scene(&mut self, scene: &Scene)
    {
        let mut nodes = vec![];
        crate::for_each_node(&scene.nodes, &mut |node| nodes.push(node.clone()));

        let (mut loops, mut random) = (0, 0);
        for node in nodes
        {
            let node = node.read().unwrap();
            if node.extras.get::<i64>("revolt_object_type").and_then(|id| ObjectType::from_id(*id)) != Some(ObjectType::Sound3d) { continue; }

            for component in node.find_components::<Sound>()
            {
                if component.read().unwrap().get_base().name != COMPONENT_NAME { continue; }
                if self.sounds.iter().any(|sound| Arc::ptr_eq(&sound.component, &component)) { continue; }

                let looped = { component_downcast!(component, Sound); component.get_data().looped };
                let wait = if looped { 0.0 } else { self.random.next() * FIRST_WAIT };
                if looped { loops += 1; } else { random += 1; }
                self.sounds.push(AmbientSound { scene_id: scene.id, component, random: !looped, wait, playing: false });
            }
        }

        if loops + random > 0
        {
            console_log!("scene '{}': {} ambient loops, {} random ambient sounds", scene.name, loops, random);
        }
    }

    pub fn update(&mut self, state: &State)
    {
        let seconds = state.stats.frame_scale / REFERENCE_UPDATE_FRAMES;
        for i in 0..self.sounds.len()
        {
            let active = state.scenes.iter().any(|scene| scene.id == self.sounds[i].scene_id && scene.active);
            let sound = &mut self.sounds[i];
            if !active
            {
                if sound.playing { sound.stop(); }
                continue;
            }
            if !sound.random
            {
                if !sound.playing { sound.play(); }
                continue;
            }

            let pause = PAUSE_MIN + self.random.next() * PAUSE_RANDOM;
            let sound = &mut self.sounds[i];
            if sound.playing
            {
                let stopped = { let component = &sound.component; component_downcast!(component, Sound); component.stopped() };
                if stopped
                {
                    sound.playing = false;
                    sound.wait = pause;
                }
                continue;
            }

            sound.wait -= seconds;
            if sound.wait <= 0.0
            {
                sound.play();
            }
        }
    }

    pub fn stop(&mut self)
    {
        for sound in &mut self.sounds
        {
            sound.stop();
        }
        self.sounds.clear();
    }

    // the scene was unloaded - its sounds go
    pub fn remove_scene(&mut self, scene_id: u32)
    {
        for sound in self.sounds.iter_mut().filter(|sound| sound.scene_id == scene_id)
        {
            sound.stop();
        }
        self.sounds.retain(|sound| sound.scene_id != scene_id);
    }

}

impl AmbientSound
{
    fn play(&mut self)
    {
        let component = &self.component;
        component_downcast_mut!(component, Sound);
        component.start();
        self.playing = true;
    }

    fn stop(&mut self)
    {
        let component = &self.component;
        component_downcast_mut!(component, Sound);
        component.stop();
        self.playing = false;
    }
}

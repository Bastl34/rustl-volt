// the music of a level like the game (revolt.exe, cd audio): a random track of the REDBOOK range, played on to the end of the range, then again from that track
// the converter writes the range as level extras (music_track_start / _end, 0 = Track02) and puts its tracks into the scene sound list

use rustl::prelude::*;
use rustl::state::resources::sound_source::SoundSourceItem;
use rustl::state::scene::components::sound::{Sound, SoundCategory, SoundType};
use rustl::state::state::State;

// the music volume of the game is a menu setting - the sounds stay in front
const MUSIC_VOLUME: f32 = 0.45;
// audio tracks of the cd start at track 2
const FIRST_CD_TRACK: i64 = 2;

pub struct Music
{
    tracks: Vec<(String, SoundSourceItem)>,
    // the random one the range starts with again after its end
    first: usize,
    current: usize,
    sound: Option<Sound>,
}

impl Music
{
    // None if the scene has no music range or its tracks are not loaded
    pub fn start(state: &State, scene_id: u32) -> Option<Self>
    {
        let scene = state.scenes.iter().find(|scene| scene.id == scene_id)?;
        let (start, end) = scene.nodes.iter().find_map(|node| range_of(node))?;

        let tracks: Vec<(String, SoundSourceItem)> = (start..=end).filter_map(|index|
        {
            let name = format!("track{:02}", index + FIRST_CD_TRACK);
            let source = state.resources.sound_sources.values().find(|source| source.read().unwrap().name == name).cloned();
            if source.is_none() { console_warning!("music: {} is not in the scene sound list - re-convert the level", name); }
            source.map(|source| (name, source))
        }).collect();
        if tracks.is_empty()
        {
            return None;
        }

        let first = random_index(tracks.len());
        let mut music = Self { tracks, first, current: first, sound: None };
        music.play();
        Some(music)
    }

    // the next track once one ended - after the last one of the range the first again
    pub fn update(&mut self)
    {
        // the track is no scene component - nothing else hands it a change of the audio options
        if let Some(sound) = self.sound.as_mut()
        {
            sound.update_state(None, None, true);
        }
        if self.sound.as_ref().is_some_and(|sound| sound.stopped())
        {
            self.current = if self.current + 1 < self.tracks.len() { self.current + 1 } else { self.first };
            self.play();
        }
    }

    pub fn stop(&mut self)
    {
        if let Some(sound) = self.sound.as_mut()
        {
            sound.stop();
        }
        self.sound = None;
    }

    fn play(&mut self)
    {
        let (name, source) = &self.tracks[self.current];
        let mut sound = Sound::new(name, source.clone(), SoundType::Stereo, false);
        sound.get_data_mut().get_mut().volume = MUSIC_VOLUME;
        // the music volume of the audio options on top
        sound.get_data_mut().get_mut().category = SoundCategory::Music;
        sound.start();
        sound.update_state(None, None, true);
        self.sound = Some(sound);
        console_log!("music: {} ({} of {})", name, self.current + 1, self.tracks.len());
    }
}

// the REDBOOK range on the level root node, somewhere below this node
fn range_of(node: &NodeItem) -> Option<(i64, i64)>
{
    let found =
    {
        let node = node.read().unwrap();
        node.extras.get::<i64>("music_track_start").copied().zip(node.extras.get::<i64>("music_track_end").copied())
    };
    if let Some((start, end)) = found.filter(|(start, end)| start <= end)
    {
        return Some((start, end));
    }
    let children = node.read().unwrap().nodes.clone();
    children.iter().find_map(range_of)
}

// no rand crate - the clock is random enough for one pick per level
fn random_index(count: usize) -> usize
{
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
    let mut x = nanos | 1;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    x as usize % count.max(1)
}

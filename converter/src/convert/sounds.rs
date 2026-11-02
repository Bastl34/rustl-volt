// wavs/**/*.wav -> assets/sounds/*.wav (clean 16 bit PCM), the scene sound list and the sound components of the cars
// the app code puts the components of a car onto the slots of its vehicle controller (by name) and plays the hits

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use super::scene::relative_path;
use super::{Log, Settings};
use crate::revolt::game_path;

// component names on a car - the app code finds the slots by them
pub const ENGINE: &str = "Engine";
pub const SQUEAL: &str = "Squeal";
pub const SCRAPE: &str = "Scrape";

// the loops of a car and the one shots the app code plays on hits - names of the scene sound list
const SKID: &str = "skid_normal";
const SCRAPE_SOUND: &str = "scrape";
const HITS: [&str; 2] = ["hit1", "hit2"];

pub fn out_dir(settings: &Settings) -> PathBuf
{
    settings.out_dir.join("sounds")
}

// sound name: the path below wavs/ without extension, lower case, folders joined by _ (garden/stream -> garden_stream)
fn sound_name(relative: &Path) -> String
{
    relative.with_extension("").to_string_lossy().replace(['\\', '/'], "_").to_lowercase()
}

fn sound_path(settings: &Settings, name: &str) -> PathBuf
{
    out_dir(settings).join(format!("{name}.wav"))
}

// every wav of the game - a file that is already newer than its wav is kept
pub fn convert_all(settings: &Settings, log: &Log) -> Result<()>
{
    let wavs = game_path(&settings.game_dir, "wavs");
    let mut files = vec![];
    collect_wavs(&wavs, &mut files)?;
    files.sort();

    // the ogg files of converter runs before asset version 5
    let old: Vec<PathBuf> = std::fs::read_dir(out_dir(settings)).map(|entries| entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("ogg"))).collect()).unwrap_or_default();
    if !old.is_empty()
    {
        old.iter().for_each(|path| { let _ = std::fs::remove_file(path); });
        log.info(format!("  {} old ogg files removed", old.len()));
    }

    let (mut converted, mut kept, mut bytes_in, mut bytes_out) = (0, 0, 0u64, 0u64);
    for file in &files
    {
        let relative = file.strip_prefix(&wavs).unwrap_or(file);
        let out = sound_path(settings, &sound_name(relative));
        let modified = |path: &Path| std::fs::metadata(path).and_then(|m| m.modified()).ok();
        if modified(&out).is_some_and(|written| modified(file).is_some_and(|wav| written >= wav))
        {
            kept += 1;
            continue;
        }

        match clean_wav(file, &out)
        {
            Ok(()) =>
            {
                converted += 1;
                bytes_in += std::fs::metadata(file).map(|m| m.len()).unwrap_or(0);
                bytes_out += std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
            },
            Err(err) => log.warn(format!("  {}: {err:#}", relative.display())),
        }
    }

    log.info(format!("  {} wav files: {converted} converted ({:.1} MB -> {:.1} MB), {kept} already there", files.len(), bytes_in as f64 / 1e6, bytes_out as f64 / 1e6));
    log.info(format!("  -> {}", out_dir(settings).display()));
    Ok(())
}

fn collect_wavs(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()>
{
    for entry in std::fs::read_dir(dir).with_context(|| format!("{} missing", dir.display()))?
    {
        let path = entry?.path();
        if path.is_dir()
        {
            collect_wavs(&path, files)?;
        }
        else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("wav"))
        {
            files.push(path);
        }
    }
    Ok(())
}

// PCM 8 / 16 bit, mono or stereo (all files of the game: mono 22050 Hz) -> 16 bit PCM with only the fmt and data chunks
fn clean_wav(wav: &Path, out: &Path) -> Result<()>
{
    let data = std::fs::read(wav)?;
    if data.len() < 12 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE"
    {
        bail!("no RIFF WAVE file");
    }

    let (mut format, mut samples) = (None, None);
    let mut pos = 12;
    while pos + 8 <= data.len()
    {
        let id = &data[pos..pos + 4];
        let size = u32::from_le_bytes(data[pos + 4..pos + 8].try_into()?) as usize;
        let body = &data[pos + 8..(pos + 8 + size).min(data.len())];
        match id
        {
            b"fmt " if body.len() >= 16 =>
            {
                let tag = u16::from_le_bytes([body[0], body[1]]);
                let channels = u16::from_le_bytes([body[2], body[3]]);
                let rate = u32::from_le_bytes(body[4..8].try_into()?);
                let bits = u16::from_le_bytes([body[14], body[15]]);
                format = Some((tag, channels, rate, bits));
            },
            b"data" => samples = Some(body),
            _ => {},
        }
        // chunks are padded to an even size
        pos += 8 + size + (size & 1);
    }

    let (Some((tag, channels, rate, bits)), Some(samples)) = (format, samples) else { bail!("fmt or data chunk missing"); };
    if tag != 1 { bail!("format {tag} is no PCM"); }

    let values: Vec<i16> = match bits
    {
        8 => samples.iter().map(|b| ((*b as i16) - 128) << 8).collect(),
        16 => samples.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect(),
        _ => bail!("{bits} bit PCM not supported"),
    };
    if rate == 0 { bail!("sample rate 0"); }
    let channels = channels.max(1);

    let data_size = (values.len() * 2) as u32;
    let mut bytes = Vec::with_capacity(44 + data_size as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_size).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * channels as u32 * 2).to_le_bytes());
    bytes.extend_from_slice(&(channels * 2).to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_size.to_le_bytes());
    values.iter().for_each(|v| bytes.extend_from_slice(&v.to_le_bytes()));

    if let Some(parent) = out.parent() { std::fs::create_dir_all(parent)?; }
    std::fs::write(out, bytes)?;
    Ok(())
}

// the engine loop of a car like the game: wincar4 the clockwork, the ufo its own, else Class 0 moto and every other class petrol (the trolley too)
pub fn engine_sound(car: &str, class: Option<i64>) -> Option<&'static str>
{
    let car = car.to_lowercase();
    match (car.as_str(), class)
    {
        ("wincar4", _) => Some("clockwrk"),
        ("ufo", _) => Some("ufo"),
        (_, Some(0) | None) => Some("moto"),
        _ => Some("petrol"),
    }
}

// the engine loops of the cars the app code can pick as opponents
pub const OPPONENT_ENGINES: [&str; 2] = ["moto", "petrol"];

// the sounds a scene with cars needs: their loops and the hits
pub fn scene_sounds(engines: &[&str]) -> Vec<String>
{
    let mut names: Vec<String> = engines.iter().map(|e| e.to_string()).collect();
    names.extend([SKID, SCRAPE_SOUND].map(str::to_string));
    names.extend(HITS.map(str::to_string));
    names.sort();
    names.dedup();
    names
}

fn sound_uuid(name: &str) -> String
{
    format!("revolt-sound-{name}")
}

// the converter entries of the scene sound list are replaced, the editor ones stay
pub fn put_scene_sounds(scene: &mut serde_json::Map<String, Value>, settings: &Settings, names: &[String], log: &Log)
{
    let mut sounds = match scene.remove("sounds") { Some(Value::Array(a)) => a, _ => vec![] };
    sounds.retain(|s| !s["uuid"].as_str().is_some_and(|u| u.starts_with("revolt-sound-")));
    for name in names
    {
        let path = sound_path(settings, name);
        if !path.exists()
        {
            log.warn(format!("  sound {name}: {} missing - convert the sounds first", path.display()));
        }
        sounds.push(json!({ "uuid": sound_uuid(name), "name": name, "source": relative_path(&settings.project_dir, &path) }));
    }
    scene.insert("sounds".into(), Value::Array(sounds));
}

// a spatial sound component on a scene object - distance_scale: the distance in meters up to which it plays at full volume
fn component(object_uuid: &str, name: &str, sound: &str, volume: f32, looped: bool, distance_scale: f32) -> Value
{
    json!({
        "type": "Sound",
        "base": {
            "uuid": format!("{object_uuid}-sound-{}", name.to_lowercase()),
            "is_enabled": true,
            "name": name,
            "info": null,
            "extras": {},
            "tags": {},
            "from_file": false,
            "export": true,
        },
        "data": {
            "sound_type": "Spatial",
            "looped": looped,
            "volume": volume,
            "speed": 1.0,
            "spatial_distance_scale": distance_scale,
            "delete_after_playback": false,
        },
        "sound_source": sound_uuid(sound),
        "duration": 0.0,
    })
}

// looped spatial sound components on the car object - silent until the vehicle controller drives them
pub fn car_components(object_uuid: &str, engine: Option<&str>) -> Vec<Value>
{
    let mut components = vec![];
    if let Some(engine) = engine
    {
        components.push(component(object_uuid, ENGINE, engine, 0.7, true, 1.0));
    }
    components.push(component(object_uuid, SQUEAL, SKID, 0.6, true, 1.0));
    components.push(component(object_uuid, SCRAPE, SCRAPE_SOUND, 0.7, true, 1.0));
    components
}

// ********** 3D sound objects (.fob 49) **********

// component name on a 3D sound object - the app code starts it (loop) or plays it now and then (random play)
pub const AMBIENT: &str = "Ambient";

// the game loads 43 global sounds, then the block of the level - sound id = 43 + index in the block
const GLOBAL_SOUNDS: usize = 43;
// sound id per name flag, 40-47 are the cut Rooftops level
const SOUND_OF_NAME: [usize; 48] = [44, 47, 48, 50, 51, 52, 45, 46, 50, 53, 43, 44, 45, 43, 43, 44, 45, 46, 47, 46, 47, 48, 46, 43, 44, 45, 46, 47, 43, 44, 45, 47, 46, 48, 47, 48, 49, 50, 54, 48, 44, 44, 44, 44, 44, 44, 44, 44];

// the sound block of a level as converted sound names
fn level_block(level: &str) -> Option<&'static [&'static str]>
{
    const TOY: &[&str] = &["toy_piano", "toy_plane", "toy_copter", "toy_dragon", "toy_creak", "toy_train", "toy_whistle", "toy_arcade", "toy_toybrick"];
    const HOOD: &[&str] = &["hood_basketball", "hood_birds1", "hood_birds2", "hood_birds3", "hood_dogbark", "hood_kids", "hood_sprink", "hood_tv", "hood_lawnmower", "hood_digger", "hood_stream", "hood_cityamb2", "hood_roadcone"];
    const GARDEN: &[&str] = &["garden_tropics2", "garden_tropics3", "garden_tropics4", "garden_stream", "garden_animal1", "garden_animal2", "garden_animal3", "garden_animal4"];
    const MUSE: &[&str] = &["muse_museumam", "muse_laserhum", "muse_alarm2", "muse_escalate", "muse_rotating", "muse_largdoor"];
    const MARKET: &[&str] = &["market_aircond1", "market_cabnhum2", "market_carpark", "market_freezer1", "market_iceyarea", "market_sdrsopen", "market_sdrsclos", "market_carton"];
    const GHOST: &[&str] = &["ghost_coyote1", "ghost_bats", "ghost_eagle1", "ghost_minedrip", "ghost_rattler", "ghost_townbell", "ghost_tumbweed"];
    const SHIP: &[&str] = &["ship_intamb1", "ship_seagulls", "ship_shiphorn", "ship_strmrain", "ship_thunder1", "ship_wash"];

    match level.to_lowercase().as_str()
    {
        "toylite" | "toy2" => Some(TOY),
        "nhood1" | "nhood2" | "stunts" | "nhood1_battle" => Some(HOOD),
        "garden1" | "bot_bat" => Some(GARDEN),
        "muse1" | "muse2" | "muse_bat" => Some(MUSE),
        "market1" | "market2" | "markar" => Some(MARKET),
        "wild_west1" | "wild_west2" => Some(GHOST),
        "ship1" | "ship2" => Some(SHIP),
        _ => None,
    }
}

// a sound of the level block by its sound id (43 + index in the block)
pub fn level_sound(level: &str, id: usize) -> Option<&'static str>
{
    level_block(level)?.get(id.checked_sub(GLOBAL_SOUNDS)?).copied()
}

pub struct Ambient
{
    pub sound: &'static str,
    // random play: one shot now and then (10-30 s pause), else a loop
    pub random: bool,
    // meters: full volume up to min, the game stops it beyond max
    pub min_distance: f32,
    pub max_distance: f32,
    pub component: Value,
}

// flags: name, range, random play - range × 0.1 scales the Miles distances 3 / 34, Miles positions are game units × 0.005
pub fn ambient(level: &str, object_uuid: &str, flags: &[i32; 4], scale: f32) -> Option<Ambient>
{
    let block = level_block(level)?;
    let id = *SOUND_OF_NAME.get(usize::try_from(flags[0]).ok()?)?;
    let sound = *block.get(id - GLOBAL_SOUNDS)?;
    let miles_to_meters = scale / 0.005;
    let range = flags[1] as f32 * 0.1;
    let (min_distance, max_distance) = (range * 3.0 * miles_to_meters, range * 34.0 * miles_to_meters);
    let random = flags[2] != 0;
    // the game plays them at full volume (127)
    let component = component(object_uuid, AMBIENT, sound, 1.0, !random, min_distance.max(0.01));
    Some(Ambient { sound, random, min_distance, max_distance, component })
}

// ********** moving objects (formats/README.md, Object motion) **********

// the sound ids the game plays for a moving object, by role - the app code plays them (extras revolt_sound_<role>)
// loop: looped at the object; the others one shots on events of its motion
fn object_sound_ids(kind: i32) -> &'static [(&'static str, usize)]
{
    match kind
    {
        3 | 4 => if kind == 3 { &[("loop", 0x2C)] } else { &[("loop", 0x2D)] },
        5 => &[("roar", 0x2E)],
        12 => &[("creak", 0x2F)],
        13 => &[("loop", 0x30), ("whistle", 0x31)],
        40 => &[("tick", 0x31)],
        56 => &[("open", 0x30), ("close", 0x31)],
        _ => &[],
    }
}

// role -> sound name of the level block (the garden sprinkler gets garden/animal3 like in the game - id 0x31 is block index 6)
pub fn object_sounds(kind: i32, level: &str) -> Vec<(&'static str, &'static str)>
{
    let Some(block) = level_block(level) else { return vec![]; };
    object_sound_ids(kind).iter().filter_map(|(role, id)| Some((*role, *block.get(id.checked_sub(GLOBAL_SOUNDS)?)?))).collect()
}

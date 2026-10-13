// music/TrackNN.ogg -> assets/music/trackNN.ogg and the tracks of a level in its scene sound list
// the game plays cd audio: REDBOOK of the .inf is a range of audio tracks, 0 = cd track 2 (track 1 is data) - the steam version maps them to music/TrackNN

use std::path::PathBuf;

use anyhow::Result;
use serde_json::{json, Value};

use super::scene::relative_path;
use super::{Log, Settings};
use crate::revolt::find_file;

// audio tracks of the cd: Track02..Track15 - the exe reads at most 14
const FIRST_CD_TRACK: i64 = 2;
const TRACK_COUNT: i64 = 14;

fn track_name(index: i64) -> String
{
    format!("track{:02}", index + FIRST_CD_TRACK)
}

fn out_path(settings: &Settings, index: i64) -> PathBuf
{
    settings.out_dir.join("music").join(format!("{}.ogg", track_name(index)))
}

// already ogg in the steam version - copied, a copy newer than the original is kept
pub fn copy_all(settings: &Settings, log: &Log) -> Result<()>
{
    let dir = settings.game_dir.join("music");
    let (mut copied, mut kept, mut missing) = (0, 0, vec![]);
    for index in 0..TRACK_COUNT
    {
        let file = format!("Track{:02}.ogg", index + FIRST_CD_TRACK);
        let Some(source) = find_file(&dir, &file) else { missing.push(file); continue; };
        let out = out_path(settings, index);
        let modified = |path: &std::path::Path| std::fs::metadata(path).and_then(|m| m.modified()).ok();
        if modified(&out).is_some_and(|copy| modified(&source).is_some_and(|original| copy >= original))
        {
            kept += 1;
            continue;
        }
        if let Some(parent) = out.parent() { std::fs::create_dir_all(parent)?; }
        std::fs::copy(&source, &out)?;
        copied += 1;
    }
    log.info(format!("  {copied} tracks copied, {kept} already there -> {}", settings.out_dir.join("music").display()));
    if !missing.is_empty() { log.warn(format!("  missing: {}", missing.join(", "))); }
    Ok(())
}

// the tracks of REDBOOK (extras music_track_start / _end) - the app code picks one of them like the game
pub fn put_scene_music(scene: &mut serde_json::Map<String, Value>, settings: &Settings, level_extras: &Value, log: &Log)
{
    let mut sounds = match scene.remove("sounds") { Some(Value::Array(a)) => a, _ => vec![] };
    sounds.retain(|s| !s["uuid"].as_str().is_some_and(|u| u.starts_with("revolt-music-")));

    let range = (level_extras["music_track_start"].as_i64(), level_extras["music_track_end"].as_i64());
    match range
    {
        (Some(start), Some(end)) if start <= end =>
        {
            let indices: Vec<i64> = (start.max(0)..=end.min(TRACK_COUNT - 1)).collect();
            for index in &indices
            {
                let name = track_name(*index);
                sounds.push(json!({ "uuid": format!("revolt-music-{name}"), "name": name, "source": relative_path(&settings.project_dir, &out_path(settings, *index)) }));
            }
            log.info(format!("  music: REDBOOK {start} {end} -> {}", indices.iter().map(|i| track_name(*i)).collect::<Vec<_>>().join(", ")));
        },
        _ => log.info("  music: no REDBOOK in the .inf"),
    }

    if !sounds.is_empty()
    {
        scene.insert("sounds".into(), Value::Array(sounds));
    }
}

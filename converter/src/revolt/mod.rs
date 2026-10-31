// readers for the original Re-Volt file formats - only parsing, no conversion

pub mod fan;
pub mod fin;
pub mod fob;
pub mod hul;
pub mod inf;
pub mod lit;
pub mod mesh;
pub mod ncp;
pub mod object_types;
pub mod pan;
pub mod parameters;
pub mod reader;
pub mod rim;
pub mod taz;
pub mod tri;
pub mod world;

use std::path::{Path, PathBuf};

// the game files mix upper and lower case (MARKET1.taz, market1a.BMP, market1j.bmp)
pub fn find_file(dir: &Path, name: &str) -> Option<PathBuf>
{
    let name = name.to_lowercase();

    std::fs::read_dir(dir).ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .find(|path| path.file_name().is_some_and(|file| file.to_string_lossy().to_lowercase() == name))
}

// a path of the game ("cars\Adeon\car.bmp", "levels/garden1") - every part case-insensitive: linux file systems are case sensitive, the folders can be Levels, CARS, ...
pub fn game_file(game_dir: &Path, path: &str) -> Option<PathBuf>
{
    path.split(['\\', '/']).filter(|part| !part.is_empty()).try_fold(game_dir.to_path_buf(), |dir, part| find_file(&dir, part))
}

// the same, missing parts as written - for folders that are read later (read_dir, find_file) and for the messages
pub fn game_path(game_dir: &Path, path: &str) -> PathBuf
{
    game_file(game_dir, path).unwrap_or_else(|| path.split(['\\', '/']).filter(|part| !part.is_empty()).fold(game_dir.to_path_buf(), |dir, part| dir.join(part)))
}

// the backwards variant of a level: levels/<level>/reversed with its own .fin / .taz / .pan / .fan, the world is the same
pub const REVERSED: &str = "reversed";
// the game's mirror mode: no files of its own, the converter mirrors the level (left / right)
pub const MIRRORED: &str = "mirrored";

pub fn has_reversed(game_dir: &Path, level: &str) -> bool
{
    let dir = game_path(game_dir, &format!("levels/{level}/{REVERSED}"));
    find_file(&dir, &format!("{level}.fin")).is_some() || find_file(&dir, &format!("{level}.taz")).is_some()
}

// a level is a folder in levels/ with a .w file named like the folder
pub fn list_levels(game_dir: &Path) -> Vec<String>
{
    let levels_dir = game_path(game_dir, "levels");
    let Ok(entries) = std::fs::read_dir(&levels_dir) else { return vec![]; };

    let mut levels: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter_map(|path| path.file_name().map(|name| name.to_string_lossy().to_string()))
        .filter(|name| find_file(&levels_dir.join(name), &format!("{name}.w")).is_some())
        .collect();

    levels.sort_by_key(|name| name.to_lowercase());
    levels
}

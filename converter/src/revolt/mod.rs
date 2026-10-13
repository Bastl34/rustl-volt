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

// the backwards variant of a level: levels/<level>/reversed with its own .fin / .taz / .pan / .fan, the world is the same
pub const REVERSED: &str = "reversed";
// the game's mirror mode: no files of its own, the converter mirrors the level (left / right)
pub const MIRRORED: &str = "mirrored";

pub fn has_reversed(game_dir: &Path, level: &str) -> bool
{
    let dir = game_dir.join("levels").join(level).join(REVERSED);
    find_file(&dir, &format!("{level}.fin")).is_some() || find_file(&dir, &format!("{level}.taz")).is_some()
}

// a level is a folder in levels/ with a .w file named like the folder
pub fn list_levels(game_dir: &Path) -> Vec<String>
{
    let Ok(entries) = std::fs::read_dir(game_dir.join("levels")) else { return vec![]; };

    let mut levels: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter_map(|path| path.file_name().map(|name| name.to_string_lossy().to_string()))
        .filter(|name| find_file(&game_dir.join("levels").join(name), &format!("{name}.w")).is_some())
        .collect();

    levels.sort_by_key(|name| name.to_lowercase());
    levels
}

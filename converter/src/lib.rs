// converts the original Re-Volt files (Steam install) into engine formats (glb, png, ...) - used by the converter window and by the game

pub mod convert;
pub mod glb;
pub mod revolt;
pub mod steam;

use std::path::Path;

use revolt::{has_reversed, list_levels, MIRRORED, REVERSED};

// a Re-Volt install: levels/ and cars/ with at least one level and one car
pub fn is_game_dir(dir: &Path) -> bool
{
    !list_levels(dir).is_empty() && !convert::cars::list(dir).is_empty()
}

// every level with its variants (race tracks: reversed, mirrored, both) and every car - what the game converts
pub fn everything(game_dir: &Path) -> (Vec<String>, Vec<String>)
{
    let levels = list_levels(game_dir).into_iter().flat_map(|id|
    {
        let variants = match has_reversed(game_dir, &id)
        {
            true => vec![format!("{id}/{REVERSED}"), format!("{id}/{MIRRORED}"), format!("{id}/{REVERSED}/{MIRRORED}")],
            false => vec![],
        };
        std::iter::once(id).chain(variants)
    }).collect();
    let cars = convert::cars::list(game_dir).into_iter().map(|(id, _)| id).collect();

    (levels, cars)
}

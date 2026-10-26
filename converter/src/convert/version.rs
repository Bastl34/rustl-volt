// assets/convert.json: which converter wrote the assets - the last run and every part (shared files, each level, each car) with its version and time
// a run that converts only some levels keeps the entries of the others from the file, so the oldest part shows what needs a re-convert
// the game reads it (code/src/assets.rs) and compares asset_version with the one it was written for

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde_json::{json, Map, Value};

use super::{Log, Settings};

// bump it when the output changes (new files, other values, other layout) - with a line in the history below
// 1: first versioned run (2026-10-13): levels, cars, HUD pages, loading screens, sounds, music, countdown
// 2: car bodies: polygons without NOENV in an own `_env` material (revolt_env) for the env map of the game
// 3: mirrored race tracks (<level>[/reversed]/mirrored -> levels/<level>_mirrored, <level>[_reversed]_mirrored.scene)
// 4: assets/models/firework.glb, putty_bomb.glb, burst.glb (the podium ceremony)
// 5: sounds as 16 bit PCM wav instead of ogg (no vorbis encoder in the game), the converter runs inside the game
// 6, 7, 8: no output change - bumped to show the conversion at the start of the game (video)
// 9: INSTANCERGBPER / MODELRGBPER of the .inf on instances and physics objects (ship2, toylite, wild_west1 darker)
// 10: tumbleweeds are light sphere bodies (car textures without mipmaps - undone in 11)
// 11: springs / axles stretched to the wheel center (revolt_length_m), pin offsets, car mipmaps on again
// 12: car aerials in own `_solid` materials without alpha mask (the mite aerial was cut away by the color key)
// 13: gfx/font.png is the font of the project (ui/font.png: RVGL's font.bmo, 512 x 512) - the font.bmp of the re-volt.io download had its glyphs a texel higher
// 14: level big cubes merged into 80 m cells (muse2 460 -> ~120 primitives), car hull spheres as JSON (revolt_spheres) instead of empty nodes
// 15: car aerials in 4 pieces on joints (aerial_joint_N, the app bends them), engine sounds like the game (wincar4 clockwork, class 1 and 2 petrol), .inf numbers with commas (bot_bat STARTPOS: start car)
// 16: models/lightning.glb (the arc of battle tag, the green strip of fxpage1), lightning_blue.glb (electro pulse)
// 17: again for 16 - converted with 16 before the green strip was in, lightning.glb was still the opaque blue-white beam
// 18: models/water_balloon.glb (wbomb.m) for the weapons
// 19: models/ball_bearing.glb (ball.m in gray), models/shock_sprite.glb (the blue web of fxpage1)
// 20: models/fxpage1.png (the skins of turbo, zap and bomb)
// 21: the start grid of Clockwork Carnage (revolt_grid "clockwork", 30 slots of the game)
// 22: the grid floor of cars.scene (assets/cars/showroom_floor.glb)
// 23: models/sparks.png (fxpage1 with black see through) for the particles
// 24: models/sparks.png with the sprite tiles faded out round
// 25: object throwers: the model of the thrown object (revolt_thrown_source), the thrower triggers (revolt_track_throws)
// 26: lighter ABC blocks (mass 0.25 instead of the game's 0.8)
// 27: ABC blocks lighter again (mass 0.08)
// 28: physics_test.scene (the markar arena + objects of the other levels)
// 29: physics_test.scene on the showroom floor, only the start car
// 30: the balls (sphere bodies) without the tilt of the .fob
// 31: the balls keep their .fob tilt again (the floating was the ground plane recovery, engine fix)
// 32: the .lit baked into every level (not only the frontend) without the square shadows (type 4) and not into pickups / stars, the runtime lights as revolt_lights
// 33: levels without REDBOOK (Market Arena) get the music range 4 9 of the supermarket tracks
pub const ASSET_VERSION: u32 = 33;
pub const FILE: &str = "convert.json";
const FORMAT: u32 = 1;

pub struct RunInfo<'a>
{
    pub levels: &'a [String],
    pub cars: &'a [String],
    pub seconds: f32,
    pub errors: usize,
    pub warnings: usize,
}

pub fn write(settings: &Settings, run: &RunInfo, log: &Log) -> Result<()>
{
    let path = settings.out_dir.join(FILE);
    let mut file = read(&path);
    let now = now();
    let part = json!({ "asset_version": ASSET_VERSION, "converter": env!("CARGO_PKG_VERSION"), "converted_at": now });

    file.insert("format".into(), json!(FORMAT));
    file.insert("asset_version".into(), json!(ASSET_VERSION));
    file.insert("converter".into(), json!(env!("CARGO_PKG_VERSION")));
    file.insert("last_run".into(), json!(
    {
        "converted_at": now,
        "game_dir": settings.game_dir.display().to_string(),
        "levels": run.levels,
        "cars": run.cars,
        "seconds": (run.seconds * 10.0).round() / 10.0,
        "errors": run.errors,
        "warnings": run.warnings,
        "settings": { "scale": settings.scale, "unlit": settings.unlit, "uv_inset": settings.uv_inset, "start_car": settings.start_car },
    }));
    file.insert("shared".into(), part.clone());
    for (key, names) in [("levels", run.levels), ("cars", run.cars)]
    {
        let entries = file.entry(key).or_insert_with(|| json!({}));
        if !entries.is_object() { *entries = json!({}); }
        let entries = entries.as_object_mut().expect("an object");
        for name in names
        {
            entries.insert(name.to_lowercase(), part.clone());
        }
    }

    // the oldest part decides whether the game wants a re-convert
    let oldest = parts(&file).filter_map(|part| part["asset_version"].as_u64()).min().unwrap_or(ASSET_VERSION as u64);
    file.insert("oldest_asset_version".into(), json!(oldest));

    std::fs::create_dir_all(&settings.out_dir)?;
    std::fs::write(&path, serde_json::to_string_pretty(&Value::Object(file))?)?;
    log.write(&path);
    log.info(format!("  asset version {ASSET_VERSION} (oldest part {oldest})"));
    Ok(())
}

// the file of the earlier runs - empty if there is none or it does not parse
fn read(path: &Path) -> Map<String, Value>
{
    std::fs::read_to_string(path).ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
}

fn parts(file: &Map<String, Value>) -> impl Iterator<Item = &Value>
{
    let groups = ["levels", "cars"].into_iter().filter_map(|key| file.get(key)?.as_object()).flat_map(|entries| entries.values());
    file.get("shared").into_iter().chain(groups)
}

// UTC as 2026-10-13T12:34:56Z - no date crate for one timestamp
fn now() -> String
{
    let seconds = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let (days, rest) = ((seconds / 86400) as i64, seconds % 86400);

    // civil date from days since 1970-01-01 (H. Hinnant)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };

    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", rest / 3600, rest % 3600 / 60, rest % 60)
}

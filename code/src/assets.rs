// assets/convert.json of the converter (converter/src/convert/version.rs): which asset version the converted files have
// the app code is written for ASSET_VERSION - older assets work as far as they go, the log asks for a re-convert
// setup.rs converts at the start when it is older or missing

use rustl::prelude::*;
use rustl::resources::resources::load_binary;
use rustl::serde_json::{self, Value};

// the asset version of the converter this code expects - raise it together with converter::version::ASSET_VERSION
pub const ASSET_VERSION: u64 = 14;
const FILE: &str = "convert.json";

pub struct AssetInfo
{
    // the oldest part (shared files, a level, a car) - None: written before the converter wrote the file
    pub oldest: Option<u64>,
    pub converter: String,
    pub converted_at: String,
    // the Re-Volt folder of the last run
    pub game_dir: String,
}

impl AssetInfo
{
    pub fn load(assets: &str) -> Option<Self>
    {
        let bytes = load_binary(&format!("{assets}/{FILE}")).ok()?;
        let file: Value = serde_json::from_slice(&bytes).ok()?;
        Some(Self
        {
            oldest: file["oldest_asset_version"].as_u64(),
            converter: file["converter"].as_str().unwrap_or_default().to_string(),
            converted_at: file["last_run"]["converted_at"].as_str().unwrap_or_default().to_string(),
            game_dir: file["last_run"]["game_dir"].as_str().unwrap_or_default().to_string(),
        })
    }

    pub fn outdated(&self) -> bool
    {
        self.oldest.is_none_or(|oldest| oldest < ASSET_VERSION)
    }
}

// once per run: what the assets are and whether they are older than this code
pub fn check(assets: &str)
{
    match AssetInfo::load(assets)
    {
        Some(info) if info.outdated() => { console_warning!("assets: version {} (converter {}, {}) - this code expects {} - re-convert", info.oldest.map_or("?".to_string(), |v| v.to_string()), info.converter, info.converted_at, ASSET_VERSION); },
        Some(info) => { console_log!("assets: version {} (converter {}, {})", info.oldest.unwrap_or_default(), info.converter, info.converted_at); },
        None => { console_warning!("assets: no {assets}/{FILE} - converted before the asset version existed, re-convert"); },
    }
}

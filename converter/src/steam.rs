// finds the Re-Volt install: registry -> Steam libraries -> app manifest

use std::path::{Path, PathBuf};

const APP_ID: u32 = 287310;

pub fn find_revolt() -> Option<PathBuf>
{
    let steam = steam_path()?;
    let vdf = std::fs::read_to_string(steam.join("steamapps").join("libraryfolders.vdf")).unwrap_or_default();

    let mut libraries = vec![steam];
    libraries.extend(quoted_values(&vdf, "path").into_iter().map(PathBuf::from));

    libraries.iter().find_map(|library| install_dir(library))
}

// HKCU\Software\Valve\Steam\SteamPath via reg.exe - no extra dependency
#[cfg(windows)]
fn steam_path() -> Option<PathBuf>
{
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    // CREATE_NO_WINDOW: the game has no console - reg.exe would open one
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let output = Command::new("reg").args(["query", r"HKCU\Software\Valve\Steam", "/v", "SteamPath"]).creation_flags(CREATE_NO_WINDOW).output().ok();
    let text = output.map(|output| String::from_utf8_lossy(&output.stdout).to_string()).unwrap_or_default();
    let registry = text.lines().find(|line| line.contains("SteamPath")).and_then(|line| line.split("REG_SZ").nth(1)).map(|path| PathBuf::from(path.trim()));

    registry.or_else(|| Some(PathBuf::from(r"C:\Program Files (x86)\Steam"))).filter(|path| path.is_dir())
}

// Linux (Proton) and macOS: the usual Steam folders
#[cfg(not(windows))]
fn steam_path() -> Option<PathBuf>
{
    let home = PathBuf::from(std::env::var_os("HOME")?);
    [".steam/steam", ".local/share/Steam", ".var/app/com.valvesoftware.Steam/.local/share/Steam", "Library/Application Support/Steam"].iter().map(|dir| home.join(dir)).find(|path| path.is_dir())
}

fn install_dir(library: &Path) -> Option<PathBuf>
{
    let apps = library.join("steamapps");
    let manifest = std::fs::read_to_string(apps.join(format!("appmanifest_{APP_ID}.acf"))).ok()?;
    let dir = apps.join("common").join(quoted_values(&manifest, "installdir").first()?);

    dir.is_dir().then_some(dir)
}

// all values of lines like: "key"  "value"
fn quoted_values(text: &str, key: &str) -> Vec<String>
{
    let key = format!("\"{key}\"");

    text.lines()
        .filter_map(|line| line.trim().strip_prefix(key.as_str()))
        .map(|rest| rest.trim().trim_matches('"').replace("\\\\", "\\"))
        .collect()
}

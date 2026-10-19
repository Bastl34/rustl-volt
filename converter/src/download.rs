// the Re-Volt files of re-volt.io (the community of RVGL) for players without the Steam version - extracted into one folder, the game folder of the conversion
// game_files.zip: the files of the game (same bytes as Steam) without gfx/font.bmp (the project has its own) and the car box pictures - those come from rvgl_assets.zip, the music from soundtrack.zip (Dreamcast)

use std::fs::File;
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use zip::ZipArchive;

use crate::convert::Log;

pub const SITE: &str = "https://re-volt.io/downloads/misc";
// below the user data folder of the game (the engine's save_folder), next to the save data
const GAME_FOLDER: &str = "rustl-volt";
const FOLDER: &str = "revolt_io";
// the finished packs and the downloads in progress
const STATE_FOLDER: &str = ".downloads";
// a progress line on the log every 2 MB
const STEP_BYTES: u64 = 2_000_000;

struct Pack
{
    file: &'static str,
    url: &'static str,
    // the size when the server sends none - and the share of the pack in the progress
    size: u64,
    music: bool,
    keep: fn(&str) -> bool,
}

const PACKS: [Pack; 3] = [
    Pack { file: "game_files.zip", url: "https://distribute.re-volt.io/packs/game_files.zip", size: 57_370_269, music: false, keep: keep_all },
    Pack { file: "rvgl_assets.zip", url: "https://distribute.re-volt.io/packs/rvgl_assets.zip", size: 7_013_046, music: false, keep: keep_rvgl },
    Pack { file: "soundtrack.zip", url: "https://distribute.re-volt.io/packs/soundtrack.zip", size: 108_541_343, music: true, keep: keep_music },
];

fn keep_all(_: &str) -> bool
{
    true
}

// the car box pictures (same bytes as Steam) and the license of the pack - the font is in the project (ui/font.png)
fn keep_rvgl(name: &str) -> bool
{
    matches!(name, "cars/misc/carbox1.bmp" | "cars/misc/carbox2.bmp" | "cars/misc/carbox3.bmp" | "licenses/license.rvgl-redist.txt")
}

// redbook/trackNN.ogg - the converter looks there when music/ is missing
fn keep_music(name: &str) -> bool
{
    name.starts_with("redbook/")
}

// <user data>/rustl-volt/revolt_io - None if the system has no user folder
pub fn default_dir() -> Option<PathBuf>
{
    let env = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty()).map(PathBuf::from);
    let base = if cfg!(target_os = "windows")
    {
        env("APPDATA")
    }
    else if cfg!(target_os = "macos")
    {
        env("HOME").map(|home| home.join("Library").join("Application Support"))
    }
    else
    {
        env("XDG_DATA_HOME").or_else(|| env("HOME").map(|home| home.join(".local").join("share")))
    }?;

    Some(base.join(GAME_FOLDER).join(FOLDER))
}

// every pack of the run is extracted already
pub fn is_complete(dir: &Path, music: bool) -> bool
{
    packs(music).all(|pack| done_marker(dir, pack).is_file())
}

// the megabytes of the packs that are not there yet
pub fn missing_megabytes(dir: &Path, music: bool) -> u64
{
    packs(music).filter(|pack| !done_marker(dir, pack).is_file()).map(|pack| pack.size).sum::<u64>().div_ceil(1_000_000)
}

fn packs(music: bool) -> impl Iterator<Item = &'static Pack>
{
    PACKS.iter().filter(move |pack| music || !pack.music)
}

fn done_marker(dir: &Path, pack: &Pack) -> PathBuf
{
    dir.join(STATE_FOLDER).join(format!("{}.done", pack.file))
}

// downloads and extracts the missing packs into dir - for a worker thread, ends with Message::Done; errors on the log
pub fn run(dir: PathBuf, music: bool, log: Log)
{
    let started = std::time::Instant::now();
    let packs: Vec<&Pack> = packs(music).collect();
    let total = packs.iter().map(|pack| pack.size).sum::<u64>().max(1) as f32;

    log.heading(format!("Download from re-volt.io ({SITE})"));
    log.info(format!("  into {}", dir.display()));
    let mut before = 0u64;
    for pack in packs
    {
        let progress = |bytes: u64| log.progress((before + bytes.min(pack.size)) as f32 / total);
        if let Err(err) = get_pack(&dir, pack, &log, &progress)
        {
            log.error(format!("{}: {err:#}", pack.file));
            break;
        }
        before += pack.size;
    }

    log.progress(1.0);
    log.heading("Done");
    log.info(format!("  {:.1} s", started.elapsed().as_secs_f32()));
    log.done();
}

fn get_pack(dir: &Path, pack: &Pack, log: &Log, progress: &dyn Fn(u64)) -> Result<()>
{
    let marker = done_marker(dir, pack);
    if marker.is_file()
    {
        log.info(format!("  {} already there", pack.file));
        return Ok(());
    }
    let state = dir.join(STATE_FOLDER);
    std::fs::create_dir_all(&state).with_context(|| format!("can not create {}", state.display()))?;

    // a complete download of an earlier run is only extracted again
    let zip = state.join(pack.file);
    if !zip.is_file()
    {
        log.step(format!("{}  ·  connecting", pack.file));
        let part = state.join(format!("{}.part", pack.file));
        download(pack, &part, log, progress)?;
        std::fs::rename(&part, &zip)?;
    }
    progress(pack.size);

    log.step(format!("{}  ·  extracting", pack.file));
    let mut archive = ZipArchive::new(File::open(&zip)?).with_context(|| zip.display().to_string())?;
    let count = extract(&mut archive, pack.file, dir, pack.keep, log)?;
    log.info(format!("  {}: {count} files extracted", pack.file));
    std::fs::write(&marker, format!("{}\n", pack.url))?;
    std::fs::remove_file(&zip)?;
    Ok(())
}

fn download(pack: &Pack, part: &Path, log: &Log, progress: &dyn Fn(u64)) -> Result<()>
{
    let mut response = ureq::get(pack.url).call().with_context(|| pack.url.to_string())?;
    let length = response.body().content_length().unwrap_or(pack.size);
    let mut reader = response.body_mut().as_reader();
    let mut file = File::create(part).with_context(|| part.display().to_string())?;

    let mut buffer = vec![0u8; 64 * 1024];
    let (mut received, mut reported) = (0u64, 0u64);
    loop
    {
        let read = reader.read(&mut buffer).with_context(|| format!("{} after {received} bytes", pack.url))?;
        if read == 0 { break; }
        file.write_all(&buffer[..read])?;
        received += read as u64;
        if received - reported >= STEP_BYTES
        {
            reported = received;
            // the share of the pack in the bar - the pack size is a guess, the server length counts
            progress(received * pack.size / length.max(1));
            log.step(format!("{}  ·  {:.0} / {:.0} MB", pack.file, received as f64 / 1e6, length as f64 / 1e6));
        }
    }
    file.flush()?;
    if received != length { bail!("{received} of {length} bytes received"); }
    log.info(format!("  {} downloaded ({:.1} MB)", pack.file, received as f64 / 1e6));
    Ok(())
}

// the entries keep wants (lowercase path with /) under their lowercase path, paths outside of dir are skipped
fn extract<R: Read + Seek>(archive: &mut ZipArchive<R>, source: &str, dir: &Path, keep: fn(&str) -> bool, log: &Log) -> Result<usize>
{
    let mut count = 0;
    for index in 0..archive.len()
    {
        let mut entry = archive.by_index(index)?;
        if entry.is_dir() { continue; }
        let Some(name) = entry.enclosed_name() else
        {
            log.warn(format!("  {source}: skipped entry {index} (outside of the folder)"));
            continue;
        };
        let name = name.to_string_lossy().replace('\\', "/").to_lowercase();
        if !keep(&name) { continue; }

        // lowercase like game_files.zip - one file per name also on case sensitive systems
        let out = dir.join(&name);
        if let Some(parent) = out.parent() { std::fs::create_dir_all(parent)?; }
        let mut file = File::create(&out).with_context(|| out.display().to_string())?;
        std::io::copy(&mut entry, &mut file)?;
        count += 1;
    }
    Ok(count)
}

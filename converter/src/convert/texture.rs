// bmp -> png, black is the color key for transparency

use std::io::Cursor;
use std::path::Path;

use anyhow::{Context, Result};
use image::{ImageFormat, RgbaImage};

use super::{Log, Settings};
use crate::revolt::find_file;

pub struct Page
{
    // file name without extension, lower case: market1a, trolley - names the textures and materials
    pub name: String,
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
    // any color keyed pixel
    pub has_alpha: bool,
}

pub fn load_page(path: &Path) -> Result<Page>
{
    let mut image: RgbaImage = image::open(path).with_context(|| format!("{}", path.display()))?.to_rgba8();

    let mut has_alpha = false;
    for pixel in image.pixels_mut()
    {
        if pixel[0] == 0 && pixel[1] == 0 && pixel[2] == 0
        {
            pixel[3] = 0;
            has_alpha = true;
        }
    }

    let mut png = vec![];
    image.write_to(&mut Cursor::new(&mut png), ImageFormat::Png)?;
    Ok(Page { name: stem(path), png, width: image.width(), height: image.height(), has_alpha })
}

fn stem(path: &Path) -> String
{
    path.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default()
}

// a bmp as it is, without the color key (sky images: black is part of the night sky)
pub fn load_plain(path: &Path) -> Result<Page>
{
    let image = image::open(path).with_context(|| format!("{}", path.display()))?.to_rgb8();
    let mut png = vec![];
    image.write_to(&mut Cursor::new(&mut png), ImageFormat::Png)?;
    Ok(Page { name: stem(path), png, width: image.width(), height: image.height(), has_alpha: false })
}

// sprites on black with hard edges (clouds): a soft edge inside the shape (blurred mask), the black around them in the mean sprite color - no dark fringe
pub fn load_soft_sprites(path: &Path, radius: i32) -> Result<Page>
{
    let mut image: RgbaImage = image::open(path).with_context(|| format!("{}", path.display()))?.to_rgba8();
    let (w, h) = (image.width() as i32, image.height() as i32);
    let mask: Vec<f32> = image.pixels().map(|p| if p[0] == 0 && p[1] == 0 && p[2] == 0 { 0.0 } else { 1.0 }).collect();

    // box blur, twice - roughly gaussian
    let blur = |source: &Vec<f32>| -> Vec<f32>
    {
        let mut horizontal = vec![0.0; source.len()];
        let mut result = vec![0.0; source.len()];
        for y in 0..h
        {
            for x in 0..w
            {
                let sum: f32 = (-radius..=radius).map(|d| source[(y * w + (x + d).clamp(0, w - 1)) as usize]).sum();
                horizontal[(y * w + x) as usize] = sum / (2 * radius + 1) as f32;
            }
        }
        for y in 0..h
        {
            for x in 0..w
            {
                let sum: f32 = (-radius..=radius).map(|d| horizontal[((y + d).clamp(0, h - 1) * w + x) as usize]).sum();
                result[(y * w + x) as usize] = sum / (2 * radius + 1) as f32;
            }
        }
        result
    };
    let soft = blur(&blur(&mask));

    let opaque: Vec<[u8; 4]> = image.pixels().filter(|p| p[0] != 0 || p[1] != 0 || p[2] != 0).map(|p| p.0).collect();
    let mean = [0, 1, 2].map(|c| (opaque.iter().map(|p| p[c] as u64).sum::<u64>() / opaque.len().max(1) as u64) as u8);

    for (i, pixel) in image.pixels_mut().enumerate()
    {
        // 0 at the edge of the shape, 1 a few pixels inside
        let alpha = ((soft[i] * 2.0 - 1.0) * mask[i]).clamp(0.0, 1.0);
        if mask[i] == 0.0 { pixel[0] = mean[0]; pixel[1] = mean[1]; pixel[2] = mean[2]; }
        pixel[3] = (alpha * 255.0) as u8;
    }

    let mut png = vec![];
    image.write_to(&mut Cursor::new(&mut png), ImageFormat::Png)?;
    Ok(Page { name: stem(path), png, width: image.width(), height: image.height(), has_alpha: true })
}

// a texture page for additive polygons as long as the engine has no additive blending: alpha = brightness, the color at full strength
// dark adds nothing in the game - here it becomes transparent, bright glows stay
pub fn additive_png(page_png: &[u8]) -> Result<Vec<u8>>
{
    let mut image = image::load_from_memory(page_png)?.to_rgba8();
    for pixel in image.pixels_mut()
    {
        let max = pixel[0].max(pixel[1]).max(pixel[2]);
        if max > 0
        {
            for c in 0..3 { pixel[c] = (pixel[c] as u32 * 255 / max as u32) as u8; }
        }
        pixel[3] = max;
    }

    let mut png = vec![];
    image.write_to(&mut Cursor::new(&mut png), ImageFormat::Png)?;
    Ok(png)
}

// texture pages <level>a.bmp, <level>b.bmp, ... - index 0 is 'a', one entry per page in use
pub fn load_pages(dir: &Path, level: &str, polygons_per_page: &[usize], log: &Log, progress: &dyn Fn(f32)) -> Vec<Option<Page>>
{
    polygons_per_page.iter().enumerate().map(|(page, polygons)|
    {
        progress(page as f32 / polygons_per_page.len() as f32);
        let file = format!("{}{}.bmp", level.to_lowercase(), (b'a' + page as u8) as char);
        let Some(path) = find_file(dir, &file) else
        {
            log.warn(format!("  texture {file} missing"));
            return None;
        };
        log.read(&path);

        match load_page(&path)
        {
            Ok(loaded) =>
            {
                let key = if loaded.has_alpha { "color key" } else { "" };
                log.info(format!("  {page}  {file:<14} {}×{} {polygons:>5} polygons  {key}", loaded.width, loaded.height));
                Some(loaded)
            },
            Err(err) =>
            {
                log.warn(format!("  texture {file}: {err:#}"));
                None
            },
        }
    }).collect()
}

// pages beyond a..j that some models use, hard coded in the exe (push page, push file)
// 24: the wire basket of the trolley (only in market1, the other levels take that one), 23 spru, 26 shadow, 27-29 fxpage1-3, 31 a page per level (toy2 dragon, toylite water)
pub fn load_extra_pages(game_dir: &Path, dir: &Path, level: &str, used: &[usize], pages: &mut Vec<Option<Page>>, log: &Log)
{
    let gfx = game_dir.join("gfx");
    let market1 = game_dir.join("levels").join("market1");
    for &page in used
    {
        let (folder, file) = match (page, level.to_lowercase().as_str())
        {
            (24 | 32, _) => (dir, "trolley.bmp"),
            (23, _) => (gfx.as_path(), "spru.bmp"),
            (26, _) => (gfx.as_path(), "shadow.bmp"),
            // the frontend loads the car box pictures over the fx pages
            (27, "frontend") => (dir, "carbox1.bmp"),
            (28, "frontend") => (dir, "carbox2.bmp"),
            (29, "frontend") => (dir, "carbox3.bmp"),
            (27, _) => (gfx.as_path(), "fxpage1.bmp"),
            (28, _) => (gfx.as_path(), "fxpage2.bmp"),
            (29, _) => (gfx.as_path(), "fxpage3.bmp"),
            (31, "toy2") => (dir, "dragon.bmp"),
            (31, "toylite") => (dir, "water.bmp"),
            _ =>
            {
                log.warn(format!("  page {page}: unknown file - untextured"));
                continue;
            },
        };
        // cars/misc: the car boxes of RVGL installs and re-volt.io downloads
        let Some(path) = find_file(folder, file).or_else(|| find_file(&market1, file)).or_else(|| find_file(&game_dir.join("cars").join("misc"), file)) else
        {
            log.warn(format!("  page {page}: {file} missing - untextured"));
            continue;
        };
        log.read(&path);

        match load_page(&path)
        {
            Ok(loaded) =>
            {
                log.info(format!("  {page}  {file:<14} {}×{} (not a level page){}", loaded.width, loaded.height, if loaded.has_alpha { "  color key" } else { "" }));
                if pages.len() <= page { pages.resize_with(page + 1, || None); }
                pages[page] = Some(loaded);
            },
            Err(err) => log.warn(format!("  page {page} {file}: {err:#}")),
        }
    }
}

// gfx/envstill.bmp (world), envroll.bmp (cars), envfront.bmp (ENVSTILL of some levels) as separate png files
pub fn convert_env_maps(settings: &Settings, log: &Log) -> Result<()>
{
    let gfx = settings.game_dir.join("gfx");
    let out = settings.out_dir.join("gfx");
    std::fs::create_dir_all(&out)?;

    for name in ["envstill", "envroll", "envfront"]
    {
        let source = find_file(&gfx, &format!("{name}.bmp")).with_context(|| format!("gfx/{name}.bmp missing"))?;
        let target = out.join(format!("{name}.png"));
        log.read(&source);
        image::open(&source)?.to_rgb8().save(&target)?;
        log.write(&target);
    }

    Ok(())
}

// the loading screens: gfx/loadlevel1-4.bmp (blue, before a level) and loadfront1-4.bmp (in color, before the frontend) are the 4 quarters of one 512 picture
// 1 top left, 2 top right, 3 bottom left, 4 bottom right
pub fn convert_loading_screens(settings: &Settings, log: &Log) -> Result<()>
{
    let gfx = settings.game_dir.join("gfx");
    let out = settings.out_dir.join("gfx");
    std::fs::create_dir_all(&out)?;

    for (name, target_name) in [("loadlevel", "loading_level.png"), ("loadfront", "loading_front.png")]
    {
        let mut picture = image::RgbImage::new(LOADING_TILE * 2, LOADING_TILE * 2);
        for quarter in 0..4u32
        {
            let source = find_file(&gfx, &format!("{name}{}.bmp", quarter + 1)).with_context(|| format!("gfx/{name}{}.bmp missing", quarter + 1))?;
            log.read(&source);
            let tile = image::open(&source)?.to_rgb8();
            image::imageops::replace(&mut picture, &tile, ((quarter % 2) * LOADING_TILE) as i64, ((quarter / 2) * LOADING_TILE) as i64);
        }
        let target = out.join(target_name);
        picture.save(&target)?;
        log.write(&target);
    }

    Ok(())
}

const LOADING_TILE: u32 = 256;


// the font of the game in the project folder (RVGL's font.bmo, ui/make_font.py) - the font.bmp of the game only without it
const PROJECT_FONT: &str = "ui/font.png";

// the pages of the HUD (revolt.exe draws it from them): font, fxpage2 (arrows, speed dots, weapon frame), spru (frames) - black and its dark gray rims are transparent
// the font: the one of the project, else font.bmp white on black - its brightness becomes the alpha, so tinted text has no dark fringe
pub fn convert_hud_pages(settings: &Settings, log: &Log) -> Result<()>
{
    let gfx = settings.game_dir.join("gfx");
    let out = settings.out_dir.join("gfx");
    std::fs::create_dir_all(&out)?;

    let project_font = settings.files_dir.join(PROJECT_FONT);
    let names: &[&str] = if project_font.is_file() { &["fxpage2", "spru"] } else { &["font", "fxpage2", "spru"] };
    if project_font.is_file()
    {
        let target = out.join("font.png");
        log.read(&project_font);
        std::fs::copy(&project_font, &target)?;
        log.write(&target);
    }
    for &name in names
    {
        let source = find_file(&gfx, &format!("{name}.bmp")).with_context(|| format!("gfx/{name}.bmp missing"))?;
        let target = out.join(format!("{name}.png"));
        log.read(&source);
        let mut image: RgbaImage = image::open(&source)?.to_rgba8();
        match name
        {
            "font" => for pixel in image.pixels_mut()
            {
                let brightness = pixel[0].max(pixel[1]).max(pixel[2]);
                *pixel = image::Rgba([255, 255, 255, brightness]);
            },
            _ => { clear_dark_rims(&mut image); soften_edges(&mut image); },
        }
        image.save(&target)?;
        log.write(&target);
    }

    // the title icons of the menu (5 per row, 48 texels): gray on black - the brightness becomes the alpha
    let frontend = settings.game_dir.join("levels").join("frontend");
    match find_file(&frontend, "frontendf.bmp")
    {
        Some(source) =>
        {
            let target = out.join("menu_icons.png");
            log.read(&source);
            let mut image: RgbaImage = image::open(&source)?.to_rgba8();
            for pixel in image.pixels_mut()
            {
                let alpha = pixel[0].max(pixel[1]).max(pixel[2]);
                let lift = |c: u8| if alpha == 0 { 0 } else { (c as u32 * 255 / alpha as u32) as u8 };
                *pixel = image::Rgba([lift(pixel[0]), lift(pixel[1]), lift(pixel[2]), alpha]);
            }
            image.save(&target)?;
            log.write(&target);
        },
        None => log.warn("  levels/frontend/frontendf.bmp missing - the menu has no title icons"),
    }

    Ok(())
}

// the rims around the sprites: (16, 16, 16), 1-2 pixels wide - deeper the dark spheres use the same gray for their shadow side
const RIM_MAX: u8 = 20;
const RIM_WIDTH: u32 = 2;

// black is transparent, and so is dark neutral gray up to RIM_WIDTH pixels from it - the rims
fn clear_dark_rims(image: &mut RgbaImage)
{
    let (w, h) = (image.width() as i32, image.height() as i32);
    let rim = |p: &image::Rgba<u8>| { let (max, min) = (p[0].max(p[1]).max(p[2]), p[0].min(p[1]).min(p[2])); max <= RIM_MAX && max - min <= 2 };

    let mut open = std::collections::VecDeque::new();
    for (x, y, pixel) in image.enumerate_pixels_mut()
    {
        if pixel[0] == 0 && pixel[1] == 0 && pixel[2] == 0
        {
            pixel[3] = 0;
            open.push_back((x as i32, y as i32, 0));
        }
    }
    while let Some((x, y, depth)) = open.pop_front()
    {
        if depth >= RIM_WIDTH { continue; }
        for (nx, ny) in [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)]
        {
            if nx < 0 || ny < 0 || nx >= w || ny >= h { continue; }
            let pixel = image.get_pixel_mut(nx as u32, ny as u32);
            if pixel[3] != 0 && rim(pixel)
            {
                pixel[3] = 0;
                open.push_back((nx, ny, depth + 1));
            }
        }
    }
}

// the outer pixels of a shape fade by their brightness - the dark anti aliasing around the sprites was a black fringe on the track
const EDGE_LAYERS: usize = 2;
const EDGE_FULL: f32 = 110.0;
// the faded color is brightened by 1 / alpha up to this - more made the edges of the dark sprites a light ring
const EDGE_BRIGHTEN: f32 = 2.0;

fn soften_edges(image: &mut RgbaImage)
{
    let (w, h) = (image.width() as i32, image.height() as i32);
    for _ in 0..EDGE_LAYERS
    {
        let edge: Vec<(u32, u32)> = image.enumerate_pixels()
            .filter(|(x, y, pixel)| pixel[3] == 255 && [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dy)|
            {
                let (nx, ny) = (*x as i32 + dx, *y as i32 + dy);
                nx >= 0 && ny >= 0 && nx < w && ny < h && image.get_pixel(nx as u32, ny as u32)[3] < 255
            }))
            .map(|(x, y, _)| (x, y))
            .collect();

        for (x, y) in edge
        {
            let pixel = image.get_pixel_mut(x, y);
            let alpha = (pixel[0].max(pixel[1]).max(pixel[2]) as f32 / EDGE_FULL).min(1.0);
            if alpha >= 1.0 { continue; }
            let brighten = (1.0 / alpha.max(0.001)).min(EDGE_BRIGHTEN);
            for channel in 0..3 { pixel[channel] = (pixel[channel] as f32 * brighten).min(255.0) as u8; }
            pixel[3] = (alpha * 254.0) as u8;
        }
    }
}

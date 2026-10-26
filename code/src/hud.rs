// the race HUD of the game (formats/README.md, HUD): its pages font / fxpage2, own boxes and speed dots, its positions on a 640 x 480 screen
// scaled with the height; right and centered items keep their distance to the right edge / the center (the game adds the extra width the same way)

use std::f32::consts::FRAC_PI_4;

use rustl::egui::{self, Color32, Mesh, Pos2, Rect, TextureHandle, TextureOptions};
use rustl::prelude::*;
use rustl::resources::resources::load_binary;
use rustl::serde_json;

use crate::race::{format_time, Neighbour, Race, Racer, SIGN_TIME};
use crate::weapons::{Hand, Weapons, WEAPONS};

pub const WIDTH: f32 = 640.0;
pub const HEIGHT: f32 = 480.0;

pub const CYAN: Color32 = Color32::from_rgb(0, 255, 255);
pub const WHITE: Color32 = Color32::WHITE;
const AMMO: Color32 = Color32::from_rgb(255, 170, 34);
// battle tag: the star of the holder (own design) - inner, outer, rim; the name of the holder
const STAR_COLORS: [Color32; 3] = [Color32::from_rgb(0xff, 0xfa, 0xc8), Color32::from_rgb(0xff, 0xb8, 0x00), Color32::from_rgb(0x80, 0x48, 0x00)];
const HOLDER: Color32 = Color32::from_rgb(255, 64, 32);
// game metres per metre of ours (like the cars ahead / behind)
const BATTLE_METRES: f32 = 0.5;
// sprite alpha, the weapon frame a bit more opaque
const SPRITE_ALPHA: u8 = 0xC0;
const FRAME_ALPHA: u8 = 0xE0;

// font.bmp: 32 glyphs of 8 x 16 texels per row from '!', the glyph is 7 wide - in units of the 256 page, the font of the project (ui/font.png) is 512 (the game reads v 1..17)
const GLYPH_FIRST: u32 = 0x21;
const GLYPHS_PER_ROW: u32 = 32;

// fxpage2 texels: up arrow, wrong way X, weapon frame
const ARROW_UV: [f32; 4] = [2.0, 66.0, 60.0, 60.0];
const WRONG_WAY_UV: [f32; 4] = [129.0, 65.0, 62.0, 62.0];
const WEAPON_FRAME_UV: [f32; 4] = [190.0, 157.0, 64.0, 64.0];
// the direction signs, a negative width mirrors the right arrows to the left ones
const SIGN_UV: [[f32; 4]; 11] =
[
    [127.0, 1.0, -62.0, 62.0], [63.0, 1.0, -62.0, 62.0], [126.0, 65.0, -61.0, 62.0], [255.0, 1.0, -62.0, 62.0], [65.0, 1.0, 62.0, 62.0], [1.0, 1.0, 62.0, 62.0],
    [65.0, 65.0, 62.0, 62.0], [193.0, 1.0, 62.0, 62.0], [1.0, 65.0, 62.0, 62.0], [129.0, 1.0, 62.0, 62.0], [193.0, 65.0, 62.0, 62.0],
];
// the 7 speed dots of the game (top left and size on screen, right anchored), drawn as own balls: the dark and the lit color of each (from fxpage2)
const DOT_POSITIONS: [[f32; 2]; 7] = [[492.0, 451.0], [502.0, 435.0], [517.0, 422.0], [536.0, 412.0], [558.0, 405.0], [581.0, 400.0], [606.0, 399.0]];
const DOT_SIZES: [f32; 7] = [14.0, 16.0, 18.0, 19.0, 21.0, 22.0, 23.0];
const DOT_DARK: [[u8; 3]; 7] = [[92, 4, 2], [87, 8, 0], [84, 22, 0], [88, 44, 0], [49, 77, 6], [22, 52, 10], [6, 52, 15]];
const DOT_LIT: [[u8; 3]; 7] = [[255, 60, 20], [255, 106, 22], [255, 190, 30], [255, 245, 40], [210, 240, 70], [140, 228, 50], [45, 208, 30]];
// a dot lights up in 0.25 s and goes out in 1 s
const DOT_RISE: f32 = 4.0;
const DOT_FALL: f32 = 1.0;
// the blue boxes of the HUD (the colors of the frame on spru.bmp): dark fill, blue rim, a light inner line
const PANEL_FILL: Color32 = Color32::from_rgba_premultiplied(4, 10, 34, 0xb0);
const PANEL_RIM: Color32 = Color32::from_rgb(0x1c, 0x2c, 0xa8);
const PANEL_LIGHT: Color32 = Color32::from_rgb(0x64, 0x78, 0xfc);
const PANEL_RIM_WIDTH: f32 = 2.5;
const PANEL_RADIUS: f32 = 8.0;
// the line between the cells of split screen, px
const CELL_LINE: f32 = 2.0;
// the version of the project bottom right: char size, white
const VERSION_W: f32 = 5.0;
const VERSION_H: f32 = 7.5;
const VERSION_COLOR: Color32 = Color32::WHITE;
// the 3D countdown of the game (models/go*.m), drawn by the camera at (0, -32, 256) game units with a lens of 640 on the 640 x 480 screen
const COUNTDOWN_MODELS: [&str; 4] = ["gogo", "go1", "go2", "go3"];
const COUNTDOWN_LENS: f32 = 640.0;
const COUNTDOWN_HEIGHT: f32 = -32.0;
const COUNTDOWN_DEPTH: f32 = 256.0;
// turns per ms while a number flips in / out (the first 100 ms and the last 100 ms of its second)
const COUNTDOWN_FLIP: f32 = 0.0025;

// mph per m/s: the speedometer of the game counts 0.01118 mph per unit/s, 100 units per m
const MPH_PER_MPS: f32 = 100.0 * 0.01118;
// km/h per mph
const KMH_PER_MPH: f32 = 1.609344;

#[derive(Clone, Copy)]
pub enum Anchor
{
    Left,
    Center,
    Right,
}

struct Textures
{
    font: TextureHandle,
    fx: TextureHandle,
    // gogo, go1, go2, go3 - triangles in game units, empty if the file is missing
    countdown: [Vec<[(Vector3<f32>, Color32); 3]>; 4],
}

pub struct Hud
{
    // the assets folder of the project
    assets: String,
    textures: Option<Textures>,
    failed: bool,
    // 0..1 per speed dot, per racer
    dots: Vec<[f32; 7]>,
    clock: f64,
    // the hint below the results (not while the race menu is open)
    pub hint: bool,
    // "Rustl-Volt v0.0.1"
    pub version: String,
    // a message in the middle and its seconds left (practice: the star found)
    pub message: Option<(String, f32)>,
    // mirror mode: the game mirrors the whole picture, the countdown too
    mirrored: bool,
    // the results after the finish - off while the championship shows its table
    pub results: bool,
    // over the 3D: the reticles of the fireworks (normalized, y down; green on the own target, red on the own car while aimed at), the flash 0..1
    pub reticles: Vec<([f32; 2], bool)>,
    pub flash: f32,
    // the lightning of the weather 0..1
    pub lightning: f32,
    // Video Settings: Show FPS
    pub show_fps: bool,
}

// draws in the 640 x 480 space of the game, scaled with the height - one mesh per texture page
pub struct Painter
{
    pub rect: Rect,
    pub scale: f32,
    // px the rect is higher than the 480 of the game at this scale - the lower half of the screen moves down by it
    pub extra: f32,
    pub meshes: Vec<Mesh>,
    // drawn below the meshes: rounded boxes
    pub shapes: Vec<egui::Shape>,
}

impl Hud
{
    pub fn new(scene: &Scene) -> Self
    {
        let source = scene.source.as_ref().map(|source| source.origin_path.replace('\\', "/")).unwrap_or_default();
        let folder = source.rsplit_once('/').map(|(folder, _)| folder.to_string()).unwrap_or_default();
        let assets = if folder.is_empty() { "assets".to_string() } else { format!("{folder}/assets") };
        Self { assets, textures: None, failed: false, dots: vec![], clock: 0.0, hint: true, version: String::new(), message: None, results: true, reticles: vec![], flash: 0.0, lightning: 0.0, mirrored: scene.name.to_lowercase().ends_with("_mirrored"), show_fps: true }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, race: &Race, weapons: Option<&Weapons>, fps: u32, dt: f32)
    {
        self.clock += dt as f64;
        self.dots.resize(race.racers.len(), [0.0; 7]);
        for (index, racer) in race.racers.iter().enumerate()
        {
            self.update_dots(index, racer.speed_steps, dt);
        }

        if self.textures.is_none() && !self.failed
        {
            match self.load(ui.ctx())
            {
                Ok(textures) => self.textures = Some(textures),
                Err(error) =>
                {
                    console_warning!("hud: {} - re-convert to get the HUD pages", error);
                    self.failed = true;
                },
            }
        }
        let Some(textures) = &self.textures else { return; };

        let rect = ui.ctx().content_rect();
        let mut painter = Painter::new(rect, vec![textures.font.id(), textures.fx.id()]);
        if let Some((_, left)) = self.message.as_mut()
        {
            *left -= dt;
        }
        self.message = self.message.take().filter(|(_, left)| *left > 0.0);

        // split view: only place, name and lap in every viewport - split screen: the whole HUD in the viewport of each player; the countdown and the results over all of them
        if !race.split_views().is_empty()
        {
            for &(index, _, [x, y, w, h]) in race.split_views()
            {
                let cell = cell_rect(rect, [x, y, w, h]);
                // the split view (V) keeps its labels as big as the height allows
                match race.multi()
                {
                    true => painter.set_rect(cell),
                    false => { painter.rect = cell; painter.scale = cell.height() / HEIGHT; painter.extra = 0.0; },
                }
                match race.multi() && race.racers[index].is_local()
                {
                    // split screen: the fps in the HUD of player 1 (bottom left) - top right it covered the times of the right cell
                    true => self.draw_racer(&mut painter, race, index, weapons, (self.show_fps && index == race.player_index()).then_some(fps)),
                    false => painter.split_label(&race.racers[index], race.settings.laps, race.battle.as_ref().map(|battle| battle.clock(index))),
                }
                if race.multi() { painter.shapes.push(egui::Shape::rect_stroke(cell, 0.0, egui::Stroke::new(CELL_LINE, Color32::BLACK), egui::StrokeKind::Inside)); }
            }
            // split screen without bots: the cells nobody has stay black
            for view in race.empty_cells()
            {
                painter.shapes.push(egui::Shape::rect_filled(cell_rect(rect, *view), 0.0, Color32::BLACK));
            }
            painter.set_rect(rect);
            // the split view (V): fps top right over all viewports
            if self.show_fps && !race.multi()
            {
                painter.text(Anchor::Right, 560.0, 8.0, 8.0, 16.0, CYAN, "fps");
                painter.text(Anchor::Right, 592.0, 8.0, 8.0, 16.0, WHITE, &fps.to_string());
            }
            if let Some((text, _)) = &self.message
            {
                message(&mut painter, text, message_y(race));
            }
            if self.results && (race.over || race.locals_finished()) { painter.results(race, self.hint); }
            painter.version(&self.version);
            let countdown = painter.countdown(race, &textures.countdown, self.mirrored);
            painter.meshes.push(countdown);
            self.overlays(&mut painter, rect);
            painter.finish_all(ui.ctx());
            return;
        }

        if let Some((text, _)) = &self.message
        {
            message(&mut painter, text, message_y(race));
        }

        let racer = race.viewed();
        // practice like the game: no times, laps, position, wrong way, signs or arrow - the speed, fps and the countdown
        if race.settings.practice
        {
            self.draw_speed(&mut painter, race.viewed_index(), racer.speed, race.settings.kmh);
            if self.show_fps && !race.multi()
            {
                painter.text(Anchor::Left, 16.0, 392.0, 8.0, 12.0, CYAN, "fps");
                painter.text(Anchor::Left, 16.0, 404.0, 8.0, 12.0, WHITE, &fps.to_string());
            }
            painter.version(&self.version);
            let countdown = painter.countdown(race, &textures.countdown, self.mirrored);
            painter.meshes.push(countdown);
            self.overlays(&mut painter, rect);
            painter.finish_all(ui.ctx());
            return;
        }

        // the power up test: no laps, times or place - the speed, the weapon and fps
        if race.settings.playground
        {
            self.draw_speed(&mut painter, race.viewed_index(), racer.speed, race.settings.kmh);
            painter.sprite(1, Anchor::Left, [16.0, 56.0, 64.0, 64.0], WEAPON_FRAME_UV, alpha(FRAME_ALPHA));
            if let Some(hand) = weapons.and_then(|weapons| weapons.hand(race.viewed_index())) { painter.weapon(hand); }
            if self.show_fps && !race.multi()
            {
                painter.text(Anchor::Left, 16.0, 392.0, 8.0, 12.0, CYAN, "fps");
                painter.text(Anchor::Left, 16.0, 404.0, 8.0, 12.0, WHITE, &fps.to_string());
            }
            painter.version(&self.version);
            self.overlays(&mut painter, rect);
            painter.finish_all(ui.ctx());
            return;
        }

        self.draw_racer(&mut painter, race, race.viewed_index(), weapons, self.show_fps.then_some(fps));

        // watching another car
        if race.watching().is_some()
        {
            let text = format!("Watching {} - B: next car", racer.name);
            painter.text(Anchor::Center, 320.0 - text.len() as f32 * 8.0 * 0.5, 88.0, 8.0, 16.0, CYAN, &text);
        }

        // the places at the end - own layout, the finish display of the game is not decoded
        if self.results && (race.over || racer.finished())
        {
            painter.results(race, self.hint);
        }

        painter.version(&self.version);
        let countdown = painter.countdown(race, &textures.countdown, self.mirrored);
        painter.meshes.push(countdown);
        self.overlays(&mut painter, rect);
        painter.finish_all(ui.ctx());
    }

    // the HUD of one racer in the rect of the painter: lap, weapon, times, wrong way, sign, place, arrow, speed, the cars around - fps if given
    fn draw_racer(&self, painter: &mut Painter, race: &Race, index: usize, weapons: Option<&Weapons>, fps: Option<u32>)
    {
        if race.battle.is_some()
        {
            self.draw_battle(painter, race, index, weapons, fps);
            return;
        }
        let racer = &race.racers[index];
        // top left: lap, weapon frame
        painter.text(Anchor::Left, 16.0, 16.0, 8.0, 16.0, CYAN, "Lap");
        let laps = race.settings.laps;
        painter.text(Anchor::Left, 16.0, 32.0, 8.0, 16.0, WHITE, &format!("{}/{}", (racer.laps + 1).min(laps), laps));
        painter.sprite(1, Anchor::Left, [16.0, 56.0, 64.0, 64.0], WEAPON_FRAME_UV, alpha(FRAME_ALPHA));
        if let Some(weapons) = weapons
        {
            if let Some(hand) = weapons.hand(index) { painter.weapon(hand); }
        }
        // split screen: the name of the player under the weapon
        if race.multi()
        {
            painter.text(Anchor::Left, 16.0, 128.0, 8.0, 16.0, AMMO, &racer.name.chars().take(16).collect::<String>());
        }

        // top right: the times, labels right aligned at 624, values at 552
        let lap = if racer.finished() { 0.0 } else { racer.lap_time(race.race_time) };
        let times = [("Last Lap", 16.0, racer.last_lap.unwrap_or(0.0)), ("Best Lap", 56.0, racer.best_lap.unwrap_or(0.0)), ("Lap", 96.0, lap), ("Race", 136.0, racer.finish_time.unwrap_or(race.race_time))];
        for (label, y, time) in times
        {
            painter.text(Anchor::Right, 624.0 - label.len() as f32 * 8.0, y, 8.0, 16.0, CYAN, label);
            painter.text(Anchor::Right, 552.0, y + 16.0, 8.0, 16.0, WHITE, &format_time(time));
        }

        // wrong way: the red X top center, blinking (256 ms on / off)
        if racer.wrong_way && !racer.finished() && (self.clock * 1000.0) as u64 & 256 != 0
        {
            painter.sprite(1, Anchor::Center, [288.0, 16.0, 64.0, 64.0], WRONG_WAY_UV, alpha(SPRITE_ALPHA));
        }

        // the direction sign of the last trigger at the same place, not while wrong way shows: fades in above 512, out below 128 of its timer
        if let Some(sign) = racer.sign.filter(|_| !racer.wrong_way && !racer.finished())
        {
            let t = racer.sign_timer;
            let fade = if t > 512.0 { (SIGN_TIME - t) * 2.0 } else if t < 128.0 { t * 2.0 } else { 255.0 };
            painter.sprite(1, Anchor::Center, [288.0, 16.0, 64.0, 64.0], SIGN_UV[sign.min(SIGN_UV.len() - 1)], alpha(fade.clamp(0.0, 255.0) as u8));
        }

        // bottom left: position, direction arrow, fps
        let position = racer.position;
        painter.text(Anchor::Left, 16.0, 392.0, 12.0, 24.0, WHITE, &position.to_string());
        painter.text(Anchor::Left, 28.0 + position.to_string().len() as f32 * 12.0, 392.0, 8.0, 12.0, CYAN, suffix(position));
        painter.arrow([96.0, 396.0], 32.0, racer.arrow, alpha(SPRITE_ALPHA));
        if let Some(fps) = fps
        {
            painter.text(Anchor::Left, 144.0, 392.0, 8.0, 12.0, CYAN, "fps");
            painter.text(Anchor::Left, 144.0, 404.0, 8.0, 12.0, WHITE, &fps.to_string());
        }

        // bottom right: speed dots and speed
        self.draw_speed(painter, index, racer.speed, race.settings.kmh);

        // the cars ahead and behind - none without opponents
        painter.neighbours(racer.ahead.as_ref(), racer.behind.as_ref());

        // split screen: a player through shows place and time in the own cell until the results come (above the middle - tall cells stretch below it)
        if race.multi() && racer.finished() && !(race.over || race.locals_finished())
        {
            let place = format!("{}{}", racer.position, suffix(racer.position));
            let time = format_time(racer.finish_time.unwrap_or(0.0));
            painter.blue_box(Anchor::Center, [200.0, 112.0, 240.0, 116.0]);
            painter.text(Anchor::Center, 320.0 - place.len() as f32 * 8.0, 120.0, 16.0, 32.0, AMMO, &place);
            painter.text(Anchor::Center, 320.0 - 8.0 * 6.0, 158.0, 12.0, 24.0, CYAN, "Finished");
            painter.text(Anchor::Center, 320.0 - time.len() as f32 * 4.0, 192.0, 8.0, 16.0, WHITE, &time);
        }
    }

    // battle tag (own layout, the game shows the own clock and the holder with his clock): top left the own clock and the weapon,
    // top right who has the star, a star top center while this car has it, bottom left the place, bottom right the speed
    fn draw_battle(&self, painter: &mut Painter, race: &Race, index: usize, weapons: Option<&Weapons>, fps: Option<u32>)
    {
        let Some(battle) = &race.battle else { return; };
        let racer = &race.racers[index];
        let holding = battle.holder == Some(index);
        painter.text(Anchor::Left, 16.0, 16.0, 8.0, 16.0, CYAN, "Time");
        painter.text(Anchor::Left, 16.0, 32.0, 8.0, 16.0, if holding { AMMO } else { WHITE }, &format_time(battle.clock(index)));
        painter.sprite(1, Anchor::Left, [16.0, 56.0, 64.0, 64.0], WEAPON_FRAME_UV, alpha(FRAME_ALPHA));
        if let Some(hand) = weapons.and_then(|weapons| weapons.hand(index)) { painter.weapon(hand); }
        if race.multi()
        {
            painter.text(Anchor::Left, 16.0, 128.0, 8.0, 16.0, AMMO, &racer.name.chars().take(16).collect::<String>());
        }

        // who has the star and his clock
        let label = "Star";
        painter.text(Anchor::Right, 624.0 - label.len() as f32 * 8.0, 16.0, 8.0, 16.0, CYAN, label);
        match battle.holder
        {
            Some(holder) =>
            {
                let name: String = race.racers[holder].name.chars().take(16).collect();
                painter.text(Anchor::Right, 624.0 - name.len() as f32 * 8.0, 32.0, 8.0, 16.0, if holding { AMMO } else { HOLDER }, &name);
                painter.text(Anchor::Right, 552.0, 48.0, 8.0, 16.0, WHITE, &format_time(battle.clock(holder)));
            },
            None => painter.text(Anchor::Right, 624.0 - 8.0 * 8.0, 32.0, 8.0, 16.0, WHITE, "nobody"),
        }

        // the own star, pulsing - else an arrow to the holder (red) or the nearest star (gold) with the distance in m of the game
        if holding && !racer.finished()
        {
            let pulse = 1.0 + 0.12 * (self.clock * 6.0).sin() as f32;
            let [inner, outer, rim] = STAR_COLORS;
            painter.star(Anchor::Center, [320.0, 44.0], 22.0 * pulse, inner, outer, rim);
        }
        else if let Some((turn, metres)) = battle.pointer(index, &race.racers).filter(|_| !race.over)
        {
            let color = if battle.holder.is_some() { Color32::from_rgba_unmultiplied(255, 72, 40, SPRITE_ALPHA) } else { Color32::from_rgba_unmultiplied(255, 200, 40, SPRITE_ALPHA) };
            painter.arrow([320.0, 44.0], 32.0, turn, color);
            let text = format!("{}m", (metres * BATTLE_METRES) as i32);
            painter.text(Anchor::Center, 320.0 - text.len() as f32 * 4.0, 84.0, 8.0, 16.0, WHITE, &text);
        }

        let position = racer.position;
        painter.text(Anchor::Left, 16.0, 392.0, 12.0, 24.0, WHITE, &position.to_string());
        painter.text(Anchor::Left, 28.0 + position.to_string().len() as f32 * 12.0, 392.0, 8.0, 12.0, CYAN, suffix(position));
        if let Some(fps) = fps
        {
            painter.text(Anchor::Left, 144.0, 392.0, 8.0, 12.0, CYAN, "fps");
            painter.text(Anchor::Left, 144.0, 404.0, 8.0, 12.0, WHITE, &fps.to_string());
        }
        self.draw_speed(painter, index, racer.speed, race.settings.kmh);
    }

    // the scale of the game's speedometer, in km/h or mph
    fn draw_speed(&self, painter: &mut Painter, index: usize, speed: f32, kmh: bool)
    {
        let dots = self.dots.get(index).copied().unwrap_or_default();
        for (i, fill) in dots.iter().enumerate()
        {
            let [x, y] = DOT_POSITIONS[i];
            let size = DOT_SIZES[i];
            painter.ball(Anchor::Right, [x + size * 0.5, y + size * 0.5], size * 0.5, DOT_DARK[i], DOT_LIT[i], *fill);
        }
        let (value, unit) = if kmh { (speed * MPH_PER_MPS * KMH_PER_MPH, "km/h") } else { (speed * MPH_PER_MPS, "mph") };
        painter.text(Anchor::Right, 560.0, 448.0, 8.0, 16.0, WHITE, &format!("{:4}", value as i32));
        painter.text(Anchor::Right, 600.0, 448.0, 8.0, 16.0, CYAN, unit);
    }

    // a dot below the speed step is full, the one at it shows the fraction - up fast, down slow
    fn update_dots(&mut self, index: usize, steps: f32, dt: f32)
    {
        let full = steps.floor();
        let Some(dots) = self.dots.get_mut(index) else { return; };
        for (i, fill) in dots.iter_mut().enumerate()
        {
            let target = if (i as f32) < full { 1.0 } else if i as f32 == full { steps.fract() } else { 0.0 };
            *fill = if *fill < target { (*fill + DOT_RISE * dt).min(target) } else { (*fill - DOT_FALL * dt).max(target) };
        }
    }

    fn load(&self, ctx: &egui::Context) -> Result<Textures, String>
    {
        let texture = |name: &str| -> Result<TextureHandle, String>
        {
            let path = format!("{}/gfx/{name}.png", self.assets);
            let bytes = load_binary(&path).map_err(|error| format!("{path}: {error}"))?;
            let image = rustl::image::load_from_memory(&bytes).map_err(|error| format!("{path}: {error}"))?.to_rgba8();
            let size = [image.width() as usize, image.height() as usize];
            Ok(ctx.load_texture(format!("revolt_{name}"), egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()), TextureOptions::LINEAR))
        };
        Ok(Textures { font: texture("font")?, fx: texture("fxpage2")?, countdown: self.load_countdown() })
    }

    // assets/models/countdown.json of the converter: per model triangles of corners [x, y, z, r, g, b]
    fn load_countdown(&self) -> [Vec<[(Vector3<f32>, Color32); 3]>; 4]
    {
        let path = format!("{}/models/countdown.json", self.assets);
        let json = load_binary(&path).ok().and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
        if json.is_none() { console_warning!("hud: {} missing - re-convert for the countdown numbers", path); }

        let corner = |c: &serde_json::Value| -> (Vector3<f32>, Color32)
        {
            let n = |i: usize| c[i].as_f64().unwrap_or(0.0) as f32;
            (Vector3::new(n(0), n(1), n(2)), Color32::from_rgb(n(3) as u8, n(4) as u8, n(5) as u8))
        };
        COUNTDOWN_MODELS.map(|name| json.as_ref().and_then(|json| json[name].as_array()).map(|triangles|
            triangles.iter().map(|t| [corner(&t[0]), corner(&t[1]), corner(&t[2])]).collect()).unwrap_or_default())
    }
}

impl Painter
{
    // one mesh per texture, the default texture of egui (white) for plain colored rects
    pub fn new(rect: Rect, textures: Vec<egui::TextureId>) -> Self
    {
        let mut painter = Self { rect, scale: 1.0, extra: 0.0, meshes: textures.into_iter().map(Mesh::with_texture).collect(), shapes: vec![] };
        painter.set_rect(rect);
        painter
    }

    // draws into this rect: scaled so that 640 x 480 fit (a narrow cell of split screen by its width), the lower half at the bottom
    pub fn set_rect(&mut self, rect: Rect)
    {
        self.rect = rect;
        self.scale = (rect.height() / HEIGHT).min(rect.width() / WIDTH);
        self.extra = (rect.height() - HEIGHT * self.scale).max(0.0);
    }

    // the meshes in this order onto the background layer
    pub fn finish(self, ctx: &egui::Context, order: &[usize])
    {
        let mut meshes: Vec<Option<Mesh>> = self.meshes.into_iter().map(Some).collect();
        let shapes: Vec<egui::Shape> = order.iter().filter_map(|&i| meshes.get_mut(i).and_then(Option::take)).filter(|mesh| !mesh.is_empty()).map(egui::Shape::mesh).collect();
        let painter = ctx.layer_painter(egui::LayerId::background());
        painter.extend(self.shapes);
        painter.extend(shapes);
    }

    // the shapes, then all meshes in their order
    pub fn finish_all(self, ctx: &egui::Context)
    {
        let order: Vec<usize> = (0..self.meshes.len()).collect();
        self.finish(ctx, &order);
    }

    // the blue box of the HUD: dark, a blue rim with a light line inside
    pub fn blue_box(&mut self, anchor: Anchor, [x, y, w, h]: [f32; 4])
    {
        self.rounded(anchor, [x, y, w, h], PANEL_FILL, PANEL_RIM, PANEL_RIM_WIDTH, PANEL_RADIUS);
        let inset = PANEL_RIM_WIDTH;
        self.rounded(anchor, [x + inset, y + inset, w - 2.0 * inset, h - 2.0 * inset], Color32::TRANSPARENT, PANEL_LIGHT, 1.0, PANEL_RADIUS - inset);
    }

    // "Rustl-Volt v0.0.1" small in the bottom right corner
    pub fn version(&mut self, text: &str)
    {
        if text.is_empty() { return; }
        self.text(Anchor::Right, WIDTH - 6.0 - text.len() as f32 * VERSION_W, HEIGHT - 4.0 - VERSION_H, VERSION_W, VERSION_H, VERSION_COLOR, text);
    }

    // a shaded ball, dark or lit by `fill` (0..1): an anti aliased edge, a light from the top left, a glint
    pub fn ball(&mut self, anchor: Anchor, [cx, cy]: [f32; 2], radius: f32, dark: [u8; 3], lit: [u8; 3], fill: f32)
    {
        let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * fill).round() as u8;
        let base = [mix(dark[0], lit[0]), mix(dark[1], lit[1]), mix(dark[2], lit[2])];
        let scale = |c: [u8; 3], k: f32| Color32::from_rgb((c[0] as f32 * k).min(255.0) as u8, (c[1] as f32 * k).min(255.0) as u8, (c[2] as f32 * k).min(255.0) as u8);
        let center = self.screen(anchor, cx, cy);
        let r = radius * self.scale;
        self.shapes.push(egui::Shape::circle_filled(center, r, scale(base, 0.45)));
        // the body: a fan from the light spot to the rim, the side away from the light darker
        let spot = center + egui::vec2(-0.35 * r, -0.35 * r);
        let inner = (r - 1.0).max(r * 0.8);
        let mut mesh = Mesh::default();
        mesh.colored_vertex(spot, scale(base, 1.0 + 0.6 * fill));
        let segments = 32;
        for i in 0..segments
        {
            let angle = i as f32 / segments as f32 * std::f32::consts::TAU;
            let away = (angle - 0.25 * std::f32::consts::PI).cos() * 0.5 + 0.5;
            mesh.colored_vertex(center + egui::vec2(angle.cos(), angle.sin()) * inner, scale(base, 0.9 - 0.4 * away));
        }
        for i in 0..segments
        {
            mesh.add_triangle(0, 1 + i, 1 + (i + 1) % segments);
        }
        self.shapes.push(egui::Shape::mesh(mesh));
        let glint = Color32::from_rgba_unmultiplied(255, 255, 240, (60.0 + 170.0 * fill) as u8);
        self.shapes.push(egui::Shape::circle_filled(spot, r * 0.28, glint));
    }

    // a five pointed star: a fan from the center (inner to outer color), an anti aliased rim
    pub fn star(&mut self, anchor: Anchor, [cx, cy]: [f32; 2], radius: f32, inner: Color32, outer: Color32, rim: Color32)
    {
        let center = self.screen(anchor, cx, cy);
        let r = radius * self.scale;
        let points: Vec<Pos2> = (0..10).map(|i|
        {
            let angle = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
            let distance = if i % 2 == 0 { r } else { r * 0.45 };
            center + egui::vec2(angle.cos(), angle.sin()) * distance
        }).collect();
        let mut mesh = Mesh::default();
        mesh.colored_vertex(center, inner);
        for point in &points
        {
            mesh.colored_vertex(*point, outer);
        }
        for i in 0..10
        {
            mesh.add_triangle(0, 1 + i, 1 + (i + 1) % 10);
        }
        self.shapes.push(egui::Shape::mesh(mesh));
        self.shapes.push(egui::Shape::closed_line(points, egui::Stroke::new(self.scale.max(1.0), rim)));
    }

    // a box with rounded corners and a rim, game space
    pub fn rounded(&mut self, anchor: Anchor, [x, y, w, h]: [f32; 4], fill: Color32, rim: Color32, rim_width: f32, radius: f32)
    {
        let rect = Rect::from_min_max(self.screen(anchor, x, y), self.screen(anchor, x + w, y + h));
        let corner = egui::CornerRadius::same((radius * self.scale).round().clamp(0.0, 255.0) as u8);
        let stroke = egui::Stroke::new(rim_width * self.scale, rim);
        self.shapes.push(egui::Shape::Rect(egui::epaint::RectShape::new(rect, corner, fill, stroke, egui::StrokeKind::Inside)));
    }

    // a bar from color `from` on the left to `to` on the right, game space
    pub fn gradient(&mut self, mesh: usize, anchor: Anchor, [x, y, w, h]: [f32; 4], from: Color32, to: Color32)
    {
        let (a, b) = (self.screen(anchor, x, y), self.screen(anchor, x + w, y + h));
        let mesh = &mut self.meshes[mesh];
        let base = mesh.vertices.len() as u32;
        let uv = egui::epaint::WHITE_UV;
        for (pos, color) in [(a, from), (Pos2::new(b.x, a.y), to), (b, to), (Pos2::new(a.x, b.y), from)]
        {
            mesh.vertices.push(egui::epaint::Vertex { pos, uv, color });
        }
        mesh.add_triangle(base, base + 1, base + 2);
        mesh.add_triangle(base, base + 2, base + 3);
    }

    // an image over a game space rect, uv 0..1
    pub fn image(&mut self, mesh: usize, anchor: Anchor, [x, y, w, h]: [f32; 4], color: Color32)
    {
        let rect = Rect::from_min_max(self.screen(anchor, x, y), self.screen(anchor, x + w, y + h));
        self.meshes[mesh].add_rect_with_uv(rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), color);
    }

    fn x(&self, anchor: Anchor, x: f32) -> f32
    {
        match anchor
        {
            Anchor::Left => self.rect.left() + x * self.scale,
            Anchor::Center => self.rect.center().x + (x - WIDTH * 0.5) * self.scale,
            Anchor::Right => self.rect.right() - (WIDTH - x) * self.scale,
        }
    }

    pub fn screen(&self, anchor: Anchor, x: f32, y: f32) -> Pos2
    {
        let below = if y >= HEIGHT * 0.5 { self.extra } else { 0.0 };
        Pos2::new(self.x(anchor, x), self.rect.top() + y * self.scale + below)
    }

    // a rect of a 256 texel page
    pub fn sprite(&mut self, page: usize, anchor: Anchor, [x, y, w, h]: [f32; 4], [u, v, uw, vh]: [f32; 4], color: Color32)
    {
        let rect = Rect::from_min_max(self.screen(anchor, x, y), self.screen(anchor, x + w, y + h));
        let uv = Rect::from_min_max(Pos2::new(u / 256.0, v / 256.0), Pos2::new((u + uw) / 256.0, (v + vh) / 256.0));
        self.meshes[page].add_rect_with_uv(rect, uv, color);
    }

    // the font of the game, every char w x h
    pub fn text(&mut self, anchor: Anchor, x: f32, y: f32, w: f32, h: f32, color: Color32, text: &str)
    {
        for (i, c) in text.chars().enumerate()
        {
            let code = c as u32;
            if code < GLYPH_FIRST || code > 0x7e { continue; }
            let index = code - GLYPH_FIRST;
            let (column, row) = ((index % GLYPHS_PER_ROW) as f32, (index / GLYPHS_PER_ROW) as f32);
            self.sprite(0, anchor, [x + i as f32 * w, y, w, h], [column * 8.0 + 0.5, row * 16.0, 7.0, 16.0], color);
        }
    }

    // the arrow of fxpage2 turned about its center, a square of half diagonal `radius` like the game
    fn arrow(&mut self, [cx, cy]: [f32; 2], radius: f32, turn: f32, color: Color32)
    {
        let [u, v, w, h] = ARROW_UV;
        let uvs = [[u, v], [u + w, v], [u + w, v + h], [u, v + h]];
        // top left first, clockwise - turned clockwise by `turn`
        let corners: Vec<Pos2> = (0..4).map(|i| { let angle = -3.0 * FRAC_PI_4 + i as f32 * 2.0 * FRAC_PI_4 + turn; self.screen(Anchor::Left, cx + radius * angle.cos(), cy + radius * angle.sin()) }).collect();
        let mesh = &mut self.meshes[1];
        let base = mesh.vertices.len() as u32;
        for (position, [tu, tv]) in corners.into_iter().zip(uvs)
        {
            mesh.vertices.push(egui::epaint::Vertex { pos: position, uv: Pos2::new(tu / 256.0, tv / 256.0), color });
        }
        mesh.add_triangle(base, base + 1, base + 2);
        mesh.add_triangle(base, base + 2, base + 3);
    }

    // "-40m name" ahead, "+78m name" behind - green to red by distance (red is close behind / far ahead), in a blue box
    fn neighbours(&mut self, ahead: Option<&Neighbour>, behind: Option<&Neighbour>)
    {
        if ahead.is_none() && behind.is_none()
        {
            return;
        }
        self.blue_box(Anchor::Left, [8.0, 426.0, 200.0, 46.0]);

        let shade = |metres: f32| (metres * 10.24).clamp(0.0, 511.0) as u32;
        if let Some(ahead) = ahead
        {
            let v = shade(ahead.metres);
            let color = if v < 256 { Color32::from_rgb(v as u8, 255, 0) } else { Color32::from_rgb(255, (511 - v) as u8, 0) };
            self.text(Anchor::Left, 16.0, 432.0, 8.0, 16.0, color, &format!("-{}m", ahead.metres as i32));
            self.text(Anchor::Left, 72.0, 432.0, 8.0, 16.0, WHITE, &ahead.name.chars().take(12).collect::<String>());
        }
        if let Some(behind) = behind
        {
            let v = shade(behind.metres);
            let color = if v < 256 { Color32::from_rgb(255, v as u8, 0) } else { Color32::from_rgb((511 - v) as u8, 255, 0) };
            self.text(Anchor::Left, 16.0, 448.0, 8.0, 16.0, color, &format!("+{}m", behind.metres as i32));
            self.text(Anchor::Left, 72.0, 448.0, 8.0, 16.0, WHITE, &behind.name.chars().take(12).collect::<String>());
        }
    }

    // 3, 2, 1, GO: the game counts t = countdown + 1 s down to 1 s at GO, then 1 s - race time down to 0; the model of t / 1 s
    fn countdown(&self, race: &Race, models: &[Vec<[(Vector3<f32>, Color32); 3]>; 4], mirrored: bool) -> Mesh
    {
        let mut mesh = Mesh::default();
        // the power up test has no countdown, not even the GO
        if race.settings.playground { return mesh; }
        let ms = match race.countdown > 0.0
        {
            true if race.countdown < 3.0 => (race.countdown * 1000.0 + 1000.0) as f32,
            false if race.race_time <= 1.0 => ((1.0 - race.race_time) * 1000.0).max(0.0) as f32,
            _ => return mesh,
        };
        let Some(model) = models.get(((ms / 1000.0) as usize).min(3)) else { return mesh; };

        // flips in the first and last 100 ms of a second - not the 3 coming in, not GO going out
        let phase = ms % 1000.0;
        let turns = if ms > 3500.0 || ms < 500.0 { 0.0 } else if phase < 100.0 { (phase - 100.0) * COUNTDOWN_FLIP } else if phase > 900.0 { (phase - 900.0) * COUNTDOWN_FLIP } else { 0.0 };
        // the 3 comes from close in 200 ms, GO flies at the camera in its last 200 ms
        let depth = if ms > 3800.0 { COUNTDOWN_DEPTH - (ms - 3800.0) } else if ms < 200.0 { COUNTDOWN_DEPTH - (200.0 - ms) } else { COUNTDOWN_DEPTH };
        let (sin, cos) = (turns * std::f32::consts::TAU).sin_cos();

        // game camera space: x right, y down, z ahead
        let mut triangles: Vec<([Pos2; 3], [Color32; 3], f32)> = model.iter().filter_map(|triangle|
        {
            let mut points = [Pos2::ZERO; 3];
            let mut z_sum = 0.0;
            for (i, (p, _)) in triangle.iter().enumerate()
            {
                let x = (p.x * cos + p.z * sin) * if mirrored { -1.0 } else { 1.0 };
                let z = -p.x * sin + p.z * cos + depth;
                if z < 1.0 { return None; }
                let y = p.y + COUNTDOWN_HEIGHT;
                points[i] = self.screen(Anchor::Center, WIDTH * 0.5 + x / z * COUNTDOWN_LENS, HEIGHT * 0.5 + y / z * COUNTDOWN_LENS);
                z_sum += z;
            }
            Some((points, triangle.map(|(_, color)| color), z_sum))
        }).collect();

        // no depth buffer: far first
        triangles.sort_by(|a, b| b.2.total_cmp(&a.2));
        for (points, colors, _) in triangles
        {
            let base = mesh.vertices.len() as u32;
            for (point, color) in points.into_iter().zip(colors)
            {
                mesh.colored_vertex(point, color);
            }
            mesh.add_triangle(base, base + 1, base + 2);
        }
        mesh
    }

    // the label of a viewport in the split view: place, name, lap - big enough for a small cell
    fn split_label(&mut self, racer: &Racer, laps: u32, clock: Option<f64>)
    {
        let color = if racer.is_local() { AMMO } else { WHITE };
        self.text(Anchor::Left, 16.0, 16.0, 16.0, 32.0, WHITE, &racer.position.to_string());
        self.text(Anchor::Left, if racer.position < 10 { 32.0 } else { 48.0 }, 16.0, 12.0, 20.0, CYAN, suffix(racer.position));
        self.text(Anchor::Left, 80.0, 16.0, 16.0, 32.0, color, &racer.name.chars().take(18).collect::<String>());
        let lap = match (clock, racer.finish_time) { (Some(clock), _) => format_time(clock), (None, Some(time)) => format_time(time), (None, None) => format!("Lap {}/{}", (racer.laps + 1).min(laps), laps) };
        self.text(Anchor::Left, 80.0, 52.0, 12.0, 24.0, CYAN, &lap);
    }

    // every car with its place and race time, centered; the ones still driving without a time
    fn results(&mut self, race: &Race, hint: bool)
    {
        let mut order: Vec<usize> = (0..race.racers.len()).collect();
        order.sort_by_key(|&index| race.racers[index].position);
        let racers: Vec<&Racer> = order.iter().map(|&index| &race.racers[index]).collect();
        // more than 15 cars (Clockwork Carnage: 30) in two columns
        let per_column = racers.len().clamp(1, 15);
        let columns = racers.len().div_ceil(per_column).max(1);
        let column_width = if columns == 1 { 340.0 } else { 300.0 };
        let left = 320.0 - columns as f32 * column_width * 0.5;
        let (row, top) = (20.0, if columns == 1 { 120.0 } else { 70.0 });
        self.blue_box(Anchor::Center, [left, top - 16.0, columns as f32 * column_width, 40.0 + row * per_column as f32 + 24.0]);
        self.text(Anchor::Center, 320.0 - 7.0 * 12.0 * 0.5, top, 12.0, 24.0, CYAN, "Results");
        for (i, racer) in racers.iter().enumerate()
        {
            let x = left + (i / per_column) as f32 * column_width;
            let y = top + 36.0 + (i % per_column) as f32 * row;
            let color = if racer.is_local() { AMMO } else { WHITE };
            self.text(Anchor::Center, x + 16.0, y, 8.0, 16.0, CYAN, &format!("{}{}", racer.position, suffix(racer.position)));
            self.text(Anchor::Center, x + 64.0, y, 8.0, 16.0, color, &racer.name.chars().take(16).collect::<String>());
            let time = match &race.battle
            {
                Some(battle) => format_time(battle.clock(order[i])),
                None => racer.finish_time.map_or_else(|| "--:--:---".to_string(), format_time),
            };
            self.text(Anchor::Center, x + column_width - 96.0, y, 8.0, 16.0, color, &time);
        }
        // below the panel
        if hint
        {
            let y = top + 48.0 + per_column as f32 * row + 16.0;
            let text = "Enter: menu";
            self.text(Anchor::Center, 320.0 - text.len() as f32 * 8.0 * 0.5, y, 8.0, 16.0, CYAN, text);
        }
    }

    // the weapon icon in the frame, added like the game (black is nothing); while it spins two icons fade into each other at half brightness
    fn weapon(&mut self, hand: &Hand)
    {
        let icon = |painter: &mut Self, index: usize, gray: f32|
        {
            let [u, v] = WEAPONS[index].2;
            let g = (gray * 255.0) as u8;
            painter.sprite(1, Anchor::Left, [32.0, 72.0, 32.0, 32.0], [u, v, 32.0, 32.0], Color32::from_rgba_premultiplied(g, g, g, 0));
        };
        match (hand.weapon, hand.spinning_icon())
        {
            (_, Some((index, fraction))) =>
            {
                icon(self, index, 0.5 * (1.0 - fraction));
                if fraction > 0.0 { icon(self, (index + 1) % WEAPONS.len(), 0.5 * fraction); }
            },
            (Some(weapon), None) =>
            {
                icon(self, weapon, 1.0);
                // the count of firework pack and water balloons, orange
                if WEAPONS[weapon].1 > 1 && hand.ammo > 0
                {
                    self.text(Anchor::Left, 60.0, 102.0, 6.0, 8.0, AMMO, &hand.ammo.to_string());
                }
            },
            (None, None) => {},
        }
    }
}

// a viewport (x, y from the bottom left, width, height - 0..1) on the screen
fn cell_rect(rect: Rect, [x, y, w, h]: [f32; 4]) -> Rect
{
    Rect::from_min_size(Pos2::new(rect.left() + x * rect.width(), rect.top() + (1.0 - y - h) * rect.height()), egui::vec2(w * rect.width(), h * rect.height()))
}

pub fn alpha(alpha: u8) -> Color32
{
    Color32::from_rgba_unmultiplied(255, 255, 255, alpha)
}

// the strings of the game (strings/english.txt): st, nd, rd, th
pub fn suffix(position: u32) -> &'static str
{
    match position
    {
        1 => "st",
        2 => "nd",
        3 => "rd",
        _ => "th",
    }
}

impl Hud
{
    // the reticles of the fireworks (a ring with four ticks, turning and pulsing; green the own target, red the own car aimed at), the bluish flash of the global pulse and the white of the lightning
    fn overlays(&self, painter: &mut Painter, rect: Rect)
    {
        let scale = rect.height() / HEIGHT;
        let clock = self.clock as f32;
        for ([x, y], red) in &self.reticles
        {
            let center = rect.min + egui::vec2(x * rect.width(), y * rect.height());
            if !rect.contains(center) { continue; }
            let color = if *red { Color32::from_rgba_unmultiplied(255, 60, 50, 230) } else { Color32::from_rgba_unmultiplied(90, 255, 80, 230) };
            let stroke = egui::Stroke::new(1.5 * scale, color);
            let radius = 8.0 * scale * (1.0 + 0.1 * (clock * 8.0).sin());
            let turn = clock * if *red { -4.0 } else { 3.0 };
            painter.shapes.push(egui::Shape::circle_stroke(center, radius, stroke));
            for quarter in 0..4
            {
                let angle = turn + quarter as f32 * std::f32::consts::FRAC_PI_2;
                let direction = egui::vec2(angle.cos(), angle.sin());
                painter.shapes.push(egui::Shape::line_segment([center + direction * radius * 0.5, center + direction * radius * 1.5], stroke));
            }
        }
        if self.flash > 0.0
        {
            let alpha = (self.flash.min(1.0) * 170.0) as u8;
            painter.shapes.push(egui::Shape::rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(170, 200, 255, alpha)));
        }
        if self.lightning > 0.0
        {
            let alpha = (self.lightning.min(1.0) * 150.0) as u8;
            painter.shapes.push(egui::Shape::rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(235, 240, 255, alpha)));
        }
    }
}

// the message in the middle, each line of it centered
fn message(painter: &mut Painter, text: &str, y: f32)
{
    for (i, line) in text.lines().enumerate()
    {
        painter.text(Anchor::Center, 320.0 - line.len() as f32 * 8.0, y + i as f32 * 28.0, 16.0, 24.0, WHITE, line);
    }
}

// the message in the middle - in battle below the arrow and its distance
fn message_y(race: &Race) -> f32
{
    if race.battle.is_some() { 112.0 } else { 80.0 }
}

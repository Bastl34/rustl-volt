mod ambient;
mod animated;
mod assets;
mod assist;
mod bot;
mod cars;
mod conveyors;
mod cup;
mod dinky;
mod field;
mod frontend;
mod hud;
mod laser;
mod menu;
mod music;
mod object_sounds;
mod object_type;
mod pad;
mod piano;
mod podium;
mod race;
mod random;
mod save;
mod setup;
mod texanim;
mod track;
mod weapons;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use rustl::prelude::*;
use rustl::state::scene::components::material::{BlendMode, Material, MaterialData};
use rustl::state::scene::components::transformation_animation::TransformationAnimation;

use ambient::Ambient;
use animated::Animated;
use cars::Showroom;
use conveyors::Conveyors;
use cup::{Championship, CupEvent, CupScreen, Phase};
use dinky::Dinkies;
use field::{Opponent, Slot};
use frontend::Frontend;
use hud::Hud;
use laser::Lasers;
use menu::{CarInfo, ChampionshipChoice, Menu, MenuEvent, RaceAction, RaceChoice, RaceMenu, RaceMenuKind, Shadows, Ssao, TrackInfo, VideoSettings, MENU_SOUND_SPEED};
use rustl::input::keyboard::Key;
use rustl::state::scene::components::component::ComponentItem;
use music::Music;
use object_type::ObjectType;
use pad::{Buttons, Device, Pad};
use piano::Piano;
use race::{Race, RaceSettings, View};
use random::Random;
use save::SaveData;
use setup::Setup;
use texanim::TexAnims;
use weapons::Weapons;
use rustl::state::scene::physics::contacts::ContactTarget;
use rustl::state::state::{State, REFERENCE_UPDATE_FRAMES};
use rustl::state::scene::components::sound::{Sound, SoundType};
use rustl::state::scene::scene_controller::scene_controller::SceneControllerBox;
use track::find_node;

rustl::app!(Game::default());

const WATER_COLOR: Vector3<f32> = Vector3::new(0.1, 0.35, 0.8);
// see through - driving through it the view stays
const WATER_ALPHA: f32 = 0.45;
// one turn in 2 seconds - radians per frame at 60 fps
const SPIN: f32 = std::f32::consts::TAU / 120.0;

// hits of a car (body against anything, m/s when it started): heard from, hard (hit2 instead of hit1), full volume
const HIT_MIN_SPEED: f32 = 1.5;
const HIT_HARD_SPEED: f32 = 6.0;
const HIT_FULL_SPEED: f32 = 12.0;
// a scrape along a wall starts touches all the time - one hit per car in this many frames
const HIT_COOLDOWN_FRAMES: u64 = 8;
const MAX_HITS_PER_FRAME: usize = 4;

// the logic of the project - it runs while the game runs (Play in the editor, always in exports)
#[derive(Default)]
pub struct Game
{
    frames: u64,
    // cars.scene: Enter / Backspace switch the driven car
    showroom: Option<Showroom>,
    // frame of the last hit sound per vehicle node
    last_hit: HashMap<u32, u64>,
    // the level music - the first loaded scene with a REDBOOK range
    music: Option<Music>,
    // the 3D sound objects of the loaded levels
    ambient: Ambient,
    // planes, horses, boats, ... and the copters
    animated: Animated,
    // the race of the first active level scene: the start car and the opponents - and the HUD of the player
    race: Option<(Race, Hud)>,
    // the cars loaded for the race - removed when Play stops
    opponents: Vec<Opponent>,
    // pickups and the weapon of the start car, once the field is there
    weapons: Option<Weapons>,
    // the menu - Play started in the frontend scene (Play in a level scene races there at once)
    menu: Option<MenuRun>,
    // the menu over the race: pause (Esc) or after the finish (Enter)
    race_menu: Option<RaceMenu>,
    // a race without pickups (time trial): the pickups and stars of the level hidden until it ends
    hidden_items: Vec<NodeItem>,
    // the game buttons of this frame: keyboard + gamepads
    pad: Pad,
    // escalators / conveyors, the museum lasers, the piano of toylite
    conveyors: Conveyors,
    lasers: Lasers,
    pianos: Vec<Piano>,
    // the texture animations of the levels (escalator steps, screens, water)
    texanims: TexAnims,
    // practice: the practice stars (type 1) of the track and whether they are found
    practice_stars: Vec<(NodeItem, bool)>,
    // the level scenes the game loaded itself (menu races) - unloaded again for the next track and at Stop
    loaded_by_game: Vec<u32>,
    // kept between runs (settings, records, stars) under the project name
    save: SaveData,
    save_game: String,
    // the best lap and the finish of each local player the records last saw
    record_seen: Vec<(Option<f64>, bool)>,
    // the frame a menu race started - the Enter that started it must not open the pause menu as well
    race_started_frame: u64,
    // the championship of the menu: its cup, the table, the tries - and the table over the race
    cup: Option<Championship>,
    cup_screen: Option<CupScreen>,
    // time trial: the laps of each local player the records last saw
    trial_laps: Vec<u32>,
    // the podium after a cup in the frontend
    ceremony: Option<podium::Ceremony>,
    // Play in the boot scene: find Re-Volt, convert, then the frontend
    setup: Option<Setup>,
    // the frontend the setup loaded - unloaded again at Stop
    frontend_by_setup: Option<u32>,
    // the sounds of the scene nodes paused with the game (engines, skids, 3D sounds) - the music plays on
    paused_sounds: Option<Vec<ComponentItem>>,
    // the fullscreen of the Video Settings last given to the window (exports only)
    fullscreen_applied: Option<bool>,
    // the graphics of the Video Settings last given to the engine, the project values before (back at Stop - the editor saves them)
    graphics_applied: Option<(u32, Ssao, Shadows)>,
    graphics_before: Option<(u32, bool, bool, bool, u32)>,
    // the car of the player whose sounds are stereo (cars::set_own_car_sounds)
    own_car: Option<NodeItem>,
    // split screen: the input device of each local player - empty for one player (all devices)
    devices: Vec<Device>,
}

struct MenuRun
{
    menu: Menu,
    frontend: Frontend,
    cars: Vec<CarInfo>,
    tracks: Vec<TrackInfo>,
    // length and name of the tracks, read once per scene (from the file if not loaded)
    track_cache: menu::TrackCache,
    // the race waiting for its level to load - the loading screen meanwhile
    pending: Option<RaceChoice>,
    // a scene came - the track list again
    tracks_dirty: bool,
    // the race the menu started and what was chosen for it (next track)
    racing: Option<MenuRace>,
    last_choice: Option<RaceChoice>,
    // the active scene when Play started
    start_scene: u32,
    // the clockwork cars on the table of the options view - spawned in the first update of the menu
    dinkies: Option<Dinkies>,
    assets: String,
    // master, effects, music of the audio device when Play started - the audio options scale the master, Stop restores them
    audio_base: (f32, f32, f32),
}

struct MenuRace
{
    scene_id: u32,
    // the stunt arena: no race, the stars to collect and whether they are collected (then hidden)
    stunts: Option<Vec<(NodeItem, bool)>>,
    // where the car of the menu started - a restart of the stunt arena puts it back there
    slot: Option<Slot>,
    // the cars of the menu when it is not the start car of the level - split screen: one per player
    players: Vec<NodeItem>,
    // the start car, its controller and camera while the car of the menu drives
    benched: Option<(NodeItem, SceneControllerBox, String)>,
    // split screen: the cameras of the players 2.., the camera of the start car and its viewport before
    cameras: Vec<u32>,
    main_view: Option<(String, [f32; 4])>,
}

impl App for Game
{
    fn init(&mut self, _context: &mut Context)
    {
        console_log!("game started");
    }

    // the scene is completely there - find nodes, cameras, ... here
    fn scene_loaded(&mut self, context: &mut Context, scene_id: u32)
    {
        let state = context.state.borrow();
        if self.save_game.is_empty()
        {
            self.save_game = match state.project.name.trim() { "" => "Rustl-Volt".to_string(), name => name.to_string() };
            self.save = SaveData::load(&self.save_game);
            console_log!("save: loaded for '{}' ({} tracks with records)", self.save_game, self.save.tracks.len());
            // the boot scene checks the assets itself
            if let Some(scene) = state.find_scene_by_id(scene_id).filter(|scene| !scene.name.eq_ignore_ascii_case(setup::BOOT_SCENE))
            {
                assets::check(&assets_folder(scene));
            }
        }
        let Some(scene) = state.scenes.iter().find(|scene| scene.id == scene_id) else { return; };

        // the boot scene is empty - the setup finds the game, converts and loads the frontend
        if scene.active && self.setup.is_none() && self.menu.is_none() && scene.name.eq_ignore_ascii_case(setup::BOOT_SCENE)
        {
            let folder = project_folder(scene);
            drop(state);
            self.setup = Some(Setup::new(&mut context.state.borrow_mut(), scene_id, folder));
            return;
        }

        // one music at a time, of the first active level scene - the inactive ones are loaded too
        if self.music.is_none() && scene.active
        {
            self.music = Music::start(&state, scene_id);
        }

        // the frontend is the menu, a level scene races at once
        let frontend = scene.active && self.menu.is_none() && scene.name.eq_ignore_ascii_case(frontend::SCENE);
        if self.race.is_none() && scene.active && !frontend
        {
            // Play in a level scene: the defaults, only the speed units of the save
            let settings = RaceSettings { kmh: self.save.race_settings(RaceSettings::default()).kmh, ..RaceSettings::default() };
            self.race = Race::start(scene, vec![], settings).map(|race| (race, Hud::new(scene)));
        }
        if let Some(run) = &mut self.menu
        {
            run.tracks_dirty = true;
        }

        self.ambient.add_scene(scene);
        self.animated.add_scene(&state, scene);
        self.lasers.add_scene(&state, scene);
        self.texanims.add_scene(scene);
        if !self.pianos.iter().any(|piano| piano.scene_id() == scene_id)
        {
            self.pianos.extend(Piano::start(&state, scene));
        }

        let mut water = 0;
        let mut spinning = 0;
        let mut levels = 0;
        for_each_node(&scene.nodes, &mut |node|
        {
            // the sky: no fog, no shadows
            if node.read().unwrap().tags.contains("revolt_skybox")
            {
                for_each_node(&vec![node.clone()], &mut |part| change_material(part, |m| { m.no_fog = true; m.cast_shadow = false; m.receive_shadow = false; }));
            }

            // the menu: no shadows at all - the lamps of the shop light it from everywhere
            if frontend
            {
                for_each_node(&vec![node.clone()], &mut |part| change_material(part, |m| m.cast_shadow = false));
            }

            // the shadows of the track are baked into its vertex colors - it only receives the ones of cars and objects
            if node.read().unwrap().tags.contains("revolt_level")
            {
                for_each_node(&vec![node.clone()], &mut |part| change_material(part, |m| m.cast_shadow = false));
                levels += 1;
            }

            // the strip lights under the ceiling of the supermarket - their shadows on the floor looked odd
            if ["flouro", "flourooff", "lflouro"].iter().any(|model| node.read().unwrap().tags.contains(*model))
            {
                for_each_node(&vec![node.clone()], &mut |part| change_material(part, |m| m.cast_shadow = false));
            }

            let object_type = object_kind(&node.read().unwrap());
            match object_type
            {
                Some(t) if t.is_water() =>
                {
                    // the game draws the water in code - blue and see through until the engine has water
                    for_each_node(&vec![node.clone()], &mut |part| change_material(part, |m| { m.base_color = WATER_COLOR; m.alpha = WATER_ALPHA; m.blend_mode = BlendMode::Blend; m.cast_shadow = false; }));
                    water += 1;
                },
                // the automatic doors are glass - no shadow
                Some(ObjectType::Slider) => for_each_node(&vec![node.clone()], &mut |part| change_material(part, |m| m.cast_shadow = false)),
                Some(ObjectType::Pickup | ObjectType::Star) =>
                {
                    // scene_loaded runs for every loaded scene - only one spin per node, or the speeds add up
                    if node.read().unwrap().find_component::<TransformationAnimation>().is_none()
                    {
                        let animation = TransformationAnimation::new("Spin", Vector3::zeros(), Vector3::new(0.0, SPIN, 0.0), Vector3::zeros());
                        node.write().unwrap().add_component(Arc::new(RwLock::new(Box::new(animation))));
                    }
                    // they spin - no shadow of themselves on themselves, their shadow on the ground stays
                    for_each_node(&vec![node.clone()], &mut |part| change_material(part, |m| m.receive_shadow = false));
                    spinning += 1;
                },
                _ => {},
            }
        });

        console_log!("scene '{}' loaded: {} tracks without cast shadows, {} water objects blue, {} pickups and stars spinning", scene.name, levels, water, spinning);
        drop(state);

        let mut state = context.state.borrow_mut();
        if frontend
        {
            self.start_menu(&mut state, scene_id);
            self.setup_done(&mut state);
        }
        if let Some(scene) = state.scenes.iter_mut().find(|scene| scene.id == scene_id)
        {
            // the start car of a level scene
            cars::setup_start_cars(scene);
            self.conveyors.add_scene(scene);

            // the converted cars - only once, the app lives from Play to Stop
            if self.showroom.is_none()
            {
                self.showroom = Showroom::start(scene);
            }
        }
        // the cars the converter placed (start car, showroom)
        if let Some(scene) = state.find_scene_by_id(scene_id)
        {
            let assets = assets_folder(scene);
            let mut placed = vec![];
            for_each_node(&scene.nodes, &mut |node| if node.read().unwrap().tags.contains("revolt_car") { placed.push(node.clone()); });
            for car in &placed { cars::apply_level_shade(scene, car); cars::hide_pins(car); }
            for car in placed { cars::add_env_map(&mut state, &car, &assets); }
            cars::add_instance_env_maps(&mut state, scene_id, &assets);
        }

        // the level of the waiting race is there
        if let Some(choice) = self.menu.as_mut().and_then(|run| run.pending.take_if(|choice| choice.scene_id == scene_id))
        {
            self.start_menu_race(&mut state, choice);
        }
    }

    // a level the game loaded went again (next track, Stop) - what was kept of it goes too
    fn scene_unloaded(&mut self, _context: &mut Context, scene_id: u32)
    {
        self.ambient.remove_scene(scene_id);
        self.animated.remove_scene(scene_id);
        self.lasers.remove_scene(scene_id);
        self.texanims.remove_scene(scene_id);
        self.conveyors.remove_scene(scene_id);
        for piano in self.pianos.iter_mut().filter(|piano| piano.scene_id() == scene_id)
        {
            piano.stop();
        }
        self.pianos.retain(|piano| piano.scene_id() != scene_id);
        if self.showroom.as_ref().is_some_and(|showroom| showroom.scene_id() == scene_id)
        {
            self.showroom = None;
        }
        // the cache is by scene id - the scene keeps its id, its track stays the same
        if let Some(run) = self.menu.as_mut()
        {
            run.tracks_dirty = true;
        }
    }

    fn update(&mut self, context: &mut Context)
    {
        self.frames += 1;

        self.play_hits(&mut context.state.borrow_mut());

        if let Some(music) = &mut self.music
        {
            music.update();
        }

        {
            let mut state = context.state.borrow_mut();
            let state = &mut *state;
            let dt = frame_seconds(state);
            // B and V only in a race - in the menu they are letters of the name
            self.pad.update(&mut state.io.input_manager, self.race.is_some());
            self.update_video(state);
            // no cursor while driving, also after Esc (the pause key) or a focus change released it
            let driving = self.race.is_some() || self.menu.as_ref().is_some_and(|run| run.racing.is_some());
            state.hide_cursor = driving && !state.paused() && self.setup.is_none();
            // the frontend was loaded already: no scene_loaded comes, the menu starts here
            if let Some(frontend) = self.setup.as_mut().and_then(|setup| setup.update(state, &self.pad.now))
            {
                self.start_menu(state, frontend);
                self.setup_done(state);
            }
            self.update_menu(state, dt);
            self.start_field(state);
            self.update_race_menu(state);
            self.update_cup(state, dt);
            self.check_records(state);
            if let Some((race, _)) = self.race.as_mut().filter(|_| !state.paused())
            {
                let pad = self.pad.now;
                // the buttons of each local player: alone all devices, split screen the own device
                let locals: Vec<Buttons> = if self.devices.is_empty() { vec![pad] } else { self.devices.iter().map(|device| self.pad.device(*device)).collect() };
                let beeps = state.scenes.iter_mut().find(|scene| scene.id == race.scene_id()).map(|scene| race.update(scene, &pad, &locals, dt)).unwrap_or_default();
                play_pitched_at(state, "countdown", &beeps, COUNTDOWN_VOLUME);

                if let Some(weapons) = self.weapons.as_mut()
                {
                    let fire: Vec<bool> = locals.iter().map(|buttons| buttons.fire).collect();
                    let sounds = state.scenes.iter().find(|scene| scene.id == race.scene_id()).map(|scene| weapons.update(scene, race, &fire, dt)).unwrap_or_default();
                    for (name, pitch) in sounds
                    {
                        play_pitched(state, name, &[pitch]);
                    }
                }
            }
        }

        self.pause_sounds(&context.state.borrow());
        self.update_own_car(&context.state.borrow());
        cars::update_hover(&mut context.state.borrow_mut());
        cars::update_suspension(&mut context.state.borrow_mut());
        self.find_practice_stars(&mut context.state.borrow_mut());
        self.ambient.update(&context.state.borrow());
        self.animated.update(&context.state.borrow());
        {
            let state = context.state.borrow();
            if !state.paused()
            {
                let dt = frame_seconds(&state);
                self.lasers.update(&state, dt);
                self.texanims.update(&state, dt);
                for piano in &mut self.pianos
                {
                    if let Some(scene) = state.find_scene_by_id(piano.scene_id()).filter(|scene| scene.active) { piano.update(scene, dt); }
                }
            }
        }

        if let Some(showroom) = &mut self.showroom
        {
            let mut state = context.state.borrow_mut();
            let step = if self.pad.now.enter { 1 } else if self.pad.now.back { -1 } else { 0 };
            if let Some(scene) = state.scenes.iter_mut().find(|scene| scene.id == showroom.scene_id())
            {
                showroom.update(scene, step);
            }
        }
    }

    // the HUD of the game while a race runs, else the menu
    fn ui(&mut self, context: &mut Context, ui: &mut rustl::egui::Ui)
    {
        let state = context.state.borrow();
        if let Some(setup) = self.setup.as_mut()
        {
            setup.ui(ui, &state);
            return;
        }
        let show_fps = self.video().show_fps;
        let dt = if state.paused() { 0.0 } else { frame_seconds(&state) };
        let version = format!("{} v{}", state.project.name.trim(), state.project.version.trim());
        if let Some((race, hud)) = self.race.as_mut()
        {
            hud.hint = self.race_menu.is_none();
            hud.show_fps = show_fps;
            hud.version = version;
            let cup = self.cup.as_ref().filter(|cup| cup.phase != Phase::Racing);
            hud.results = cup.is_none();
            hud.ui(ui, race, self.weapons.as_ref(), state.stats.last_fps, dt);
            if let (Some(cup), Some(screen)) = (cup, self.cup_screen.as_mut())
            {
                screen.ui(ui, cup);
            }
            if let Some(menu) = self.race_menu.as_mut()
            {
                menu.ui(ui);
            }
            return;
        }
        if let Some(run) = self.menu.as_mut()
        {
            run.menu.version = version;
            // the level of the race loads: its name, how far, the file of the loader
            if let Some(choice) = &run.pending
            {
                let (progress, file) = { let status = state.scene_loading.read().unwrap(); (status.progress(), status.current.clone()) };
                run.menu.loading_ui(ui, choice, &run.cars, &run.tracks, progress, &file);
                return;
            }
            if let Some(collected) = run.racing.as_ref().and_then(|racing| racing.stunts.as_ref()).map(|stars| stars.iter().map(|(_, done)| *done).collect::<Vec<bool>>())
            {
                run.menu.stunt_ui(ui, &collected, self.race_menu.is_some());
                if let Some(menu) = self.race_menu.as_mut()
                {
                    menu.ui(ui);
                }
                return;
            }
        }
        if let Some(ceremony) = self.ceremony.as_mut()
        {
            ceremony.ui(ui);
            return;
        }
        if let Some(run) = self.menu.as_mut().filter(|run| run.racing.is_none())
        {
            run.menu.ui(ui, &run.cars, &run.tracks);
        }
    }

    // Play stops - the editor keeps the scene, so the cars go back to how they were
    fn exit(&mut self, context: &mut Context)
    {
        if let Some(mut music) = self.music.take()
        {
            music.stop();
        }
        self.ambient.stop();
        self.animated.stop();
        self.lasers.stop();
        self.texanims.stop();
        for mut piano in self.pianos.drain(..) { piano.stop(); }
        {
            let mut state = context.state.borrow_mut();
            self.cup = None;
            self.cup_screen = None;
            if let Some(mut ceremony) = self.ceremony.take() { ceremony.stop(&mut state); }
            self.conveyors.stop(&mut state);
            let racing = self.menu.as_mut().and_then(|run| run.racing.take());
            self.leave_race(&mut state, racing);
            if let Some(mut run) = self.menu.take()
            {
                if let Some(dinkies) = run.dinkies.take() { dinkies.stop(&mut state); }
                run.frontend.stop(&mut state);
                set_volumes(&mut state, run.audio_base);
                state.set_active_scene(run.start_scene);
            }
            // the editor gets its scenes back as they were loaded before Play
            self.unload_game_scenes(&mut state, None);
            state.hide_cursor = false;
            if let Some((msaa, ssao, ssao_half_res, shadow, shadow_resolution)) = self.graphics_before.take()
            {
                state.rendering.msaa.set(msaa);
                state.rendering.ssao = ssao;
                state.rendering.ssao_half_res = ssao_half_res;
                state.rendering.shadow.set(shadow);
                state.rendering.shadow_map_resolution.set(shadow_resolution);
            }
            if let Some(car) = self.own_car.take() { cars::set_own_car_sounds(&car, false); }
            // Stop while the setup runs: back to the boot scene
            if let Some(setup) = self.setup.take()
            {
                state.set_active_scene(setup.boot_scene);
                self.frontend_by_setup = self.frontend_by_setup.or(setup.loaded_frontend);
                setup.finish(&mut state);
            }
            if let Some(frontend) = self.frontend_by_setup.take() { state.unload_scene(frontend); }
        }

        if let Some(mut showroom) = self.showroom.take()
        {
            let mut state = context.state.borrow_mut();
            if let Some(scene) = state.scenes.iter_mut().find(|scene| scene.id == showroom.scene_id())
            {
                showroom.stop(scene);
            }
        }
    }
}

impl Game
{
    // the opponents of the race, once the body of the player car exists (it leaves the front slot for them) - then the pickups
    fn start_field(&mut self, state: &mut State)
    {
        let Some((race, _)) = self.race.as_mut() else { return; };
        if race.ready() { return; }
        let scene_id = race.scene_id();
        let Some(scene) = state.scenes.iter().find(|scene| scene.id == scene_id) else { return; };

        if race.wants_opponents()
        {
            let player = race.player().node.clone();
            if !scene.physics.has_vehicle(player.read().unwrap().id) { return; }

            let car = player.read().unwrap().extras.get::<String>("revolt_car").cloned().unwrap_or_default();
            let rating = field::rating(&player);
            // the championship races against the opponents of its cup, always the same
            let candidates: Vec<&str> = match self.cup.as_ref()
            {
                Some(cup) => cup.opponents.iter().map(String::as_str).collect(),
                None => field::pick(&car, rating, &mut Random::default()),
            };
            let assets = assets_folder(scene);
            let locals = race.locals().len();
            let count = race.settings.cars.saturating_sub(locals).min(race.grid().len().saturating_sub(locals));
            let grid = race.grid().to_vec();
            console_log!("field: {} (rating {}), {} opponents", car, rating, count);

            self.opponents = field::spawn(state, scene_id, &assets, &candidates, &grid, count);
            race.add_opponents(self.opponents.clone());
        }
        else
        {
            race.set_ready();
        }

        if !race.settings.pickups
        {
            let practice = race.settings.practice;
            if let Some(scene) = state.scenes.iter().find(|scene| scene.id == scene_id)
            {
                // practice: the practice star stays out to be found
                self.hidden_items = hide_items(scene, |kind, star_type| kind == ObjectType::Pickup || !(practice && star_type == Some(1)));
                if practice
                {
                    for_each_node(&scene.nodes, &mut |node| if object_kind(&node.read().unwrap()) == Some(ObjectType::Star) && node.read().unwrap().extras.get::<i64>("revolt_param_type") == Some(&1) { self.practice_stars.push((node.clone(), false)); });
                }
                console_log!("race: no pickups - {} pickups and stars hidden, {} practice stars", self.hidden_items.len(), self.practice_stars.len());
            }
            return;
        }
        let scene = state.scenes.iter().find(|scene| scene.id == scene_id);
        // the practice stars (type 1) are only out in practice mode - the global pickup star (type 0) stays
        if let Some(scene) = scene
        {
            self.hidden_items = hide_items(scene, |kind, star_type| kind == ObjectType::Star && star_type == Some(1));
        }
        let sounds: Vec<_> = state.resources.sound_sources.values().cloned().collect();
        self.weapons = scene.map(|scene| Weapons::start(scene, race.racers.len(), &sounds));
    }

    // Play in the frontend scene: the menu with its camera, the cars of the boxes
    // the Video Settings: of the menu, else of the save (Play in a level scene)
    fn video(&self) -> VideoSettings
    {
        self.menu.as_ref().map_or_else(|| self.save.video_settings(), |run| run.menu.video)
    }

    // F toggles fullscreen (not on the name screen) and the window follows the setting - exports only, the editor has its own F
    fn update_video(&mut self, state: &mut State)
    {
        self.update_graphics(state);
        if cfg!(feature = "dynamic") { return; }
        let typing = self.menu.as_ref().is_some_and(|run| run.racing.is_none() && run.menu.typing());
        if !typing && state.io.input_manager.keyboard.is_pressed(Key::F)
        {
            let mut video = self.video();
            video.fullscreen = !video.fullscreen;
            if let Some(run) = self.menu.as_mut() { run.menu.video = video; }
            self.save.set_video_settings(&video);
            self.save.store(&self.save_game);
        }
        let fullscreen = self.video().fullscreen;
        if self.fullscreen_applied != Some(fullscreen)
        {
            state.rendering.fullscreen.set(fullscreen);
            self.fullscreen_applied = Some(fullscreen);
        }
    }

    // antialiasing, ambient occlusion and shadows of the Video Settings - also in the editor, Stop gives the project values back
    fn update_graphics(&mut self, state: &mut State)
    {
        let video = self.video();
        let graphics = (video.msaa, video.ssao, video.shadows);
        if self.graphics_applied == Some(graphics) { return; }
        let rendering = &mut state.rendering;
        if self.graphics_before.is_none()
        {
            self.graphics_before = Some((*rendering.msaa.get_ref(), rendering.ssao, rendering.ssao_half_res, *rendering.shadow.get_ref(), *rendering.shadow_map_resolution.get_ref()));
        }
        rendering.msaa.set(video.msaa);
        rendering.ssao = video.ssao != Ssao::Off;
        rendering.ssao_half_res = video.ssao == Ssao::Half;
        rendering.shadow.set(video.shadows != Shadows::Off);
        if let Some(resolution) = video.shadows.resolution() { rendering.shadow_map_resolution.set(resolution); }
        self.graphics_applied = Some(graphics);
    }

    // the sounds of the driven car are stereo (race, stunt arena) - the car before gets its spatial ones back; split screen: all spatial, every camera hears its cars
    fn update_own_car(&mut self, state: &State)
    {
        let car = match (&self.race, self.menu.as_ref().and_then(|run| run.racing.as_ref()))
        {
            (Some((race, _)), _) => (!race.multi()).then(|| race.player().node.clone()),
            (None, Some(racing)) => racing.players.first().cloned().or_else(|| state.find_scene_by_id(racing.scene_id).and_then(|scene| find_node(&scene.nodes, &|node| node.tags.contains("revolt_start_car")))),
            _ => None,
        };
        let same = |a: &NodeItem, b: &NodeItem| Arc::ptr_eq(a, b);
        if match (&car, &self.own_car) { (Some(a), Some(b)) => same(a, b), (None, None) => true, _ => false } { return; }
        if let Some(old) = self.own_car.take() { cars::set_own_car_sounds(&old, false); }
        if let Some(car) = &car { cars::set_own_car_sounds(car, true); }
        self.own_car = car;
    }

    // the pause menu pauses the sounds of the active scenes (engines, skids, 3D sounds) and starts them again after it
    fn pause_sounds(&mut self, state: &State)
    {
        match (state.paused(), self.paused_sounds.is_some())
        {
            (true, false) =>
            {
                let mut paused = vec![];
                for scene in state.scenes.iter().filter(|scene| scene.active)
                {
                    for node in Scene::list_all_child_nodes(&scene.nodes)
                    {
                        for sound in node.read().unwrap().find_components::<Sound>()
                        {
                            let mut item = sound.write().unwrap();
                            let Some(sound_component) = item.as_any_mut().downcast_mut::<Sound>() else { continue; };
                            if sound_component.running()
                            {
                                sound_component.pause();
                                drop(item);
                                paused.push(sound.clone());
                            }
                        }
                    }
                }
                self.paused_sounds = Some(paused);
            },
            (false, true) =>
            {
                for sound in self.paused_sounds.take().unwrap_or_default()
                {
                    if let Some(sound) = sound.write().unwrap().as_any_mut().downcast_mut::<Sound>() { sound.start(); }
                }
            },
            _ => {},
        }
    }

    // the menu runs: Stop goes back to the boot scene, the mouse setting of the project again
    fn setup_done(&mut self, state: &mut State)
    {
        let Some(setup) = self.setup.take() else { return; };
        if let Some(run) = self.menu.as_mut() { run.start_scene = setup.boot_scene; }
        self.frontend_by_setup = setup.loaded_frontend;
        setup.finish(state);
    }

    fn start_menu(&mut self, state: &mut State, scene_id: u32)
    {
        let Some(scene) = state.find_scene_by_id(scene_id) else { return; };
        let folder = project_folder(scene);
        let assets = assets_folder(scene);
        let Some(frontend) = Frontend::start(state, scene_id, &assets) else
        {
            console_warning!("menu: the frontend scene has no camera");
            return;
        };
        let cars = menu::load_cars(&folder);
        let audio_base = { let device = state.io.audio_device.read().unwrap(); let data = device.data.get_ref(); (data.volume, data.effects_volume, data.music_volume) };
        let mut menu = Menu::new(&folder);
        menu.restore(&self.save);
        self.menu = Some(MenuRun { menu, frontend, cars, tracks: vec![], track_cache: Default::default(), pending: None, tracks_dirty: true, racing: None, last_choice: None, start_scene: scene_id, dinkies: None, assets, audio_base });
    }

    // the menu while no race runs: the 3D side, its sounds, the start of a race; in the race Esc goes back
    fn update_menu(&mut self, state: &mut State, dt: f32)
    {
        let Some(run) = self.menu.as_mut() else { return; };
        let (master, effects, music) = run.audio_base;
        let audio = run.menu.audio;
        let step = |value: u32| value as f32 / menu::AUDIO_STEPS as f32;
        set_volumes(state, (master * step(audio.master), effects * step(audio.effects), music * step(audio.music)));
        // in the race the race menu leads back, in the stunt arena its own menu (Enter / Esc / Backspace)
        if let Some(racing) = run.racing.as_mut()
        {
            let Some(stars) = racing.stunts.as_mut() else { return; };
            let mut taken = 0;
            if !state.paused()
            {
                let car = racing.players.first().cloned().or_else(|| state.find_scene_by_id(racing.scene_id).and_then(|scene| find_node(&scene.nodes, &|node| node.tags.contains("revolt_start_car"))));
                if let Some(car) = car.and_then(|car| node_position(&car))
                {
                    for (index, (star, done)) in stars.iter_mut().enumerate().filter(|(_, (_, done))| !*done)
                    {
                        if node_position(star).is_some_and(|p| (p - car).norm() < STAR_REACH)
                        {
                            *done = true;
                            star.write().unwrap().settings.visible = false;
                            self.save.stunt_found.insert(index);
                            taken += 1;
                        }
                    }
                }
            }
            if taken > 0
            {
                play_pitched(state, "pickup", &[1.0]);
                self.save.stunt_stars = self.save.stunt_stars.max(self.save.stunt_found.len() as u32);
                self.save.store(&self.save_game);
                run.menu.saved = self.save.clone();
            }

            let scene_id = racing.scene_id;
            let pad = self.pad.now;
            let action = match self.race_menu.as_mut()
            {
                Some(menu) =>
                {
                    let (action, sounds) = menu.update(&pad);
                    for (name, speed) in sounds
                    {
                        play_pitched(state, name, &[speed]);
                    }
                    action
                },
                None =>
                {
                    if pad.key_enter || pad.key_back || pad.pause
                    {
                        let assets = state.find_scene_by_id(scene_id).map(|scene| assets_folder(scene)).unwrap_or_default();
                        self.race_menu = Some(RaceMenu::new(RaceMenuKind::Stunts, &assets, true));
                        state.game_pause = true;
                        play_pitched(state, "menunext", &[MENU_SOUND_SPEED]);
                    }
                    None
                },
            };
            let Some(action) = action else { return; };
            self.race_menu = None;
            state.game_pause = false;
            match action
            {
                RaceAction::Restart => self.restart_stunts(state),
                RaceAction::MainMenu => self.end_menu_race(state),
                _ => {},
            }
            return;
        }

        // the level of the race loads - scene_loaded starts it
        if run.pending.is_some()
        {
            return;
        }
        // the podium after a cup, then the main menu
        if let Some(ceremony) = self.ceremony.as_mut()
        {
            ceremony.update(state, &mut run.frontend, dt);
            if ceremony.done()
            {
                ceremony.stop(state);
                self.ceremony = None;
            }
            return;
        }

        if run.tracks_dirty
        {
            run.tracks = menu::load_tracks(state, &mut run.track_cache);
            run.tracks_dirty = false;
        }
        // split screen: the player choosing name and car steers the menu with the own device
        run.menu.gamepads = state.io.input_manager.gamepads.values().filter(|gamepad| gamepad.connected).map(|gamepad| gamepad.id).collect();
        run.menu.max_msaa = state.rendering_adapter.max_msaa_samples;
        let buttons = run.menu.input_device().map_or(self.pad.now, |device| self.pad.device(device));
        run.menu.update(&mut state.io.input_manager.keyboard, &buttons, &run.cars, &run.tracks, dt);
        // name, settings and audio options are kept as soon as they change
        if run.menu.store(&mut self.save)
        {
            self.save.store(&self.save_game);
            run.menu.saved = self.save.clone();
        }
        let target = run.menu.target(&run.cars, &run.tracks);
        run.frontend.update(state, &target, dt);
        match run.dinkies.as_mut()
        {
            Some(dinkies) => dinkies.update(state, dt),
            None => run.dinkies = Some(Dinkies::start(state, run.frontend.scene_id, &run.assets)),
        }

        let mut start = None;
        let mut championship = None;
        for event in run.menu.take_events()
        {
            match event
            {
                MenuEvent::Sound(name, speed) => play_pitched(state, name, &[speed]),
                MenuEvent::Quit => state.stop_request = true,
                MenuEvent::Start(choice) => start = Some(choice),
                MenuEvent::Championship(choice) => championship = Some(choice),
                MenuEvent::Stunts(car, player_name) =>
                {
                    let scene = state.scenes.iter().find(|scene| scene.name.eq_ignore_ascii_case("stunts"));
                    match scene
                    {
                        Some(scene) => start = Some(RaceChoice { scene_id: scene.id, car, player_name, settings: RaceSettings::default(), stunts: true, cup_stage: None, players: vec![] }),
                        None => { console_warning!("menu: no stunts scene in the project - convert the stunts level"); },
                    }
                },
            }
        }
        if let Some(choice) = start
        {
            self.request_menu_race(state, choice);
        }
        if let Some(choice) = championship
        {
            self.start_championship(state, choice);
        }
    }

    // Select Cup -> Summary -> Enter: the cup with its opponents, the first race
    fn start_championship(&mut self, state: &mut State, choice: ChampionshipChoice)
    {
        self.cup = Some(Championship::new(choice.cup, &choice.car, &choice.player_name, choice.settings));
        if let Some(run) = self.menu.as_ref() { self.cup_screen = Some(CupScreen::new(&run.assets)); }
        self.start_cup_race(state);
    }

    // the race of the current stage (again after a failed try) - its scene by name, the cup ends if it is not converted
    fn start_cup_race(&mut self, state: &mut State)
    {
        let Some(cup) = self.cup.as_ref() else { return; };
        let name = cup.scene_name();
        let Some(scene_id) = state.scenes.iter().find(|scene| scene.name.eq_ignore_ascii_case(&name)).map(|scene| scene.id) else
        {
            console_warning!("championship: no scene '{}' in the project - convert that track", name);
            self.end_championship(state, None, vec!["TRACK MISSING".to_string(), format!("CONVERT {}", name.to_uppercase())]);
            return;
        };
        let choice = RaceChoice { scene_id, car: cup.car.clone(), player_name: cup.player_name.clone(), settings: cup.race_settings(), stunts: false, cup_stage: Some((cup.stage, cup.stages())), players: vec![] };
        let racing = self.menu.as_mut().and_then(|run| run.racing.take());
        self.leave_race(state, racing);
        if let Some(mut music) = self.music.take()
        {
            music.stop();
        }
        self.request_menu_race(state, choice);
    }

    // the table, the try again question, the next race - every frame while the championship runs
    fn update_cup(&mut self, state: &mut State, dt: f32)
    {
        let Some(cup) = self.cup.as_mut() else { return; };
        if cup.phase == Phase::Racing || self.race_menu.is_some() { return; }
        let events = cup.update(&self.pad.now, dt);
        self.handle_cup_events(state, events);
    }

    fn handle_cup_events(&mut self, state: &mut State, events: Vec<CupEvent>)
    {
        for event in events
        {
            match event
            {
                CupEvent::Sound(name, speed) => play_pitched(state, name, &[speed]),
                CupEvent::Race => self.start_cup_race(state),
                CupEvent::End(rank) =>
                {
                    self.end_championship(state, rank, vec![]);
                    return;
                },
            }
        }
    }

    // the cup is over: a win opens the next cup, back to the menu with the message
    fn end_championship(&mut self, state: &mut State, rank: Option<usize>, message: Vec<String>)
    {
        let Some(cup) = self.cup.take() else { return; };
        self.cup_screen = None;
        let new = rank == Some(1) && self.save.cups_won.insert(cup.cup);
        if new
        {
            self.save.store(&self.save_game);
            if let Some(run) = self.menu.as_mut() { run.menu.saved = self.save.clone(); }
        }
        console_log!("championship: {} over, place {:?}", cup::CUPS[cup.cup].name, rank);
        self.end_menu_race(state);
        if let Some(run) = self.menu.as_mut().filter(|_| !message.is_empty()) { run.menu.show_message(message); }
        // the end of the cup: the podium in the frontend
        if let (Some(rank), Some(run)) = (rank, self.menu.as_ref())
        {
            self.ceremony = Some(podium::Ceremony::new(run.frontend.scene_id, &run.assets, cup.cup, rank, new, cup.podium(), &cup.car));
        }
    }

    // a level that is not loaded yet loads first - the loading screen shows meanwhile, scene_loaded starts the race
    fn request_menu_race(&mut self, state: &mut State, choice: RaceChoice)
    {
        let scene_id = choice.scene_id;
        if state.is_scene_loaded(scene_id)
        {
            self.start_menu_race(state, choice);
            return;
        }

        // a scene loading already (Load at start) is not one of the game
        let unloaded = !state.is_scene_loading(scene_id);
        if !state.load_scene(scene_id)
        {
            console_warning!("menu: scene {} can not be loaded", scene_id);
            return;
        }
        if unloaded && !self.loaded_by_game.contains(&scene_id)
        {
            self.loaded_by_game.push(scene_id);
        }
        if let Some(mut music) = self.music.take()
        {
            music.stop();
        }
        if let Some(run) = self.menu.as_mut()
        {
            run.pending = Some(choice);
        }
    }

    // the levels the game loaded, except `keep` and the active one
    fn unload_game_scenes(&mut self, state: &mut State, keep: Option<u32>)
    {
        let unload: Vec<u32> = self.loaded_by_game.iter().copied()
            .filter(|id| Some(*id) != keep && state.find_scene_by_id(*id).is_some_and(|scene| !scene.active))
            .collect();
        for scene_id in unload
        {
            // still loading: stays in the list for the next time
            if state.unload_scene(scene_id)
            {
                self.loaded_by_game.retain(|id| *id != scene_id);
            }
        }
    }

    // the level of the menu becomes the active scene, the car of the menu drives (the start car steps aside for another one)
    fn start_menu_race(&mut self, state: &mut State, choice: RaceChoice)
    {
        let scene_id = choice.scene_id;
        if let Some(mut music) = self.music.take()
        {
            music.stop();
        }
        state.set_active_scene(scene_id);
        self.race_started_frame = self.frames;
        self.trial_laps.clear();
        self.record_seen.clear();
        self.devices.clear();
        // one level at a time - the ones of earlier races go
        self.unload_game_scenes(state, Some(scene_id));
        console_log!("menu: race on scene {} with {} ({:?}), {} players", scene_id, choice.car, choice.settings, choice.players.len().max(1));

        let mut racing = MenuRace { scene_id, stunts: None, slot: None, players: vec![], benched: None, cameras: vec![], main_view: None };
        let multi = !choice.players.is_empty();
        // split screen: the cells of the players, the ones left over show bots
        let cells = if multi { race::split_cells(choice.players.len()) } else { vec![] };
        self.spawn_players(state, &choice, &cells, &mut racing);

        let Some(scene) = state.find_scene_by_id(scene_id) else { return; };
        self.race = Race::start(scene, racing.players.clone(), choice.settings).map(|race| (race, Hud::new(scene)));
        let main_cam = racing.main_view.as_ref().and_then(|(name, _)| scene.cameras.iter().find(|cam| cam.name == *name)).map(|cam| cam.id);
        if let Some((race, _)) = self.race.as_mut()
        {
            let locals = race.locals();
            match (multi && locals.len() == choice.players.len(), main_cam)
            {
                (true, Some(main_cam)) =>
                {
                    let cams: Vec<u32> = std::iter::once(main_cam).chain(racing.cameras.iter().copied()).collect();
                    let mut views: Vec<View> = vec![];
                    for (player, (&index, chosen)) in locals.iter().zip(&choice.players).enumerate()
                    {
                        race.racers[index].name = chosen.name.clone();
                        race.racers[index].mode = chosen.mode;
                        views.push((index, cams[player], cells[player]));
                    }
                    race.set_multi(views, cells[locals.len()..].to_vec());
                    self.devices = choice.players.iter().map(|player| player.device).collect();
                },
                _ =>
                {
                    let index = race.player_index();
                    race.racers[index].name = choice.player_name.clone();
                },
            }
        }
        self.music = Music::start(state, scene_id);
        if self.race.is_none() && choice.stunts
        {
            // free driving: the stunt stars (type 1) to collect - the pickups and the global star are not out
            if let Some(scene) = state.find_scene_by_id(scene_id)
            {
                let mut stars = vec![];
                for_each_node(&scene.nodes, &mut |node|
                {
                    let read = node.read().unwrap();
                    if read.extras.get::<i64>("revolt_object_type").and_then(|id| ObjectType::from_id(*id)) == Some(ObjectType::Star) && read.extras.get::<i64>("revolt_param_type") == Some(&1) { stars.push((node.clone(), false)); }
                });
                for (index, (star, done)) in stars.iter_mut().enumerate()
                {
                    if self.save.stunt_found.contains(&index) { *done = true; star.write().unwrap().settings.visible = false; }
                }
                self.hidden_items = hide_items(scene, |kind, star_type| kind == ObjectType::Pickup || (kind == ObjectType::Star && star_type != Some(1)));
                console_log!("stunt arena: {} stars, {} pickups hidden", stars.len(), self.hidden_items.len());
                racing.stunts = Some(stars);
            }
        }
        let stunts = racing.stunts.is_some();
        if let Some(run) = self.menu.as_mut()
        {
            run.racing = Some(racing);
            run.last_choice = Some(choice.clone());
        }
        if self.race.is_none() && !stunts
        {
            console_warning!("menu: no race on this scene - back to the menu");
            self.end_menu_race(state);
        }
    }

    // the cars of the menu instead of the start car (it brings the grid): behind the opponents, split screen one per player with an own camera and the bindings of the own device
    fn spawn_players(&mut self, state: &mut State, choice: &RaceChoice, cells: &[[f32; 4]], racing: &mut MenuRace)
    {
        let scene_id = choice.scene_id;
        let multi = !choice.players.is_empty();
        let Some(scene) = state.find_scene_by_id_mut(scene_id) else { return; };
        let assets = assets_folder(scene);
        let Some(start_car) = find_node(&scene.nodes, &|node| node.tags.contains("revolt_start_car")) else { return; };
        let start_id = start_car.read().unwrap().extras.get::<String>("revolt_car").cloned().unwrap_or_default();
        if start_id == choice.car && !multi { return; }

        // the slots the race gives the players: behind the opponents - not on one of them
        let cars: Vec<(String, Option<Device>)> = if multi { choice.players.iter().map(|player| (player.car.clone(), Some(player.device))).collect() } else { vec![(choice.car.clone(), None)] };
        let grid = field::grid(&start_car.read().unwrap());
        let opponents = if choice.stunts { 0 } else { choice.settings.cars.saturating_sub(cars.len()).min(grid.len().saturating_sub(cars.len())) };
        let start_slot = || { let position = start_car.read().unwrap().find_component::<Transformation>().map(|t| { component_downcast!(t, Transformation); t.get_data().position }).unwrap_or_default(); Slot { floor: position, heading: -Vector3::z() } };
        let slots: Vec<Slot> = (0..cars.len()).map(|i| grid.get(opponents + i).copied().unwrap_or_else(start_slot)).collect();
        racing.slot = slots.first().copied();
        let Some((controller, cam_name)) = field::bench_start_car(scene, &start_car) else { return; };

        // split screen: player 1 has the camera of the start car, the others copies of it
        if multi
        {
            if let Some(cam) = scene.get_game_camera_mut(&cam_name)
            {
                let data = rustl::serde_json::to_value(cam.get_data()).unwrap_or_default();
                racing.main_view = Some((cam_name.clone(), race::viewport(&data)));
                let [x, y, w, h] = cells[0];
                cam.update_viewport(x, y, w, h);
                for (i, cell) in cells.iter().enumerate().take(cars.len()).skip(1)
                {
                    racing.cameras.push(race::add_camera(scene, &data, &format!("Re-Volt player {}", i + 1), *cell));
                }
            }
        }

        let mut error = None;
        for (i, ((car, device), slot)) in cars.iter().zip(&slots).enumerate()
        {
            let cam = if i == 0 { cam_name.clone() } else { format!("Re-Volt player {}", i + 1) };
            match field::spawn_player(state, scene_id, &assets, car, i, *slot, &cam)
            {
                Ok(node) =>
                {
                    let id = node.read().unwrap().id;
                    if let (Some(device), Some(scene)) = (device, state.find_scene_by_id_mut(scene_id)) { race::with_vehicle(scene, id, |vehicle, _| vehicle.controls = device.controls()); }
                    racing.players.push(node);
                },
                Err(text) => { error = Some(text); break; },
            }
        }
        let Some(scene) = state.find_scene_by_id_mut(scene_id) else { return; };
        match error
        {
            None => racing.benched = Some((start_car, controller, cam_name)),
            Some(error) =>
            {
                console_warning!("menu: {} - the start car drives", error);
                field::remove_players(scene, &racing.players);
                racing.players.clear();
                remove_player_cameras(scene, racing);
                field::unbench_start_car(scene, &start_car, controller, &cam_name);
            },
        }
    }

    // back from a race of the menu to the frontend
    fn end_menu_race(&mut self, state: &mut State)
    {
        let racing = self.menu.as_mut().and_then(|run| run.racing.take());
        self.leave_race(state, racing);
        if let Some(mut music) = self.music.take()
        {
            music.stop();
        }
        let Some(run) = self.menu.as_mut() else { return; };
        state.set_active_scene(run.frontend.scene_id);
        self.music = Music::start(state, run.frontend.scene_id);
        run.menu.back_from_race();
        run.frontend.jump(frontend::VIEW_MAIN);
        run.frontend.clear_car(state);
    }

    // Enter after the finish, Esc in a race of the menu: the race menu (a pause stops the race meanwhile)
    fn update_race_menu(&mut self, state: &mut State)
    {
        let Some((scene_id, done)) = self.race.as_ref().map(|(race, _)| (race.scene_id(), race.over || race.locals_finished())) else
        {
            let stunts = self.menu.as_ref().and_then(|run| run.racing.as_ref()).is_some_and(|racing| racing.stunts.is_some());
            if !stunts { self.race_menu = None; }
            return;
        };
        let pad = self.pad.now;
        // the championship: Enter after the finish ends the race and shows the table (no race menu), its table takes the keys meanwhile
        if let Some(cup) = self.cup.as_mut()
        {
            if cup.phase != Phase::Racing { return; }
            if done && self.race_menu.is_none() && self.frames != self.race_started_frame && (pad.enter || pad.start)
            {
                let Some((race, _)) = self.race.as_mut() else { return; };
                race.over = true;
                let events = cup.finish_race(race);
                self.handle_cup_events(state, events);
                return;
            }
        }
        let action = match self.race_menu.as_mut()
        {
            Some(menu) =>
            {
                let (action, sounds) = menu.update(&pad);
                for (name, speed) in sounds
                {
                    play_pitched(state, name, &[speed]);
                }
                action
            },
            None =>
            {
                // in the race Enter (keyboard - the pad A is the handbrake) or Start, Esc only in exports (the editor stops Play on it)
                let kind = if self.frames == self.race_started_frame { None }
                    else if done && (pad.enter || pad.start) { Some(RaceMenuKind::Finished) }
                    else if !done && (pad.key_enter || pad.start || self.menu.is_some() && pad.pause) { Some(RaceMenuKind::Pause) }
                    else { None };
                let kind = kind.map(|kind| if self.cup.is_some() && kind == RaceMenuKind::Pause { RaceMenuKind::Championship } else { kind });
                if let Some(kind) = kind
                {
                    let assets = state.find_scene_by_id(scene_id).map(|scene| assets_folder(scene)).unwrap_or_default();
                    self.race_menu = Some(RaceMenu::new(kind, &assets, self.menu.is_some()));
                    state.game_pause = matches!(kind, RaceMenuKind::Pause | RaceMenuKind::Championship);
                    play_pitched(state, "menunext", &[MENU_SOUND_SPEED]);
                }
                None
            },
        };
        let Some(action) = action else { return; };
        self.race_menu = None;
        state.game_pause = false;
        match action
        {
            RaceAction::Resume => {},
            RaceAction::Restart => self.restart_race(state),
            RaceAction::NextTrack => self.next_track(state),
            RaceAction::MainMenu if self.cup.is_some() => self.end_championship(state, None, vec![]),
            RaceAction::MainMenu => self.end_menu_race(state),
            RaceAction::GiveUp =>
            {
                let events = self.cup.as_mut().map(|cup| cup.give_up()).unwrap_or_default();
                self.handle_cup_events(state, events);
            },
        }
    }

    // back to the grid and the countdown, the pickups again
    fn restart_race(&mut self, state: &mut State)
    {
        let Some((race, _)) = self.race.as_mut() else { return; };
        race.restart();
        self.trial_laps.clear();
        for (star, found) in &mut self.practice_stars
        {
            *found = false;
            star.write().unwrap().settings.visible = true;
        }
        if let Some(weapons) = self.weapons.take()
        {
            weapons.stop();
            let sounds: Vec<_> = state.resources.sound_sources.values().cloned().collect();
            self.weapons = state.scenes.iter().find(|scene| scene.id == race.scene_id()).map(|scene| Weapons::start(scene, race.racers.len(), &sounds));
        }
    }

    // the stunt arena again: the car back where it started, found stars stay collected
    fn restart_stunts(&mut self, state: &mut State)
    {
        let Some(racing) = self.menu.as_mut().and_then(|run| run.racing.as_mut()) else { return; };
        let Some(scene) = state.find_scene_by_id_mut(racing.scene_id) else { return; };
        let Some(car) = racing.players.first().cloned().or_else(|| find_node(&scene.nodes, &|node| node.tags.contains("revolt_start_car"))) else { return; };
        let slot = racing.slot.or_else(|| field::grid(&car.read().unwrap()).first().copied());
        let Some(slot) = slot else { return; };
        let id = car.read().unwrap().id;
        race::with_vehicle(scene, id, |vehicle, scene| vehicle.place(scene, slot.floor + Vector3::y() * (cars::ground_height(vehicle) + cars::LIFT), slot.heading));
    }

    // the next track of the track menu with the same car and settings
    fn next_track(&mut self, state: &mut State)
    {
        let Some(run) = self.menu.as_mut() else { return; };
        let Some(mut choice) = run.last_choice.clone() else { return; };
        if run.tracks.is_empty() { return; }
        let current = run.tracks.iter().position(|track| track.scene_id == choice.scene_id || track.reversed == Some(choice.scene_id));
        let index = current.map_or(0, |i| (i + 1) % run.tracks.len());
        choice.scene_id = run.tracks[index].scene_id;
        run.menu.set_track(index);
        let racing = run.racing.take();
        self.leave_race(state, racing);
        if let Some(mut music) = self.music.take()
        {
            music.stop();
        }
        self.request_menu_race(state, choice);
    }

    // the race ends: weapons, opponents, the car of the menu go, the start car comes back
    fn leave_race(&mut self, state: &mut State, racing: Option<MenuRace>)
    {
        self.race_menu = None;
        state.game_pause = false;
        for (star, _) in self.practice_stars.drain(..)
        {
            star.write().unwrap().settings.visible = true;
        }
        for node in self.hidden_items.drain(..)
        {
            node.write().unwrap().settings.visible = true;
        }
        if let Some(weapons) = self.weapons.take()
        {
            weapons.stop();
        }
        if let Some((mut race, _)) = self.race.take()
        {
            if let Some(scene) = state.scenes.iter_mut().find(|scene| scene.id == race.scene_id())
            {
                race.stop(scene);
                field::remove(scene, &self.opponents);
            }
        }
        if let Some(racing) = racing
        {
            for (star, _) in racing.stunts.iter().flatten()
            {
                star.write().unwrap().settings.visible = true;
            }
            if let Some(scene) = state.scenes.iter_mut().find(|scene| scene.id == racing.scene_id)
            {
                field::remove_players(scene, &racing.players);
                remove_player_cameras(scene, &racing);
                if let Some((car, controller, cam_name)) = racing.benched { field::unbench_start_car(scene, &car, controller, &cam_name); }
            }
        }
        self.opponents.clear();
        self.devices.clear();
    }

    // practice: the player car near a practice star finds it (the game shows that for 2 s)
    fn find_practice_stars(&mut self, state: &mut State)
    {
        if self.practice_stars.is_empty() || state.paused() { return; }
        let Some((race, hud)) = self.race.as_mut() else { return; };
        let Some(car) = node_position(&race.player().node) else { return; };
        let mut found = 0;
        for (star, done) in self.practice_stars.iter_mut().filter(|(_, done)| !*done)
        {
            if node_position(star).is_some_and(|p| (p - car).norm() < STAR_REACH)
            {
                *done = true;
                star.write().unwrap().settings.visible = false;
                found += 1;
            }
        }
        if found == 0 { return; }
        let track = state.find_scene_by_id(race.scene_id()).map(|scene| scene.name.to_lowercase()).unwrap_or_default();
        if self.save.practice_stars.insert(track)
        {
            self.save.store(&self.save_game);
            if let Some(run) = self.menu.as_mut() { run.menu.saved = self.save.clone(); }
        }
        let left = self.practice_stars.iter().filter(|(_, done)| !*done).count();
        hud.message = Some((if left == 0 { "Star found!".to_string() } else { format!("Star found! {left} left") }, PRACTICE_MESSAGE_TIME));
        console_log!("practice: star found, {} left", left);
        play_pitched(state, "pickup", &[1.0]);
    }

    // a new best lap or race time of a local player is kept per track (not in practice) and shown for 2 s - split screen with the name in front,
    // the progress (won race, challenge times) only alone
    fn check_records(&mut self, state: &State)
    {
        let Some((race, hud)) = self.race.as_mut() else { return; };
        if race.settings.practice { return; }
        let locals = race.locals();
        let multi = locals.len() > 1;
        self.trial_laps.resize(locals.len(), 0);
        self.record_seen.resize(locals.len(), (None, false));
        let track = state.find_scene_by_id(race.scene_id()).map(|scene| scene.name.to_lowercase()).unwrap_or_default();
        let mut changed = false;
        for (local, &index) in locals.iter().enumerate()
        {
            let player = &race.racers[index];
            let label = |text: &str| if multi { format!("{}: {text}", player.name) } else { text.to_string() };
            let car = player.node.read().unwrap().extras.get::<String>("revolt_car").cloned().unwrap_or_default();
            // time trial: every lap into the Best Trial Times, the challenge lap time of the track (the Progress Table)
            if race.settings.trial && player.laps != self.trial_laps[local]
            {
                self.trial_laps[local] = player.laps;
                if let Some(lap) = player.last_lap.filter(|_| player.laps > 0)
                {
                    let place = self.save.record_trial(&track, lap, &car, &player.name);
                    let level = track.trim_end_matches("_mirrored").trim_end_matches("_reversed");
                    let (reversed, mirrored) = (track.contains("_reversed"), track.ends_with("_mirrored"));
                    let challenge = cup::LEVELS.iter().find(|(l, ..)| *l == level).map(|(_, _, normal, back)| if reversed && !mirrored { *back } else { *normal });
                    if !multi && challenge.is_some_and(|time| lap < time) && self.save.challenges.insert(track.clone())
                    {
                        hud.message = Some((label("Challenge Time Beaten!"), PRACTICE_MESSAGE_TIME));
                    }
                    else if place == Some(0)
                    {
                        hud.message = Some((label("New best trial time!"), PRACTICE_MESSAGE_TIME));
                    }
                    changed = true;
                }
            }
            let seen = (player.best_lap, player.finish_time.is_some());
            if seen == self.record_seen[local] { continue; }
            self.record_seen[local] = seen;

            if let Some(time) = player.best_lap.filter(|_| !track.is_empty())
            {
                if self.save.record_lap(&track, time, &car, &player.name)
                {
                    changed = true;
                    hud.message = Some((label("New best lap!"), PRACTICE_MESSAGE_TIME));
                }
            }
            if let Some(time) = player.finish_time.filter(|_| !track.is_empty())
            {
                if self.save.record_race(&track, race.settings.laps, time, &car, &player.name)
                {
                    changed = true;
                    hud.message = Some((label("New track record!"), PRACTICE_MESSAGE_TIME));
                }
                // first against others - alone is no win
                if !multi && player.position == 1 && race.racers.len() > 1 && self.save.record_win(&track)
                {
                    changed = true;
                }
            }
        }
        if !changed { return; }
        console_log!("save: new record on {}", track);
        self.save.store(&self.save_game);
        if let Some(run) = self.menu.as_mut() { run.menu.saved = self.save.clone(); }
    }

    // hit1 / hit2 of the game when a car body hits something - the converter puts both into the scene sound list
    fn play_hits(&mut self, state: &mut State)
    {
        let source = |name: &str| state.resources.sound_sources.values().find(|source| source.read().unwrap().name == name).cloned();
        let (Some(soft), Some(hard)) = (source("hit1"), source("hit2")) else { return; };

        // the speed and, for a bot, its node - the bots sound at their car, the player car is heard everywhere; split screen: every car at its place
        let multi = self.race.as_ref().is_some_and(|(race, _)| race.multi());
        let mut hits: Vec<(f32, Option<NodeItem>)> = vec![];
        for scene in &state.scenes
        {
            for contact in scene.physics.contact_events().iter().filter(|contact| contact.started() && contact.impact_speed >= HIT_MIN_SPEED)
            {
                let vehicle = [contact.target, contact.other].into_iter().find_map(|target| match target { ContactTarget::Vehicle { node_id } => Some(node_id), _ => None });
                let Some(vehicle) = vehicle else { continue; };
                if self.last_hit.get(&vehicle).is_some_and(|frame| self.frames - frame < HIT_COOLDOWN_FRAMES) { continue; }

                self.last_hit.insert(vehicle, self.frames);
                let bot = scene.find_node_by_id(vehicle).filter(|node| multi || node.read().unwrap().tags.contains(field::BOT_TAG));
                hits.push((contact.impact_speed, bot));
            }
        }

        hits.sort_by(|a, b| b.0.total_cmp(&a.0));
        hits.truncate(MAX_HITS_PER_FRAME);
        for (speed, bot) in hits
        {
            let volume = ((speed - HIT_MIN_SPEED) / (HIT_FULL_SPEED - HIT_MIN_SPEED)).clamp(0.15, 1.0);
            let hard_hit = speed >= HIT_HARD_SPEED;
            match bot
            {
                Some(node) =>
                {
                    let name = field::HIT_SOUNDS[hard_hit as usize].0;
                    let components = node.read().unwrap().find_components::<Sound>();
                    let Some(component) = components.iter().find(|component| component.read().unwrap().get_base().name == name) else { continue; };
                    component_downcast_mut!(component, Sound);
                    component.get_data_mut().get_mut().volume = volume;
                    component.stop();
                    component.start();
                },
                None => state.play_one_shot_sound_source(if hard_hit { hard.clone() } else { soft.clone() }, volume),
            }
        }
    }
}

// split screen: the cameras of the players 2.. go, the camera of the start car gets its viewport back
fn remove_player_cameras(scene: &mut Scene, racing: &MenuRace)
{
    for id in &racing.cameras
    {
        scene.delete_camera_by_id(*id);
    }
    if let Some((name, [x, y, w, h])) = &racing.main_view
    {
        if let Some(cam) = scene.get_game_camera_mut(name) { cam.update_viewport(*x, *y, *w, *h); }
    }
}

// the countdown beeps (the GO plays two at once) - at full volume they were twice as loud as the rest
const COUNTDOWN_VOLUME: f32 = 0.4;

// m from the car to a stunt star to collect it
const STAR_REACH: f32 = 1.2;
// s the practice message stays
const PRACTICE_MESSAGE_TIME: f32 = 2.0;

fn node_position(node: &NodeItem) -> Option<Vector3<f32>>
{
    let node = node.read().unwrap();
    let transform = node.find_component::<Transformation>()?;
    component_downcast!(transform, Transformation);
    let position = transform.get_data().position;
    Some(position)
}

// hidden now: the pickups and stars the rule picks (object type, star type) - the game shows the practice stars (type 1) in practice mode and the stunt arena only
fn hide_items(scene: &Scene, rule: impl Fn(ObjectType, Option<i64>) -> bool) -> Vec<NodeItem>
{
    let mut hidden = vec![];
    for_each_node(&scene.nodes, &mut |node|
    {
        let (kind, star_type) = { let read = node.read().unwrap(); (object_kind(&read), read.extras.get::<i64>("revolt_param_type").copied()) };
        let hide = kind.is_some_and(|kind| matches!(kind, ObjectType::Pickup | ObjectType::Star) && rule(kind, star_type));
        if hide && node.read().unwrap().settings.visible
        {
            node.write().unwrap().settings.visible = false;
            hidden.push(node.clone());
        }
    });
    hidden
}

// the type of a .fob object of the scene - the model inside its glb carries the type too, without the params
fn object_kind(node: &Node) -> Option<ObjectType>
{
    if !node.tags.contains("revolt_object") { return None; }
    node.extras.get::<i64>("revolt_object_type").and_then(|id| ObjectType::from_id(*id))
}

// the assets folder next to the scene file
fn assets_folder(scene: &Scene) -> String
{
    match project_folder(scene)
    {
        folder if folder.is_empty() => "assets".to_string(),
        folder => format!("{folder}/assets"),
    }
}

// the folder of the scene file, empty if it has none
fn project_folder(scene: &Scene) -> String
{
    let source = scene.source.as_ref().map(|source| source.origin_path.replace('\\', "/")).unwrap_or_default();
    source.rsplit_once('/').map(|(folder, _)| folder.to_string()).unwrap_or_default()
}

// the time of this frame, s - at most 0.25 so a stall does not jump the clocks
fn frame_seconds(state: &State) -> f32
{
    (state.stats.frame_scale / REFERENCE_UPDATE_FRAMES).min(0.25)
}

// master, effects, music volume of the audio device - written only when they change
fn set_volumes(state: &mut State, (master, effects, music): (f32, f32, f32))
{
    let mut device = state.io.audio_device.write().unwrap();
    let data = device.data.get_ref();
    if (data.volume, data.effects_volume, data.music_volume) == (master, effects, music) { return; }
    let data = device.data.get_mut();
    data.volume = master;
    data.effects_volume = effects;
    data.music_volume = music;
}

// one shots of a sound of the scene sound list at these speeds (1 = as recorded)
fn play_pitched(state: &mut State, name: &str, pitches: &[f32])
{
    play_pitched_at(state, name, pitches, 1.0);
}

fn play_pitched_at(state: &mut State, name: &str, pitches: &[f32], volume: f32)
{
    if pitches.is_empty() { return; }
    let Some(source) = state.resources.sound_sources.values().find(|source| source.read().unwrap().name == name).cloned() else
    {
        console_warning!("sound '{}' is not in the scene sound list - re-convert the level", name);
        return;
    };
    for &pitch in pitches
    {
        let mut sound = Sound::new(name, source.clone(), SoundType::Stereo, false);
        sound.get_data_mut().get_mut().speed = pitch;
        sound.get_data_mut().get_mut().volume = volume;
        sound.start();
        sound.update_state(None, None, true);
        state.oneshot_sounds.push(sound);
    }
}

// every node below the given ones, depth first
pub fn for_each_node(nodes: &Vec<NodeItem>, f: &mut dyn FnMut(&NodeItem))
{
    for node in nodes
    {
        f(node);
        let children = node.read().unwrap().nodes.clone();
        for_each_node(&children, f);
    }
}

// the material of one node, if it has one
fn change_material(node: &NodeItem, change: impl Fn(&mut MaterialData))
{
    let node = node.read().unwrap();
    if let Some(material) = node.find_component::<Material>()
    {
        component_downcast_mut!(material, Material);
        change(material.get_data_mut().get_mut());
    }
}

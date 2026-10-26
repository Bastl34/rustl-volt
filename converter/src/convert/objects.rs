// .fob game objects -> one glb per model + the scene objects that place them
// the cup object of the frontend (type 48, flag 0 = the cup)
const CUP: i32 = 48;
// the object thrower (type 42, flag 1 = the type it throws): its model is the thrown object, not placed - the app code makes it
const THROWER: i32 = 42;

// the type whose model an object shows - a thrower the one it throws
fn model_kind(object: &Object) -> i32
{
    if object.kind == THROWER { object.flags[1] } else { object.kind }
}

// the shared models (models/, levels/frontend/, cars/trolley/) use the texture pages of the level, so the glbs are per level

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use anyhow::Result;
use serde_json::{json, Value};

use super::bodies;
use super::effects;
use super::frontend;
use super::motion;
use super::sounds;
use super::geometry::{rotation_quat, to_gltf, MeshWriter};
use super::scene::relative_path;
use super::texture::Page;
use super::{Log, Settings};
use crate::glb::Glb;
use crate::revolt::find_file;
use crate::revolt::fob::{self, Object};
use crate::revolt::mesh::{Mesh, FACE_TEXANIM};
use crate::revolt::object_types::{self, object_type};
use crate::revolt::world::TexFrame;

pub struct ObjectModel
{
    // relative to the game folder: models/beachball.m
    pub file: String,
    // file name without extension, lower case
    pub stem: String,
    pub mesh: Mesh,
}

#[derive(Default)]
pub struct LevelObjects
{
    pub objects: Vec<Object>,
    // per object: index into models, None = the type has no model or it could not be read
    pub model_of: Vec<Option<usize>>,
    // per object: its moving parts (object_types::parts), indices into models
    pub parts_of: Vec<Vec<Option<usize>>>,
    pub models: Vec<ObjectModel>,
}

impl LevelObjects
{
    pub fn texture_pages(&self) -> impl Iterator<Item = i32> + '_
    {
        self.models.iter().flat_map(|m| &m.mesh.polygons).filter(|p| !p.has(FACE_TEXANIM)).map(|p| p.texture as i32)
    }
}

// reads the .fob and every model it uses
pub fn load(game_dir: &Path, dir: &Path, level: &str, log: &Log) -> Result<LevelObjects>
{
    let Some(fob_path) = find_file(dir, &format!("{level}.fob")) else
    {
        log.info("  no .fob file - no game objects");
        return Ok(LevelObjects::default());
    };
    log.read(&fob_path);
    let objects = fob::load(&fob_path)?;

    let mut by_file: HashMap<String, Option<usize>> = HashMap::new();
    let mut models: Vec<ObjectModel> = vec![];
    let mut model_of = vec![];
    let mut parts_of = vec![];
    // frontend: one car box model per car id, with the picture of that car
    let mut car_boxes: HashMap<i32, Option<usize>> = HashMap::new();

    // one model file once - the game folder mixes upper and lower case
    let mut load_model = |file: &str, offset: i16, models: &mut Vec<ObjectModel>| -> Option<usize>
    {
        *by_file.entry(file.to_string()).or_insert_with(||
        {
            let relative = Path::new(file);
            let Some(path) = find_file(&game_dir.join(relative.parent()?), &relative.file_name()?.to_string_lossy()) else
            {
                log.warn(format!("  {file} not found"));
                return None;
            };
            log.read(&path);
            match Mesh::load(&path)
            {
                Ok((mut mesh, lods)) =>
                {
                    if lods > 1 { log.info(format!("  {file}: {lods} levels of detail - the first one is used")); }
                    if offset != 0
                    {
                        for polygon in mesh.polygons.iter_mut().filter(|p| !p.has(FACE_TEXANIM) && p.texture >= 0)
                        {
                            polygon.texture += offset;
                        }
                        log.info(format!("  {file}: texture pages + {offset} (like the game)"));
                    }
                    let stem = path.file_stem()?.to_string_lossy().to_lowercase();
                    models.push(ObjectModel { file: file.to_string(), stem, mesh });
                    Some(models.len() - 1)
                },
                Err(err) =>
                {
                    log.warn(format!("  {file}: {err:#} - the objects get no model"));
                    None
                },
            }
        })
    };

    for object in &objects
    {
        if object_type(object.kind).is_none()
        {
            log.warn(format!("  unknown object type {} at {:?}", object.kind, object.position));
        }

        let offset = object_types::page_offset(model_kind(object));
        let mut model = object_types::model(object.kind, &object.flags, level).and_then(|file| load_model(&file, offset, &mut models));
        if object.kind == frontend::CAR_BOX && frontend::is_frontend(level)
        {
            model = model.and_then(|base| *car_boxes.entry(object.flags[0]).or_insert_with(||
            {
                let car = object.flags[0];
                let Some(mesh) = frontend::car_box(&models[base].mesh, car) else
                {
                    log.warn("  carbox.m: no picture polygon (flag 0x800) - all boxes look the same");
                    return Some(base);
                };
                models.push(ObjectModel { file: models[base].file.clone(), stem: frontend::car_box_stem(car), mesh });
                Some(models.len() - 1)
            }));
        }
        model_of.push(model);
        let parts: Vec<Option<usize>> = object_types::parts(object.kind, level).iter().map(|file| load_model(file, offset, &mut models)).collect();
        parts_of.push(parts);
    }

    Ok(LevelObjects { objects, model_of, parts_of, models })
}

// writes the model glbs, returns the scene objects - objects without a model become empty nodes (markers)
pub fn write(level_objects: &LevelObjects, level: &str, pages: &[Option<Page>], anims: &[Vec<TexFrame>], settings: &Settings, log: &Log, progress: &dyn Fn(f32)) -> Result<Vec<Value>>
{
    let LevelObjects { objects, model_of, parts_of, models } = level_objects;
    if objects.is_empty()
    {
        log.info("  none");
        return Ok(vec![]);
    }

    let objects_dir = settings.level_dir(level).join("objects");
    let mut sources = vec![];
    // models written as dynamic bodies - their objects collide
    let mut dynamic_models = BTreeSet::new();

    log.info(format!("  {:<16} {:>7} {:>9} {:>9}  {}", "model", "objects", "polygons", "triangles", "type (extras of the model)"));
    for (m, model) in models.iter().enumerate()
    {
        progress(m as f32 / models.len() as f32);
        let users: Vec<&Object> = objects.iter().zip(model_of).filter(|(_, of)| **of == Some(m)).map(|(o, _)| o).collect();
        // only a part of moving objects (propeller, wheels, ...) or only moving objects - in the glbs of those objects
        if users.iter().all(|user| motion::moves(user.kind))
        {
            sources.push(String::new());
            continue;
        }
        let kinds: BTreeSet<i32> = users.iter().map(|o| model_kind(o)).collect();
        let kind = *kinds.first().unwrap_or(&-1);
        let kind_type = object_type(kind);

        let mut glb = Glb::default();
        let mut writer = MeshWriter::new(pages, anims, settings);
        let mut primitives = BTreeMap::new();
        for polygon in &model.mesh.polygons
        {
            writer.add(&mut primitives, &model.mesh, polygon, None, -1);
        }

        // pages the level does not have (fx pages and others) - those polygons stay untextured
        let missing: BTreeSet<i16> = model.mesh.polygons.iter()
            .filter(|p| !p.has(FACE_TEXANIM) && p.texture >= 0 && usize::try_from(p.texture).ok().and_then(|i| pages.get(i)).is_none_or(|page| page.is_none()))
            .map(|p| p.texture)
            .collect();

        let mut extras = serde_json::Map::new();
        extras.insert("revolt_object_type".into(), json!(kind));
        extras.insert("revolt_object_type_name".into(), json!(kind_type.map_or("unknown", |t| t.name)));
        extras.insert("revolt_object_flags".into(), json!(kind_type.map(|t| t.flags.join(",")).unwrap_or_default()));
        extras.insert("revolt_model".into(), json!(model.file));

        let path = objects_dir.join(format!("{}.glb", model.stem));
        let mut body_text = None;
        let triangles = match writer.write(&mut glb, &model.stem, primitives)
        {
            Some(written) =>
            {
                let root = match bodies::body(kind)
                {
                    Some(body) => match bodies::write(&mut glb, &mut writer, kind, &body, &model.file, &model.stem, &model.mesh, written.mesh, extras.clone(), &settings.game_dir, settings.scale, settings.mirrored, log)
                    {
                        Ok(dynamic) => { body_text = Some(dynamic.text); dynamic.root },
                        Err(err) =>
                        {
                            log.warn(format!("  {}: {err:#} - static, without collision", model.stem));
                            glb.node(json!({ "name": model.stem, "mesh": written.mesh, "extras": extras }))
                        },
                    },
                    None =>
                    {
                        // frontend: the name wheel of the stand, the track pictures of the big screen
                        let first_user = model_of.iter().position(|of| *of == Some(m));
                        let parts: Vec<&Mesh> = first_user.map(|i| parts_of[i].iter().flatten().map(|&p| &models[p].mesh).collect()).unwrap_or_default();
                        let children = match frontend::is_frontend(level) { true => frontend::children(kind, &mut glb, &mut writer, &parts, &settings.game_dir, settings.scale, log), false => vec![] };
                        match children.is_empty()
                        {
                            true => glb.node(json!({ "name": model.stem, "mesh": written.mesh, "extras": extras })),
                            false => glb.node(json!({ "name": model.stem, "mesh": written.mesh, "extras": extras, "children": children })),
                        }
                    },
                };
                glb.save(&path, &[root])?;
                written.triangles
            },
            None => { log.warn(format!("  {}: no polygons", model.stem)); 0 },
        };
        if let Some(text) = &body_text
        {
            dynamic_models.insert(m);
            log.info(format!("  {:<16} {text}", ""));
        }

        let type_text = kinds.iter().map(|k| format!("{k} {}", object_type(*k).map_or("unknown", |t| t.name))).collect::<Vec<_>>().join(", ");
        log.info(format!("  {:<16} {:>7} {:>9} {triangles:>9}  {type_text}", model.stem, users.len(), model.mesh.polygons.len()));
        if kinds.len() > 1
        {
            log.warn(format!("  {}: used by several types - the extras name the first one", model.stem));
        }
        if !missing.is_empty()
        {
            log.warn(format!("  {}: texture pages {missing:?} are not in the level - untextured for now", model.stem));
        }
        log.write(&path);
        sources.push(relative_path(&settings.project_dir, &path));
    }

    log.info("");
    let scale = settings.scale;
    let mut scene_objects = vec![];
    let mut counters: HashMap<i32, usize> = HashMap::new();
    let mut markers = 0;
    let mut ambients = (0, 0);
    let mut moving = 0;
    // the beam of the lasers, written with the first one
    let mut laser_glb: Option<String> = None;
    for (index, (object, model)) in objects.iter().zip(model_of).enumerate()
    {
        let kind_type = object_type(object.kind);
        let type_name = kind_type.map_or(format!("type {}", object.kind), |t| t.name.to_string());
        let number = counters.entry(object.kind).or_default();
        let type_tag = type_name.to_lowercase().replace(' ', "_").replace('.', "");
        let name = format!("{type_tag}_{:03}", number);
        *number += 1;

        let up = object.up;
        let look = object.look;
        let mut right = [up[1] * look[2] - up[2] * look[1], up[2] * look[0] - up[0] * look[2], up[0] * look[1] - up[1] * look[0]];
        let mut look = look;
        // the outer cups of the frontend (Bronze, Platinum) face away with their .fob axes - turned around the up axis
        if object.kind == CUP && matches!(object.flags[0], 0 | 3) { right = right.map(|v| -v); look = look.map(|v| -v); }
        let rotation = rotation_quat(&[right, up, look]);
        let position = to_gltf(object.position).map(|v| v * scale);

        let flags = match kind_type
        {
            Some(t) => object.flags.iter().zip(t.flags).filter(|(_, label)| !label.is_empty()).map(|(v, label)| format!("{label} {v}")).collect::<Vec<_>>().join(", "),
            None => format!("{:?}", object.flags),
        };
        let source = model.map(|m| models[m].stem.clone()).unwrap_or_else(|| "(marker, no model)".to_string());
        let [px, py, pz] = object.position;
        log.info(format!("  {name:<22} {:>2} {type_name:<16} ({px:.0}, {py:.0}, {pz:.0}) --> ({:.2}, {:.2}, {:.2}) m  {source}  {flags}",
            object.kind, position[0], position[1], position[2]));

        // type and flags as they are - the units of ranges and speeds are unknown
        let mut extras = serde_json::Map::new();
        extras.insert("revolt_object_type".into(), json!(object.kind));
        extras.insert("revolt_object_type_name".into(), json!(type_name));
        for (i, value) in object.flags.iter().enumerate()
        {
            extras.insert(format!("revolt_flag_{i}"), json!(*value));
        }
        if let Some(t) = kind_type
        {
            for (value, label) in object.flags.iter().zip(t.flags).filter(|(_, label)| !label.is_empty())
            {
                extras.insert(format!("revolt_param_{}", label.replace([' ', '.'], "_").trim_matches('_').replace("__", "_")), json!(*value));
            }
        }
        if let Some(m) = model
        {
            extras.insert("revolt_model".into(), json!(models[*m].file));
        }

        // only the rigid bodies collide - the other objects move in code in the original (not done yet)
        let thrower = object.kind == THROWER;
        let dynamic = !thrower && model.is_some_and(|m| dynamic_models.contains(&m));
        let mut tags = vec![json!("revolt_object"), json!(type_tag)];
        if dynamic { tags.push(json!("revolt_physics")); }

        let mut scene_object = json!({
            "name": name,
            "options": { "settings": { "collision": dynamic, "camera_collision": false } },
            "position": position,
            "rotation": [0.0, 0.0, 0.0],
            "rotation_quat": rotation,
            "scale": [1.0, 1.0, 1.0],
            "extras": extras,
            // find them by tag in the game code: all objects, or all of one type (pickup, water_ripples, 3d_sound, ...), revolt_physics
            "tags": tags,
        });
        match model
        {
            Some(m) if thrower => { scene_object["extras"]["revolt_thrown_source"] = json!(sources[*m]); markers += 1; },
            Some(m) => scene_object["source"] = json!(sources[*m]),
            None => markers += 1,
        }

        // laser: a beam along the look axis, the app code gives it its length, flicker and sounds
        if object.kind == effects::LASER
        {
            if laser_glb.is_none()
            {
                let path = objects_dir.join("laser.glb");
                effects::write_laser(&path, &settings.game_dir, log)?;
                laser_glb = Some(relative_path(&settings.project_dir, &path));
            }
            scene_object["source"] = json!(laser_glb);
            for (role, id) in effects::LASER_SOUNDS
            {
                if let Some(sound) = sounds::level_sound(level, id) { scene_object["extras"][format!("revolt_sound_{role}")] = json!(sound); }
            }
        }

        // 3D sound: a spatial sound of the level block - the scene writer puts revolt_sound into the scene sound list
        if object.kind == 49
        {
            match sounds::ambient(level, &format!("revolt-{}-{name}", level.to_lowercase()), &object.flags, scale)
            {
                Some(ambient) =>
                {
                    scene_object["extras"]["revolt_sound"] = json!(ambient.sound);
                    scene_object["extras"]["revolt_sound_random"] = json!(ambient.random);
                    scene_object["extras"]["revolt_sound_min_distance"] = json!(ambient.min_distance);
                    scene_object["extras"]["revolt_sound_max_distance"] = json!(ambient.max_distance);
                    scene_object["components"] = json!([ambient.component]);
                    log.info(format!("  {:<22} sound {} ({}), full volume up to {:.1} m, the game stops it beyond {:.1} m", "", ambient.sound, if ambient.random { "random play" } else { "loop" }, ambient.min_distance, ambient.max_distance));
                    if ambient.random { ambients.1 += 1; } else { ambients.0 += 1; }
                },
                None => log.warn(format!("  {name}: no sound for name {} in level {level}", object.flags[0])),
            }
        }
        // a scripted motion: a glb of its own with the animation
        if motion::moves(object.kind) && model.is_some()
        {
            let mut glb = Glb::default();
            let mut writer = MeshWriter::new(pages, anims, settings);
            let models = motion::Models { models, model: *model, parts: &parts_of[index] };
            if let Some(written) = motion::write(&mut glb, &mut writer, objects, index, models, scale, settings.mirrored)
            {
                let path = objects_dir.join(format!("{name}.glb"));
                glb.save(&path, &[written.root])?;
                log.info(format!("  {:<22} motion: {}", "", written.text));
                log.write(&path);
                motion::place(&mut scene_object, &written, relative_path(&settings.project_dir, &path), scale);
                for (role, sound) in sounds::object_sounds(object.kind, level)
                {
                    scene_object["extras"][format!("revolt_sound_{role}")] = json!(sound);
                    log.info(format!("  {:<22} sound {role}: {sound}", ""));
                }
                moving += 1;
            }
        }
        scene_objects.push(scene_object);
    }

    if moving > 0
    {
        log.info(format!("  {moving} moving objects with a glb of their own (tag revolt_animated) - the app code starts the animations"));
    }
    let dynamic_count = scene_objects.iter().filter(|o| o["options"]["settings"]["collision"] == json!(true)).count();
    log.info(format!("  {} objects · {} with a model · {markers} markers without one (sounds, fog, sparks, ...) · {dynamic_count} dynamic bodies (tag revolt_physics), the rest without collision · type, flags and params in the extras of every object", scene_objects.len(), scene_objects.len() - markers));
    if ambients != (0, 0)
    {
        log.info(format!("  3D sounds: {} loops, {} random play (tag 3d_sound, component '{}', the app code starts them)", ambients.0, ambients.1, sounds::AMBIENT));
    }
    Ok(scene_objects)
}

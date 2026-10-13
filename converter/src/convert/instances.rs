// .fin instances -> one glb per .prm model (+ its .ncp as hidden collision nodes) + the scene objects that place them

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::{json, Value};

use super::collision::{self, CollisionStats};
use super::geometry::{rotation_quat, to_gltf, MeshWriter};
use super::scene::relative_path;
use super::texture::Page;
use super::{Log, Settings};
use crate::glb::Glb;
use crate::revolt::fin::{self, Instance, FIN_FLAG_NAMES, FIN_HIDE, FIN_NO_CAMERA_COLLISION, FIN_NO_OBJECT_COLLISION};
use crate::revolt::find_file;
use crate::revolt::mesh::{Mesh, FACE_TEXANIM};
use crate::revolt::ncp::{self, Ncp};
use crate::revolt::world::TexFrame;

pub struct Model
{
    // file name without extension, lower case: bush1
    pub stem: String,
    pub mesh: Mesh,
    // the original collides with an instance only if a .ncp of the same name exists
    pub ncp: Option<Ncp>,
}

impl Model
{
    // an empty .ncp collides with nothing
    pub fn collision(&self) -> bool
    {
        self.ncp.as_ref().is_some_and(|ncp| !ncp.polygons.is_empty())
    }
}

#[derive(Default)]
pub struct LevelInstances
{
    pub instances: Vec<Instance>,
    // per instance: index into models, None = no .prm found
    pub model_of: Vec<Option<usize>>,
    pub models: Vec<Model>,
}

impl LevelInstances
{
    // texture page per polygon of every model (animated ones excluded)
    pub fn texture_pages(&self) -> impl Iterator<Item = i32> + '_
    {
        self.models.iter().flat_map(|m| &m.mesh.polygons).filter(|p| !p.has(FACE_TEXANIM)).map(|p| p.texture as i32)
    }
}

// reads the .fin (in fin_dir - the reversed folder has its own) and every .prm it uses (in dir)
pub fn load(dir: &Path, fin_dir: &Path, level: &str, log: &Log) -> Result<LevelInstances>
{
    let Some(fin_path) = find_file(fin_dir, &format!("{level}.fin")) else
    {
        log.info("  no .fin file - no instances");
        return Ok(LevelInstances::default());
    };
    log.read(&fin_path);
    let instances = fin::load(&fin_path)?;

    let prm_files = list_prm(dir);
    let mut by_name: HashMap<String, Option<usize>> = HashMap::new();
    let mut models: Vec<Model> = vec![];
    let mut model_of = vec![];

    for instance in &instances
    {
        let model = *by_name.entry(instance.name.clone()).or_insert_with(||
        {
            let path = resolve_model(&prm_files, &instance.name, log)?;
            log.read(&path);
            let mesh = match Mesh::load(&path)
            {
                Ok((mesh, lods)) =>
                {
                    if lods > 1 { log.info(format!("  {}: {lods} levels of detail - the first one is used", path.display())); }
                    mesh
                },
                Err(err) => { log.warn(format!("  {}: {err:#}", path.display())); return None; },
            };
            let stem = path.file_stem()?.to_string_lossy().to_lowercase();
            let ncp = find_file(dir, &format!("{stem}.ncp")).and_then(|path|
            {
                log.read(&path);
                ncp::load(&path).map_err(|err| log.warn(format!("  {}: {err:#} - no collision", path.display()))).ok()
            });
            models.push(Model { stem, mesh, ncp });
            Some(models.len() - 1)
        });
        model_of.push(model);
    }

    Ok(LevelInstances { instances, model_of, models })
}

// writes the model glbs, returns the scene objects of all instances
pub fn write(level_instances: &LevelInstances, level: &str, pages: &[Option<Page>], anims: &[Vec<TexFrame>], settings: &Settings, log: &Log, progress: &dyn Fn(f32)) -> Result<Vec<Value>>
{
    let LevelInstances { instances, model_of, models } = level_instances;
    if instances.is_empty()
    {
        log.info("  none");
        return Ok(vec![]);
    }

    let models_dir = settings.level_dir(level).join("models");
    let mut sources = vec![];
    let mut collision_stats = CollisionStats::default();

    log.info(format!("  {:<12} {:>9} {:>9} {:>9}  {}", "model", "instances", "polygons", "triangles", "collision"));
    for (m, model) in models.iter().enumerate()
    {
        progress(m as f32 / models.len() as f32);
        // hidden instances are not drawn in the original - a model with only hidden ones is not written
        let count = instances.iter().zip(model_of).filter(|(instance, of)| **of == Some(m) && instance.flag & FIN_HIDE == 0).count();
        if count == 0
        {
            log.info(format!("  {:<12} skipped - only hidden instances", model.stem));
            sources.push(String::new());
            continue;
        }
        let mut glb = Glb::default();
        let mut writer = MeshWriter::new(pages, anims, settings);
        let mut primitives = BTreeMap::new();
        for polygon in &model.mesh.polygons
        {
            writer.add(&mut primitives, &model.mesh, polygon, None, -1);
        }

        let path = models_dir.join(format!("{}.glb", model.stem));
        let mut stats = CollisionStats::default();
        let triangles = match writer.write(&mut glb, &model.stem, primitives)
        {
            Some(written) =>
            {
                let root = match &model.ncp
                {
                    Some(model_ncp) if model.collision() =>
                    {
                        // the visible mesh does not collide, the hidden .ncp nodes do
                        let visible = glb.node(json!({ "name": format!("{}_mesh", model.stem), "mesh": written.mesh, "extras": { "rustl_collision": false } }));
                        let mut children = vec![visible];
                        children.extend(collision::add_nodes(&mut glb, model_ncp, settings.scale, true, &mut stats));
                        glb.node(json!({ "name": model.stem, "children": children }))
                    },
                    _ => glb.node(json!({ "name": model.stem, "mesh": written.mesh })),
                };
                glb.save(&path, &[root])?;
                written.triangles
            },
            None => { log.warn(format!("  {}: no polygons", model.stem)); 0 },
        };
        collision_stats.add(&stats);

        let collision = match &model.ncp
        {
            Some(_) if model.collision() => format!("yes ({}.ncp, {} polygons --> {} triangles)", model.stem, stats.polygons, stats.triangles),
            Some(_) => format!("no ({}.ncp is empty)", model.stem),
            None => "no".to_string(),
        };
        log.info(format!("  {:<12} {count:>9} {:>9} {triangles:>9}  {collision}", model.stem, model.mesh.polygons.len()));
        log.write(&path);
        sources.push(relative_path(&settings.project_dir, &path));
    }

    log.info("");
    let scale = settings.scale;
    let mut objects = vec![];
    let mut counters: HashMap<usize, usize> = HashMap::new();
    let mut hidden = 0;
    let mut colliding = 0;
    for (instance, model) in instances.iter().zip(model_of)
    {
        let Some(m) = *model else { continue; };
        let model = &models[m];
        if instance.flag & FIN_HIDE != 0
        {
            hidden += 1;
            continue;
        }

        // collision needs a .ncp of the model, the flags can switch it off per instance
        let collision = model.collision() && instance.flag & FIN_NO_OBJECT_COLLISION == 0;
        let camera_collision = model.collision() && instance.flag & FIN_NO_CAMERA_COLLISION == 0;
        if collision { colliding += 1; }
        let number = counters.entry(m).or_default();
        let name = format!("{}_{:03}", model.stem, number);
        *number += 1;

        let position = to_gltf(instance.position).map(|v| v * scale);
        let rotation = rotation_quat(&instance.axes);
        let [px, py, pz] = instance.position;
        let flags = FIN_FLAG_NAMES.iter().filter(|(bit, _)| instance.flag & bit != 0).map(|(_, name)| *name).collect::<Vec<_>>().join(", ");
        log.info(format!("  {name:<14} ({px:.0}, {py:.0}, {pz:.0}) --> ({:.2}, {:.2}, {:.2}) m  quat ({:.3}, {:.3}, {:.3}, {:.3})  collision {}  flags {} ({flags})  color {:?}",
            position[0], position[1], position[2], rotation[0], rotation[1], rotation[2], rotation[3],
            if collision { "yes" } else { "no" }, instance.flag, instance.color));

        objects.push(json!({
            "source": sources[m],
            "name": name,
            "options": { "settings": { "collision": collision, "camera_collision": camera_collision } },
            // find them by tag in the game code: all instances, or all of one model
            "tags": ["revolt_instance", model.stem],
            // the .fin values as they are (re-volt units)
            "extras": {
                "revolt_model": model.stem,
                "revolt_fin_flags": instance.flag,
                "revolt_color_r": instance.color[0],
                "revolt_color_g": instance.color[1],
                "revolt_color_b": instance.color[2],
                "revolt_env_color": format!("{:08x}", instance.env_color),
                "revolt_priority": instance.priority,
                "revolt_lod_bias": instance.lod_bias as f64,
            },
            "position": position,
            "rotation": [0.0, 0.0, 0.0],
            "rotation_quat": rotation,
            "scale": [1.0, 1.0, 1.0],
        }));
    }

    log.info("");
    log.info("  collision of the models (.ncp --> hidden nodes collision_<surface> in the model glb, the visible mesh has none)");
    collision_stats.log(log);

    let missing = model_of.iter().filter(|m| m.is_none()).count();
    if missing > 0
    {
        log.warn(format!("  {missing} instances skipped - their .prm was not found"));
    }
    if hidden > 0
    {
        log.info(format!("  {hidden} hidden instances left out (fin flag hide)"));
    }
    log.info(format!("  {} instances · collision on {colliding} of them (.ncp of the model, not switched off by the fin flags) · the .fin values in the extras of every instance", objects.len()));
    Ok(objects)
}

fn list_prm(dir: &Path) -> Vec<PathBuf>
{
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir).into_iter().flatten()
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("prm")))
        .collect();
    files.sort();
    files
}

// the .fin keeps 8 characters of the name: FOUNTAIN -> fountain1.prm
fn resolve_model(prm_files: &[PathBuf], name: &str, log: &Log) -> Option<PathBuf>
{
    let stem = |path: &PathBuf| path.file_stem().map(|s| s.to_string_lossy().to_uppercase()).unwrap_or_default();
    let name = name.to_uppercase();

    if let Some(exact) = prm_files.iter().find(|path| stem(path) == name)
    {
        return Some(exact.clone());
    }

    let candidates: Vec<&PathBuf> = prm_files.iter().filter(|path| stem(path).starts_with(&name)).collect();
    match candidates.as_slice()
    {
        [] =>
        {
            log.warn(format!("  {name}: no .prm found"));
            None
        },
        [single] =>
        {
            log.info(format!("  {name} --> {} (name cut to 8 characters in the .fin)", single.file_name()?.to_string_lossy()));
            Some((*single).clone())
        },
        [first, ..] =>
        {
            let all: Vec<String> = candidates.iter().filter_map(|path| path.file_name().map(|n| n.to_string_lossy().to_string())).collect();
            log.warn(format!("  {name} is ambiguous ({}) - taking {}", all.join(", "), first.file_name()?.to_string_lossy()));
            Some((*first).clone())
        },
    }
}

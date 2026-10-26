// models/go3.m, go2.m, go1.m, gogo.m - the 3D countdown of the game - as triangles for the HUD of the app code
// assets/models/countdown.json: per model a list of triangles, a corner = [x, y, z, r, g, b] in game units (y down, clockwise), sRGB colors

use anyhow::{Context, Result};
use serde_json::{json, Map, Value};

use super::{Log, Settings};
use crate::revolt::find_file;
use crate::revolt::mesh::Mesh;

const MODELS: [&str; 4] = ["go3", "go2", "go1", "gogo"];

pub fn convert(settings: &Settings, log: &Log) -> Result<()>
{
    let dir = settings.game_dir.join("models");
    let mut models = Map::new();

    for name in MODELS
    {
        let path = find_file(&dir, &format!("{name}.m")).with_context(|| format!("models/{name}.m missing"))?;
        log.read(&path);
        let (mesh, _) = Mesh::load(&path)?;

        let mut triangles = vec![];
        for polygon in &mesh.polygons
        {
            let corners = polygon.corners();
            if polygon.indices[..corners].iter().any(|&i| i as usize >= mesh.vertices.len()) { continue; }
            let corner = |c: usize|
            {
                let p = mesh.vertices[polygon.indices[c] as usize].position;
                let color = polygon.colors[c];
                json!([p[0], p[1], p[2], (color >> 16) & 0xff, (color >> 8) & 0xff, color & 0xff])
            };
            triangles.push(json!([corner(0), corner(1), corner(2)]));
            if corners == 4
            {
                triangles.push(json!([corner(0), corner(2), corner(3)]));
            }
        }
        log.info(format!("  {name}: {} triangles", triangles.len()));
        models.insert(name.into(), Value::Array(triangles));
    }

    let out = settings.out_dir.join("models").join("countdown.json");
    std::fs::create_dir_all(out.parent().unwrap())?;
    std::fs::write(&out, serde_json::to_string(&Value::Object(models))?)?;
    log.write(&out);
    Ok(())
}

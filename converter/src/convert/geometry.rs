// re-volt polygons -> glb meshes: primitives per material, materials with lazily embedded texture pages
// used for the level (.w) and the models (.prm)

use std::collections::{BTreeMap, HashMap};

use serde_json::{json, Value};

use super::texture::{additive_png, Page};
use super::{Log, Settings};
use crate::glb::Glb;
use crate::revolt::mesh::*;
use crate::revolt::world::TexFrame;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Blend
{
    Opaque,
    Alpha,
    Additive,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MaterialKey
{
    pub texture: i16,
    pub blend: Blend,
    pub double_sided: bool,
    pub env: bool,
    pub mirror: bool,
    // index of the .rim plane the polygon lies on, -1 = none
    pub mirror_plane: i16,
    // texture animation index, -1 = none
    pub anim: i16,
}

pub struct WrittenMesh
{
    pub mesh: usize,
    pub primitives: usize,
    pub vertices: usize,
    pub triangles: usize,
}

// one per glb - the texture pages are embedded on first use
pub struct MeshWriter<'a>
{
    pages: &'a [Option<Page>],
    anims: &'a [Vec<TexFrame>],
    settings: &'a Settings,
    textures: HashMap<usize, usize>,
    // the page variants for additive polygons
    additive_textures: HashMap<usize, usize>,
    materials: HashMap<MaterialKey, usize>,
    pub material_triangles: BTreeMap<MaterialKey, usize>,
    pub inset_polygons: usize,
    pub bad_polygons: usize,
}

impl<'a> MeshWriter<'a>
{
    pub fn new(pages: &'a [Option<Page>], anims: &'a [Vec<TexFrame>], settings: &'a Settings) -> Self
    {
        Self
        {
            pages, anims, settings,
            textures: HashMap::new(),
            additive_textures: HashMap::new(),
            materials: HashMap::new(),
            material_triangles: BTreeMap::new(),
            inset_polygons: 0,
            bad_polygons: 0,
        }
    }

    pub fn texture_count(&self) -> usize { self.textures.len() + self.additive_textures.len() }
    pub fn material_count(&self) -> usize { self.materials.len() }

    // one polygon into the primitive of its material
    pub fn add(&mut self, primitives: &mut BTreeMap<MaterialKey, Primitive>, mesh: &Mesh, polygon: &Polygon, env_color: Option<u32>, mirror_plane: i16)
    {
        self.add_with_target(primitives, mesh, polygon, env_color, mirror_plane, None);
    }

    // with a morph target: a mesh with the same vertices in another pose (dragon2 -> dragon3)
    pub fn add_with_target(&mut self, primitives: &mut BTreeMap<MaterialKey, Primitive>, mesh: &Mesh, polygon: &Polygon, env_color: Option<u32>, mirror_plane: i16, target: Option<&Mesh>)
    {
        if polygon.has(FACE_SKIP) { return; }

        // animated polygons show the first frame for now
        let (texture, uvs) = match polygon.has(FACE_TEXANIM).then(|| self.anims.get(polygon.texture as usize)).flatten().and_then(|frames| frames.first())
        {
            Some(frame) => (frame.texture as i16, frame.uvs),
            None => (polygon.texture, polygon.uvs),
        };
        let uvs = match self.page(texture)
        {
            Some(page) if self.settings.uv_inset =>
            {
                self.inset_polygons += 1;
                inset_uvs(uvs, polygon.corners(), page.width, page.height)
            },
            _ => uvs,
        };

        let blend = match (polygon.has(FACE_TRANSLUCENT), polygon.has(FACE_ADDITIVE))
        {
            (false, _) => Blend::Opaque,
            (true, false) => Blend::Alpha,
            (true, true) => Blend::Additive,
        };
        let key = MaterialKey
        {
            texture,
            blend,
            double_sided: polygon.has(FACE_DOUBLE),
            env: polygon.has(FACE_ENV),
            mirror: polygon.has(FACE_MIRROR),
            mirror_plane,
            anim: if polygon.has(FACE_TEXANIM) { polygon.texture } else { -1 },
        };

        if !primitives.entry(key).or_default().add(mesh, polygon, &uvs, blend, env_color, target, self.settings.scale)
        {
            self.bad_polygons += 1;
        }
    }

    // the primitives as one glb mesh - None if there is nothing in it
    pub fn write(&mut self, glb: &mut Glb, name: &str, primitives: BTreeMap<MaterialKey, Primitive>) -> Option<WrittenMesh>
    {
        let mut json = vec![];
        let mut vertices = 0;
        let mut triangles = 0;
        for (key, primitive) in primitives
        {
            let material = self.material(glb, key);
            let primitive_triangles = primitive.indices.len() / 3;
            *self.material_triangles.entry(key).or_default() += primitive_triangles;
            triangles += primitive_triangles;
            vertices += primitive.positions.len() / 3;
            json.push(primitive.write(glb, material));
        }

        if json.is_empty() { return None; }
        let primitives = json.len();
        let morph = json.iter().any(|primitive| primitive.get("targets").is_some());
        let mesh = glb.mesh(name, json);
        if morph { glb.mesh_weights(mesh, &[0.0]); }
        Some(WrittenMesh { mesh, primitives, vertices, triangles })
    }

    pub fn log_materials(&self, log: &Log)
    {
        match self.settings.uv_inset
        {
            true => log.info(format!("  UV inset: {} textured polygons pulled half a texel inward (atlas seams)", self.inset_polygons)),
            false => log.info("  UV inset: off"),
        }
        let additive: usize = self.material_triangles.iter().filter(|(key, _)| key.blend == Blend::Additive).map(|(_, count)| count).sum();
        if additive > 0
        {
            log.info(format!("  additive: {additive} triangles get a page variant with brightness as alpha (the engine has no additive blending yet)"));
        }
        if self.bad_polygons > 0
        {
            log.warn(format!("  {} polygons with invalid vertex indices skipped", self.bad_polygons));
        }

        for (key, count) in &self.material_triangles
        {
            let anim = format!("texanim {}", key.anim);
            let mirror = if key.mirror_plane >= 0 { format!("mirror plane {}", key.mirror_plane) } else { "mirror (no plane)".to_string() };
            let flags = [(key.double_sided, "double-sided"), (key.env, "env"), (key.mirror, mirror.as_str()), (key.anim >= 0, anim.as_str())]
                .iter().filter(|(on, _)| *on).map(|(_, name)| *name).collect::<Vec<_>>().join(", ");
            log.info(format!("  {:<32} {:<9} {count:>6} triangles  {flags}", self.material_name(*key), format!("{:?}", key.blend)));
        }
    }

    fn page(&self, texture: i16) -> Option<&'a Page>
    {
        usize::try_from(texture).ok().and_then(|i| self.pages.get(i)).and_then(|page| page.as_ref())
    }

    // market1f_double_env, untextured_alpha, ...
    pub fn material_name(&self, key: MaterialKey) -> String
    {
        let mut name = match self.page(key.texture)
        {
            Some(page) => page.name.clone(),
            None => "untextured".to_string(),
        };
        match key.blend
        {
            Blend::Opaque => {},
            Blend::Alpha => name += "_alpha",
            Blend::Additive => name += "_add",
        }
        if key.double_sided { name += "_double"; }
        if key.env { name += "_env"; }
        if key.mirror { name += "_mirror"; }
        if key.mirror_plane >= 0 { name += &key.mirror_plane.to_string(); }
        if key.anim >= 0 { name += &format!("_anim{}", key.anim); }
        name
    }

    fn material(&mut self, glb: &mut Glb, key: MaterialKey) -> usize
    {
        if let Some(&material) = self.materials.get(&key) { return material; }

        let page = self.page(key.texture);
        let mut material = json!({
            "name": self.material_name(key),
            "pbrMetallicRoughness": { "metallicFactor": 0.0, "roughnessFactor": 1.0 },
            "doubleSided": key.double_sided,
        });

        if let Some(page) = page
        {
            let index = key.texture as usize;
            let file = format!("{}.png", page.name);
            let texture = match key.blend
            {
                // no additive blending in the engine yet - brightness as alpha comes close
                Blend::Additive => *self.additive_textures.entry(index).or_insert_with(||
                {
                    let png = additive_png(&page.png).unwrap_or_else(|_| page.png.clone());
                    glb.png_texture(&png, &file.replace(".png", "_additive.png"))
                }),
                _ => *self.textures.entry(index).or_insert_with(|| glb.png_texture(&page.png, &file)),
            };
            material["pbrMetallicRoughness"]["baseColorTexture"] = json!({ "index": texture });
        }

        match key.blend
        {
            Blend::Alpha | Blend::Additive => material["alphaMode"] = json!("BLEND"),
            Blend::Opaque if page.is_some_and(|page| page.has_alpha) =>
            {
                material["alphaMode"] = json!("MASK");
                material["alphaCutoff"] = json!(0.5);
            },
            Blend::Opaque => {},
        }

        // hints for the engine - additive blending, env mapping, mirrors and texture animation are not in glTF
        let mut extras = serde_json::Map::new();
        if key.blend == Blend::Additive { extras.insert("revolt_blend".into(), json!("additive")); }
        if key.env { extras.insert("revolt_env".into(), json!(true)); }
        if key.mirror { extras.insert("revolt_mirror".into(), json!(true)); }
        if key.mirror_plane >= 0 { extras.insert("revolt_mirror_plane".into(), json!(key.mirror_plane)); }
        if let Some(frames) = usize::try_from(key.anim).ok().and_then(|i| self.anims.get(i))
        {
            extras.insert("revolt_texanim".into(), json!(key.anim));
            extras.insert("revolt_texanim_frames".into(), json!(frames.len()));
            extras.insert("revolt_texanim_duration".into(), json!(frames.iter().map(|f| f.delay as f64).sum::<f64>()));
            // the engine reads no arrays from extras - the frames as a json string
            let data: Vec<Value> = frames.iter().map(|f| json!({ "texture": f.texture, "delay": f.delay, "uvs": f.uvs })).collect();
            extras.insert("revolt_texanim_data".into(), json!(serde_json::to_string(&data).unwrap_or_default()));
        }
        if !extras.is_empty() { material["extras"] = Value::Object(extras); }

        if self.settings.unlit
        {
            material["extensions"] = json!({ "KHR_materials_unlit": {} });
            glb.use_extension("KHR_materials_unlit");
        }

        let index = glb.material(material);
        self.materials.insert(key, index);
        index
    }
}

#[derive(Default)]
pub struct Primitive
{
    positions: Vec<f32>,
    normals: Vec<f32>,
    uvs: Vec<f32>,
    colors: Vec<f32>,
    env_colors: Vec<f32>,
    // morph target: position deltas
    target_positions: Vec<f32>,
    indices: Vec<u32>,
}

impl Primitive
{
    // false if the polygon points to vertices that do not exist
    fn add(&mut self, mesh: &Mesh, polygon: &Polygon, uvs: &[[f32; 2]; 4], blend: Blend, env_color: Option<u32>, target: Option<&Mesh>, scale: f32) -> bool
    {
        let corners = polygon.corners();
        if polygon.indices[..corners].iter().any(|&i| i as usize >= mesh.vertices.len() || target.is_some_and(|t| i as usize >= t.vertices.len()))
        {
            return false;
        }

        let base = (self.positions.len() / 3) as u32;
        for c in 0..corners
        {
            let vertex = &mesh.vertices[polygon.indices[c] as usize];
            self.positions.extend(to_gltf(vertex.position).map(|v| v * scale));
            self.normals.extend(to_gltf(vertex.normal));
            self.uvs.extend(uvs[c]);
            if let Some(target) = target
            {
                let moved = &target.vertices[polygon.indices[c] as usize];
                let delta = [0, 1, 2].map(|i| moved.position[i] - vertex.position[i]);
                self.target_positions.extend(to_gltf(delta).map(|v| v * scale));
            }

            // translucent polygons carry their alpha in the vertex color
            let alpha = if blend == Blend::Alpha { None } else { Some(1.0) };
            self.colors.extend(linear_rgba(polygon.colors[c], alpha));
            if let Some(env_color) = env_color
            {
                self.env_colors.extend(linear_rgba(env_color, None));
            }
        }

        // clockwise in re-volt - flipped to counter clockwise
        self.indices.extend([base, base + 2, base + 1]);
        if corners == 4
        {
            self.indices.extend([base, base + 3, base + 2]);
        }

        true
    }

    fn write(&self, glb: &mut Glb, material: usize) -> Value
    {
        let mut attributes = json!({
            "POSITION": glb.floats(&self.positions, "VEC3", true),
            "NORMAL": glb.floats(&self.normals, "VEC3", false),
            "TEXCOORD_0": glb.floats(&self.uvs, "VEC2", false),
            "COLOR_0": glb.floats(&self.colors, "VEC4", false),
        });
        if !self.env_colors.is_empty()
        {
            attributes["COLOR_1"] = json!(glb.floats(&self.env_colors, "VEC4", false));
        }

        let mut primitive = json!({ "attributes": attributes, "indices": glb.indices(&self.indices), "material": material });
        if !self.target_positions.is_empty()
        {
            primitive["targets"] = json!([{ "POSITION": glb.floats(&self.target_positions, "VEC3", true) }]);
        }
        primitive
    }
}

// moves every corner half a texel toward the uv center of the polygon - the bilinear filter then stays inside its atlas tile
// an axis narrower than one texel stays as it is
fn inset_uvs(mut uvs: [[f32; 2]; 4], corners: usize, width: u32, height: u32) -> [[f32; 2]; 4]
{
    for (axis, size) in [(0, width as f32), (1, height as f32)]
    {
        let min = uvs[..corners].iter().map(|uv| uv[axis]).fold(f32::MAX, f32::min);
        let max = uvs[..corners].iter().map(|uv| uv[axis]).fold(f32::MIN, f32::max);
        if (max - min) * size <= 1.0 { continue; }

        let center = (min + max) / 2.0;
        let half_texel = 0.5 / size;
        for uv in &mut uvs[..corners]
        {
            if uv[axis] < center { uv[axis] += half_texel; } else if uv[axis] > center { uv[axis] -= half_texel; }
        }
    }
    uvs
}

// re-volt: x right, y down, z forward -> glTF: y up (a 180 degree turn around x)
pub fn to_gltf(v: [f32; 3]) -> [f32; 3]
{
    [v[0], -v[1], -v[2]]
}

// the colors are applied to sRGB textures in the original - linear for glTF
pub fn linear_rgba(color: u32, alpha: Option<f32>) -> [f32; 4]
{
    let [r, g, b, a] = color_rgba(color);
    let linear = |c: u8|
    {
        let c = c as f32 / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };

    [linear(r), linear(g), linear(b), alpha.unwrap_or(a as f32 / 255.0)]
}

// the .fin rows are the local axes in world space (columns of the rotation), turned into glTF space: S R S with S = diag(1, -1, -1)
// returns [x, y, z, w] like the scene format
pub fn rotation_quat(axes: &[[f32; 3]; 3]) -> [f32; 4]
{
    let s = [1.0, -1.0, -1.0];
    let m = |i: usize, j: usize| s[i] * s[j] * axes[j][i];

    let trace = m(0, 0) + m(1, 1) + m(2, 2);
    let [x, y, z, w] = if trace > 0.0
    {
        let k = 0.5 / (trace + 1.0).sqrt();
        [(m(2, 1) - m(1, 2)) * k, (m(0, 2) - m(2, 0)) * k, (m(1, 0) - m(0, 1)) * k, 0.25 / k]
    }
    else if m(0, 0) > m(1, 1) && m(0, 0) > m(2, 2)
    {
        let k = 2.0 * (1.0 + m(0, 0) - m(1, 1) - m(2, 2)).sqrt();
        [0.25 * k, (m(0, 1) + m(1, 0)) / k, (m(0, 2) + m(2, 0)) / k, (m(2, 1) - m(1, 2)) / k]
    }
    else if m(1, 1) > m(2, 2)
    {
        let k = 2.0 * (1.0 + m(1, 1) - m(0, 0) - m(2, 2)).sqrt();
        [(m(0, 1) + m(1, 0)) / k, 0.25 * k, (m(1, 2) + m(2, 1)) / k, (m(0, 2) - m(2, 0)) / k]
    }
    else
    {
        let k = 2.0 * (1.0 + m(2, 2) - m(0, 0) - m(1, 1)).sqrt();
        [(m(0, 2) + m(2, 0)) / k, (m(1, 2) + m(2, 1)) / k, 0.25 * k, (m(1, 0) - m(0, 1)) / k]
    };

    let length = (x * x + y * y + z * z + w * w).sqrt();
    [x / length, y / length, z / length, w / length]
}

// minimal glTF 2.0 binary writer: one buffer, embedded png textures

use std::path::Path;

use anyhow::Result;
use serde_json::{json, Value};

const ARRAY_BUFFER: u32 = 34962;
const ELEMENT_ARRAY_BUFFER: u32 = 34963;
const FLOAT: u32 = 5126;
const UNSIGNED_INT: u32 = 5125;

#[derive(Default)]
pub struct Glb
{
    bin: Vec<u8>,
    buffer_views: Vec<Value>,
    accessors: Vec<Value>,
    images: Vec<Value>,
    textures: Vec<Value>,
    materials: Vec<Value>,
    meshes: Vec<Value>,
    nodes: Vec<Value>,
    animations: Vec<Value>,
    extensions_used: Vec<&'static str>,
    // minFilter LINEAR: no mipmaps like the game (cars - small details on the page blur into their neighbours)
    pub no_mipmaps: bool,
}

impl Glb
{
    fn push_view(&mut self, bytes: &[u8], target: Option<u32>) -> usize
    {
        while self.bin.len() % 4 != 0 { self.bin.push(0); }

        let mut view = json!({ "buffer": 0, "byteOffset": self.bin.len(), "byteLength": bytes.len() });
        if let Some(target) = target { view["target"] = json!(target); }

        self.bin.extend_from_slice(bytes);
        self.buffer_views.push(view);
        self.buffer_views.len() - 1
    }

    // kind: "VEC2", "VEC3", "VEC4" - bounds are required for POSITION
    pub fn floats(&mut self, data: &[f32], kind: &str, bounds: bool) -> usize
    {
        let components = match kind { "VEC2" => 2, "VEC3" => 3, "VEC4" => 4, _ => 1 };
        let bytes: Vec<u8> = data.iter().flat_map(|value| value.to_le_bytes()).collect();
        let view = self.push_view(&bytes, Some(ARRAY_BUFFER));

        let mut accessor = json!({ "bufferView": view, "componentType": FLOAT, "count": data.len() / components, "type": kind });
        if bounds
        {
            let mut min = vec![f32::MAX; components];
            let mut max = vec![f32::MIN; components];
            for item in data.chunks(components)
            {
                for c in 0..components
                {
                    min[c] = min[c].min(item[c]);
                    max[c] = max[c].max(item[c]);
                }
            }
            accessor["min"] = json!(min);
            accessor["max"] = json!(max);
        }

        self.accessors.push(accessor);
        self.accessors.len() - 1
    }

    pub fn indices(&mut self, data: &[u32]) -> usize
    {
        let bytes: Vec<u8> = data.iter().flat_map(|value| value.to_le_bytes()).collect();
        let view = self.push_view(&bytes, Some(ELEMENT_ARRAY_BUFFER));

        self.accessors.push(json!({ "bufferView": view, "componentType": UNSIGNED_INT, "count": data.len(), "type": "SCALAR" }));
        self.accessors.len() - 1
    }

    // returns the texture index - repeating
    pub fn png_texture(&mut self, png: &[u8], name: &str) -> usize
    {
        self.texture_with_sampler(png, name, 0)
    }

    // clamped to the edge - no seams where two images meet (skybox)
    pub fn png_texture_clamped(&mut self, png: &[u8], name: &str) -> usize
    {
        self.texture_with_sampler(png, name, 1)
    }

    fn texture_with_sampler(&mut self, png: &[u8], name: &str, sampler: usize) -> usize
    {
        let view = self.push_view(png, None);
        self.images.push(json!({ "bufferView": view, "mimeType": "image/png", "name": name }));
        self.textures.push(json!({ "source": self.images.len() - 1, "sampler": sampler }));
        self.textures.len() - 1
    }

    pub fn material(&mut self, material: Value) -> usize
    {
        self.materials.push(material);
        self.materials.len() - 1
    }

    pub fn mesh(&mut self, name: &str, primitives: Vec<Value>) -> usize
    {
        self.meshes.push(json!({ "name": name, "primitives": primitives }));
        self.meshes.len() - 1
    }

    // keyframe data: no buffer target, min / max for the time input
    pub fn keyframes(&mut self, data: &[f32], kind: &str) -> usize
    {
        let components = match kind { "VEC3" => 3, "VEC4" => 4, _ => 1 };
        let bytes: Vec<u8> = data.iter().flat_map(|value| value.to_le_bytes()).collect();
        let view = self.push_view(&bytes, None);

        let mut accessor = json!({ "bufferView": view, "componentType": FLOAT, "count": data.len() / components, "type": kind });
        if kind == "SCALAR"
        {
            accessor["min"] = json!([data.iter().copied().fold(f32::MAX, f32::min)]);
            accessor["max"] = json!([data.iter().copied().fold(f32::MIN, f32::max)]);
        }
        self.accessors.push(accessor);
        self.accessors.len() - 1
    }

    // channels: (node, path "translation" / "rotation" / "weights", times, values) - one sampler each, linear
    pub fn animation(&mut self, name: &str, channels: Vec<(usize, &str, usize, usize)>)
    {
        let samplers: Vec<Value> = channels.iter().map(|(_, _, input, output)| json!({ "input": input, "output": output, "interpolation": "LINEAR" })).collect();
        let channels: Vec<Value> = channels.iter().enumerate().map(|(i, (node, path, _, _))| json!({ "sampler": i, "target": { "node": node, "path": path } })).collect();
        self.animations.push(json!({ "name": name, "samplers": samplers, "channels": channels }));
    }

    // default morph weights of a mesh
    pub fn mesh_weights(&mut self, mesh: usize, weights: &[f32])
    {
        self.meshes[mesh]["weights"] = json!(weights);
    }

    pub fn node(&mut self, node: Value) -> usize
    {
        self.nodes.push(node);
        self.nodes.len() - 1
    }

    pub fn use_extension(&mut self, name: &'static str)
    {
        if !self.extensions_used.contains(&name) { self.extensions_used.push(name); }
    }

    pub fn save(&self, path: &Path, roots: &[usize]) -> Result<()>
    {
        let mut root = json!({
            "asset": { "version": "2.0", "generator": "rustl re-volt converter" },
            "scene": 0,
            "scenes": [{ "nodes": roots }],
            "buffers": [{ "byteLength": self.bin.len() }],
        });

        // glTF forbids empty arrays
        let arrays = [
            ("bufferViews", &self.buffer_views), ("accessors", &self.accessors), ("images", &self.images), ("textures", &self.textures),
            ("materials", &self.materials), ("meshes", &self.meshes), ("nodes", &self.nodes), ("animations", &self.animations),
        ];
        for (key, array) in arrays
        {
            if !array.is_empty() { root[key] = json!(array); }
        }
        if !self.textures.is_empty()
        {
            // linear, linear mipmap linear (or linear) - 0 repeat, 1 clamp to edge
            let min_filter = if self.no_mipmaps { 9729 } else { 9987 };
            root["samplers"] = json!([
                { "magFilter": 9729, "minFilter": min_filter, "wrapS": 10497, "wrapT": 10497 },
                { "magFilter": 9729, "minFilter": min_filter, "wrapS": 33071, "wrapT": 33071 },
            ]);
        }
        if !self.extensions_used.is_empty()
        {
            root["extensionsUsed"] = json!(self.extensions_used);
        }

        let mut json_chunk = serde_json::to_vec(&root)?;
        while json_chunk.len() % 4 != 0 { json_chunk.push(b' '); }
        let mut bin_chunk = self.bin.clone();
        while bin_chunk.len() % 4 != 0 { bin_chunk.push(0); }

        let total = 12 + 8 + json_chunk.len() + 8 + bin_chunk.len();
        let mut out = Vec::with_capacity(total);
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&(json_chunk.len() as u32).to_le_bytes());
        out.extend_from_slice(b"JSON");
        out.extend_from_slice(&json_chunk);
        out.extend_from_slice(&(bin_chunk.len() as u32).to_le_bytes());
        out.extend_from_slice(b"BIN\0");
        out.extend_from_slice(&bin_chunk);

        if let Some(dir) = path.parent() { std::fs::create_dir_all(dir)?; }
        std::fs::write(path, out)?;
        Ok(())
    }
}

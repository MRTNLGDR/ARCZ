//! Escritor GLB 2.0 mínimo e real, sem conversor externo.
//!
//! O arquivo produzido contém posições, normais, UVs, índices, materiais PBR
//! e `EXT_mesh_gpu_instancing` quando a cena possui lotes instanciados. Todos
//! os offsets são alinhados a quatro bytes conforme a especificação glTF.

use anyhow::{bail, Context, Result};
use arcz_procedural::input::AlphaMode;
use arcz_procedural::mesh::{InstanceBatch, Material, Primitive, SceneOutput};
use serde_json::{json, Map, Value};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

const ARRAY_BUFFER: u32 = 34_962;
const ELEMENT_ARRAY_BUFFER: u32 = 34_963;
const COMPONENT_F32: u32 = 5_126;
const COMPONENT_U32: u32 = 5_125;
const JSON_CHUNK: u32 = 0x4E4F_534A;
const BIN_CHUNK: u32 = 0x004E_4942;

#[derive(Default)]
struct BufferBuilder {
    bytes: Vec<u8>,
    views: Vec<Value>,
    accessors: Vec<Value>,
}

impl BufferBuilder {
    fn align4(&mut self) {
        while self.bytes.len() % 4 != 0 { self.bytes.push(0); }
    }

    fn push_view(&mut self, raw: &[u8], target: Option<u32>) -> usize {
        self.align4();
        let offset = self.bytes.len();
        self.bytes.extend_from_slice(raw);
        let mut view = Map::new();
        view.insert("buffer".to_owned(), json!(0));
        view.insert("byteOffset".to_owned(), json!(offset));
        view.insert("byteLength".to_owned(), json!(raw.len()));
        if let Some(target) = target { view.insert("target".to_owned(), json!(target)); }
        self.views.push(Value::Object(view));
        self.views.len() - 1
    }

    fn push_vec2(&mut self, values: &[[f32; 2]], target: Option<u32>) -> usize {
        let mut raw = Vec::with_capacity(values.len() * 8);
        for value in values { for component in value { raw.extend_from_slice(&component.to_le_bytes()); } }
        let view = self.push_view(&raw, target);
        self.accessors.push(json!({
            "bufferView": view, "componentType": COMPONENT_F32,
            "count": values.len(), "type": "VEC2"
        }));
        self.accessors.len() - 1
    }

    fn push_vec3(&mut self, values: &[[f32; 3]], target: Option<u32>, min_max: bool) -> usize {
        let mut raw = Vec::with_capacity(values.len() * 12);
        let mut minimum = [f32::INFINITY; 3];
        let mut maximum = [f32::NEG_INFINITY; 3];
        for value in values {
            for axis in 0..3 {
                raw.extend_from_slice(&value[axis].to_le_bytes());
                minimum[axis] = minimum[axis].min(value[axis]);
                maximum[axis] = maximum[axis].max(value[axis]);
            }
        }
        let view = self.push_view(&raw, target);
        let mut accessor = Map::new();
        accessor.insert("bufferView".to_owned(), json!(view));
        accessor.insert("componentType".to_owned(), json!(COMPONENT_F32));
        accessor.insert("count".to_owned(), json!(values.len()));
        accessor.insert("type".to_owned(), json!("VEC3"));
        if min_max {
            accessor.insert("min".to_owned(), json!(minimum));
            accessor.insert("max".to_owned(), json!(maximum));
        }
        self.accessors.push(Value::Object(accessor));
        self.accessors.len() - 1
    }

    fn push_vec4(&mut self, values: &[[f32; 4]], target: Option<u32>) -> usize {
        let mut raw = Vec::with_capacity(values.len() * 16);
        for value in values { for component in value { raw.extend_from_slice(&component.to_le_bytes()); } }
        let view = self.push_view(&raw, target);
        self.accessors.push(json!({
            "bufferView": view, "componentType": COMPONENT_F32,
            "count": values.len(), "type": "VEC4"
        }));
        self.accessors.len() - 1
    }

    fn push_indices(&mut self, values: &[u32]) -> usize {
        let mut raw = Vec::with_capacity(values.len() * 4);
        let mut maximum = 0_u32;
        for value in values { raw.extend_from_slice(&value.to_le_bytes()); maximum = maximum.max(*value); }
        let view = self.push_view(&raw, Some(ELEMENT_ARRAY_BUFFER));
        self.accessors.push(json!({
            "bufferView": view, "componentType": COMPONENT_U32,
            "count": values.len(), "type": "SCALAR", "min": [0], "max": [maximum]
        }));
        self.accessors.len() - 1
    }
}

pub fn write_glb(path: &Path, scene: &SceneOutput) -> Result<()> {
    if scene.primitives.is_empty() && scene.instance_batches.is_empty() {
        bail!("cena sem geometria");
    }
    let mut buffer = BufferBuilder::default();
    let material_index: std::collections::BTreeMap<_, _> = scene.materials.iter().enumerate()
        .map(|(index, material)| (material.id.as_str(), index)).collect();
    let materials: Vec<Value> = scene.materials.iter().map(material_json).collect();
    let mut meshes = Vec::<Value>::new();
    let mut nodes = Vec::<Value>::new();
    let mut scene_nodes = Vec::<usize>::new();

    for primitive in &scene.primitives {
        let gltf_primitive = primitive_json(primitive, &material_index, &mut buffer)?;
        let mesh_index = meshes.len();
        meshes.push(json!({"name": primitive.name.clone(), "primitives": [gltf_primitive], "extras": primitive.extras.clone()}));
        let node_index = nodes.len();
        nodes.push(json!({"name": primitive.name.clone(), "mesh": mesh_index}));
        scene_nodes.push(node_index);
    }

    for batch in &scene.instance_batches {
        append_instance_batch(batch, &material_index, &mut buffer, &mut meshes, &mut nodes, &mut scene_nodes)?;
    }

    buffer.align4();
    let mut root = Map::new();
    root.insert("asset".to_owned(), json!({
        "version":"2.0", "generator":"ARCZ arcz-generation-cli",
        "extras":{"coordinateSystem":"ENU_LOCAL","provenance":scene.provenance.clone(),"warnings":scene.warnings.clone()}
    }));
    root.insert("scene".to_owned(), json!(0));
    root.insert("scenes".to_owned(), json!([{"name":"ARCZ generated scene","nodes":scene_nodes}]));
    root.insert("nodes".to_owned(), Value::Array(nodes));
    root.insert("meshes".to_owned(), Value::Array(meshes));
    root.insert("materials".to_owned(), Value::Array(materials));
    root.insert("buffers".to_owned(), json!([{"byteLength":buffer.bytes.len()}]));
    root.insert("bufferViews".to_owned(), Value::Array(buffer.views));
    root.insert("accessors".to_owned(), Value::Array(buffer.accessors));
    if !scene.instance_batches.is_empty() {
        root.insert("extensionsUsed".to_owned(), json!(["EXT_mesh_gpu_instancing"]));
    }
    let mut json_bytes = serde_json::to_vec(&Value::Object(root)).context("serializar glTF")?;
    while json_bytes.len() % 4 != 0 { json_bytes.push(b' '); }
    while buffer.bytes.len() % 4 != 0 { buffer.bytes.push(0); }

    let total_length = 12_usize
        .checked_add(8 + json_bytes.len()).and_then(|value| value.checked_add(8 + buffer.bytes.len()))
        .context("GLB grande demais")?;
    let total_length = u32::try_from(total_length).context("GLB excede limite de 4 GiB")?;
    let parent = path.parent().context("destino GLB sem diretório")?;
    std::fs::create_dir_all(parent).context("criar diretório GLB")?;
    let file = File::create(path).with_context(|| format!("criar {}", path.display()))?;
    let mut writer = BufWriter::new(file);
    writer.write_all(b"glTF")?;
    writer.write_all(&2_u32.to_le_bytes())?;
    writer.write_all(&total_length.to_le_bytes())?;
    writer.write_all(&(json_bytes.len() as u32).to_le_bytes())?;
    writer.write_all(&JSON_CHUNK.to_le_bytes())?;
    writer.write_all(&json_bytes)?;
    writer.write_all(&(buffer.bytes.len() as u32).to_le_bytes())?;
    writer.write_all(&BIN_CHUNK.to_le_bytes())?;
    writer.write_all(&buffer.bytes)?;
    writer.flush()?;
    writer.get_ref().sync_all()?;
    Ok(())
}

fn primitive_json(primitive: &Primitive, materials: &std::collections::BTreeMap<&str, usize>,
                  buffer: &mut BufferBuilder) -> Result<Value> {
    if primitive.positions.is_empty() || primitive.indices.is_empty() { bail!("primitive {} vazia", primitive.name); }
    let position = buffer.push_vec3(&primitive.positions, Some(ARRAY_BUFFER), true);
    let normal = buffer.push_vec3(&primitive.normals, Some(ARRAY_BUFFER), false);
    let uv = buffer.push_vec2(&primitive.uvs, Some(ARRAY_BUFFER));
    let indices = buffer.push_indices(&primitive.indices);
    let material = materials.get(primitive.material_id.as_str())
        .with_context(|| format!("material desconhecido: {}", primitive.material_id))?;
    Ok(json!({
        "attributes":{"POSITION":position,"NORMAL":normal,"TEXCOORD_0":uv},
        "indices":indices,"material":material,"mode":4
    }))
}

fn append_instance_batch(batch: &InstanceBatch, materials: &std::collections::BTreeMap<&str, usize>,
                         buffer: &mut BufferBuilder, meshes: &mut Vec<Value>, nodes: &mut Vec<Value>,
                         scene_nodes: &mut Vec<usize>) -> Result<()> {
    if batch.transforms.is_empty() { bail!("batch {} sem transforms", batch.name); }
    let primitives: Vec<Value> = batch.primitives.iter()
        .map(|primitive| primitive_json(primitive, materials, buffer)).collect::<Result<_>>()?;
    let mesh_index = meshes.len();
    meshes.push(json!({"name":batch.name.clone(),"primitives":primitives,"extras":batch.extras.clone()}));
    let translations: Vec<[f32; 3]> = batch.transforms.iter().map(|t| t.translation).collect();
    let rotations: Vec<[f32; 4]> = batch.transforms.iter().map(|t| t.rotation).collect();
    let scales: Vec<[f32; 3]> = batch.transforms.iter().map(|t| t.scale).collect();
    let translation = buffer.push_vec3(&translations, Some(ARRAY_BUFFER), false);
    let rotation = buffer.push_vec4(&rotations, Some(ARRAY_BUFFER));
    let scale = buffer.push_vec3(&scales, Some(ARRAY_BUFFER), false);
    let node_index = nodes.len();
    nodes.push(json!({
        "name":batch.name.clone(),"mesh":mesh_index,
        "extensions":{"EXT_mesh_gpu_instancing":{"attributes":{
            "TRANSLATION":translation,"ROTATION":rotation,"SCALE":scale
        }}}
    }));
    scene_nodes.push(node_index);
    Ok(())
}

fn material_json(material: &Material) -> Value {
    let alpha = match material.alpha_mode {
        AlphaMode::Opaque => "OPAQUE",
        AlphaMode::Mask => "MASK",
        AlphaMode::Blend => "BLEND",
    };
    let mut value = json!({
        "name":material.id.clone(),
        "pbrMetallicRoughness":{
            "baseColorFactor":material.base_color,
            "metallicFactor":material.metallic,
            "roughnessFactor":material.roughness
        },
        "doubleSided":material.double_sided,
        "alphaMode":alpha
    });
    if material.alpha_mode == AlphaMode::Mask { value["alphaCutoff"] = json!(0.5); }
    value
}

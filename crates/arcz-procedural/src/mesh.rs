use arcz_budget::Resources;
use arcz_validation::{validate_mesh, MeshView};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

use crate::input::{AlphaMode, MaterialInput};
use crate::ProceduralError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Material {
    pub id: String,
    pub base_color: [f32; 4],
    pub roughness: f32,
    pub metallic: f32,
    pub double_sided: bool,
    pub alpha_mode: AlphaMode,
}

impl From<MaterialInput> for Material {
    fn from(value: MaterialInput) -> Self {
        Self {
            id: value.id,
            base_color: value.base_color,
            roughness: value.roughness,
            metallic: value.metallic,
            double_sided: value.double_sided,
            alpha_mode: value.alpha_mode,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Primitive {
    pub name: String,
    pub material_id: String,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    #[serde(default)]
    pub extras: Value,
}

impl Primitive {
    pub fn triangle_count(&self) -> usize { self.indices.len() / 3 }

    pub fn validate(&self) -> Result<Vec<String>, ProceduralError> {
        let report = validate_mesh(MeshView {
            positions: &self.positions,
            normals: &self.normals,
            uvs: &self.uvs,
            indices: &self.indices,
        }).map_err(|error| ProceduralError::InvalidMesh {
            name: self.name.clone(), reason: error.to_string(),
        })?;
        Ok(report.warnings)
    }

    pub fn append(&mut self, other: &Primitive) -> Result<(), ProceduralError> {
        if self.material_id != other.material_id {
            return Err(ProceduralError::MaterialMismatch {
                expected: self.material_id.clone(), actual: other.material_id.clone(),
            });
        }
        let base = u32::try_from(self.positions.len())
            .map_err(|_| ProceduralError::GeometryTooLarge)?;
        self.positions.extend_from_slice(&other.positions);
        self.normals.extend_from_slice(&other.normals);
        self.uvs.extend_from_slice(&other.uvs);
        for index in &other.indices {
            self.indices.push(base.checked_add(*index).ok_or(ProceduralError::GeometryTooLarge)?);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceTransform {
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceBatch {
    pub name: String,
    pub primitives: Vec<Primitive>,
    pub transforms: Vec<InstanceTransform>,
    #[serde(default)]
    pub extras: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SceneOutput {
    pub materials: Vec<Material>,
    pub primitives: Vec<Primitive>,
    pub instance_batches: Vec<InstanceBatch>,
    pub warnings: Vec<String>,
    pub provenance: Vec<Value>,
}

impl SceneOutput {
    pub fn validate(&mut self) -> Result<(), ProceduralError> {
        let material_ids: std::collections::BTreeSet<_> =
            self.materials.iter().map(|material| material.id.as_str()).collect();
        if material_ids.len() != self.materials.len() {
            return Err(ProceduralError::DuplicateMaterial);
        }
        for primitive in &self.primitives {
            if !material_ids.contains(primitive.material_id.as_str()) {
                return Err(ProceduralError::UnknownMaterial(primitive.material_id.clone()));
            }
            self.warnings.extend(primitive.validate()?);
        }
        for batch in &self.instance_batches {
            if batch.transforms.is_empty() {
                return Err(ProceduralError::EmptyInstanceBatch(batch.name.clone()));
            }
            for transform in &batch.transforms {
                if transform.translation.iter().chain(transform.rotation.iter()).chain(transform.scale.iter())
                    .any(|value| !value.is_finite()) {
                    return Err(ProceduralError::NonFinite);
                }
            }
            for primitive in &batch.primitives {
                if !material_ids.contains(primitive.material_id.as_str()) {
                    return Err(ProceduralError::UnknownMaterial(primitive.material_id.clone()));
                }
                self.warnings.extend(primitive.validate()?);
            }
        }
        Ok(())
    }

    pub fn metrics(&self) -> SceneMetrics {
        let normal_vertices: usize = self.primitives.iter().map(|p| p.positions.len()).sum();
        let normal_triangles: usize = self.primitives.iter().map(Primitive::triangle_count).sum();
        let base_vertices: usize = self.instance_batches.iter()
            .flat_map(|batch| batch.primitives.iter()).map(|p| p.positions.len()).sum();
        let base_triangles: usize = self.instance_batches.iter()
            .flat_map(|batch| batch.primitives.iter()).map(Primitive::triangle_count).sum();
        let instances: usize = self.instance_batches.iter().map(|batch| batch.transforms.len()).sum();
        let instanced_triangles: usize = self.instance_batches.iter().map(|batch| {
            let base: usize = batch.primitives.iter().map(Primitive::triangle_count).sum();
            base.saturating_mul(batch.transforms.len())
        }).sum();
        let bytes = self.primitives.iter()
            .chain(self.instance_batches.iter().flat_map(|batch| batch.primitives.iter()))
            .map(|p| p.positions.len() * 12 + p.normals.len() * 12 + p.uvs.len() * 8 + p.indices.len() * 4)
            .sum::<usize>()
            + self.instance_batches.iter().map(|batch| batch.transforms.len() * (12 + 16 + 12)).sum::<usize>();
        SceneMetrics {
            primitives: self.primitives.len(),
            instance_batches: self.instance_batches.len(),
            instances,
            vertices: normal_vertices + base_vertices,
            triangles: normal_triangles + instanced_triangles,
            base_triangles: normal_triangles + base_triangles,
            materials: self.materials.len(),
            geometry_bytes: bytes,
        }
    }

    pub fn estimated_resources(&self) -> Resources {
        let metrics = self.metrics();
        Resources {
            triangles: metrics.triangles as u64,
            instances: metrics.instances as u64,
            draw_calls: (self.primitives.len()
                + self.instance_batches.iter().map(|b| b.primitives.len()).sum::<usize>()) as u64,
            geometry_mb: metrics.geometry_bytes as f64 / (1024.0 * 1024.0),
            materials: metrics.materials as u64,
            ..Resources::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneMetrics {
    pub primitives: usize,
    pub instance_batches: usize,
    pub instances: usize,
    pub vertices: usize,
    pub triangles: usize,
    pub base_triangles: usize,
    pub materials: usize,
    pub geometry_bytes: usize,
}

#[derive(Debug, Default)]
pub struct MeshGroups {
    values: BTreeMap<(String, String), Primitive>,
}

impl MeshGroups {
    pub fn get_mut(&mut self, name: impl Into<String>, material_id: impl Into<String>) -> &mut Primitive {
        let name = name.into();
        let material_id = material_id.into();
        self.values.entry((name.clone(), material_id.clone())).or_insert_with(|| Primitive {
            name, material_id, ..Primitive::default()
        })
    }

    pub fn into_primitives(self) -> Vec<Primitive> {
        self.values.into_values().filter(|p| !p.indices.is_empty()).collect()
    }
}

pub fn add_triangle(primitive: &mut Primitive, a: [f32; 3], b: [f32; 3], c: [f32; 3],
                    uv: [[f32; 2]; 3]) -> Result<(), ProceduralError> {
    let normal = triangle_normal(a, b, c)?;
    let base = u32::try_from(primitive.positions.len()).map_err(|_| ProceduralError::GeometryTooLarge)?;
    primitive.positions.extend([a, b, c]);
    primitive.normals.extend([normal; 3]);
    primitive.uvs.extend(uv);
    primitive.indices.extend([base, base + 1, base + 2]);
    Ok(())
}

pub fn add_quad(primitive: &mut Primitive, a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3],
                uv_scale: [f32; 2]) -> Result<(), ProceduralError> {
    add_triangle(primitive, a, b, c, [[0.0, 0.0], [uv_scale[0], 0.0], uv_scale])?;
    add_triangle(primitive, a, c, d, [[0.0, 0.0], uv_scale, [0.0, uv_scale[1]]])
}

pub fn triangle_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> Result<[f32; 3], ProceduralError> {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    let length = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if !length.is_finite() || length < 1.0e-10 { return Err(ProceduralError::DegenerateTriangle); }
    Ok([n[0] / length, n[1] / length, n[2] / length])
}

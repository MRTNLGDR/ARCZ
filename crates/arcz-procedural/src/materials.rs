use std::collections::BTreeMap;

use crate::input::{AlphaMode, MaterialInput};
use crate::mesh::Material;
use crate::ProceduralError;

pub fn resolve(inputs: &[MaterialInput]) -> Result<Vec<Material>, ProceduralError> {
    let mut materials: BTreeMap<String, Material> = defaults()
        .into_iter()
        .map(|material| (material.id.clone(), material))
        .collect();
    for input in inputs.iter().cloned() {
        if input.id.trim().is_empty()
            || input.base_color.iter().any(|v| !v.is_finite())
            || !input.roughness.is_finite()
            || !(0.0..=1.0).contains(&input.roughness)
            || !input.metallic.is_finite()
            || !(0.0..=1.0).contains(&input.metallic)
        {
            return Err(ProceduralError::InvalidMaterial(input.id));
        }
        materials.insert(input.id.clone(), input.into());
    }
    Ok(materials.into_values().collect())
}

pub fn defaults() -> Vec<Material> {
    vec![
        material("terrain.grass", [0.31, 0.39, 0.24, 1.0], 0.95, 0.0),
        material("terrain.soil", [0.34, 0.24, 0.16, 1.0], 1.0, 0.0),
        material("parcel.surface", [0.50, 0.52, 0.45, 1.0], 0.95, 0.0),
        material("road.asphalt", [0.095, 0.105, 0.115, 1.0], 0.92, 0.0),
        material("sidewalk.concrete", [0.48, 0.47, 0.44, 1.0], 0.88, 0.0),
        material("facade.offwhite", [0.78, 0.76, 0.69, 1.0], 0.82, 0.0),
        material("facade.gray", [0.35, 0.36, 0.36, 1.0], 0.78, 0.0),
        material("roof.ceramic", [0.42, 0.14, 0.085, 1.0], 0.90, 0.0),
        material("roof.metal", [0.22, 0.24, 0.25, 1.0], 0.55, 0.15),
        material("balcony.concrete", [0.62, 0.61, 0.58, 1.0], 0.85, 0.0),
        Material {
            id: "glass.window".to_owned(),
            base_color: [0.12, 0.23, 0.28, 0.72],
            roughness: 0.18,
            metallic: 0.0,
            double_sided: true,
            alpha_mode: AlphaMode::Blend,
        },
        material("vegetation.trunk", [0.25, 0.14, 0.07, 1.0], 0.92, 0.0),
        Material {
            id: "vegetation.leaf".to_owned(),
            base_color: [0.12, 0.30, 0.10, 1.0],
            roughness: 0.94,
            metallic: 0.0,
            double_sided: true,
            alpha_mode: AlphaMode::Opaque,
        },
    ]
}

fn material(id: &str, base_color: [f32; 4], roughness: f32, metallic: f32) -> Material {
    Material {
        id: id.to_owned(),
        base_color,
        roughness,
        metallic,
        double_sided: false,
        alpha_mode: AlphaMode::Opaque,
    }
}

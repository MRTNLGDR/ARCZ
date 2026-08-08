use arcz_vegetation::{scatter, ScatterRequest};
use serde_json::json;

use crate::input::{Quality, VegetationZone};
use crate::mesh::{add_quad, add_triangle, InstanceBatch, InstanceTransform, Primitive};
use crate::ProceduralError;

pub fn generate(
    zones: &[VegetationZone],
    quality: Quality,
    seed: u64,
    density_multiplier: f64,
) -> Result<(Vec<InstanceBatch>, Vec<String>, Vec<serde_json::Value>), ProceduralError> {
    if zones.is_empty() {
        return Err(ProceduralError::InputMissing("vegetation_zones"));
    }
    if !density_multiplier.is_finite() || !(0.0..=4.0).contains(&density_multiplier) {
        return Err(ProceduralError::Vegetation {
            id: "density_multiplier".to_owned(),
            reason: "fora de 0..4".to_owned(),
        });
    }
    let base = tree_base_mesh(quality)?;
    let mut transforms = Vec::new();
    let mut warnings = Vec::new();
    let mut provenance = Vec::new();
    for zone in zones {
        let result = scatter(&ScatterRequest {
            polygon: zone.polygon_enu_m.clone(),
            seed: arcz_determinism::Seed(seed)
                .derive("vegetation_zone", &zone.id)
                .0,
            target_count: ((zone.target_count as f64) * density_multiplier)
                .round()
                .clamp(0.0, 1_000_000.0) as usize,
            minimum_distance_m: zone.minimum_distance_m,
            exclusions: zone.exclusions.clone(),
            attempts_per_instance: 50,
            variants: zone.variants.clone(),
        })
        .map_err(|error| ProceduralError::Vegetation {
            id: zone.id.clone(),
            reason: error.to_string(),
        })?;
        if result.len() < zone.target_count {
            warnings.push(format!(
                "zona {}: {} de {} instâncias couberam respeitando exclusões",
                zone.id,
                result.len(),
                zone.target_count
            ));
        }
        for instance in result {
            let half = instance.rotation_rad * 0.5;
            transforms.push(InstanceTransform {
                translation: [
                    instance.position[0] as f32,
                    zone.base_m as f32,
                    -instance.position[1] as f32,
                ],
                rotation: [0.0, half.sin() as f32, 0.0, half.cos() as f32],
                scale: [instance.scale as f32; 3],
            });
        }
        provenance.push(
            json!({"zone":zone.id,"source":zone.source.source,"source_ref":zone.source.source_ref,
            "confidence":zone.source.confidence,"estimated":zone.source.estimated}),
        );
    }
    if transforms.is_empty() {
        return Err(ProceduralError::NoGeometry("vegetation"));
    }
    Ok((
        vec![InstanceBatch {
            name: "vegetation".to_owned(),
            primitives: base,
            transforms,
            extras: json!({"extension":"EXT_mesh_gpu_instancing","coordinate_system":"ENU_LOCAL"}),
        }],
        warnings,
        provenance,
    ))
}

fn tree_base_mesh(quality: Quality) -> Result<Vec<Primitive>, ProceduralError> {
    let sides = match quality {
        Quality::Leve => 5,
        Quality::Equilibrado => 6,
        Quality::Alto => 8,
        Quality::Cinematico => 10,
    };
    let mut trunk = Primitive {
        name: "tree-trunk".to_owned(),
        material_id: "vegetation.trunk".to_owned(),
        ..Primitive::default()
    };
    let radius = 0.18_f32;
    let height = 2.4_f32;
    for side in 0..sides {
        let a0 = std::f32::consts::TAU * side as f32 / sides as f32;
        let a1 = std::f32::consts::TAU * (side + 1) as f32 / sides as f32;
        let p0 = [a0.cos() * radius, 0.0, a0.sin() * radius];
        let p1 = [a1.cos() * radius, 0.0, a1.sin() * radius];
        let p2 = [p1[0], height, p1[2]];
        let p3 = [p0[0], height, p0[2]];
        add_quad(&mut trunk, p1, p0, p3, p2, [1.0, 1.0])?;
    }
    let mut canopy = Primitive {
        name: "tree-canopy".to_owned(),
        material_id: "vegetation.leaf".to_owned(),
        ..Primitive::default()
    };
    let center = [0.0, height + 1.35, 0.0];
    let top = [0.0, height + 3.0, 0.0];
    let bottom = [0.0, height + 0.15, 0.0];
    let ring = [
        [1.35, center[1], 0.0],
        [0.0, center[1], 1.35],
        [-1.35, center[1], 0.0],
        [0.0, center[1], -1.35],
    ];
    for i in 0..ring.len() {
        let a = ring[i];
        let b = ring[(i + 1) % ring.len()];
        add_triangle(&mut canopy, a, b, top, [[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]])?;
        add_triangle(
            &mut canopy,
            b,
            a,
            bottom,
            [[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]],
        )?;
    }
    Ok(vec![trunk, canopy])
}

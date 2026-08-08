use arcz_determinism::Seed;
use arcz_facade::{layout, FacadeRequest, ModuleKind};
use arcz_roof::{generate as generate_roof, RoofRequest};
use arcz_validation::Point2;
use serde_json::json;

use crate::geometry::{axis_aligned_infill, ensure_ccw, render_point};
use crate::input::{BuildingCategory, BuildingInput, EstimatedInfill, ParcelInput, Quality, RoofSpec, SourceEvidence};
use crate::mesh::{add_quad, MeshGroups, Primitive};
use crate::ProceduralError;

pub fn generate(explicit: &[BuildingInput], parcels: &[ParcelInput], category: BuildingCategory,
                allow_estimated_infill: bool, infill: &EstimatedInfill, quality: Quality, seed: u64)
    -> Result<(MeshGroups, Vec<String>, Vec<serde_json::Value>), ProceduralError> {
    let mut buildings: Vec<BuildingInput> = explicit.iter().filter(|item| category_matches(item.category, category)).cloned().collect();
    let mut warnings = Vec::new();
    let mut provenance = Vec::new();
    if buildings.is_empty() && allow_estimated_infill {
        for (index, parcel) in parcels.iter().enumerate() {
            if let Some(footprint) = axis_aligned_infill(&parcel.polygon_enu_m, infill.front_setback_m,
                                                        infill.side_setback_m, infill.maximum_coverage) {
                let mut rng = Seed(seed).derive("estimated_infill", &parcel.id).rng();
                let height = rng.range_f64(infill.house_height_m[0], infill.house_height_m[1]);
                let floors = if height > 4.5 { 2 } else { 1 };
                let building = BuildingInput {
                    id: format!("estimated:{}:{index}", parcel.id), footprint_enu_m: footprint,
                    base_m: parcel.elevation_m, height_m: height, floors,
                    category: BuildingCategory::House,
                    roof: RoofSpec::default(), wall_material_id: "facade.offwhite".to_owned(),
                    roof_material_id: "roof.ceramic".to_owned(), glass_material_id: "glass.window".to_owned(),
                    balcony_material_id: "balcony.concrete".to_owned(), commercial_ground_floor: false,
                    facade_module_width_m: 2.8, balcony_probability: 0.12,
                    source: SourceEvidence { source: "procedural_infill".to_owned(), source_ref: parcel.id.clone(),
                        confidence: 0.35, estimated: true },
                };
                provenance.push(json!({"entity":building.id,"estimated":true,"source_parcel":parcel.id,"confidence":0.35}));
                buildings.push(building);
            } else {
                warnings.push(format!("lote {} não comportou implantação estimada segura", parcel.id));
            }
        }
    }
    if buildings.is_empty() {
        return Err(ProceduralError::InputMissing(match category {
            BuildingCategory::House => "buildings(category=house) ou allow_estimated_infill",
            _ => "buildings(category=building/commercial/industrial)",
        }));
    }
    let mut groups = MeshGroups::default();
    for building in &buildings {
        generate_one(&mut groups, building, quality, Seed(seed).derive("building", &building.id), &mut warnings)?;
        provenance.push(json!({"entity":building.id,"source":building.source.source,"source_ref":building.source.source_ref,
            "confidence":building.source.confidence,"estimated":building.source.estimated}));
    }
    Ok((groups, warnings, provenance))
}

fn category_matches(value: BuildingCategory, requested: BuildingCategory) -> bool {
    match requested {
        BuildingCategory::House => value == BuildingCategory::House,
        BuildingCategory::Building => value != BuildingCategory::House,
        _ => value == requested,
    }
}

fn generate_one(groups: &mut MeshGroups, building: &BuildingInput, quality: Quality, seed: Seed,
                warnings: &mut Vec<String>) -> Result<(), ProceduralError> {
    let footprint = ensure_ccw(&building.footprint_enu_m)?;
    if !building.base_m.is_finite() || !building.height_m.is_finite() || building.height_m < 2.2
        || building.floors == 0 || building.floors > 200 || !(0.0..=1.0).contains(&building.balcony_probability) {
        return Err(ProceduralError::InvalidBuilding(building.id.clone()));
    }
    append_walls(groups.get_mut("building-walls", building.wall_material_id.clone()),
                 &footprint, building.base_m, building.base_m + building.height_m)?;
    let roof = generate_roof(&RoofRequest {
        footprint: footprint.clone(), wall_top_m: building.base_m + building.height_m,
        kind: building.roof.kind, pitch_deg: building.roof.pitch_deg, eave_m: building.roof.eave_m,
    }).map_err(|error| ProceduralError::Roof { id: building.id.clone(), reason: error.to_string() })?;
    warnings.extend(roof.warnings.into_iter().map(|warning| format!("{}: {warning}", building.id)));
    append_roof(groups.get_mut("building-roofs", building.roof_material_id.clone()), roof.mesh)?;
    if quality.facade_detail() {
        append_facades(groups, &footprint, building, quality, seed)?;
    }
    Ok(())
}

fn append_walls(primitive: &mut Primitive, footprint: &[Point2], base: f64, top: f64)
    -> Result<(), ProceduralError> {
    for i in 0..footprint.len() {
        let a = footprint[i];
        let b = footprint[(i + 1) % footprint.len()];
        let length = (b[0] - a[0]).hypot(b[1] - a[1]);
        add_quad(primitive, render_point(a, base)?, render_point(b, base)?,
                 render_point(b, top)?, render_point(a, top)?,
                 [(length / 3.0) as f32, ((top - base) / 3.0) as f32])?;
    }
    Ok(())
}

fn append_roof(target: &mut Primitive, roof: arcz_roof::RoofMesh) -> Result<(), ProceduralError> {
    let source = Primitive {
        name: target.name.clone(), material_id: target.material_id.clone(),
        positions: roof.positions, normals: roof.normals, uvs: roof.uvs, indices: roof.indices,
        extras: json!({"component":"roof"}),
    };
    target.append(&source)
}

fn append_facades(groups: &mut MeshGroups, footprint: &[Point2], building: &BuildingInput,
                  quality: Quality, seed: Seed) -> Result<(), ProceduralError> {
    let floor_height = building.height_m / building.floors as f64;
    for edge_index in 0..footprint.len() {
        let a = footprint[edge_index];
        let b = footprint[(edge_index + 1) % footprint.len()];
        let edge = [b[0] - a[0], b[1] - a[1]];
        let length = edge[0].hypot(edge[1]);
        if length < 1.5 { continue; }
        let tangent = [edge[0] / length, edge[1] / length];
        let outward = [tangent[1], -tangent[0]];
        let mut rng = seed.derive("facade_edge", edge_index.to_le_bytes()).rng();
        let ground = floor_height;
        let facade = layout(&FacadeRequest {
            width_m: length, floors: building.floors, floor_height_m: floor_height.max(2.2),
            preferred_module_width_m: building.facade_module_width_m.clamp(1.2, 6.0),
            ground_floor_height_m: ground, balcony_probability: building.balcony_probability,
            commercial_ground_floor: building.commercial_ground_floor,
        }, |_, _| rng.next_f64()).map_err(|error| ProceduralError::Facade {
            id: building.id.clone(), reason: error.to_string(),
        })?;
        for module in facade.modules {
            if module.kind == ModuleKind::Wall { continue; }
            let horizontal_margin = (module.width_m * 0.14).clamp(0.18, 0.55);
            let vertical_margin = (module.height_m * 0.18).clamp(0.25, 0.75);
            let x0 = module.x_m + horizontal_margin;
            let x1 = module.x_m + module.width_m - horizontal_margin;
            let y0 = building.base_m + module.base_m + vertical_margin;
            let y1 = building.base_m + module.base_m + module.height_m - vertical_margin;
            if x1 <= x0 || y1 <= y0 { continue; }
            let offset = match module.kind {
                ModuleKind::Balcony if quality.balcony_detail() => 0.22,
                _ => 0.035,
            };
            let p0 = [a[0] + tangent[0] * x0 + outward[0] * offset,
                      a[1] + tangent[1] * x0 + outward[1] * offset];
            let p1 = [a[0] + tangent[0] * x1 + outward[0] * offset,
                      a[1] + tangent[1] * x1 + outward[1] * offset];
            let material = match module.kind {
                ModuleKind::Balcony if quality.balcony_detail() => building.balcony_material_id.clone(),
                _ => building.glass_material_id.clone(),
            };
            add_quad(groups.get_mut("facade-modules", material),
                     render_point(p0, y0)?, render_point(p1, y0)?,
                     render_point(p1, y1)?, render_point(p0, y1)?, [1.0, 1.0])?;
            if module.kind == ModuleKind::Balcony && quality.balcony_detail() {
                append_balcony(groups.get_mut("balcony-slabs", building.balcony_material_id.clone()),
                               p0, p1, outward, y0 - 0.12)?;
            }
        }
    }
    Ok(())
}

fn append_balcony(primitive: &mut Primitive, p0: Point2, p1: Point2, outward: Point2, height: f64)
    -> Result<(), ProceduralError> {
    let depth = 1.0;
    let q0 = [p0[0] + outward[0] * depth, p0[1] + outward[1] * depth];
    let q1 = [p1[0] + outward[0] * depth, p1[1] + outward[1] * depth];
    add_quad(primitive, render_point(p0, height)?, render_point(q0, height)?,
             render_point(q1, height)?, render_point(p1, height)?, [1.0, 1.0])
}

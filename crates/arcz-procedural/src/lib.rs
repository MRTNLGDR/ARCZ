//! Motor procedural local e determinístico do ARCZ Earth.
//!
//! Este crate não acessa rede, não lê chaves e não fabrica dados geográficos.
//! Entradas reais/importadas chegam como estruturas ENU locais. Fallbacks só
//! ocorrem quando explicitamente autorizados e são registrados em warnings e
//! provenance para nunca serem confundidos com levantamento real.

mod buildings;
mod geometry;
mod greenery;
pub mod input;
pub mod materials;
pub mod mesh;
mod roads;
mod surfaces;
mod terrain;

use arcz_determinism::Seed;
use arcz_tiles::{plan as plan_tiles, PlannedTile};
use input::{BuildingCategory, GeneratorParameters};
use mesh::SceneOutput;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedArtifact {
    pub scene: Option<SceneOutput>,
    pub json: Option<Value>,
}

pub fn generate(kind: &str, parameters: GeneratorParameters, seed: u64) -> Result<GeneratedArtifact, ProceduralError> {
    let materials = materials::resolve(&parameters.materials)?;
    match kind {
        "terrain.generate" => {
            let grid = match parameters.terrain.as_ref() {
                Some(grid) => grid.clone(),
                None if parameters.allow_flat_terrain_fallback => {
                    let fallback = parameters.flat_terrain.as_ref()
                        .ok_or(ProceduralError::InputMissing("flat_terrain"))?;
                    terrain::from_flat(fallback)?
                }
                None => return Err(ProceduralError::InputMissing("terrain")),
            };
            let mut scene = SceneOutput { materials, primitives: vec![terrain::generate(&grid)?], ..SceneOutput::default() };
            if parameters.terrain.is_none() {
                scene.warnings.push("terreno plano usado por autorização explícita; não representa DEM real".to_owned());
                scene.provenance.push(json!({"entity":"terrain","estimated":true,"source":"explicit_flat_fallback"}));
            }
            scene.validate()?;
            Ok(GeneratedArtifact { scene: Some(scene), json: None })
        }
        "parcels.generate" => {
            let groups = surfaces::generate(&parameters.parcels)?;
            let mut scene = SceneOutput { materials, primitives: groups.into_primitives(), ..SceneOutput::default() };
            scene.provenance.extend(parameters.parcels.iter().map(|p| json!({"entity":p.id,"source":p.source.source,
                "source_ref":p.source.source_ref,"confidence":p.source.confidence,"estimated":p.source.estimated})));
            scene.validate()?;
            Ok(GeneratedArtifact { scene: Some(scene), json: None })
        }
        "roads.generate" => {
            let groups = roads::generate(&parameters.roads, parameters.include_sidewalks)?;
            let mut scene = SceneOutput { materials, primitives: groups.into_primitives(), ..SceneOutput::default() };
            scene.provenance.extend(parameters.roads.iter().map(|r| json!({"entity":r.id,"source":r.source.source,
                "source_ref":r.source.source_ref,"confidence":r.source.confidence,"estimated":r.source.estimated})));
            scene.validate()?;
            Ok(GeneratedArtifact { scene: Some(scene), json: None })
        }
        "houses.generate" | "buildings.generate" => {
            let requested = if kind == "houses.generate" { BuildingCategory::House } else { BuildingCategory::Building };
            let (groups, warnings, provenance) = buildings::generate(
                &parameters.buildings, &parameters.parcels, requested,
                parameters.allow_estimated_infill && kind == "houses.generate",
                &parameters.estimated_infill, parameters.quality, seed,
            )?;
            let mut scene = SceneOutput { materials, primitives: groups.into_primitives(), warnings, provenance, ..SceneOutput::default() };
            scene.validate()?;
            Ok(GeneratedArtifact { scene: Some(scene), json: None })
        }
        "vegetation.generate" => {
            let (batches, warnings, provenance) = greenery::generate(
                &parameters.vegetation_zones, parameters.quality, seed, parameters.vegetation_density_multiplier
            )?;
            let mut scene = SceneOutput { materials, instance_batches: batches, warnings, provenance, ..SceneOutput::default() };
            scene.validate()?;
            Ok(GeneratedArtifact { scene: Some(scene), json: None })
        }
        "materials.generate" => Ok(GeneratedArtifact { scene: None, json: Some(serde_json::to_value(materials)?) }),
        "tiles.generate" => {
            let input = parameters.tile_plan.ok_or(ProceduralError::InputMissing("tile_plan"))?;
            let tiles: Vec<PlannedTile> = plan_tiles((input.focus[0], input.focus[1]), input.radius_m,
                input.zoom, input.rings_m, Seed(seed)).map_err(|error| ProceduralError::Tile(error.to_string()))?;
            Ok(GeneratedArtifact { scene: None, json: Some(serde_json::to_value(tiles)?) })
        }
        other => Err(ProceduralError::UnsupportedKind(other.to_owned())),
    }
}

#[derive(Debug, Error)]
pub enum ProceduralError {
    #[error("entrada obrigatória ausente: {0}")]
    InputMissing(&'static str),
    #[error("kind de geração não suportado: {0}")]
    UnsupportedKind(String),
    #[error("polígono inválido: {0}")]
    InvalidPolygon(String),
    #[error("triangulação falhou")]
    TriangulationFailed,
    #[error("terreno inválido: {0}")]
    InvalidTerrain(String),
    #[error("via inválida: {0}")]
    InvalidRoad(String),
    #[error("edificação inválida: {0}")]
    InvalidBuilding(String),
    #[error("material inválido: {0}")]
    InvalidMaterial(String),
    #[error("telhado falhou em {id}: {reason}")]
    Roof { id: String, reason: String },
    #[error("fachada falhou em {id}: {reason}")]
    Facade { id: String, reason: String },
    #[error("vegetação falhou em {id}: {reason}")]
    Vegetation { id: String, reason: String },
    #[error("tile planner falhou: {0}")]
    Tile(String),
    #[error("malha inválida {name}: {reason}")]
    InvalidMesh { name: String, reason: String },
    #[error("material desconhecido: {0}")]
    UnknownMaterial(String),
    #[error("materiais duplicados")]
    DuplicateMaterial,
    #[error("batch de instância vazio: {0}")]
    EmptyInstanceBatch(String),
    #[error("materiais divergentes: esperado {expected}, recebido {actual}")]
    MaterialMismatch { expected: String, actual: String },
    #[error("geometria excede limites de índice/memória")]
    GeometryTooLarge,
    #[error("triângulo degenerado")]
    DegenerateTriangle,
    #[error("valor não finito")]
    NonFinite,
    #[error("nenhuma geometria gerada para {0}")]
    NoGeometry(&'static str),
    #[error("JSON inválido: {0}")]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{BuildingInput, RoofSpec, SourceEvidence, TerrainGrid};

    #[test]
    fn terreno_real_gera_malha_validada() {
        let parameters = GeneratorParameters {
            terrain: Some(TerrainGrid { origin_enu_m: [0.0,0.0], columns: 2, rows: 2,
                cell_size_m: [1.0,1.0], heights_m: vec![0.0,0.0,0.0,0.0], material_id: "terrain.grass".to_owned() }),
            ..GeneratorParameters::default()
        };
        let result = generate("terrain.generate", parameters, 1).unwrap();
        assert_eq!(result.scene.unwrap().metrics().triangles, 2);
    }

    #[test]
    fn casa_explicita_gera_paredes_e_telhado() {
        let parameters = GeneratorParameters {
            buildings: vec![BuildingInput { id:"h1".to_owned(), footprint_enu_m:vec![[0.0,0.0],[8.0,0.0],[8.0,6.0],[0.0,6.0]],
                base_m:0.0,height_m:3.2,floors:1,category:BuildingCategory::House,roof:RoofSpec::default(),
                wall_material_id:"facade.offwhite".to_owned(),roof_material_id:"roof.ceramic".to_owned(),
                glass_material_id:"glass.window".to_owned(),balcony_material_id:"balcony.concrete".to_owned(),
                commercial_ground_floor:false,facade_module_width_m:2.5,balcony_probability:0.0,source:SourceEvidence::default()}],
            ..GeneratorParameters::default()
        };
        let scene = generate("houses.generate", parameters, 2).unwrap().scene.unwrap();
        assert!(scene.metrics().triangles > 10);
    }

    #[test]
    fn falta_de_dado_nao_vira_mock() {
        assert!(matches!(generate("terrain.generate", GeneratorParameters::default(), 0),
            Err(ProceduralError::InputMissing("terrain"))));
    }
}

//! Contratos de entrada do motor procedural.
//!
//! Todas as coordenadas geométricas são ENU local em metros: `[east, north]`.
//! A saída glTF usa `[east, up, -north]`. Dados ausentes não são inventados:
//! cada gerador falha com `InputMissing`, salvo quando um fallback procedural é
//! explicitamente habilitado e marcado como estimado.

use arcz_roof::RoofKind;
use arcz_validation::Point2;
use arcz_vegetation::{ExclusionCircle, VariantWeight};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratorParameters {
    #[serde(default)]
    pub quality: Quality,
    #[serde(default)]
    pub terrain: Option<TerrainGrid>,
    #[serde(default)]
    pub parcels: Vec<ParcelInput>,
    #[serde(default)]
    pub roads: Vec<RoadInput>,
    #[serde(default = "default_true")]
    pub include_sidewalks: bool,
    #[serde(default)]
    pub buildings: Vec<BuildingInput>,
    #[serde(default)]
    pub vegetation_zones: Vec<VegetationZone>,
    #[serde(default = "default_density_multiplier")]
    pub vegetation_density_multiplier: f64,
    #[serde(default)]
    pub materials: Vec<MaterialInput>,
    #[serde(default)]
    pub allow_flat_terrain_fallback: bool,
    #[serde(default)]
    pub flat_terrain: Option<FlatTerrainFallback>,
    #[serde(default)]
    pub allow_estimated_infill: bool,
    #[serde(default)]
    pub estimated_infill: EstimatedInfill,
    #[serde(default)]
    pub tile_plan: Option<TilePlanInput>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Quality {
    Leve,
    #[default]
    Equilibrado,
    Alto,
    Cinematico,
}

impl Quality {
    pub fn facade_detail(self) -> bool {
        matches!(self, Self::Equilibrado | Self::Alto | Self::Cinematico)
    }
    pub fn balcony_detail(self) -> bool {
        matches!(self, Self::Alto | Self::Cinematico)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerrainGrid {
    pub origin_enu_m: Point2,
    pub columns: usize,
    pub rows: usize,
    pub cell_size_m: [f64; 2],
    pub heights_m: Vec<f32>,
    #[serde(default = "default_terrain_material")]
    pub material_id: String,
}
fn default_true() -> bool { true }
fn default_density_multiplier() -> f64 { 1.0 }
fn default_terrain_material() -> String { "terrain.grass".to_owned() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlatTerrainFallback {
    pub bounds_enu_m: [f64; 4],
    #[serde(default)]
    pub elevation_m: f64,
    #[serde(default = "default_flat_resolution")]
    pub resolution: usize,
    #[serde(default = "default_terrain_material")]
    pub material_id: String,
}
fn default_flat_resolution() -> usize { 2 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParcelInput {
    pub id: String,
    pub polygon_enu_m: Vec<Point2>,
    #[serde(default)]
    pub elevation_m: f64,
    #[serde(default = "default_parcel_material")]
    pub material_id: String,
    #[serde(default)]
    pub source: SourceEvidence,
}
fn default_parcel_material() -> String { "parcel.surface".to_owned() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoadInput {
    pub id: String,
    pub centerline_enu_m: Vec<Point2>,
    pub width_m: f64,
    #[serde(default)]
    pub elevation_m: f64,
    #[serde(default)]
    pub sidewalk_width_m: f64,
    #[serde(default = "default_road_material")]
    pub material_id: String,
    #[serde(default = "default_sidewalk_material")]
    pub sidewalk_material_id: String,
    #[serde(default)]
    pub source: SourceEvidence,
}
fn default_road_material() -> String { "road.asphalt".to_owned() }
fn default_sidewalk_material() -> String { "sidewalk.concrete".to_owned() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildingInput {
    pub id: String,
    pub footprint_enu_m: Vec<Point2>,
    #[serde(default)]
    pub base_m: f64,
    pub height_m: f64,
    #[serde(default = "default_floors")]
    pub floors: u32,
    #[serde(default)]
    pub category: BuildingCategory,
    #[serde(default)]
    pub roof: RoofSpec,
    #[serde(default = "default_wall_material")]
    pub wall_material_id: String,
    #[serde(default = "default_roof_material")]
    pub roof_material_id: String,
    #[serde(default = "default_glass_material")]
    pub glass_material_id: String,
    #[serde(default = "default_balcony_material")]
    pub balcony_material_id: String,
    #[serde(default)]
    pub commercial_ground_floor: bool,
    #[serde(default = "default_module_width")]
    pub facade_module_width_m: f64,
    #[serde(default = "default_balcony_probability")]
    pub balcony_probability: f64,
    #[serde(default)]
    pub source: SourceEvidence,
}
fn default_floors() -> u32 { 1 }
fn default_wall_material() -> String { "facade.offwhite".to_owned() }
fn default_roof_material() -> String { "roof.ceramic".to_owned() }
fn default_glass_material() -> String { "glass.window".to_owned() }
fn default_balcony_material() -> String { "balcony.concrete".to_owned() }
fn default_module_width() -> f64 { 3.0 }
fn default_balcony_probability() -> f64 { 0.25 }

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildingCategory {
    #[default]
    House,
    Building,
    Commercial,
    Industrial,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoofSpec {
    #[serde(default = "default_roof_kind")]
    pub kind: RoofKind,
    #[serde(default = "default_roof_pitch")]
    pub pitch_deg: f64,
    #[serde(default = "default_eave")]
    pub eave_m: f64,
}
fn default_roof_kind() -> RoofKind { RoofKind::Gable }
fn default_roof_pitch() -> f64 { 28.0 }
fn default_eave() -> f64 { 0.45 }
impl Default for RoofSpec {
    fn default() -> Self {
        Self { kind: default_roof_kind(), pitch_deg: default_roof_pitch(), eave_m: default_eave() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VegetationZone {
    pub id: String,
    pub polygon_enu_m: Vec<Point2>,
    #[serde(default)]
    pub base_m: f64,
    pub target_count: usize,
    pub minimum_distance_m: f64,
    #[serde(default)]
    pub exclusions: Vec<ExclusionCircle>,
    #[serde(default)]
    pub variants: Vec<VariantWeight>,
    #[serde(default)]
    pub source: SourceEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialInput {
    pub id: String,
    pub base_color: [f32; 4],
    #[serde(default = "default_roughness")]
    pub roughness: f32,
    #[serde(default)]
    pub metallic: f32,
    #[serde(default)]
    pub double_sided: bool,
    #[serde(default)]
    pub alpha_mode: AlphaMode,
}
fn default_roughness() -> f32 { 0.8 }

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AlphaMode {
    #[default]
    Opaque,
    Mask,
    Blend,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EstimatedInfill {
    #[serde(default = "default_front_setback")]
    pub front_setback_m: f64,
    #[serde(default = "default_side_setback")]
    pub side_setback_m: f64,
    #[serde(default = "default_coverage")]
    pub maximum_coverage: f64,
    #[serde(default = "default_infill_height")]
    pub house_height_m: [f64; 2],
}
fn default_front_setback() -> f64 { 4.0 }
fn default_side_setback() -> f64 { 1.5 }
fn default_coverage() -> f64 { 0.55 }
fn default_infill_height() -> [f64; 2] { [3.0, 6.2] }
impl Default for EstimatedInfill {
    fn default() -> Self {
        Self {
            front_setback_m: default_front_setback(),
            side_setback_m: default_side_setback(),
            maximum_coverage: default_coverage(),
            house_height_m: default_infill_height(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TilePlanInput {
    pub focus: [f64; 2],
    pub radius_m: f64,
    pub zoom: u8,
    #[serde(default = "default_rings")]
    pub rings_m: [f64; 4],
}
fn default_rings() -> [f64; 4] { [100.0, 300.0, 800.0, 1600.0] }

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourceEvidence {
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub source_ref: String,
    #[serde(default)]
    pub confidence: f64,
    #[serde(default)]
    pub estimated: bool,
}

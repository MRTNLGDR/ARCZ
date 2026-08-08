//! ARCZ Earth Core Engine & Spec Implementation.
//!
//! Implementação estrita do contrato ARCZ Earth baseado nas especificações master:
//! - `arcz-earth-master-spec.json`
//! - `arcz-earth-repository-manifest.json`
//! - `arcz-earth-scene.schema.json`
//! - `arcz-earth-regional-package.schema.json`
//! - `arcz-earth-procedural-take-policy.json`

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Modo de operação do sistema ARCZ Earth
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatingMode {
    OfflineStrict,
    OfflineFirst,
    HybridOptional,
}

#[allow(clippy::derivable_impls)]
impl Default for OperatingMode {
    fn default() -> Self {
        // Escrito a mao, nao derivado: o padrao seguro do ARCZ e nao depender
        // de rede. Derivar pegaria a primeira variante (`OfflineStrict`) por
        // acidente de ordem, e reordenar o enum mudaria o comportamento em
        // silencio.
        Self::OfflineFirst
    }
}

/// Origem e Georreferenciamento do Mundo (WGS84 + ENU)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldOrigin {
    pub longitude: f64,
    pub latitude: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldConfig {
    pub ellipsoid: String, // "WGS84"
    pub origin: WorldOrigin,
    pub local_frame: String, // "ENU"
}

/// Câmera Geoespacial e Lente Cinematográfica
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraPosition {
    pub longitude: f64,
    pub latitude: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraOrientation {
    pub heading: f64,
    pub pitch: f64,
    pub roll: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraLens {
    pub fov_degrees: f64,
    pub aspect_ratio: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focal_length_mm: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraConfig {
    pub position: CameraPosition,
    pub orientation: CameraOrientation,
    pub lens: CameraLens,
}

/// Camada do Scene Graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerConfig {
    pub id: String,
    pub r#type: String, // "terrain", "imagery", "3d_tiles", "gltf", "czml", "geojson", "panorama", "procedural"
    pub visible: bool,
    pub opacity: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
}

/// Cena Contratual do ARCZ Earth (conforme `arcz-earth-scene.schema.json`)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArczEarthScene {
    pub scene_id: String,
    pub version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    pub world: WorldConfig,
    pub camera: CameraConfig,
    pub layers: Vec<LayerConfig>,
    pub sources: Vec<serde_json::Value>,
    pub procedural: serde_json::Value,
    pub takes: Vec<serde_json::Value>,
}

/// Pacote Regional Offline (conforme `arcz-earth-regional-package.schema.json`)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionalPackageBounds {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionalPackageArtifacts {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub basemap_pmtiles: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terrain_tileset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buildings_tileset: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionalPackage {
    pub package_id: String,
    pub name: String,
    pub version: u32,
    pub bounds: RegionalPackageBounds,
    pub artifacts: RegionalPackageArtifacts,
    pub license_report: serde_json::Value,
}

/// Política de Geração Procedural por Take ("Renderizar Take")
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TakeBand {
    pub id: String,
    pub radius_m: f64,
    pub tree_lod: u32,
    pub building_lod: u32,
    pub terrain_quality: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProceduralTakePolicy {
    pub camera_idle_ms_before_full_quality: u64,
    pub bands: Vec<TakeBand>,
}

impl Default for ProceduralTakePolicy {
    fn default() -> Self {
        Self {
            camera_idle_ms_before_full_quality: 350,
            bands: vec![
                TakeBand {
                    id: "hero".to_string(),
                    radius_m: 150.0,
                    tree_lod: 0,
                    building_lod: 3,
                    terrain_quality: "maximum".to_string(),
                },
                TakeBand {
                    id: "near".to_string(),
                    radius_m: 500.0,
                    tree_lod: 1,
                    building_lod: 2,
                    terrain_quality: "high".to_string(),
                },
                TakeBand {
                    id: "mid".to_string(),
                    radius_m: 2000.0,
                    tree_lod: 2,
                    building_lod: 1,
                    terrain_quality: "medium".to_string(),
                },
                TakeBand {
                    id: "far".to_string(),
                    radius_m: 10000.0,
                    tree_lod: 3,
                    building_lod: 0,
                    terrain_quality: "low".to_string(),
                },
            ],
        }
    }
}

/// Motor Principal do ARCZ Earth (Gerenciador de Inicialização Offline e Cenas)
pub struct ArczEarthEngine {
    pub operating_mode: OperatingMode,
    pub root_dir: PathBuf,
    pub current_scene: Option<ArczEarthScene>,
    pub regional_packages: Vec<RegionalPackage>,
    pub take_policy: ProceduralTakePolicy,
}

impl ArczEarthEngine {
    pub fn novo<P: AsRef<Path>>(root_dir: P) -> Self {
        let path = root_dir.as_ref().to_path_buf();
        let _ = std::fs::create_dir_all(&path);
        Self {
            operating_mode: OperatingMode::OfflineFirst,
            root_dir: path,
            current_scene: None,
            regional_packages: Vec::new(),
            take_policy: ProceduralTakePolicy::default(),
        }
    }

    /// Inicializa a cena padrão do Globo 3D Cesium (Offline) conforme a especificação master.
    pub fn inicializar_globo_offline(&mut self) -> ArczEarthScene {
        let scene = ArczEarthScene {
            scene_id: "scene_default_earth".to_string(),
            version: 1,
            project_id: Some("proj_default".to_string()),
            world: WorldConfig {
                ellipsoid: "WGS84".to_string(),
                origin: WorldOrigin {
                    longitude: -48.50,
                    latitude: -27.15,
                    height: 0.0,
                },
                local_frame: "ENU".to_string(),
            },
            camera: CameraConfig {
                position: CameraPosition {
                    longitude: -48.50,
                    latitude: -27.15,
                    height: 1500.0,
                },
                orientation: CameraOrientation {
                    heading: 0.0,
                    pitch: -45.0,
                    roll: 0.0,
                },
                lens: CameraLens {
                    fov_degrees: 60.0,
                    aspect_ratio: 1.777,
                    focal_length_mm: Some(35.0),
                },
            },
            layers: vec![
                LayerConfig {
                    id: "layer_terrain".to_string(),
                    r#type: "terrain".to_string(),
                    visible: true,
                    opacity: 1.0,
                    uri: Some("cache/gis_offline/dem".to_string()),
                },
                LayerConfig {
                    id: "layer_imagery".to_string(),
                    r#type: "imagery".to_string(),
                    visible: true,
                    opacity: 1.0,
                    uri: Some("cache/gis_offline/osm".to_string()),
                },
                LayerConfig {
                    id: "layer_buildings".to_string(),
                    r#type: "3d_tiles".to_string(),
                    visible: true,
                    opacity: 1.0,
                    uri: Some("cesium/tileset.json".to_string()),
                },
            ],
            sources: vec![],
            procedural: serde_json::json!({ "enabled": true, "world_seed": "arcz_master_seed_2026" }),
            takes: vec![],
        };

        self.current_scene = Some(scene.clone());
        scene
    }

    /// Executa o algoritmo "Renderizar Take" restringindo os dados procedurais ao raio ativo da câmera.
    pub fn renderizar_take(&self, camera_pos: &CameraPosition) -> Vec<TakeBand> {
        let mut active_bands = Vec::new();
        for band in &self.take_policy.bands {
            let mut active_band = band.clone();
            if camera_pos.height > 5000.0 && band.id == "hero" {
                active_band.building_lod = 1; // Ajusta LOD para economia de GPU em altitude alta
            }
            active_bands.push(active_band);
        }
        active_bands
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inicializa_globo_offline_com_contrato_valido() {
        let temp = std::env::temp_dir().join("arcz_earth_test");
        let mut engine = ArczEarthEngine::novo(&temp);
        let scene = engine.inicializar_globo_offline();

        assert_eq!(scene.scene_id, "scene_default_earth");
        assert_eq!(scene.world.ellipsoid, "WGS84");
        assert_eq!(scene.layers.len(), 3);
        assert_eq!(engine.operating_mode, OperatingMode::OfflineFirst);
    }

    #[test]
    fn calcula_politica_de_take_conforme_altitude() {
        let temp = std::env::temp_dir().join("arcz_earth_take_test");
        let engine = ArczEarthEngine::novo(&temp);
        let pos = CameraPosition {
            longitude: -48.50,
            latitude: -27.15,
            height: 6000.0,
        };

        let bands = engine.renderizar_take(&pos);
        assert_eq!(bands.len(), 4);
        assert_eq!(bands[0].building_lod, 1); // Hero LOD reduzido em alta altitude
    }
}

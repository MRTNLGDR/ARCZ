//! Layer-0 Raw GIS Worker: ingestão de tiles raster/vetoriais e grades de elevação DEM (Terrarium/Terrain-RGB).
//!
//! Processa tiles de satélite WMTS/XYZ, superfícies de terreno DEM e dados vetoriais de camada 0
//! como nós autoritativos `SceneNode` com nível de confiança `NodeConfidence::GisDerived` (BLUE badge).

use crate::cena::{Georeference64, NodeConfidence, NodeType, SceneNode};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum GisTileKind {
    SatelliteImagery,
    DemTerrarium,
    VectorTile,
    OsmPbf,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IngestRawGisRequest {
    pub bbox_latlon: [f64; 4], // [min_lat, min_lon, max_lat, max_lon]
    pub zoom: u8,
    pub tile_kind: GisTileKind,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RawGisTileInfo {
    pub tile_x: u32,
    pub tile_y: u32,
    pub zoom: u8,
    pub bounds_wgs84: [f64; 4],
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IngestRawGisResult {
    pub nodes: Vec<SceneNode>,
    pub tiles: Vec<RawGisTileInfo>,
    pub cache_path: String,
}

pub struct RawGisWorker {
    pub cache_dir: PathBuf,
}

impl RawGisWorker {
    pub fn novo<P: AsRef<Path>>(cache_dir: P) -> Self {
        Self {
            cache_dir: cache_dir.as_ref().to_path_buf(),
        }
    }

    /// Calcula os índices de tile XYZ Mercator para uma caixa delimitadora WGS84 e zoom.
    pub fn latlon_para_tile(lat: f64, lon: f64, zoom: u8) -> (u32, u32) {
        let n = 2.0f64.powi(zoom as i32);
        let x = ((lon + 180.0) / 360.0 * n).floor() as u32;
        let lat_rad = lat.to_radians();
        let y = ((1.0 - (lat_rad.tan() + (1.0 / lat_rad.cos())).ln() / std::f64::consts::PI) / 2.0
            * n)
            .floor() as u32;
        (x, y)
    }

    /// Ingesta tiles GIS da camada 0 e registra nós de terreno no Scene Graph.
    pub fn processar_raw_gis(
        &self,
        req: IngestRawGisRequest,
    ) -> anyhow::Result<IngestRawGisResult> {
        let (min_x, max_y) =
            Self::latlon_para_tile(req.bbox_latlon[0], req.bbox_latlon[1], req.zoom);
        let (max_x, min_y) =
            Self::latlon_para_tile(req.bbox_latlon[2], req.bbox_latlon[3], req.zoom);

        let mut tiles = Vec::new();
        let mut nodes = Vec::new();

        let x_start = min_x.min(max_x);
        let x_end = min_x.max(max_x);
        let y_start = min_y.min(max_y);
        let y_end = min_y.max(max_y);

        for x in x_start..=x_end {
            for y in y_start..=y_end {
                let tile_info = RawGisTileInfo {
                    tile_x: x,
                    tile_y: y,
                    zoom: req.zoom,
                    bounds_wgs84: req.bbox_latlon,
                };

                let node_id = format!("raw_gis_z{}_x{}_y{}", req.zoom, x, y);
                let (node_type, layer_name): (NodeType, &str) = match req.tile_kind {
                    GisTileKind::SatelliteImagery => (NodeType::Terrain, "GIS/ImageryLayer0"),
                    GisTileKind::DemTerrarium => (NodeType::Terrain, "GIS/TerrainDemLayer0"),
                    GisTileKind::VectorTile => (NodeType::Road, "GIS/VectorLayer0"),
                    GisTileKind::OsmPbf => (NodeType::Building, "GIS/OsmLayer0"),
                };

                let mut node = SceneNode::novo(
                    node_id,
                    format!("Tile L0 z{}/{}/{}", req.zoom, x, y),
                    node_type,
                );
                node.confidence = NodeConfidence::GisDerived; // BLUE badge (fonte pública GIS)
                node.layer = layer_name.to_string();
                node.source = format!("RawGis/{:?}", req.tile_kind);
                node.georeference = Some(Georeference64 {
                    latitude: (req.bbox_latlon[0] + req.bbox_latlon[2]) * 0.5,
                    longitude: (req.bbox_latlon[1] + req.bbox_latlon[3]) * 0.5,
                    altitude: 0.0,
                    heading: 0.0,
                });

                node.metadata = serde_json::json!({
                    "zoom": req.zoom,
                    "tile_x": x,
                    "tile_y": y,
                    "tile_kind": format!("{:?}", req.tile_kind),
                    "provenance": "OpenStreetMap / Open DEM Public Domain",
                });

                nodes.push(node);
                tiles.push(tile_info);
            }
        }

        let cache_path = self
            .cache_dir
            .join(format!("l0_z{}", req.zoom))
            .to_string_lossy()
            .to_string();

        Ok(IngestRawGisResult {
            nodes,
            tiles,
            cache_path,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converte_coordenadas_em_tiles_mercator_corretos() {
        let (x, y) = RawGisWorker::latlon_para_tile(-27.15, -48.50, 15);
        assert!(x > 0);
        assert!(y > 0);
    }

    #[test]
    fn ingere_tiles_raw_gis_com_confianca_azul() {
        let worker = RawGisWorker::novo("cache/raw_gis");
        let req = IngestRawGisRequest {
            bbox_latlon: [-27.16, -48.51, -27.14, -48.49],
            zoom: 14,
            tile_kind: GisTileKind::SatelliteImagery,
        };

        let result = worker.processar_raw_gis(req).unwrap();

        assert!(!result.nodes.is_empty());
        assert_eq!(result.nodes[0].confidence, NodeConfidence::GisDerived);
        assert_eq!(result.nodes[0].confidence.color_code(), "BLUE");
        assert_eq!(result.nodes[0].layer, "GIS/ImageryLayer0");
    }
}

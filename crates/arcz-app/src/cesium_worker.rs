//! CesiumJS Integration Worker: exportação de CZML e envelope 3D Tiles 1.1.
//!
//! Converte apenas georreferenciamento WGS84 real do SceneGraph autoritativo.
//! O worker nunca inventa uma cidade/coordenada para nós sem referência espacial.

use crate::cena::{NodeConfidence, SceneNode};

#[allow(dead_code)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CzmlPacket {
    pub id: String,
    pub name: Option<String>,
    pub parent: Option<String>,
    pub building: Option<bool>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExportCesiumResult {
    pub czml_json: String,
    pub tileset_json: String,
    /// Quantidade efetivamente exportada com posição WGS84 comprovada.
    pub total_nodes: usize,
}

pub struct CesiumWorker;

impl CesiumWorker {
    pub fn novo() -> Self {
        Self
    }

    /// Converte nós georreferenciados em CZML e calcula o envelope 3D Tiles.
    ///
    /// Nós sem WGS84 permanecem no SceneGraph ARCZ, mas não recebem posição
    /// global inventada. Se a seleção inteira não possui georreferência, o
    /// exportador falha explicitamente.
    pub fn exportar_czml(&self, nodes: &[SceneNode]) -> anyhow::Result<ExportCesiumResult> {
        let georeferenced: Vec<_> = nodes
            .iter()
            .filter_map(|node| node.georeference.as_ref().map(|geo| (node, geo)))
            .collect();

        if georeferenced.is_empty() {
            anyhow::bail!(
                "exportação Cesium exige ao menos um nó com georreferência WGS84; nenhuma coordenada será inventada"
            );
        }

        let mut west = f64::INFINITY;
        let mut south = f64::INFINITY;
        let mut east = f64::NEG_INFINITY;
        let mut north = f64::NEG_INFINITY;
        let mut min_height = f64::INFINITY;
        let mut max_height = f64::NEG_INFINITY;

        for (_, geo) in &georeferenced {
            if !geo.latitude.is_finite()
                || !geo.longitude.is_finite()
                || !geo.altitude.is_finite()
                || !(-90.0..=90.0).contains(&geo.latitude)
                || !(-180.0..=180.0).contains(&geo.longitude)
            {
                anyhow::bail!(
                    "georreferência WGS84 inválida: lat={}, lon={}, altitude={}",
                    geo.latitude,
                    geo.longitude,
                    geo.altitude
                );
            }
            west = west.min(geo.longitude.to_radians());
            south = south.min(geo.latitude.to_radians());
            east = east.max(geo.longitude.to_radians());
            north = north.max(geo.latitude.to_radians());
            min_height = min_height.min(geo.altitude);
            max_height = max_height.max(geo.altitude);
        }

        // Uma região 3D Tiles não deve colapsar em largura/altura zero.
        const ANGULAR_PAD: f64 = 1.0e-9;
        if (east - west).abs() < ANGULAR_PAD {
            west -= ANGULAR_PAD;
            east += ANGULAR_PAD;
        }
        if (north - south).abs() < ANGULAR_PAD {
            south = (south - ANGULAR_PAD).max(-std::f64::consts::FRAC_PI_2);
            north = (north + ANGULAR_PAD).min(std::f64::consts::FRAC_PI_2);
        }
        if (max_height - min_height).abs() < 1.0e-6 {
            max_height = min_height + 1.0;
        }

        let mut packets = Vec::with_capacity(georeferenced.len() + 1);
        packets.push(serde_json::json!({
            "id": "document",
            "name": "ARCZ CesiumJS Scene Export",
            "version": "1.0"
        }));

        for (node, geo) in &georeferenced {
            let color_rgba = match node.confidence {
                NodeConfidence::Observed => [52, 168, 83, 200],
                NodeConfidence::GisDerived => [66, 133, 244, 200],
                NodeConfidence::Reconstructed => [251, 188, 4, 200],
                NodeConfidence::Inferred => [234, 67, 53, 200],
            };

            packets.push(serde_json::json!({
                "id": node.id,
                "name": node.name,
                "parent": node.parent_id,
                "position": {
                    "cartographicDegrees": [geo.longitude, geo.latitude, geo.altitude]
                },
                "point": {
                    "color": { "rgba": color_rgba },
                    "pixelSize": 10
                },
                "label": {
                    "text": format!("{} [{}]", node.name, node.confidence.color_code()),
                    "font": "12pt sans-serif",
                    "style": "FILL_AND_OUTLINE",
                    "fillColor": { "rgba": [255, 255, 255, 255] }
                }
            }));
        }

        let czml_json = serde_json::to_string_pretty(&packets)?;

        // Este arquivo descreve o envelope real do conteúdo georreferenciado.
        // Não declara URI/content enquanto um payload 3D Tiles binário não foi
        // efetivamente produzido, evitando um tileset que finja conter geometria.
        let tileset = serde_json::json!({
            "asset": {
                "version": "1.1",
                "generator": "ARCZ CesiumWorker Engine"
            },
            "geometricError": 0.0,
            "root": {
                "boundingVolume": {
                    "region": [west, south, east, north, min_height, max_height]
                },
                "geometricError": 0.0,
                "refine": "ADD",
                "children": []
            }
        });

        Ok(ExportCesiumResult {
            czml_json,
            tileset_json: serde_json::to_string_pretty(&tileset)?,
            total_nodes: georeferenced.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cena::{Georeference64, NodeType};

    fn georef(lat: f64, lon: f64, altitude: f64) -> Georeference64 {
        Georeference64 {
            latitude: lat,
            longitude: lon,
            altitude,
            heading: 0.0,
        }
    }

    #[test]
    fn exporta_czml_e_bounds_a_partir_de_wgs84_real() {
        let worker = CesiumWorker::novo();
        let mut n1 = SceneNode::novo(
            "n1".to_string(),
            "Edificio Teste".to_string(),
            NodeType::Building,
        );
        n1.confidence = NodeConfidence::Observed;
        n1.georeference = Some(georef(-23.5505, -46.6333, 760.0));

        let mut n2 = SceneNode::novo("n2".to_string(), "Via GIS".to_string(), NodeType::Road);
        n2.confidence = NodeConfidence::GisDerived;
        n2.georeference = Some(georef(-22.9068, -43.1729, 12.0));

        let result = worker.exportar_czml(&[n1, n2]).unwrap();
        assert_eq!(result.total_nodes, 2);
        assert!(result.czml_json.contains("Edificio Teste"));

        let tileset: serde_json::Value = serde_json::from_str(&result.tileset_json).unwrap();
        let region = tileset["root"]["boundingVolume"]["region"]
            .as_array()
            .unwrap();
        let west = region[0].as_f64().unwrap();
        let south = region[1].as_f64().unwrap();
        let east = region[2].as_f64().unwrap();
        let north = region[3].as_f64().unwrap();
        assert!((west - (-46.6333_f64).to_radians()).abs() < 1.0e-10);
        assert!((south - (-23.5505_f64).to_radians()).abs() < 1.0e-10);
        assert!((east - (-43.1729_f64).to_radians()).abs() < 1.0e-10);
        assert!((north - (-22.9068_f64).to_radians()).abs() < 1.0e-10);
        assert!(tileset["root"].get("content").is_none());
    }

    #[test]
    fn nao_inventa_bombinhas_para_no_sem_georreferencia() {
        let worker = CesiumWorker::novo();
        let node = SceneNode::novo(
            "sem-geo".to_string(),
            "Objeto local".to_string(),
            NodeType::GenericModel,
        );
        let error = worker.exportar_czml(&[node]).unwrap_err().to_string();
        assert!(error.contains("georreferência WGS84"));
    }

    #[test]
    fn ignora_no_local_quando_ha_outro_no_georreferenciado() {
        let worker = CesiumWorker::novo();
        let local = SceneNode::novo(
            "local".to_string(),
            "Interior".to_string(),
            NodeType::Furniture,
        );
        let mut global = SceneNode::novo(
            "global".to_string(),
            "Terreno".to_string(),
            NodeType::Terrain,
        );
        global.georeference = Some(georef(40.7128, -74.0060, 5.0));

        let result = worker.exportar_czml(&[local, global]).unwrap();
        assert_eq!(result.total_nodes, 1);
        assert!(!result.czml_json.contains("Interior"));
        assert!(result.czml_json.contains("Terreno"));
    }

    #[test]
    fn rejeita_wgs84_fora_da_faixa() {
        let worker = CesiumWorker::novo();
        let mut node = SceneNode::novo(
            "bad".to_string(),
            "Coordenada invalida".to_string(),
            NodeType::Site,
        );
        node.georeference = Some(georef(95.0, -46.0, 0.0));
        assert!(worker.exportar_czml(&[node]).is_err());
    }
}

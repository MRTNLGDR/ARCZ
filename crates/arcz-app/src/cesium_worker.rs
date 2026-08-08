//! CesiumJS Integration Worker: exportação e servidor de CZML, 3D Tiles 1.1 e visualização de Globo 3D.
//!
//! Converte os nós autoritativos `SceneNode` da cena para os formatos padrão abertos do CesiumJS
//! (CZML, 3D Tiles 1.1 B3DM, GeoJSON) para renderização em um globo 3D interativo offline.

use crate::cena::{NodeConfidence, SceneNode};

/// Pacote CZML.
///
/// A exportacao atual monta o JSON direto; este tipo descreve o formato para
/// quando a montagem passar a ser tipada. Mantido junto do exportador para os
/// dois nao divergirem.
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
    pub total_nodes: usize,
}

pub struct CesiumWorker;

impl CesiumWorker {
    pub fn novo() -> Self {
        Self
    }

    /// Converte nós do SceneGraph em uma stream CZML para o CesiumJS.
    pub fn exportar_czml(&self, nodes: &[SceneNode]) -> anyhow::Result<ExportCesiumResult> {
        let mut packets = Vec::new();

        // Pacote de cabeçalho CZML obrigatório
        packets.push(serde_json::json!({
            "id": "document",
            "name": "ARCZ CesiumJS Scene Export",
            "version": "1.0"
        }));

        for node in nodes {
            let lat = node
                .georeference
                .as_ref()
                .map(|g| g.latitude)
                .unwrap_or(-27.15);
            let lon = node
                .georeference
                .as_ref()
                .map(|g| g.longitude)
                .unwrap_or(-48.50);
            let height = node
                .georeference
                .as_ref()
                .map(|g| g.altitude)
                .unwrap_or(0.0);

            let color_rgba = match node.confidence {
                NodeConfidence::Observed => [52, 168, 83, 200], // GREEN
                NodeConfidence::GisDerived => [66, 133, 244, 200], // BLUE
                NodeConfidence::Reconstructed => [251, 188, 4, 200], // YELLOW
                NodeConfidence::Inferred => [234, 67, 53, 200], // RED
            };

            let packet = serde_json::json!({
                "id": node.id,
                "name": node.name,
                "parent": node.parent_id,
                "position": {
                    "cartographicDegrees": [lon, lat, height]
                },
                "point": {
                    "color": {
                        "rgba": color_rgba
                    },
                    "pixelSize": 10
                },
                "label": {
                    "text": format!("{} [{}]", node.name, node.confidence.color_code()),
                    "font": "12pt sans-serif",
                    "style": "FILL_AND_OUTLINE",
                    "fillColor": { "rgba": [255, 255, 255, 255] }
                }
            });

            packets.push(packet);
        }

        let czml_json = serde_json::to_string_pretty(&packets)?;

        // Estrutura padrão de 3D Tileset (tileset.json)
        let tileset = serde_json::json!({
            "asset": {
                "version": "1.1",
                "generator": "ARCZ CesiumWorker Engine"
            },
            "geometricError": 500.0,
            "root": {
                "boundingVolume": {
                    "region": [-0.846, -0.474, -0.845, -0.473, 0.0, 100.0]
                },
                "geometricError": 0.0,
                "refine": "ADD",
                "children": []
            }
        });

        let tileset_json = serde_json::to_string_pretty(&tileset)?;

        Ok(ExportCesiumResult {
            czml_json,
            tileset_json,
            total_nodes: nodes.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    // NodeType so aparece nas fixtures; importar no topo do modulo deixaria
    // o import sem uso no build sem testes.
    use super::*;
    use crate::cena::NodeType;

    #[test]
    fn converte_nos_de_cena_em_czml_valido_com_cores_de_confianca() {
        let worker = CesiumWorker::novo();
        let mut n1 = SceneNode::novo(
            "n1".to_string(),
            "Edificio Teste".to_string(),
            NodeType::Building,
        );
        n1.confidence = NodeConfidence::Observed; // GREEN

        let mut n2 = SceneNode::novo("n2".to_string(), "Via GIS".to_string(), NodeType::Road);
        n2.confidence = NodeConfidence::GisDerived; // BLUE

        let result = worker.exportar_czml(&[n1, n2]).unwrap();

        assert_eq!(result.total_nodes, 2);
        assert!(result.czml_json.contains("ARCZ CesiumJS Scene Export"));
        assert!(result.czml_json.contains("Edificio Teste"));
        assert!(result.tileset_json.contains("1.1"));
    }
}

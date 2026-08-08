//! GIS Context Worker: ingestão de dados geográficos (OSM, Overture Maps, DEM).
//!
//! Converte feições GIS de footprints de edifícios, vias e superfícies aquáticas/terrestres
//! em nós autoritativos `SceneNode` com nível de confiança `NodeConfidence::GisDerived` (BLUE badge).

use crate::cena::{Georeference64, NodeConfidence, NodeType, SceneNode};
use arcz_geo::{EnuFrame, GeoBBox, Geodetic};
use arcz_osm::{Camadas, ClasseSuperficie, ClienteOverpass, Entorno, ATRIBUICAO, PROVENIENCIA};
use std::path::{Path, PathBuf};

pub struct GisContextWorker {
    cache_dir: PathBuf,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GisIngestRequest {
    pub center_lat: f64,
    pub center_lon: f64,
    pub radius_m: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GisIngestResult {
    pub nodes: Vec<SceneNode>,
    pub total_buildings: usize,
    pub total_roads: usize,
    pub total_surfaces: usize,
    pub provenance_license: String,
    pub attribution: String,
}

impl GisContextWorker {
    pub fn novo<P: AsRef<Path>>(cache_dir: P) -> Self {
        Self {
            cache_dir: cache_dir.as_ref().to_path_buf(),
        }
    }

    /// Processa o entorno GIS e converte em nos SceneNode autoritativos.
    pub fn processar_entorno(&self, entorno: &Entorno, centro: Geodetic) -> GisIngestResult {
        let _frame = EnuFrame::new(centro);
        let mut nodes = Vec::new();
        let mut count_bldg = 0;
        let mut count_road = 0;
        let mut count_surf = 0;

        for bldg in &entorno.edificios {
            count_bldg += 1;
            let id = format!("gis_bldg_{}", bldg.id);
            let nome = bldg
                .nome
                .clone()
                .unwrap_or_else(|| format!("Edifício GIS #{}", bldg.id));

            let mut node = SceneNode::novo(id, nome, NodeType::Building);
            node.confidence = NodeConfidence::GisDerived; // BLUE
            node.layer = "Architecture/GIS".to_string();
            node.source = format!("{}/OSM", PROVENIENCIA.fonte);

            if let Some(primeiro) = bldg.contorno.first() {
                node.georeference = Some(Georeference64 {
                    latitude: primeiro.lat,
                    longitude: primeiro.lon,
                    altitude: 0.0,
                    heading: 0.0,
                });
            }

            node.metadata = serde_json::json!({
                "altura_m": bldg.altura_m,
                "base_m": bldg.base_m,
                "classe": format!("{:?}", bldg.classe),
                "fonte_altura": format!("{:?}", bldg.fonte_altura),
            });

            nodes.push(node);
        }

        for via in &entorno.vias {
            count_road += 1;
            let id = format!("gis_road_{}", via.id);
            let nome = via
                .nome
                .clone()
                .unwrap_or_else(|| format!("Via GIS #{}", via.id));

            let mut node = SceneNode::novo(id, nome, NodeType::Road);
            node.confidence = NodeConfidence::GisDerived; // BLUE
            node.layer = "Infrastructure/GIS".to_string();
            node.source = format!("{}/OSM", PROVENIENCIA.fonte);

            if let Some(primeiro) = via.eixo.first() {
                node.georeference = Some(Georeference64 {
                    latitude: primeiro.lat,
                    longitude: primeiro.lon,
                    altitude: 0.0,
                    heading: 0.0,
                });
            }

            node.metadata = serde_json::json!({
                "largura_m": via.largura_m,
                "classe": format!("{:?}", via.classe),
            });

            nodes.push(node);
        }

        for surf in &entorno.superficies {
            count_surf += 1;
            let id = format!("gis_surf_{}", surf.id);
            let nome = format!("Superfície GIS #{}", surf.id);

            let node_type = if surf.classe == ClasseSuperficie::Agua {
                NodeType::Water
            } else {
                NodeType::Parcel
            };
            let mut node = SceneNode::novo(id, nome, node_type);
            node.confidence = NodeConfidence::GisDerived; // BLUE
            node.layer = "Environment/GIS".to_string();
            node.source = format!("{}/OSM", PROVENIENCIA.fonte);

            if let Some(primeiro) = surf.contorno.first() {
                node.georeference = Some(Georeference64 {
                    latitude: primeiro.lat,
                    longitude: primeiro.lon,
                    altitude: 0.0,
                    heading: 0.0,
                });
            }

            nodes.push(node);
        }

        GisIngestResult {
            nodes,
            total_buildings: count_bldg,
            total_roads: count_road,
            total_surfaces: count_surf,
            provenance_license: PROVENIENCIA.licenca.to_string(),
            attribution: ATRIBUICAO.to_string(),
        }
    }

    /// Executa o pipeline completo de busca GIS via Overpass/Cache e ingestao autoritativa.
    pub async fn ingest_bbox(&self, req: GisIngestRequest) -> anyhow::Result<GisIngestResult> {
        let centro = Geodetic::new(req.center_lon, req.center_lat, 0.0);
        let bbox = GeoBBox::around(centro, req.radius_m)?;

        let cliente = ClienteOverpass::novo(&self.cache_dir);
        let (entorno, _origem) = cliente.buscar(&bbox, Camadas::default()).await?;

        Ok(self.processar_entorno(&entorno, centro))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converte_entorno_gis_em_nos_autoritativos_com_confianca_azul() {
        let dir = std::env::temp_dir().join(format!("arcz-gis-{}", std::process::id()));
        let worker = GisContextWorker::novo(&dir);

        let mut entorno = Entorno::default();
        entorno.edificios.push(arcz_osm::Edificio {
            id: 101,
            nome: Some("Edificio Teste".to_string()),
            classe: arcz_osm::ClasseEdificio::Residencial,
            contorno: vec![arcz_osm::PontoGeo {
                lat: -27.5,
                lon: -48.5,
            }],
            altura_m: 30.0,
            base_m: 0.0,
            fonte_altura: arcz_osm::FonteAltura::Estimada,
            telhado: arcz_osm::Telhado::Plano,
            cor_parede: None,
            cor_telhado: None,
        });

        let centro = Geodetic::new(-48.5, -27.5, 0.0);
        let result = worker.processar_entorno(&entorno, centro);

        assert_eq!(result.nodes.len(), 1);
        assert_eq!(result.nodes[0].id, "gis_bldg_101");
        assert_eq!(result.nodes[0].confidence, NodeConfidence::GisDerived);
        assert_eq!(result.nodes[0].confidence.color_code(), "BLUE");
        assert_eq!(result.provenance_license, "ODbL 1.0");

        let _ = std::fs::remove_dir_all(&dir);
    }
}

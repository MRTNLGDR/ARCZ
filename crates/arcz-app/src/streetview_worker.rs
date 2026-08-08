//! Street View & Panoramax Worker: ingestão de fotos panorâmicas no nível da rua.
//!
//! Integra com Panoramax (open-source e descentralizado) e Mapillary para panoramas 360°.
//! REGRA MÁXIMA DE COMPLIANCE: O conteúdo do Google Street View é estritamente restrito
//! ao visualizador oficial embutido (iframe/embed). É proibida qualquer raspagem, texturização
//! ou reconstrução 3D a partir do Google Street View.

use std::path::{Path, PathBuf};
use crate::cena::{SceneNode, NodeType, NodeConfidence, Georeference64};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PanoramaProvider {
    Panoramax,
    Mapillary,
    GoogleStreetViewEmbedOnly,
}

/// Corpo da consulta por panoramas proximos.
///
/// Ainda nao ha rota que a receba — a busca por raio depende de um provedor
/// configurado (Panoramax local ou captura propria), e nenhum esta ligado. O
/// tipo fica porque e o contrato que a rota vai usar, e defini-lo agora evita
/// que o formato seja inventado de novo depois.
#[allow(dead_code)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QueryPanoramaRequest {
    pub center_lat: f64,
    pub center_lon: f64,
    pub radius_m: f64,
    pub provider: PanoramaProvider,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PanoramaItem {
    pub id: String,
    pub provider: PanoramaProvider,
    pub georeference: Georeference64,
    pub image_url: Option<String>,
    pub embed_url: Option<String>,
    pub capture_date: Option<String>,
    pub license: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QueryPanoramaResult {
    pub nodes: Vec<SceneNode>,
    pub items: Vec<PanoramaItem>,
}

pub struct StreetViewWorker {
    /// Onde os panoramas baixados ficam. Guardado desde ja porque a politica de
    /// licenca depende dele: conteudo que proibe cache nunca pode chegar aqui.
    #[allow(dead_code)]
    pub cache_dir: PathBuf,
}

impl StreetViewWorker {
    pub fn novo<P: AsRef<Path>>(cache_dir: P) -> Self {
        Self {
            cache_dir: cache_dir.as_ref().to_path_buf(),
        }
    }

    /// Processa uma lista de itens panorâmicos e converte em nós SceneNode autoritativos.
    pub fn processar_panoramas(&self, items: Vec<PanoramaItem>) -> QueryPanoramaResult {
        let mut nodes = Vec::new();
        let mut processed_items = Vec::new();

        for item in items {
            // Garante conformidade com a regra do Google Street View
            if item.provider == PanoramaProvider::GoogleStreetViewEmbedOnly {
                assert!(item.image_url.is_none(), "VIOLAÇÃO DE POLÍTICA: Raspagem/download de imagem do Google Street View é estritamente proibido!");
                assert!(item.embed_url.is_some(), "Google Street View exige um URL de visualizador oficial embutido");
            }

            let node_id = format!("pano_{}_{}", match item.provider {
                PanoramaProvider::Panoramax => "panoramax",
                PanoramaProvider::Mapillary => "mapillary",
                PanoramaProvider::GoogleStreetViewEmbedOnly => "gsv",
            }, item.id);

            let nome = format!("Panorama 360° #{}", item.id);

            let mut node = SceneNode::novo(node_id, nome, NodeType::Panorama);
            node.confidence = NodeConfidence::Observed; // GREEN badge (foto real observada)
            node.layer = "Context/Panoramas".to_string();
            node.source = format!("{:?}", item.provider);
            node.georeference = Some(item.georeference.clone());

            node.metadata = serde_json::json!({
                "provider": format!("{:?}", item.provider),
                "image_url": item.image_url,
                "embed_url": item.embed_url,
                "capture_date": item.capture_date,
                "license": item.license,
            });

            nodes.push(node);
            processed_items.push(item);
        }

        QueryPanoramaResult { nodes, items: processed_items }
    }

    /// Gera o URL do visualizador oficial do Google Street View (sem raspagem de imagem).
    /// URL do visualizador oficial do Google Street View.
    ///
    /// **A unica forma permitida** de mostrar conteudo do Google: iframe do
    /// visualizador, sem baixar, cachear, texturizar ou reconstruir. Ainda nao
    /// ha tela que a chame — o modo Google Preview esta desligado por padrao e
    /// exige chave do proprio usuario.
    #[allow(dead_code)]
    pub fn gerar_embed_google_street_view(lat: f64, lon: f64, heading: f64, pitch: f64) -> String {
        format!(
            "https://www.google.com/maps/embed/v1/streetview?key=EMBED_ONLY&location={:.6},{:.6}&heading={:.1}&pitch={:.1}",
            lat, lon, heading, pitch
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converte_panoramas_em_nos_autoritativos_com_confianca_verde() {
        let dir = std::env::temp_dir().join(format!("arcz-pano-{}", std::process::id()));
        let worker = StreetViewWorker::novo(&dir);

        let item = PanoramaItem {
            id: "px_12345".to_string(),
            provider: PanoramaProvider::Panoramax,
            georeference: Georeference64 {
                latitude: -27.1544,
                longitude: -48.5022,
                altitude: 2.0,
                heading: 180.0,
            },
            image_url: Some("https://panoramax.ign.fr/api/v1/items/px_12345/sd.jpg".to_string()),
            embed_url: None,
            capture_date: Some("2026-01-15".to_string()),
            license: "CC-BY-SA-4.0".to_string(),
        };

        let result = worker.processar_panoramas(vec![item]);

        assert_eq!(result.nodes.len(), 1);
        assert_eq!(result.nodes[0].confidence, NodeConfidence::Observed);
        assert_eq!(result.nodes[0].confidence.color_code(), "GREEN");
        assert_eq!(result.nodes[0].node_type, NodeType::Panorama);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[should_panic(expected = "VIOLAÇÃO DE POLÍTICA")]
    fn recusa_raspagem_de_imagem_do_google_street_view() {
        let dir = std::env::temp_dir().join(format!("arcz-pano-gsv-{}", std::process::id()));
        let worker = StreetViewWorker::novo(&dir);

        let item_ilegal = PanoramaItem {
            id: "gsv_illegal".to_string(),
            provider: PanoramaProvider::GoogleStreetViewEmbedOnly,
            georeference: Georeference64 {
                latitude: -27.1544,
                longitude: -48.5022,
                altitude: 2.0,
                heading: 0.0,
            },
            image_url: Some("https://illegal-scrape.com/tile.jpg".to_string()), // PROIBIDO!
            embed_url: Some("https://google.com/maps/embed".to_string()),
            capture_date: None,
            license: "Restricted".to_string(),
        };

        let _ = worker.processar_panoramas(vec![item_ilegal]);
    }
}

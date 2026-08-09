//! Cliente da Overpass API com cache em disco.
//!
//! A Overpass e um servico publico voluntario com politica de uso justo: pedir a
//! mesma bbox duas vezes e desperdicio e caminho rapido para o `429`. Por isso o
//! resultado cru vai para disco com a chave derivada da propria consulta, e a
//! segunda chamada nunca sai da maquina.
//!
//! A consulta usa `out geom`, que devolve a geometria embutida em cada `way`.
//! A alternativa (`out body` + resolver `refs` contra os `node`) exige montar um
//! indice de nos e trata rela\u{e7}\u{f5}es a mao — mais codigo para o mesmo resultado.

use std::path::{Path, PathBuf};
use std::time::Duration;

use arcz_geo::GeoBBox;
use sha2::{Digest, Sha256};

use crate::entidade::*;

/// Espelhos publicos, na ordem de tentativa. Quando um responde `429` ou cai, o
/// proximo assume — um unico espelho fora do ar nao pode parar o app.
pub const ESPELHOS: &[&str] = &[
    "https://overpass-api.de/api/interpreter",
    "https://overpass.kumi.systems/api/interpreter",
    "https://maps.mail.ru/osm/tools/overpass/api/interpreter",
];

/// Atribuicao exigida pela ODbL. Precisa aparecer em qualquer imagem publicada.
pub const ATRIBUICAO: &str = "\u{a9} colaboradores do OpenStreetMap";

#[derive(Debug, thiserror::Error)]
pub enum OsmError {
    #[error("todos os espelhos Overpass falharam; ultimo erro: {0}")]
    TodosOsEspelhosFalharam(String),
    #[error("resposta da Overpass nao e JSON valido: {0}")]
    JsonInvalido(String),
    #[error("erro de disco no cache: {0}")]
    Disco(#[from] std::io::Error),
    #[error("area pedida e grande demais: {0:.1} km2 (limite {1:.0} km2)")]
    AreaGrandeDemais(f64, f64),
}

/// Teto de area por consulta. Acima disto a Overpass tende a estourar o timeout
/// e devolver erro depois de 3 minutos esperando — melhor recusar na hora.
pub const AREA_MAXIMA_KM2: f64 = 25.0;

/// O que buscar. Separado da bbox porque o usuario pode querer so os predios,
/// ou so as ruas, e cada camada custa tempo de servidor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Camadas {
    pub edificios: bool,
    pub vias: bool,
    pub superficies: bool,
    pub arvores: bool,
}

impl Default for Camadas {
    fn default() -> Self {
        Self {
            edificios: true,
            vias: true,
            superficies: true,
            arvores: true,
        }
    }
}

impl Camadas {
    pub fn apenas_edificios() -> Self {
        Self {
            edificios: true,
            vias: false,
            superficies: false,
            arvores: false,
        }
    }

    pub fn nenhuma(&self) -> bool {
        !self.edificios && !self.vias && !self.superficies && !self.arvores
    }
}

/// Monta a consulta Overpass QL para a bbox.
///
/// A ordem dos parametros da bbox no Overpass e `(sul,oeste,norte,leste)` — o
/// contrario da convencao `(oeste,sul,leste,norte)` do `GeoBBox`. Trocar isso
/// devolve dados do lugar errado sem nenhum erro, entao ha teste guardando.
pub fn montar_consulta(bbox: &GeoBBox, camadas: Camadas) -> String {
    let b = format!(
        "{:.7},{:.7},{:.7},{:.7}",
        bbox.south, bbox.west, bbox.north, bbox.east
    );

    let mut corpo = String::new();
    if camadas.edificios {
        corpo.push_str(&format!("  way[\"building\"]({b});\n"));
        corpo.push_str(&format!("  way[\"building:part\"]({b});\n"));
    }
    if camadas.vias {
        corpo.push_str(&format!("  way[\"highway\"]({b});\n"));
    }
    if camadas.superficies {
        corpo.push_str(&format!(
            "  way[\"natural\"~\"^(water|wood|scrub|beach|sand)$\"]({b});\n"
        ));
        corpo.push_str(&format!(
            "  way[\"landuse\"~\"^(forest|grass|meadow|village_green|recreation_ground)$\"]({b});\n"
        ));
        corpo.push_str(&format!(
            "  way[\"leisure\"~\"^(park|garden|pitch)$\"]({b});\n"
        ));
        corpo.push_str(&format!("  way[\"waterway\"=\"riverbank\"]({b});\n"));
    }
    if camadas.arvores {
        corpo.push_str(&format!("  node[\"natural\"=\"tree\"]({b});\n"));
    }

    format!("[out:json][timeout:90];\n(\n{corpo});\nout geom;\n")
}

/// Elemento cru da resposta Overpass.
#[derive(Debug, Clone, serde::Deserialize)]
struct Elemento {
    #[serde(rename = "type")]
    tipo: String,
    id: i64,
    #[serde(default)]
    lat: Option<f64>,
    #[serde(default)]
    lon: Option<f64>,
    #[serde(default)]
    geometry: Vec<PontoGeo>,
    #[serde(default)]
    tags: Tags,
}

#[derive(Debug, serde::Deserialize)]
struct Resposta {
    #[serde(default)]
    elements: Vec<Elemento>,
}

/// Traduz a resposta crua da Overpass no `Entorno` normalizado.
pub fn interpretar(json: &str) -> Result<Entorno, OsmError> {
    let r: Resposta =
        serde_json::from_str(json).map_err(|e| OsmError::JsonInvalido(e.to_string()))?;

    let mut entorno = Entorno::default();

    for el in r.elements {
        match el.tipo.as_str() {
            "node" => {
                if el.tags.get("natural").map(String::as_str) == Some("tree") {
                    if let (Some(lat), Some(lon)) = (el.lat, el.lon) {
                        let altura = el
                            .tags
                            .get("height")
                            .and_then(|s| metros_da_tag(s))
                            .unwrap_or(9.0);
                        entorno.arvores.push(Arvore {
                            posicao: PontoGeo { lat, lon },
                            altura_m: altura,
                            raio_copa_m: el
                                .tags
                                .get("diameter_crown")
                                .and_then(|s| metros_da_tag(s))
                                .map(|d| d / 2.0)
                                .unwrap_or(altura * 0.32),
                            // Sem `Math.random` disponivel e sem querer estado
                            // global: o proprio id do no ja e uma fonte estavel
                            // de variacao, e estavel importa (reabrir o projeto
                            // nao pode girar as arvores todas).
                            giro_rad: giro_estavel(el.id),
                            mapeada: true,
                        });
                    }
                }
            }
            "way" => {
                if el.geometry.len() < 2 {
                    continue;
                }
                classificar_way(&el, &mut entorno);
            }
            _ => {}
        }
    }

    Ok(entorno)
}

fn classificar_way(el: &Elemento, entorno: &mut Entorno) {
    let nome = el.tags.get("name").cloned();

    // Predio. `building=no` existe e significa explicitamente "nao e predio".
    if let Some(v) = el
        .tags
        .get("building")
        .or_else(|| el.tags.get("building:part"))
    {
        if v != "no" {
            if let Some(contorno) = fechar_anel(&el.geometry) {
                let classe = ClasseEdificio::da_tag(v);
                let (altura_m, base_m, fonte_altura) = altura_do_edificio(&el.tags, classe);
                entorno.edificios.push(Edificio {
                    id: el.id,
                    nome,
                    classe,
                    contorno,
                    altura_m,
                    base_m,
                    fonte_altura,
                    telhado: Telhado::tipico(classe, altura_m / ALTURA_PAVIMENTO_M),
                    cor_parede: None,
                    cor_telhado: None,
                });
            }
            return;
        }
    }

    // Via.
    if let Some(v) = el.tags.get("highway") {
        if let Some(classe) = ClasseVia::da_tag(v) {
            entorno.vias.push(Via {
                id: el.id,
                nome,
                classe,
                largura_m: largura_da_via(&el.tags, classe),
                eixo: el.geometry.clone(),
            });
        }
        return;
    }

    // Superficie de uso do solo.
    let classe = el
        .tags
        .get("natural")
        .and_then(|v| match v.as_str() {
            "water" => Some(ClasseSuperficie::Agua),
            "wood" | "scrub" => Some(ClasseSuperficie::Mata),
            "beach" | "sand" => Some(ClasseSuperficie::Praia),
            _ => None,
        })
        .or_else(|| {
            el.tags.get("landuse").and_then(|v| match v.as_str() {
                "forest" => Some(ClasseSuperficie::Mata),
                "grass" | "meadow" | "village_green" | "recreation_ground" => {
                    Some(ClasseSuperficie::Grama)
                }
                _ => None,
            })
        })
        .or_else(|| {
            el.tags.get("leisure").and_then(|v| match v.as_str() {
                "park" | "garden" => Some(ClasseSuperficie::Grama),
                "pitch" => Some(ClasseSuperficie::Quadra),
                _ => None,
            })
        })
        .or_else(|| {
            (el.tags.get("waterway").map(String::as_str) == Some("riverbank"))
                .then_some(ClasseSuperficie::Agua)
        });

    if let Some(classe) = classe {
        if let Some(contorno) = fechar_anel(&el.geometry) {
            entorno.superficies.push(Superficie {
                id: el.id,
                classe,
                contorno,
            });
        }
    }
}

/// Normaliza um anel: exige que seja fechado no OSM e devolve **sem** o ponto
/// repetido, que e o que a triangulacao espera.
fn fechar_anel(g: &[PontoGeo]) -> Option<Vec<PontoGeo>> {
    if g.len() < 4 {
        return None;
    }
    let primeiro = g[0];
    let ultimo = g[g.len() - 1];
    // Um way de predio no OSM e sempre fechado. Se nao for, e um pedaco de
    // multipoligono que nao da para extrudar sozinho.
    if (primeiro.lat - ultimo.lat).abs() > 1e-9 || (primeiro.lon - ultimo.lon).abs() > 1e-9 {
        return None;
    }
    let anel: Vec<PontoGeo> = g[..g.len() - 1].to_vec();
    (anel.len() >= 3).then_some(anel)
}

/// Giro pseudoaleatorio derivado do id, estavel entre execucoes.
fn giro_estavel(id: i64) -> f64 {
    // Hash inteiro barato (splitmix64) so para espalhar bits do id.
    let mut z = (id as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z as f64 / u64::MAX as f64) * std::f64::consts::TAU
}

/// Cliente com cache.
pub struct ClienteOverpass {
    cache: PathBuf,
    espelhos: Vec<String>,
    timeout: Duration,
}

impl ClienteOverpass {
    pub fn novo(cache: impl AsRef<Path>) -> Self {
        Self {
            cache: cache.as_ref().to_path_buf(),
            espelhos: ESPELHOS.iter().map(|s| s.to_string()).collect(),
            timeout: Duration::from_secs(120),
        }
    }

    /// Caminho do arquivo de cache para uma consulta.
    pub fn caminho_cache(&self, consulta: &str) -> PathBuf {
        let mut h = Sha256::new();
        h.update(consulta.as_bytes());
        let d = h.finalize();
        self.cache.join(format!("overpass-{d:x}.json"))
    }

    /// Busca o entorno da bbox. Le do cache quando ja houver.
    pub async fn buscar(
        &self,
        bbox: &GeoBBox,
        camadas: Camadas,
    ) -> Result<(Entorno, Origem), OsmError> {
        if camadas.nenhuma() {
            return Ok((Entorno::default(), Origem::Cache));
        }
        let area = area_km2(bbox);
        if area > AREA_MAXIMA_KM2 {
            return Err(OsmError::AreaGrandeDemais(area, AREA_MAXIMA_KM2));
        }

        let consulta = montar_consulta(bbox, camadas);
        let arquivo = self.caminho_cache(&consulta);

        if let Ok(txt) = std::fs::read_to_string(&arquivo) {
            log::info!("overpass: cache {}", arquivo.display());
            return Ok((interpretar(&txt)?, Origem::Cache));
        }

        let cliente = reqwest::Client::builder()
            .timeout(self.timeout)
            // A Overpass exige User-Agent identificavel; sem isso ela bloqueia.
            .user_agent("ARCZ/0.1 (editor 3D; https://github.com/legrand/arcz)")
            .build()
            .map_err(|e| OsmError::TodosOsEspelhosFalharam(e.to_string()))?;

        let mut ultimo = String::from("nenhuma tentativa");
        for espelho in &self.espelhos {
            match cliente.post(espelho).body(consulta.clone()).send().await {
                Ok(r) if r.status().is_success() => {
                    let txt = r
                        .text()
                        .await
                        .map_err(|e| OsmError::TodosOsEspelhosFalharam(e.to_string()))?;
                    let entorno = interpretar(&txt)?;
                    if let Some(pai) = arquivo.parent() {
                        std::fs::create_dir_all(pai)?;
                    }
                    std::fs::write(&arquivo, &txt)?;
                    log::info!("overpass: {} -> {}", espelho, entorno.resumo());
                    return Ok((entorno, Origem::Rede));
                }
                Ok(r) => {
                    ultimo = format!("{espelho} respondeu {}", r.status());
                    log::warn!("overpass: {ultimo}");
                }
                Err(e) => {
                    ultimo = format!("{espelho}: {e}");
                    log::warn!("overpass: {ultimo}");
                }
            }
        }
        Err(OsmError::TodosOsEspelhosFalharam(ultimo))
    }
}

/// De onde os dados vieram nesta chamada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origem {
    Cache,
    Rede,
}

/// Area aproximada da bbox em km2.
pub fn area_km2(b: &GeoBBox) -> f64 {
    let lat_media = (b.north + b.south) * 0.5;
    let m_por_grau_lat = 111_132.0;
    let m_por_grau_lon = 111_320.0 * lat_media.to_radians().cos();
    let h = (b.north - b.south) * m_por_grau_lat;
    let w = (b.east - b.west) * m_por_grau_lon;
    (w * h).abs() / 1e6
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bbox_bombinhas() -> GeoBBox {
        GeoBBox::new(-48.5060, -27.1580, -48.4985, -27.1510).unwrap()
    }

    #[test]
    fn a_consulta_usa_a_ordem_sul_oeste_norte_leste() {
        // O Overpass inverte a convencao do GeoBBox. Trocar produz dados do
        // lugar errado silenciosamente, e o app so mostra um mapa vazio.
        let q = montar_consulta(&bbox_bombinhas(), Camadas::apenas_edificios());
        assert!(
            q.contains("(-27.1580000,-48.5060000,-27.1510000,-48.4985000)"),
            "ordem da bbox errada em:\n{q}"
        );
    }

    #[test]
    fn as_camadas_desligadas_nao_entram_na_consulta() {
        let q = montar_consulta(&bbox_bombinhas(), Camadas::apenas_edificios());
        assert!(q.contains("\"building\""));
        assert!(!q.contains("\"highway\""), "pediu via sem precisar:\n{q}");
        assert!(!q.contains("natural\"=\"tree"), "pediu arvore sem precisar");

        let q = montar_consulta(&bbox_bombinhas(), Camadas::default());
        for esperado in ["\"building\"", "\"highway\"", "landuse", "natural\"=\"tree"] {
            assert!(q.contains(esperado), "faltou {esperado} em:\n{q}");
        }
    }

    #[test]
    fn interpreta_predio_com_altura_e_via_com_faixas() {
        let json = r#"{"elements":[
          {"type":"way","id":1,"tags":{"building":"apartments","building:levels":"8","name":"Zenite"},
           "geometry":[{"lat":-27.1545,"lon":-48.5023},{"lat":-27.1545,"lon":-48.5021},
                       {"lat":-27.1543,"lon":-48.5021},{"lat":-27.1543,"lon":-48.5023},
                       {"lat":-27.1545,"lon":-48.5023}]},
          {"type":"way","id":2,"tags":{"highway":"residential","lanes":"2","name":"R. Onca Pintada"},
           "geometry":[{"lat":-27.1546,"lon":-48.5030},{"lat":-27.1546,"lon":-48.5010}]},
          {"type":"node","id":3,"lat":-27.1544,"lon":-48.5025,"tags":{"natural":"tree"}}
        ]}"#;
        let e = interpretar(json).unwrap();

        assert_eq!(e.edificios.len(), 1);
        let p = &e.edificios[0];
        assert_eq!(p.nome.as_deref(), Some("Zenite"));
        assert_eq!(p.classe, ClasseEdificio::Apartamentos);
        assert_eq!(p.altura_m, 24.0);
        // O anel volta sem o ponto repetido: 5 pontos no JSON, 4 no contorno.
        assert_eq!(p.contorno.len(), 4);

        assert_eq!(e.vias.len(), 1);
        assert_eq!(e.vias[0].largura_m, 6.4);

        assert_eq!(e.arvores.len(), 1);
        assert!(e.arvores[0].mapeada);
    }

    #[test]
    fn building_no_nao_vira_predio() {
        // `building=no` e uma negacao explicita no OSM. Extrudar isso planta
        // caixas onde o mapeador disse que nao ha nada.
        let json = r#"{"elements":[
          {"type":"way","id":1,"tags":{"building":"no"},
           "geometry":[{"lat":0,"lon":0},{"lat":0,"lon":0.001},
                       {"lat":0.001,"lon":0.001},{"lat":0,"lon":0}]}]}"#;
        assert!(interpretar(json).unwrap().edificios.is_empty());
    }

    #[test]
    fn anel_aberto_e_descartado() {
        // Pedaco de multipoligono: extrudar geraria uma parede solta no ar.
        let json = r#"{"elements":[
          {"type":"way","id":1,"tags":{"building":"yes"},
           "geometry":[{"lat":0,"lon":0},{"lat":0,"lon":0.001},{"lat":0.001,"lon":0.001},
                       {"lat":0.002,"lon":0.004}]}]}"#;
        assert!(interpretar(json).unwrap().edificios.is_empty());
    }

    #[test]
    fn superficies_saem_das_tres_familias_de_tag() {
        let anel = r#""geometry":[{"lat":0,"lon":0},{"lat":0,"lon":0.001},
                     {"lat":0.001,"lon":0.001},{"lat":0,"lon":0}]"#;
        let json = format!(
            r#"{{"elements":[
              {{"type":"way","id":1,"tags":{{"natural":"water"}},{anel}}},
              {{"type":"way","id":2,"tags":{{"landuse":"forest"}},{anel}}},
              {{"type":"way","id":3,"tags":{{"leisure":"park"}},{anel}}}]}}"#
        );
        let e = interpretar(&json).unwrap();
        let classes: Vec<_> = e.superficies.iter().map(|s| s.classe).collect();
        assert_eq!(
            classes,
            vec![
                ClasseSuperficie::Agua,
                ClasseSuperficie::Mata,
                ClasseSuperficie::Grama
            ]
        );
    }

    #[test]
    fn o_giro_da_arvore_e_estavel_entre_execucoes() {
        // Reabrir o projeto nao pode girar a floresta inteira.
        assert_eq!(giro_estavel(12345), giro_estavel(12345));
        assert_ne!(giro_estavel(12345), giro_estavel(12346));
        for id in [1, 2, 999, -40, i64::MAX] {
            let g = giro_estavel(id);
            assert!(
                (0.0..=std::f64::consts::TAU).contains(&g),
                "giro {g} fora de faixa"
            );
        }
    }

    #[test]
    fn a_chave_do_cache_muda_com_a_consulta() {
        let c = ClienteOverpass::novo("cache");
        let a = c.caminho_cache(&montar_consulta(&bbox_bombinhas(), Camadas::default()));
        let b = c.caminho_cache(&montar_consulta(
            &bbox_bombinhas(),
            Camadas::apenas_edificios(),
        ));
        assert_ne!(a, b, "camadas diferentes reusariam o mesmo arquivo");
    }

    #[test]
    fn area_grande_demais_e_recusada_antes_da_rede() {
        // 1 grau ~ 111 km de lado: 12 mil km2, muito acima do teto.
        let enorme = GeoBBox::new(-49.0, -28.0, -48.0, -27.0).unwrap();
        assert!(area_km2(&enorme) > AREA_MAXIMA_KM2);
        assert!(area_km2(&bbox_bombinhas()) < AREA_MAXIMA_KM2);
    }

    #[test]
    fn json_invalido_vira_erro_e_nao_panico() {
        assert!(matches!(
            interpretar("{nao e json"),
            Err(OsmError::JsonInvalido(_))
        ));
    }

    #[test]
    fn resposta_vazia_e_valida() {
        // Area sem mapeamento no OSM e um caso normal, nao um erro.
        let e = interpretar(r#"{"elements":[]}"#).unwrap();
        assert!(e.vazio());
    }
}

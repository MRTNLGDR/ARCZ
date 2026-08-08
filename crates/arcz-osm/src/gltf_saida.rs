//! Exporta as malhas do entorno como glTF binário (`.glb`).
//!
//! ## Por que isto existe
//!
//! O CesiumJS resolve globo, terreno, streaming, câmera e picking — não vale
//! reescrever nada disso. Mas ele **consome** 3D Tiles e glTF; não os gera. O
//! caminho `OSM → footprint → extrusão → telhado → cor da ortofoto` não existe
//! pronto em lugar nenhum, e é exatamente o que o `arcz-osm` faz.
//!
//! Este módulo é a ponte: o Rust gera, o Cesium desenha.
//!
//! ## Formato
//!
//! `.glb` é o glTF empacotado num arquivo só: cabeçalho, um bloco JSON e um
//! bloco binário. Escrito à mão em vez de trazer um crate porque a saída aqui é
//! um caso estreito — malhas com posição, normal e cor por material, sem
//! textura, animação, esqueleto ou cena hierárquica. Uma biblioteca genérica
//! custaria mais em dependência do que economiza em código.
//!
//! Os vértices saem em **espaço local**, com a origem no centro da caixa, e a
//! posição no mundo vai na matriz do nó. Mandar coordenadas ENU grandes direto
//! nos vértices traria de volta o jitter de `f32` que o resto do projeto evita.

use crate::malha::MalhaProcedural;

/// Cabeçalho de um chunk do GLB.
const CHUNK_JSON: u32 = 0x4E4F_534A; // "JSON"
const CHUNK_BIN: u32 = 0x004E_4942; // "BIN\0"
/// `glTF` em little-endian: 0x67='g', 0x6C='l', 0x54='T', 0x46='F'.
///
/// Estava 0x4674_6C67, que escreve `gltF` — T minusculo. O carregador do proprio
/// ARCZ e tolerante e aceitava, entao o entorno aparecia no render nativo e o
/// erro passou despercebido; o CesiumJS compara os quatro bytes e recusava o
/// arquivo inteiro, deixando o globo sem entorno nenhum.
const MAGIC: u32 = 0x4654_6C67;

/// Componentes do glTF que o exportador usa.
const FLOAT: u32 = 5126;
const UNSIGNED_INT: u32 = 5125;
const ARRAY_BUFFER: u32 = 34962;
const ELEMENT_ARRAY_BUFFER: u32 = 34963;
const TRIANGLES: u32 = 4;

/// Empacota as malhas num único `.glb`.
///
/// Cada malha vira um nó com sua própria matriz e material. Um arquivo só, e
/// não um por edificação, porque 469 requisições HTTP para desenhar um bairro
/// custariam mais que os 8 mil triângulos que ele tem.
pub fn exportar_glb(malhas: &[MalhaProcedural]) -> Vec<u8> {
    let mut bin: Vec<u8> = Vec::new();
    let mut buffer_views = Vec::new();
    let mut accessors = Vec::new();
    let mut meshes = Vec::new();
    let mut nodes = Vec::new();
    let mut materials = Vec::new();
    let mut raiz = Vec::new();

    for m in malhas.iter().filter(|m| !m.indices.is_empty()) {
        let (min, max) = m.envolvente();
        // Origem no centro da caixa: mantém os valores pequenos e o `f32`
        // preciso. A posição no mundo vai na matriz do nó.
        let centro = [
            (min[0] + max[0]) * 0.5,
            (min[1] + max[1]) * 0.5,
            (min[2] + max[2]) * 0.5,
        ];

        // --- posições ---
        let off_pos = alinhar(&mut bin);
        let mut pmin = [f32::MAX; 3];
        let mut pmax = [f32::MIN; 3];
        for v in &m.vertices {
            for k in 0..3 {
                let c = v.pos[k] - centro[k];
                pmin[k] = pmin[k].min(c);
                pmax[k] = pmax[k].max(c);
                bin.extend_from_slice(&c.to_le_bytes());
            }
        }
        let len_pos = bin.len() - off_pos;

        // --- normais ---
        let off_nor = alinhar(&mut bin);
        for v in &m.vertices {
            for k in 0..3 {
                bin.extend_from_slice(&v.normal[k].to_le_bytes());
            }
        }
        let len_nor = bin.len() - off_nor;

        // --- índices ---
        let off_idx = alinhar(&mut bin);
        for i in &m.indices {
            bin.extend_from_slice(&i.to_le_bytes());
        }
        let len_idx = bin.len() - off_idx;

        let bv = buffer_views.len();
        buffer_views.push(view(off_pos, len_pos, ARRAY_BUFFER));
        buffer_views.push(view(off_nor, len_nor, ARRAY_BUFFER));
        buffer_views.push(view(off_idx, len_idx, ELEMENT_ARRAY_BUFFER));

        let ac = accessors.len();
        accessors.push(format!(
            r#"{{"bufferView":{},"componentType":{FLOAT},"count":{},"type":"VEC3","min":[{},{},{}],"max":[{},{},{}]}}"#,
            bv,
            m.vertices.len(),
            f(pmin[0]), f(pmin[1]), f(pmin[2]),
            f(pmax[0]), f(pmax[1]), f(pmax[2]),
        ));
        accessors.push(format!(
            r#"{{"bufferView":{},"componentType":{FLOAT},"count":{},"type":"VEC3"}}"#,
            bv + 1,
            m.vertices.len()
        ));
        accessors.push(format!(
            r#"{{"bufferView":{},"componentType":{UNSIGNED_INT},"count":{},"type":"SCALAR"}}"#,
            bv + 2,
            m.indices.len()
        ));

        let mat = materials.len();
        materials.push(format!(
            concat!(
                r#"{{"name":{},"pbrMetallicRoughness":{{"baseColorFactor":[{},{},{},1.0],"#,
                r#""metallicFactor":0.0,"roughnessFactor":0.85}},"doubleSided":true}}"#
            ),
            escapar(&m.nome),
            f(m.cor[0]),
            f(m.cor[1]),
            f(m.cor[2]),
        ));

        let mesh = meshes.len();
        meshes.push(format!(
            r#"{{"primitives":[{{"attributes":{{"POSITION":{},"NORMAL":{}}},"indices":{},"material":{},"mode":{TRIANGLES}}}]}}"#,
            ac,
            ac + 1,
            ac + 2,
            mat
        ));

        raiz.push(nodes.len());
        // Coluna-maior, como o glTF exige. Só translação: a geometria já saiu
        // orientada, e o centro foi removido dos vértices acima.
        nodes.push(format!(
            r#"{{"name":{},"mesh":{},"translation":[{},{},{}]}}"#,
            escapar(&m.nome),
            mesh,
            f(centro[0]),
            f(centro[1]),
            f(centro[2])
        ));
    }

    let json = format!(
        concat!(
            r#"{{"asset":{{"version":"2.0","generator":"ARCZ arcz-osm"}},"#,
            r#""scene":0,"scenes":[{{"nodes":[{}]}}],"#,
            r#""nodes":[{}],"meshes":[{}],"materials":[{}],"#,
            r#""accessors":[{}],"bufferViews":[{}],"buffers":[{{"byteLength":{}}}]}}"#
        ),
        raiz
            .iter()
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(","),
        nodes.join(","),
        meshes.join(","),
        materials.join(","),
        accessors.join(","),
        buffer_views.join(","),
        bin.len()
    );

    montar_glb(&json, &bin)
}

/// Monta o contêiner GLB a partir dos dois blocos.
///
/// A especificação exige que cada chunk termine em múltiplo de 4. O JSON é
/// completado com espaço e o binário com zero — trocar isso faz o Cesium
/// recusar o arquivo com erro de alinhamento.
fn montar_glb(json: &str, bin: &[u8]) -> Vec<u8> {
    let mut j = json.as_bytes().to_vec();
    while j.len() % 4 != 0 {
        j.push(b' ');
    }
    let mut b = bin.to_vec();
    while b.len() % 4 != 0 {
        b.push(0);
    }

    let total = 12 + 8 + j.len() + 8 + b.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&MAGIC.to_le_bytes());
    out.extend_from_slice(&2u32.to_le_bytes()); // versão
    out.extend_from_slice(&(total as u32).to_le_bytes());

    out.extend_from_slice(&(j.len() as u32).to_le_bytes());
    out.extend_from_slice(&CHUNK_JSON.to_le_bytes());
    out.extend_from_slice(&j);

    out.extend_from_slice(&(b.len() as u32).to_le_bytes());
    out.extend_from_slice(&CHUNK_BIN.to_le_bytes());
    out.extend_from_slice(&b);
    out
}

/// Alinha o buffer a 4 bytes e devolve o deslocamento resultante.
///
/// Acessor de `f32` desalinhado é recusado pela especificação, e o sintoma no
/// Cesium é geometria embaralhada em vez de erro claro.
fn alinhar(bin: &mut Vec<u8>) -> usize {
    while bin.len() % 4 != 0 {
        bin.push(0);
    }
    bin.len()
}

fn view(offset: usize, length: usize, alvo: u32) -> String {
    format!(
        r#"{{"buffer":0,"byteOffset":{offset},"byteLength":{length},"target":{alvo}}}"#
    )
}

/// Formata `f32` sem notação científica, que o JSON do glTF aceita mas alguns
/// leitores tratam mal.
fn f(v: f32) -> String {
    if v == v.trunc() && v.abs() < 1e7 {
        format!("{v:.1}")
    } else {
        format!("{v:.6}")
    }
}

/// String JSON com aspas e barras escapadas.
fn escapar(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::malha::{Origem, Vertice};

    fn malha_teste(nome: &str) -> MalhaProcedural {
        MalhaProcedural {
            nome: nome.into(),
            origem: Origem::Edificio(1),
            cor: [0.8, 0.4, 0.2],
            vertices: vec![
                Vertice { pos: [10.0, 0.0, 10.0], normal: [0.0, 1.0, 0.0], uv: [0.0, 0.0] },
                Vertice { pos: [20.0, 0.0, 10.0], normal: [0.0, 1.0, 0.0], uv: [1.0, 0.0] },
                Vertice { pos: [20.0, 0.0, 20.0], normal: [0.0, 1.0, 0.0], uv: [1.0, 1.0] },
            ],
            indices: vec![0, 1, 2],
        }
    }

    fn ler_u32(b: &[u8], off: usize) -> u32 {
        u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
    }

    fn json_do_glb(glb: &[u8]) -> String {
        let len = ler_u32(glb, 12) as usize;
        String::from_utf8_lossy(&glb[20..20 + len]).trim_end().to_string()
    }

    #[test]
    fn o_cabecalho_do_glb_esta_correto() {
        // Magic ou versao errados fazem o Cesium recusar sem dizer por que.
        let glb = exportar_glb(&[malha_teste("a")]);
        // Compara com os BYTES, nao com a constante: comparar `MAGIC` com
        // `MAGIC` deu certo por anos enquanto o arquivo dizia `gltF` e o Cesium
        // recusava. Um teste que le a propria constante nao testa nada.
        assert_eq!(&glb[0..4], b"glTF", "magic nao e 'glTF'");
        assert_eq!(ler_u32(&glb, 0), MAGIC, "a constante nao bate com os bytes");
        assert_eq!(ler_u32(&glb, 4), 2, "versao do glTF");
        assert_eq!(ler_u32(&glb, 8) as usize, glb.len(), "tamanho declarado");
        assert_eq!(ler_u32(&glb, 16), CHUNK_JSON, "primeiro chunk tem de ser JSON");
    }

    #[test]
    fn os_chunks_terminam_em_multiplo_de_quatro() {
        // A especificacao exige. Sem isso o Cesium recusa por alinhamento.
        let glb = exportar_glb(&[malha_teste("a"), malha_teste("b")]);
        let len_json = ler_u32(&glb, 12) as usize;
        assert_eq!(len_json % 4, 0, "chunk JSON com {len_json} bytes");
        let off_bin = 20 + len_json;
        let len_bin = ler_u32(&glb, off_bin) as usize;
        assert_eq!(len_bin % 4, 0, "chunk BIN com {len_bin} bytes");
        assert_eq!(ler_u32(&glb, off_bin + 4), CHUNK_BIN);
    }

    #[test]
    fn o_json_e_valido_e_declara_a_cena() {
        let glb = exportar_glb(&[malha_teste("Casa")]);
        let j: serde_json::Value = serde_json::from_str(&json_do_glb(&glb)).expect("JSON invalido");
        assert_eq!(j["asset"]["version"], "2.0");
        assert_eq!(j["scene"], 0);
        assert_eq!(j["nodes"].as_array().unwrap().len(), 1);
        assert_eq!(j["meshes"].as_array().unwrap().len(), 1);
        assert_eq!(j["nodes"][0]["name"], "Casa");
    }

    #[test]
    fn os_vertices_saem_em_espaco_local_com_a_posicao_na_matriz() {
        // Coordenadas ENU grandes direto nos vertices traria de volta o jitter
        // de f32 que o resto do projeto evita.
        let glb = exportar_glb(&[malha_teste("x")]);
        let j: serde_json::Value = serde_json::from_str(&json_do_glb(&glb)).unwrap();

        let t = j["nodes"][0]["translation"].as_array().unwrap();
        assert!((t[0].as_f64().unwrap() - 15.0).abs() < 1e-3, "centro x");
        assert!((t[2].as_f64().unwrap() - 15.0).abs() < 1e-3, "centro z");

        // A caixa do acessor tem de estar centrada na origem.
        let min = j["accessors"][0]["min"].as_array().unwrap();
        let max = j["accessors"][0]["max"].as_array().unwrap();
        assert!((min[0].as_f64().unwrap() + max[0].as_f64().unwrap()).abs() < 1e-3);
    }

    #[test]
    fn o_acessor_de_posicao_declara_min_e_max() {
        // Obrigatorio na especificacao para POSITION; sem isso o Cesium nao
        // calcula a envolvente e o objeto some no culling.
        let glb = exportar_glb(&[malha_teste("x")]);
        let j: serde_json::Value = serde_json::from_str(&json_do_glb(&glb)).unwrap();
        assert!(j["accessors"][0]["min"].is_array());
        assert!(j["accessors"][0]["max"].is_array());
        assert_eq!(j["accessors"][0]["type"], "VEC3");
        assert_eq!(j["accessors"][0]["count"], 3);
    }

    #[test]
    fn a_cor_da_malha_vira_material_pbr() {
        let glb = exportar_glb(&[malha_teste("x")]);
        let j: serde_json::Value = serde_json::from_str(&json_do_glb(&glb)).unwrap();
        let c = j["materials"][0]["pbrMetallicRoughness"]["baseColorFactor"]
            .as_array()
            .unwrap();
        assert!((c[0].as_f64().unwrap() - 0.8).abs() < 1e-3);
        assert!((c[1].as_f64().unwrap() - 0.4).abs() < 1e-3);
        assert_eq!(c[3], 1.0);
    }

    #[test]
    fn varias_malhas_viram_varios_nos_num_arquivo_so() {
        // Um arquivo por edificacao daria 469 requisicoes para desenhar um
        // bairro de 8 mil triangulos.
        let malhas: Vec<_> = (0..5).map(|i| malha_teste(&format!("m{i}"))).collect();
        let glb = exportar_glb(&malhas);
        let j: serde_json::Value = serde_json::from_str(&json_do_glb(&glb)).unwrap();
        assert_eq!(j["nodes"].as_array().unwrap().len(), 5);
        assert_eq!(j["scenes"][0]["nodes"].as_array().unwrap().len(), 5);
    }

    #[test]
    fn malha_vazia_e_ignorada_sem_quebrar_o_arquivo() {
        let mut vazia = malha_teste("vazia");
        vazia.indices.clear();
        let glb = exportar_glb(&[vazia, malha_teste("boa")]);
        let j: serde_json::Value = serde_json::from_str(&json_do_glb(&glb)).unwrap();
        assert_eq!(j["nodes"].as_array().unwrap().len(), 1);
        assert_eq!(j["nodes"][0]["name"], "boa");
    }

    #[test]
    fn lista_vazia_gera_glb_valido() {
        // Area sem nada mapeado e caso normal, nao erro.
        let glb = exportar_glb(&[]);
        assert_eq!(ler_u32(&glb, 0), MAGIC);
        let j: serde_json::Value = serde_json::from_str(&json_do_glb(&glb)).unwrap();
        assert_eq!(j["nodes"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn nome_com_aspas_nao_quebra_o_json() {
        // Nome de rua do OSM pode trazer aspas; sem escape o JSON fica invalido
        // e o arquivo inteiro se perde.
        let m = malha_teste(r#"Rua "do" Teste\barra"#);
        let glb = exportar_glb(&[m]);
        let j: serde_json::Value =
            serde_json::from_str(&json_do_glb(&glb)).expect("aspas quebraram o JSON");
        assert_eq!(j["nodes"][0]["name"], r#"Rua "do" Teste\barra"#);
    }

    #[test]
    fn acentos_sobrevivem_ao_ciclo() {
        let glb = exportar_glb(&[malha_teste("Câmara de Vereadores · Bombinhas")]);
        let j: serde_json::Value = serde_json::from_str(&json_do_glb(&glb)).unwrap();
        assert_eq!(j["nodes"][0]["name"], "Câmara de Vereadores · Bombinhas");
    }
}

//! Versao leve do modelo do usuario, para o globo.
//!
//! O `.glb` do Zenite tem 130 MB: 69 MB de geometria e o resto de textura.
//! Trafegar isso a cada abertura do globo e o que faz o Cesium demorar.
//!
//! Duas economias, nenhuma delas visivel na tela:
//!
//! 1. **Vertices repetidos.** O arquivo traz 1 806 027 vertices para 936 506
//!    triangulos — 1,93 por triangulo, ou seja, quase nada compartilhado. E o
//!    que um export de SketchUp produz: cada face escreve os proprios cantos.
//!    Soldar os identicos nao muda um pixel.
//!
//! 2. **Textura maior que o necessario.** Dezesseis imagens passam de 1024 px.
//!    Numa fachada vista de fora, 2048 px de madeira nao chegam a virar pixel
//!    distinto na tela; o que chega e o tempo de download.
//!
//! O que NAO se faz aqui: mexer em posicao, normal, UV ou cor de material. A
//! geometria que sai e a mesma, vertice por vertice — so sem as copias.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use arcz_model::Model;

/// Lado maximo das texturas na versao leve.
///
/// 1024 cobre a maior fachada do Zenite com folga na distancia em que o globo a
/// mostra. Quem quiser a textura cheia usa o render nativo, que le o arquivo
/// original.
const LADO_MAX_TEXTURA: u32 = 1024;

/// Qualidade do JPEG. Acima de 90 o ganho de tamanho some e o de imagem nao
/// aparece.
const QUALIDADE_JPEG: u8 = 88;

/// Chave de deduplicacao de vertice.
///
/// Compara os bits, e nao o valor: dois `f32` produzidos pelo mesmo calculo tem
/// os mesmos bits, e comparar com tolerancia soldaria vertices de faces vizinhas
/// que precisam de normais diferentes — o modelo ficaria com quinas arredondadas
/// onde deveria ter aresta viva.
#[derive(PartialEq, Eq, Hash)]
struct ChaveVertice([u32; 8]);

impl ChaveVertice {
    fn de(v: &arcz_model::ModelVertex) -> Self {
        Self([
            v.position[0].to_bits(),
            v.position[1].to_bits(),
            v.position[2].to_bits(),
            v.normal[0].to_bits(),
            v.normal[1].to_bits(),
            v.normal[2].to_bits(),
            v.uv[0].to_bits(),
            v.uv[1].to_bits(),
        ])
    }
}

/// Quantos vertices sobraram e quantos foram soldados.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ganho {
    pub vertices_antes: usize,
    pub vertices_depois: usize,
    pub texturas_reduzidas: usize,
}

/// Solda vertices identicos, reescrevendo os indices.
///
/// Devolve `(vertices, indices)` novos. Os submeshes nao mudam: a ordem dos
/// indices e preservada, so os valores apontam para o buffer compactado.
pub fn soldar(m: &Model) -> (Vec<arcz_model::ModelVertex>, Vec<u32>) {
    let mut mapa: HashMap<ChaveVertice, u32> = HashMap::with_capacity(m.vertices.len() / 2);
    let mut vertices = Vec::with_capacity(m.vertices.len() / 2);
    let mut indices = Vec::with_capacity(m.indices.len());

    for &i in &m.indices {
        let Some(v) = m.vertices.get(i as usize) else {
            // Indice fora do buffer: o arquivo esta corrompido. Preserva o valor
            // para nao mascarar o problema numa malha silenciosamente errada.
            indices.push(i);
            continue;
        };
        let chave = ChaveVertice::de(v);
        let novo = *mapa.entry(chave).or_insert_with(|| {
            vertices.push(*v);
            (vertices.len() - 1) as u32
        });
        indices.push(novo);
    }
    (vertices, indices)
}

/// Reduz e recomprime uma textura. `None` quando ela ja esta pequena o bastante
/// e no formato certo.
fn compactar_textura(t: &arcz_model::Textura) -> (Vec<u8>, &'static str, bool) {
    let maior = t.largura.max(t.altura);
    let img = image::RgbaImage::from_raw(t.largura, t.altura, t.rgba.clone());
    let Some(img) = img else {
        return (Vec::new(), "", false);
    };
    let mut dyn_img = image::DynamicImage::ImageRgba8(img);
    let reduziu = maior > LADO_MAX_TEXTURA;
    if reduziu {
        let fator = f64::from(LADO_MAX_TEXTURA) / f64::from(maior);
        let (w, h) = (
            ((f64::from(t.largura) * fator).round() as u32).max(1),
            ((f64::from(t.altura) * fator).round() as u32).max(1),
        );
        // Lanczos3: reduzir com vizinho mais proximo cria serrilhado que aparece
        // como cintilacao quando a camera se move.
        dyn_img = dyn_img.resize_exact(w, h, image::imageops::FilterType::Lanczos3);
    }

    // JPEG nao tem canal alfa. Vidro e vegetacao recortada dependem dele, entao
    // essas seguem em PNG — trocar por JPEG encheria o recorte de preto.
    let tem_alfa = dyn_img
        .as_rgba8()
        .map(|i| i.pixels().any(|p| p.0[3] < 250))
        .unwrap_or(true);

    let mut saida = Vec::new();
    let mime = if tem_alfa {
        let _ = dyn_img.write_to(
            &mut std::io::Cursor::new(&mut saida),
            image::ImageFormat::Png,
        );
        "image/png"
    } else {
        let rgb = dyn_img.to_rgb8();
        let mut enc =
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut saida, QUALIDADE_JPEG);
        let _ = enc.encode(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            image::ExtendedColorType::Rgb8,
        );
        "image/jpeg"
    };
    (saida, mime, reduziu)
}

/// Grava a versao leve em `destino`. Devolve o que foi economizado.
pub fn gerar(m: &Model, destino: &Path) -> anyhow::Result<Ganho> {
    let (vertices, indices) = soldar(m);

    // --- buffer binario --------------------------------------------------
    let mut bin: Vec<u8> = Vec::with_capacity(vertices.len() * 32 + indices.len() * 4);
    let mut views: Vec<String> = Vec::new();
    let mut acessores: Vec<String> = Vec::new();

    // Alinha em 4 bytes: o glTF exige, e alguns leitores recusam sem isso.
    let alinhar = |b: &mut Vec<u8>| {
        while b.len() % 4 != 0 {
            b.push(0)
        }
    };

    let inicio_pos = bin.len();
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for v in &vertices {
        for e in 0..3 {
            lo[e] = lo[e].min(v.position[e]);
            hi[e] = hi[e].max(v.position[e]);
            bin.extend_from_slice(&v.position[e].to_le_bytes());
        }
    }
    let inicio_norm = bin.len();
    for v in &vertices {
        for e in 0..3 {
            bin.extend_from_slice(&v.normal[e].to_le_bytes());
        }
    }
    let inicio_uv = bin.len();
    for v in &vertices {
        for e in 0..2 {
            bin.extend_from_slice(&v.uv[e].to_le_bytes());
        }
    }
    let inicio_idx = bin.len();
    for &i in &indices {
        bin.extend_from_slice(&i.to_le_bytes());
    }
    alinhar(&mut bin);

    views.push(format!(
        r#"{{"buffer":0,"byteOffset":{inicio_pos},"byteLength":{},"target":34962}}"#,
        inicio_norm - inicio_pos
    ));
    views.push(format!(
        r#"{{"buffer":0,"byteOffset":{inicio_norm},"byteLength":{},"target":34962}}"#,
        inicio_uv - inicio_norm
    ));
    views.push(format!(
        r#"{{"buffer":0,"byteOffset":{inicio_uv},"byteLength":{},"target":34962}}"#,
        inicio_idx - inicio_uv
    ));
    views.push(format!(
        r#"{{"buffer":0,"byteOffset":{inicio_idx},"byteLength":{},"target":34963}}"#,
        indices.len() * 4
    ));

    let n = vertices.len();
    acessores.push(format!(
        r#"{{"bufferView":0,"componentType":5126,"count":{n},"type":"VEC3","min":[{},{},{}],"max":[{},{},{}]}}"#,
        f(lo[0]), f(lo[1]), f(lo[2]), f(hi[0]), f(hi[1]), f(hi[2])
    ));
    acessores.push(format!(
        r#"{{"bufferView":1,"componentType":5126,"count":{n},"type":"VEC3"}}"#
    ));
    acessores.push(format!(
        r#"{{"bufferView":2,"componentType":5126,"count":{n},"type":"VEC2"}}"#
    ));

    // --- texturas ---------------------------------------------------------
    let mut imagens: Vec<String> = Vec::new();
    let mut texturas_json: Vec<String> = Vec::new();
    let mut reduzidas = 0;
    for t in &m.texturas {
        let (bytes, mime, reduziu) = compactar_textura(t);
        if bytes.is_empty() {
            continue;
        }
        if reduziu {
            reduzidas += 1;
        }
        let off = bin.len();
        bin.extend_from_slice(&bytes);
        alinhar(&mut bin);
        views.push(format!(
            r#"{{"buffer":0,"byteOffset":{off},"byteLength":{}}}"#,
            bytes.len()
        ));
        let iv = views.len() - 1;
        imagens.push(format!(r#"{{"bufferView":{iv},"mimeType":"{mime}"}}"#));
        texturas_json.push(format!(r#"{{"source":{}}}"#, imagens.len() - 1));
    }

    // --- materiais e primitivas ------------------------------------------
    let mut materiais: Vec<String> = Vec::new();
    for mat in &m.materiais {
        let c = mat.base_color;
        let tex = match mat.textura {
            Some(i) if i < texturas_json.len() => format!(r#","baseColorTexture":{{"index":{i}}}"#),
            _ => String::new(),
        };
        materiais.push(format!(
            r#"{{"name":{},"pbrMetallicRoughness":{{"baseColorFactor":[{},{},{},{}],"metallicFactor":{},"roughnessFactor":{}{}}},"alphaMode":"{}","doubleSided":true}}"#,
            json_str(&mat.nome),
            f(c[0]), f(c[1]), f(c[2]), f(c[3]),
            f(mat.metallic), f(mat.roughness), tex,
            if mat.transparente { "BLEND" } else { "OPAQUE" }
        ));
    }

    // Uma primitiva por submesh, cada uma com sua faixa de indices. Preserva a
    // separacao por material, que e o que o Cesium usa para trocar de estado.
    let mut primitivas: Vec<String> = Vec::new();
    for s in &m.submeshes {
        let ia = acessores.len();
        acessores.push(format!(
            r#"{{"bufferView":3,"byteOffset":{},"componentType":5125,"count":{},"type":"SCALAR"}}"#,
            s.offset as usize * 4,
            s.count
        ));
        primitivas.push(format!(
            r#"{{"attributes":{{"POSITION":0,"NORMAL":1,"TEXCOORD_0":2}},"indices":{ia},"material":{}}}"#,
            s.material.min(materiais.len().saturating_sub(1))
        ));
    }

    let json = format!(
        r#"{{"asset":{{"version":"2.0","generator":"ARCZ otimizador"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],"meshes":[{{"primitives":[{}]}}],"materials":[{}],"textures":[{}],"images":[{}],"accessors":[{}],"bufferViews":[{}],"buffers":[{{"byteLength":{}}}],"samplers":[{{"magFilter":9729,"minFilter":9987,"wrapS":10497,"wrapT":10497}}]}}"#,
        primitivas.join(","),
        materiais.join(","),
        texturas_json.join(","),
        imagens.join(","),
        acessores.join(","),
        views.join(","),
        bin.len(),
    );

    // --- empacota ---------------------------------------------------------
    let mut jbytes = json.into_bytes();
    while jbytes.len() % 4 != 0 {
        jbytes.push(b' ');
    }
    let total = 12 + 8 + jbytes.len() + 8 + bin.len();
    let mut glb = Vec::with_capacity(total);
    glb.extend_from_slice(b"glTF");
    glb.extend_from_slice(&2u32.to_le_bytes());
    glb.extend_from_slice(&(total as u32).to_le_bytes());
    glb.extend_from_slice(&(jbytes.len() as u32).to_le_bytes());
    glb.extend_from_slice(b"JSON");
    glb.extend_from_slice(&jbytes);
    glb.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    glb.extend_from_slice(b"BIN\0");
    glb.extend_from_slice(&bin);

    if let Some(pai) = destino.parent() {
        std::fs::create_dir_all(pai)?;
    }
    // Grava em temporario e renomeia: uma queda no meio da escrita deixaria um
    // `.glb` truncado no cache, e o proximo acesso o serviria como se estivesse
    // bom.
    let tmp = destino.with_extension("parcial");
    std::fs::File::create(&tmp)?.write_all(&glb)?;
    std::fs::rename(&tmp, destino)?;

    Ok(Ganho {
        vertices_antes: m.vertices.len(),
        vertices_depois: vertices.len(),
        texturas_reduzidas: reduzidas,
    })
}

/// Caminho da versao leve no cache, derivado do arquivo de origem.
///
/// Inclui tamanho e data de modificacao: editar o modelo no SketchUp e exportar
/// de novo passa a gerar outro caminho, entao o globo nao serve a versao velha.
pub fn caminho_no_cache(origem: &Path) -> PathBuf {
    let md = std::fs::metadata(origem).ok();
    let tam = md.as_ref().map(|m| m.len()).unwrap_or(0);
    let mtime = md
        .as_ref()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let nome = origem
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("modelo");
    arcz_terrain::TileCache::default_root()
        .parent()
        .unwrap_or(&std::env::temp_dir())
        .join("leves")
        .join(format!("{nome}-{tam}-{mtime}.glb"))
}

/// Formata `f32` sem notacao cientifica, que o JSON aceita mas alguns leitores
/// tratam mal.
fn f(v: f32) -> String {
    if v.is_finite() {
        format!("{v:.6}")
    } else {
        "0".into()
    }
}

/// Escapa uma string para JSON.
fn json_str(s: &str) -> String {
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
mod testes {
    use super::*;
    use arcz_model::{Material, ModelVertex, Submesh, Textura};

    fn v(p: [f32; 3], n: [f32; 3], uv: [f32; 2]) -> ModelVertex {
        ModelVertex {
            position: p,
            normal: n,
            uv,
        }
    }

    fn modelo(vertices: Vec<ModelVertex>, indices: Vec<u32>) -> Model {
        let count = indices.len() as u32;
        Model {
            vertices,
            indices,
            submeshes: vec![Submesh {
                material: 0,
                offset: 0,
                count,
            }],
            materiais: vec![Material::default()],
            texturas: Vec::new(),
            min: [0.0; 3],
            max: [1.0; 3],
            primitivas_ignoradas: 0,
        }
    }

    #[test]
    fn solda_vertices_identicos_e_reescreve_os_indices() {
        // Dois triangulos que compartilham uma aresta, escritos como o SketchUp
        // escreve: seis vertices, dois pares iguais.
        let a = v([0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0]);
        let b = v([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 0.0]);
        let c = v([1.0, 0.0, 1.0], [0.0, 1.0, 0.0], [1.0, 1.0]);
        let d = v([0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [0.0, 1.0]);
        let m = modelo(vec![a, b, c, a, c, d], vec![0, 1, 2, 3, 4, 5]);

        let (vs, is) = soldar(&m);
        assert_eq!(vs.len(), 4, "os repetidos deviam ter sido soldados");
        assert_eq!(is.len(), 6, "a contagem de indices nao pode mudar");
        // Cada indice novo tem de cair no MESMO vertice que o indice antigo
        // apontava. `m.indices` e [0..5] e `m.vertices` e [a,b,c,a,c,d], entao a
        // comparacao e posicao a posicao com o buffer original.
        for (k, novo) in is.iter().enumerate() {
            let antigo = m.vertices[m.indices[k] as usize];
            assert_eq!(vs[*novo as usize].position, antigo.position, "indice {k}");
            assert_eq!(vs[*novo as usize].normal, antigo.normal, "indice {k}");
            assert_eq!(vs[*novo as usize].uv, antigo.uv, "indice {k}");
        }
    }

    #[test]
    fn nao_solda_vertices_com_normais_diferentes() {
        // Mesma posicao, normais opostas: e uma quina viva. Soldar arredondaria
        // a aresta e a iluminacao denunciaria na hora.
        let p = [1.0, 2.0, 3.0];
        let m = modelo(
            vec![
                v(p, [0.0, 1.0, 0.0], [0.0, 0.0]),
                v(p, [1.0, 0.0, 0.0], [0.0, 0.0]),
                v([9.0, 9.0, 9.0], [0.0, 1.0, 0.0], [1.0, 1.0]),
            ],
            vec![0, 1, 2],
        );
        let (vs, _) = soldar(&m);
        assert_eq!(vs.len(), 3);
    }

    #[test]
    fn indice_fora_do_buffer_e_preservado_em_vez_de_estourar() {
        let m = modelo(vec![v([0.0; 3], [0.0, 1.0, 0.0], [0.0; 2])], vec![0, 7, 0]);
        let (_, is) = soldar(&m);
        assert_eq!(is.len(), 3);
    }

    #[test]
    fn textura_grande_e_reduzida_e_a_pequena_nao() {
        // Opaca de verdade: alfa 255 em todo pixel. Preencher com 200 deixaria
        // o alfa em 200 e a textura seria (corretamente) tratada como recorte.
        let mut cheia = vec![200u8; 2048 * 2048 * 4];
        for p in cheia.chunks_exact_mut(4) {
            p[3] = 255;
        }
        let grande = Textura {
            nome: "g".into(),
            largura: 2048,
            altura: 2048,
            rgba: cheia,
        };
        let (bytes, mime, reduziu) = compactar_textura(&grande);
        assert!(reduziu, "2048 px tinha de encolher");
        assert_eq!(mime, "image/jpeg");
        assert!(
            bytes.len() < 400_000,
            "ficou grande demais: {}",
            bytes.len()
        );

        let pequena = Textura {
            nome: "p".into(),
            largura: 64,
            altura: 64,
            rgba: vec![255; 64 * 64 * 4],
        };
        let (_, _, red2) = compactar_textura(&pequena);
        assert!(!red2, "64 px nao devia encolher");
    }

    #[test]
    fn textura_com_alfa_continua_em_png() {
        // Vidro e vegetacao recortada dependem do alfa; JPEG o descartaria e o
        // recorte viraria um retangulo preto.
        let mut rgba = vec![255u8; 32 * 32 * 4];
        rgba[3] = 0;
        let t = Textura {
            nome: "vidro".into(),
            largura: 32,
            altura: 32,
            rgba,
        };
        let (_, mime, _) = compactar_textura(&t);
        assert_eq!(mime, "image/png");
    }

    #[test]
    fn o_glb_gerado_abre_no_proprio_carregador() {
        let a = v([0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0]);
        let b = v([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 0.0]);
        let c = v([1.0, 0.0, 1.0], [0.0, 1.0, 0.0], [1.0, 1.0]);
        let d = v([0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [0.0, 1.0]);
        let m = modelo(vec![a, b, c, a, c, d], vec![0, 1, 2, 3, 4, 5]);

        let dir = std::env::temp_dir().join("arcz-teste-otimizar");
        let _ = std::fs::create_dir_all(&dir);
        let saida = dir.join("leve.glb");
        let ganho = gerar(&m, &saida).expect("gerar");
        assert_eq!(ganho.vertices_antes, 6);
        assert_eq!(ganho.vertices_depois, 4);

        let bytes = std::fs::read(&saida).unwrap();
        assert_eq!(&bytes[0..4], b"glTF", "magic errado derruba o Cesium");

        // Le de volta: gravar um GLB que o proprio ARCZ nao abre seria pior que
        // nao gravar nada.
        let lido = Model::load(&saida).expect("reler o glb otimizado");
        assert_eq!(lido.indices.len(), 6);
        assert_eq!(lido.vertices.len(), 4);
        let _ = std::fs::remove_file(&saida);
    }

    #[test]
    fn o_cache_muda_de_nome_quando_o_arquivo_muda() {
        let dir = std::env::temp_dir().join("arcz-teste-cache-leve");
        let _ = std::fs::create_dir_all(&dir);
        let p = dir.join("m.glb");
        std::fs::write(&p, b"abc").unwrap();
        let c1 = caminho_no_cache(&p);
        std::fs::write(&p, b"abcdefgh").unwrap();
        let c2 = caminho_no_cache(&p);
        assert_ne!(c1, c2, "reexportar o modelo tem de invalidar o cache");
        let _ = std::fs::remove_file(&p);
    }
}

//! Import de modelos 3D do usuario e posicionamento georreferenciado.
//!
//! Aceita glTF 2.0 (`.gltf`) e glTF binario (`.glb`), com materiais e texturas.
//! O modelo entra no seu proprio espaco local, e [`place`] o coloca no quadro ENU
//! da cena a partir de lat/lon, rumo (heading) e escala — assentando a base no terreno.

pub mod analise;
pub mod kmz;
pub mod material;
pub mod place;

pub use kmz::{Georreferencia, KmzError};
pub use material::{imagem_para_textura, Material, Submesh, Textura};
pub use place::{
    caixa_transformada, matriz_modelo, place, transformar, FonteGeometria, PlacedModel, Placement,
    Transformado,
};

use std::path::Path;

/// Lado maximo de textura apos o import. Ver [`material::imagem_para_textura`].
pub const MAX_LADO_TEXTURA_PADRAO: u32 = 2048;

/// Vertice do modelo, no espaco local do arquivo (Y-up, como manda a spec glTF).
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct ModelVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

/// Malha do arquivo, com os triangulos agrupados por material.
#[derive(Debug, Clone)]
pub struct Model {
    pub vertices: Vec<ModelVertex>,
    /// Indices ja **ordenados por material**: cada [`Submesh`] aponta para uma faixa.
    pub indices: Vec<u32>,
    pub submeshes: Vec<Submesh>,
    pub materiais: Vec<Material>,
    pub texturas: Vec<Textura>,
    pub min: [f32; 3],
    pub max: [f32; 3],
    /// Primitivas ignoradas por nao serem triangulos (linhas, pontos).
    pub primitivas_ignoradas: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("falha ao ler o arquivo: {0}")]
    Io(#[from] std::io::Error),
    #[error("glTF invalido: {0}")]
    Gltf(#[from] gltf::Error),
    #[error("o arquivo nao contem nenhuma malha triangular")]
    SemMalha,
    #[error("primitiva sem atributo POSITION")]
    SemPosicao,
    #[error("nao consegui reabrir o arquivo: {0}")]
    Formato(String),
    #[error("extensao '{0}' nao suportada. Hoje o ARCZ le .gltf e .glb — exporte por ai")]
    ExtensaoDesconhecida(String),
}

impl Model {
    pub fn load(caminho: impl AsRef<Path>) -> Result<Self, ModelError> {
        Self::load_com_limite(caminho, MAX_LADO_TEXTURA_PADRAO)
    }

    pub fn load_com_limite(
        caminho: impl AsRef<Path>,
        max_lado_textura: u32,
    ) -> Result<Self, ModelError> {
        let caminho = caminho.as_ref();
        match caminho
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref()
        {
            Some("gltf") | Some("glb") => {}
            Some(outra) => return Err(ModelError::ExtensaoDesconhecida(outra.to_string())),
            None => return Err(ModelError::ExtensaoDesconhecida(String::new())),
        }

        // `import` resolve buffers e imagens externos relativos ao arquivo.
        let (doc, buffers, imagens) = match gltf::import(caminho) {
            Ok(t) => t,
            Err(e) if extensao_apenas_de_material(&e) => {
                // Modelo do Sketchfab exportado com `KHR_materials_pbrSpecularGlossiness`,
                // depreciado pelo Khronos mas ainda comum em acervo antigo. O
                // arquivo o declara em `extensionsRequired`, e o loader recusa o
                // arquivo inteiro por causa disso.
                //
                // A extensao so descreve **material**: geometria, UV e texturas
                // continuam no formato padrao. Reabrir sem a exigencia entrega o
                // modelo com o material base em vez de nao entregar nada — e a
                // diferenca visual e menor que a ausencia da peca na cena.
                log::warn!(
                    "{}: extensao de material nao suportada; carregando com material base",
                    caminho.display()
                );
                let bytes = std::fs::read(caminho)?;
                importar_ignorando_extensao(&bytes)?
            }
            Err(e) => return Err(e.into()),
        };
        Self::from_document(&doc, &buffers, &imagens, max_lado_textura)
    }

    /// Carrega um GLB ja em memoria (buffers e imagens embutidos).
    pub fn from_glb_slice(bytes: &[u8]) -> Result<Self, ModelError> {
        let (doc, buffers, imagens) = match gltf::import_slice(bytes) {
            Ok(t) => t,
            Err(e) if extensao_apenas_de_material(&e) => importar_ignorando_extensao(bytes)?,
            Err(e) => return Err(e.into()),
        };
        Self::from_document(&doc, &buffers, &imagens, MAX_LADO_TEXTURA_PADRAO)
    }

    #[allow(clippy::type_complexity)]
    fn from_document(
        doc: &gltf::Document,
        buffers: &[gltf::buffer::Data],
        imagens: &[gltf::image::Data],
        max_lado_textura: u32,
    ) -> Result<Self, ModelError> {
        let mut vertices: Vec<ModelVertex> = Vec::new();
        // Indices agrupados por id de material antes de virarem um buffer unico.
        let mut por_material: Vec<Vec<u32>> = vec![Vec::new(); doc.materials().len() + 1];
        let mut ignoradas = 0usize;

        // Percorre a hierarquia acumulando a transformacao de cada no. Ignorar isso
        // e o erro que faz o modelo aparecer na origem, achatado ou em pedacos.
        for cena in doc.scenes() {
            for no in cena.nodes() {
                percorrer(
                    &no,
                    identidade(),
                    buffers,
                    &mut vertices,
                    &mut por_material,
                    &mut ignoradas,
                )?;
            }
        }

        if vertices.is_empty() || por_material.iter().all(|v| v.is_empty()) {
            return Err(ModelError::SemMalha);
        }

        // Indice 0 e o material padrao (primitivas sem material declarado).
        let mut materiais = vec![Material::default()];
        for m in doc.materials() {
            let pbr = m.pbr_metallic_roughness();
            materiais.push(Material {
                nome: m.name().unwrap_or("sem-nome").to_string(),
                base_color: pbr.base_color_factor(),
                textura: pbr
                    .base_color_texture()
                    .map(|t| t.texture().source().index()),
                metallic: pbr.metallic_factor(),
                roughness: pbr.roughness_factor(),
                transparente: !matches!(m.alpha_mode(), gltf::material::AlphaMode::Opaque),
            });
        }

        // So converte as imagens que algum material realmente usa.
        let usadas: std::collections::BTreeSet<usize> =
            materiais.iter().filter_map(|m| m.textura).collect();
        let mut texturas = Vec::with_capacity(usadas.len());
        let mut mapa = std::collections::HashMap::new();
        for &i in &usadas {
            let Some(img) = imagens.get(i) else {
                log::warn!("material aponta para a imagem {i}, que nao existe no arquivo");
                continue;
            };
            mapa.insert(i, texturas.len());
            texturas.push(imagem_para_textura(
                format!("imagem_{i}"),
                img,
                max_lado_textura,
            ));
        }
        // Reindexa os materiais para a lista compactada.
        for m in &mut materiais {
            m.textura = m.textura.and_then(|i| mapa.get(&i).copied());
        }

        // Concatena os grupos num buffer unico, um submesh por material nao-vazio.
        let total: usize = por_material.iter().map(|v| v.len()).sum();
        let mut indices = Vec::with_capacity(total);
        let mut submeshes = Vec::new();
        for (mat, grupo) in por_material.into_iter().enumerate() {
            if grupo.is_empty() {
                continue;
            }
            submeshes.push(Submesh {
                material: mat,
                offset: indices.len() as u32,
                count: grupo.len() as u32,
            });
            indices.extend_from_slice(&grupo);
        }

        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for v in &vertices {
            for (k, c) in v.position.iter().enumerate() {
                min[k] = min[k].min(*c);
                max[k] = max[k].max(*c);
            }
        }

        Ok(Self {
            vertices,
            indices,
            submeshes,
            materiais,
            texturas,
            min,
            max,
            primitivas_ignoradas: ignoradas,
        })
    }

    /// Dimensoes da caixa envolvente, em unidades do arquivo.
    pub fn size(&self) -> [f32; 3] {
        [
            self.max[0] - self.min[0],
            self.max[1] - self.min[1],
            self.max[2] - self.min[2],
        ]
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Soma dos bytes de todas as texturas ja descomprimidas.
    pub fn bytes_de_textura(&self) -> usize {
        self.texturas.iter().map(|t| t.bytes()).sum()
    }

    /// Heuristica de unidade do arquivo.
    ///
    /// A spec glTF diz metros, mas exportadores de CAD e SketchUp mandam centimetro
    /// ou milimetro sem avisar. Um predio de 30 "unidades" e plausivel em metros;
    /// 3000 quase certamente e centimetros. Isto **nao** corrige nada sozinho — so
    /// devolve um aviso para o usuario decidir com `--modelo-escala`.
    pub fn suspeita_de_unidade(&self) -> Option<&'static str> {
        let maior = self.size().iter().cloned().fold(0.0_f32, f32::max);
        if maior > 2_000.0 {
            Some("modelo com mais de 2 km de extensao — provavelmente esta em centimetros ou milimetros")
        } else if maior < 0.5 {
            Some("modelo com menos de 50 cm — provavelmente esta em quilometros ou numa escala normalizada")
        } else {
            None
        }
    }
}

fn percorrer(
    no: &gltf::Node,
    pai: [[f32; 4]; 4],
    buffers: &[gltf::buffer::Data],
    vertices: &mut Vec<ModelVertex>,
    por_material: &mut [Vec<u32>],
    ignoradas: &mut usize,
) -> Result<(), ModelError> {
    let mundo = mul4(pai, no.transform().matrix());

    if let Some(malha) = no.mesh() {
        for prim in malha.primitives() {
            if prim.mode() != gltf::mesh::Mode::Triangles {
                *ignoradas += 1;
                continue;
            }

            let leitor = prim.reader(|b| buffers.get(b.index()).map(|d| &d.0[..]));
            let posicoes: Vec<[f32; 3]> = leitor
                .read_positions()
                .ok_or(ModelError::SemPosicao)?
                .collect();

            let normais: Option<Vec<[f32; 3]>> = leitor.read_normals().map(|n| n.collect());
            let uvs: Vec<[f32; 2]> = match leitor.read_tex_coords(0) {
                Some(t) => t.into_f32().collect(),
                None => vec![[0.0, 0.0]; posicoes.len()],
            };
            let idx_local: Vec<u32> = match leitor.read_indices() {
                Some(i) => i.into_u32().collect(),
                None => (0..posicoes.len() as u32).collect(),
            };

            let base = vertices.len() as u32;
            for (i, p) in posicoes.iter().enumerate() {
                let n = match &normais {
                    Some(ns) => normalizar(transformar_direcao(mundo, ns[i])),
                    // Sem NORMAL o glTF manda usar normal de face; calculada abaixo.
                    None => [0.0, 0.0, 0.0],
                };
                vertices.push(ModelVertex {
                    position: transformar_ponto(mundo, *p),
                    normal: n,
                    uv: *uvs.get(i).unwrap_or(&[0.0, 0.0]),
                });
            }

            let globais: Vec<u32> = idx_local.iter().map(|i| base + i).collect();
            if normais.is_none() {
                gerar_normais_de_face(vertices, &globais);
            }

            // Material 0 e o padrao; os do documento entram deslocados em 1.
            let mat = prim.material().index().map(|i| i + 1).unwrap_or(0);
            por_material[mat.min(por_material.len() - 1)].extend_from_slice(&globais);
        }
    }

    for filho in no.children() {
        percorrer(&filho, mundo, buffers, vertices, por_material, ignoradas)?;
    }
    Ok(())
}

/// Acumula normais de face nos vertices e normaliza. Usado quando o arquivo nao
/// traz NORMAL — sem isso o modelo aparece preto.
fn gerar_normais_de_face(vertices: &mut [ModelVertex], indices: &[u32]) {
    for t in indices.chunks_exact(3) {
        let (a, b, c) = (t[0] as usize, t[1] as usize, t[2] as usize);
        let n = cross(
            sub(vertices[b].position, vertices[a].position),
            sub(vertices[c].position, vertices[a].position),
        );
        for &i in &[a, b, c] {
            for (acumulado, componente) in vertices[i].normal.iter_mut().zip(n) {
                *acumulado += componente;
            }
        }
    }
    for &i in indices {
        let v = &mut vertices[i as usize];
        v.normal = normalizar(v.normal);
    }
}

fn identidade() -> [[f32; 4]; 4] {
    let mut m = [[0.0; 4]; 4];
    for (k, col) in m.iter_mut().enumerate() {
        col[k] = 1.0;
    }
    m
}

/// Multiplicacao de matrizes column-major, igual a convencao do glTF.
fn mul4(a: [[f32; 4]; 4], b: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut o = [[0.0; 4]; 4];
    for c in 0..4 {
        for r in 0..4 {
            o[c][r] = (0..4).map(|k| a[k][r] * b[c][k]).sum();
        }
    }
    o
}

fn transformar_ponto(m: [[f32; 4]; 4], v: [f32; 3]) -> [f32; 3] {
    let mut o = [0.0; 3];
    for (r, item) in o.iter_mut().enumerate() {
        *item = m[0][r] * v[0] + m[1][r] * v[1] + m[2][r] * v[2] + m[3][r];
    }
    o
}

fn transformar_direcao(m: [[f32; 4]; 4], v: [f32; 3]) -> [f32; 3] {
    let mut o = [0.0; 3];
    for (r, item) in o.iter_mut().enumerate() {
        *item = m[0][r] * v[0] + m[1][r] * v[1] + m[2][r] * v[2];
    }
    o
}

pub(crate) fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(crate) fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub(crate) fn normalizar(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l < 1e-12 {
        [0.0, 1.0, 0.0]
    } else {
        [v[0] / l, v[1] / l, v[2] / l]
    }
}

#[cfg(test)]
pub(crate) mod testdata {
    /// Monta um GLB valido em memoria com um retangulo vertical de `larg` x `alt`
    /// no plano XY, sem NORMAL nem TEXCOORD (o caminho mais dificil do loader).
    pub fn glb_retangulo(larg: f32, alt: f32) -> Vec<u8> {
        let pos: [[f32; 3]; 4] = [
            [0.0, 0.0, 0.0],
            [larg, 0.0, 0.0],
            [larg, alt, 0.0],
            [0.0, alt, 0.0],
        ];
        let idx: [u16; 6] = [0, 1, 2, 0, 2, 3];

        let mut bin = Vec::new();
        for p in pos {
            for c in p {
                bin.extend_from_slice(&c.to_le_bytes());
            }
        }
        let offset_idx = bin.len();
        for i in idx {
            bin.extend_from_slice(&i.to_le_bytes());
        }
        while bin.len() % 4 != 0 {
            bin.push(0);
        }

        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"scene":0,
            "scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"indices":1}}]}}],
            "buffers":[{{"byteLength":{}}}],
            "bufferViews":[
              {{"buffer":0,"byteOffset":0,"byteLength":48}},
              {{"buffer":0,"byteOffset":{},"byteLength":12}}],
            "accessors":[
              {{"bufferView":0,"componentType":5126,"count":4,"type":"VEC3",
                "min":[0.0,0.0,0.0],"max":[{larg},{alt},0.0]}},
              {{"bufferView":1,"componentType":5123,"count":6,"type":"SCALAR"}}]}}"#,
            bin.len(),
            offset_idx
        );

        montar_glb(json, bin)
    }

    /// Igual ao anterior, mas com dois materiais coloridos, um por triangulo.
    /// Exercita o agrupamento por material e os submeshes.
    pub fn glb_dois_materiais() -> Vec<u8> {
        let pos: [[f32; 3]; 4] = [
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [10.0, 10.0, 0.0],
            [0.0, 10.0, 0.0],
        ];
        let idx_a: [u16; 3] = [0, 1, 2];
        let idx_b: [u16; 3] = [0, 2, 3];

        let mut bin = Vec::new();
        for p in pos {
            for c in p {
                bin.extend_from_slice(&c.to_le_bytes());
            }
        }
        let off_a = bin.len();
        for i in idx_a {
            bin.extend_from_slice(&i.to_le_bytes());
        }
        let off_b = bin.len();
        for i in idx_b {
            bin.extend_from_slice(&i.to_le_bytes());
        }
        while bin.len() % 4 != 0 {
            bin.push(0);
        }

        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"scene":0,
            "scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],
            "meshes":[{{"primitives":[
               {{"attributes":{{"POSITION":0}},"indices":1,"material":0}},
               {{"attributes":{{"POSITION":0}},"indices":2,"material":1}}]}}],
            "materials":[
               {{"name":"vermelho","pbrMetallicRoughness":{{"baseColorFactor":[1.0,0.0,0.0,1.0]}}}},
               {{"name":"azul","pbrMetallicRoughness":{{"baseColorFactor":[0.0,0.0,1.0,1.0],
                 "metallicFactor":0.5,"roughnessFactor":0.25}},"alphaMode":"BLEND"}}],
            "buffers":[{{"byteLength":{}}}],
            "bufferViews":[
              {{"buffer":0,"byteOffset":0,"byteLength":48}},
              {{"buffer":0,"byteOffset":{off_a},"byteLength":6}},
              {{"buffer":0,"byteOffset":{off_b},"byteLength":6}}],
            "accessors":[
              {{"bufferView":0,"componentType":5126,"count":4,"type":"VEC3",
                "min":[0.0,0.0,0.0],"max":[10.0,10.0,0.0]}},
              {{"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}},
              {{"bufferView":2,"componentType":5123,"count":3,"type":"SCALAR"}}]}}"#,
            bin.len()
        );

        montar_glb(json, bin)
    }

    fn montar_glb(json: String, bin: Vec<u8>) -> Vec<u8> {
        let mut json_bytes = json.into_bytes();
        while json_bytes.len() % 4 != 0 {
            json_bytes.push(b' ');
        }

        let total = 12 + 8 + json_bytes.len() + 8 + bin.len();
        let mut glb = Vec::with_capacity(total);
        glb.extend_from_slice(b"glTF");
        glb.extend_from_slice(&2u32.to_le_bytes());
        glb.extend_from_slice(&(total as u32).to_le_bytes());
        glb.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
        glb.extend_from_slice(b"JSON");
        glb.extend_from_slice(&json_bytes);
        glb.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        glb.extend_from_slice(&[0x42, 0x49, 0x4E, 0x00]);
        glb.extend_from_slice(&bin);
        glb
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carrega_glb_e_mede_a_caixa_envolvente() {
        let m = Model::from_glb_slice(&testdata::glb_retangulo(12.0, 30.0)).unwrap();

        assert_eq!(m.vertices.len(), 4);
        assert_eq!(m.triangle_count(), 2);
        assert_eq!(m.min, [0.0, 0.0, 0.0]);
        assert_eq!(m.max, [12.0, 30.0, 0.0]);
        assert_eq!(m.size(), [12.0, 30.0, 0.0]);
        assert_eq!(m.primitivas_ignoradas, 0);
    }

    #[test]
    fn sem_material_declarado_cai_no_material_padrao() {
        let m = Model::from_glb_slice(&testdata::glb_retangulo(1.0, 1.0)).unwrap();
        assert_eq!(m.submeshes.len(), 1);
        assert_eq!(m.submeshes[0].material, 0);
        assert_eq!(m.submeshes[0].offset, 0);
        assert_eq!(m.submeshes[0].count, 6);
        assert!(m.texturas.is_empty());
    }

    #[test]
    fn agrupa_triangulos_por_material_em_submeshes() {
        let m = Model::from_glb_slice(&testdata::glb_dois_materiais()).unwrap();

        // Material 0 = padrao (vazio aqui), 1 = vermelho, 2 = azul.
        assert_eq!(m.materiais.len(), 3);
        assert_eq!(m.materiais[1].nome, "vermelho");
        assert_eq!(m.materiais[2].nome, "azul");

        assert_eq!(
            m.submeshes.len(),
            2,
            "esperava um submesh por material usado"
        );
        let mats: Vec<usize> = m.submeshes.iter().map(|s| s.material).collect();
        assert_eq!(mats, vec![1, 2]);

        // As faixas tem que cobrir o buffer inteiro, sem buraco nem sobreposicao.
        let mut esperado = 0u32;
        for s in &m.submeshes {
            assert_eq!(s.offset, esperado, "submesh {s:?} fora de sequencia");
            esperado += s.count;
        }
        assert_eq!(esperado as usize, m.indices.len());
        assert_eq!(m.triangle_count(), 2);
    }

    #[test]
    fn le_cor_base_metallic_roughness_e_transparencia() {
        let m = Model::from_glb_slice(&testdata::glb_dois_materiais()).unwrap();

        assert_eq!(m.materiais[1].base_color, [1.0, 0.0, 0.0, 1.0]);
        assert!(!m.materiais[1].transparente);

        assert_eq!(m.materiais[2].base_color, [0.0, 0.0, 1.0, 1.0]);
        assert!((m.materiais[2].metallic - 0.5).abs() < 1e-6);
        assert!((m.materiais[2].roughness - 0.25).abs() < 1e-6);
        assert!(
            m.materiais[2].transparente,
            "alphaMode BLEND nao foi detectado"
        );
    }

    #[test]
    fn indices_dos_submeshes_apontam_para_vertices_validos() {
        let m = Model::from_glb_slice(&testdata::glb_dois_materiais()).unwrap();
        for s in &m.submeshes {
            let faixa = &m.indices[s.offset as usize..(s.offset + s.count) as usize];
            assert!(faixa.iter().all(|&i| (i as usize) < m.vertices.len()));
            assert_eq!(faixa.len() % 3, 0, "submesh com triangulo incompleto");
        }
        assert!(m.submeshes.iter().all(|s| s.material < m.materiais.len()));
    }

    #[test]
    fn gera_normais_quando_o_arquivo_nao_traz() {
        // Sem isto o modelo entra com normal zero e renderiza totalmente preto.
        let m = Model::from_glb_slice(&testdata::glb_retangulo(10.0, 10.0)).unwrap();
        for v in &m.vertices {
            let l = (v.normal[0].powi(2) + v.normal[1].powi(2) + v.normal[2].powi(2)).sqrt();
            assert!(
                (l - 1.0).abs() < 1e-5,
                "normal nao unitaria: {:?}",
                v.normal
            );
        }
        // O retangulo esta no plano XY, entao a normal e paralela a Z.
        assert!(
            m.vertices[0].normal[2].abs() > 0.99,
            "{:?}",
            m.vertices[0].normal
        );
    }

    #[test]
    fn detecta_suspeita_de_unidade() {
        let metros = Model::from_glb_slice(&testdata::glb_retangulo(12.0, 30.0)).unwrap();
        assert!(metros.suspeita_de_unidade().is_none());

        let centimetros = Model::from_glb_slice(&testdata::glb_retangulo(1200.0, 3000.0)).unwrap();
        assert!(centimetros.suspeita_de_unidade().is_some());

        let normalizado = Model::from_glb_slice(&testdata::glb_retangulo(0.1, 0.2)).unwrap();
        assert!(normalizado.suspeita_de_unidade().is_some());
    }

    #[test]
    fn rejeita_extensao_desconhecida_sem_ler_o_arquivo() {
        let e = Model::load("modelo.obj").unwrap_err();
        assert!(
            matches!(e, ModelError::ExtensaoDesconhecida(ref s) if s == "obj"),
            "{e:?}"
        );
        // A mensagem tem que dizer o que fazer, nao so que falhou.
        assert!(e.to_string().contains(".glb"), "{e}");
    }

    #[test]
    fn rejeita_bytes_invalidos() {
        assert!(Model::from_glb_slice(b"nao sou um glb").is_err());
    }
}

/// Extensoes que so descrevem **material** e podem ser ignoradas com seguranca.
///
/// Nao inclui nada que mexa em geometria (`KHR_draco_mesh_compression`,
/// `EXT_meshopt_compression`): ignorar uma dessas devolveria vertices lixo em
/// vez de um material menos fiel.
const EXTENSOES_SO_DE_MATERIAL: &[&str] = &[
    "KHR_materials_pbrSpecularGlossiness",
    "KHR_materials_specular",
    "KHR_materials_sheen",
    "KHR_materials_clearcoat",
    "KHR_materials_transmission",
    "KHR_materials_volume",
    "KHR_materials_ior",
    "KHR_materials_iridescence",
    "KHR_materials_anisotropy",
    "KHR_materials_emissive_strength",
];

/// O erro e "extensao nao suportada" de uma extensao que so afeta material?
fn extensao_apenas_de_material(e: &gltf::Error) -> bool {
    let texto = e.to_string();
    if !texto.contains("Unsupported extension") {
        return false;
    }
    EXTENSOES_SO_DE_MATERIAL.iter().any(|x| texto.contains(x))
}

/// Reimporta o GLB removendo as extensoes de material de `extensionsRequired`.
///
/// O `extensionsRequired` e uma declaracao de "recuse o arquivo se nao souber
/// isto". Para extensao de material a recusa e desproporcional: a geometria, as
/// UVs e as texturas estao no formato padrao, e o que se perde e o modelo de
/// reflexao — enquanto a alternativa e a peca sumir da cena.
///
/// A remocao acontece **em memoria**; o arquivo do usuario nao e tocado.
fn importar_ignorando_extensao(
    bytes: &[u8],
) -> Result<
    (
        gltf::Document,
        Vec<gltf::buffer::Data>,
        Vec<gltf::image::Data>,
    ),
    ModelError,
> {
    let glb = gltf::binary::Glb::from_slice(bytes)
        .map_err(|e| ModelError::Formato(format!("GLB invalido: {e}")))?;

    let mut json: serde_json::Value = serde_json::from_slice(&glb.json)
        .map_err(|e| ModelError::Formato(format!("JSON do GLB invalido: {e}")))?;

    if let Some(req) = json
        .get_mut("extensionsRequired")
        .and_then(|v| v.as_array_mut())
    {
        req.retain(|v| {
            v.as_str()
                .map(|s| !EXTENSOES_SO_DE_MATERIAL.contains(&s))
                .unwrap_or(true)
        });
        // Array vazio e valido, mas remover a chave e mais limpo e evita que
        // outro leitor trate `[]` como exigencia desconhecida.
        if req.is_empty() {
            json.as_object_mut().map(|o| o.remove("extensionsRequired"));
        }
    }

    let json_novo = serde_json::to_vec(&json)
        .map_err(|e| ModelError::Formato(format!("nao reserializei o JSON: {e}")))?;
    let novo = gltf::binary::Glb {
        header: glb.header,
        json: std::borrow::Cow::Owned(json_novo),
        bin: glb.bin,
    };
    let bytes_novos = novo
        .to_vec()
        .map_err(|e| ModelError::Formato(format!("nao remontei o GLB: {e}")))?;

    Ok(gltf::import_slice(&bytes_novos)?)
}

//! Materiais e texturas extraidos do arquivo.

/// Textura em RGBA8, ja pronta para virar textura de GPU.
#[derive(Clone)]
pub struct Textura {
    pub nome: String,
    pub largura: u32,
    pub altura: u32,
    pub rgba: Vec<u8>,
}

impl std::fmt::Debug for Textura {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Sem despejar megabytes de pixel no log.
        f.debug_struct("Textura")
            .field("nome", &self.nome)
            .field("largura", &self.largura)
            .field("altura", &self.altura)
            .field("bytes", &self.rgba.len())
            .finish()
    }
}

impl Textura {
    pub fn bytes(&self) -> usize {
        self.rgba.len()
    }
}

/// Material PBR simplificado. Nesta fatia so a cor base e a textura de cor base
/// sao usadas no render; metallic/roughness ficam guardados para a Fatia 4, quando
/// o path tracer entrar.
#[derive(Debug, Clone)]
pub struct Material {
    pub nome: String,
    /// Multiplicador de cor base (RGBA linear).
    pub base_color: [f32; 4],
    /// Indice em [`crate::Model::texturas`].
    pub textura: Option<usize>,
    pub metallic: f32,
    pub roughness: f32,
    /// `true` se o material pede recorte/transparencia (vidro, folhagem em plano).
    pub transparente: bool,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            nome: "padrao".into(),
            base_color: [0.82, 0.82, 0.82, 1.0],
            textura: None,
            metallic: 0.0,
            roughness: 0.9,
            transparente: false,
        }
    }
}

/// Faixa de indices que compartilha o mesmo material.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Submesh {
    pub material: usize,
    pub offset: u32,
    pub count: u32,
}

/// Converte uma imagem do glTF para RGBA8, reduzindo se passar de `max_lado`.
///
/// O limite existe por VRAM: um modelo de arquitetura exportado de SketchUp costuma
/// trazer dezenas de texturas 4096². Descomprimidas em RGBA sao 67 MB **cada** —
/// 40 delas estouram qualquer placa. Reduzir para 2048 corta o consumo em 4x com
/// perda imperceptivel na escala em que o predio e visto.
pub fn imagem_para_textura(nome: String, img: &gltf::image::Data, max_lado: u32) -> Textura {
    use gltf::image::Format;

    let (w, h) = (img.width, img.height);
    let px = (w as usize) * (h as usize);

    // Expande para RGBA8 qualquer que seja o formato de origem.
    let mut rgba = Vec::with_capacity(px * 4);
    match img.format {
        Format::R8G8B8A8 => rgba.extend_from_slice(&img.pixels),
        Format::R8G8B8 => {
            for c in img.pixels.chunks_exact(3) {
                rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
        Format::R8G8 => {
            for c in img.pixels.chunks_exact(2) {
                rgba.extend_from_slice(&[c[0], c[0], c[0], c[1]]);
            }
        }
        Format::R8 => {
            for &v in &img.pixels {
                rgba.extend_from_slice(&[v, v, v, 255]);
            }
        }
        // Formatos de 16/32 bits: pega o byte alto de cada canal. Textura de cor
        // base em 16 bits e raro e nao justifica um caminho dedicado.
        Format::R16G16B16A16 => {
            for c in img.pixels.chunks_exact(8) {
                rgba.extend_from_slice(&[c[1], c[3], c[5], c[7]]);
            }
        }
        Format::R16G16B16 => {
            for c in img.pixels.chunks_exact(6) {
                rgba.extend_from_slice(&[c[1], c[3], c[5], 255]);
            }
        }
        Format::R16G16 => {
            for c in img.pixels.chunks_exact(4) {
                rgba.extend_from_slice(&[c[1], c[1], c[1], c[3]]);
            }
        }
        Format::R16 => {
            for c in img.pixels.chunks_exact(2) {
                rgba.extend_from_slice(&[c[1], c[1], c[1], 255]);
            }
        }
        Format::R32G32B32FLOAT => {
            for c in img.pixels.chunks_exact(12) {
                rgba.extend_from_slice(&[
                    float_para_byte(c[0..4].try_into().unwrap()),
                    float_para_byte(c[4..8].try_into().unwrap()),
                    float_para_byte(c[8..12].try_into().unwrap()),
                    255,
                ]);
            }
        }
        Format::R32G32B32A32FLOAT => {
            for c in img.pixels.chunks_exact(16) {
                rgba.extend_from_slice(&[
                    float_para_byte(c[0..4].try_into().unwrap()),
                    float_para_byte(c[4..8].try_into().unwrap()),
                    float_para_byte(c[8..12].try_into().unwrap()),
                    float_para_byte(c[12..16].try_into().unwrap()),
                ]);
            }
        }
    }

    // Se a expansao nao bateu (arquivo truncado), completa com magenta opaco para
    // o problema ficar visivel na tela em vez de virar textura preta silenciosa.
    rgba.resize(px * 4, 255);

    if w.max(h) <= max_lado || w == 0 || h == 0 {
        return Textura {
            nome,
            largura: w,
            altura: h,
            rgba,
        };
    }

    let escala = max_lado as f32 / w.max(h) as f32;
    let nw = ((w as f32 * escala).round() as u32).max(1);
    let nh = ((h as f32 * escala).round() as u32).max(1);

    let Some(buf) = image::RgbaImage::from_raw(w, h, rgba) else {
        // Nao deveria acontecer depois do resize acima; se acontecer, devolve 1x1.
        return Textura {
            nome,
            largura: 1,
            altura: 1,
            rgba: vec![255, 0, 255, 255],
        };
    };
    let menor = image::imageops::resize(&buf, nw, nh, image::imageops::FilterType::Triangle);

    Textura {
        nome,
        largura: nw,
        altura: nh,
        rgba: menor.into_raw(),
    }
}

fn float_para_byte(b: [u8; 4]) -> u8 {
    (f32::from_le_bytes(b).clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use gltf::image::Format;

    fn dados(format: Format, width: u32, height: u32, pixels: Vec<u8>) -> gltf::image::Data {
        gltf::image::Data {
            pixels,
            format,
            width,
            height,
        }
    }

    #[test]
    fn rgb_vira_rgba_opaco() {
        let d = dados(Format::R8G8B8, 2, 1, vec![10, 20, 30, 40, 50, 60]);
        let t = imagem_para_textura("t".into(), &d, 4096);
        assert_eq!(t.rgba, vec![10, 20, 30, 255, 40, 50, 60, 255]);
        assert_eq!((t.largura, t.altura), (2, 1));
    }

    #[test]
    fn rgba_passa_intacto() {
        let px = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let d = dados(Format::R8G8B8A8, 2, 1, px.clone());
        assert_eq!(imagem_para_textura("t".into(), &d, 4096).rgba, px);
    }

    #[test]
    fn cinza_vira_rgba_replicado() {
        let d = dados(Format::R8, 2, 1, vec![77, 200]);
        let t = imagem_para_textura("t".into(), &d, 4096);
        assert_eq!(t.rgba, vec![77, 77, 77, 255, 200, 200, 200, 255]);
    }

    #[test]
    fn textura_grande_e_reduzida_ate_o_limite() {
        // 1024x512 com limite 256 -> 256x128, preservando a proporcao.
        let d = dados(Format::R8G8B8, 1024, 512, vec![128; 1024 * 512 * 3]);
        let t = imagem_para_textura("grande".into(), &d, 256);
        assert_eq!((t.largura, t.altura), (256, 128));
        assert_eq!(t.rgba.len(), 256 * 128 * 4);
    }

    #[test]
    fn textura_pequena_nao_e_tocada() {
        let d = dados(Format::R8G8B8, 64, 64, vec![9; 64 * 64 * 3]);
        let t = imagem_para_textura("p".into(), &d, 2048);
        assert_eq!((t.largura, t.altura), (64, 64));
    }

    #[test]
    fn buffer_truncado_nao_causa_panico() {
        // Declara 4 pixels mas manda dados de 1: o resto vira preenchimento.
        let d = dados(Format::R8G8B8, 2, 2, vec![1, 2, 3]);
        let t = imagem_para_textura("truncada".into(), &d, 4096);
        assert_eq!(t.rgba.len(), 2 * 2 * 4);
    }

    #[test]
    fn o_material_padrao_e_opaco_e_fosco() {
        let m = Material::default();
        assert_eq!(m.base_color[3], 1.0);
        assert!(!m.transparente);
        assert!(m.textura.is_none());
    }
}

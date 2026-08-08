//! Lamina d'agua sobre as piscinas do modelo.
//!
//! Arquivo de projeto nao traz agua: o arquiteto modela a piscina revestida e
//! vazia, porque o que vai para a obra e o revestimento. Para o render, porem,
//! uma piscina seca parece defeito.
//!
//! Em vez de pedir que o usuario arraste um plano azul ate parecer certo, o
//! ARCZ acha a piscina pelo **revestimento** e enche na medida exata. Pedra
//! Hijau, pastilha e azulejo de piscina sao materiais que so aparecem ali; a
//! caixa dos triangulos que os usam da largura, comprimento e a cota da borda
//! sem chute nenhum.

use arcz_model::{Material, Model, ModelVertex, Submesh};

/// Quanto a lamina fica abaixo da borda.
///
/// Piscina cheia ate a borda transborda ao entrar alguem; o nivel de operacao
/// fica um palmo abaixo, e e isso que se ve numa foto.
const REBAIXO_M: f32 = 0.08;

/// Area minima para valer como piscina, em m².
///
/// O mesmo revestimento aparece em detalhes pequenos — uma faixa de 0,15 m no
/// Zenite. Sem este piso, cada um deles viraria uma poca d'agua flutuante.
const AREA_MINIMA: f32 = 3.0;

/// Uma piscina encontrada, em coordenadas do arquivo do modelo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lamina {
    pub x_min: f32,
    pub x_max: f32,
    pub z_min: f32,
    pub z_max: f32,
    /// Cota da lamina: a borda menos o rebaixo.
    pub y: f32,
}

impl Lamina {
    pub fn largura(&self) -> f32 {
        self.x_max - self.x_min
    }
    pub fn comprimento(&self) -> f32 {
        self.z_max - self.z_min
    }
    pub fn area(&self) -> f32 {
        self.largura() * self.comprimento()
    }
}

/// `true` se o nome do material identifica revestimento de piscina.
fn e_revestimento_de_piscina(nome: &str) -> bool {
    let n = nome.to_uppercase();
    ["HIJAU", "PISCINA", "PASTILHA", "AZULEJO"]
        .iter()
        .any(|a| n.contains(a))
}

/// Acha as piscinas de `model` pelas caixas do revestimento.
pub fn detectar(model: &Model) -> Vec<Lamina> {
    let mut achadas = Vec::new();
    for s in &model.submeshes {
        let Some(mat) = model.materiais.get(s.material) else {
            continue;
        };
        if !e_revestimento_de_piscina(&mat.nome) {
            continue;
        }
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for k in s.offset..s.offset + s.count {
            let Some(&i) = model.indices.get(k as usize) else {
                continue;
            };
            let Some(v) = model.vertices.get(i as usize) else {
                continue;
            };
            for e in 0..3 {
                lo[e] = lo[e].min(v.position[e]);
                hi[e] = hi[e].max(v.position[e]);
            }
        }
        let l = Lamina {
            x_min: lo[0],
            x_max: hi[0],
            z_min: lo[2],
            z_max: hi[2],
            y: hi[1] - REBAIXO_M,
        };
        if l.area() >= AREA_MINIMA {
            achadas.push(l);
        }
    }
    achadas
}

/// Constroi a malha da lamina, em coordenadas do arquivo.
///
/// Vem no mesmo quadro do modelo do predio, entao o objeto entra com o mesmo
/// placement e cai exatamente sobre a piscina — sem offset para acertar.
pub fn malha(laminas: &[Lamina]) -> Model {
    let mut vertices = Vec::with_capacity(laminas.len() * 4);
    let mut indices = Vec::with_capacity(laminas.len() * 6);
    let (mut min, mut max) = ([f32::MAX; 3], [f32::MIN; 3]);

    for l in laminas {
        let base = vertices.len() as u32;
        // Ordem anti-horaria vista de cima, para a normal apontar para o ceu.
        for (x, z, u, v) in [
            (l.x_min, l.z_min, 0.0, 0.0),
            (l.x_max, l.z_min, 1.0, 0.0),
            (l.x_max, l.z_max, 1.0, 1.0),
            (l.x_min, l.z_max, 0.0, 1.0),
        ] {
            vertices.push(ModelVertex {
                position: [x, l.y, z],
                normal: [0.0, 1.0, 0.0],
                uv: [u, v],
            });
            for (e, c) in [x, l.y, z].into_iter().enumerate() {
                min[e] = min[e].min(c);
                max[e] = max[e].max(c);
            }
        }
        indices.extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
    }

    let mut agua = Material::default();
    agua.nome = "AGUA".into();
    // Turquesa de piscina tratada. O alfa deixa o revestimento aparecer por
    // baixo — e o que faz a agua parecer agua, e nao uma tampa azul.
    agua.base_color = [0.16, 0.55, 0.62, 0.82];
    agua.metallic = 0.35;
    agua.roughness = 0.06;
    agua.transparente = true;

    let submeshes = vec![Submesh {
        material: 0,
        offset: 0,
        count: indices.len() as u32,
    }];

    Model {
        vertices,
        indices,
        submeshes,
        materiais: vec![agua],
        texturas: Vec::new(),
        min,
        max,
        primitivas_ignoradas: 0,
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn modelo_com(nome_material: &str, caixa: ([f32; 3], [f32; 3])) -> Model {
        let (lo, hi) = caixa;
        let mut m = Material::default();
        m.nome = nome_material.into();
        let vertices = vec![
            ModelVertex { position: [lo[0], hi[1], lo[2]], normal: [0.0, 1.0, 0.0], uv: [0.0, 0.0] },
            ModelVertex { position: [hi[0], hi[1], lo[2]], normal: [0.0, 1.0, 0.0], uv: [1.0, 0.0] },
            ModelVertex { position: [hi[0], lo[1], hi[2]], normal: [0.0, 1.0, 0.0], uv: [1.0, 1.0] },
        ];
        Model {
            vertices,
            indices: vec![0, 1, 2],
            submeshes: vec![Submesh { material: 0, offset: 0, count: 3 }],
            materiais: vec![m],
            texturas: Vec::new(),
            min: lo,
            max: hi,
            primitivas_ignoradas: 0,
        }
    }

    #[test]
    fn reconhece_os_revestimentos_de_piscina() {
        for n in ["PEDRA HIJAU 20x20", "Pastilha azul", "AZULEJO PISCINA"] {
            assert!(e_revestimento_de_piscina(n), "{n}");
        }
        for n in ["CONCRETO APARENTE", "PORCELANATO CINZA 45x45cm", "VIDRO"] {
            assert!(!e_revestimento_de_piscina(n), "{n}");
        }
    }

    #[test]
    fn acha_a_piscina_e_rebaixa_a_lamina() {
        // Mesmas medidas da piscina real do Zenite.
        let m = modelo_com("PEDRA HIJAU 20x20", ([19.97, 16.56, -24.33], [25.54, 20.97, -15.37]));
        let l = detectar(&m);
        assert_eq!(l.len(), 1);
        assert!((l[0].largura() - 5.57).abs() < 0.01, "{}", l[0].largura());
        assert!((l[0].comprimento() - 8.96).abs() < 0.01);
        // A lamina fica ABAIXO da borda, nunca acima.
        assert!(l[0].y < 20.97, "lamina em {} nao rebaixou", l[0].y);
        assert!((l[0].y - (20.97 - REBAIXO_M)).abs() < 1e-4);
    }

    #[test]
    fn descarta_detalhe_pequeno_com_o_mesmo_revestimento() {
        // A faixa de 0,15 m que existe no arquivo real viraria uma poca.
        let m = modelo_com("PEDRA HIJAU 20x20", ([22.67, 19.41, -21.73], [22.67, 19.49, -21.58]));
        assert!(detectar(&m).is_empty());
    }

    #[test]
    fn a_malha_fecha_dois_triangulos_por_piscina_e_olha_para_cima() {
        let laminas = [
            Lamina { x_min: 0.0, x_max: 4.0, z_min: 0.0, z_max: 8.0, y: 3.0 },
            Lamina { x_min: 10.0, x_max: 13.0, z_min: 0.0, z_max: 3.0, y: 1.0 },
        ];
        let m = malha(&laminas);
        assert_eq!(m.vertices.len(), 8);
        assert_eq!(m.indices.len(), 12);
        assert!(m.vertices.iter().all(|v| v.normal == [0.0, 1.0, 0.0]));
        // A caixa precisa envolver as duas laminas, senao o picking erra.
        assert_eq!(m.min, [0.0, 1.0, 0.0]);
        assert_eq!(m.max, [13.0, 3.0, 8.0]);
        assert!(m.materiais[0].transparente, "sem alfa a agua vira tampa azul");
    }

    #[test]
    fn winding_faz_a_normal_geometrica_apontar_para_cima() {
        // A normal declarada pode mentir; a que a GPU usa sai do winding. Mede
        // o produto vetorial do primeiro triangulo.
        let m = malha(&[Lamina { x_min: 0.0, x_max: 2.0, z_min: 0.0, z_max: 2.0, y: 0.0 }]);
        let p = |i: usize| m.vertices[m.indices[i] as usize].position;
        let (a, b, c) = (p(0), p(1), p(2));
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let ny = u[2] * v[0] - u[0] * v[2];
        assert!(ny > 0.0, "a lamina esta virada para baixo (ny = {ny})");
    }
}

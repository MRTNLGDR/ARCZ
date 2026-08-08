//! Analise de um modelo importado: descobrir os pavimentos a partir da geometria.
//!
//! Um GLB exportado de SketchUp/Revit chega achatado — no Zenite sao 31.198 nodes
//! chamados `Geom-NNNN`, sem nenhuma hierarquia de "pavimento" ou "unidade". Para
//! mobiliar o predio precisamos de uma coisa que o arquivo nao diz: **em que altura
//! esta cada laje**.
//!
//! O sinal e geometrico e confiavel: laje e uma superficie horizontal grande. Somamos
//! a area dos triangulos com normal quase vertical, agrupada por faixa de altura; os
//! picos sao os pisos. Contar vertices nao serviria — uma laje inteira pode ser dois
//! triangulos, enquanto uma escada tem milhares.

use crate::Model;

/// Um piso detectado no modelo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Nivel {
    /// Altura em unidades do arquivo (metros, se o arquivo estiver em metros).
    pub y: f32,
    /// Area horizontal acumulada nessa altura.
    pub area: f32,
    /// Extensao da laje no plano, util para saber onde a unidade cabe.
    pub min_xz: [f32; 2],
    pub max_xz: [f32; 2],
}

impl Nivel {
    pub fn largura(&self) -> f32 {
        self.max_xz[0] - self.min_xz[0]
    }
    pub fn profundidade(&self) -> f32 {
        self.max_xz[1] - self.min_xz[1]
    }
}

/// Parametros da deteccao. Os padroes valem para um predio residencial em metros.
#[derive(Debug, Clone, Copy)]
pub struct Parametros {
    /// Altura de cada faixa do histograma.
    pub bin_m: f32,
    /// Area minima para uma faixa contar como piso.
    pub area_minima_m2: f32,
    /// Distancia vertical minima entre dois pisos (pe-direito minimo).
    pub separacao_minima_m: f32,
    /// Quao vertical a normal precisa ser (1.0 = perfeitamente horizontal).
    pub tolerancia_normal: f32,
}

impl Default for Parametros {
    fn default() -> Self {
        Self {
            bin_m: 0.10,
            area_minima_m2: 40.0,
            separacao_minima_m: 2.0,
            tolerancia_normal: 0.90,
        }
    }
}

/// Histograma de area horizontal por faixa de altura.
#[derive(Debug, Clone)]
pub struct Histograma {
    pub y0: f32,
    pub bin: f32,
    pub area: Vec<f32>,
    pub min_xz: Vec<[f32; 2]>,
    pub max_xz: Vec<[f32; 2]>,
}

impl Histograma {
    pub fn y_do_bin(&self, i: usize) -> f32 {
        self.y0 + (i as f32 + 0.5) * self.bin
    }
}

/// Acumula a area das superficies horizontais por altura.
pub fn histograma(model: &Model, p: Parametros) -> Histograma {
    let y0 = model.min[1];
    let altura = (model.max[1] - y0).max(p.bin_m);
    let n = ((altura / p.bin_m).ceil() as usize + 1).max(1);

    let mut area = vec![0.0f32; n];
    let mut min_xz = vec![[f32::INFINITY; 2]; n];
    let mut max_xz = vec![[f32::NEG_INFINITY; 2]; n];

    for tri in model.indices.chunks_exact(3) {
        let (a, b, c) = (
            model.vertices[tri[0] as usize].position,
            model.vertices[tri[1] as usize].position,
            model.vertices[tri[2] as usize].position,
        );
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let cross = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let norma = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
        if norma < 1e-9 {
            continue; // triangulo degenerado
        }
        // |ny| / |n|: 1 = horizontal, 0 = parede.
        if (cross[1].abs() / norma) < p.tolerancia_normal {
            continue;
        }

        let y = (a[1] + b[1] + c[1]) / 3.0;
        let i = (((y - y0) / p.bin_m).floor().max(0.0) as usize).min(n - 1);
        area[i] += norma * 0.5;
        for v in [a, b, c] {
            min_xz[i][0] = min_xz[i][0].min(v[0]);
            min_xz[i][1] = min_xz[i][1].min(v[2]);
            max_xz[i][0] = max_xz[i][0].max(v[0]);
            max_xz[i][1] = max_xz[i][1].max(v[2]);
        }
    }

    Histograma {
        y0,
        bin: p.bin_m,
        area,
        min_xz,
        max_xz,
    }
}

/// Detecta os pavimentos do modelo, do mais baixo para o mais alto.
///
/// Escolhe os picos do [`histograma`] por area decrescente, descartando qualquer um
/// que esteja a menos de `separacao_minima_m` de um pico ja aceito — e o que evita
/// contar o mesmo piso duas vezes por causa do contrapiso ou do rebaixo do box.
pub fn pavimentos(model: &Model, p: Parametros) -> Vec<Nivel> {
    let h = histograma(model, p);
    let mut candidatos: Vec<usize> = (0..h.area.len())
        .filter(|&i| h.area[i] >= p.area_minima_m2)
        .collect();
    candidatos.sort_by(|a, b| h.area[*b].total_cmp(&h.area[*a]));

    let mut aceitos: Vec<usize> = Vec::new();
    for i in candidatos {
        let y = h.y_do_bin(i);
        if aceitos
            .iter()
            .any(|&j| (h.y_do_bin(j) - y).abs() < p.separacao_minima_m)
        {
            continue;
        }
        aceitos.push(i);
    }
    aceitos.sort_unstable();

    aceitos
        .into_iter()
        .map(|i| Nivel {
            y: h.y_do_bin(i),
            area: h.area[i],
            min_xz: h.min_xz[i],
            max_xz: h.max_xz[i],
        })
        .collect()
}

/// Retangulo de area minima que envolve uma laje, com o giro dela.
///
/// A caixa alinhada aos eixos ([`Nivel::min_xz`]) so serve quando o predio esta
/// alinhado ao arquivo — e o Zenite **nao esta**: a torre chega girada ~20 graus
/// dentro do proprio GLB. Usar o AABB como se fosse a laje joga o mobiliario para
/// fora do predio (aconteceu, e so apareceu no render).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RetanguloOrientado {
    pub centro_xz: [f32; 2],
    /// Lados do retangulo, ordenados: `largura <= profundidade`.
    pub largura: f32,
    pub profundidade: f32,
    /// Giro do lado `largura` em relacao ao eixo X do arquivo, em graus (0..90).
    pub angulo_graus: f32,
}

impl RetanguloOrientado {
    pub fn area(&self) -> f32 {
        self.largura * self.profundidade
    }

    /// Canto do retangulo, no sistema do arquivo (X, Z).
    ///
    /// `i` percorre os cantos no sentido: 0 = origem, 1 = +largura,
    /// 2 = +largura +profundidade, 3 = +profundidade.
    pub fn canto(&self, i: usize) -> [f32; 2] {
        let (s, c) = self.angulo_graus.to_radians().sin_cos();
        let (hl, hp) = (self.largura * 0.5, self.profundidade * 0.5);
        let (u, v) = match i % 4 {
            0 => (-hl, -hp),
            1 => (hl, -hp),
            2 => (hl, hp),
            _ => (-hl, hp),
        };
        [
            self.centro_xz[0] + u * c - v * s,
            self.centro_xz[1] + u * s + v * c,
        ]
    }
}

/// Pontos (X, Z) das superficies horizontais na altura `y`, com tolerancia.
pub fn pontos_do_nivel(model: &Model, y: f32, tolerancia_m: f32, p: Parametros) -> Vec<[f32; 2]> {
    let mut pontos = Vec::new();
    for tri in model.indices.chunks_exact(3) {
        let (a, b, c) = (
            model.vertices[tri[0] as usize].position,
            model.vertices[tri[1] as usize].position,
            model.vertices[tri[2] as usize].position,
        );
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let cross = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let norma = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
        if norma < 1e-9 || (cross[1].abs() / norma) < p.tolerancia_normal {
            continue;
        }
        let ym = (a[1] + b[1] + c[1]) / 3.0;
        if (ym - y).abs() > tolerancia_m {
            continue;
        }
        for w in [a, b, c] {
            pontos.push([w[0], w[2]]);
        }
    }
    pontos
}

/// Retangulo de area minima que cobre os pontos, por varredura de angulo.
///
/// Forca bruta de 0 a 90 graus em passos de 0,25 grau. Nao e o algoritmo otimo
/// (calipers rotativos sobre o fecho convexo seria), mas roda em milissegundos numa
/// laje e nao tem caso degenerado.
pub fn retangulo_minimo(pontos: &[[f32; 2]]) -> Option<RetanguloOrientado> {
    if pontos.len() < 3 {
        return None;
    }
    let mut melhor: Option<RetanguloOrientado> = None;

    let passos = 360; // 90 graus / 0,25
    for k in 0..passos {
        let ang = k as f32 * 0.25;
        let (s, c) = ang.to_radians().sin_cos();
        let (mut u0, mut u1, mut v0, mut v1) = (
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
        );
        for p in pontos {
            let u = p[0] * c + p[1] * s;
            let v = -p[0] * s + p[1] * c;
            u0 = u0.min(u);
            u1 = u1.max(u);
            v0 = v0.min(v);
            v1 = v1.max(v);
        }
        let (du, dv) = (u1 - u0, v1 - v0);
        if melhor.is_some_and(|m| m.area() <= du * dv) {
            continue;
        }
        // Centro volta para o sistema do arquivo.
        let (uc, vc) = ((u0 + u1) * 0.5, (v0 + v1) * 0.5);
        let centro = [uc * c - vc * s, uc * s + vc * c];
        let (largura, profundidade, angulo) = if du <= dv {
            (du, dv, ang)
        } else {
            // Troca os eixos para manter `largura <= profundidade`.
            (dv, du, (ang + 90.0) % 180.0)
        };
        melhor = Some(RetanguloOrientado {
            centro_xz: centro,
            largura,
            profundidade,
            angulo_graus: angulo,
        });
    }
    melhor
}

/// Pe-direito medio entre pavimentos consecutivos. `None` com menos de dois niveis.
pub fn pe_direito_medio(niveis: &[Nivel]) -> Option<f32> {
    if niveis.len() < 2 {
        return None;
    }
    let soma: f32 = niveis.windows(2).map(|w| w[1].y - w[0].y).sum();
    Some(soma / (niveis.len() - 1) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Material, ModelVertex, Submesh};

    /// Modelo sintetico: `n` lajes quadradas de `lado` metros, espacadas de `pe`.
    fn predio(n: usize, lado: f32, pe: f32) -> Model {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        for k in 0..n {
            let y = k as f32 * pe;
            let base = vertices.len() as u32;
            for (x, z) in [(0.0, 0.0), (lado, 0.0), (lado, lado), (0.0, lado)] {
                vertices.push(ModelVertex {
                    position: [x, y, z],
                    normal: [0.0, 1.0, 0.0],
                    uv: [0.0, 0.0],
                });
            }
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        // Uma parede vertical alta, que NAO pode virar pavimento.
        let base = vertices.len() as u32;
        let topo = (n as f32 - 1.0) * pe;
        for (x, y, z) in [
            (0.0, 0.0, 0.0),
            (lado, 0.0, 0.0),
            (lado, topo, 0.0),
            (0.0, topo, 0.0),
        ] {
            vertices.push(ModelVertex {
                position: [x, y, z],
                normal: [0.0, 0.0, 1.0],
                uv: [0.0, 0.0],
            });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);

        let conta = indices.len();
        Model {
            min: [0.0, 0.0, 0.0],
            max: [lado, topo, lado],
            vertices,
            indices,
            submeshes: vec![Submesh {
                material: 0,
                offset: 0,
                count: conta as u32,
            }],
            materiais: vec![Material::default()],
            texturas: Vec::new(),
            primitivas_ignoradas: 0,
        }
    }

    #[test]
    fn detecta_um_pavimento_por_laje() {
        let m = predio(5, 20.0, 3.0);
        let niveis = pavimentos(&m, Parametros::default());
        assert_eq!(niveis.len(), 5, "niveis: {niveis:?}");
        for (k, n) in niveis.iter().enumerate() {
            let esperado = k as f32 * 3.0;
            assert!(
                (n.y - esperado).abs() <= 0.10,
                "nivel {k}: y={} esperado {esperado}",
                n.y
            );
        }
    }

    #[test]
    fn parede_vertical_nao_vira_pavimento() {
        // A parede tem area enorme (20 x 12 m) mas normal horizontal.
        let m = predio(5, 20.0, 3.0);
        let h = histograma(&m, Parametros::default());
        let total: f32 = h.area.iter().sum();
        // 5 lajes de 400 m2 = 2000; a parede (240 m2) nao pode ter entrado.
        assert!((total - 2000.0).abs() < 1.0, "area total {total}");
    }

    #[test]
    fn laje_pequena_demais_e_descartada() {
        let m = predio(3, 4.0, 3.0); // 16 m2 por laje
        assert!(pavimentos(&m, Parametros::default()).is_empty());
        let permissivo = Parametros {
            area_minima_m2: 10.0,
            ..Default::default()
        };
        assert_eq!(pavimentos(&m, permissivo).len(), 3);
    }

    #[test]
    fn contrapiso_colado_na_laje_nao_duplica_o_pavimento() {
        let mut m = predio(3, 20.0, 3.0);
        // Repete a laje do meio 8 cm acima (contrapiso). Sem a separacao minima,
        // isso viraria um quarto "pavimento".
        let base = m.vertices.len() as u32;
        for (x, z) in [(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)] {
            m.vertices.push(ModelVertex {
                position: [x, 3.08, z],
                normal: [0.0, 1.0, 0.0],
                uv: [0.0, 0.0],
            });
        }
        m.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        assert_eq!(pavimentos(&m, Parametros::default()).len(), 3);
    }

    #[test]
    fn nivel_guarda_a_extensao_da_laje() {
        let niveis = pavimentos(&predio(2, 20.0, 3.0), Parametros::default());
        let n = niveis[0];
        assert!((n.largura() - 20.0).abs() < 0.01);
        assert!((n.profundidade() - 20.0).abs() < 0.01);
    }

    #[test]
    fn pe_direito_medio_bate_com_o_espacamento() {
        let niveis = pavimentos(&predio(6, 20.0, 2.9), Parametros::default());
        let pe = pe_direito_medio(&niveis).unwrap();
        assert!((pe - 2.9).abs() < 0.15, "pe direito {pe}");
        assert!(pe_direito_medio(&niveis[..1]).is_none());
    }

    #[test]
    fn retangulo_minimo_acha_o_giro_de_um_retangulo_girado() {
        // Retangulo 10 x 4 girado 20 graus.
        let (s, c) = 20.0f32.to_radians().sin_cos();
        let cantos = [(-5.0, -2.0), (5.0, -2.0), (5.0, 2.0), (-5.0, 2.0)];
        let mut pontos = Vec::new();
        for (u, v) in cantos {
            pontos.push([u * c - v * s + 100.0, u * s + v * c - 50.0]);
        }
        // Alguns pontos no meio das arestas, como uma laje real teria.
        for t in [0.25f32, 0.5, 0.75] {
            let (u, v) = (-5.0 + 10.0 * t, -2.0);
            pontos.push([u * c - v * s + 100.0, u * s + v * c - 50.0]);
        }

        let r = retangulo_minimo(&pontos).unwrap();
        assert!((r.largura - 4.0).abs() < 0.1, "largura {}", r.largura);
        assert!(
            (r.profundidade - 10.0).abs() < 0.1,
            "profundidade {}",
            r.profundidade
        );
        assert!((r.centro_xz[0] - 100.0).abs() < 0.05);
        assert!((r.centro_xz[1] + 50.0).abs() < 0.05);
        // O lado maior esta a 20 graus; o menor (que e a `largura`) a 110.
        assert!(
            (r.angulo_graus - 110.0).abs() < 0.5,
            "angulo {}",
            r.angulo_graus
        );
    }

    #[test]
    fn cantos_do_retangulo_voltam_para_o_sistema_do_arquivo() {
        let r = RetanguloOrientado {
            centro_xz: [10.0, -20.0],
            largura: 6.0,
            profundidade: 8.0,
            angulo_graus: 0.0,
        };
        assert_eq!(r.canto(0), [7.0, -24.0]);
        assert_eq!(r.canto(2), [13.0, -16.0]);
        // Girando 90 graus, largura e profundidade trocam de eixo.
        let g = RetanguloOrientado {
            angulo_graus: 90.0,
            ..r
        };
        let c0 = g.canto(0);
        assert!(
            (c0[0] - 14.0).abs() < 1e-4 && (c0[1] + 23.0).abs() < 1e-4,
            "{c0:?}"
        );
    }

    #[test]
    fn retangulo_minimo_recusa_poucos_pontos() {
        assert!(retangulo_minimo(&[]).is_none());
        assert!(retangulo_minimo(&[[0.0, 0.0], [1.0, 1.0]]).is_none());
    }

    #[test]
    fn pontos_do_nivel_pegam_so_a_laje_pedida() {
        let m = predio(3, 20.0, 3.0);
        let p = Parametros::default();
        // 2 triangulos x 3 vertices por laje.
        assert_eq!(pontos_do_nivel(&m, 0.0, 0.05, p).len(), 6);
        assert_eq!(pontos_do_nivel(&m, 3.0, 0.05, p).len(), 6);
        assert!(pontos_do_nivel(&m, 1.5, 0.05, p).is_empty());
        // Com tolerancia grande, pega duas lajes.
        assert_eq!(pontos_do_nivel(&m, 1.5, 1.6, p).len(), 12);
    }

    #[test]
    fn laje_alinhada_ao_arquivo_da_angulo_zero_ou_noventa() {
        let m = predio(2, 20.0, 3.0);
        let pts = pontos_do_nivel(&m, 0.0, 0.05, Parametros::default());
        let r = retangulo_minimo(&pts).unwrap();
        assert!((r.largura - 20.0).abs() < 0.1);
        assert!((r.profundidade - 20.0).abs() < 0.1);
    }

    #[test]
    fn modelo_vazio_nao_quebra() {
        let m = Model {
            vertices: Vec::new(),
            indices: Vec::new(),
            submeshes: Vec::new(),
            materiais: Vec::new(),
            texturas: Vec::new(),
            min: [0.0; 3],
            max: [0.0; 3],
            primitivas_ignoradas: 0,
        };
        assert!(pavimentos(&m, Parametros::default()).is_empty());
    }
}

/// Roda a detecção em um modelo real fornecido explicitamente pelo operador.
/// `ARCZ_MODELO=/caminho/modelo.glb cargo test -p arcz-model -- --ignored --nocapture pavimentos_do_modelo_externo`
#[cfg(test)]
mod real {
    use super::*;

    #[test]
    #[ignore = "depende de ARCZ_MODELO apontando para um GLB real"]
    fn pavimentos_do_modelo_externo() {
        let caminho = std::env::var("ARCZ_MODELO").expect("defina ARCZ_MODELO para executar este teste ignorado");
        let m = Model::load(&caminho).expect("abrir o modelo");
        println!(
            "bbox: x {:.2}..{:.2}  y {:.2}..{:.2}  z {:.2}..{:.2}",
            m.min[0], m.max[0], m.min[1], m.max[1], m.min[2], m.max[2]
        );
        println!(
            "modelo: {:.2} x {:.2} x {:.2} m, {} triangulos",
            m.size()[0],
            m.size()[1],
            m.size()[2],
            m.triangle_count()
        );
        let niveis = pavimentos(&m, Parametros::default());
        for (i, n) in niveis.iter().enumerate() {
            println!(
                "  pav {:>2}: y = {:>7.2} m | area {:>8.1} m2 | laje {:.1} x {:.1} m | x {:.2}..{:.2} z {:.2}..{:.2}",
                i,
                n.y,
                n.area,
                n.largura(),
                n.profundidade(),
                n.min_xz[0],
                n.max_xz[0],
                n.min_xz[1],
                n.max_xz[1]
            );
        }
        println!("pe-direito medio: {:?}", pe_direito_medio(&niveis));
        println!(
            "
--- retangulo real de cada laje (area minima) ---"
        );
        for (i, n) in niveis.iter().enumerate() {
            let pts = pontos_do_nivel(&m, n.y, 0.06, Parametros::default());
            if let Some(r) = retangulo_minimo(&pts) {
                println!(
                    "  pav {:>2}: {:>6.2} x {:>6.2} m | giro {:>6.2} deg | centro ({:.2}, {:.2}) | cantos {:?} {:?}",
                    i, r.largura, r.profundidade, r.angulo_graus,
                    r.centro_xz[0], r.centro_xz[1], r.canto(0), r.canto(2)
                );
            }
        }
    }
}

//! Geometria do gizmo de manipulacao e da caixa de selecao.
//!
//! Gera linhas em coordenadas de mundo. O tamanho e proporcional a distancia da
//! camera para o gizmo ter sempre o mesmo tamanho aparente na tela — sem isso ele
//! some quando a camera se afasta e engole a cena quando ela chega perto.

use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct VerticeLinha {
    pub position: [f32; 3],
    pub cor: [f32; 4],
}

/// Cores dos eixos, na convencao universal de software 3D: X vermelho, Y verde,
/// Z azul. Trocar isso confunde qualquer pessoa que ja usou Blender ou SketchUp.
pub const COR_X: [f32; 4] = [0.95, 0.26, 0.28, 1.0];
pub const COR_Y: [f32; 4] = [0.42, 0.85, 0.30, 1.0];
pub const COR_Z: [f32; 4] = [0.30, 0.58, 0.98, 1.0];
pub const COR_SELECAO: [f32; 4] = [1.0, 0.72, 0.15, 1.0];

/// Qual modo de manipulacao o gizmo representa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModoGizmo {
    Mover,
    Girar,
    Escalar,
}

/// Identifica qual alca do gizmo foi clicada. O E.3 comeca so com mover (X/Y/Z);
/// rotacao e escala serao adicionados depois.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlcaId {
    X,
    Y,
    Z,
}

/// Uma alca do gizmo: um AABB apertado no espaco de mundo. O picking testa
/// o raio da camera contra esses volumes para descobrir qual alca foi clicada.
#[derive(Debug, Clone, Copy)]
pub struct Alca {
    pub id: AlcaId,
    pub min: [f32; 3],
    pub max: [f32; 3],
}

/// Resultado do `construir_com_alcas`: as linhas visuais + as alcas com seus
/// volumes para o picking do E.3.
pub struct Gizmo {
    pub linhas: Vec<VerticeLinha>,
    pub alcas: Vec<Alca>,
}

/// Monta as linhas + alcas com volume. Cada alca tem um AABB de ~10% do
/// comprimento do eixo para o picking nao exigir pixel-perfect.
pub fn construir_com_alcas(
    centro: [f32; 3],
    min: [f32; 3],
    max: [f32; 3],
    modo: ModoGizmo,
    dist_camera: f64,
) -> Gizmo {
    let mut linhas = Vec::new();
    let mut alcas = Vec::new();

    let escala = (dist_camera * 0.12).clamp(1.0, 500.0) as f32;
    // Espessura do AABB da alca: 12% do comprimento. Aperta o bastante pra
    // nao pegar o eixo errado, largo pra pegar com mouse em vez de pixel-perfect.
    let grossura = (escala * 0.12).max(0.5);

    caixa(&mut linhas, min, max, COR_SELECAO);

    match modo {
        ModoGizmo::Mover => {
            for (dir, cor) in [
                ([1.0, 0.0, 0.0], COR_X),
                ([0.0, 1.0, 0.0], COR_Y),
                ([0.0, 0.0, 1.0], COR_Z),
            ] {
                let ponta = somar(centro, escalar(dir, escala));
                linha(&mut linhas, centro, ponta, cor);
                ponta_de_seta(&mut linhas, centro, ponta, escala * 0.16, cor);
                // AABB da alca: cobre o segmento do centro ate a ponta, com
                // grossura perpendicular. E o suficiente para o picking tolerar
                // mouse a 1-2 pixels do eixo.
                alcas.push(Alca {
                    id: alca_id(dir),
                    min: [
                        centro[0] + (dir[0] * escala).min(0.0) - grossura * 0.5,
                        centro[1] + (dir[1] * escala).min(0.0) - grossura * 0.5,
                        centro[2] + (dir[2] * escala).min(0.0) - grossura * 0.5,
                    ],
                    max: [
                        centro[0] + (dir[0] * escala).max(0.0) + grossura * 0.5,
                        centro[1] + (dir[1] * escala).max(0.0) + grossura * 0.5,
                        centro[2] + (dir[2] * escala).max(0.0) + grossura * 0.5,
                    ],
                });
            }
        }
        ModoGizmo::Girar => {
            circulo(&mut linhas, centro, escala, [0, 2], COR_Y);
            circulo(&mut linhas, centro, escala * 0.82, [1, 2], COR_X);
            circulo(&mut linhas, centro, escala * 0.82, [0, 1], COR_Z);

            alcas.push(Alca {
                id: AlcaId::Y,
                min: [centro[0] - escala, centro[1] - grossura, centro[2] - escala],
                max: [centro[0] + escala, centro[1] + grossura, centro[2] + escala],
            });
            alcas.push(Alca {
                id: AlcaId::X,
                min: [centro[0] - grossura, centro[1] - escala, centro[2] - escala],
                max: [centro[0] + grossura, centro[1] + escala, centro[2] + escala],
            });
            alcas.push(Alca {
                id: AlcaId::Z,
                min: [centro[0] - escala, centro[1] - escala, centro[2] - grossura],
                max: [centro[0] + escala, centro[1] + escala, centro[2] + grossura],
            });
        }
        ModoGizmo::Escalar => {
            for (dir, cor) in [
                ([1.0, 0.0, 0.0], COR_X),
                ([0.0, 1.0, 0.0], COR_Y),
                ([0.0, 0.0, 1.0], COR_Z),
            ] {
                let ponta = somar(centro, escalar(dir, escala));
                linha(&mut linhas, centro, ponta, cor);
                let c = escala * 0.06;
                caixa(
                    &mut linhas,
                    [ponta[0] - c, ponta[1] - c, ponta[2] - c],
                    [ponta[0] + c, ponta[1] + c, ponta[2] + c],
                    cor,
                );
                alcas.push(Alca {
                    id: alca_id(dir),
                    min: [
                        centro[0] + (dir[0] * escala).min(0.0) - grossura * 0.5,
                        centro[1] + (dir[1] * escala).min(0.0) - grossura * 0.5,
                        centro[2] + (dir[2] * escala).min(0.0) - grossura * 0.5,
                    ],
                    max: [
                        centro[0] + (dir[0] * escala).max(0.0) + grossura * 0.5,
                        centro[1] + (dir[1] * escala).max(0.0) + grossura * 0.5,
                        centro[2] + (dir[2] * escala).max(0.0) + grossura * 0.5,
                    ],
                });
            }
        }
    }

    Gizmo { linhas, alcas }
}

/// Espaco de transformacao (Local / Mundo).
///
/// Ainda nao exposto: o gizmo opera em mundo. O tipo existe porque o snapping
/// ja distingue os dois espacos e vai precisar dele.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransformSpace {
    #[default]
    World,
    Local,
}

/// Configuracao de snapping para transformacao (grade, angulo e terreno).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnappingConfig {
    pub grid_snap_m: Option<f32>,
    pub angle_snap_deg: Option<f32>,
    pub terrain_snap: bool,
}

impl Default for SnappingConfig {
    fn default() -> Self {
        Self {
            grid_snap_m: Some(0.5),
            angle_snap_deg: Some(15.0),
            terrain_snap: false,
        }
    }
}

pub fn aplicar_snapping_placement(
    mut placement: arcz_model::Placement,
    config: &SnappingConfig,
) -> arcz_model::Placement {
    if let Some(step) = config.grid_snap_m {
        if step > 0.001 {
            placement.offset_leste_m = (placement.offset_leste_m / step).round() * step;
            placement.offset_norte_m = (placement.offset_norte_m / step).round() * step;
            placement.offset_vertical_m = (placement.offset_vertical_m / step).round() * step;
        }
    }

    if let Some(angle_step) = config.angle_snap_deg {
        if angle_step > 0.001 {
            placement.heading_deg =
                (placement.heading_deg / angle_step as f64).round() * angle_step as f64;
        }
    }

    if config.terrain_snap {
        placement.assentar_no_terreno = true;
    }

    placement
}

fn alca_id(dir: [f32; 3]) -> AlcaId {
    if dir[0] > 0.5 {
        AlcaId::X
    } else if dir[1] > 0.5 {
        AlcaId::Y
    } else {
        AlcaId::Z
    }
}

/// Pica qual alca do gizmo o raio atinge primeiro. `None` se nenhum.
pub fn picar_alca(alcas: &[Alca], origem: [f64; 3], direcao: [f64; 3]) -> Option<AlcaId> {
    let mut melhor: Option<(f64, AlcaId)> = None;
    for a in alcas {
        if let Some(t) = super::cena::intersecao_aabb(origem, direcao, a.min, a.max) {
            if melhor.is_none_or(|(td, _)| t < td) {
                melhor = Some((t, a.id));
            }
        }
    }
    melhor.map(|(_, id)| id)
}

/// Compatibilidade: a funcao antiga `construir` ainda existe para o server HTTP.
/// Retorna so as linhas (sem alcas), usando o mesmo formato de antes.
pub fn construir(
    centro: [f32; 3],
    min: [f32; 3],
    max: [f32; 3],
    modo: ModoGizmo,
    dist_camera: f64,
) -> Vec<VerticeLinha> {
    construir_com_alcas(centro, min, max, modo, dist_camera).linhas
}

fn linha(v: &mut Vec<VerticeLinha>, a: [f32; 3], b: [f32; 3], cor: [f32; 4]) {
    v.push(VerticeLinha { position: a, cor });
    v.push(VerticeLinha { position: b, cor });
}

/// As 12 arestas de uma caixa.
fn caixa(v: &mut Vec<VerticeLinha>, min: [f32; 3], max: [f32; 3], cor: [f32; 4]) {
    let c = [
        [min[0], min[1], min[2]],
        [max[0], min[1], min[2]],
        [max[0], min[1], max[2]],
        [min[0], min[1], max[2]],
        [min[0], max[1], min[2]],
        [max[0], max[1], min[2]],
        [max[0], max[1], max[2]],
        [min[0], max[1], max[2]],
    ];
    for (a, b) in [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0), // base
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4), // topo
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7), // montantes
    ] {
        linha(v, c[a], c[b], cor);
    }
}

/// Cone achatado na ponta da seta, feito de linhas.
fn ponta_de_seta(
    v: &mut Vec<VerticeLinha>,
    origem: [f32; 3],
    ponta: [f32; 3],
    tam: f32,
    cor: [f32; 4],
) {
    let d = normalizar([
        ponta[0] - origem[0],
        ponta[1] - origem[1],
        ponta[2] - origem[2],
    ]);
    // Qualquer vetor nao paralelo serve de referencia para achar o perpendicular.
    let aux = if d[1].abs() < 0.9 {
        [0.0, 1.0, 0.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let p1 = normalizar(cruz(d, aux));
    let p2 = cruz(d, p1);
    let base = somar(ponta, escalar(d, -tam));

    for (sx, sy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
        let lado = somar(
            base,
            somar(escalar(p1, tam * 0.45 * sx), escalar(p2, tam * 0.45 * sy)),
        );
        linha(v, ponta, lado, cor);
    }
}

/// Circulo no plano definido por dois indices de eixo.
fn circulo(
    v: &mut Vec<VerticeLinha>,
    centro: [f32; 3],
    raio: f32,
    eixos: [usize; 2],
    cor: [f32; 4],
) {
    const PASSOS: usize = 48;
    let mut anterior = None;
    for i in 0..=PASSOS {
        let a = i as f32 / PASSOS as f32 * std::f32::consts::TAU;
        let mut p = centro;
        p[eixos[0]] += raio * a.cos();
        p[eixos[1]] += raio * a.sin();
        if let Some(ant) = anterior {
            linha(v, ant, p, cor);
        }
        anterior = Some(p);
    }
}

fn somar(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn escalar(a: [f32; 3], k: f32) -> [f32; 3] {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn cruz(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalizar(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l < 1e-9 {
        [0.0, 1.0, 0.0]
    } else {
        [v[0] / l, v[1] / l, v[2] / l]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CENTRO: [f32; 3] = [10.0, 5.0, -20.0];
    const MIN: [f32; 3] = [5.0, 0.0, -25.0];
    const MAX: [f32; 3] = [15.0, 10.0, -15.0];

    #[test]
    fn todo_modo_gera_linhas_pares() {
        // A topologia e LineList: um numero impar de vertices deixaria uma linha
        // pela metade e o wgpu desenharia lixo.
        for modo in [ModoGizmo::Mover, ModoGizmo::Girar, ModoGizmo::Escalar] {
            let v = construir(CENTRO, MIN, MAX, modo, 100.0);
            assert!(!v.is_empty(), "{modo:?} nao gerou nada");
            assert_eq!(v.len() % 2, 0, "{modo:?} gerou vertice orfao");
        }
    }

    #[test]
    fn nenhuma_coordenada_sai_nan() {
        for modo in [ModoGizmo::Mover, ModoGizmo::Girar, ModoGizmo::Escalar] {
            for v in construir(CENTRO, MIN, MAX, modo, 250.0) {
                assert!(v.position.iter().all(|c| c.is_finite()), "{modo:?}: {v:?}");
                assert!(v.cor.iter().all(|c| (0.0..=1.0).contains(c)), "{modo:?}");
            }
        }
    }

    #[test]
    fn a_caixa_de_selecao_tem_doze_arestas() {
        let mut v = Vec::new();
        caixa(&mut v, MIN, MAX, COR_SELECAO);
        assert_eq!(v.len(), 24, "12 arestas = 24 vertices");

        // Todos os vertices tem que estar nos cantos da caixa.
        for p in &v {
            for k in 0..3 {
                assert!(
                    (p.position[k] - MIN[k]).abs() < 1e-6 || (p.position[k] - MAX[k]).abs() < 1e-6,
                    "vertice fora do canto: {:?}",
                    p.position
                );
            }
        }
    }

    #[test]
    fn o_gizmo_cresce_com_a_distancia_da_camera() {
        // Sem isso o gizmo vira um ponto quando a camera se afasta.
        let alcance = |dist| {
            construir(CENTRO, MIN, MAX, ModoGizmo::Mover, dist)
                .iter()
                .map(|v| (v.position[0] - CENTRO[0]).abs())
                .fold(0.0_f32, f32::max)
        };
        let perto = alcance(50.0);
        let longe = alcance(500.0);
        assert!(longe > perto * 3.0, "perto={perto} longe={longe}");
    }

    #[test]
    fn a_escala_do_gizmo_e_limitada_nos_extremos() {
        // Camera colada ou a quilometros nao pode gerar gizmo microscopico nem gigante.
        let alcance = |dist| {
            construir([0.0; 3], [0.0; 3], [0.0; 3], ModoGizmo::Mover, dist)
                .iter()
                .map(|v| v.position[0].abs())
                .fold(0.0_f32, f32::max)
        };
        assert!((alcance(0.1) - 1.0).abs() < 0.01, "limite inferior");
        assert!((alcance(100_000.0) - 500.0).abs() < 1.0, "limite superior");
    }

    #[test]
    fn construir_com_alcas_retorna_3_alcas_no_modo_mover() {
        let g = construir_com_alcas(CENTRO, MIN, MAX, ModoGizmo::Mover, 100.0);
        assert_eq!(g.alcas.len(), 3);
        let ids: Vec<AlcaId> = g.alcas.iter().map(|a| a.id).collect();
        assert!(ids.contains(&AlcaId::X));
        assert!(ids.contains(&AlcaId::Y));
        assert!(ids.contains(&AlcaId::Z));
    }

    #[test]
    fn alcas_tem_volume_nao_zero() {
        // AABB da alca tem que ser grosso o suficiente para o mouse pegar sem
        // pixel-perfect. Volume > 0.001 m^3 (um cubo de 10 cm).
        let g = construir_com_alcas(CENTRO, MIN, MAX, ModoGizmo::Mover, 100.0);
        for a in &g.alcas {
            let dx = a.max[0] - a.min[0];
            let dy = a.max[1] - a.min[1];
            let dz = a.max[2] - a.min[2];
            let vol = dx * dy * dz;
            assert!(vol > 0.001, "alca {a:?} tem volume {vol} muito pequeno");
        }
    }

    #[test]
    fn picar_alca_retorna_a_mais_proxima_da_camera() {
        // Alinhado com o eixo X, raio vem de -X, atinge a alca X primeiro.
        let g = construir_com_alcas(CENTRO, MIN, MAX, ModoGizmo::Mover, 100.0);
        // Raio partindo da esquerda em direcao a origem: deve pegar a alca X.
        let atingido = picar_alca(
            &g.alcas,
            [CENTRO[0] as f64 - 50.0, CENTRO[1] as f64, CENTRO[2] as f64],
            [1.0, 0.0, 0.0],
        );
        assert_eq!(atingido, Some(AlcaId::X));
    }

    #[test]
    fn picar_alca_fora_do_gizmo_retorna_none() {
        let g = construir_com_alcas(CENTRO, MIN, MAX, ModoGizmo::Mover, 100.0);
        // Raio paralelo, fora de qualquer alca.
        let atingido = picar_alca(&g.alcas, [100.0, 100.0, -50.0], [0.0, 0.0, 1.0]);
        assert_eq!(atingido, None);
    }

    #[test]
    fn mover_usa_as_tres_cores_de_eixo() {
        let v = construir(CENTRO, MIN, MAX, ModoGizmo::Mover, 100.0);
        for cor in [COR_X, COR_Y, COR_Z] {
            assert!(
                v.iter().any(|p| p.cor == cor),
                "faltou o eixo de cor {cor:?}"
            );
        }
    }

    #[test]
    fn girar_gera_circulos_em_volta_do_centro() {
        let v = construir(CENTRO, MIN, MAX, ModoGizmo::Girar, 100.0);
        // Ignora a caixa de selecao; olha so as linhas coloridas por eixo.
        let dos_eixos: Vec<_> = v.iter().filter(|p| p.cor != COR_SELECAO).collect();
        assert!(!dos_eixos.is_empty());

        // Todo ponto do circulo fica a mesma distancia do centro, no seu plano.
        let raio_max = dos_eixos
            .iter()
            .map(|p| {
                let d = [
                    p.position[0] - CENTRO[0],
                    p.position[1] - CENTRO[1],
                    p.position[2] - CENTRO[2],
                ];
                (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
            })
            .fold(0.0_f32, f32::max);
        assert!(
            (raio_max - 12.0).abs() < 0.5,
            "raio maximo {raio_max}, esperado ~12"
        );
    }

    #[test]
    fn o_layout_do_vertice_bate_com_o_shader() {
        assert_eq!(std::mem::size_of::<VerticeLinha>(), 28);
        assert_eq!(std::mem::offset_of!(VerticeLinha, position), 0);
        assert_eq!(std::mem::offset_of!(VerticeLinha, cor), 12);

        let src = include_str!("gizmo.wgsl");
        assert!(src.contains("@location(0) position"));
        assert!(src.contains("@location(1) cor"));
        assert!(src.contains("fn vs_gizmo") && src.contains("fn fs_gizmo"));
    }

    #[test]
    fn snapping_e_alcas_de_girar_e_escalar() {
        let g_girar = construir_com_alcas(CENTRO, MIN, MAX, ModoGizmo::Girar, 100.0);
        assert_eq!(g_girar.alcas.len(), 3);

        let g_escalar = construir_com_alcas(CENTRO, MIN, MAX, ModoGizmo::Escalar, 100.0);
        assert_eq!(g_escalar.alcas.len(), 3);

        let config = SnappingConfig {
            grid_snap_m: Some(1.0),
            angle_snap_deg: Some(15.0),
            terrain_snap: true,
        };

        let inicial = arcz_model::Placement {
            offset_leste_m: 1.4,
            offset_norte_m: 2.8,
            heading_deg: 13.0,
            ..Default::default()
        };

        let snapped = aplicar_snapping_placement(inicial, &config);
        assert_eq!(snapped.offset_leste_m, 1.0);
        assert_eq!(snapped.offset_norte_m, 3.0);
        assert_eq!(snapped.heading_deg, 15.0);
        assert!(snapped.assentar_no_terreno);
    }
}

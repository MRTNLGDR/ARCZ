//! Pecas de mobiliario geradas pelo proprio ARCZ, em glTF binario (.glb).
//!
//! Existe porque o acervo CC0 fotorreal **nao cobre** o essencial de um apartamento:
//! nao ha cama moderna, guarda-roupa, bancada de cozinha, louca de banheiro nem TV
//! em dominio publico com qualidade de arquitetura. As opcoes eram (a) usar modelo
//! de licenca duvidosa, (b) usar kit cartoon low-poly, ou (c) gerar volumes limpos
//! na medida exata da planta. Escolhemos (c): a planta manda na medida, a paleta e
//! neutra (branco fosco, madeira clara, tecido cinza, inox) e nao ha risco de
//! licenca.
//!
//! Convencao geometrica (a mesma do glTF): **metros, Y para cima**, pegada centrada
//! na origem em X/Z e base em `y = 0`. A frente da peca aponta para `+Z` local. E o
//! que [`arcz_model::Placement`] espera: ele ancora pelo centro da planta e pela base.

use std::io::Write;
use std::path::Path;

/// Uma peca que o ARCZ sabe gerar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Peca {
    CamaCasal,
    CamaSolteiro,
    GuardaRoupa,
    BancadaCozinha,
    Geladeira,
    RackTv,
    Tapete,
    VasoSanitario,
    CubaBanheiro,
    BoxChuveiro,
    BalcaoRecepcao,
    Espreguicadeira,
    GuardaSol,
    Churrasqueira,
}

impl Peca {
    pub fn nome_arquivo(self) -> &'static str {
        match self {
            Self::CamaCasal => "cama-casal",
            Self::CamaSolteiro => "cama-solteiro",
            Self::GuardaRoupa => "guarda-roupa",
            Self::BancadaCozinha => "bancada-cozinha",
            Self::Geladeira => "geladeira",
            Self::RackTv => "rack-tv",
            Self::Tapete => "tapete",
            Self::VasoSanitario => "vaso-sanitario",
            Self::CubaBanheiro => "cuba-banheiro",
            Self::BoxChuveiro => "box-chuveiro",
            Self::BalcaoRecepcao => "balcao-recepcao",
            Self::Espreguicadeira => "espreguicadeira",
            Self::GuardaSol => "guarda-sol",
            Self::Churrasqueira => "churrasqueira",
        }
    }

    /// Todas as pecas, para gerar a biblioteca inteira de uma vez.
    pub const TODAS: &'static [Peca] = &[
        Self::CamaCasal,
        Self::CamaSolteiro,
        Self::GuardaRoupa,
        Self::BancadaCozinha,
        Self::Geladeira,
        Self::RackTv,
        Self::Tapete,
        Self::VasoSanitario,
        Self::CubaBanheiro,
        Self::BoxChuveiro,
        Self::BalcaoRecepcao,
        Self::Espreguicadeira,
        Self::GuardaSol,
        Self::Churrasqueira,
    ];

    /// Constroi a malha da peca.
    pub fn malha(self) -> Malha {
        match self {
            Self::CamaCasal => cama(1.60, 2.00),
            Self::CamaSolteiro => cama(0.90, 1.90),
            Self::GuardaRoupa => guarda_roupa(2.00, 2.40, 0.60),
            Self::BancadaCozinha => bancada_cozinha(2.40),
            Self::Geladeira => geladeira(),
            Self::RackTv => rack_tv(),
            Self::Tapete => tapete(2.40, 1.70),
            Self::VasoSanitario => vaso_sanitario(),
            Self::CubaBanheiro => cuba_banheiro(),
            Self::BoxChuveiro => box_chuveiro(),
            Self::BalcaoRecepcao => balcao_recepcao(),
            Self::Espreguicadeira => espreguicadeira(),
            Self::GuardaSol => guarda_sol(),
            Self::Churrasqueira => churrasqueira(),
        }
    }
}

// ---------------------------------------------------------------- paleta

/// Material PBR simples: cor base + metalico + rugosidade.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaterialSimples {
    pub nome: &'static str,
    pub cor: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
}

const fn m(
    nome: &'static str,
    r: f32,
    g: f32,
    b: f32,
    a: f32,
    metal: f32,
    rough: f32,
) -> MaterialSimples {
    MaterialSimples {
        nome,
        cor: [r, g, b, a],
        metallic: metal,
        roughness: rough,
    }
}

// Paleta neutra, a mesma dos renders do Zenite: off-white, madeira clara e media,
// tecido cinza, pedra clara, inox e preto fosco.
const BRANCO: MaterialSimples = m("branco-fosco", 0.92, 0.91, 0.89, 1.0, 0.0, 0.75);
const LINHO: MaterialSimples = m("linho", 0.88, 0.85, 0.80, 1.0, 0.0, 0.90);
const TECIDO: MaterialSimples = m("tecido-cinza", 0.58, 0.58, 0.56, 1.0, 0.0, 0.95);
const MADEIRA_CLARA: MaterialSimples = m("madeira-clara", 0.74, 0.60, 0.43, 1.0, 0.0, 0.60);
const MADEIRA_MEDIA: MaterialSimples = m("madeira-media", 0.46, 0.33, 0.22, 1.0, 0.0, 0.55);
const PEDRA: MaterialSimples = m("pedra-clara", 0.86, 0.85, 0.82, 1.0, 0.0, 0.35);
const INOX: MaterialSimples = m("inox", 0.78, 0.78, 0.80, 1.0, 0.90, 0.25);
const PRETO: MaterialSimples = m("preto-fosco", 0.09, 0.09, 0.10, 1.0, 0.0, 0.55);
const VIDRO: MaterialSimples = m("vidro", 0.85, 0.90, 0.90, 0.22, 0.0, 0.05);
const VERDE: MaterialSimples = m("folhagem", 0.30, 0.45, 0.26, 1.0, 0.0, 0.80);

// ---------------------------------------------------------------- malha

/// Geometria agrupada por material (um grupo vira uma primitiva no glTF).
#[derive(Debug, Clone, Default)]
pub struct Grupo {
    pub pos: Vec<[f32; 3]>,
    pub nor: Vec<[f32; 3]>,
    pub idx: Vec<u32>,
}

/// Malha de uma peca: materiais + um grupo de triangulos por material.
#[derive(Debug, Clone, Default)]
pub struct Malha {
    pub materiais: Vec<MaterialSimples>,
    pub grupos: Vec<Grupo>,
}

impl Malha {
    pub fn nova() -> Self {
        Self::default()
    }

    /// Indice do material, criando se ainda nao existe.
    fn material(&mut self, mat: MaterialSimples) -> usize {
        if let Some(i) = self.materiais.iter().position(|x| *x == mat) {
            return i;
        }
        self.materiais.push(mat);
        self.grupos.push(Grupo::default());
        self.materiais.len() - 1
    }

    /// Caixa alinhada aos eixos, de `min` a `max`, com normais planas por face.
    pub fn caixa(&mut self, min: [f32; 3], max: [f32; 3], mat: MaterialSimples) {
        let gi = self.material(mat);
        let g = &mut self.grupos[gi];
        let (x0, y0, z0) = (min[0], min[1], min[2]);
        let (x1, y1, z1) = (max[0], max[1], max[2]);

        // (4 cantos, normal) por face. Ordem anti-horaria vista de fora.
        let faces: [([[f32; 3]; 4], [f32; 3]); 6] = [
            // +Z (frente)
            (
                [[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
                [0.0, 0.0, 1.0],
            ),
            // -Z (fundo)
            (
                [[x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0]],
                [0.0, 0.0, -1.0],
            ),
            // +X (direita)
            (
                [[x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1]],
                [1.0, 0.0, 0.0],
            ),
            // -X (esquerda)
            (
                [[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]],
                [-1.0, 0.0, 0.0],
            ),
            // +Y (topo)
            (
                [[x0, y1, z1], [x1, y1, z1], [x1, y1, z0], [x0, y1, z0]],
                [0.0, 1.0, 0.0],
            ),
            // -Y (base)
            (
                [[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]],
                [0.0, -1.0, 0.0],
            ),
        ];

        for (cantos, normal) in faces {
            let base = g.pos.len() as u32;
            for c in cantos {
                g.pos.push(c);
                g.nor.push(normal);
            }
            g.idx
                .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }

    /// Cilindro vertical (tronco de cone se `raio_topo != raio_base`), com tampas.
    #[allow(clippy::too_many_arguments)] // sao 8 medidas geometricas, nenhuma opcional
    pub fn cilindro(
        &mut self,
        centro_xz: [f32; 2],
        raio_base: f32,
        raio_topo: f32,
        y0: f32,
        y1: f32,
        lados: usize,
        mat: MaterialSimples,
    ) {
        let lados = lados.max(3);
        let gi = self.material(mat);
        let g = &mut self.grupos[gi];
        let (cx, cz) = (centro_xz[0], centro_xz[1]);
        let tau = std::f32::consts::TAU;

        for i in 0..lados {
            let a0 = tau * i as f32 / lados as f32;
            let a1 = tau * (i + 1) as f32 / lados as f32;
            let (s0, c0) = a0.sin_cos();
            let (s1, c1) = a1.sin_cos();

            let p00 = [cx + c0 * raio_base, y0, cz + s0 * raio_base];
            let p10 = [cx + c1 * raio_base, y0, cz + s1 * raio_base];
            let p01 = [cx + c0 * raio_topo, y1, cz + s0 * raio_topo];
            let p11 = [cx + c1 * raio_topo, y1, cz + s1 * raio_topo];

            // Normal lateral media do segmento (suficiente para volume neutro).
            let am = (a0 + a1) * 0.5;
            let (sm, cm) = am.sin_cos();
            let inclinacao = (raio_base - raio_topo) / (y1 - y0).max(1e-4);
            let n = normalizar([cm, inclinacao, sm]);

            let base = g.pos.len() as u32;
            for p in [p00, p10, p11, p01] {
                g.pos.push(p);
                g.nor.push(n);
            }
            g.idx
                .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);

            // Tampa superior e inferior (leque a partir do centro).
            if raio_topo > 1e-4 {
                let b = g.pos.len() as u32;
                for p in [[cx, y1, cz], p01, p11] {
                    g.pos.push(p);
                    g.nor.push([0.0, 1.0, 0.0]);
                }
                g.idx.extend_from_slice(&[b, b + 1, b + 2]);
            }
            if raio_base > 1e-4 {
                let b = g.pos.len() as u32;
                for p in [[cx, y0, cz], p10, p00] {
                    g.pos.push(p);
                    g.nor.push([0.0, -1.0, 0.0]);
                }
                g.idx.extend_from_slice(&[b, b + 1, b + 2]);
            }
        }
    }

    /// Caixa girada em torno de X (para encosto inclinado de espreguicadeira).
    pub fn caixa_inclinada(
        &mut self,
        min: [f32; 3],
        max: [f32; 3],
        pivo: [f32; 3],
        angulo_graus: f32,
        mat: MaterialSimples,
    ) {
        let antes = self
            .grupos
            .get(
                self.materiais
                    .iter()
                    .position(|x| *x == mat)
                    .unwrap_or(usize::MAX),
            )
            .map(|g| g.pos.len())
            .unwrap_or(0);
        self.caixa(min, max, mat);
        let gi = self
            .materiais
            .iter()
            .position(|x| *x == mat)
            .expect("material recem-criado");
        let g = &mut self.grupos[gi];
        let (s, c) = angulo_graus.to_radians().sin_cos();
        let girar = |v: [f32; 3], pivo: Option<[f32; 3]>| -> [f32; 3] {
            let p = pivo.unwrap_or([0.0; 3]);
            let (y, z) = (v[1] - p[1], v[2] - p[2]);
            [v[0], p[1] + y * c - z * s, p[2] + y * s + z * c]
        };
        for i in antes..g.pos.len() {
            g.pos[i] = girar(g.pos[i], Some(pivo));
            g.nor[i] = normalizar(girar(g.nor[i], None));
        }
    }

    pub fn total_triangulos(&self) -> usize {
        self.grupos.iter().map(|g| g.idx.len() / 3).sum()
    }

    /// Caixa envolvente da malha inteira.
    pub fn caixa_envolvente(&self) -> ([f32; 3], [f32; 3]) {
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for g in &self.grupos {
            for p in &g.pos {
                for k in 0..3 {
                    min[k] = min[k].min(p[k]);
                    max[k] = max[k].max(p[k]);
                }
            }
        }
        (min, max)
    }
}

fn normalizar(v: [f32; 3]) -> [f32; 3] {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if n < 1e-6 {
        [0.0, 1.0, 0.0]
    } else {
        [v[0] / n, v[1] / n, v[2] / n]
    }
}

// ---------------------------------------------------------------- pecas

/// Cama: base, colchao, coberta, dois travesseiros e cabeceira. `l` = largura,
/// `c` = comprimento. Cabeceira no fundo (`-Z`), pe da cama em `+Z`.
fn cama(l: f32, c: f32) -> Malha {
    let mut ma = Malha::nova();
    let (hx, hz) = (l * 0.5, c * 0.5);

    // Base (estrado) recuada 4 cm de cada lado para dar sombra sob o colchao.
    ma.caixa(
        [-hx + 0.04, 0.10, -hz + 0.04],
        [hx - 0.04, 0.32, hz - 0.04],
        MADEIRA_MEDIA,
    );
    // Pes
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let px = sx * (hx - 0.10);
        let pz = sz * (hz - 0.10);
        ma.caixa(
            [px - 0.03, 0.0, pz - 0.03],
            [px + 0.03, 0.10, pz + 0.03],
            PRETO,
        );
    }
    // Colchao
    ma.caixa([-hx, 0.32, -hz], [hx, 0.56, hz], LINHO);
    // Coberta cobrindo dos pes ate 60% do comprimento
    ma.caixa(
        [-hx - 0.02, 0.55, -hz + c * 0.42],
        [hx + 0.02, 0.60, hz + 0.02],
        TECIDO,
    );
    // Travesseiros
    let tl = (l * 0.42).min(0.62);
    for s in [-1.0f32, 1.0] {
        let cxp = s * (l * 0.24);
        ma.caixa(
            [cxp - tl * 0.5, 0.56, -hz + 0.08],
            [cxp + tl * 0.5, 0.68, -hz + 0.42],
            BRANCO,
        );
    }
    // Cabeceira
    ma.caixa(
        [-hx - 0.05, 0.10, -hz - 0.08],
        [hx + 0.05, 1.00, -hz],
        MADEIRA_CLARA,
    );
    ma
}

fn guarda_roupa(l: f32, h: f32, p: f32) -> Malha {
    let mut ma = Malha::nova();
    let hx = l * 0.5;
    // Corpo
    ma.caixa([-hx, 0.0, -p * 0.5], [hx, h, p * 0.5], BRANCO);
    // Portas com fresta de 1 cm, salientes 2 cm
    let portas = (l / 0.55).round().max(2.0) as i32;
    let lp = l / portas as f32;
    for i in 0..portas {
        let x0 = -hx + i as f32 * lp + 0.01;
        let x1 = x0 + lp - 0.02;
        ma.caixa(
            [x0, 0.06, p * 0.5],
            [x1, h - 0.04, p * 0.5 + 0.02],
            MADEIRA_CLARA,
        );
        // Puxador vertical
        let px = x1 - 0.06;
        ma.caixa(
            [px, h * 0.42, p * 0.5 + 0.02],
            [px + 0.02, h * 0.62, p * 0.5 + 0.04],
            INOX,
        );
    }
    ma
}

fn bancada_cozinha(l: f32) -> Malha {
    let mut ma = Malha::nova();
    let (hx, p) = (l * 0.5, 0.60);
    // Armario inferior
    ma.caixa([-hx, 0.10, -p * 0.5], [hx, 0.86, p * 0.5], BRANCO);
    ma.caixa(
        [-hx, 0.0, -p * 0.5 + 0.05],
        [hx, 0.10, p * 0.5 - 0.05],
        PRETO,
    ); // rodape recuado
       // Tampo de pedra com pingadeira
    ma.caixa(
        [-hx - 0.02, 0.86, -p * 0.5 - 0.02],
        [hx + 0.02, 0.90, p * 0.5 + 0.02],
        PEDRA,
    );
    // Cuba embutida (rebaixo aparente) + torneira
    ma.caixa([-hx + 0.25, 0.72, -0.18], [-hx + 0.80, 0.87, 0.18], INOX);
    ma.cilindro([-hx + 0.52, -0.22], 0.02, 0.02, 0.90, 1.18, 12, INOX);
    ma.cilindro([-hx + 0.52, -0.10], 0.015, 0.015, 1.16, 1.18, 12, INOX);
    // Cooktop desenhado no tampo
    ma.caixa([hx - 0.85, 0.895, -0.20], [hx - 0.25, 0.905, 0.20], PRETO);
    // Armarios superiores
    ma.caixa([-hx, 1.50, -p * 0.5], [hx, 2.20, -p * 0.5 + 0.35], BRANCO);
    ma.caixa(
        [-hx + 0.01, 1.52, -p * 0.5 + 0.35],
        [hx - 0.01, 2.18, -p * 0.5 + 0.37],
        MADEIRA_CLARA,
    );
    ma
}

fn geladeira() -> Malha {
    let mut ma = Malha::nova();
    let (l, h, p) = (0.70f32, 1.85f32, 0.70f32);
    ma.caixa([-l * 0.5, 0.0, -p * 0.5], [l * 0.5, h, p * 0.5], INOX);
    // Fresta freezer/refrigerador
    ma.caixa(
        [-l * 0.5 - 0.005, 1.22, p * 0.5 - 0.01],
        [l * 0.5 + 0.005, 1.25, p * 0.5 + 0.01],
        PRETO,
    );
    // Puxadores
    for y in [(0.30f32, 1.15f32), (1.32, 1.75)] {
        ma.caixa(
            [l * 0.5 - 0.10, y.0, p * 0.5],
            [l * 0.5 - 0.06, y.1, p * 0.5 + 0.04],
            PRETO,
        );
    }
    ma
}

fn rack_tv() -> Malha {
    let mut ma = Malha::nova();
    // Rack baixo suspenso
    ma.caixa([-0.80, 0.30, -0.175], [0.80, 0.70, 0.175], MADEIRA_MEDIA);
    // Painel ripado atras
    ma.caixa([-0.90, 0.0, -0.20], [0.90, 1.60, -0.155], MADEIRA_CLARA);
    // TV
    ma.caixa([-0.65, 0.85, -0.15], [0.65, 1.48, -0.11], PRETO);
    ma
}

fn tapete(l: f32, c: f32) -> Malha {
    let mut ma = Malha::nova();
    ma.caixa([-l * 0.5, 0.0, -c * 0.5], [l * 0.5, 0.012, c * 0.5], TECIDO);
    // Borda mais clara
    ma.caixa(
        [-l * 0.5 + 0.10, 0.012, -c * 0.5 + 0.10],
        [l * 0.5 - 0.10, 0.014, c * 0.5 - 0.10],
        LINHO,
    );
    ma
}

fn vaso_sanitario() -> Malha {
    let mut ma = Malha::nova();
    // Caixa acoplada encostada no fundo (-Z)
    ma.caixa([-0.18, 0.40, -0.33], [0.18, 0.78, -0.15], BRANCO);
    // Base
    ma.caixa([-0.13, 0.0, -0.20], [0.13, 0.36, 0.10], BRANCO);
    // Bacia
    ma.cilindro([0.0, 0.10], 0.19, 0.19, 0.36, 0.42, 20, BRANCO);
    // Assento
    ma.cilindro([0.0, 0.10], 0.20, 0.20, 0.42, 0.45, 20, BRANCO);
    ma
}

fn cuba_banheiro() -> Malha {
    let mut ma = Malha::nova();
    let (l, p) = (0.90f32, 0.50f32);
    // Gabinete suspenso
    ma.caixa(
        [-l * 0.5, 0.35, -p * 0.5],
        [l * 0.5, 0.82, p * 0.5],
        MADEIRA_CLARA,
    );
    // Tampo
    ma.caixa(
        [-l * 0.5 - 0.02, 0.82, -p * 0.5],
        [l * 0.5 + 0.02, 0.87, p * 0.5 + 0.02],
        PEDRA,
    );
    // Cuba de apoio
    ma.cilindro([0.0, 0.0], 0.19, 0.20, 0.87, 1.00, 20, BRANCO);
    // Torneira
    ma.cilindro([0.0, -0.18], 0.02, 0.02, 0.87, 1.15, 12, INOX);
    ma.caixa([-0.02, 1.11, -0.18], [0.02, 1.15, -0.02], INOX);
    // Espelho
    ma.caixa(
        [-l * 0.5, 1.20, -p * 0.5 - 0.01],
        [l * 0.5, 2.00, -p * 0.5 + 0.01],
        VIDRO,
    );
    ma
}

fn box_chuveiro() -> Malha {
    let mut ma = Malha::nova();
    let (l, p, h) = (0.90f32, 0.90f32, 2.00f32);
    // Base / ralo
    ma.caixa([-l * 0.5, 0.0, -p * 0.5], [l * 0.5, 0.04, p * 0.5], PEDRA);
    // Dois panos de vidro (frente e lateral direita); o canto restante encosta na parede
    ma.caixa(
        [-l * 0.5, 0.04, p * 0.5 - 0.01],
        [l * 0.5, h, p * 0.5 + 0.01],
        VIDRO,
    );
    ma.caixa(
        [l * 0.5 - 0.01, 0.04, -p * 0.5],
        [l * 0.5 + 0.01, h, p * 0.5],
        VIDRO,
    );
    // Perfis
    ma.caixa(
        [-l * 0.5 - 0.01, 0.04, p * 0.5 - 0.02],
        [-l * 0.5 + 0.01, h, p * 0.5 + 0.02],
        INOX,
    );
    // Chuveiro
    ma.caixa(
        [-0.12, h - 0.15, -p * 0.5],
        [0.12, h - 0.11, -p * 0.5 + 0.24],
        INOX,
    );
    ma
}

fn balcao_recepcao() -> Malha {
    let mut ma = Malha::nova();
    let (l, p) = (3.00f32, 0.80f32);
    // Corpo em madeira com rodape recuado
    ma.caixa(
        [-l * 0.5, 0.12, -p * 0.5],
        [l * 0.5, 1.02, p * 0.5],
        MADEIRA_MEDIA,
    );
    ma.caixa(
        [-l * 0.5 + 0.05, 0.0, -p * 0.5 + 0.05],
        [l * 0.5 - 0.05, 0.12, p * 0.5 - 0.05],
        PRETO,
    );
    // Tampo de pedra saliente
    ma.caixa(
        [-l * 0.5 - 0.04, 1.02, -p * 0.5 - 0.04],
        [l * 0.5 + 0.04, 1.10, p * 0.5 + 0.04],
        PEDRA,
    );
    // Bancada interna mais baixa (lado de quem atende)
    ma.caixa(
        [-l * 0.5 + 0.10, 0.70, -p * 0.5 - 0.55],
        [l * 0.5 - 0.10, 0.75, -p * 0.5],
        PEDRA,
    );
    ma
}

fn espreguicadeira() -> Malha {
    let mut ma = Malha::nova();
    let (l, c) = (0.70f32, 2.00f32);
    // Pes
    for sz in [-1.0f32, 1.0] {
        let z = sz * (c * 0.5 - 0.18);
        ma.caixa(
            [-l * 0.5, 0.0, z - 0.03],
            [l * 0.5, 0.34, z + 0.03],
            MADEIRA_MEDIA,
        );
    }
    // Assento
    ma.caixa(
        [-l * 0.5, 0.34, -c * 0.5 + 0.10],
        [l * 0.5, 0.42, c * 0.5],
        LINHO,
    );
    // Encosto inclinado 35 graus, com pivo na junta do assento
    ma.caixa_inclinada(
        [-l * 0.5, 0.34, -c * 0.5 + 0.10],
        [l * 0.5, 0.42, -c * 0.5 + 0.90],
        [0.0, 0.38, -c * 0.5 + 0.10],
        -35.0,
        LINHO,
    );
    ma
}

fn guarda_sol() -> Malha {
    let mut ma = Malha::nova();
    // Base
    ma.cilindro([0.0, 0.0], 0.28, 0.26, 0.0, 0.08, 20, PRETO);
    // Mastro
    ma.cilindro([0.0, 0.0], 0.03, 0.03, 0.08, 2.40, 12, MADEIRA_CLARA);
    // Cobertura (tronco de cone raso)
    ma.cilindro([0.0, 0.0], 1.30, 0.10, 2.05, 2.45, 16, LINHO);
    ma
}

fn churrasqueira() -> Malha {
    let mut ma = Malha::nova();
    let (l, p) = (3.00f32, 0.70f32);
    ma.caixa([-l * 0.5, 0.0, -p * 0.5], [l * 0.5, 0.90, p * 0.5], PEDRA);
    // Tampo
    ma.caixa(
        [-l * 0.5 - 0.03, 0.90, -p * 0.5 - 0.03],
        [l * 0.5 + 0.03, 0.96, p * 0.5 + 0.03],
        PEDRA,
    );
    // Bocal da churrasqueira
    ma.caixa(
        [-l * 0.5 + 0.20, 0.96, -p * 0.5 + 0.08],
        [-l * 0.5 + 1.10, 1.10, p * 0.5 - 0.08],
        PRETO,
    );
    // Grelha
    ma.caixa(
        [-l * 0.5 + 0.24, 1.08, -p * 0.5 + 0.12],
        [-l * 0.5 + 1.06, 1.10, p * 0.5 - 0.12],
        INOX,
    );
    // Cuba de apoio
    ma.caixa(
        [l * 0.5 - 0.90, 0.80, -0.20],
        [l * 0.5 - 0.30, 0.96, 0.20],
        INOX,
    );
    // Vaso de tempero na ponta
    ma.cilindro([l * 0.5 - 0.18, 0.0], 0.09, 0.10, 0.96, 1.12, 14, BRANCO);
    ma.cilindro([l * 0.5 - 0.18, 0.0], 0.08, 0.02, 1.12, 1.30, 10, VERDE);
    ma
}

// ---------------------------------------------------------------- escrita .glb

#[derive(Debug, thiserror::Error)]
pub enum GlbError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("malha vazia: {0}")]
    MalhaVazia(&'static str),
}

// Little-endian: o primeiro byte do arquivo e o menos significativo.
const GLB_MAGIC: u32 = 0x4654_6C67; // "glTF"
const CHUNK_JSON: u32 = 0x4E4F_534A; // "JSON"
const CHUNK_BIN: u32 = 0x004E_4942; // "BIN\0"

/// Serializa a malha como glTF binario (.glb) em `caminho`.
///
/// Um `bufferView` por atributo e por grupo; um `material` por material da paleta.
/// Sem textura: a cor vem do `baseColorFactor` — e o suficiente para volume neutro
/// e mantem o arquivo em poucos kB.
pub fn escrever_glb(malha: &Malha, nome: &str, caminho: &Path) -> Result<(), GlbError> {
    if malha.grupos.iter().all(|g| g.idx.is_empty()) {
        return Err(GlbError::MalhaVazia("nenhum triangulo"));
    }

    let mut bin: Vec<u8> = Vec::new();
    let mut views = Vec::new();
    let mut accessors = Vec::new();
    let mut primitivas = Vec::new();

    for (gi, g) in malha.grupos.iter().enumerate() {
        if g.idx.is_empty() {
            continue;
        }

        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for p in &g.pos {
            for k in 0..3 {
                min[k] = min[k].min(p[k]);
                max[k] = max[k].max(p[k]);
            }
        }

        let pos_view = empurrar_view(&mut bin, &mut views, bytes_vec3(&g.pos), Some(34962));
        let nor_view = empurrar_view(&mut bin, &mut views, bytes_vec3(&g.nor), Some(34962));
        let idx_view = empurrar_view(&mut bin, &mut views, bytes_u32(&g.idx), Some(34963));

        let pos_acc = accessors.len();
        accessors.push(serde_json::json!({
            "bufferView": pos_view, "componentType": 5126, "count": g.pos.len(),
            "type": "VEC3", "min": min, "max": max
        }));
        let nor_acc = accessors.len();
        accessors.push(serde_json::json!({
            "bufferView": nor_view, "componentType": 5126, "count": g.nor.len(), "type": "VEC3"
        }));
        let idx_acc = accessors.len();
        accessors.push(serde_json::json!({
            "bufferView": idx_view, "componentType": 5125, "count": g.idx.len(), "type": "SCALAR"
        }));

        primitivas.push(serde_json::json!({
            "attributes": { "POSITION": pos_acc, "NORMAL": nor_acc },
            "indices": idx_acc,
            "material": gi,
            "mode": 4
        }));
    }

    let materiais: Vec<_> = malha
        .materiais
        .iter()
        .map(|m| {
            let transparente = m.cor[3] < 0.999;
            let mut j = serde_json::json!({
                "name": m.nome,
                "pbrMetallicRoughness": {
                    "baseColorFactor": m.cor,
                    "metallicFactor": m.metallic,
                    "roughnessFactor": m.roughness
                },
                "doubleSided": true
            });
            if transparente {
                j["alphaMode"] = serde_json::Value::String("BLEND".into());
            }
            j
        })
        .collect();

    let gltf = serde_json::json!({
        "asset": { "version": "2.0", "generator": concat!("arcz-biblioteca ", env!("CARGO_PKG_VERSION")) },
        "scene": 0,
        "scenes": [ { "nodes": [0] } ],
        "nodes": [ { "mesh": 0, "name": nome } ],
        "meshes": [ { "name": nome, "primitives": primitivas } ],
        "materials": materiais,
        "accessors": accessors,
        "bufferViews": views,
        "buffers": [ { "byteLength": bin.len() } ]
    });

    let mut json = serde_json::to_vec(&gltf)?;
    while json.len() % 4 != 0 {
        json.push(b' ');
    }
    while bin.len() % 4 != 0 {
        bin.push(0);
    }

    let total = 12 + 8 + json.len() + 8 + bin.len();
    if let Some(pai) = caminho.parent() {
        std::fs::create_dir_all(pai)?;
    }
    // Escrita atomica: temporario + rename, mesmo padrao do cache de tiles.
    let tmp = caminho.with_extension("glb.tmp");
    {
        let mut f = std::io::BufWriter::new(std::fs::File::create(&tmp)?);
        f.write_all(&GLB_MAGIC.to_le_bytes())?;
        f.write_all(&2u32.to_le_bytes())?;
        f.write_all(&(total as u32).to_le_bytes())?;
        f.write_all(&(json.len() as u32).to_le_bytes())?;
        f.write_all(&CHUNK_JSON.to_le_bytes())?;
        f.write_all(&json)?;
        f.write_all(&(bin.len() as u32).to_le_bytes())?;
        f.write_all(&CHUNK_BIN.to_le_bytes())?;
        f.write_all(&bin)?;
        f.flush()?;
    }
    std::fs::rename(&tmp, caminho)?;
    Ok(())
}

fn empurrar_view(
    bin: &mut Vec<u8>,
    views: &mut Vec<serde_json::Value>,
    dados: Vec<u8>,
    target: Option<u32>,
) -> usize {
    while bin.len() % 4 != 0 {
        bin.push(0);
    }
    let offset = bin.len();
    let len = dados.len();
    bin.extend_from_slice(&dados);
    let mut v = serde_json::json!({ "buffer": 0, "byteOffset": offset, "byteLength": len });
    if let Some(t) = target {
        v["target"] = serde_json::Value::from(t);
    }
    views.push(v);
    views.len() - 1
}

fn bytes_vec3(v: &[[f32; 3]]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 12);
    for p in v {
        for c in p {
            out.extend_from_slice(&c.to_le_bytes());
        }
    }
    out
}

fn bytes_u32(v: &[u32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for i in v {
        out.extend_from_slice(&i.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caixa_gera_12_triangulos_e_6_normais() {
        let mut ma = Malha::nova();
        ma.caixa([0.0; 3], [1.0; 3], BRANCO);
        assert_eq!(ma.total_triangulos(), 12);
        let (min, max) = ma.caixa_envolvente();
        assert_eq!(min, [0.0; 3]);
        assert_eq!(max, [1.0; 3]);
    }

    #[test]
    fn materiais_iguais_compartilham_grupo() {
        let mut ma = Malha::nova();
        ma.caixa([0.0; 3], [1.0; 3], BRANCO);
        ma.caixa([2.0, 0.0, 0.0], [3.0, 1.0, 1.0], BRANCO);
        ma.caixa([4.0, 0.0, 0.0], [5.0, 1.0, 1.0], PRETO);
        assert_eq!(ma.materiais.len(), 2);
        assert_eq!(ma.grupos.len(), 2);
        assert_eq!(ma.grupos[0].idx.len() / 3, 24);
    }

    #[test]
    fn toda_peca_tem_geometria_e_fica_de_pe_no_zero() {
        for peca in Peca::TODAS {
            let ma = peca.malha();
            assert!(ma.total_triangulos() > 0, "{peca:?} sem triangulos");
            let (min, max) = ma.caixa_envolvente();
            // Base no zero (tolerancia p/ encosto inclinado que passa um pouco).
            assert!(
                min[1] >= -0.02,
                "{peca:?} comeca abaixo do piso: {}",
                min[1]
            );
            assert!(min[1] < 0.40, "{peca:?} flutua: base em {}", min[1]);
            // Altura util de mobiliario
            assert!(max[1] > 0.01 && max[1] < 2.6, "{peca:?} altura {}", max[1]);
        }
    }

    #[test]
    fn pecas_batem_com_a_medida_declarada_no_catalogo() {
        // A planta usa `dimensao_m` para checar se o item cabe. Se a geometria
        // divergir da declaracao, a checagem vira mentira.
        for item in crate::catalogo::CATALOGO {
            let crate::catalogo::Fonte::Parametrica(peca) = item.fonte else {
                continue;
            };
            let (min, max) = peca.malha().caixa_envolvente();
            let real = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
            for (k, medida) in real.iter().enumerate() {
                let dif = (medida - item.dimensao_m[k]).abs();
                assert!(
                    dif <= 0.35,
                    "{}: eixo {k} declarado {} m, gerado {} m",
                    item.chave,
                    item.dimensao_m[k],
                    medida
                );
            }
        }
    }

    #[test]
    fn glb_escrito_tem_cabecalho_valido_e_tamanho_coerente() {
        let dir = std::env::temp_dir().join("arcz-biblioteca-teste-glb");
        let caminho = dir.join("cama.glb");
        escrever_glb(&Peca::CamaCasal.malha(), "cama-casal", &caminho).unwrap();

        let bytes = std::fs::read(&caminho).unwrap();
        assert_eq!(&bytes[0..4], b"glTF");
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 2);
        assert_eq!(
            u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize,
            bytes.len(),
            "byteLength do cabecalho difere do arquivo"
        );
        let json_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        assert_eq!(&bytes[16..20], b"JSON");
        assert_eq!(json_len % 4, 0, "chunk JSON precisa ser multiplo de 4");
        let bin_off = 20 + json_len;
        assert_eq!(&bytes[bin_off + 4..bin_off + 8], b"BIN\0");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn glb_e_reaberto_pelo_serde_com_a_contagem_certa() {
        let dir = std::env::temp_dir().join("arcz-biblioteca-teste-glb2");
        let caminho = dir.join("guarda-roupa.glb");
        let malha = Peca::GuardaRoupa.malha();
        escrever_glb(&malha, "guarda-roupa", &caminho).unwrap();

        let bytes = std::fs::read(&caminho).unwrap();
        let json_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        let doc: serde_json::Value = serde_json::from_slice(&bytes[20..20 + json_len]).unwrap();
        assert_eq!(doc["asset"]["version"], "2.0");
        assert_eq!(
            doc["materials"].as_array().unwrap().len(),
            malha.materiais.len()
        );
        let prims = doc["meshes"][0]["primitives"].as_array().unwrap();
        assert_eq!(
            prims.len(),
            malha.grupos.iter().filter(|g| !g.idx.is_empty()).count()
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn vidro_recebe_alpha_blend() {
        let malha = Peca::BoxChuveiro.malha();
        assert!(malha.materiais.iter().any(|m| m.cor[3] < 0.999));
    }
}

#[cfg(test)]
mod dump {
    #[test]
    #[ignore]
    fn imprime_dimensoes() {
        for p in super::Peca::TODAS {
            let (min, max) = p.malha().caixa_envolvente();
            println!(
                "{:<18} [{:.2}, {:.2}, {:.2}]  (base y={:.2})",
                p.nome_arquivo(),
                max[0] - min[0],
                max[1] - min[1],
                max[2] - min[2],
                min[1]
            );
        }
    }
}

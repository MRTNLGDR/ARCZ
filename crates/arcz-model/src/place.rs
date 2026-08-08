//! Posicionamento georreferenciado: leva o modelo do espaco local para o quadro ENU.
//!
//! Ordem das operacoes (importa): **escala -> rotacao de rumo -> assentamento ->
//! translacao para lat/lon**. Rotacionar depois de transladar giraria o predio em
//! torno do centro da cena em vez do proprio eixo.

use arcz_geo::{EnuFrame, Geodetic};

use crate::{normalizar, Material, Model, ModelVertex, Submesh, Textura};

/// Como o modelo deve ser colocado no mundo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// Coordenada onde o ponto de ancoragem do modelo vai parar.
    pub lat_deg: f64,
    pub lon_deg: f64,
    /// Rumo em graus, sentido horario a partir do norte (0 = norte, 90 = leste).
    pub heading_deg: f64,
    /// Multiplicador de unidade. 1.0 = o arquivo ja esta em metros (spec glTF).
    pub escala: f32,
    /// Assenta a base da caixa envolvente na altura do terreno.
    pub assentar_no_terreno: bool,
    /// Deslocamento vertical extra, em metros, aplicado depois do assentamento.
    /// Negativo enterra o modelo (util para embutir fundacao num terreno inclinado).
    pub offset_vertical_m: f32,
    /// Ajuste fino horizontal em metros, aplicado **depois** da rotacao de rumo.
    ///
    /// Existe porque a ancoragem pelo centro da caixa envolvente erra sempre que o
    /// arquivo inclui o entorno (rua, calcada, lotes vizinhos) alem do predio: o que
    /// cai na coordenada e o centro do conjunto, nao o do predio. Em vez de adivinhar
    /// qual parte da malha e "o predio", o operador desloca no eixo do mundo ate a
    /// geometria bater com a ortofoto.
    pub offset_leste_m: f32,
    pub offset_norte_m: f32,
}

impl Default for Placement {
    fn default() -> Self {
        Self {
            lat_deg: 0.0,
            lon_deg: 0.0,
            heading_deg: 0.0,
            escala: 1.0,
            assentar_no_terreno: true,
            offset_vertical_m: 0.0,
            offset_leste_m: 0.0,
            offset_norte_m: 0.0,
        }
    }
}

/// Modelo ja em coordenadas de render (x=leste, y=cima, z=-norte), pronto para a GPU.
#[derive(Debug, Clone)]
pub struct PlacedModel {
    pub vertices: Vec<ModelVertex>,
    pub indices: Vec<u32>,
    /// Faixas de indices por material, herdadas do arquivo.
    pub submeshes: Vec<Submesh>,
    pub materiais: Vec<Material>,
    pub texturas: Vec<Textura>,
    pub min_enu: [f32; 3],
    pub max_enu: [f32; 3],
    /// Dimensoes reais em metros depois da escala. E o numero que o usuario confere
    /// contra a planta ("meu predio tem 42 m de altura").
    pub tamanho_real_m: [f32; 3],
    /// Altitude absoluta em que a base ficou, em metros.
    pub altitude_base_m: f64,
}

impl PlacedModel {
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    pub fn center(&self) -> [f32; 3] {
        [
            (self.min_enu[0] + self.max_enu[0]) * 0.5,
            (self.min_enu[1] + self.max_enu[1]) * 0.5,
            (self.min_enu[2] + self.max_enu[2]) * 0.5,
        ]
    }
}

/// Geometria original guardada para reposicionar sem reabrir o arquivo.
///
/// So os vertices e a caixa envolvente — indices, materiais e texturas nao mudam com
/// a posicao, entao ficam de fora. No Zenite isso e ~30 MB em vez dos 201 MB que um
/// clone do modelo inteiro custaria.
#[derive(Debug, Clone)]
pub struct FonteGeometria {
    pub vertices: Vec<ModelVertex>,
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl FonteGeometria {
    pub fn from_model(m: &Model) -> Self {
        Self {
            vertices: m.vertices.clone(),
            min: m.min,
            max: m.max,
        }
    }
}

/// Resultado cru de [`transformar`]: vertices no quadro ENU e a caixa envolvente.
pub struct Transformado {
    pub vertices: Vec<ModelVertex>,
    pub min_enu: [f32; 3],
    pub max_enu: [f32; 3],
    pub altitude_base_m: f64,
}

/// Matriz que leva o vertice do espaco do arquivo para o quadro ENU de render.
///
/// E a mesma transformacao que [`transformar`] aplica vertice a vertice, mas em
/// forma de matriz 4x4 (column-major, pronta para WGSL). Com ela a GPU faz o
/// trabalho: mover o objeto passa a custar 64 bytes de uniform em vez de
/// retransformar e reenviar a malha inteira — no Zenite, 30 MB por quadro.
///
/// Composicao: `T(destino) * R(rumo) * S(escala) * T(-ancora)`.
pub fn matriz_modelo(
    fonte_min: [f32; 3],
    fonte_max: [f32; 3],
    frame: &EnuFrame,
    p: &Placement,
    altura_terreno_m: f64,
) -> [[f32; 4]; 4] {
    let s = p.escala;
    let (sin_h, cos_h) = (p.heading_deg as f32).to_radians().sin_cos();

    let centro_x = (fonte_min[0] + fonte_max[0]) * 0.5;
    let centro_z = (fonte_min[2] + fonte_max[2]) * 0.5;
    let base_y = fonte_min[1] * s;

    let altitude_base = if p.assentar_no_terreno {
        altura_terreno_m + p.offset_vertical_m as f64
    } else {
        base_y as f64 + p.offset_vertical_m as f64
    };

    let base_render = frame
        .geodetic_to_enu(Geodetic::new(p.lon_deg, p.lat_deg, altitude_base))
        .to_render_f32();
    let destino = [
        base_render[0] + p.offset_leste_m,
        base_render[1],
        base_render[2] - p.offset_norte_m,
    ];

    // A ancora e subtraida ANTES da escala e da rotacao; o resultado abaixo ja e a
    // composicao expandida, evitando multiplicar tres matrizes em tempo de execucao.
    let ax = centro_x * s;
    let ay = base_y;
    let az = centro_z * s;

    // Colunas da rotacao em Y combinada com a escala uniforme.
    let c0 = [s * cos_h, 0.0, -s * sin_h];
    let c1 = [0.0, s, 0.0];
    let c2 = [s * sin_h, 0.0, s * cos_h];

    // Translacao = destino - R*S*ancora.
    let tx = destino[0] - (c0[0] * (ax / s) + c2[0] * (az / s)) * 1.0;
    let tz = destino[2] - (c0[2] * (ax / s) + c2[2] * (az / s)) * 1.0;
    let ty = destino[1] - ay;

    [
        [c0[0], c0[1], c0[2], 0.0],
        [c1[0], c1[1], c1[2], 0.0],
        [c2[0], c2[1], c2[2], 0.0],
        [tx, ty, tz, 1.0],
    ]
}

/// Caixa envolvente depois de aplicada a matriz, sem tocar nos vertices.
///
/// Transforma os 8 cantos da caixa local. Para rotacao em torno de Y e escala
/// uniforme isso e exato — nao ha deformacao que faca um vertice interno sair da
/// caixa dos cantos.
pub fn caixa_transformada(
    fonte_min: [f32; 3],
    fonte_max: [f32; 3],
    m: [[f32; 4]; 4],
) -> ([f32; 3], [f32; 3]) {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];

    for i in 0..8 {
        let p = [
            if i & 1 == 0 {
                fonte_min[0]
            } else {
                fonte_max[0]
            },
            if i & 2 == 0 {
                fonte_min[1]
            } else {
                fonte_max[1]
            },
            if i & 4 == 0 {
                fonte_min[2]
            } else {
                fonte_max[2]
            },
        ];
        for k in 0..3 {
            let v = m[0][k] * p[0] + m[1][k] * p[1] + m[2][k] * p[2] + m[3][k];
            min[k] = min[k].min(v);
            max[k] = max[k].max(v);
        }
    }
    (min, max)
}

/// Aplica escala, rumo, assentamento e translacao aos vertices.
///
/// Separado de [`place`] para o preview poder reposicionar o modelo a cada ajuste
/// sem tocar em materiais nem texturas.
pub fn transformar(
    fonte: &FonteGeometria,
    frame: &EnuFrame,
    p: &Placement,
    altura_terreno_m: f64,
) -> Transformado {
    let s = p.escala;

    // Rumo horario a partir do norte. Em coordenadas de render o norte e -Z e o
    // leste e +X, entao um giro horario visto de cima e uma rotacao de +heading
    // em torno de +Y.
    let (sin_h, cos_h) = (p.heading_deg as f32).to_radians().sin_cos();
    let girar = |v: [f32; 3]| -> [f32; 3] {
        [
            v[0] * cos_h + v[2] * sin_h,
            v[1],
            -v[0] * sin_h + v[2] * cos_h,
        ]
    };

    // Ancoragem horizontal: centro da planta do modelo.
    let centro_x = (fonte.min[0] + fonte.max[0]) * 0.5;
    let centro_z = (fonte.min[2] + fonte.max[2]) * 0.5;
    let base_y = fonte.min[1] * s;

    let altitude_base = if p.assentar_no_terreno {
        altura_terreno_m + p.offset_vertical_m as f64
    } else {
        base_y as f64 + p.offset_vertical_m as f64
    };

    // Origem do modelo no quadro ENU, mais o ajuste fino. O offset e somado em
    // coordenadas de render (x=leste, z=-norte), depois da rotacao, para que "mover
    // 5 m para o leste" seja leste do mundo e nao do modelo.
    let base_render = frame
        .geodetic_to_enu(Geodetic::new(p.lon_deg, p.lat_deg, altitude_base))
        .to_render_f32();
    let destino = [
        base_render[0] + p.offset_leste_m,
        base_render[1],
        base_render[2] - p.offset_norte_m,
    ];

    let mut vertices = Vec::with_capacity(fonte.vertices.len());
    let mut min_enu = [f32::INFINITY; 3];
    let mut max_enu = [f32::NEG_INFINITY; 3];

    for v in &fonte.vertices {
        let local = [
            (v.position[0] - centro_x) * s,
            v.position[1] * s - base_y,
            (v.position[2] - centro_z) * s,
        ];
        let girado = girar(local);
        let position = [
            girado[0] + destino[0],
            girado[1] + destino[1],
            girado[2] + destino[2],
        ];

        for (k, c) in position.iter().enumerate() {
            min_enu[k] = min_enu[k].min(*c);
            max_enu[k] = max_enu[k].max(*c);
        }

        vertices.push(ModelVertex {
            position,
            // Escala uniforme e rotacao rigida preservam normais: basta girar.
            normal: normalizar(girar(v.normal)),
            uv: v.uv,
        });
    }

    Transformado {
        vertices,
        min_enu,
        max_enu,
        altitude_base_m: altitude_base,
    }
}

/// Coloca `model` no quadro `frame`.
///
/// Consome o modelo: indices, submeshes, materiais e texturas sao **movidos** para o
/// resultado. Com arquivos de centenas de MB (o Zenite tem 130 MB), clonar as
/// texturas so para manter o original vivo dobraria o pico de memoria a toa.
///
/// `altura_terreno_m` e a altitude absoluta do terreno na coordenada de destino —
/// quem amostra o DEM e o chamador, para este crate nao depender de `arcz-terrain`.
pub fn place(model: Model, frame: &EnuFrame, p: &Placement, altura_terreno_m: f64) -> PlacedModel {
    let s = p.escala;
    let tam = model.size();

    let t = transformar(
        &FonteGeometria::from_model(&model),
        frame,
        p,
        altura_terreno_m,
    );
    let (vertices, min_enu, max_enu, altitude_base) =
        (t.vertices, t.min_enu, t.max_enu, t.altitude_base_m);

    PlacedModel {
        vertices,
        indices: model.indices,
        submeshes: model.submeshes,
        materiais: model.materiais,
        texturas: model.texturas,
        min_enu,
        max_enu,
        tamanho_real_m: [tam[0] * s, tam[1] * s, tam[2] * s],
        altitude_base_m: altitude_base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdata::glb_retangulo;

    const CENTRO: Geodetic = Geodetic::new(-46.633_308, -23.550_520, 700.0);

    fn cenario() -> (Model, EnuFrame) {
        // Retangulo de 20 m de largura por 50 m de altura.
        let m = Model::from_glb_slice(&glb_retangulo(20.0, 50.0)).unwrap();
        (m, EnuFrame::new(CENTRO))
    }

    #[test]
    fn escala_um_preserva_as_dimensoes_reais() {
        let (m, frame) = cenario();
        let p = Placement {
            lat_deg: CENTRO.lat_deg,
            lon_deg: CENTRO.lon_deg,
            ..Default::default()
        };
        let posto = place(m.clone(), &frame, &p, 700.0);

        assert!((posto.tamanho_real_m[0] - 20.0).abs() < 1e-3);
        assert!((posto.tamanho_real_m[1] - 50.0).abs() < 1e-3);

        // A altura medida na cena tem que bater com a altura real do modelo.
        let altura = posto.max_enu[1] - posto.min_enu[1];
        assert!((altura - 50.0).abs() < 1e-3, "altura na cena: {altura}");
    }

    #[test]
    fn escala_converte_centimetros_para_metros() {
        // Mesmo predio exportado em centimetros: 2000 x 5000 unidades.
        let m = Model::from_glb_slice(&glb_retangulo(2000.0, 5000.0)).unwrap();
        let frame = EnuFrame::new(CENTRO);
        let p = Placement {
            lat_deg: CENTRO.lat_deg,
            lon_deg: CENTRO.lon_deg,
            escala: 0.01,
            ..Default::default()
        };
        let posto = place(m.clone(), &frame, &p, 700.0);

        assert!((posto.tamanho_real_m[0] - 20.0).abs() < 1e-2);
        assert!((posto.tamanho_real_m[1] - 50.0).abs() < 1e-2);
    }

    #[test]
    fn assenta_a_base_exatamente_na_altura_do_terreno() {
        let (m, frame) = cenario();
        let p = Placement {
            lat_deg: CENTRO.lat_deg,
            lon_deg: CENTRO.lon_deg,
            ..Default::default()
        };
        // Terreno 40 m acima da origem do quadro (que esta em 700 m).
        let posto = place(m.clone(), &frame, &p, 740.0);

        assert!((posto.altitude_base_m - 740.0).abs() < 1e-6);
        // A origem do quadro esta em 700 m, entao a base cai em +40 m no eixo Y.
        assert!(
            (posto.min_enu[1] - 40.0).abs() < 0.01,
            "base em {} m, esperado 40",
            posto.min_enu[1]
        );
        // E o topo, 50 m acima da base.
        assert!(
            (posto.max_enu[1] - 90.0).abs() < 0.01,
            "topo: {}",
            posto.max_enu[1]
        );
    }

    #[test]
    fn offset_vertical_enterra_e_levanta() {
        let (m, frame) = cenario();
        let base = Placement {
            lat_deg: CENTRO.lat_deg,
            lon_deg: CENTRO.lon_deg,
            ..Default::default()
        };
        let no_chao = place(m.clone(), &frame, &base, 700.0);
        let enterrado = place(
            m.clone(),
            &frame,
            &Placement {
                offset_vertical_m: -3.0,
                ..base
            },
            700.0,
        );
        assert!((enterrado.min_enu[1] - (no_chao.min_enu[1] - 3.0)).abs() < 1e-3);
    }

    #[test]
    fn ancora_no_centro_da_planta_e_nao_num_canto() {
        let (m, frame) = cenario();
        let p = Placement {
            lat_deg: CENTRO.lat_deg,
            lon_deg: CENTRO.lon_deg,
            ..Default::default()
        };
        let posto = place(m.clone(), &frame, &p, 700.0);

        // O modelo vai de x=0 a x=20 no arquivo; ancorado pelo centro, na cena ele
        // tem que ficar simetrico em torno de x=0.
        let centro_x = (posto.min_enu[0] + posto.max_enu[0]) * 0.5;
        assert!(centro_x.abs() < 0.01, "centro em x = {centro_x}");
        assert!((posto.min_enu[0] + 10.0).abs() < 0.01);
        assert!((posto.max_enu[0] - 10.0).abs() < 0.01);
    }

    #[test]
    fn heading_gira_no_sentido_horario_a_partir_do_norte() {
        let (m, frame) = cenario();
        let base = Placement {
            lat_deg: CENTRO.lat_deg,
            lon_deg: CENTRO.lon_deg,
            ..Default::default()
        };

        // Sem rotacao o retangulo tem 20 m em X (leste-oeste) e ~0 em Z.
        let a = place(m.clone(), &frame, &base, 700.0);
        assert!((a.max_enu[0] - a.min_enu[0] - 20.0).abs() < 0.01);
        assert!((a.max_enu[2] - a.min_enu[2]).abs() < 0.01);

        // Com 90 graus a extensao troca de eixo.
        let b = place(
            m.clone(),
            &frame,
            &Placement {
                heading_deg: 90.0,
                ..base
            },
            700.0,
        );
        assert!(
            (b.max_enu[0] - b.min_enu[0]).abs() < 0.01,
            "X deveria ter colapsado: {} .. {}",
            b.min_enu[0],
            b.max_enu[0]
        );
        assert!((b.max_enu[2] - b.min_enu[2] - 20.0).abs() < 0.01);

        // A altura nunca muda com o rumo.
        assert!((b.max_enu[1] - b.min_enu[1] - 50.0).abs() < 0.01);
    }

    #[test]
    fn heading_360_equivale_a_zero() {
        let (m, frame) = cenario();
        let base = Placement {
            lat_deg: CENTRO.lat_deg,
            lon_deg: CENTRO.lon_deg,
            ..Default::default()
        };
        let a = place(m.clone(), &frame, &base, 700.0);
        let b = place(
            m.clone(),
            &frame,
            &Placement {
                heading_deg: 360.0,
                ..base
            },
            700.0,
        );
        for k in 0..3 {
            assert!((a.min_enu[k] - b.min_enu[k]).abs() < 0.01);
            assert!((a.max_enu[k] - b.max_enu[k]).abs() < 0.01);
        }
    }

    #[test]
    fn deslocar_a_coordenada_move_o_modelo_na_direcao_certa() {
        let (m, frame) = cenario();
        let base = Placement {
            lat_deg: CENTRO.lat_deg,
            lon_deg: CENTRO.lon_deg,
            ..Default::default()
        };
        let centro = place(m.clone(), &frame, &base, 700.0);

        // ~1 km ao norte.
        let norte = place(
            m.clone(),
            &frame,
            &Placement {
                lat_deg: CENTRO.lat_deg + 1000.0 / 111_132.0,
                ..base
            },
            700.0,
        );
        // Norte e -Z em coordenadas de render.
        let dz = norte.center()[2] - centro.center()[2];
        assert!(dz < -900.0 && dz > -1100.0, "deslocamento em z: {dz}");

        // ~1 km a leste.
        let leste = place(
            m.clone(),
            &frame,
            &Placement {
                lon_deg: CENTRO.lon_deg + 1000.0 / (111_132.0 * CENTRO.lat_deg.to_radians().cos()),
                ..base
            },
            700.0,
        );
        let dx = leste.center()[0] - centro.center()[0];
        assert!(dx > 900.0 && dx < 1100.0, "deslocamento em x: {dx}");
    }

    #[test]
    fn offset_horizontal_desloca_nos_eixos_do_mundo() {
        let (m, frame) = cenario();
        let base = Placement {
            lat_deg: CENTRO.lat_deg,
            lon_deg: CENTRO.lon_deg,
            ..Default::default()
        };
        let origem = place(m.clone(), &frame, &base, 700.0);

        let deslocado = place(
            m.clone(),
            &frame,
            &Placement {
                offset_leste_m: 12.0,
                offset_norte_m: -7.0,
                ..base
            },
            700.0,
        );

        let c0 = origem.center();
        let c1 = deslocado.center();
        assert!(
            (c1[0] - c0[0] - 12.0).abs() < 1e-3,
            "leste: {} -> {}",
            c0[0],
            c1[0]
        );
        // norte e -Z: 7 m para o SUL sobem o z em +7.
        assert!(
            (c1[2] - c0[2] - 7.0).abs() < 1e-3,
            "norte: {} -> {}",
            c0[2],
            c1[2]
        );
        assert!(
            (c1[1] - c0[1]).abs() < 1e-3,
            "o offset horizontal mexeu na altura"
        );
    }

    #[test]
    fn offset_horizontal_independe_do_rumo() {
        // "Mover 10 m para o leste" tem que ser leste do mundo, nao do modelo —
        // senao ajustar o rumo estragaria o alinhamento ja feito.
        let (m, frame) = cenario();
        for heading in [0.0, 45.0, 137.0, 300.0] {
            let sem = place(
                m.clone(),
                &frame,
                &Placement {
                    lat_deg: CENTRO.lat_deg,
                    lon_deg: CENTRO.lon_deg,
                    heading_deg: heading,
                    ..Default::default()
                },
                700.0,
            );
            let com = place(
                m.clone(),
                &frame,
                &Placement {
                    lat_deg: CENTRO.lat_deg,
                    lon_deg: CENTRO.lon_deg,
                    heading_deg: heading,
                    offset_leste_m: 10.0,
                    ..Default::default()
                },
                700.0,
            );
            let d = com.center()[0] - sem.center()[0];
            assert!((d - 10.0).abs() < 1e-3, "rumo {heading}: deslocou {d} m");
        }
    }

    /// A matriz tem que produzir exatamente o mesmo resultado que transformar
    /// vertice a vertice. Se divergirem, a GPU desenha num lugar e o picking
    /// calcula em outro — e clicar no objeto passa a errar.
    #[test]
    fn a_matriz_concorda_com_a_transformacao_vertice_a_vertice() {
        let (m, frame) = cenario();
        let fonte = FonteGeometria::from_model(&m);

        for p in [
            Placement {
                lat_deg: CENTRO.lat_deg,
                lon_deg: CENTRO.lon_deg,
                ..Default::default()
            },
            Placement {
                lat_deg: CENTRO.lat_deg,
                lon_deg: CENTRO.lon_deg,
                heading_deg: 137.0,
                escala: 2.5,
                offset_leste_m: 30.0,
                offset_norte_m: -12.0,
                offset_vertical_m: 4.0,
                ..Default::default()
            },
        ] {
            let t = transformar(&fonte, &frame, &p, 700.0);
            let mat = matriz_modelo(fonte.min, fonte.max, &frame, &p, 700.0);

            for (i, v) in fonte.vertices.iter().enumerate() {
                let q = v.position;
                let esperado = t.vertices[i].position;
                for k in 0..3 {
                    let obtido = mat[0][k] * q[0] + mat[1][k] * q[1] + mat[2][k] * q[2] + mat[3][k];
                    assert!(
                        (obtido - esperado[k]).abs() < 0.01,
                        "vertice {i} eixo {k}: matriz deu {obtido}, CPU deu {}",
                        esperado[k]
                    );
                }
            }
        }
    }

    #[test]
    fn a_caixa_pela_matriz_bate_com_a_caixa_dos_vertices() {
        let (m, frame) = cenario();
        let fonte = FonteGeometria::from_model(&m);
        let p = Placement {
            lat_deg: CENTRO.lat_deg,
            lon_deg: CENTRO.lon_deg,
            heading_deg: 62.0,
            escala: 1.7,
            ..Default::default()
        };

        let t = transformar(&fonte, &frame, &p, 700.0);
        let mat = matriz_modelo(fonte.min, fonte.max, &frame, &p, 700.0);
        let (min, max) = caixa_transformada(fonte.min, fonte.max, mat);

        for k in 0..3 {
            assert!((min[k] - t.min_enu[k]).abs() < 0.05, "min eixo {k}");
            assert!((max[k] - t.max_enu[k]).abs() < 0.05, "max eixo {k}");
        }
    }

    #[test]
    fn a_matriz_e_finita_em_qualquer_parametro() {
        let (m, frame) = cenario();
        let fonte = FonteGeometria::from_model(&m);
        for heading in [0.0, 90.0, 180.0, 359.9] {
            for escala in [0.01, 1.0, 2.0] {
                let mat = matriz_modelo(
                    fonte.min,
                    fonte.max,
                    &frame,
                    &Placement {
                        lat_deg: CENTRO.lat_deg,
                        lon_deg: CENTRO.lon_deg,
                        heading_deg: heading,
                        escala,
                        ..Default::default()
                    },
                    700.0,
                );
                assert!(mat.iter().flatten().all(|v| v.is_finite()));
                // A ultima linha de uma matriz afim e sempre (0,0,0,1).
                assert_eq!(
                    [mat[0][3], mat[1][3], mat[2][3], mat[3][3]],
                    [0.0, 0.0, 0.0, 1.0]
                );
            }
        }
    }

    #[test]
    fn normais_continuam_unitarias_depois_de_girar() {
        let (m, frame) = cenario();
        let posto = place(
            m.clone(),
            &frame,
            &Placement {
                lat_deg: CENTRO.lat_deg,
                lon_deg: CENTRO.lon_deg,
                heading_deg: 37.0,
                escala: 2.5,
                ..Default::default()
            },
            700.0,
        );
        for v in &posto.vertices {
            let l = (v.normal[0].powi(2) + v.normal[1].powi(2) + v.normal[2].powi(2)).sqrt();
            assert!((l - 1.0).abs() < 1e-5, "normal {:?}", v.normal);
            assert!(v.position.iter().all(|c| c.is_finite()));
        }
    }

    #[test]
    fn sem_assentar_respeita_a_altitude_do_arquivo() {
        let (m, frame) = cenario();
        let posto = place(
            m.clone(),
            &frame,
            &Placement {
                lat_deg: CENTRO.lat_deg,
                lon_deg: CENTRO.lon_deg,
                assentar_no_terreno: false,
                ..Default::default()
            },
            740.0,
        );
        // Ignora os 740 m do terreno: a base do arquivo esta em y=0.
        assert!((posto.altitude_base_m - 0.0).abs() < 1e-6);
    }
}

#[cfg(test)]
mod coerencia_cpu_gpu {
    use super::*;
    use arcz_geo::Geodetic;

    /// A matriz que a GPU usa e a transformacao que a CPU faz TEM que dar o mesmo
    /// ponto. Se divergirem, a caixa de selecao (CPU) aponta para um lugar e o
    /// desenho (GPU) para outro — bug invisivel em teste unitario de cada lado.
    #[test]
    fn matriz_da_gpu_bate_com_transformar_da_cpu() {
        let frame = arcz_geo::EnuFrame::new(Geodetic::new(-48.5, -27.15, 0.0));
        let fonte = FonteGeometria {
            vertices: vec![
                crate::ModelVertex {
                    position: [3.0, 1.0, -7.0],
                    normal: [0.0, 1.0, 0.0],
                    uv: [0.0, 0.0],
                },
                crate::ModelVertex {
                    position: [-2.0, 0.0, 5.0],
                    normal: [0.0, 1.0, 0.0],
                    uv: [0.0, 0.0],
                },
            ],
            min: [-2.0, 0.0, -7.0],
            max: [3.0, 1.0, 5.0],
        };

        for (heading, escala, leste, norte, vertical, assentar) in [
            (0.0, 1.0, 0.0, 0.0, 0.0, true),
            (0.0, 1.0, 12.5, -8.75, 25.25, true),
            (37.0, 1.0, 4.0, 9.0, 3.2, true),
            (180.0, 2.5, -6.0, 2.0, 1.0, false),
            (270.0, 0.4, 30.0, -30.0, 12.0, true),
        ] {
            let p = Placement {
                lat_deg: -27.15,
                lon_deg: -48.5,
                heading_deg: heading,
                escala,
                assentar_no_terreno: assentar,
                offset_vertical_m: vertical,
                offset_leste_m: leste,
                offset_norte_m: norte,
            };
            let solo = 9.0;

            let cpu = transformar(&fonte, &frame, &p, solo);
            let m = matriz_modelo(fonte.min, fonte.max, &frame, &p, solo);

            for (i, v) in fonte.vertices.iter().enumerate() {
                let x = v.position;
                let gpu = [
                    m[0][0] * x[0] + m[1][0] * x[1] + m[2][0] * x[2] + m[3][0],
                    m[0][1] * x[0] + m[1][1] * x[1] + m[2][1] * x[2] + m[3][1],
                    m[0][2] * x[0] + m[1][2] * x[1] + m[2][2] * x[2] + m[3][2],
                ];
                let c = cpu.vertices[i].position;
                for k in 0..3 {
                    assert!(
                        (gpu[k] - c[k]).abs() < 0.01,
                        "heading {heading} escala {escala}: eixo {k} CPU {} GPU {}",
                        c[k],
                        gpu[k]
                    );
                }
            }
        }
    }
}

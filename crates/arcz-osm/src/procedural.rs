//! Gera o tecido urbano onde o OpenStreetMap nao mapeou.
//!
//! O teste real em Bombinhas devolveu **74 vias para 9 predios**: a malha
//! viaria esta mapeada, as edificacoes nao. Renderizar so o que o OSM tem
//! produziria um bairro fantasma. Este modulo preenche as quadras usando as
//! ruas reais como estrutura.
//!
//! ## Por que lotes de frente, e nao quadras fechadas
//!
//! A abordagem de manual (CityEngine) extrai o poligono da quadra achando os
//! ciclos minimos do arranjo planar das ruas, e so entao subdivide em lotes.
//! Isso exige que a rede viaria seja limpa: sem pontas soltas, sem cruzamentos
//! nao-nodais, sem vias saindo da bbox. Dados de OSM em cidade pequena nao sao
//! assim — das 74 vias de Bombinhas, varias sao servidoes sem saida e muitas
//! atravessam a borda do recorte. O arranjo planar falharia justamente onde
//! mais se precisa dele.
//!
//! Lotear pela **frente da via** nao depende de a rede fechar: cada rua produz
//! lotes ao longo dos seus dois lados, e a colisao impede sobreposicao. O
//! resultado tem a mesma leitura urbana (edificacoes alinhadas a rua, com
//! recuo) e degrada bem quando o dado e ruim.
//!
//! ## O que este modulo nao faz
//!
//! Nao inventa alturas que contradigam o OSM: onde ha predio mapeado, ele vence
//! e o gerador desvia. As edificacoes sinteticas saem marcadas com
//! [`FonteAltura::Estimada`] e id negativo, para nunca serem confundidas com
//! levantamento real.

use arcz_geo::{EnuFrame, Geodetic};

use crate::entidade::*;
use crate::triangulacao::{self, P2};

/// Parametros do loteamento. Os padroes descrevem bairro litoraneo brasileiro
/// de baixa densidade, que e o caso de Jose Amandio.
#[derive(Debug, Clone, Copy)]
pub struct RegrasUrbanas {
    /// Testada do lote (frente para a rua), em metros.
    pub testada_m: f64,
    /// Profundidade do lote a partir do alinhamento, em metros.
    pub profundidade_m: f64,
    /// Calcada entre o meio-fio e o alinhamento do lote.
    pub calcada_m: f64,
    /// Recuo frontal obrigatorio entre o alinhamento e a edificacao.
    pub recuo_frontal_m: f64,
    /// Fracao do lote que a edificacao ocupa em planta (taxa de ocupacao).
    pub taxa_ocupacao: f64,
    /// Faixa de pavimentos, sorteada por lote.
    pub pavimentos: (u32, u32),
    /// Folga minima entre edificacoes vizinhas, em metros.
    pub afastamento_m: f64,
    /// Teto de edificacoes geradas.
    pub max_edificacoes: usize,
    /// Semente do sorteio. Fixa-la mantem a cidade identica entre execucoes.
    pub semente: u64,
}

impl Default for RegrasUrbanas {
    fn default() -> Self {
        Self {
            testada_m: 12.0,
            profundidade_m: 25.0,
            calcada_m: 2.0,
            recuo_frontal_m: 4.0,
            taxa_ocupacao: 0.55,
            pavimentos: (1, 3),
            afastamento_m: 2.5,
            max_edificacoes: 3_000,
            semente: 0x2E17_E5A1_14E7_0000,
        }
    }
}

impl RegrasUrbanas {
    /// Perfil mais denso, para eixos comerciais.
    pub fn adensado() -> Self {
        Self {
            testada_m: 14.0,
            profundidade_m: 28.0,
            recuo_frontal_m: 2.0,
            taxa_ocupacao: 0.70,
            pavimentos: (2, 6),
            ..Default::default()
        }
    }
}

/// Grade espacial uniforme para consulta de vizinhanca.
///
/// Sem ela o loteamento e O(n²) contra tudo que ja existe: alguns milhares de
/// candidatos contra alguns milhares de ocupantes trava a geracao.
struct Grade {
    celula: f64,
    baldes: std::collections::HashMap<(i64, i64), Vec<usize>>,
    /// Circulos envolventes: (centro, raio).
    ocupantes: Vec<(P2, f64)>,
}

impl Grade {
    fn nova(celula: f64) -> Self {
        Self {
            celula,
            baldes: std::collections::HashMap::new(),
            ocupantes: Vec::new(),
        }
    }

    fn chave(&self, p: P2) -> (i64, i64) {
        (
            (p[0] / self.celula).floor() as i64,
            (p[1] / self.celula).floor() as i64,
        )
    }

    fn inserir(&mut self, centro: P2, raio: f64) {
        let idx = self.ocupantes.len();
        self.ocupantes.push((centro, raio));
        let (x0, y0) = self.chave([centro[0] - raio, centro[1] - raio]);
        let (x1, y1) = self.chave([centro[0] + raio, centro[1] + raio]);
        for x in x0..=x1 {
            for y in y0..=y1 {
                self.baldes.entry((x, y)).or_default().push(idx);
            }
        }
    }

    /// Ha algum ocupante cujo circulo invada o disco (`centro`, `raio`)?
    fn colide(&self, centro: P2, raio: f64) -> bool {
        let (x0, y0) = self.chave([centro[0] - raio, centro[1] - raio]);
        let (x1, y1) = self.chave([centro[0] + raio, centro[1] + raio]);
        for x in x0..=x1 {
            for y in y0..=y1 {
                let Some(b) = self.baldes.get(&(x, y)) else {
                    continue;
                };
                for i in b {
                    let (c, r) = self.ocupantes[*i];
                    let d = (c[0] - centro[0]).hypot(c[1] - centro[1]);
                    if d < r + raio {
                        return true;
                    }
                }
            }
        }
        false
    }
}

/// Recorta o segmento `ab` ao quadrado de meia-extensao `m` (Liang-Barsky).
///
/// Devolve as duas pontas ja recortadas, ou `None` se o segmento nao encosta no
/// quadrado. Trata sozinho os tres casos que aparecem em dados de OSM:
///
/// - vertice dentro e o seguinte fora (a rua sai do recorte);
/// - os **dois** vertices fora, mas o segmento atravessando o recorte — uma rua
///   longa cujos vertices caem fora dos dois lados. Testar so os vertices, como
///   a primeira versao fazia, apagava essas ruas por inteiro;
/// - segmento degenerado (vertices repetidos, comuns no OSM).
fn clipar_segmento(a: P2, b: P2, m: f64) -> Option<(P2, P2)> {
    let d = [b[0] - a[0], b[1] - a[1]];
    if d[0].abs() < 1e-12 && d[1].abs() < 1e-12 {
        // Degenerado: so sobrevive se o ponto ja estiver dentro.
        return (a[0].abs() <= m && a[1].abs() <= m).then_some((a, a));
    }

    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    // Quatro bordas: -x >= -m, x <= m, -y >= -m, y <= m.
    for (p, q) in [
        (-d[0], a[0] + m),
        (d[0], m - a[0]),
        (-d[1], a[1] + m),
        (d[1], m - a[1]),
    ] {
        if p.abs() < 1e-12 {
            // Paralelo a esta borda: so falha se ja estiver do lado de fora.
            if q < 0.0 {
                return None;
            }
            continue;
        }
        let t = q / p;
        if p < 0.0 {
            if t > t1 {
                return None;
            }
            t0 = t0.max(t);
        } else {
            if t < t0 {
                return None;
            }
            t1 = t1.min(t);
        }
    }

    Some((
        [a[0] + d[0] * t0, a[1] + d[1] * t0],
        [a[0] + d[0] * t1, a[1] + d[1] * t1],
    ))
}

/// Distancia de um ponto ao segmento `ab`.
fn dist_segmento(p: P2, a: P2, b: P2) -> f64 {
    let (vx, vy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = vx * vx + vy * vy;
    if l2 < 1e-12 {
        return (p[0] - a[0]).hypot(p[1] - a[1]);
    }
    let t = (((p[0] - a[0]) * vx + (p[1] - a[1]) * vy) / l2).clamp(0.0, 1.0);
    (p[0] - (a[0] + t * vx)).hypot(p[1] - (a[1] + t * vy))
}

fn proximo(semente: &mut u64) -> f64 {
    *semente = semente.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *semente;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64
}

/// Perfil de ocupacao conforme a hierarquia viaria: avenida atrai comercio e
/// altura, servidao atrai casa baixa.
fn perfil(classe: ClasseVia, regras: &RegrasUrbanas, r: f64) -> (ClasseEdificio, u32) {
    let (min, max) = regras.pavimentos;
    match classe {
        ClasseVia::Rodovia | ClasseVia::Arterial => {
            let n = min + 1 + (r * (max as f64 + 2.0 - min as f64 - 1.0)) as u32;
            (
                if r > 0.55 {
                    ClasseEdificio::Apartamentos
                } else {
                    ClasseEdificio::Comercial
                },
                n.max(2),
            )
        }
        ClasseVia::Coletora => {
            let n = min + (r * (max as f64 + 1.0 - min as f64)) as u32;
            (
                if r > 0.7 {
                    ClasseEdificio::Comercial
                } else {
                    ClasseEdificio::Residencial
                },
                n.max(1),
            )
        }
        ClasseVia::Local => {
            let n = min + (r * (max - min + 1) as f64) as u32;
            (
                if r > 0.85 {
                    ClasseEdificio::Apartamentos
                } else {
                    ClasseEdificio::Residencial
                },
                n.clamp(1, max),
            )
        }
        _ => (ClasseEdificio::Residencial, 1),
    }
}

/// Loteia as frentes das vias e devolve as edificacoes sinteticas.
///
/// Elas saem no mesmo tipo [`Edificio`] do OSM, entao seguem pelo mesmo
/// caminho de extrusao — nada no gerador de malha precisa saber a diferenca.
pub fn gerar_edificacoes(
    entorno: &Entorno,
    frame: &EnuFrame,
    regras: RegrasUrbanas,
) -> Vec<Edificio> {
    if regras.max_edificacoes == 0 {
        return Vec::new();
    }
    let plano = |p: PontoGeo| -> P2 {
        let e = frame.geodetic_to_enu(Geodetic::new(p.lon, p.lat, 0.0));
        [e.e, e.n]
    };

    // Ocupacao inicial: o que o OSM ja mapeou tem precedencia absoluta.
    let mut grade = Grade::nova(30.0);
    for ed in &entorno.edificios {
        let anel: Vec<P2> = ed.contorno.iter().map(|p| plano(*p)).collect();
        if anel.len() < 3 {
            continue;
        }
        let c = triangulacao::centroide(&anel);
        let raio = anel
            .iter()
            .map(|p| (p[0] - c[0]).hypot(p[1] - c[1]))
            .fold(0.0, f64::max);
        grade.inserir(c, raio + regras.afastamento_m);
    }

    // Poligonos onde nao se constroi: agua, praia, parque, mata, quadra.
    let vedados: Vec<(Vec<P2>, [f64; 4])> = entorno
        .superficies
        .iter()
        .map(|s| {
            let anel: Vec<P2> = s.contorno.iter().map(|p| plano(*p)).collect();
            let env = triangulacao::envolvente(&anel);
            (anel, env)
        })
        .collect();

    // Eixos de todas as vias, para o lote nunca cair sobre a pista — inclusive
    // a de uma rua transversal, que e o caso que mais salta aos olhos.
    let eixos: Vec<(Vec<P2>, f64)> = entorno
        .vias
        .iter()
        .map(|v| {
            (
                v.eixo.iter().map(|p| plano(*p)).collect(),
                v.largura_m * 0.5 + regras.calcada_m,
            )
        })
        .collect();

    let mut saida = Vec::new();
    let mut semente = regras.semente;

    for via in &entorno.vias {
        if saida.len() >= regras.max_edificacoes {
            break;
        }
        // Trilha e calcada nao geram frente de lote.
        if matches!(via.classe, ClasseVia::Trilha | ClasseVia::Pedestre) {
            continue;
        }

        let eixo: Vec<P2> = via.eixo.iter().map(|p| plano(*p)).collect();
        if eixo.len() < 2 {
            continue;
        }

        // Distancia do eixo ate o **alinhamento** (a face frontal do lote).
        // O centro da edificacao so pode ser calculado depois de sortear a
        // profundidade dela, mais abaixo: usar uma profundidade media aqui
        // deixava as construcoes mais fundas invadirem o recuo frontal.
        let alinhamento = via.largura_m * 0.5 + regras.calcada_m + regras.recuo_frontal_m;

        let mut percorrido = 0.0f64;
        for i in 0..eixo.len() - 1 {
            let (a, b) = (eixo[i], eixo[i + 1]);
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let comp = (dx * dx + dy * dy).sqrt();
            if comp < 1e-6 {
                continue;
            }
            let d = [dx / comp, dy / comp];
            let n = [-d[1], d[0]];

            // Avanca em passos de uma testada ao longo do segmento.
            let mut s = regras.testada_m * 0.5 - percorrido % regras.testada_m;
            if s < 0.0 {
                s += regras.testada_m;
            }
            while s < comp {
                if saida.len() >= regras.max_edificacoes {
                    break;
                }
                let sobre_eixo = [a[0] + d[0] * s, a[1] + d[1] * s];

                for lado in [1.0f64, -1.0] {
                    if saida.len() >= regras.max_edificacoes {
                        break;
                    }
                    let r_forma = proximo(&mut semente);
                    let r_altura = proximo(&mut semente);
                    let r_pular = proximo(&mut semente);

                    // Vazios sao o que faz um bairro parecer bairro: lotes
                    // baldios, esquinas livres, estacionamento.
                    if r_pular < 0.18 {
                        continue;
                    }

                    // A edificacao ocupa parte do lote; o resto vira quintal.
                    let frente = regras.testada_m * (0.62 + r_forma * 0.24);
                    let fundo =
                        regras.profundidade_m * regras.taxa_ocupacao * (0.8 + r_forma * 0.3);

                    // O centro fica a meia profundidade do alinhamento, entao a
                    // face frontal cai exatamente no recuo, qualquer que seja o
                    // sorteio da profundidade.
                    let recuo = alinhamento + fundo * 0.5;
                    let centro = [
                        sobre_eixo[0] + n[0] * recuo * lado,
                        sobre_eixo[1] + n[1] * recuo * lado,
                    ];
                    let raio = (frente * frente + fundo * fundo).sqrt() * 0.5;

                    if grade.colide(centro, raio + regras.afastamento_m) {
                        continue;
                    }
                    // Nao construir sobre nenhuma pista.
                    if eixos.iter().any(|(pontos, meia)| {
                        pontos
                            .windows(2)
                            .any(|w| dist_segmento(centro, w[0], w[1]) < *meia + raio * 0.7)
                    }) {
                        continue;
                    }
                    // Nao construir dentro de agua, praia, parque ou mata.
                    if vedados.iter().any(|(anel, env)| {
                        centro[0] >= env[0]
                            && centro[0] <= env[2]
                            && centro[1] >= env[1]
                            && centro[1] <= env[3]
                            && triangulacao::contem(anel, centro)
                    }) {
                        continue;
                    }

                    let (classe, pavimentos) = perfil(via.classe, &regras, r_altura);
                    // Retangulo alinhado a rua: `d` e a frente, `n` a
                    // profundidade. E o que produz a leitura de quarteirao.
                    let (hf, hp) = (frente * 0.5, fundo * 0.5);
                    let canto = |sf: f64, sp: f64| -> PontoGeo {
                        let e = centro[0] + d[0] * hf * sf + n[0] * hp * sp;
                        let nn = centro[1] + d[1] * hf * sf + n[1] * hp * sp;
                        let g = frame.enu_to_geodetic(arcz_geo::Enu::new(e, nn, 0.0));
                        PontoGeo {
                            lat: g.lat_deg,
                            lon: g.lon_deg,
                        }
                    };

                    grade.inserir(centro, raio + regras.afastamento_m);
                    saida.push(Edificio {
                        // Id negativo: sinaliza "sintetico" sem colidir com o
                        // espaco de ids do OSM, que e sempre positivo.
                        id: -(saida.len() as i64 + 1),
                        nome: None,
                        classe,
                        contorno: vec![
                            canto(-1.0, -1.0),
                            canto(1.0, -1.0),
                            canto(1.0, 1.0),
                            canto(-1.0, 1.0),
                        ],
                        altura_m: pavimentos as f64 * ALTURA_PAVIMENTO_M,
                        base_m: 0.0,
                        fonte_altura: FonteAltura::Estimada,
                        telhado: Telhado::tipico(classe, pavimentos as f64),
                        cor_parede: None,
                        cor_telhado: None,
                    });
                }
                s += regras.testada_m;
            }
            percorrido += comp;
        }
    }

    saida
}

/// Recorta o entorno a um quadrado de `meia_extensao_m` em torno da origem.
///
/// **Necessario, nao cosmetico.** O Overpass devolve cada `way` *inteiro* assim
/// que ele toca a bbox — nao recortado. Uma rua de 3 km que cruza o canto do
/// recorte entra completa, e o loteamento a segue ate o fim: o bairro gerado
/// escapa muito alem do terreno carregado e fica boiando no ceu. Foi exatamente
/// o que apareceu no primeiro render sobre Bombinhas.
///
/// As vias sao **truncadas**, nao descartadas: cortar fora a metade que sai do
/// recorte preserva o trecho util. Os poligonos sao descartados por centroide,
/// porque recortar poligono exige Sutherland-Hodgman e o ganho nao paga — um
/// parque meio fora do recorte so aparece um pouco maior que o terreno.
pub fn recortar(entorno: &mut Entorno, frame: &EnuFrame, meia_extensao_m: f64) {
    let m = meia_extensao_m.max(1.0);
    let plano = |p: PontoGeo| -> P2 {
        let e = frame.geodetic_to_enu(Geodetic::new(p.lon, p.lat, 0.0));
        [e.e, e.n]
    };
    let dentro = |p: PontoGeo| {
        let q = plano(p);
        q[0].abs() <= m && q[1].abs() <= m
    };

    // Vias: parte cada eixo nos trechos que ficam dentro, **interpolando** o
    // ponto exato onde a rua cruza a divisa.
    //
    // A versao anterior guardava o primeiro vertice de fora para a rua "chegar
    // ate a borda". Mas esse vertice pode estar a centenas de metros dali, e o
    // resultado era faixa de asfalto saindo do terreno e flutuando no ceu — bem
    // visivel na primeira vista aerea de Bombinhas.
    let de_geo = |q: P2| -> PontoGeo {
        let g = frame.enu_to_geodetic(arcz_geo::Enu::new(q[0], q[1], 0.0));
        PontoGeo {
            lat: g.lat_deg,
            lon: g.lon_deg,
        }
    };

    let quase_igual = |a: P2, b: P2| (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-6;

    let mut vias = Vec::with_capacity(entorno.vias.len());
    for via in entorno.vias.drain(..) {
        let pts: Vec<P2> = via.eixo.iter().map(|p| plano(*p)).collect();
        let mut trecho: Vec<P2> = Vec::new();

        for par in pts.windows(2) {
            match clipar_segmento(par[0], par[1], m) {
                Some((ini, fim)) => {
                    // Emenda no trecho corrente quando este segmento comeca onde
                    // o anterior parou; senao, abre um trecho novo.
                    if trecho.last().is_none_or(|u| !quase_igual(*u, ini)) {
                        if trecho.len() >= 2 {
                            vias.push(Via {
                                eixo: trecho.iter().map(|q| de_geo(*q)).collect(),
                                ..via.clone()
                            });
                        }
                        trecho.clear();
                        trecho.push(ini);
                    }
                    if !quase_igual(ini, fim) {
                        trecho.push(fim);
                    }
                }
                None => {
                    if trecho.len() >= 2 {
                        vias.push(Via {
                            eixo: trecho.iter().map(|q| de_geo(*q)).collect(),
                            ..via.clone()
                        });
                    }
                    trecho.clear();
                }
            }
        }
        if trecho.len() >= 2 {
            vias.push(Via {
                eixo: trecho.iter().map(|q| de_geo(*q)).collect(),
                ..via
            });
        }
    }
    entorno.vias = vias;

    let centro_dentro = |anel: &[PontoGeo]| {
        if anel.is_empty() {
            return false;
        }
        let pts: Vec<P2> = anel.iter().map(|p| plano(*p)).collect();
        let c = triangulacao::centroide(&pts);
        c[0].abs() <= m * 1.5 && c[1].abs() <= m * 1.5
    };
    entorno.edificios.retain(|e| centro_dentro(&e.contorno));
    entorno.superficies.retain(|s| centro_dentro(&s.contorno));
    entorno.arvores.retain(|a| dentro(a.posicao));
}

/// Preenche o entorno no lugar, mantendo o que o OSM ja tinha.
pub fn adensar(entorno: &mut Entorno, frame: &EnuFrame, regras: RegrasUrbanas) -> usize {
    let novos = gerar_edificacoes(entorno, frame, regras);
    let n = novos.len();
    entorno.edificios.extend(novos);
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> EnuFrame {
        EnuFrame::new(Geodetic::new(-48.5022653, -27.1544967, 0.0))
    }

    fn plano(f: &EnuFrame, p: PontoGeo) -> P2 {
        let e = f.geodetic_to_enu(Geodetic::new(p.lon, p.lat, 0.0));
        [e.e, e.n]
    }

    /// Rua reta de ~400 m no eixo leste-oeste, passando pela origem.
    fn rua(classe: ClasseVia, largura: f64) -> Via {
        let (lat, lon) = (-27.1544967, -48.5022653);
        Via {
            id: 1,
            nome: Some("Rua de teste".into()),
            classe,
            largura_m: largura,
            eixo: vec![
                PontoGeo {
                    lat,
                    lon: lon - 0.002,
                },
                PontoGeo {
                    lat,
                    lon: lon + 0.002,
                },
            ],
        }
    }

    fn entorno_com_rua() -> Entorno {
        Entorno {
            vias: vec![rua(ClasseVia::Local, 6.0)],
            ..Default::default()
        }
    }

    #[test]
    fn uma_rua_vazia_ganha_edificacoes_dos_dois_lados() {
        let e = entorno_com_rua();
        let f = frame();
        let novos = gerar_edificacoes(&e, &f, RegrasUrbanas::default());
        assert!(
            novos.len() > 10,
            "so {} edificacoes em 400 m de rua",
            novos.len()
        );

        let lados: Vec<f64> = novos
            .iter()
            .map(|ed| {
                let anel: Vec<P2> = ed.contorno.iter().map(|p| plano(&f, *p)).collect();
                triangulacao::centroide(&anel)[1].signum()
            })
            .collect();
        assert!(lados.iter().any(|s| *s > 0.0), "nada ao norte da rua");
        assert!(lados.iter().any(|s| *s < 0.0), "nada ao sul da rua");
    }

    #[test]
    fn nenhuma_edificacao_invade_a_pista() {
        // O erro mais visivel possivel: casa no meio da rua.
        let e = entorno_com_rua();
        let f = frame();
        let via = &e.vias[0];
        let meia = via.largura_m * 0.5;

        for ed in gerar_edificacoes(&e, &f, RegrasUrbanas::default()) {
            for p in &ed.contorno {
                let q = plano(&f, *p);
                assert!(
                    q[1].abs() > meia,
                    "vertice a {:.2} m do eixo, dentro da pista de {meia} m",
                    q[1].abs()
                );
            }
        }
    }

    #[test]
    fn o_recuo_frontal_e_respeitado() {
        let e = entorno_com_rua();
        let f = frame();
        let r = RegrasUrbanas::default();
        let minimo = e.vias[0].largura_m * 0.5 + r.calcada_m + r.recuo_frontal_m;

        for ed in gerar_edificacoes(&e, &f, r) {
            let d = ed
                .contorno
                .iter()
                .map(|p| plano(&f, *p)[1].abs())
                .fold(f64::MAX, f64::min);
            assert!(d >= minimo - 0.5, "recuo de {d:.2} m, minimo {minimo} m");
        }
    }

    #[test]
    fn o_recuo_vale_para_qualquer_profundidade_sorteada() {
        // O teste anterior passou por sorte durante um tempo: o recuo era medido
        // a partir de uma profundidade *media*, entao as edificacoes mais fundas
        // avancavam sobre a calcada. Trocar a semente por outro motivo expos o
        // caso. Aqui varias sementes cobrem a faixa inteira do sorteio.
        let f = frame();
        let e = entorno_com_rua();
        let r = RegrasUrbanas::default();
        let minimo = e.vias[0].largura_m * 0.5 + r.calcada_m + r.recuo_frontal_m;

        for semente in [0u64, 1, 7, 42, 1234, u64::MAX / 3, u64::MAX] {
            let regras = RegrasUrbanas { semente, ..r };
            for ed in gerar_edificacoes(&e, &f, regras) {
                let d = ed
                    .contorno
                    .iter()
                    .map(|p| plano(&f, *p)[1].abs())
                    .fold(f64::MAX, f64::min);
                assert!(
                    d >= minimo - 0.01,
                    "semente {semente}: recuo de {d:.2} m, minimo {minimo} m"
                );
            }
        }
    }

    #[test]
    fn as_edificacoes_nao_se_sobrepoem() {
        let e = entorno_com_rua();
        let f = frame();
        let novos = gerar_edificacoes(&e, &f, RegrasUrbanas::default());

        let discos: Vec<(P2, f64)> = novos
            .iter()
            .map(|ed| {
                let anel: Vec<P2> = ed.contorno.iter().map(|p| plano(&f, *p)).collect();
                let c = triangulacao::centroide(&anel);
                // Raio inscrito: metade da menor dimensao. Dois retangulos cujos
                // circulos inscritos nao se tocam nao podem se sobrepor.
                let r = anel
                    .iter()
                    .map(|p| (p[0] - c[0]).hypot(p[1] - c[1]))
                    .fold(f64::MAX, f64::min);
                (c, r)
            })
            .collect();

        for i in 0..discos.len() {
            for j in i + 1..discos.len() {
                let (a, ra) = discos[i];
                let (b, rb) = discos[j];
                let d = (a[0] - b[0]).hypot(a[1] - b[1]);
                assert!(d > ra + rb, "edificacoes {i} e {j} se sobrepoem (d={d:.2})");
            }
        }
    }

    #[test]
    fn o_que_o_osm_mapeou_bloqueia_o_gerador() {
        // Onde ha levantamento real, o sintetico desvia — nunca sobrescreve.
        let f = frame();
        let mut e = entorno_com_rua();
        let (lat, lon) = (-27.1544967 + 0.00025, -48.5022653);
        let d = 0.0004;
        e.edificios.push(Edificio {
            id: 999,
            nome: Some("Existente".into()),
            classe: ClasseEdificio::Apartamentos,
            contorno: vec![
                PontoGeo { lat, lon: lon - d },
                PontoGeo { lat, lon: lon + d },
                PontoGeo {
                    lat: lat + 0.0002,
                    lon: lon + d,
                },
                PontoGeo {
                    lat: lat + 0.0002,
                    lon: lon - d,
                },
            ],
            altura_m: 24.0,
            base_m: 0.0,
            fonte_altura: FonteAltura::Medida,
            telhado: Telhado::Plano,
            cor_parede: None,
            cor_telhado: None,
        });

        let anel: Vec<P2> = e.edificios[0]
            .contorno
            .iter()
            .map(|p| plano(&f, *p))
            .collect();
        let centro = triangulacao::centroide(&anel);
        let raio = anel
            .iter()
            .map(|p| (p[0] - centro[0]).hypot(p[1] - centro[1]))
            .fold(0.0, f64::max);

        for ed in gerar_edificacoes(&e, &f, RegrasUrbanas::default()) {
            let c = triangulacao::centroide(
                &ed.contorno
                    .iter()
                    .map(|p| plano(&f, *p))
                    .collect::<Vec<_>>(),
            );
            let dist = (c[0] - centro[0]).hypot(c[1] - centro[1]);
            assert!(dist > raio, "edificacao sintetica dentro do predio mapeado");
        }
    }

    #[test]
    fn nao_se_constroi_dentro_da_agua_nem_do_parque() {
        let f = frame();
        let mut e = entorno_com_rua();
        let (lat, lon) = (-27.1544967, -48.5022653);
        // Lagoa cobrindo todo o lado norte da rua.
        e.superficies.push(Superficie {
            id: 1,
            classe: ClasseSuperficie::Agua,
            contorno: vec![
                PontoGeo {
                    lat: lat + 0.00008,
                    lon: lon - 0.002,
                },
                PontoGeo {
                    lat: lat + 0.00008,
                    lon: lon + 0.002,
                },
                PontoGeo {
                    lat: lat + 0.001,
                    lon: lon + 0.002,
                },
                PontoGeo {
                    lat: lat + 0.001,
                    lon: lon - 0.002,
                },
            ],
        });

        let anel: Vec<P2> = e.superficies[0]
            .contorno
            .iter()
            .map(|p| plano(&f, *p))
            .collect();
        let novos = gerar_edificacoes(&e, &f, RegrasUrbanas::default());
        assert!(!novos.is_empty(), "o lado sul deveria continuar loteado");

        for ed in &novos {
            let c = triangulacao::centroide(
                &ed.contorno
                    .iter()
                    .map(|p| plano(&f, *p))
                    .collect::<Vec<_>>(),
            );
            assert!(!triangulacao::contem(&anel, c), "edificacao dentro da agua");
        }
    }

    #[test]
    fn a_geracao_e_identica_entre_execucoes() {
        // Sem isso, reabrir o projeto reconstroi a cidade inteira diferente e
        // qualquer enquadramento de camera salvo perde o sentido.
        let e = entorno_com_rua();
        let f = frame();
        let a = gerar_edificacoes(&e, &f, RegrasUrbanas::default());
        let b = gerar_edificacoes(&e, &f, RegrasUrbanas::default());
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.altura_m, y.altura_m);
            assert_eq!(x.contorno[0].lat, y.contorno[0].lat);
            assert_eq!(x.contorno[0].lon, y.contorno[0].lon);
        }
    }

    #[test]
    fn sementes_diferentes_produzem_cidades_diferentes() {
        let e = entorno_com_rua();
        let f = frame();
        let a = gerar_edificacoes(&e, &f, RegrasUrbanas::default());
        let b = gerar_edificacoes(
            &e,
            &f,
            RegrasUrbanas {
                semente: 7,
                ..Default::default()
            },
        );
        let iguais = a
            .iter()
            .zip(b.iter())
            .filter(|(x, y)| x.altura_m == y.altura_m)
            .count();
        assert!(iguais < a.len(), "a semente nao mudou nada");
    }

    #[test]
    fn a_avenida_gera_mais_altura_que_a_servidao() {
        // Hierarquia viaria e o principal sinal de densidade num bairro.
        let f = frame();
        let media = |classe, largura| {
            let e = Entorno {
                vias: vec![rua(classe, largura)],
                ..Default::default()
            };
            let v = gerar_edificacoes(&e, &f, RegrasUrbanas::default());
            v.iter().map(|x| x.altura_m).sum::<f64>() / v.len() as f64
        };
        let avenida = media(ClasseVia::Arterial, 12.0);
        let local = media(ClasseVia::Local, 6.0);
        assert!(
            avenida > local,
            "avenida {avenida:.1} m <= local {local:.1} m"
        );
    }

    #[test]
    fn calcada_e_trilha_nao_geram_lote() {
        let f = frame();
        for classe in [ClasseVia::Pedestre, ClasseVia::Trilha] {
            let e = Entorno {
                vias: vec![rua(classe, 2.0)],
                ..Default::default()
            };
            assert!(
                gerar_edificacoes(&e, &f, RegrasUrbanas::default()).is_empty(),
                "{classe:?} gerou lote"
            );
        }
    }

    #[test]
    fn o_teto_de_edificacoes_e_respeitado() {
        let e = entorno_com_rua();
        let regras = RegrasUrbanas {
            max_edificacoes: 5,
            ..Default::default()
        };
        assert_eq!(gerar_edificacoes(&e, &frame(), regras).len(), 5);
        let zero = RegrasUrbanas {
            max_edificacoes: 0,
            ..Default::default()
        };
        assert!(gerar_edificacoes(&e, &frame(), zero).is_empty());
    }

    #[test]
    fn as_sinteticas_sao_distinguiveis_das_reais() {
        // O usuario precisa saber o que e levantamento e o que e palpite antes
        // de renderizar um 8K e apresentar a um cliente.
        for ed in gerar_edificacoes(&entorno_com_rua(), &frame(), RegrasUrbanas::default()) {
            assert!(ed.id < 0, "id sintetico deveria ser negativo: {}", ed.id);
            assert_eq!(ed.fonte_altura, FonteAltura::Estimada);
        }
    }

    #[test]
    fn recortar_trunca_a_via_que_atravessa_a_borda() {
        // O caso que quebrou o primeiro render: o Overpass entrega o way
        // inteiro, e o loteamento seguia a rua por quilometros alem do terreno.
        let f = frame();
        let (lat, lon) = (-27.1544967, -48.5022653);
        let mut e = Entorno {
            vias: vec![Via {
                id: 1,
                nome: None,
                classe: ClasseVia::Local,
                largura_m: 6.0,
                // ~1 km de extensao, saindo largamente do recorte de 200 m.
                eixo: vec![
                    PontoGeo {
                        lat,
                        lon: lon - 0.005,
                    },
                    PontoGeo { lat, lon },
                    PontoGeo {
                        lat,
                        lon: lon + 0.005,
                    },
                ],
            }],
            ..Default::default()
        };
        recortar(&mut e, &f, 200.0);

        for via in &e.vias {
            for p in &via.eixo {
                let q = plano(&f, *p);
                assert!(
                    q[0].abs() < 700.0,
                    "vertice a {:.0} m do centro sobreviveu ao recorte de 200 m",
                    q[0].abs()
                );
            }
        }
    }

    #[test]
    fn a_via_recortada_termina_exatamente_na_divisa() {
        // O defeito visto no render: a rua saia do terreno e ficava boiando,
        // porque o recorte guardava o proximo vertice do OSM — que podia estar
        // centenas de metros alem da borda.
        let f = frame();
        let (lat, lon) = (-27.1544967, -48.5022653);
        let mut e = Entorno {
            vias: vec![Via {
                id: 1,
                nome: None,
                classe: ClasseVia::Local,
                largura_m: 6.0,
                eixo: vec![
                    PontoGeo {
                        lat,
                        lon: lon - 0.02,
                    },
                    PontoGeo { lat, lon },
                    PontoGeo {
                        lat,
                        lon: lon + 0.02,
                    },
                ],
            }],
            ..Default::default()
        };
        let m = 200.0;
        recortar(&mut e, &f, m);

        assert!(!e.vias.is_empty(), "a rua sumiu inteira");
        for via in &e.vias {
            for p in &via.eixo {
                let q = plano(&f, *p);
                assert!(
                    q[0].abs() <= m + 0.5 && q[1].abs() <= m + 0.5,
                    "vertice em {q:?} passou da divisa de {m} m"
                );
            }
        }
        // E precisa mesmo alcancar a borda, nao parar no ultimo vertice interno.
        let alcance = e
            .vias
            .iter()
            .flat_map(|v| v.eixo.iter())
            .map(|p| plano(&f, *p)[0].abs())
            .fold(0.0, f64::max);
        assert!(alcance > m - 1.0, "a rua parou a {alcance:.1} m da divisa");
    }

    #[test]
    fn o_clipping_corta_na_borda_do_quadrado() {
        let m = 100.0;

        // Sai pela direita.
        let (a, b) = clipar_segmento([0.0, 0.0], [400.0, 0.0], m).unwrap();
        assert!(a[0].abs() < 1e-9, "inicio moveu: {a:?}");
        assert!((b[0] - m).abs() < 1e-9, "fim em {b:?}");

        // Diagonal: para no eixo que atinge o limite primeiro.
        let (_, b) = clipar_segmento([0.0, 0.0], [400.0, 200.0], m).unwrap();
        assert!(
            (b[0] - m).abs() < 1e-9 && (b[1] - 50.0).abs() < 1e-9,
            "{b:?}"
        );

        // Atravessa inteiro sem nenhum vertice dentro — o caso que apagava ruas.
        let (a, b) = clipar_segmento([-400.0, 0.0], [400.0, 0.0], m).unwrap();
        assert!(
            (a[0] + m).abs() < 1e-9 && (b[0] - m).abs() < 1e-9,
            "{a:?} {b:?}"
        );

        // Totalmente fora, sem tocar o quadrado.
        assert!(clipar_segmento([200.0, 200.0], [400.0, 400.0], m).is_none());
        assert!(clipar_segmento([-400.0, 300.0], [400.0, 300.0], m).is_none());

        // Degenerado: sobrevive so se ja estiver dentro.
        assert!(clipar_segmento([5.0, 5.0], [5.0, 5.0], m).is_some());
        assert!(clipar_segmento([500.0, 5.0], [500.0, 5.0], m).is_none());
    }

    #[test]
    fn uma_via_que_atravessa_o_recorte_vira_um_trecho_so() {
        // Entra por um lado e sai pelo outro: um unico trecho, com as duas
        // pontas na divisa.
        let f = frame();
        let (lat, lon) = (-27.1544967, -48.5022653);
        let mut e = Entorno {
            vias: vec![Via {
                id: 1,
                nome: None,
                classe: ClasseVia::Local,
                largura_m: 6.0,
                eixo: vec![
                    PontoGeo {
                        lat,
                        lon: lon - 0.02,
                    },
                    PontoGeo {
                        lat,
                        lon: lon + 0.02,
                    },
                ],
            }],
            ..Default::default()
        };
        recortar(&mut e, &f, 200.0);
        assert_eq!(e.vias.len(), 1);
        assert_eq!(e.vias[0].eixo.len(), 2);
    }

    #[test]
    fn recortar_derruba_o_que_esta_totalmente_fora() {
        let f = frame();
        let (lat, lon) = (-27.1544967, -48.5022653);
        let longe = PontoGeo {
            lat: lat + 0.05,
            lon: lon + 0.05,
        };
        let mut e = Entorno {
            arvores: vec![
                Arvore {
                    posicao: PontoGeo { lat, lon },
                    altura_m: 8.0,
                    raio_copa_m: 2.0,
                    giro_rad: 0.0,
                    mapeada: true,
                },
                Arvore {
                    posicao: longe,
                    altura_m: 8.0,
                    raio_copa_m: 2.0,
                    giro_rad: 0.0,
                    mapeada: true,
                },
            ],
            ..Default::default()
        };
        recortar(&mut e, &f, 200.0);
        assert_eq!(e.arvores.len(), 1, "a arvore distante deveria ter saido");
    }

    #[test]
    fn recortar_preserva_o_que_esta_dentro() {
        let mut e = entorno_com_rua();
        let antes = e.vias[0].eixo.len();
        recortar(&mut e, &frame(), 5_000.0);
        assert_eq!(e.vias.len(), 1);
        assert_eq!(e.vias[0].eixo.len(), antes, "recorte folgado alterou a via");
    }

    #[test]
    fn o_recorte_limita_a_extensao_do_loteamento() {
        // A verificacao de ponta a ponta do bug: sem recorte o bairro escapa do
        // terreno; com recorte ele fica contido.
        let f = frame();
        let (lat, lon) = (-27.1544967, -48.5022653);
        let mut e = Entorno {
            vias: vec![Via {
                id: 1,
                nome: None,
                classe: ClasseVia::Local,
                largura_m: 6.0,
                eixo: vec![
                    PontoGeo {
                        lat,
                        lon: lon - 0.01,
                    },
                    PontoGeo {
                        lat,
                        lon: lon + 0.01,
                    },
                ],
            }],
            ..Default::default()
        };
        recortar(&mut e, &f, 300.0);
        for ed in gerar_edificacoes(&e, &f, RegrasUrbanas::default()) {
            let c = triangulacao::centroide(
                &ed.contorno
                    .iter()
                    .map(|p| plano(&f, *p))
                    .collect::<Vec<_>>(),
            );
            assert!(
                c[0].abs() < 1_200.0,
                "edificacao a {:.0} m, muito alem do recorte de 300 m",
                c[0].abs()
            );
        }
    }

    #[test]
    fn adensar_preserva_o_que_ja_existia() {
        let mut e = entorno_com_rua();
        e.edificios.push(Edificio {
            id: 500,
            nome: Some("Real".into()),
            classe: ClasseEdificio::Outro,
            contorno: vec![],
            altura_m: 10.0,
            base_m: 0.0,
            fonte_altura: FonteAltura::Medida,
            telhado: Telhado::Plano,
            cor_parede: None,
            cor_telhado: None,
        });
        let n = adensar(&mut e, &frame(), RegrasUrbanas::default());
        assert!(n > 0);
        assert_eq!(e.edificios.len(), n + 1);
        assert_eq!(e.edificios[0].id, 500, "o predio real saiu da lista");
    }

    #[test]
    fn a_grade_espacial_acha_o_que_esta_perto_e_ignora_o_que_esta_longe() {
        let mut g = Grade::nova(10.0);
        g.inserir([0.0, 0.0], 5.0);
        assert!(g.colide([4.0, 0.0], 2.0));
        assert!(!g.colide([100.0, 100.0], 2.0));
        // Encostando exatamente na borda nao e colisao.
        assert!(!g.colide([8.0, 0.0], 3.0));
        // Atravessando varias celulas.
        g.inserir([95.0, 95.0], 40.0);
        assert!(g.colide([60.0, 95.0], 1.0));
    }

    #[test]
    fn a_distancia_ao_segmento_trata_as_pontas() {
        let (a, b) = ([0.0, 0.0], [10.0, 0.0]);
        assert!((dist_segmento([5.0, 3.0], a, b) - 3.0).abs() < 1e-9);
        // Alem da ponta: a distancia e ate o extremo, nao ate a reta infinita.
        assert!((dist_segmento([14.0, 0.0], a, b) - 4.0).abs() < 1e-9);
        assert!((dist_segmento([-3.0, 4.0], a, b) - 5.0).abs() < 1e-9);
        // Segmento degenerado nao pode dar NaN.
        assert!(dist_segmento([1.0, 1.0], a, a).is_finite());
    }
}

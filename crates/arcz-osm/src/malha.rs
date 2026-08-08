//! Converte o `Entorno` normalizado em geometria 3D no quadro ENU do projeto.
//!
//! Regras que valem para tudo aqui:
//!
//! - Toda conta acontece em `f64` no quadro ENU; a queda para `f32` e a ultima
//!   coisa, ja com o valor pequeno (metros a partir da origem local). E a mesma
//!   disciplina do `arcz-geo` — misturar as duas coisas traz o jitter de volta.
//! - Nenhum triangulo e emitido "no escuro": `emitir` confere a normal
//!   geometrica contra a normal desejada e corrige o winding. Raciocinar sobre
//!   handedness na mao (o render usa x=leste, y=cima, **z=-norte**, que e uma
//!   reflexao do plano ENU) e onde esse tipo de codigo costuma errar.
//! - A altura do terreno entra por callback. Assim este crate nao depende do
//!   `arcz-terrain`, e os testes rodam com um terreno analitico.

use arcz_geo::{EnuFrame, Geodetic};

use crate::entidade::*;
use crate::triangulacao::{self, P2};

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertice {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

/// De onde a malha veio, para o Outliner e para o clique devolverem o objeto
/// certo em vez de "um triangulo do entorno".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origem {
    Edificio(i64),
    Vias(ClasseVia),
    Superficie(ClasseSuperficie),
    Vegetacao,
}

#[derive(Debug, Clone)]
pub struct MalhaProcedural {
    pub nome: String,
    pub origem: Origem,
    pub cor: [f32; 3],
    pub vertices: Vec<Vertice>,
    pub indices: Vec<u32>,
}

impl MalhaProcedural {
    fn nova(nome: String, origem: Origem, cor: [f32; 3]) -> Self {
        Self {
            nome,
            origem,
            cor,
            vertices: Vec::new(),
            indices: Vec::new(),
        }
    }

    pub fn triangulos(&self) -> usize {
        self.indices.len() / 3
    }

    /// Caixa envolvente em coordenadas de render: `(min, max)`.
    pub fn envolvente(&self) -> ([f32; 3], [f32; 3]) {
        let mut min = [f32::MAX; 3];
        let mut max = [f32::MIN; 3];
        for v in &self.vertices {
            for i in 0..3 {
                min[i] = min[i].min(v.pos[i]);
                max[i] = max[i].max(v.pos[i]);
            }
        }
        (min, max)
    }

    /// Emite um triangulo garantindo que ele seja visivel do lado de `normal`.
    ///
    /// O winding correto nao e obvio: o plano de trabalho e (leste, norte) e o
    /// render e (x=leste, y=cima, z=-norte) — uma reflexao, que troca a
    /// orientacao. Em vez de deduzir, mede-se: se a normal geometrica aponta ao
    /// contrario da desejada, trocam-se dois indices.
    fn emitir(
        &mut self,
        a: [f64; 3],
        b: [f64; 3],
        c: [f64; 3],
        normal: [f64; 3],
        uv: [[f32; 2]; 3],
    ) {
        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let geo = [
            ab[1] * ac[2] - ab[2] * ac[1],
            ab[2] * ac[0] - ab[0] * ac[2],
            ab[0] * ac[1] - ab[1] * ac[0],
        ];
        let escala = geo[0] * geo[0] + geo[1] * geo[1] + geo[2] * geo[2];
        if escala < 1e-16 {
            return; // triangulo degenerado: area zero, nao contribui com nada
        }
        let inverter = geo[0] * normal[0] + geo[1] * normal[1] + geo[2] * normal[2] < 0.0;

        let n32 = [normal[0] as f32, normal[1] as f32, normal[2] as f32];
        let base = self.vertices.len() as u32;
        for (p, t) in [(a, uv[0]), (b, uv[1]), (c, uv[2])] {
            self.vertices.push(Vertice {
                pos: [p[0] as f32, p[1] as f32, p[2] as f32],
                normal: n32,
                uv: t,
            });
        }
        if inverter {
            self.indices.extend_from_slice(&[base, base + 2, base + 1]);
        } else {
            self.indices.extend_from_slice(&[base, base + 1, base + 2]);
        }
    }
}

/// Ajustes do gerador.
#[derive(Debug, Clone, Copy)]
pub struct Opcoes {
    /// Semear arvores dentro dos poligonos de mata/grama, alem das mapeadas.
    pub semear_vegetacao: bool,
    /// Teto de arvores geradas, para nao explodir a cena numa area florestada.
    pub max_arvores: usize,
    /// Gerar as superficies de uso do solo (parque, praia, agua).
    pub superficies: bool,
    /// Gerar as faixas de via.
    pub vias: bool,
}

impl Default for Opcoes {
    fn default() -> Self {
        Self {
            semear_vegetacao: true,
            max_arvores: 4_000,
            superficies: true,
            vias: true,
        }
    }
}

/// Terreno consultavel: recebe (leste, norte) em metros no quadro ENU e devolve
/// a cota em metros.
pub trait Terreno {
    fn altura(&self, leste: f64, norte: f64) -> f64;
}

/// Terreno plano, util quando o DEM ainda nao carregou e nos testes.
pub struct TerrenoPlano(pub f64);

impl Terreno for TerrenoPlano {
    fn altura(&self, _e: f64, _n: f64) -> f64 {
        self.0
    }
}

impl<F: Fn(f64, f64) -> f64> Terreno for F {
    fn altura(&self, e: f64, n: f64) -> f64 {
        self(e, n)
    }
}

/// Projeta um ponto geografico no plano ENU (leste, norte), em metros.
fn para_plano(frame: &EnuFrame, p: PontoGeo) -> P2 {
    let e = frame.geodetic_to_enu(Geodetic::new(p.lon, p.lat, 0.0));
    [e.e, e.n]
}

/// Converte (leste, norte, cota) para a ordem que os shaders esperam.
fn para_render(e: f64, n: f64, u: f64) -> [f64; 3] {
    [e, u, -n]
}

/// Gera todas as malhas do entorno.
pub fn gerar(
    entorno: &Entorno,
    frame: &EnuFrame,
    terreno: &impl Terreno,
    op: Opcoes,
) -> Vec<MalhaProcedural> {
    let mut malhas = Vec::new();

    for ed in &entorno.edificios {
        if let Some(m) = gerar_edificio(ed, frame, terreno) {
            malhas.push(m);
        }
    }

    if op.vias {
        malhas.extend(gerar_vias(&entorno.vias, frame, terreno));
    }
    if op.superficies {
        malhas.extend(gerar_superficies(&entorno.superficies, frame, terreno));
    }

    let arvores = reunir_arvores(entorno, frame, op);
    if !arvores.is_empty() {
        malhas.push(gerar_vegetacao(&arvores, terreno));
    }

    malhas.retain(|m| !m.indices.is_empty());
    malhas
}

/// Extruda um predio: paredes verticais + laje de cobertura.
pub fn gerar_edificio(
    ed: &Edificio,
    frame: &EnuFrame,
    terreno: &impl Terreno,
) -> Option<MalhaProcedural> {
    let anel: Vec<P2> = ed.contorno.iter().map(|p| para_plano(frame, *p)).collect();
    if anel.len() < 3 || triangulacao::area(&anel) < 1.0 {
        // Abaixo de 1 m² e ruido de mapeamento, nao edificacao.
        return None;
    }

    // A cota da base e a **menor** do contorno. Usar a media deixaria metade do
    // predio flutuando na encosta; usar a maior o enterraria. A menor garante
    // que ele encoste no chao em todo o perimetro, e a saia fica escondida.
    let cota = anel
        .iter()
        .map(|p| terreno.altura(p[0], p[1]))
        .fold(f64::MAX, f64::min);

    let base = cota + ed.base_m;
    let topo = base + ed.altura_m;

    let nome = ed
        .nome
        .clone()
        .unwrap_or_else(|| format!("Edificacao {}", ed.id));
    // Cor amostrada da ortofoto quando o app a forneceu; senao, a tipica da
    // classe. Prefere-se a da cobertura porque e ela que domina a vista aerea,
    // que e o enquadramento de apresentacao mais comum.
    //
    // Parede e cobertura ainda dividem um material so. Separa-las exige duas
    // malhas por edificacao (o dobro de draw calls num bairro de 774), e o
    // ganho nao se justifica antes do PBR entrar.
    let cor = ed
        .cor_telhado
        .or(ed.cor_parede)
        .unwrap_or_else(|| ed.classe.cor_base());
    let mut m = MalhaProcedural::nova(nome, Origem::Edificio(ed.id), cor);

    // Paredes. A UV horizontal acompanha o comprimento em metros e a vertical a
    // altura em metros, para a textura nao esticar em fachadas longas.
    let n = anel.len();
    for i in 0..n {
        let a = anel[i];
        let b = anel[(i + 1) % n];
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let comp = (dx * dx + dy * dy).sqrt();
        if comp < 1e-6 {
            continue;
        }
        // Normal horizontal da parede, no espaco de render.
        let normal = para_render(dy / comp, -dx / comp, 0.0);
        let alt = (topo - base) as f32;
        let c = comp as f32;

        let a0 = para_render(a[0], a[1], base);
        let b0 = para_render(b[0], b[1], base);
        let a1 = para_render(a[0], a[1], topo);
        let b1 = para_render(b[0], b[1], topo);

        m.emitir(a0, b0, b1, normal, [[0.0, 0.0], [c, 0.0], [c, alt]]);
        m.emitir(a0, b1, a1, normal, [[0.0, 0.0], [c, alt], [0.0, alt]]);
    }

    // Cobertura.
    match ed.telhado {
        Telhado::DuasAguas { altura_m, .. } if altura_m > 0.05 => {
            duas_aguas(&mut m, &anel, topo, altura_m)
        }
        _ => laje(&mut m, &anel, topo),
    }

    (!m.indices.is_empty()).then_some(m)
}

/// Laje plana: o anel triangulado na cota do topo.
fn laje(m: &mut MalhaProcedural, anel: &[P2], topo: f64) {
    let cima = [0.0, 1.0, 0.0];
    for t in triangulacao::triangular(anel) {
        let p: Vec<[f64; 3]> = t
            .iter()
            .map(|k| para_render(anel[*k][0], anel[*k][1], topo))
            .collect();
        let uv = [
            [anel[t[0]][0] as f32, anel[t[0]][1] as f32],
            [anel[t[1]][0] as f32, anel[t[1]][1] as f32],
            [anel[t[2]][0] as f32, anel[t[2]][1] as f32],
        ];
        m.emitir(p[0], p[1], p[2], cima, uv);
    }
}

/// Telhado de duas águas sobre a caixa envolvente da planta.
///
/// É o que separa "casa" de "caixa" numa vista aérea — e a maior parte de um
/// bairro brasileiro de baixa densidade é casa com telha aparente, não laje.
///
/// A cumeeira **precisa de vértices próprios**: os do contorno estão todos na
/// borda, e usá-los para o telhado deixa tudo na altura da parede. Por isso a
/// geometria é construída do zero — duas águas inclinadas e duas empenas
/// triangulares — em vez de reaproveitar a triangulação do contorno.
///
/// A cumeeira corre pelo **eixo maior**: no sentido curto as águas sairiam
/// curtas e íngremes, o oposto do que se constrói.
///
/// Só se aplica sobre a caixa envolvente, o que é exato para planta retangular
/// (o caso das edificações sintéticas, que são retângulos) e aproximado para
/// contorno irregular do OSM. Um "L" ganha um telhado que cobre o retângulo
/// inteiro — visualmente aceitável a distância de bairro, e honesto: a
/// alternativa seria um telhado furado.
fn duas_aguas(m: &mut MalhaProcedural, anel: &[P2], topo: f64, altura: f64) {
    let [x0, y0, x1, y1] = triangulacao::envolvente(anel);
    let (largura, profundidade) = (x1 - x0, y1 - y0);
    // Planta degenerada não tem telhado que faça sentido.
    if largura < 1e-6 || profundidade < 1e-6 {
        laje(m, anel, topo);
        return;
    }

    let cume = topo + altura;
    // Cantos da caixa, na altura da parede.
    let (a, b, c, d) = (
        [x0, y0], // sudoeste
        [x1, y0], // sudeste
        [x1, y1], // nordeste
        [x0, y1], // noroeste
    );

    // Cumeeira ao longo do eixo maior, no meio do eixo menor.
    let (r0, r1, agua1, agua2, empena1, empena2) = if largura >= profundidade {
        let ym = (y0 + y1) * 0.5;
        (
            [x0, ym],
            [x1, ym],
            (a, b), // água sul
            (c, d), // água norte
            (a, d), // empena oeste
            (b, c), // empena leste
        )
    } else {
        let xm = (x0 + x1) * 0.5;
        (
            [xm, y0],
            [xm, y1],
            (b, c), // água leste
            (d, a), // água oeste
            (a, b), // empena sul
            (c, d), // empena norte
        )
    };

    let na_parede = |p: P2| para_render(p[0], p[1], topo);
    let no_cume = |p: P2| para_render(p[0], p[1], cume);
    let uv0 = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]];
    let uv1 = [[0.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

    // As duas águas. A normal é a geométrica do próprio quadrilátero, virada
    // para cima — `emitir` corrige o winding a partir dela.
    for (p, q) in [agua1, agua2] {
        let (pp, qq) = (na_parede(p), na_parede(q));
        // A cumeeira é a mesma aresta para as duas águas; o que muda entre elas
        // é a aresta da parede que cada uma encontra.
        let (c0, c1) = (no_cume(r0), no_cume(r1));
        let n = normal_para_cima(pp, qq, c1);
        m.emitir(pp, qq, c1, n, uv0);
        m.emitir(pp, c1, c0, n, uv1);
    }

    // As duas empenas: triângulo entre a parede e a cumeeira em cada topo.
    for (p, q) in [empena1, empena2] {
        let (pp, qq) = (na_parede(p), na_parede(q));
        let meio = [(p[0] + q[0]) * 0.5, (p[1] + q[1]) * 0.5];
        let topo_empena = no_cume(meio);
        let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
        let comp = (dx * dx + dy * dy).sqrt();
        if comp < 1e-6 {
            continue;
        }
        let normal = para_render(dy / comp, -dx / comp, 0.0);
        m.emitir(pp, qq, topo_empena, normal, uv0);
    }
}

/// Normal geométrica do triângulo, sempre apontando para cima.
fn normal_para_cima(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> [f64; 3] {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
    ];
    if n[1] < 0.0 {
        [-n[0], -n[1], -n[2]]
    } else {
        n
    }
}

/// Faixas de rodagem, agregadas por classe para nao gerar um draw call por rua.
fn gerar_vias(vias: &[Via], frame: &EnuFrame, terreno: &impl Terreno) -> Vec<MalhaProcedural> {
    use std::collections::BTreeMap;
    let mut por_classe: BTreeMap<u8, MalhaProcedural> = BTreeMap::new();

    for via in vias {
        let eixo: Vec<P2> = via.eixo.iter().map(|p| para_plano(frame, *p)).collect();
        let bordas = match faixa(&eixo, via.largura_m * 0.5) {
            Some(b) => b,
            None => continue,
        };

        let chave = via.classe as u8;
        let m = por_classe.entry(chave).or_insert_with(|| {
            MalhaProcedural::nova(
                format!("Vias — {}", nome_classe_via(via.classe)),
                Origem::Vias(via.classe),
                via.classe.cor_base(),
            )
        });

        let folga = via.classe.folga_m();
        let cima = [0.0, 1.0, 0.0];
        let mut percorrido = 0.0f64;

        for i in 0..bordas.len() - 1 {
            let (e0, d0) = bordas[i];
            let (e1, d1) = bordas[i + 1];
            let passo = {
                let c = eixo[i + 1];
                let a = eixo[i];
                ((c[0] - a[0]).powi(2) + (c[1] - a[1]).powi(2)).sqrt()
            };

            let cota = |p: P2| terreno.altura(p[0], p[1]) + folga;
            let pe0 = para_render(e0[0], e0[1], cota(e0));
            let pd0 = para_render(d0[0], d0[1], cota(d0));
            let pe1 = para_render(e1[0], e1[1], cota(e1));
            let pd1 = para_render(d1[0], d1[1], cota(d1));

            let (v0, v1) = (percorrido as f32, (percorrido + passo) as f32);
            let w = via.largura_m as f32;
            m.emitir(pe0, pd0, pd1, cima, [[0.0, v0], [w, v0], [w, v1]]);
            m.emitir(pe0, pd1, pe1, cima, [[0.0, v0], [w, v1], [0.0, v1]]);
            percorrido += passo;
        }
    }

    por_classe.into_values().collect()
}

fn nome_classe_via(c: ClasseVia) -> &'static str {
    match c {
        ClasseVia::Rodovia => "rodovia",
        ClasseVia::Arterial => "arterial",
        ClasseVia::Coletora => "coletora",
        ClasseVia::Local => "local",
        ClasseVia::Servico => "servico",
        ClasseVia::Pedestre => "calcada",
        ClasseVia::Trilha => "trilha",
    }
}

/// Calcula as bordas esquerda e direita de uma polyline com *miter join*.
///
/// Deslocar cada segmento sozinho abre buracos em forma de cunha em cada curva.
/// O miter fecha a junta usando a bissetriz; o limite de 4x evita a espicula
/// infinita quando a via dobra quase 180° (retorno de rua sem saida).
fn faixa(eixo: &[P2], meia: f64) -> Option<Vec<(P2, P2)>> {
    // Remove pontos repetidos, que zeram a direcao e propagam NaN.
    let mut p: Vec<P2> = Vec::with_capacity(eixo.len());
    for q in eixo {
        if p.last()
            .map(|u: &P2| (u[0] - q[0]).hypot(u[1] - q[1]) > 1e-6)
            .unwrap_or(true)
        {
            p.push(*q);
        }
    }
    if p.len() < 2 || meia <= 0.0 {
        return None;
    }

    let dir = |a: P2, b: P2| -> P2 {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l = (dx * dx + dy * dy).sqrt();
        [dx / l, dy / l]
    };

    let mut saida = Vec::with_capacity(p.len());
    for i in 0..p.len() {
        let d = if i == 0 {
            dir(p[0], p[1])
        } else if i == p.len() - 1 {
            dir(p[i - 1], p[i])
        } else {
            let a = dir(p[i - 1], p[i]);
            let b = dir(p[i], p[i + 1]);
            let s = [a[0] + b[0], a[1] + b[1]];
            let l = (s[0] * s[0] + s[1] * s[1]).sqrt();
            if l < 1e-9 {
                // Inversao de 180°: nao ha bissetriz, mantem a direcao anterior.
                a
            } else {
                [s[0] / l, s[1] / l]
            }
        };

        // Normal a esquerda da direcao de marcha.
        let nrm = [-d[1], d[0]];

        // Fator de miter: alonga o deslocamento na curva para as bordas se
        // encontrarem exatamente na bissetriz.
        let fator = if i == 0 || i == p.len() - 1 {
            1.0
        } else {
            let a = dir(p[i - 1], p[i]);
            let cos = (a[0] * d[0] + a[1] * d[1]).clamp(-1.0, 1.0);
            (1.0 / cos.max(0.25)).min(4.0)
        };

        let off = meia * fator;
        saida.push((
            [p[i][0] + nrm[0] * off, p[i][1] + nrm[1] * off],
            [p[i][0] - nrm[0] * off, p[i][1] - nrm[1] * off],
        ));
    }
    Some(saida)
}

/// Poligonos de uso do solo, agregados por classe.
fn gerar_superficies(
    sups: &[Superficie],
    frame: &EnuFrame,
    terreno: &impl Terreno,
) -> Vec<MalhaProcedural> {
    use std::collections::BTreeMap;
    let mut por_classe: BTreeMap<u8, MalhaProcedural> = BTreeMap::new();
    let cima = [0.0, 1.0, 0.0];

    for s in sups {
        let anel: Vec<P2> = s.contorno.iter().map(|p| para_plano(frame, *p)).collect();
        if anel.len() < 3 || triangulacao::area(&anel) < 4.0 {
            continue;
        }
        let m = por_classe.entry(s.classe as u8).or_insert_with(|| {
            MalhaProcedural::nova(
                format!("Superficie — {}", nome_classe_superficie(s.classe)),
                Origem::Superficie(s.classe),
                s.classe.cor_base(),
            )
        });

        let folga = s.classe.folga_m();
        // A agua e plana: ondular a lamina pelo DEM produz um lago inclinado.
        // A cota vira a mediana amostrada do contorno.
        let cota_agua = if s.classe == ClasseSuperficie::Agua {
            let mut hs: Vec<f64> = anel.iter().map(|p| terreno.altura(p[0], p[1])).collect();
            hs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            Some(hs[hs.len() / 2] + folga)
        } else {
            None
        };

        for t in triangulacao::triangular(&anel) {
            let p: Vec<[f64; 3]> = t
                .iter()
                .map(|k| {
                    let q = anel[*k];
                    let u = cota_agua.unwrap_or_else(|| terreno.altura(q[0], q[1]) + folga);
                    para_render(q[0], q[1], u)
                })
                .collect();
            let uv = [
                [anel[t[0]][0] as f32 * 0.1, anel[t[0]][1] as f32 * 0.1],
                [anel[t[1]][0] as f32 * 0.1, anel[t[1]][1] as f32 * 0.1],
                [anel[t[2]][0] as f32 * 0.1, anel[t[2]][1] as f32 * 0.1],
            ];
            m.emitir(p[0], p[1], p[2], cima, uv);
        }
    }

    por_classe.into_values().collect()
}

fn nome_classe_superficie(c: ClasseSuperficie) -> &'static str {
    match c {
        ClasseSuperficie::Mata => "mata",
        ClasseSuperficie::Grama => "grama",
        ClasseSuperficie::Praia => "areia",
        ClasseSuperficie::Agua => "agua",
        ClasseSuperficie::Quadra => "quadra",
    }
}

/// Junta as arvores mapeadas com as semeadas nos poligonos de vegetacao.
fn reunir_arvores(entorno: &Entorno, frame: &EnuFrame, op: Opcoes) -> Vec<(P2, Arvore)> {
    let mut saida: Vec<(P2, Arvore)> = entorno
        .arvores
        .iter()
        .map(|a| (para_plano(frame, a.posicao), *a))
        .collect();

    if !op.semear_vegetacao {
        saida.truncate(op.max_arvores);
        return saida;
    }

    for s in &entorno.superficies {
        let densidade = s.classe.arvores_por_hectare();
        if densidade <= 0.0 || saida.len() >= op.max_arvores {
            continue;
        }
        let anel: Vec<P2> = s.contorno.iter().map(|p| para_plano(frame, *p)).collect();
        let a_m2 = triangulacao::area(&anel);
        if a_m2 < 25.0 {
            continue;
        }

        // Grade com jitter: distribui sem os aglomerados que a amostragem
        // puramente aleatoria produz, e o passo sai da densidade pedida.
        let passo = (10_000.0 / densidade).sqrt().max(2.0);
        let [x0, y0, x1, y1] = triangulacao::envolvente(&anel);
        let mut semente = s.id as u64 ^ 0xA5A5_5A5A_1234_9876;

        let mut y = y0;
        while y < y1 && saida.len() < op.max_arvores {
            let mut x = x0;
            while x < x1 && saida.len() < op.max_arvores {
                let (r1, s1) = proximo(semente);
                let (r2, s2) = proximo(s1);
                let (r3, s3) = proximo(s2);
                semente = s3;

                let _ = r3;
                let p = [x + (r1 - 0.5) * passo, y + (r2 - 0.5) * passo];
                if triangulacao::contem(&anel, p) {
                    // Espécie e porte vêm de `especies`, a partir da própria
                    // posição. Antes disso toda árvore era sorteada na mesma
                    // faixa de 6 a 14 m com a mesma esbeltez — um bosque
                    // uniforme, que não parece bairro. Agora palmeira é alta e
                    // estreita, arbusto não passa de altura de muro, e a mesma
                    // coordenada devolve sempre a mesma árvore.
                    let bioma = match s.classe {
                        ClasseSuperficie::Mata => crate::especies::Bioma::MATA_ATLANTICA,
                        _ => crate::especies::Bioma::LITORAL_SC,
                    };
                    let inst = crate::especies::resolver(bioma, p[0], p[1], s.id as u64);
                    saida.push((
                        p,
                        Arvore {
                            posicao: PontoGeo { lat: 0.0, lon: 0.0 },
                            altura_m: inst.altura_m,
                            raio_copa_m: inst.raio_copa_m,
                            giro_rad: inst.giro_rad,
                            mapeada: false,
                        },
                    ));
                }
                x += passo;
            }
            y += passo;
        }
    }

    saida.truncate(op.max_arvores);
    saida
}

/// PRNG determinístico. A vegetacao precisa cair no mesmo lugar toda vez que o
/// projeto abre — uma floresta que se reorganiza a cada carregamento tornaria
/// impossivel enquadrar uma camera.
fn proximo(semente: u64) -> (f64, u64) {
    let mut z = semente.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let s = z;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    ((z >> 11) as f64 / (1u64 << 53) as f64, s)
}

/// Uma malha unica com todas as arvores: tronco prismatico + copa conica dupla.
///
/// Uma malha por arvore geraria milhares de draw calls. Como a vegetacao de
/// fundo nao e selecionavel individualmente, agregar e a escolha certa.
fn gerar_vegetacao(arvores: &[(P2, Arvore)], terreno: &impl Terreno) -> MalhaProcedural {
    let mut m = MalhaProcedural::nova(
        format!("Vegetacao ({} arvores)", arvores.len()),
        Origem::Vegetacao,
        [0.20, 0.38, 0.16],
    );

    const LADOS: usize = 6;
    for (p, arv) in arvores {
        let base = terreno.altura(p[0], p[1]);
        let h = arv.altura_m;
        let r_tronco = (h * 0.035).clamp(0.06, 0.5);
        let h_tronco = h * 0.35;
        let r_copa = arv.raio_copa_m.max(0.5);

        let anguloi = |i: usize| arv.giro_rad + (i as f64 / LADOS as f64) * std::f64::consts::TAU;

        // Tronco.
        for i in 0..LADOS {
            let (a0, a1) = (anguloi(i), anguloi(i + 1));
            let q0 = [p[0] + r_tronco * a0.cos(), p[1] + r_tronco * a0.sin()];
            let q1 = [p[0] + r_tronco * a1.cos(), p[1] + r_tronco * a1.sin()];
            let nx = (a0 + a1) * 0.5;
            let normal = para_render(nx.cos(), nx.sin(), 0.0);
            let b0 = para_render(q0[0], q0[1], base);
            let b1 = para_render(q1[0], q1[1], base);
            let t0 = para_render(q0[0], q0[1], base + h_tronco);
            let t1 = para_render(q1[0], q1[1], base + h_tronco);
            m.emitir(b0, b1, t1, normal, [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]]);
            m.emitir(b0, t1, t0, normal, [[0.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        }

        // Copa: dois cones empilhados, o de baixo mais largo.
        for (nivel, (z0, z1, raio)) in [
            (base + h_tronco * 0.8, base + h * 0.78, r_copa),
            (base + h * 0.62, base + h, r_copa * 0.62),
        ]
        .into_iter()
        .enumerate()
        {
            let apice = para_render(p[0], p[1], z1);
            let giro = arv.giro_rad + nivel as f64 * 0.5;
            for i in 0..LADOS {
                let a0 = giro + (i as f64 / LADOS as f64) * std::f64::consts::TAU;
                let a1 = giro + ((i + 1) as f64 / LADOS as f64) * std::f64::consts::TAU;
                let q0 = [p[0] + raio * a0.cos(), p[1] + raio * a0.sin()];
                let q1 = [p[0] + raio * a1.cos(), p[1] + raio * a1.sin()];
                let c0 = para_render(q0[0], q0[1], z0);
                let c1 = para_render(q1[0], q1[1], z0);
                // Normal inclinada para fora e para cima, como a face do cone.
                let am = (a0 + a1) * 0.5;
                let incl = (z1 - z0).max(0.1);
                let l = (raio * raio + incl * incl).sqrt();
                let normal = para_render(am.cos() * incl / l, am.sin() * incl / l, raio / l);
                m.emitir(c0, c1, apice, normal, [[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]]);
                // Fundo da saia, para a copa nao ser vazada vista de baixo.
                let centro = para_render(p[0], p[1], z0);
                m.emitir(
                    c0,
                    centro,
                    c1,
                    [0.0, -1.0, 0.0],
                    [[0.0, 0.0], [0.5, 0.5], [1.0, 0.0]],
                );
            }
        }
    }

    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> EnuFrame {
        EnuFrame::new(Geodetic::new(-48.5022653, -27.1544967, 0.0))
    }

    /// Quadrado de ~20 m de lado em torno da origem do frame.
    fn contorno_quadrado(lado_graus: f64) -> Vec<PontoGeo> {
        let (lat, lon) = (-27.1544967, -48.5022653);
        vec![
            PontoGeo { lat, lon },
            PontoGeo {
                lat,
                lon: lon + lado_graus,
            },
            PontoGeo {
                lat: lat + lado_graus,
                lon: lon + lado_graus,
            },
            PontoGeo {
                lat: lat + lado_graus,
                lon,
            },
        ]
    }

    fn edificio(altura: f64) -> Edificio {
        Edificio {
            id: 7,
            nome: Some("Teste".into()),
            classe: ClasseEdificio::Apartamentos,
            contorno: contorno_quadrado(0.0002),
            altura_m: altura,
            base_m: 0.0,
            fonte_altura: FonteAltura::Medida,
            telhado: Telhado::Plano,
            cor_parede: None,
            cor_telhado: None,
        }
    }

    /// Confere que a normal gravada em cada vertice concorda com a normal
    /// geometrica do triangulo — ou seja, que o winding esta certo em todo lugar.
    fn winding_consistente(m: &MalhaProcedural) -> bool {
        m.indices.chunks(3).all(|t| {
            let (a, b, c) = (
                m.vertices[t[0] as usize],
                m.vertices[t[1] as usize],
                m.vertices[t[2] as usize],
            );
            let ab = [
                b.pos[0] - a.pos[0],
                b.pos[1] - a.pos[1],
                b.pos[2] - a.pos[2],
            ];
            let ac = [
                c.pos[0] - a.pos[0],
                c.pos[1] - a.pos[1],
                c.pos[2] - a.pos[2],
            ];
            let g = [
                ab[1] * ac[2] - ab[2] * ac[1],
                ab[2] * ac[0] - ab[0] * ac[2],
                ab[0] * ac[1] - ab[1] * ac[0],
            ];
            g[0] * a.normal[0] + g[1] * a.normal[1] + g[2] * a.normal[2] >= -1e-6
        })
    }

    #[test]
    fn o_predio_tem_a_altura_pedida_e_encosta_no_terreno() {
        let m = gerar_edificio(&edificio(24.0), &frame(), &TerrenoPlano(12.0)).unwrap();
        let (min, max) = m.envolvente();
        assert!((min[1] - 12.0).abs() < 0.01, "base em {}", min[1]);
        assert!((max[1] - 36.0).abs() < 0.01, "topo em {}", max[1]);
    }

    #[test]
    fn todas_as_faces_do_predio_apontam_para_fora() {
        // Este e o teste que protege contra o erro classico: o plano de trabalho
        // e (leste, norte) e o render e (x, y, -n), uma reflexao. Deduzir o
        // winding na mao inverte metade das paredes, e o predio fica vazado.
        let m = gerar_edificio(&edificio(15.0), &frame(), &TerrenoPlano(0.0)).unwrap();
        assert!(winding_consistente(&m), "ha faces com winding invertido");
    }

    #[test]
    fn a_contagem_de_triangulos_bate_com_a_geometria() {
        // 4 paredes x 2 triangulos + 2 do telhado.
        let m = gerar_edificio(&edificio(10.0), &frame(), &TerrenoPlano(0.0)).unwrap();
        assert_eq!(m.triangulos(), 10);
    }

    #[test]
    fn na_encosta_o_predio_assenta_pela_cota_mais_baixa() {
        // Terreno inclinado 10% no leste. Usar a media deixaria metade do predio
        // no ar; a cota minima garante contato em todo o perimetro.
        let rampa = |e: f64, _n: f64| e * 0.1;
        let m = gerar_edificio(&edificio(9.0), &frame(), &rampa).unwrap();
        let (min, _) = m.envolvente();
        let esperado = m
            .vertices
            .iter()
            .map(|v| v.pos[0] as f64 * 0.1)
            .fold(f64::MAX, f64::min);
        assert!(
            (min[1] as f64 - esperado).abs() < 0.5,
            "base {} nao acompanha a cota minima {esperado}",
            min[1]
        );
    }

    #[test]
    fn predio_minusculo_e_descartado() {
        let mut ed = edificio(5.0);
        ed.contorno = contorno_quadrado(0.000_002); // ~0,2 m de lado
        assert!(gerar_edificio(&ed, &frame(), &TerrenoPlano(0.0)).is_none());
    }

    #[test]
    fn a_faixa_de_via_nao_abre_buraco_na_curva() {
        // Cotovelo de 90°. Sem miter, o lado interno da curva abre uma cunha e a
        // rua aparece rasgada.
        let eixo: Vec<P2> = vec![[0.0, 0.0], [20.0, 0.0], [20.0, 20.0]];
        let b = faixa(&eixo, 3.0).unwrap();
        assert_eq!(b.len(), 3);

        // No vertice do cotovelo o deslocamento tem de ser maior que a meia
        // largura — e a assinatura do miter.
        let (esq, dir) = b[1];
        let d = ((esq[0] - dir[0]).powi(2) + (esq[1] - dir[1]).powi(2)).sqrt();
        assert!(d > 6.0, "largura na curva {d} nao foi alargada pelo miter");
        assert!(d < 6.0 * 4.0, "miter explodiu para {d}");
    }

    #[test]
    fn a_faixa_ignora_pontos_repetidos() {
        // Duplicatas existem no OSM e zeram a direcao, propagando NaN por toda a
        // malha — o sintoma e a cena inteira sumir.
        let eixo: Vec<P2> = vec![[0.0, 0.0], [0.0, 0.0], [10.0, 0.0], [10.0, 0.0]];
        let b = faixa(&eixo, 3.0).unwrap();
        assert_eq!(b.len(), 2);
        for (e, d) in b {
            assert!(
                e[0].is_finite() && e[1].is_finite(),
                "NaN na borda esquerda"
            );
            assert!(d[0].is_finite() && d[1].is_finite(), "NaN na borda direita");
        }
    }

    #[test]
    fn faixa_com_um_ponto_so_nao_gera_geometria() {
        assert!(faixa(&[[0.0, 0.0]], 3.0).is_none());
        assert!(faixa(&[], 3.0).is_none());
        assert!(faixa(&[[0.0, 0.0], [1.0, 0.0]], 0.0).is_none());
    }

    #[test]
    fn a_via_fica_acima_do_terreno_para_nao_brigar_no_z_buffer() {
        let via = Via {
            id: 1,
            nome: None,
            classe: ClasseVia::Local,
            largura_m: 6.0,
            eixo: contorno_quadrado(0.0003),
        };
        let ms = gerar_vias(&[via], &frame(), &TerrenoPlano(5.0));
        assert_eq!(ms.len(), 1);
        let (min, _) = ms[0].envolvente();
        let folga = ClasseVia::Local.folga_m() as f32;
        assert!(
            (min[1] - (5.0 + folga)).abs() < 1e-3,
            "via em {} em vez de {}",
            min[1],
            5.0 + folga
        );
        assert!(winding_consistente(&ms[0]));
    }

    #[test]
    fn as_vias_sao_agregadas_por_classe() {
        // Uma malha por rua daria centenas de draw calls num bairro.
        let fazer = |id, classe| Via {
            id,
            nome: None,
            classe,
            largura_m: 6.0,
            eixo: contorno_quadrado(0.0003),
        };
        let ms = gerar_vias(
            &[
                fazer(1, ClasseVia::Local),
                fazer(2, ClasseVia::Local),
                fazer(3, ClasseVia::Arterial),
            ],
            &frame(),
            &TerrenoPlano(0.0),
        );
        assert_eq!(ms.len(), 2, "esperava uma malha por classe");
    }

    #[test]
    fn a_lamina_de_agua_e_plana_mesmo_com_o_terreno_inclinado() {
        // Um lago que acompanha o DEM sai inclinado — visualmente errado e
        // imediatamente perceptivel numa renderizacao.
        let rampa = |e: f64, _n: f64| e * 0.05;
        let s = Superficie {
            id: 1,
            classe: ClasseSuperficie::Agua,
            contorno: contorno_quadrado(0.0004),
        };
        let ms = gerar_superficies(&[s], &frame(), &rampa);
        assert_eq!(ms.len(), 1);
        let (min, max) = ms[0].envolvente();
        assert!(
            (max[1] - min[1]).abs() < 1e-3,
            "lamina de agua variou {} m em altura",
            max[1] - min[1]
        );
    }

    #[test]
    fn a_grama_acompanha_o_relevo() {
        // O oposto da agua: um gramado plano numa encosta flutuaria.
        let rampa = |e: f64, _n: f64| e * 0.05;
        let s = Superficie {
            id: 1,
            classe: ClasseSuperficie::Grama,
            contorno: contorno_quadrado(0.0004),
        };
        let ms = gerar_superficies(&[s], &frame(), &rampa);
        let (min, max) = ms[0].envolvente();
        assert!(max[1] - min[1] > 0.5, "gramado saiu plano sobre a rampa");
    }

    #[test]
    fn a_vegetacao_e_semeada_dentro_da_mata_e_so_dentro() {
        let s = Superficie {
            id: 42,
            classe: ClasseSuperficie::Mata,
            contorno: contorno_quadrado(0.0006),
        };
        let entorno = Entorno {
            superficies: vec![s.clone()],
            ..Default::default()
        };
        let f = frame();
        let arvores = reunir_arvores(&entorno, &f, Opcoes::default());
        assert!(!arvores.is_empty(), "nenhuma arvore semeada na mata");

        let anel: Vec<P2> = s.contorno.iter().map(|p| para_plano(&f, *p)).collect();
        for (p, _) in &arvores {
            assert!(
                triangulacao::contem(&anel, *p),
                "arvore em {p:?} caiu fora do poligono"
            );
        }
    }

    #[test]
    fn a_semeadura_e_identica_entre_execucoes() {
        // Reabrir o projeto nao pode reorganizar a floresta: enquadramentos de
        // camera salvos deixariam de valer.
        let entorno = Entorno {
            superficies: vec![Superficie {
                id: 42,
                classe: ClasseSuperficie::Mata,
                contorno: contorno_quadrado(0.0006),
            }],
            ..Default::default()
        };
        let f = frame();
        let a = reunir_arvores(&entorno, &f, Opcoes::default());
        let b = reunir_arvores(&entorno, &f, Opcoes::default());
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.0, y.0);
            assert_eq!(x.1.altura_m, y.1.altura_m);
        }
    }

    #[test]
    fn o_teto_de_arvores_e_respeitado() {
        let entorno = Entorno {
            superficies: vec![Superficie {
                id: 1,
                classe: ClasseSuperficie::Mata,
                contorno: contorno_quadrado(0.01), // ~1,1 km de lado
            }],
            ..Default::default()
        };
        let op = Opcoes {
            max_arvores: 50,
            ..Default::default()
        };
        assert_eq!(reunir_arvores(&entorno, &frame(), op).len(), 50);
    }

    #[test]
    fn a_malha_de_vegetacao_tem_winding_consistente() {
        let arv = Arvore {
            posicao: PontoGeo { lat: 0.0, lon: 0.0 },
            altura_m: 10.0,
            raio_copa_m: 3.0,
            giro_rad: 0.7,
            mapeada: true,
        };
        let m = gerar_vegetacao(&[([0.0, 0.0], arv)], &TerrenoPlano(0.0));
        assert!(m.triangulos() > 0);
        assert!(winding_consistente(&m), "copa ou tronco com face invertida");

        let (min, max) = m.envolvente();
        assert!(
            (min[1]).abs() < 0.01,
            "arvore nao encosta no chao: {}",
            min[1]
        );
        assert!((max[1] - 10.0).abs() < 0.01, "altura da arvore: {}", max[1]);
    }

    #[test]
    fn gerar_produz_todas_as_camadas_e_nenhuma_malha_vazia() {
        let entorno = Entorno {
            edificios: vec![edificio(20.0)],
            vias: vec![Via {
                id: 1,
                nome: None,
                classe: ClasseVia::Local,
                largura_m: 6.0,
                eixo: contorno_quadrado(0.0005),
            }],
            superficies: vec![Superficie {
                id: 2,
                classe: ClasseSuperficie::Grama,
                contorno: contorno_quadrado(0.0005),
            }],
            arvores: vec![Arvore {
                posicao: PontoGeo {
                    lat: -27.1544,
                    lon: -48.5022,
                },
                altura_m: 8.0,
                raio_copa_m: 2.5,
                giro_rad: 0.0,
                mapeada: true,
            }],
        };
        let malhas = gerar(&entorno, &frame(), &TerrenoPlano(3.0), Opcoes::default());

        assert!(malhas
            .iter()
            .any(|m| matches!(m.origem, Origem::Edificio(_))));
        assert!(malhas.iter().any(|m| matches!(m.origem, Origem::Vias(_))));
        assert!(malhas
            .iter()
            .any(|m| matches!(m.origem, Origem::Superficie(_))));
        assert!(malhas.iter().any(|m| m.origem == Origem::Vegetacao));

        for m in &malhas {
            assert!(!m.indices.is_empty(), "malha vazia: {}", m.nome);
            assert!(winding_consistente(m), "winding invertido em {}", m.nome);
            assert!(
                m.vertices
                    .iter()
                    .all(|v| v.pos.iter().all(|c| c.is_finite())),
                "NaN em {}",
                m.nome
            );
        }
    }

    #[test]
    fn o_telhado_de_duas_aguas_sobe_acima_da_parede() {
        // O que separa "casa" de "caixa" na vista aerea. O tipo `Telhado` ja
        // existia e o gerador o ignorava: tudo saia com laje.
        let mut ed = edificio(6.0);
        ed.telhado = Telhado::DuasAguas {
            altura_m: 2.0,
            beiral_m: 0.0,
        };
        let m = gerar_edificio(&ed, &frame(), &TerrenoPlano(0.0)).unwrap();
        let (_, max) = m.envolvente();
        assert!(
            (max[1] - 8.0).abs() < 0.01,
            "topo em {} m; parede 6 + cumeeira 2 = 8",
            max[1]
        );
    }

    #[test]
    fn a_laje_nao_sobe_nada() {
        let mut ed = edificio(6.0);
        ed.telhado = Telhado::Plano;
        let m = gerar_edificio(&ed, &frame(), &TerrenoPlano(0.0)).unwrap();
        let (_, max) = m.envolvente();
        assert!((max[1] - 6.0).abs() < 0.01, "topo em {}", max[1]);
    }

    #[test]
    fn o_telhado_de_duas_aguas_tem_winding_consistente() {
        // Agua com face invertida some no culling e deixa o predio aberto.
        let mut ed = edificio(6.0);
        ed.telhado = Telhado::DuasAguas {
            altura_m: 2.5,
            beiral_m: 0.0,
        };
        let m = gerar_edificio(&ed, &frame(), &TerrenoPlano(0.0)).unwrap();
        assert!(winding_consistente(&m), "face invertida no telhado");
    }

    #[test]
    fn a_cumeeira_segue_o_eixo_maior_da_planta() {
        // Cumeeira no sentido curto daria aguas ingremes e curtas — o oposto do
        // que se constroi. Numa planta alongada em leste-oeste, o ponto mais
        // alto tem de estar no meio em norte-sul.
        let (lat, lon) = (-27.1544967, -48.5022653);
        let mut ed = edificio(5.0);
        ed.telhado = Telhado::DuasAguas {
            altura_m: 3.0,
            beiral_m: 0.0,
        };
        // 4x mais larga em longitude que em latitude.
        ed.contorno = vec![
            PontoGeo { lat, lon },
            PontoGeo {
                lat,
                lon: lon + 0.0008,
            },
            PontoGeo {
                lat: lat + 0.0002,
                lon: lon + 0.0008,
            },
            PontoGeo {
                lat: lat + 0.0002,
                lon,
            },
        ];
        let m = gerar_edificio(&ed, &frame(), &TerrenoPlano(0.0)).unwrap();

        // O vertice mais alto deve estar perto do centro em Z (norte-sul), nao
        // numa das pontas em X.
        let alto = m
            .vertices
            .iter()
            .max_by(|a, b| a.pos[1].partial_cmp(&b.pos[1]).unwrap())
            .unwrap();
        let (min, max) = m.envolvente();
        let centro_z = (min[2] + max[2]) * 0.5;
        let meia_z = (max[2] - min[2]) * 0.5;
        assert!(
            (alto.pos[2] - centro_z).abs() < meia_z * 0.35,
            "cumeeira em z={} nao esta no meio ({centro_z} +- {meia_z})",
            alto.pos[2]
        );
    }

    #[test]
    fn planta_degenerada_cai_para_laje_sem_nan() {
        // Meia-largura zero levaria a divisao por zero e NaN na malha inteira.
        let (lat, lon) = (-27.1544967, -48.5022653);
        let mut ed = edificio(4.0);
        ed.telhado = Telhado::DuasAguas {
            altura_m: 2.0,
            beiral_m: 0.0,
        };
        // Faixa muito fina: profundidade quase nula.
        ed.contorno = vec![
            PontoGeo { lat, lon },
            PontoGeo {
                lat,
                lon: lon + 0.0004,
            },
            PontoGeo {
                lat: lat + 0.0000001,
                lon: lon + 0.0004,
            },
            PontoGeo {
                lat: lat + 0.0000001,
                lon,
            },
        ];
        if let Some(m) = gerar_edificio(&ed, &frame(), &TerrenoPlano(0.0)) {
            assert!(
                m.vertices
                    .iter()
                    .all(|v| v.pos.iter().all(|c| c.is_finite())),
                "NaN na malha"
            );
        }
    }

    #[test]
    fn entorno_vazio_gera_lista_vazia() {
        let m = gerar(
            &Entorno::default(),
            &frame(),
            &TerrenoPlano(0.0),
            Opcoes::default(),
        );
        assert!(m.is_empty());
    }
}

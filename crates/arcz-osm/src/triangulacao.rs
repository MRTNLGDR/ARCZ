//! Triangulacao de poligonos simples por *ear clipping*.
//!
//! Um leque de triangulos a partir do primeiro vertice (`fan`) so funciona em
//! poligonos convexos. Contornos de predio no OSM sao raramente convexos — um
//! "L", um "U" ou um patio interno sao comuns — e o leque produz triangulos
//! **fora** do predio, que aparecem como abas atravessando a rua.
//!
//! Ear clipping e O(n²) no pior caso, mas n aqui e a contagem de vertices de um
//! footprint: quase sempre entre 4 e 40. Uma malha de Delaunay ou uma
//! dependencia externa (earcutr, lyon) nao pagariam o custo.
//!
//! Buracos (patios internos) **nao** sao tratados: exigem multipoligono do OSM,
//! que a consulta atual nao pede. Um predio com patio sai solido — visualmente
//! aceitavel e honesto, ja que a alternativa seria geometria invertida.

/// Ponto 2D no plano local, em metros.
pub type P2 = [f64; 2];

/// Area assinada pelo teorema do shoelace. Positiva = anti-horaria.
pub fn area_assinada(p: &[P2]) -> f64 {
    let n = p.len();
    if n < 3 {
        return 0.0;
    }
    let mut s = 0.0;
    for i in 0..n {
        let j = (i + 1) % n;
        s += p[i][0] * p[j][1] - p[j][0] * p[i][1];
    }
    s * 0.5
}

/// Area absoluta em m², util para densidade de vegetacao e para o HUD.
pub fn area(p: &[P2]) -> f64 {
    area_assinada(p).abs()
}

fn cruz(o: P2, a: P2, b: P2) -> f64 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

/// `p` esta dentro do triangulo `abc` (bordas contam como dentro)?
fn dentro(a: P2, b: P2, c: P2, p: P2) -> bool {
    let d1 = cruz(a, b, p);
    let d2 = cruz(b, c, p);
    let d3 = cruz(c, a, p);
    let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(neg && pos)
}

/// Triangula o anel e devolve indices em trincas, referindo o array original.
///
/// A saida esta sempre em **ordem anti-horaria** (normal para +Z no plano),
/// independentemente da orientacao do contorno de entrada — o OSM nao garante
/// orientacao, e telhados com winding invertido somem no *backface culling*.
pub fn triangular(anel: &[P2]) -> Vec<[usize; 3]> {
    let n = anel.len();
    if n < 3 {
        return Vec::new();
    }
    if n == 3 {
        return vec![if area_assinada(anel) >= 0.0 {
            [0, 1, 2]
        } else {
            [0, 2, 1]
        }];
    }

    // Trabalha sempre anti-horario; inverte a lista de indices se preciso.
    let horario = area_assinada(anel) < 0.0;
    let mut restantes: Vec<usize> = if horario {
        (0..n).rev().collect()
    } else {
        (0..n).collect()
    };

    let mut saida = Vec::with_capacity(n - 2);
    // Cada orelha cortada remove um vertice; sem progresso em uma volta inteira
    // o poligono e degenerado (auto-intersecao, pontos colineares) e a saida
    // parcial e melhor que travar.
    let mut sem_progresso = 0;

    while restantes.len() > 3 {
        if sem_progresso > restantes.len() {
            break;
        }
        let m = restantes.len();
        let mut cortou = false;

        for i in 0..m {
            let ia = restantes[(i + m - 1) % m];
            let ib = restantes[i];
            let ic = restantes[(i + 1) % m];
            let (a, b, c) = (anel[ia], anel[ib], anel[ic]);

            // Vertice reflexo nao e orelha.
            if cruz(a, b, c) <= 0.0 {
                continue;
            }
            // Nenhum outro vertice pode estar dentro da orelha.
            let livre = restantes
                .iter()
                .filter(|k| **k != ia && **k != ib && **k != ic)
                .all(|k| !dentro(a, b, c, anel[*k]));
            if !livre {
                continue;
            }

            saida.push([ia, ib, ic]);
            restantes.remove(i);
            cortou = true;
            break;
        }

        if cortou {
            sem_progresso = 0;
        } else {
            // Descarta o vertice mais "achatado" e segue; e o que sobra a fazer
            // num anel degenerado.
            sem_progresso += 1;
            if restantes.len() > 3 {
                restantes.remove(0);
            }
        }
    }

    if restantes.len() == 3 {
        saida.push([restantes[0], restantes[1], restantes[2]]);
    }
    saida
}

/// Centroide da area (nao a media dos vertices — que puxa para onde ha mais
/// pontos e desloca o resultado em plantas com vertices concentrados).
///
/// **Atencao:** o centroide de um poligono concavo pode cair *fora* dele. Num
/// "U" ele cai no vao. Para ancorar rotulo ou pino use [`ponto_interno`], que
/// garante estar dentro.
pub fn centroide(p: &[P2]) -> P2 {
    let a = area_assinada(p);
    if a.abs() < 1e-9 {
        let n = p.len().max(1) as f64;
        return [
            p.iter().map(|q| q[0]).sum::<f64>() / n,
            p.iter().map(|q| q[1]).sum::<f64>() / n,
        ];
    }
    let n = p.len();
    let (mut cx, mut cy) = (0.0, 0.0);
    for i in 0..n {
        let j = (i + 1) % n;
        let f = p[i][0] * p[j][1] - p[j][0] * p[i][1];
        cx += (p[i][0] + p[j][0]) * f;
        cy += (p[i][1] + p[j][1]) * f;
    }
    [cx / (6.0 * a), cy / (6.0 * a)]
}

/// Um ponto **garantidamente dentro** do poligono, para ancorar rotulo e pino.
///
/// Usa o baricentro do maior triangulo da triangulacao: como o triangulo e um
/// pedaco do proprio poligono, seu baricentro nunca escapa. Escolher o *maior*
/// coloca a ancora na parte mais folgada da planta, e nao num apendice estreito.
pub fn ponto_interno(anel: &[P2]) -> Option<P2> {
    if anel.len() < 3 {
        return None;
    }
    let tris = triangular(anel);
    let melhor = tris.iter().max_by(|a, b| {
        let ar = |t: &[usize; 3]| area(&[anel[t[0]], anel[t[1]], anel[t[2]]]);
        ar(a)
            .partial_cmp(&ar(b))
            .unwrap_or(std::cmp::Ordering::Equal)
    })?;
    let (a, b, c) = (anel[melhor[0]], anel[melhor[1]], anel[melhor[2]]);
    Some([(a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0])
}

/// Um ponto esta dentro do poligono? Ray casting horizontal.
pub fn contem(anel: &[P2], p: P2) -> bool {
    let n = anel.len();
    if n < 3 {
        return false;
    }
    let mut dentro = false;
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (anel[i], anel[j]);
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let t = (p[1] - a[1]) / (b[1] - a[1]);
            if p[0] < a[0] + t * (b[0] - a[0]) {
                dentro = !dentro;
            }
        }
        j = i;
    }
    dentro
}

/// Caixa envolvente alinhada aos eixos: `[min_x, min_y, max_x, max_y]`.
pub fn envolvente(anel: &[P2]) -> [f64; 4] {
    let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for p in anel {
        b[0] = b[0].min(p[0]);
        b[1] = b[1].min(p[1]);
        b[2] = b[2].max(p[0]);
        b[3] = b[3].max(p[1]);
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Soma das areas dos triangulos, para conferir contra a area do poligono.
    fn area_dos_triangulos(anel: &[P2], tris: &[[usize; 3]]) -> f64 {
        tris.iter()
            .map(|t| area(&[anel[t[0]], anel[t[1]], anel[t[2]]]))
            .sum()
    }

    fn quadrado() -> Vec<P2> {
        vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]
    }

    /// "L" — o caso que quebra o leque de triangulos.
    fn ele() -> Vec<P2> {
        vec![
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 4.0],
            [4.0, 4.0],
            [4.0, 10.0],
            [0.0, 10.0],
        ]
    }

    #[test]
    fn a_area_do_shoelace_bate_com_a_conta_manual() {
        assert_eq!(area(&quadrado()), 100.0);
        // "L": 10x4 + 4x6 = 64
        assert_eq!(area(&ele()), 64.0);
    }

    #[test]
    fn o_quadrado_vira_dois_triangulos_cobrindo_a_area() {
        let q = quadrado();
        let t = triangular(&q);
        assert_eq!(t.len(), 2);
        assert!((area_dos_triangulos(&q, &t) - 100.0).abs() < 1e-9);
    }

    #[test]
    fn o_l_concavo_nao_gera_triangulo_fora_do_poligono() {
        // Este e o teste central do modulo. Um leque a partir do vertice 0
        // cobriria 80 m² num poligono de 64 m², transbordando para a rua.
        let l = ele();
        let t = triangular(&l);
        assert_eq!(t.len(), l.len() - 2, "contagem de triangulos errada");
        let soma = area_dos_triangulos(&l, &t);
        assert!(
            (soma - 64.0).abs() < 1e-9,
            "triangulos cobrem {soma} m² para um poligono de 64 m²"
        );
    }

    #[test]
    fn a_saida_e_sempre_anti_horaria() {
        // Sem isso o telhado some no backface culling em metade dos predios,
        // porque o OSM nao padroniza a orientacao dos aneis.
        for anel in [quadrado(), quadrado().into_iter().rev().collect(), ele()] {
            for t in triangular(&anel) {
                let a = area_assinada(&[anel[t[0]], anel[t[1]], anel[t[2]]]);
                assert!(a > 0.0, "triangulo horario: {a}");
            }
        }
    }

    #[test]
    fn um_poligono_de_muitos_lados_fecha_a_area() {
        // Circulo de 32 lados, raio 5: area ~ pi*25 com erro de discretizacao.
        let anel: Vec<P2> = (0..32)
            .map(|i| {
                let a = i as f64 / 32.0 * std::f64::consts::TAU;
                [5.0 * a.cos(), 5.0 * a.sin()]
            })
            .collect();
        let t = triangular(&anel);
        assert_eq!(t.len(), 30);
        let esperado = area(&anel);
        assert!((area_dos_triangulos(&anel, &t) - esperado).abs() < 1e-9);
    }

    #[test]
    fn anel_degenerado_nao_trava_nem_entra_em_panico() {
        // Pontos colineares e repetidos existem no OSM. O laco precisa terminar.
        let colinear: Vec<P2> = vec![[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [3.0, 0.0]];
        let _ = triangular(&colinear);

        let repetido: Vec<P2> = vec![[0.0, 0.0], [0.0, 0.0], [5.0, 0.0], [5.0, 5.0]];
        let t = triangular(&repetido);
        assert!(t.len() <= 2);

        assert!(triangular(&[]).is_empty());
        assert!(triangular(&[[0.0, 0.0], [1.0, 1.0]]).is_empty());
    }

    /// "U" — o poligono onde o centroide de area cai no vazio.
    fn u() -> Vec<P2> {
        vec![
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 10.0],
            [8.0, 10.0],
            [8.0, 3.0],
            [2.0, 3.0],
            [2.0, 10.0],
            [0.0, 10.0],
        ]
    }

    #[test]
    fn o_centroide_e_a_media_ponderada_pela_area() {
        // Conferencia contra a decomposicao manual do "L": retangulo 10x4 em
        // (5,2) mais retangulo 4x6 em (2,7), area total 64.
        let c = centroide(&ele());
        assert!(
            (c[0] - 3.875).abs() < 1e-9 && (c[1] - 3.875).abs() < 1e-9,
            "{c:?}"
        );
    }

    #[test]
    fn o_centroide_de_um_concavo_pode_cair_fora_dele() {
        // Propriedade matematica, nao defeito: no "U" o centroide fica no vao.
        // Documentado aqui para que ninguem o use como ancora de rotulo.
        let u = u();
        let c = centroide(&u);
        assert!(!contem(&u, c), "o U deixou de ser um contraexemplo: {c:?}");
    }

    #[test]
    fn o_ponto_interno_fica_dentro_ate_nos_concavos() {
        for anel in [quadrado(), ele(), u()] {
            let p = ponto_interno(&anel).expect("sem ponto interno");
            assert!(contem(&anel, p), "ponto {p:?} caiu fora");
        }
        assert!(ponto_interno(&[[0.0, 0.0], [1.0, 1.0]]).is_none());
    }

    #[test]
    fn contem_acerta_dentro_e_fora() {
        let l = ele();
        assert!(contem(&l, [1.0, 1.0]));
        assert!(contem(&l, [9.0, 1.0]));
        // Recorte do "L": ha vazio em (9, 9).
        assert!(!contem(&l, [9.0, 9.0]));
        assert!(!contem(&l, [-1.0, 5.0]));
        assert!(!contem(&l, [100.0, 100.0]));
    }

    #[test]
    fn a_envolvente_cobre_todos_os_pontos() {
        let b = envolvente(&ele());
        assert_eq!(b, [0.0, 0.0, 10.0, 10.0]);
    }
}

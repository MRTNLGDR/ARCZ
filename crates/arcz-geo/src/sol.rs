//! Posicao do Sol a partir de data, hora UTC e coordenada geografica.
//!
//! Implementa o algoritmo do NOAA Solar Calculator. E o mesmo que o Google Earth usa
//! para o controle de horario, e a precisao (melhor que 0,1° para datas entre 1900 e
//! 2100) e muito acima do necessario para sombra arquitetonica.
//!
//! Serve a dois propositos no ARCZ: iluminar a cena de forma coerente com o local e
//! a hora, e habilitar **estudo de insolacao** — saber quanta sombra um predio
//! projeta no vizinho as 9h de junho e argumento de venda, nao enfeite.

use std::f64::consts::PI;

/// Onde o Sol esta no ceu, visto de um ponto da Terra.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PosicaoSolar {
    /// Angulo acima do horizonte, em graus. Negativo = abaixo (noite).
    pub elevacao_deg: f64,
    /// Azimute em graus, horario a partir do norte (0 = N, 90 = L, 180 = S, 270 = O).
    pub azimute_deg: f64,
    /// Declinacao solar do dia, em graus. Varia entre ±23,44° ao longo do ano.
    pub declinacao_deg: f64,
    /// Equacao do tempo, em minutos. Diferenca entre o meio-dia solar e o do relogio.
    pub equacao_do_tempo_min: f64,
}

impl PosicaoSolar {
    /// `true` quando o Sol esta acima do horizonte.
    pub fn dia(&self) -> bool {
        self.elevacao_deg > 0.0
    }

    /// Direcao **para** o Sol, em coordenadas de render (x=leste, y=cima, z=-norte).
    ///
    /// E exatamente o que o shader espera em `Globais.luz.xyz`.
    pub fn direcao_render(&self) -> [f32; 3] {
        let el = self.elevacao_deg.to_radians();
        let az = self.azimute_deg.to_radians();
        let horizontal = el.cos();
        [
            (horizontal * az.sin()) as f32,  // leste
            el.sin() as f32,                 // cima
            (-horizontal * az.cos()) as f32, // -norte
        ]
    }
}

/// Instante em UTC. Campos crus para o crate nao depender de uma lib de data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InstanteUtc {
    pub ano: i32,
    pub mes: u32,
    pub dia: u32,
    pub hora: f64,
}

impl InstanteUtc {
    pub fn new(ano: i32, mes: u32, dia: u32, hora: f64) -> Self {
        Self {
            ano,
            mes,
            dia,
            hora,
        }
    }

    /// Dia juliano no instante dado.
    pub fn dia_juliano(&self) -> f64 {
        let (mut a, mut m) = (self.ano, self.mes as i32);
        // Janeiro e fevereiro contam como meses 13 e 14 do ano anterior.
        if m <= 2 {
            a -= 1;
            m += 12;
        }
        let a_sec = (a as f64 / 100.0).floor();
        // Correcao gregoriana.
        let b = 2.0 - a_sec + (a_sec / 4.0).floor();

        (365.25 * (a as f64 + 4716.0)).floor()
            + (30.6001 * (m as f64 + 1.0)).floor()
            + self.dia as f64
            + b
            - 1524.5
            + self.hora / 24.0
    }
}

/// Calcula a posicao do Sol.
pub fn posicao(instante: InstanteUtc, lat_deg: f64, lon_deg: f64) -> PosicaoSolar {
    // Seculos julianos desde J2000.0.
    let t = (instante.dia_juliano() - 2_451_545.0) / 36_525.0;

    // Longitude media geometrica do Sol, em graus.
    let l0 = (280.466_46 + t * (36_000.769_83 + t * 0.000_303_2)).rem_euclid(360.0);
    // Anomalia media.
    let m = 357.529_11 + t * (35_999.050_29 - 0.000_153_7 * t);
    let m_rad = m.to_radians();

    // Equacao do centro.
    let c = (1.914_602 - t * (0.004_817 + 0.000_014 * t)) * m_rad.sin()
        + (0.019_993 - 0.000_101 * t) * (2.0 * m_rad).sin()
        + 0.000_289 * (3.0 * m_rad).sin();

    let longitude_verdadeira = l0 + c;
    // Longitude aparente, ja com a nutacao principal.
    let omega = 125.04 - 1934.136 * t;
    let lambda = longitude_verdadeira - 0.005_69 - 0.004_78 * omega.to_radians().sin();

    // Obliquidade da ecliptica, corrigida.
    let e0 = 23.0 + (26.0 + (21.448 - t * (46.815 + t * (0.000_59 - t * 0.001_813))) / 60.0) / 60.0;
    let epsilon = e0 + 0.002_56 * omega.to_radians().cos();

    // Declinacao solar.
    let declinacao = (epsilon.to_radians().sin() * lambda.to_radians().sin())
        .asin()
        .to_degrees();

    // Equacao do tempo, em minutos.
    let y = (epsilon.to_radians() / 2.0).tan().powi(2);
    let l0_rad = l0.to_radians();
    let et = 4.0
        * (y * (2.0 * l0_rad).sin() - 2.0 * 0.016_708_634 * m_rad.sin()
            + 4.0 * 0.016_708_634 * y * m_rad.sin() * (2.0 * l0_rad).cos()
            - 0.5 * y * y * (4.0 * l0_rad).sin()
            - 1.25 * 0.016_708_634_f64.powi(2) * (2.0 * m_rad).sin())
        .to_degrees();

    // Hora solar verdadeira, em minutos desde a meia-noite local.
    let hora_solar = (instante.hora * 60.0 + et + 4.0 * lon_deg).rem_euclid(1440.0);
    // Angulo horario: 0 no meio-dia solar, negativo de manha.
    let angulo_horario = hora_solar / 4.0 - 180.0;

    let lat_rad = lat_deg.to_radians();
    let dec_rad = declinacao.to_radians();
    let ah_rad = angulo_horario.to_radians();

    let cos_zenite = lat_rad.sin() * dec_rad.sin() + lat_rad.cos() * dec_rad.cos() * ah_rad.cos();
    let zenite = cos_zenite.clamp(-1.0, 1.0).acos();
    let elevacao = 90.0 - zenite.to_degrees();

    // Azimute a partir do norte, sentido horario.
    let sin_zenite = zenite.sin();
    let azimute = if sin_zenite.abs() < 1e-9 {
        // Sol no zenite exato: azimute e indefinido; 180° e a convencao.
        180.0
    } else {
        let cos_az = (lat_rad.sin() * zenite.cos() - dec_rad.sin()) / (lat_rad.cos() * sin_zenite);
        let az = cos_az.clamp(-1.0, 1.0).acos().to_degrees();
        // De manha (angulo horario negativo) o Sol esta a leste; de tarde, a oeste.
        // Trocar estes dois ramos poe a sombra no lado errado do predio o dia
        // inteiro — e o teste `o_sol_nasce_a_leste_e_se_poe_a_oeste` trava isso.
        if angulo_horario > 0.0 {
            (az + 180.0).rem_euclid(360.0)
        } else {
            (540.0 - az).rem_euclid(360.0)
        }
    };

    PosicaoSolar {
        elevacao_deg: elevacao,
        azimute_deg: azimute,
        declinacao_deg: declinacao,
        equacao_do_tempo_min: et,
    }
}

/// Converte hora local para UTC somando o fuso. Brasilia = -3.
pub fn utc_de_local(ano: i32, mes: u32, dia: u32, hora_local: f64, fuso_horas: f64) -> InstanteUtc {
    let mut h = hora_local - fuso_horas;
    let mut d = dia;
    // Ajuste simples de virada de dia. Suficiente porque a posicao solar so depende
    // do dia juliano, e um dia a mais ou a menos entra pelo campo `hora`.
    while h < 0.0 {
        h += 24.0;
        d = d.saturating_sub(1).max(1);
    }
    while h >= 24.0 {
        h -= 24.0;
        d += 1;
    }
    InstanteUtc::new(ano, mes, d, h)
}

/// Angulo entre duas direcoes, em graus. Util para testes.
pub fn angulo_entre(a: [f32; 3], b: [f32; 3]) -> f64 {
    let p = (a[0] * b[0] + a[1] * b[1] + a[2] * b[2]) as f64;
    let na = ((a[0] * a[0] + a[1] * a[1] + a[2] * a[2]) as f64).sqrt();
    let nb = ((b[0] * b[0] + b[1] * b[1] + b[2] * b[2]) as f64).sqrt();
    (p / (na * nb)).clamp(-1.0, 1.0).acos() * 180.0 / PI
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOMBINHAS_LAT: f64 = -27.154_496_7;
    const BOMBINHAS_LON: f64 = -48.502_265_3;

    #[test]
    fn dia_juliano_bate_com_epocas_conhecidas() {
        // J2000.0 = 1 jan 2000, 12:00 UTC.
        let j2000 = InstanteUtc::new(2000, 1, 1, 12.0).dia_juliano();
        assert!((j2000 - 2_451_545.0).abs() < 1e-6, "J2000 deu {j2000}");

        // Inicio do calendario gregoriano.
        let g = InstanteUtc::new(1582, 10, 15, 0.0).dia_juliano();
        assert!((g - 2_299_160.5).abs() < 1e-6, "deu {g}");

        // Um dia depois e exatamente +1.
        let a = InstanteUtc::new(2026, 7, 30, 6.0).dia_juliano();
        let b = InstanteUtc::new(2026, 7, 31, 6.0).dia_juliano();
        assert!((b - a - 1.0).abs() < 1e-9);
    }

    #[test]
    fn declinacao_atinge_os_tropicos_nos_solsticios() {
        // A declinacao solar oscila entre ±23,44° — e o que define os tropicos.
        let verao_norte = posicao(InstanteUtc::new(2026, 6, 21, 12.0), 0.0, 0.0);
        assert!(
            (verao_norte.declinacao_deg - 23.44).abs() < 0.1,
            "solsticio de junho: {}",
            verao_norte.declinacao_deg
        );

        let verao_sul = posicao(InstanteUtc::new(2026, 12, 21, 12.0), 0.0, 0.0);
        assert!(
            (verao_sul.declinacao_deg + 23.44).abs() < 0.1,
            "solsticio de dezembro: {}",
            verao_sul.declinacao_deg
        );
    }

    #[test]
    fn declinacao_e_quase_zero_nos_equinocios() {
        for (mes, dia) in [(3, 20), (9, 22)] {
            let p = posicao(InstanteUtc::new(2026, mes, dia, 12.0), 0.0, 0.0);
            assert!(
                p.declinacao_deg.abs() < 1.0,
                "equinocio {mes}/{dia}: declinacao {}",
                p.declinacao_deg
            );
        }
    }

    #[test]
    fn no_equador_ao_meio_dia_do_equinocio_o_sol_fica_quase_a_pino() {
        // Longitude 0, meio-dia UTC, equinocio: o Sol passa quase pelo zenite.
        let p = posicao(InstanteUtc::new(2026, 3, 20, 12.0), 0.0, 0.0);
        assert!(
            p.elevacao_deg > 88.0,
            "elevacao no equinocio ao meio-dia: {}",
            p.elevacao_deg
        );
    }

    #[test]
    fn o_sol_nasce_a_leste_e_se_poe_a_oeste() {
        // Bombinhas, equinocio. De manha o azimute fica no quadrante leste; de
        // tarde, no oeste. Inverter isto poria a sombra do lado errado do predio.
        let manha = posicao(
            utc_de_local(2026, 3, 20, 8.0, -3.0),
            BOMBINHAS_LAT,
            BOMBINHAS_LON,
        );
        let tarde = posicao(
            utc_de_local(2026, 3, 20, 16.0, -3.0),
            BOMBINHAS_LAT,
            BOMBINHAS_LON,
        );

        assert!(manha.dia() && tarde.dia());
        assert!(
            (45.0..135.0).contains(&manha.azimute_deg),
            "as 8h o azimute deveria estar a leste, deu {}",
            manha.azimute_deg
        );
        assert!(
            (225.0..315.0).contains(&tarde.azimute_deg),
            "as 16h o azimute deveria estar a oeste, deu {}",
            tarde.azimute_deg
        );
    }

    #[test]
    fn no_hemisferio_sul_o_sol_do_meio_dia_fica_ao_norte() {
        // Diferenca classica entre hemisferios: em Bombinhas o Sol do meio-dia
        // esta ao NORTE. Se o azimute der ~180°, o modelo esta espelhado.
        let p = posicao(
            utc_de_local(2026, 6, 21, 12.0, -3.0),
            BOMBINHAS_LAT,
            BOMBINHAS_LON,
        );
        assert!(
            p.azimute_deg < 45.0 || p.azimute_deg > 315.0,
            "meio-dia de inverno no sul: azimute deveria ser ~0° (norte), deu {}",
            p.azimute_deg
        );
    }

    #[test]
    fn elevacao_maxima_do_dia_bate_com_a_formula_da_latitude() {
        // No meio-dia SOLAR: elevacao = 90 - |latitude - declinacao|.
        // Confere o calculo inteiro contra uma identidade independente.
        //
        // Procura o pico varrendo o dia, em vez de assumir 12h do relogio: as 12h
        // nao sao o meio-dia solar (a longitude dentro do fuso e a equacao do tempo
        // deslocam ate ~30 min, o que muda a elevacao em graus).
        for (mes, dia) in [(1, 15), (4, 10), (6, 21), (9, 5), (12, 21)] {
            let mut pico = f64::NEG_INFINITY;
            let mut dec_no_pico = 0.0;
            for i in 0..24 * 60 {
                let p = posicao(
                    utc_de_local(2026, mes, dia, i as f64 / 60.0, -3.0),
                    BOMBINHAS_LAT,
                    BOMBINHAS_LON,
                );
                if p.elevacao_deg > pico {
                    pico = p.elevacao_deg;
                    dec_no_pico = p.declinacao_deg;
                }
            }
            let esperado = 90.0 - (BOMBINHAS_LAT - dec_no_pico).abs();
            assert!(
                (pico - esperado).abs() < 0.2,
                "{mes}/{dia}: pico {pico} vs esperado {esperado}"
            );
        }
    }

    #[test]
    fn de_madrugada_o_sol_esta_abaixo_do_horizonte() {
        let p = posicao(
            utc_de_local(2026, 7, 30, 2.0, -3.0),
            BOMBINHAS_LAT,
            BOMBINHAS_LON,
        );
        assert!(!p.dia(), "as 2h da manha a elevacao deu {}", p.elevacao_deg);
        assert!(p.elevacao_deg < -10.0);
    }

    #[test]
    fn o_dia_de_verao_e_mais_longo_que_o_de_inverno() {
        // Conta as horas com Sol acima do horizonte em Bombinhas.
        let horas_com_sol = |mes, dia| {
            (0..24 * 4)
                .filter(|i| {
                    let h = *i as f64 / 4.0;
                    posicao(
                        utc_de_local(2026, mes, dia, h, -3.0),
                        BOMBINHAS_LAT,
                        BOMBINHAS_LON,
                    )
                    .dia()
                })
                .count() as f64
                / 4.0
        };

        let verao = horas_com_sol(12, 21);
        let inverno = horas_com_sol(6, 21);
        assert!(
            verao > inverno + 1.5,
            "verao {verao} h nao e maior que inverno {inverno} h"
        );
        // Sanidade: nesta latitude o dia fica entre 10 e 14 horas.
        assert!((10.0..14.5).contains(&verao), "verao {verao} h");
        assert!((9.0..12.0).contains(&inverno), "inverno {inverno} h");
    }

    #[test]
    fn a_direcao_de_render_e_unitaria_e_coerente_com_a_elevacao() {
        for h in [7.0, 10.0, 13.0, 17.0] {
            let p = posicao(
                utc_de_local(2026, 7, 30, h, -3.0),
                BOMBINHAS_LAT,
                BOMBINHAS_LON,
            );
            let d = p.direcao_render();
            let n = ((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]) as f64).sqrt();
            assert!((n - 1.0).abs() < 1e-5, "as {h}h: norma {n}");
            // A componente Y e o seno da elevacao, por construcao.
            assert!(
                ((d[1] as f64) - p.elevacao_deg.to_radians().sin()).abs() < 1e-6,
                "as {h}h: y = {} nao bate com a elevacao {}",
                d[1],
                p.elevacao_deg
            );
        }
    }

    #[test]
    fn sol_ao_norte_aponta_para_menos_z_no_render() {
        // Azimute 0 (norte) tem que virar -Z; azimute 90 (leste) tem que virar +X.
        let norte = PosicaoSolar {
            elevacao_deg: 0.0,
            azimute_deg: 0.0,
            declinacao_deg: 0.0,
            equacao_do_tempo_min: 0.0,
        };
        assert!(angulo_entre(norte.direcao_render(), [0.0, 0.0, -1.0]) < 0.01);

        let leste = PosicaoSolar {
            azimute_deg: 90.0,
            ..norte
        };
        assert!(angulo_entre(leste.direcao_render(), [1.0, 0.0, 0.0]) < 0.01);

        let zenite = PosicaoSolar {
            elevacao_deg: 90.0,
            ..norte
        };
        assert!(angulo_entre(zenite.direcao_render(), [0.0, 1.0, 0.0]) < 0.01);
    }

    #[test]
    fn equacao_do_tempo_fica_na_faixa_conhecida() {
        // Ao longo do ano ela varia entre cerca de -14 e +16 minutos.
        let mut menor = f64::INFINITY;
        let mut maior = f64::NEG_INFINITY;
        for mes in 1..=12 {
            for dia in [1, 15] {
                let et =
                    posicao(InstanteUtc::new(2026, mes, dia, 12.0), 0.0, 0.0).equacao_do_tempo_min;
                menor = menor.min(et);
                maior = maior.max(et);
            }
        }
        assert!((-16.0..-10.0).contains(&menor), "minimo da EoT: {menor}");
        assert!((13.0..18.0).contains(&maior), "maximo da EoT: {maior}");
    }

    #[test]
    fn fuso_horario_desloca_o_instante_corretamente() {
        let utc = utc_de_local(2026, 7, 30, 15.0, -3.0);
        assert_eq!((utc.dia, utc.hora), (30, 18.0));

        // Virada de dia para tras.
        let madrugada = utc_de_local(2026, 7, 30, 1.0, -3.0);
        assert_eq!(madrugada.hora, 4.0);

        // Virada para frente.
        let noite = utc_de_local(2026, 7, 30, 22.0, -3.0);
        assert_eq!((noite.dia, noite.hora), (31, 1.0));
    }
}

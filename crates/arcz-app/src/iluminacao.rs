//! Traducao entre "data e hora no local" e os uniforms de iluminacao da GPU.

use arcz_geo::sol::{self, PosicaoSolar};

/// Momento do dia a simular, em hora local.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Momento {
    pub ano: i32,
    pub mes: u32,
    pub dia: u32,
    pub hora_local: f64,
    /// Fuso em horas. Brasilia = -3.
    pub fuso: f64,
}

impl Default for Momento {
    fn default() -> Self {
        // Meio da tarde de um dia de verao: sol alto o bastante para iluminar bem as
        // fachadas e baixo o bastante para haver sombra legivel.
        Self {
            ano: 2026,
            mes: 3,
            dia: 21,
            hora_local: 15.0,
            fuso: -3.0,
        }
    }
}

impl Momento {
    pub fn posicao_solar(&self, lat_deg: f64, lon_deg: f64) -> PosicaoSolar {
        sol::posicao(
            sol::utc_de_local(self.ano, self.mes, self.dia, self.hora_local, self.fuso),
            lat_deg,
            lon_deg,
        )
    }

    /// Uniform de luz: `xyz` aponta para o Sol, `w` e a fracao ambiente.
    ///
    /// A ambiente sobe quando o Sol desce: com o Sol no horizonte quase toda a luz
    /// que chega e indireta (ceu), e usar um valor fixo deixaria as fachadas em
    /// sombra totalmente pretas ao entardecer.
    pub fn uniform_luz(&self, lat_deg: f64, lon_deg: f64) -> ([f32; 4], PosicaoSolar) {
        let p = self.posicao_solar(lat_deg, lon_deg);
        let d = p.direcao_render();

        let ambiente = if p.elevacao_deg > 25.0 {
            0.30
        } else if p.elevacao_deg > 0.0 {
            // Sobe de 0.30 ate 0.55 conforme o Sol se aproxima do horizonte.
            0.30 + (25.0 - p.elevacao_deg) / 25.0 * 0.25
        } else {
            // Noite: so a luz do ceu, e bem fraca.
            (0.55 + p.elevacao_deg / 18.0 * 0.45).clamp(0.10, 0.55)
        };

        ([d[0], d[1], d[2], ambiente as f32], p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAT: f64 = -27.154_496_7;
    const LON: f64 = -48.502_265_3;

    #[test]
    fn o_padrao_cai_de_dia() {
        let (luz, p) = Momento::default().uniform_luz(LAT, LON);
        assert!(p.dia(), "o momento padrao deveria ter sol: {p:?}");
        assert!(luz[1] > 0.0, "a luz deveria vir de cima: {luz:?}");
    }

    #[test]
    fn a_luz_acompanha_o_sol_ao_longo_do_dia() {
        // De manha a luz vem do leste (+x); de tarde, do oeste (-x).
        let manha = Momento {
            hora_local: 8.0,
            ..Default::default()
        }
        .uniform_luz(LAT, LON)
        .0;
        let tarde = Momento {
            hora_local: 17.0,
            ..Default::default()
        }
        .uniform_luz(LAT, LON)
        .0;

        assert!(
            manha[0] > 0.3,
            "as 8h a luz deveria vir do leste: {manha:?}"
        );
        assert!(tarde[0] < -0.3, "as 17h deveria vir do oeste: {tarde:?}");
    }

    #[test]
    fn a_ambiente_sobe_quando_o_sol_desce() {
        let meio_dia = Momento {
            hora_local: 12.0,
            ..Default::default()
        }
        .uniform_luz(LAT, LON)
        .0[3];
        let entardecer = Momento {
            hora_local: 18.0,
            ..Default::default()
        }
        .uniform_luz(LAT, LON)
        .0[3];

        assert!(
            entardecer > meio_dia,
            "ambiente no entardecer ({entardecer}) deveria superar a do meio-dia ({meio_dia})"
        );
    }

    #[test]
    fn a_ambiente_fica_sempre_numa_faixa_util() {
        // Nem 0 (sombra preta) nem 1 (cena chapada), em nenhuma hora do ano.
        for mes in [1, 6, 12] {
            for i in 0..48 {
                let m = Momento {
                    mes,
                    hora_local: i as f64 / 2.0,
                    ..Default::default()
                };
                let a = m.uniform_luz(LAT, LON).0[3];
                assert!(
                    (0.09..=0.56).contains(&a),
                    "mes {mes} as {}h: ambiente {a}",
                    m.hora_local
                );
            }
        }
    }

    #[test]
    fn a_direcao_da_luz_e_sempre_unitaria() {
        for i in 0..24 {
            let luz = Momento {
                hora_local: i as f64,
                ..Default::default()
            }
            .uniform_luz(LAT, LON)
            .0;
            let n = (luz[0] * luz[0] + luz[1] * luz[1] + luz[2] * luz[2]).sqrt();
            assert!((n - 1.0).abs() < 1e-5, "as {i}h: norma {n}");
        }
    }

    #[test]
    fn de_madrugada_o_sol_esta_abaixo_do_horizonte() {
        let (luz, p) = Momento {
            hora_local: 3.0,
            ..Default::default()
        }
        .uniform_luz(LAT, LON);
        assert!(!p.dia());
        assert!(luz[1] < 0.0, "a luz deveria vir de baixo: {luz:?}");
    }
}

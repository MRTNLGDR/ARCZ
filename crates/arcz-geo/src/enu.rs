//! Quadro local ENU (East / North / Up) — a peca central da estrategia anti-jitter.
//!
//! Toda a cena renderizavel do ARCZ vive num [`EnuFrame`] ancorado num ponto de
//! referencia do projeto. Coordenadas ECEF (~6.4e6 m) entram em `f64`, saem em ENU
//! (tipicamente < 2e4 m) e so entao viram `f32` para a GPU.

use crate::wgs84::{Ecef, Geodetic};

/// Ponto no quadro local, em metros: `e` para leste, `n` para norte, `u` para cima.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Enu {
    pub e: f64,
    pub n: f64,
    pub u: f64,
}

impl Enu {
    pub const fn new(e: f64, n: f64, u: f64) -> Self {
        Self { e, n, u }
    }

    /// Trunca para `f32` na ordem que os shaders do ARCZ esperam (x=leste, y=cima, z=-norte).
    ///
    /// Esta e a **unica** conversao para `f32` autorizada no pipeline geometrico.
    /// Y-up destro, com -Z apontando para o norte, e a convencao usada em `arcz-app`.
    pub fn to_render_f32(self) -> [f32; 3] {
        [self.e as f32, self.u as f32, -self.n as f32]
    }

    pub fn length(self) -> f64 {
        (self.e * self.e + self.n * self.n + self.u * self.u).sqrt()
    }
}

/// Quadro ENU ancorado numa origem geodetica.
///
/// A matriz de rotacao e guardada explicitamente porque ela e reusada milhoes de vezes
/// na construcao das malhas de terreno — recalcular seno/cosseno por vertice custa caro.
#[derive(Debug, Clone, Copy)]
pub struct EnuFrame {
    origin_geodetic: Geodetic,
    origin_ecef: Ecef,
    /// Linhas da matriz ECEF->ENU: `[leste, norte, cima]`, cada uma um versor em ECEF.
    east: [f64; 3],
    north: [f64; 3],
    up: [f64; 3],
}

impl EnuFrame {
    /// Cria um quadro ancorado em `origin`.
    pub fn new(origin: Geodetic) -> Self {
        let lat = origin.lat_deg.to_radians();
        let lon = origin.lon_deg.to_radians();
        let (sin_lat, cos_lat) = lat.sin_cos();
        let (sin_lon, cos_lon) = lon.sin_cos();

        Self {
            origin_geodetic: origin,
            origin_ecef: origin.to_ecef(),
            east: [-sin_lon, cos_lon, 0.0],
            north: [-sin_lat * cos_lon, -sin_lat * sin_lon, cos_lat],
            up: [cos_lat * cos_lon, cos_lat * sin_lon, sin_lat],
        }
    }

    pub fn origin_geodetic(&self) -> Geodetic {
        self.origin_geodetic
    }

    pub fn origin_ecef(&self) -> Ecef {
        self.origin_ecef
    }

    /// Projeta um ponto ECEF no quadro local.
    pub fn ecef_to_enu(&self, p: Ecef) -> Enu {
        let d = p - self.origin_ecef;
        let v = [d.x, d.y, d.z];
        Enu {
            e: dot(self.east, v),
            n: dot(self.north, v),
            u: dot(self.up, v),
        }
    }

    /// Volta do quadro local para ECEF (transposta da rotacao, que e ortonormal).
    pub fn enu_to_ecef(&self, p: Enu) -> Ecef {
        Ecef {
            x: self.east[0] * p.e + self.north[0] * p.n + self.up[0] * p.u,
            y: self.east[1] * p.e + self.north[1] * p.n + self.up[1] * p.u,
            z: self.east[2] * p.e + self.north[2] * p.n + self.up[2] * p.u,
        } + self.origin_ecef
    }

    /// Atalho usado em todo o carregamento de terreno.
    pub fn geodetic_to_enu(&self, p: Geodetic) -> Enu {
        self.ecef_to_enu(p.to_ecef())
    }

    pub fn enu_to_geodetic(&self, p: Enu) -> Geodetic {
        self.enu_to_ecef(p).to_geodetic()
    }

    /// Distancia horizontal da origem ate `p`, usada para decidir quando rebasear.
    pub fn horizontal_distance_to(&self, p: Geodetic) -> f64 {
        let enu = self.geodetic_to_enu(p);
        (enu.e * enu.e + enu.n * enu.n).sqrt()
    }

    /// Distancia alem da qual `f32` deixa de garantir precisao milimetrica.
    ///
    /// `f32` tem 24 bits de mantissa: em 16 km o ulp e ~1 mm. Passou disso, rebaseie.
    pub const REBASE_THRESHOLD_M: f64 = 16_000.0;

    /// `true` quando a camera se afastou o bastante para o quadro precisar ser reancorado.
    pub fn needs_rebase(&self, camera: Geodetic) -> bool {
        self.horizontal_distance_to(camera) > Self::REBASE_THRESHOLD_M
    }
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAO_PAULO: Geodetic = Geodetic::new(-46.633_308, -23.550_520, 760.0);

    #[test]
    fn origem_do_quadro_e_o_zero_do_quadro() {
        let f = EnuFrame::new(SAO_PAULO);
        let o = f.geodetic_to_enu(SAO_PAULO);
        assert!(o.length() < 1e-6, "origem deu {o:?}");
    }

    #[test]
    fn eixos_sao_ortonormais() {
        let f = EnuFrame::new(SAO_PAULO);
        for v in [f.east, f.north, f.up] {
            assert!(
                (dot(v, v) - 1.0).abs() < 1e-12,
                "versor nao unitario: {v:?}"
            );
        }
        assert!(dot(f.east, f.north).abs() < 1e-12);
        assert!(dot(f.east, f.up).abs() < 1e-12);
        assert!(dot(f.north, f.up).abs() < 1e-12);
    }

    #[test]
    fn subida_pura_vira_up_puro() {
        let f = EnuFrame::new(SAO_PAULO);
        let acima = Geodetic::new(
            SAO_PAULO.lon_deg,
            SAO_PAULO.lat_deg,
            SAO_PAULO.alt_m + 100.0,
        );
        let enu = f.geodetic_to_enu(acima);
        assert!(enu.e.abs() < 1e-6, "e = {}", enu.e);
        assert!(enu.n.abs() < 1e-6, "n = {}", enu.n);
        assert!((enu.u - 100.0).abs() < 1e-6, "u = {}", enu.u);
    }

    #[test]
    fn curvatura_da_terra_aparece_como_queda_do_plano_tangente() {
        // Um ponto a 1 km ao norte, na mesma altitude elipsoidal, fica ~7.8 cm ABAIXO
        // do plano tangente local. queda ~= d^2 / (2R). Se este teste falhar, o quadro
        // esta plano demais — sinal de que alguem trocou o elipsoide por uma esfera
        // ou linearizou a projecao.
        let f = EnuFrame::new(SAO_PAULO);
        let d_metros = 1_000.0;
        let d_graus = d_metros / 111_132.0; // ~1 grau de latitude em metros
        let ao_norte = Geodetic::new(
            SAO_PAULO.lon_deg,
            SAO_PAULO.lat_deg + d_graus,
            SAO_PAULO.alt_m,
        );

        let enu = f.geodetic_to_enu(ao_norte);
        let queda_esperada = d_metros * d_metros / (2.0 * 6_371_000.0);

        assert!(enu.e.abs() < 1.0, "e deveria ser ~0, deu {}", enu.e);
        assert!(
            (enu.n - d_metros).abs() < 5.0,
            "n deveria ser ~1000, deu {}",
            enu.n
        );
        assert!(
            (enu.u.abs() - queda_esperada).abs() < 0.02,
            "queda deu {} m, esperado ~{} m",
            enu.u,
            -queda_esperada
        );
        assert!(enu.u < 0.0, "a queda tem que ser negativa, deu {}", enu.u);
    }

    #[test]
    fn roundtrip_enu_ecef_em_toda_a_area_util() {
        let f = EnuFrame::new(SAO_PAULO);
        let mut pior = 0.0_f64;
        let mut e = -20_000.0;
        while e <= 20_000.0 {
            let mut n = -20_000.0;
            while n <= 20_000.0 {
                let p = Enu::new(e, n, 350.0);
                let back = f.ecef_to_enu(f.enu_to_ecef(p));
                pior = pior
                    .max((back.e - p.e).abs())
                    .max((back.n - p.n).abs())
                    .max((back.u - p.u).abs());
                n += 2_500.0;
            }
            e += 2_500.0;
        }
        assert!(pior < 1e-6, "pior erro do roundtrip ENU: {pior} m");
    }

    #[test]
    fn rebase_dispara_so_depois_do_limite() {
        let f = EnuFrame::new(SAO_PAULO);
        let perto = f.enu_to_geodetic(Enu::new(5_000.0, 5_000.0, 0.0));
        let longe = f.enu_to_geodetic(Enu::new(30_000.0, 0.0, 0.0));
        assert!(!f.needs_rebase(perto));
        assert!(f.needs_rebase(longe));
    }

    /// Este e o teste que justifica a arquitetura inteira do crate.
    ///
    /// Ele **mede** a perda de precisao de jogar ECEF direto num `f32` (o erro classico
    /// que faz a geometria tremer) e prova que o quadro ENU elimina o problema.
    #[test]
    fn f32_em_ecef_perde_precisao_metrica() {
        let ecef = SAO_PAULO.to_ecef();
        let erro_ecef = [ecef.x, ecef.y, ecef.z]
            .iter()
            .map(|&v| (v as f32 as f64 - v).abs())
            .fold(0.0_f64, f64::max);

        // Ponto a 15 km da origem: o pior caso dentro do limite de rebase.
        let enu = Enu::new(15_000.0, -12_000.0, 812.0);
        let erro_enu = [enu.e, enu.n, enu.u]
            .iter()
            .map(|&v| (v as f32 as f64 - v).abs())
            .fold(0.0_f64, f64::max);

        // ECEF em f32 erra na casa dos decimetros — visivel como jitter na tela.
        assert!(
            erro_ecef > 0.1,
            "esperava perda >0.1 m em ECEF/f32, deu {erro_ecef} m"
        );
        // ENU em f32 erra menos que 2 mm.
        assert!(
            erro_enu < 0.002,
            "esperava perda <2 mm em ENU/f32, deu {erro_enu} m"
        );
        assert!(
            erro_ecef / erro_enu > 100.0,
            "ENU deveria ser ao menos 100x melhor; ecef={erro_ecef} enu={erro_enu}"
        );
    }
}

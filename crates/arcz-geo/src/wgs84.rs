//! Elipsoide WGS84 e conversao geodetica <-> ECEF (Earth-Centered Earth-Fixed).

/// Semi-eixo maior do WGS84, em metros.
pub const A: f64 = 6_378_137.0;
/// Achatamento do WGS84.
pub const F: f64 = 1.0 / 298.257_223_563;
/// Semi-eixo menor do WGS84, em metros.
pub const B: f64 = A * (1.0 - F);
/// Primeira excentricidade ao quadrado.
pub const E2: f64 = F * (2.0 - F);
/// Segunda excentricidade ao quadrado.
pub const EP2: f64 = (A * A - B * B) / (B * B);

/// Coordenada geodetica: longitude/latitude em graus, altitude elipsoidal em metros.
///
/// Altitude e **elipsoidal** (altura acima do elipsoide WGS84), nao ortometrica.
/// DEMs como o Copernicus GLO-30 sao ortometricos (referidos ao geoide EGM2008); a
/// diferenca no Brasil chega a ~-6 m. A correcao de geoide entra na Fatia 2 — ate la
/// tratamos as duas como iguais, o que e aceitavel porque o erro e uniforme na regiao
/// e nao produz deformacao relativa visivel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geodetic {
    pub lon_deg: f64,
    pub lat_deg: f64,
    pub alt_m: f64,
}

/// Coordenada cartesiana geocentrica, em metros.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ecef {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Geodetic {
    pub const fn new(lon_deg: f64, lat_deg: f64, alt_m: f64) -> Self {
        Self {
            lon_deg,
            lat_deg,
            alt_m,
        }
    }

    /// Converte para ECEF. Formula fechada, sem iteracao.
    pub fn to_ecef(self) -> Ecef {
        let lat = self.lat_deg.to_radians();
        let lon = self.lon_deg.to_radians();
        let (sin_lat, cos_lat) = lat.sin_cos();
        let (sin_lon, cos_lon) = lon.sin_cos();

        // Raio de curvatura da primeira vertical.
        let n = A / (1.0 - E2 * sin_lat * sin_lat).sqrt();

        Ecef {
            x: (n + self.alt_m) * cos_lat * cos_lon,
            y: (n + self.alt_m) * cos_lat * sin_lon,
            z: (n * (1.0 - E2) + self.alt_m) * sin_lat,
        }
    }
}

impl Ecef {
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    /// Converte para geodetica pelo metodo de Bowring.
    ///
    /// Bowring e nao-iterativo e da erro abaixo de 1 mm para altitudes terrestres
    /// (|h| < 10 km), que cobre qualquer coisa que o ARCZ vai renderizar.
    pub fn to_geodetic(self) -> Geodetic {
        let p = (self.x * self.x + self.y * self.y).sqrt();

        // Degenerescencia nos polos: p -> 0 faz atan2 e a divisao por cos(lat) explodirem.
        if p < 1e-9 {
            let sign = if self.z >= 0.0 { 1.0 } else { -1.0 };
            return Geodetic {
                lon_deg: 0.0,
                lat_deg: 90.0 * sign,
                alt_m: self.z.abs() - B,
            };
        }

        let theta = (self.z * A).atan2(p * B);
        let (sin_t, cos_t) = theta.sin_cos();

        let lat =
            (self.z + EP2 * B * sin_t * sin_t * sin_t).atan2(p - E2 * A * cos_t * cos_t * cos_t);
        let lon = self.y.atan2(self.x);

        let sin_lat = lat.sin();
        let n = A / (1.0 - E2 * sin_lat * sin_lat).sqrt();
        let alt = p / lat.cos() - n;

        Geodetic {
            lon_deg: lon.to_degrees(),
            lat_deg: lat.to_degrees(),
            alt_m: alt,
        }
    }

    pub fn length(self) -> f64 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }
}

impl std::ops::Sub for Ecef {
    type Output = Ecef;
    fn sub(self, o: Ecef) -> Ecef {
        Ecef::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl std::ops::Add for Ecef {
    type Output = Ecef;
    fn add(self, o: Ecef) -> Ecef {
        Ecef::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tolerancia de 0.1 mm: qualquer coisa pior que isso indica erro de formula,
    /// nao de ponto flutuante.
    const TOL_M: f64 = 1e-4;

    fn assert_geodetic_eq(got: Geodetic, want: Geodetic) {
        // 1e-9 grau ~= 0.11 mm no equador.
        assert!(
            (got.lon_deg - want.lon_deg).abs() < 1e-9,
            "lon: got {} want {}",
            got.lon_deg,
            want.lon_deg
        );
        assert!(
            (got.lat_deg - want.lat_deg).abs() < 1e-9,
            "lat: got {} want {}",
            got.lat_deg,
            want.lat_deg
        );
        assert!(
            (got.alt_m - want.alt_m).abs() < TOL_M,
            "alt: got {} want {}",
            got.alt_m,
            want.alt_m
        );
    }

    #[test]
    fn origem_do_datum_bate_com_o_semi_eixo_maior() {
        let e = Geodetic::new(0.0, 0.0, 0.0).to_ecef();
        assert!((e.x - A).abs() < TOL_M, "x = {}", e.x);
        assert!(e.y.abs() < TOL_M);
        assert!(e.z.abs() < TOL_M);
    }

    #[test]
    fn polo_norte_bate_com_o_semi_eixo_menor() {
        let e = Geodetic::new(0.0, 90.0, 0.0).to_ecef();
        assert!(e.x.abs() < TOL_M, "x = {}", e.x);
        assert!(e.y.abs() < TOL_M, "y = {}", e.y);
        assert!((e.z - B).abs() < TOL_M, "z = {} B = {}", e.z, B);
    }

    #[test]
    fn noventa_graus_leste_cai_no_eixo_y() {
        let e = Geodetic::new(90.0, 0.0, 0.0).to_ecef();
        assert!(e.x.abs() < TOL_M);
        assert!((e.y - A).abs() < TOL_M);
    }

    #[test]
    fn roundtrip_geodetico_ecef_em_pontos_reais() {
        // Sao Paulo, Curitiba, Greenwich, Everest, Ushuaia, Mar Morto (altitude negativa).
        let pontos = [
            Geodetic::new(-46.633_308, -23.550_520, 760.0),
            Geodetic::new(-49.271_170, -25.428_950, 934.6),
            Geodetic::new(0.0, 51.477_928, 45.0),
            Geodetic::new(86.925_026, 27.988_056, 8_848.86),
            Geodetic::new(-68.303_000, -54.801_900, 23.0),
            Geodetic::new(35.500_000, 31.500_000, -430.0),
        ];

        for p in pontos {
            let back = p.to_ecef().to_geodetic();
            assert_geodetic_eq(back, p);
        }
    }

    #[test]
    fn roundtrip_cobre_a_grade_global() {
        let mut pior = 0.0_f64;
        let mut lat = -89.0;
        while lat <= 89.0 {
            let mut lon = -180.0;
            while lon < 180.0 {
                let p = Geodetic::new(lon, lat, 500.0);
                let back = p.to_ecef().to_geodetic();
                // Erro medido em metros no ponto, nao em graus.
                let erro = (p.to_ecef() - back.to_ecef()).length();
                pior = pior.max(erro);
                lon += 7.5;
            }
            lat += 7.0;
        }
        assert!(pior < TOL_M, "pior erro do roundtrip: {pior} m");
    }
}

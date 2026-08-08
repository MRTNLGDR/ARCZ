//! Retangulo geografico em graus decimais.

use crate::wgs84::Geodetic;

/// Limite de latitude do Web Mercator. Alem disso a projecao vai para o infinito.
pub const WEB_MERCATOR_LAT_LIMIT: f64 = 85.051_128_779_806_6;

/// Retangulo lon/lat, em graus. `west < east` e `south < north`.
///
/// **Nao suporta cruzamento do antimeridiano** (west > east). Isso e deliberado:
/// nenhum caso de uso do ARCZ atravessa ±180°, e suportar isso espalharia casos
/// especiais por todo o pipeline de tiles. [`GeoBBox::new`] rejeita a entrada.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeoBBox {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BBoxError {
    /// west >= east, ou o retangulo cruza o antimeridiano.
    LongitudeInvalida,
    /// south >= north.
    LatitudeInvalida,
    /// Fora do dominio do Web Mercator.
    ForaDoMercator,
}

impl core::fmt::Display for BBoxError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let m = match self {
            Self::LongitudeInvalida => "west deve ser menor que east (antimeridiano nao suportado)",
            Self::LatitudeInvalida => "south deve ser menor que north",
            Self::ForaDoMercator => "latitude fora do dominio do Web Mercator (|lat| <= 85.0511)",
        };
        f.write_str(m)
    }
}

impl std::error::Error for BBoxError {}

impl GeoBBox {
    pub fn new(west: f64, south: f64, east: f64, north: f64) -> Result<Self, BBoxError> {
        // NaN e testado explicitamente: com NaN toda comparacao e falsa, entao um
        // `west >= east` sozinho deixaria NaN passar e contaminar a cena inteira.
        if !west.is_finite() || !east.is_finite() {
            return Err(BBoxError::LongitudeInvalida);
        }
        if !south.is_finite() || !north.is_finite() {
            return Err(BBoxError::LatitudeInvalida);
        }
        if west >= east || !(-180.0..=180.0).contains(&west) || !(-180.0..=180.0).contains(&east) {
            return Err(BBoxError::LongitudeInvalida);
        }
        if south >= north {
            return Err(BBoxError::LatitudeInvalida);
        }
        if south < -WEB_MERCATOR_LAT_LIMIT || north > WEB_MERCATOR_LAT_LIMIT {
            return Err(BBoxError::ForaDoMercator);
        }
        Ok(Self {
            west,
            south,
            east,
            north,
        })
    }

    /// Constroi um retangulo quadrado de `lado_m` metros centrado em `centro`.
    ///
    /// Util para o fluxo real do app: o usuario da um endereco/coordenada e um raio,
    /// nao quatro numeros.
    pub fn around(centro: Geodetic, lado_m: f64) -> Result<Self, BBoxError> {
        // Grau de latitude ~ constante; grau de longitude encolhe com cos(lat).
        const M_POR_GRAU_LAT: f64 = 111_132.0;
        let meio = lado_m / 2.0;
        let d_lat = meio / M_POR_GRAU_LAT;
        let d_lon = meio / (M_POR_GRAU_LAT * centro.lat_deg.to_radians().cos().abs().max(1e-6));
        Self::new(
            centro.lon_deg - d_lon,
            centro.lat_deg - d_lat,
            centro.lon_deg + d_lon,
            centro.lat_deg + d_lat,
        )
    }

    pub fn center(&self) -> Geodetic {
        Geodetic::new(
            (self.west + self.east) / 2.0,
            (self.south + self.north) / 2.0,
            0.0,
        )
    }

    pub fn width_deg(&self) -> f64 {
        self.east - self.west
    }

    pub fn height_deg(&self) -> f64 {
        self.north - self.south
    }

    pub fn contains(&self, p: Geodetic) -> bool {
        p.lon_deg >= self.west
            && p.lon_deg <= self.east
            && p.lat_deg >= self.south
            && p.lat_deg <= self.north
    }

    pub fn intersects(&self, other: &GeoBBox) -> bool {
        self.west < other.east
            && other.west < self.east
            && self.south < other.north
            && other.south < self.north
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejeita_retangulo_invertido() {
        assert_eq!(
            GeoBBox::new(10.0, 0.0, 5.0, 1.0),
            Err(BBoxError::LongitudeInvalida)
        );
        assert_eq!(
            GeoBBox::new(0.0, 10.0, 5.0, 1.0),
            Err(BBoxError::LatitudeInvalida)
        );
    }

    #[test]
    fn rejeita_antimeridiano_em_vez_de_gerar_tiles_errados() {
        // west=170, east=-170 e o caso classico que silenciosamente produz
        // uma faixa de tiles cobrindo o planeta inteiro ao contrario.
        assert_eq!(
            GeoBBox::new(170.0, -1.0, -170.0, 1.0),
            Err(BBoxError::LongitudeInvalida)
        );
    }

    #[test]
    fn rejeita_nan_e_infinito() {
        // Sem a checagem explicita, NaN passa por qualquer comparacao e vira geometria
        // inteira em NaN la na frente, longe da causa.
        assert!(GeoBBox::new(f64::NAN, 0.0, 1.0, 1.0).is_err());
        assert!(GeoBBox::new(0.0, f64::NAN, 1.0, 1.0).is_err());
        assert!(GeoBBox::new(0.0, 0.0, f64::NAN, 1.0).is_err());
        assert!(GeoBBox::new(0.0, 0.0, 1.0, f64::NAN).is_err());
        assert!(GeoBBox::new(f64::NEG_INFINITY, 0.0, 1.0, 1.0).is_err());
        assert!(GeoBBox::new(0.0, 0.0, f64::INFINITY, 1.0).is_err());
    }

    #[test]
    fn rejeita_polos_fora_do_mercator() {
        assert_eq!(
            GeoBBox::new(-1.0, -89.0, 1.0, 89.0),
            Err(BBoxError::ForaDoMercator)
        );
    }

    #[test]
    fn around_produz_quadrado_com_o_lado_pedido() {
        let centro = Geodetic::new(-46.633_308, -23.550_520, 0.0);
        let b = GeoBBox::around(centro, 4_000.0).unwrap();

        let c = b.center();
        assert!((c.lon_deg - centro.lon_deg).abs() < 1e-9);
        assert!((c.lat_deg - centro.lat_deg).abs() < 1e-9);

        // Mede o lado de verdade, em metros, pelo quadro ENU.
        let frame = crate::enu::EnuFrame::new(centro);
        let sw = frame.geodetic_to_enu(Geodetic::new(b.west, b.south, 0.0));
        let ne = frame.geodetic_to_enu(Geodetic::new(b.east, b.north, 0.0));

        let largura = ne.e - sw.e;
        let altura = ne.n - sw.n;
        // 2% de tolerancia: a aproximacao de graus->metros e de primeira ordem.
        assert!((largura - 4_000.0).abs() < 80.0, "largura = {largura} m");
        assert!((altura - 4_000.0).abs() < 80.0, "altura = {altura} m");
    }

    #[test]
    fn contains_e_intersects() {
        let a = GeoBBox::new(-1.0, -1.0, 1.0, 1.0).unwrap();
        assert!(a.contains(Geodetic::new(0.0, 0.0, 0.0)));
        assert!(!a.contains(Geodetic::new(2.0, 0.0, 0.0)));

        let b = GeoBBox::new(0.5, 0.5, 2.0, 2.0).unwrap();
        let c = GeoBBox::new(5.0, 5.0, 6.0, 6.0).unwrap();
        assert!(a.intersects(&b));
        assert!(!a.intersects(&c));
    }
}

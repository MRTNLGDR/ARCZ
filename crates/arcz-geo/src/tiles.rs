//! Indexacao de tiles no esquema slippy/XYZ (Web Mercator, EPSG:3857).
//!
//! E o esquema usado por praticamente todas as fontes publicas de DEM e imagery
//! (AWS Terrain Tiles, NASA GIBS, EOX s2cloudless, OSM), entao o ARCZ fala esse
//! dialeto nativamente em vez de reprojetar na mao.

use crate::bbox::{GeoBBox, WEB_MERCATOR_LAT_LIMIT};
use crate::wgs84::Geodetic;

/// Identificador de um tile: zoom, coluna (x, oeste->leste), linha (y, **norte->sul**).
///
/// Atencao ao eixo Y: no esquema XYZ ele cresce para o sul. O esquema TMS usa o
/// oposto; [`TileId::to_tms_y`] converte quando alguma fonte exigir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileId {
    pub z: u8,
    pub x: u32,
    pub y: u32,
}

impl TileId {
    pub const fn new(z: u8, x: u32, y: u32) -> Self {
        Self { z, x, y }
    }

    /// Quantidade de tiles por eixo neste zoom (2^z).
    pub const fn count_per_axis(z: u8) -> u32 {
        1u32 << z
    }

    /// Tile que contem o ponto dado.
    ///
    /// Latitudes fora do dominio do Mercator sao grampeadas ao limite, em vez de
    /// produzir indice invalido.
    pub fn from_geodetic(p: Geodetic, z: u8) -> Self {
        let n = Self::count_per_axis(z) as f64;
        let lat = p
            .lat_deg
            .clamp(-WEB_MERCATOR_LAT_LIMIT, WEB_MERCATOR_LAT_LIMIT);
        let lon = p.lon_deg.clamp(-180.0, 180.0);

        let x = ((lon + 180.0) / 360.0 * n).floor();
        let y = ((1.0 - asinh_tan_lat(lat) / core::f64::consts::PI) / 2.0 * n).floor();

        Self {
            z,
            x: (x as i64).clamp(0, n as i64 - 1) as u32,
            y: (y as i64).clamp(0, n as i64 - 1) as u32,
        }
    }

    /// Retangulo geografico coberto por este tile.
    pub fn bounds(&self) -> GeoBBox {
        let n = Self::count_per_axis(self.z) as f64;
        let west = self.x as f64 / n * 360.0 - 180.0;
        let east = (self.x + 1) as f64 / n * 360.0 - 180.0;
        // y cresce para o sul, entao y+1 e a borda SUL.
        let north = mercator_y_to_lat(1.0 - 2.0 * (self.y as f64 / n));
        let south = mercator_y_to_lat(1.0 - 2.0 * ((self.y + 1) as f64 / n));

        GeoBBox {
            west,
            south,
            east,
            north,
        }
    }

    /// Converte a linha para a convencao TMS (y crescendo para o norte).
    pub const fn to_tms_y(&self) -> u32 {
        Self::count_per_axis(self.z) - 1 - self.y
    }

    /// Latitude/longitude de um ponto interno ao tile, dado em coordenada normalizada
    /// `[0,1]²` com origem no canto **noroeste** — a mesma convencao dos pixels da
    /// imagem baixada, o que evita inverter eixo na hora de amostrar o DEM.
    pub fn pixel_to_geodetic(&self, u: f64, v: f64) -> Geodetic {
        let n = Self::count_per_axis(self.z) as f64;
        let lon = (self.x as f64 + u) / n * 360.0 - 180.0;
        let lat = mercator_y_to_lat(1.0 - 2.0 * ((self.y as f64 + v) / n));
        Geodetic::new(lon, lat, 0.0)
    }

    pub fn parent(&self) -> Option<TileId> {
        if self.z == 0 {
            None
        } else {
            Some(TileId::new(self.z - 1, self.x / 2, self.y / 2))
        }
    }
}

/// Faixa retangular de tiles num mesmo zoom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileRange {
    pub z: u8,
    pub x_min: u32,
    pub x_max: u32,
    pub y_min: u32,
    pub y_max: u32,
}

impl TileRange {
    /// Menor faixa de tiles que cobre completamente o retangulo dado.
    pub fn covering(bbox: &GeoBBox, z: u8) -> Self {
        let nw = TileId::from_geodetic(Geodetic::new(bbox.west, bbox.north, 0.0), z);
        let se = TileId::from_geodetic(Geodetic::new(bbox.east, bbox.south, 0.0), z);
        Self {
            z,
            x_min: nw.x.min(se.x),
            x_max: nw.x.max(se.x),
            y_min: nw.y.min(se.y),
            y_max: nw.y.max(se.y),
        }
    }

    pub fn count(&self) -> u64 {
        (self.x_max - self.x_min + 1) as u64 * (self.y_max - self.y_min + 1) as u64
    }

    pub fn iter(&self) -> impl Iterator<Item = TileId> + '_ {
        let z = self.z;
        (self.y_min..=self.y_max)
            .flat_map(move |y| (self.x_min..=self.x_max).map(move |x| TileId::new(z, x, y)))
    }

    /// Retangulo geografico da faixa inteira (uniao dos bounds dos tiles).
    pub fn bounds(&self) -> GeoBBox {
        let nw = TileId::new(self.z, self.x_min, self.y_min).bounds();
        let se = TileId::new(self.z, self.x_max, self.y_max).bounds();
        GeoBBox {
            west: nw.west,
            south: se.south,
            east: se.east,
            north: nw.north,
        }
    }
}

/// `asinh(tan(lat))` — a projecao Y do Web Mercator, escrita da forma numericamente
/// estavel (a forma `ln(tan + sec)` perde precisao perto do equador).
fn asinh_tan_lat(lat_deg: f64) -> f64 {
    lat_deg.to_radians().tan().asinh()
}

/// Inversa: recebe Y do Mercator normalizado em `[-1, 1]` e devolve latitude em graus.
fn mercator_y_to_lat(y_norm: f64) -> f64 {
    (y_norm * core::f64::consts::PI).sinh().atan().to_degrees()
}

/// Longitude -> coordenada X global do Web Mercator, normalizada em `[0, 1]`
/// (0 = -180°, 1 = +180°).
///
/// Usada tanto para amostrar mosaicos de tile quanto para gerar UV de textura. Textura
/// de imagery **e** Mercator: interpolar UV linearmente em latitude desalinha a imagem
/// do terreno progressivamente conforme a latitude sobe. Este par de funcoes existe
/// para que ninguem seja tentado a fazer isso.
pub fn lon_to_mercator_x(lon_deg: f64) -> f64 {
    (lon_deg.clamp(-180.0, 180.0) + 180.0) / 360.0
}

/// Latitude -> coordenada Y global do Web Mercator, normalizada em `[0, 1]`,
/// com **0 no norte** (mesma orientacao dos pixels e do indice `y` do tile).
pub fn lat_to_mercator_y(lat_deg: f64) -> f64 {
    let lat = lat_deg.clamp(-WEB_MERCATOR_LAT_LIMIT, WEB_MERCATOR_LAT_LIMIT);
    (1.0 - asinh_tan_lat(lat) / core::f64::consts::PI) / 2.0
}

/// Inversa de [`lat_to_mercator_y`].
pub fn mercator_y_to_lat_deg(y_norm: f64) -> f64 {
    mercator_y_to_lat(1.0 - 2.0 * y_norm)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAO_PAULO: Geodetic = Geodetic::new(-46.633_308, -23.550_520, 0.0);

    #[test]
    fn zoom_zero_tem_um_tile_so() {
        let t = TileId::from_geodetic(SAO_PAULO, 0);
        assert_eq!(t, TileId::new(0, 0, 0));

        let b = t.bounds();
        assert!((b.west + 180.0).abs() < 1e-9);
        assert!((b.east - 180.0).abs() < 1e-9);
        assert!(
            (b.north - WEB_MERCATOR_LAT_LIMIT).abs() < 1e-6,
            "{}",
            b.north
        );
        assert!(
            (b.south + WEB_MERCATOR_LAT_LIMIT).abs() < 1e-6,
            "{}",
            b.south
        );
    }

    #[test]
    fn zoom_um_separa_os_quatro_quadrantes() {
        // NO, NE, SO, SE
        assert_eq!(
            TileId::from_geodetic(Geodetic::new(-1.0, 1.0, 0.0), 1),
            TileId::new(1, 0, 0)
        );
        assert_eq!(
            TileId::from_geodetic(Geodetic::new(1.0, 1.0, 0.0), 1),
            TileId::new(1, 1, 0)
        );
        assert_eq!(
            TileId::from_geodetic(Geodetic::new(-1.0, -1.0, 0.0), 1),
            TileId::new(1, 0, 1)
        );
        assert_eq!(
            TileId::from_geodetic(Geodetic::new(1.0, -1.0, 0.0), 1),
            TileId::new(1, 1, 1)
        );
    }

    #[test]
    fn tile_sempre_contem_o_ponto_que_o_gerou() {
        let pontos = [
            SAO_PAULO,
            Geodetic::new(0.0, 0.0, 0.0),
            Geodetic::new(-179.9, -84.0, 0.0),
            Geodetic::new(179.9, 84.0, 0.0),
            Geodetic::new(139.6917, 35.6895, 0.0),
        ];
        for p in pontos {
            for z in 0..=18u8 {
                let t = TileId::from_geodetic(p, z);
                let b = t.bounds();
                assert!(
                    p.lon_deg >= b.west - 1e-9 && p.lon_deg <= b.east + 1e-9,
                    "z={z} lon {} fora de [{}, {}]",
                    p.lon_deg,
                    b.west,
                    b.east
                );
                assert!(
                    p.lat_deg >= b.south - 1e-9 && p.lat_deg <= b.north + 1e-9,
                    "z={z} lat {} fora de [{}, {}]",
                    p.lat_deg,
                    b.south,
                    b.north
                );
            }
        }
    }

    #[test]
    fn tiles_vizinhos_encostam_sem_buraco_nem_sobreposicao() {
        let t = TileId::new(12, 1517, 2324);
        let leste = TileId::new(12, 1518, 2324);
        let sul = TileId::new(12, 1517, 2325);

        assert!((t.bounds().east - leste.bounds().west).abs() < 1e-12);
        assert!((t.bounds().south - sul.bounds().north).abs() < 1e-12);
    }

    #[test]
    fn pixel_uv_percorre_o_tile_do_noroeste_ao_sudeste() {
        let t = TileId::new(14, 6070, 9298);
        let b = t.bounds();

        let nw = t.pixel_to_geodetic(0.0, 0.0);
        let se = t.pixel_to_geodetic(1.0, 1.0);

        assert!((nw.lon_deg - b.west).abs() < 1e-9);
        assert!((nw.lat_deg - b.north).abs() < 1e-9);
        assert!((se.lon_deg - b.east).abs() < 1e-9);
        assert!((se.lat_deg - b.south).abs() < 1e-9);
        // v=0 e o NORTE. Inverter isso e o bug classico de DEM de cabeca pra baixo.
        assert!(nw.lat_deg > se.lat_deg);
    }

    #[test]
    fn parent_contem_o_filho() {
        let t = TileId::new(14, 6070, 9298);
        let p = t.parent().unwrap();
        assert_eq!(p.z, 13);

        let tb = t.bounds();
        let pb = p.bounds();
        assert!(pb.west <= tb.west + 1e-9 && pb.east >= tb.east - 1e-9);
        assert!(pb.south <= tb.south + 1e-9 && pb.north >= tb.north - 1e-9);
        assert!(TileId::new(0, 0, 0).parent().is_none());
    }

    #[test]
    fn range_cobre_a_bbox_inteira() {
        let bbox = GeoBBox::around(SAO_PAULO, 6_000.0).unwrap();
        let z = 14;
        let range = TileRange::covering(&bbox, z);

        assert!(range.count() >= 1);
        let rb = range.bounds();
        assert!(rb.west <= bbox.west + 1e-9, "{} > {}", rb.west, bbox.west);
        assert!(rb.east >= bbox.east - 1e-9);
        assert!(rb.south <= bbox.south + 1e-9);
        assert!(rb.north >= bbox.north - 1e-9);

        // O iterador tem que devolver exatamente `count()` tiles distintos.
        let tiles: Vec<_> = range.iter().collect();
        assert_eq!(tiles.len() as u64, range.count());
        let unicos: std::collections::HashSet<_> = tiles.iter().collect();
        assert_eq!(unicos.len(), tiles.len());
    }

    #[test]
    fn mercator_normalizado_bate_com_o_indice_do_tile() {
        for z in [0u8, 5, 12, 18] {
            let n = TileId::count_per_axis(z) as f64;
            for p in [
                SAO_PAULO,
                Geodetic::new(13.405, 52.52, 0.0),
                Geodetic::new(0.0, 0.0, 0.0),
            ] {
                let t = TileId::from_geodetic(p, z);
                let x = (lon_to_mercator_x(p.lon_deg) * n).floor() as u32;
                let y = (lat_to_mercator_y(p.lat_deg) * n).floor() as u32;
                assert_eq!((t.x, t.y), (x, y), "z={z} p={p:?}");
            }
        }
    }

    #[test]
    fn mercator_y_faz_roundtrip() {
        for lat in [-84.0, -23.55, -0.0001, 0.0, 12.3, 52.52, 84.0] {
            let back = mercator_y_to_lat_deg(lat_to_mercator_y(lat));
            assert!((back - lat).abs() < 1e-9, "lat {lat} -> {back}");
        }
        // 0 = norte, 1 = sul. Inverter isso vira DEM/textura de cabeca pra baixo.
        assert!(lat_to_mercator_y(80.0) < lat_to_mercator_y(-80.0));
        assert!((lat_to_mercator_y(0.0) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn tms_inverte_o_eixo_y() {
        let t = TileId::new(2, 1, 0);
        assert_eq!(t.to_tms_y(), 3);
        assert_eq!(TileId::new(2, 1, 3).to_tms_y(), 0);
    }
}

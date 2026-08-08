//! Mosaicos: junta uma faixa de tiles num unico buffer amostravel por lon/lat.
//!
//! A Fatia 0 monta **um** mosaico de altura e **um** de imagem para a regiao inteira,
//! em vez de streaming por tile. E o suficiente para areas de ate ~20 km e mantem a
//! malha e a textura triviais de validar. O streaming com quadtree entra na Fatia 2,
//! quando a area passa a ser maior que a memoria.

use arcz_geo::tiles::{lat_to_mercator_y, lon_to_mercator_x};
use arcz_geo::{GeoBBox, TileId, TileRange};
use tokio::task::JoinSet;

use crate::cache::TileCache;
use crate::source::{DemSource, ImagerySource, License};
use crate::TerrainError;

/// Teto de tiles por faixa. 1024 tiles de 256 px = 65536² px de mosaico, ~4 GB em RGBA.
/// Bater nesse limite significa que a area/zoom estao errados, nao que falta memoria.
pub const MAX_TILES_POR_FAIXA: u64 = 1024;

/// Um tile ja decodificado. `None` significa que a fonte nao tem cobertura ali
/// (oceano, borda do dataset) — nao e erro.
pub type TileDecodificado = (TileId, Option<Vec<f32>>);

/// Resultado do download+decode de um tile de DEM: `(lado_em_pixels, alturas)`.
type ResultadoDem = Result<(u32, Vec<f32>), TerrainError>;

/// Grade de alturas em metros cobrindo uma faixa de tiles.
#[derive(Debug, Clone)]
pub struct HeightMosaic {
    range: TileRange,
    tile_size: u32,
    width: u32,
    height: u32,
    /// Altura em metros, linha por linha, do noroeste para o sudeste.
    data: Vec<f32>,
    /// Quantos tiles vieram 404 (oceano / fora de cobertura) e foram preenchidos com 0.
    pub tiles_ausentes: u32,
}

impl HeightMosaic {
    pub fn range(&self) -> TileRange {
        self.range
    }
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn bounds(&self) -> GeoBBox {
        self.range.bounds()
    }

    /// Altura no pixel (grampeado nas bordas).
    pub fn at(&self, x: i64, y: i64) -> f32 {
        let x = x.clamp(0, self.width as i64 - 1) as usize;
        let y = y.clamp(0, self.height as i64 - 1) as usize;
        self.data[y * self.width as usize + x]
    }

    /// Altura interpolada bilinearmente na coordenada geografica dada.
    ///
    /// Converte lon/lat pela projecao Mercator exata — nao por regra de tres em graus,
    /// que introduz erro crescente com a latitude.
    pub fn sample_geodetic(&self, lon_deg: f64, lat_deg: f64) -> f32 {
        let n = TileId::count_per_axis(self.range.z) as f64;
        let ts = self.tile_size as f64;

        // Pixel global -> pixel local do mosaico. O -0.5 alinha a amostra ao CENTRO
        // do pixel; sem isso a malha fica meio pixel deslocada da textura.
        let px = lon_to_mercator_x(lon_deg) * n * ts - self.range.x_min as f64 * ts - 0.5;
        let py = lat_to_mercator_y(lat_deg) * n * ts - self.range.y_min as f64 * ts - 0.5;

        let x0 = px.floor();
        let y0 = py.floor();
        let fx = (px - x0) as f32;
        let fy = (py - y0) as f32;
        let (x0, y0) = (x0 as i64, y0 as i64);

        let h00 = self.at(x0, y0);
        let h10 = self.at(x0 + 1, y0);
        let h01 = self.at(x0, y0 + 1);
        let h11 = self.at(x0 + 1, y0 + 1);

        let topo = h00 + (h10 - h00) * fx;
        let base = h01 + (h11 - h01) * fx;
        topo + (base - topo) * fy
    }

    /// Multiplica todas as alturas por `fator` (exagero vertical).
    ///
    /// Aplicado no DEM, e nao no vertice pronto, para que as normais saiam coerentes
    /// com o relevo exagerado — escalar so o eixo Y depois deixaria a iluminacao
    /// descrevendo um terreno que nao e o desenhado.
    pub fn escalar_alturas(&mut self, fator: f32) {
        if (fator - 1.0).abs() < f32::EPSILON {
            return;
        }
        for h in &mut self.data {
            *h *= fator;
        }
    }

    /// Eleva ao nivel do mar tudo que estiver abaixo de `minimo_m`.
    ///
    /// O AWS Terrain Tiles inclui **batimetria**: no mar as alturas sao profundidades
    /// (em Bombinhas o mosaico chega a -1272 m). Para visualizacao arquitetonica isso
    /// e ruido — afunda a agua num buraco e destroi a escala vertical da cena. O
    /// padrao do ARCZ e achatar no zero; quem quiser o fundo do mar pede
    /// explicitamente.
    pub fn achatar_batimetria(&mut self, minimo_m: f32) -> u32 {
        let mut afetados = 0;
        for h in &mut self.data {
            if *h < minimo_m {
                *h = minimo_m;
                afetados += 1;
            }
        }
        afetados
    }

    /// Menor e maior altura do mosaico, util para enquadrar a camera.
    pub fn min_max(&self) -> (f32, f32) {
        self.data
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &h| {
                (lo.min(h), hi.max(h))
            })
    }

    /// Monta um mosaico a partir de tiles ja decodificados. Exposto para teste.
    pub fn from_tiles(range: TileRange, tile_size: u32, tiles: Vec<TileDecodificado>) -> Self {
        let cols = range.x_max - range.x_min + 1;
        let rows = range.y_max - range.y_min + 1;
        let width = cols * tile_size;
        let height = rows * tile_size;
        let mut data = vec![0.0_f32; (width as usize) * (height as usize)];
        let mut ausentes = 0;

        for (id, alturas) in tiles {
            let Some(alturas) = alturas else {
                ausentes += 1;
                continue;
            };
            let ox = (id.x - range.x_min) * tile_size;
            let oy = (id.y - range.y_min) * tile_size;
            for ty in 0..tile_size {
                let src = (ty * tile_size) as usize;
                let dst = ((oy + ty) * width + ox) as usize;
                data[dst..dst + tile_size as usize]
                    .copy_from_slice(&alturas[src..src + tile_size as usize]);
            }
        }

        Self {
            range,
            tile_size,
            width,
            height,
            data,
            tiles_ausentes: ausentes,
        }
    }
}

/// O pixel e vegetacao numa ortofoto?
///
/// Regra simples e deliberadamente conservadora: o verde tem de dominar os dois
/// outros canais com margem. Telha, laje, reboco, areia e asfalto ficam de fora
/// dessa condicao; grama, mata e copa de arvore caem nela. Nao e classificacao
/// de sensoriamento remoto — para isso faltaria o infravermelho, que a ortofoto
/// RGB nao traz —, e sim um descarte barato de amostras que sujariam a cor.
pub fn e_vegetacao(r: f64, g: f64, b: f64) -> bool {
    g > r * 1.12 && g > b * 1.12 && g > 24.0
}

/// Mosaico RGBA de imagery, na mesma projecao dos tiles.
#[derive(Debug, Clone)]
pub struct ImageMosaic {
    range: TileRange,
    width: u32,
    height: u32,
    /// RGBA8, linha por linha, do noroeste para o sudeste.
    pub rgba: Vec<u8>,
    pub tiles_ausentes: u32,
}

impl ImageMosaic {
    /// Cor media da ortofoto num disco de `raio_m` em torno do ponto, em sRGB
    /// normalizado.
    ///
    /// Serve para o entorno procedural herdar a cor real do lugar em vez de sair
    /// branco: o telhado de uma casa de Bombinhas fica com o tom da telha que
    /// aparece na imagem de satelite.
    ///
    /// A media sobre um disco, e nao um pixel unico, existe porque um pixel
    /// isolado cai com frequencia numa sombra, num carro ou numa arvore — e a
    /// edificacao inteira herdaria essa cor. Amostrar uma vizinhanca do tamanho
    /// da propria construcao devolve o tom dominante.
    pub fn cor_media(&self, lon_deg: f64, lat_deg: f64, raio_m: f64) -> [f32; 3] {
        let (px, py) = self.pixel_de(lon_deg, lat_deg);

        // Metros por pixel na latitude dada, para converter o raio.
        let n = TileId::count_per_axis(self.range.z) as f64;
        let circunferencia = 40_075_016.686 * lat_deg.to_radians().cos();
        let m_por_pixel = circunferencia / (n * 256.0);
        let raio_px = (raio_m / m_por_pixel.max(1e-9)).clamp(1.0, 24.0);

        let r0 = raio_px as i64;
        let (mut soma, mut n_amostras) = ([0.0f64; 3], 0u32);
        let mut vistos = 0u32;
        for dy in -r0..=r0 {
            for dx in -r0..=r0 {
                if (dx * dx + dy * dy) as f64 > raio_px * raio_px {
                    continue;
                }
                let x = px as i64 + dx;
                let y = py as i64 + dy;
                if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
                    continue;
                }
                let i = ((y as usize) * self.width as usize + x as usize) * 4;
                let (r, g, b) = (
                    self.rgba[i] as f64,
                    self.rgba[i + 1] as f64,
                    self.rgba[i + 2] as f64,
                );
                vistos += 1;
                // Copa de arvore no lote pintava a casa de verde-mata. O verde de
                // vegetacao e o unico tom em que o canal G domina os outros dois
                // com folga; telha, laje, reboco e asfalto nunca fazem isso.
                if e_vegetacao(r, g, b) {
                    continue;
                }
                soma[0] += r;
                soma[1] += g;
                soma[2] += b;
                n_amostras += 1;
            }
        }

        if n_amostras == 0 {
            // Ou o ponto caiu fora do mosaico, ou o lote e so vegetacao. Nos dois
            // casos um cinza neutro e mais honesto que herdar o verde.
            let _ = vistos;
            return [0.62, 0.60, 0.58];
        }
        let k = 1.0 / (n_amostras as f64 * 255.0);
        [
            (soma[0] * k) as f32,
            (soma[1] * k) as f32,
            (soma[2] * k) as f32,
        ]
    }

    /// Cor media ignorando o filtro de vegetacao. Existe para os testes poderem
    /// medir o efeito do filtro em vez de confiar nele.
    #[cfg(test)]
    fn cor_media_crua(&self, lon_deg: f64, lat_deg: f64, raio_px: i64) -> [f32; 3] {
        let (px, py) = self.pixel_de(lon_deg, lat_deg);
        let (mut soma, mut n) = ([0.0f64; 3], 0u32);
        for dy in -raio_px..=raio_px {
            for dx in -raio_px..=raio_px {
                let x = px as i64 + dx;
                let y = py as i64 + dy;
                if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
                    continue;
                }
                let i = ((y as usize) * self.width as usize + x as usize) * 4;
                for (k, s) in soma.iter_mut().enumerate() {
                    *s += self.rgba[i + k] as f64;
                }
                n += 1;
            }
        }
        let k = 1.0 / (n.max(1) as f64 * 255.0);
        [
            (soma[0] * k) as f32,
            (soma[1] * k) as f32,
            (soma[2] * k) as f32,
        ]
    }

    /// Lado do tile em pixels, deduzido do mosaico.
    ///
    /// Nao se pode assumir 256: `from_raw` aceita qualquer tamanho, e os testes
    /// usam tiles pequenos justamente para caber no codigo. Fixar 256 faria a
    /// amostragem errar o pixel em silencio.
    fn tile_size(&self) -> f64 {
        let colunas = (self.range.x_max - self.range.x_min + 1).max(1);
        (self.width / colunas) as f64
    }

    /// Pixel do mosaico correspondente a uma coordenada geodetica.
    fn pixel_de(&self, lon_deg: f64, lat_deg: f64) -> (f64, f64) {
        let n = TileId::count_per_axis(self.range.z) as f64;
        let ts = self.tile_size();
        (
            lon_to_mercator_x(lon_deg) * n * ts - self.range.x_min as f64 * ts - 0.5,
            lat_to_mercator_y(lat_deg) * n * ts - self.range.y_min as f64 * ts - 0.5,
        )
    }

    /// Constroi a partir de um buffer RGBA ja montado. Usado por testes e por fontes
    /// que nao passam pelo caminho de download (ex.: ortofoto local).
    pub fn from_raw(range: TileRange, tile_size: u32, rgba: Vec<u8>) -> Self {
        let width = (range.x_max - range.x_min + 1) * tile_size;
        let height = (range.y_max - range.y_min + 1) * tile_size;
        assert_eq!(
            rgba.len(),
            (width as usize) * (height as usize) * 4,
            "buffer RGBA nao bate com {width}x{height}"
        );
        Self {
            range,
            width,
            height,
            rgba,
            tiles_ausentes: 0,
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn range(&self) -> TileRange {
        self.range
    }
    pub fn bounds(&self) -> GeoBBox {
        self.range.bounds()
    }

    /// Quantos pixels de textura cobrem a area pedida, no eixo maior.
    ///
    /// E o numero que decide se o mapa vai aparecer ou virar uma mancha de cor
    /// uniforme: a fonte padrao (NASA GIBS, z8) tem 500 m/pixel, entao uma area de
    /// 400 m recebe **menos de um pixel** de textura.
    pub fn pixels_para_area(&self, bbox: &GeoBBox) -> f64 {
        let n = TileId::count_per_axis(self.range.z) as f64;
        let ts = (self.width / (self.range.x_max - self.range.x_min + 1)) as f64;

        let dx = (lon_to_mercator_x(bbox.east) - lon_to_mercator_x(bbox.west)) * n * ts;
        let dy = (lat_to_mercator_y(bbox.south) - lat_to_mercator_y(bbox.north)) * n * ts;
        dx.abs().max(dy.abs())
    }

    /// Aviso quando a textura nao tem resolucao para a area, com a saida pratica.
    ///
    /// Existe porque o sintoma — terreno chapado, sem ruas nem telhados — parece um
    /// bug de render, mas e so falta de resolucao na fonte. Sem esta mensagem o
    /// usuario perde tempo procurando defeito onde nao ha.
    pub fn aviso_de_resolucao(&self, bbox: &GeoBBox, fonte: ImagerySource) -> Option<String> {
        const MINIMO_UTIL: f64 = 256.0;
        let px = self.pixels_para_area(bbox);
        if px >= MINIMO_UTIL {
            return None;
        }

        let lado_m = bbox.width_deg() * 111_132.0 * bbox.center().lat_deg.to_radians().cos();
        let m_por_px = if px > 0.01 {
            lado_m / px
        } else {
            f64::INFINITY
        };

        Some(format!(
            "a imagery cobre a area com apenas {px:.0} pixels ({m_por_px:.0} m por pixel) — \
             o terreno vai aparecer chapado, sem ruas nem telhados. Fonte atual: {} \
             (zoom maximo {}). Para escala urbana use --imagery esri --zoom-img 18 \
             --aceito-licenca (atencao: licenca restritiva).",
            fonte.nome(),
            fonte.zoom_maximo()
        ))
    }

    /// UV de textura para uma coordenada geografica, em `[0,1]²` com origem no noroeste.
    ///
    /// Calculado em Mercator, igual a projecao da propria textura. Fazer isso
    /// linearmente em latitude e o erro que faz a imagem "escorregar" do relevo.
    pub fn uv_for(&self, lon_deg: f64, lat_deg: f64) -> [f32; 2] {
        let n = TileId::count_per_axis(self.range.z) as f64;
        let cols = (self.range.x_max - self.range.x_min + 1) as f64;
        let rows = (self.range.y_max - self.range.y_min + 1) as f64;

        let u = (lon_to_mercator_x(lon_deg) * n - self.range.x_min as f64) / cols;
        let v = (lat_to_mercator_y(lat_deg) * n - self.range.y_min as f64) / rows;
        [u as f32, v as f32]
    }
}

/// Decodifica um tile *terrarium*: altura = `(R * 256 + G + B / 256) - 32768` metros.
pub fn decode_terrarium(id: TileId, bytes: &[u8]) -> Result<(u32, Vec<f32>), TerrainError> {
    let img = image::load_from_memory(bytes)?.to_rgb8();
    let (w, h) = img.dimensions();
    if w != h || w == 0 || !w.is_power_of_two() {
        return Err(TerrainError::DimensaoInvalida {
            id: format!("{}/{}/{}", id.z, id.x, id.y),
            largura: w,
            altura: h,
        });
    }

    let alturas = img
        .pixels()
        .map(|p| (p[0] as f32) * 256.0 + (p[1] as f32) + (p[2] as f32) / 256.0 - 32768.0)
        .collect();

    Ok((w, alturas))
}

/// Baixa e monta o mosaico de altura que cobre `bbox` no zoom pedido.
pub async fn fetch_height_mosaic(
    cache: &TileCache,
    source: DemSource,
    bbox: &GeoBBox,
    zoom: u8,
) -> Result<HeightMosaic, TerrainError> {
    let zoom = zoom.min(source.zoom_maximo());
    let range = TileRange::covering(bbox, zoom);
    checar_tamanho(range.count())?;

    let mut set: JoinSet<(TileId, ResultadoDem)> = JoinSet::new();
    for id in range.iter() {
        let cache = cache.clone();
        let url = source.url(id);
        set.spawn(async move {
            let r = match cache.get(&url).await {
                Ok(bytes) => decode_terrarium(id, &bytes),
                Err(e) => Err(e),
            };
            (id, r)
        });
    }

    let mut tile_size: Option<u32> = None;
    let mut tiles = Vec::with_capacity(range.count() as usize);

    while let Some(join) = set.join_next().await {
        let (id, resultado) = join.expect("tarefa de download entrou em panico");
        match resultado {
            Ok((size, alturas)) => {
                match tile_size {
                    None => tile_size = Some(size),
                    Some(s) if s != size => {
                        return Err(TerrainError::TamanhoInconsistente(s, size))
                    }
                    _ => {}
                }
                tiles.push((id, Some(alturas)));
            }
            // Sem cobertura (oceano, borda) e normal: vira nivel do mar.
            Err(TerrainError::TileAusente(url)) => {
                log::warn!("DEM ausente, preenchendo com 0 m: {url}");
                tiles.push((id, None));
            }
            Err(e) => return Err(e),
        }
    }

    // Faixa inteira sem cobertura: nao ha como saber o tamanho do tile, use o nominal.
    let tile_size = tile_size.unwrap_or(256);
    Ok(HeightMosaic::from_tiles(range, tile_size, tiles))
}

/// Baixa e monta o mosaico de imagery que cobre `bbox` no zoom pedido.
///
/// Fontes com licenca nao-comercial ou restritiva exigem `aceitar_licenca = true`.
pub async fn fetch_image_mosaic(
    cache: &TileCache,
    source: ImagerySource,
    bbox: &GeoBBox,
    zoom: u8,
    aceitar_licenca: bool,
) -> Result<ImageMosaic, TerrainError> {
    if !source.license().comercialmente_segura() && !aceitar_licenca {
        return Err(TerrainError::LicencaNaoAceita {
            fonte: source.nome().to_string(),
            licenca: source.license(),
        });
    }

    let zoom = zoom.min(source.zoom_maximo());
    let range = TileRange::covering(bbox, zoom);
    checar_tamanho(range.count())?;

    let mut set: JoinSet<(TileId, Result<image::RgbaImage, TerrainError>)> = JoinSet::new();
    for id in range.iter() {
        let cache = cache.clone();
        let url = source.url(id);
        set.spawn(async move {
            let r = match cache.get(&url).await {
                Ok(bytes) => image::load_from_memory(&bytes)
                    .map(|i| i.to_rgba8())
                    .map_err(TerrainError::from),
                Err(e) => Err(e),
            };
            (id, r)
        });
    }

    let mut recebidos: Vec<(TileId, Option<image::RgbaImage>)> =
        Vec::with_capacity(range.count() as usize);
    let mut tile_size: Option<u32> = None;

    while let Some(join) = set.join_next().await {
        let (id, resultado) = join.expect("tarefa de download entrou em panico");
        match resultado {
            Ok(img) => {
                let (w, h) = img.dimensions();
                if w != h {
                    return Err(TerrainError::DimensaoInvalida {
                        id: format!("{}/{}/{}", id.z, id.x, id.y),
                        largura: w,
                        altura: h,
                    });
                }
                match tile_size {
                    None => tile_size = Some(w),
                    Some(s) if s != w => return Err(TerrainError::TamanhoInconsistente(s, w)),
                    _ => {}
                }
                recebidos.push((id, Some(img)));
            }
            Err(TerrainError::TileAusente(url)) => {
                log::warn!("imagery ausente, preenchendo com transparente: {url}");
                recebidos.push((id, None));
            }
            Err(e) => return Err(e),
        }
    }

    let ts = tile_size.unwrap_or(256);
    let cols = range.x_max - range.x_min + 1;
    let rows = range.y_max - range.y_min + 1;
    let width = cols * ts;
    let height = rows * ts;
    let mut rgba = vec![0u8; (width as usize) * (height as usize) * 4];
    let mut ausentes = 0;

    for (id, img) in recebidos {
        let Some(img) = img else {
            ausentes += 1;
            continue;
        };
        let ox = ((id.x - range.x_min) * ts) as usize;
        let oy = ((id.y - range.y_min) * ts) as usize;
        let src = img.as_raw();
        for ty in 0..ts as usize {
            let s = ty * ts as usize * 4;
            let d = ((oy + ty) * width as usize + ox) * 4;
            rgba[d..d + ts as usize * 4].copy_from_slice(&src[s..s + ts as usize * 4]);
        }
    }

    Ok(ImageMosaic {
        range,
        width,
        height,
        rgba,
        tiles_ausentes: ausentes,
    })
}

fn checar_tamanho(n: u64) -> Result<(), TerrainError> {
    if n > MAX_TILES_POR_FAIXA {
        Err(TerrainError::FaixaGrandeDemais(n, MAX_TILES_POR_FAIXA))
    } else {
        Ok(())
    }
}

/// Reexportado para o app poder mostrar a licenca sem depender de `source`.
pub fn licenca_de(source: ImagerySource) -> License {
    source.license()
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcz_geo::Geodetic;

    /// Gera um PNG terrarium sintetico onde a altura e funcao conhecida do pixel.
    fn png_terrarium(size: u32, altura: impl Fn(u32, u32) -> f32) -> Vec<u8> {
        let mut img = image::RgbImage::new(size, size);
        for y in 0..size {
            for x in 0..size {
                let v = altura(x, y) + 32768.0;
                let total = (v * 256.0).round().clamp(0.0, 16_777_215.0) as u32;
                let r = (total >> 16) as u8;
                let g = ((total >> 8) & 0xff) as u8;
                let b = (total & 0xff) as u8;
                img.put_pixel(x, y, image::Rgb([r, g, b]));
            }
        }
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    #[test]
    fn terrarium_decodifica_alturas_conhecidas() {
        let id = TileId::new(10, 1, 1);
        // Alturas que exercitam o byte B (fracao de 1/256 m) e valores negativos.
        let png = png_terrarium(16, |x, y| (x as f32) * 100.0 - (y as f32) * 0.5 - 400.0);
        let (size, alturas) = decode_terrarium(id, &png).unwrap();

        assert_eq!(size, 16);
        assert_eq!(alturas.len(), 256);
        for y in 0..16u32 {
            for x in 0..16u32 {
                let esperado = (x as f32) * 100.0 - (y as f32) * 0.5 - 400.0;
                let obtido = alturas[(y * 16 + x) as usize];
                // Quantizacao do terrarium e 1/256 m.
                assert!(
                    (obtido - esperado).abs() < 1.0 / 256.0 + 1e-4,
                    "({x},{y}): {obtido} != {esperado}"
                );
            }
        }
    }

    #[test]
    fn terrarium_representa_nivel_do_mar_e_fossa() {
        let id = TileId::new(1, 0, 0);
        for alvo in [0.0_f32, -10_000.0, 8_848.0] {
            let png = png_terrarium(8, |_, _| alvo);
            let (_, a) = decode_terrarium(id, &png).unwrap();
            assert!((a[0] - alvo).abs() < 0.01, "alvo {alvo} deu {}", a[0]);
        }
    }

    #[test]
    fn rejeita_tile_nao_quadrado() {
        let mut img = image::RgbImage::new(8, 4);
        img.put_pixel(0, 0, image::Rgb([0, 0, 0]));
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();

        let e = decode_terrarium(TileId::new(1, 0, 0), &out.into_inner()).unwrap_err();
        assert!(matches!(e, TerrainError::DimensaoInvalida { .. }), "{e:?}");
    }

    /// Mosaico 2x2 onde cada tile tem uma altura constante distinta. Amostrar o centro
    /// de cada tile tem que devolver exatamente aquela altura — prova que o offset de
    /// tiles e a projecao Mercator estao coerentes.
    #[test]
    fn mosaico_posiciona_cada_tile_no_lugar_certo() {
        let z = 6;
        let range = TileRange {
            z,
            x_min: 20,
            x_max: 21,
            y_min: 35,
            y_max: 36,
        };
        let ts = 8;

        let mut tiles = Vec::new();
        for id in range.iter() {
            let marca = ((id.x - range.x_min) * 2 + (id.y - range.y_min)) as f32 * 1000.0;
            tiles.push((id, Some(vec![marca; (ts * ts) as usize])));
        }

        let m = HeightMosaic::from_tiles(range, ts, tiles);
        assert_eq!(m.width(), 16);
        assert_eq!(m.height(), 16);
        assert_eq!(m.tiles_ausentes, 0);

        for id in range.iter() {
            let centro = id.pixel_to_geodetic(0.5, 0.5);
            let esperado = ((id.x - range.x_min) * 2 + (id.y - range.y_min)) as f32 * 1000.0;
            let obtido = m.sample_geodetic(centro.lon_deg, centro.lat_deg);
            assert!(
                (obtido - esperado).abs() < 1e-3,
                "tile {id:?}: esperado {esperado}, obtido {obtido}"
            );
        }
    }

    #[test]
    fn tile_ausente_vira_nivel_do_mar_sem_quebrar() {
        let range = TileRange {
            z: 4,
            x_min: 0,
            x_max: 1,
            y_min: 0,
            y_max: 0,
        };
        let tiles = vec![
            (TileId::new(4, 0, 0), Some(vec![500.0; 16])),
            (TileId::new(4, 1, 0), None),
        ];
        let m = HeightMosaic::from_tiles(range, 4, tiles);

        assert_eq!(m.tiles_ausentes, 1);
        assert_eq!(m.at(0, 0), 500.0);
        assert_eq!(m.at(7, 0), 0.0);
        let (lo, hi) = m.min_max();
        assert_eq!((lo, hi), (0.0, 500.0));
    }

    #[test]
    fn achatar_batimetria_so_mexe_no_que_esta_abaixo_do_mar() {
        let range = TileRange {
            z: 4,
            x_min: 0,
            x_max: 0,
            y_min: 0,
            y_max: 0,
        };
        // Fundo do mar, praia e morro.
        let alturas = vec![-1272.0, -5.0, 0.0, 12.0, 184.0, -0.1, 900.0, 3.0];
        let mut m = HeightMosaic::from_tiles(
            range,
            2,
            vec![(TileId::new(4, 0, 0), Some(alturas[..4].to_vec()))],
        );
        m.data = alturas.clone();

        let afetados = m.achatar_batimetria(0.0);
        assert_eq!(afetados, 3, "esperava 3 valores negativos achatados");

        let (lo, hi) = m.min_max();
        assert_eq!(lo, 0.0, "ainda ha profundidade negativa");
        assert_eq!(hi, 900.0, "o relevo acima do mar nao pode mudar");
        // Terra firme intacta.
        assert_eq!(m.data[3], 12.0);
        assert_eq!(m.data[4], 184.0);
    }

    #[test]
    fn achatar_batimetria_em_terreno_seco_nao_faz_nada() {
        let range = TileRange {
            z: 4,
            x_min: 0,
            x_max: 0,
            y_min: 0,
            y_max: 0,
        };
        let mut m = HeightMosaic::from_tiles(
            range,
            2,
            vec![(TileId::new(4, 0, 0), Some(vec![700.0, 720.0, 760.0, 800.0]))],
        );
        assert_eq!(m.achatar_batimetria(0.0), 0);
        assert_eq!(m.min_max(), (700.0, 800.0));
    }

    #[test]
    fn interpolacao_bilinear_e_monotona_entre_dois_valores() {
        let range = TileRange {
            z: 8,
            x_min: 100,
            x_max: 100,
            y_min: 100,
            y_max: 100,
        };
        // Rampa horizontal: coluna 0 = 0 m, coluna 7 = 700 m.
        let alturas: Vec<f32> = (0..64).map(|i| ((i % 8) as f32) * 100.0).collect();
        let m = HeightMosaic::from_tiles(range, 8, vec![(TileId::new(8, 100, 100), Some(alturas))]);

        let id = TileId::new(8, 100, 100);
        let mut anterior = f32::NEG_INFINITY;
        for i in 0..=20 {
            let u = i as f64 / 20.0;
            let p = id.pixel_to_geodetic(u, 0.5);
            let h = m.sample_geodetic(p.lon_deg, p.lat_deg);
            assert!(
                h >= anterior - 1e-3,
                "rampa nao monotona em u={u}: {h} < {anterior}"
            );
            anterior = h;
        }
        assert!(
            anterior > 500.0,
            "a rampa nao chegou perto do topo: {anterior}"
        );
    }

    #[test]
    fn uv_de_imagery_vai_de_zero_no_noroeste_a_um_no_sudeste() {
        let range = TileRange {
            z: 10,
            x_min: 380,
            x_max: 381,
            y_min: 580,
            y_max: 581,
        };
        let m = ImageMosaic {
            range,
            width: 512,
            height: 512,
            rgba: vec![0; 512 * 512 * 4],
            tiles_ausentes: 0,
        };

        let b = range.bounds();
        let nw = m.uv_for(b.west, b.north);
        let se = m.uv_for(b.east, b.south);

        assert!(nw[0].abs() < 1e-6 && nw[1].abs() < 1e-6, "NO deu {nw:?}");
        assert!(
            (se[0] - 1.0).abs() < 1e-6 && (se[1] - 1.0).abs() < 1e-6,
            "SE deu {se:?}"
        );

        // v cresce para o sul.
        let meio_norte = m.uv_for(b.center().lon_deg, b.north);
        let meio_sul = m.uv_for(b.center().lon_deg, b.south);
        assert!(meio_norte[1] < meio_sul[1]);
    }

    #[test]
    fn avisa_quando_a_imagery_e_grosseira_demais_para_a_area() {
        // Reproduz o caso real: area de 400 m com NASA GIBS z8 (500 m/pixel).
        // O terreno inteiro recebe menos de um pixel de textura e vira uma
        // mancha de cor unica — sintoma que parece bug de render, mas nao e.
        let bbox = GeoBBox::around(Geodetic::new(-48.5022653, -27.1544967, 0.0), 400.0).unwrap();
        let range = TileRange::covering(&bbox, 8);
        let ts = 256;
        let cols = range.x_max - range.x_min + 1;
        let rows = range.y_max - range.y_min + 1;
        let m = ImageMosaic::from_raw(
            range,
            ts,
            vec![128u8; (cols * ts) as usize * (rows * ts) as usize * 4],
        );

        let px = m.pixels_para_area(&bbox);
        assert!(
            px < 2.0,
            "z8 numa area de 400 m deveria dar ~1 px, deu {px}"
        );

        let aviso = m
            .aviso_de_resolucao(&bbox, ImagerySource::NasaBlueMarble)
            .expect("deveria avisar");
        assert!(
            aviso.contains("esri"),
            "o aviso precisa dizer a saida: {aviso}"
        );
        assert!(aviso.contains("chapado"), "{aviso}");
    }

    #[test]
    fn nao_avisa_quando_a_resolucao_e_suficiente() {
        // Mesma area em z18 (Esri): ~1400 px de textura, muito acima do minimo.
        let bbox = GeoBBox::around(Geodetic::new(-48.5022653, -27.1544967, 0.0), 400.0).unwrap();
        let range = TileRange::covering(&bbox, 18);
        let ts = 256;
        let cols = range.x_max - range.x_min + 1;
        let rows = range.y_max - range.y_min + 1;
        let m = ImageMosaic::from_raw(
            range,
            ts,
            vec![200u8; (cols * ts) as usize * (rows * ts) as usize * 4],
        );

        let px = m.pixels_para_area(&bbox);
        assert!(
            px > 500.0,
            "z18 em 400 m deveria dar centenas de px, deu {px}"
        );
        assert!(m
            .aviso_de_resolucao(&bbox, ImagerySource::EsriWorldImagery)
            .is_none());
    }

    #[test]
    fn a_contagem_de_pixels_cresce_com_o_zoom() {
        let bbox = GeoBBox::around(Geodetic::new(-48.5, -27.15, 0.0), 500.0).unwrap();
        let px = |z: u8| {
            let range = TileRange::covering(&bbox, z);
            let ts = 256;
            let cols = range.x_max - range.x_min + 1;
            let rows = range.y_max - range.y_min + 1;
            ImageMosaic::from_raw(
                range,
                ts,
                vec![0u8; (cols * ts) as usize * (rows * ts) as usize * 4],
            )
            .pixels_para_area(&bbox)
        };
        // Cada nivel de zoom dobra a resolucao linear.
        let a = px(14);
        let b = px(15);
        assert!(
            (b / a - 2.0).abs() < 0.05,
            "z15 deveria ter o dobro de px de z14: {a} -> {b}"
        );
    }

    #[test]
    fn faixa_grande_demais_e_recusada_antes_de_baixar() {
        assert!(checar_tamanho(MAX_TILES_POR_FAIXA).is_ok());
        let e = checar_tamanho(MAX_TILES_POR_FAIXA + 1).unwrap_err();
        assert!(matches!(e, TerrainError::FaixaGrandeDemais(..)), "{e:?}");
    }

    #[tokio::test]
    async fn imagery_nao_comercial_e_bloqueada_por_padrao() {
        let dir = std::env::temp_dir().join(format!("arcz-lic-{}", std::process::id()));
        let cache = TileCache::new(&dir).unwrap();
        let bbox = GeoBBox::around(Geodetic::new(-46.63, -23.55, 0.0), 1000.0).unwrap();

        // Nao chega a fazer rede: a checagem de licenca vem antes.
        let e = fetch_image_mosaic(&cache, ImagerySource::EoxS2Cloudless, &bbox, 12, false)
            .await
            .unwrap_err();
        assert!(matches!(e, TerrainError::LicencaNaoAceita { .. }), "{e:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod tests_cor_media {
    use super::*;
    use arcz_geo::{TileId, TileRange};

    /// Mosaico 8x8 de um tile so, com uma cor por pixel definida pelo caller.
    fn mosaico(pintar: impl Fn(u32, u32) -> [u8; 3]) -> (ImageMosaic, f64, f64) {
        let range = TileRange { z: 1, x_min: 0, x_max: 0, y_min: 0, y_max: 0 };
        let ts = 8u32;
        let mut rgba = Vec::with_capacity((ts * ts * 4) as usize);
        for y in 0..ts {
            for x in 0..ts {
                let c = pintar(x, y);
                rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
        let m = ImageMosaic::from_raw(range, ts, rgba);
        let centro = TileId::new(1, 0, 0).pixel_to_geodetic(0.5, 0.5);
        (m, centro.lon_deg, centro.lat_deg)
    }

    const TELHA: [u8; 3] = [180, 90, 60];
    const COPA: [u8; 3] = [60, 120, 40];

    #[test]
    fn a_cor_media_de_um_lote_uniforme_e_a_propria_cor() {
        let (m, lon, lat) = mosaico(|_, _| TELHA);
        let c = m.cor_media(lon, lat, 1.0);
        assert!((c[0] - 180.0 / 255.0).abs() < 0.01, "R = {}", c[0]);
        assert!((c[1] - 90.0 / 255.0).abs() < 0.01, "G = {}", c[1]);
        assert!((c[2] - 60.0 / 255.0).abs() < 0.01, "B = {}", c[2]);
    }

    #[test]
    fn a_copa_de_arvore_nao_pinta_a_casa_de_verde() {
        // O defeito visto no render de Bombinhas: metade do disco de amostragem
        // caia numa arvore do lote e a casa inteira saia verde-mata.
        let (m, lon, lat) = mosaico(|x, _| if x % 2 == 0 { COPA } else { TELHA });
        let filtrada = m.cor_media(lon, lat, 1.0e6);
        let crua = m.cor_media_crua(lon, lat, 3);

        // Sem filtro o verde contamina (G sobe); com filtro sai telha pura.
        assert!(crua[1] > filtrada[1] + 0.05, "filtro nao mudou nada: crua G {} vs {}", crua[1], filtrada[1]);
        assert!((filtrada[0] - 180.0 / 255.0).abs() < 0.02, "R contaminado: {}", filtrada[0]);
    }

    #[test]
    fn lote_todo_vegetado_vira_cinza_neutro_e_nao_verde() {
        let (m, lon, lat) = mosaico(|_, _| COPA);
        let c = m.cor_media(lon, lat, 1.0e6);
        assert_eq!(c, [0.62, 0.60, 0.58]);
    }

    #[test]
    fn fora_do_mosaico_devolve_o_cinza_neutro() {
        let (m, _, _) = mosaico(|_, _| TELHA);
        // Lado oposto do planeta em relacao ao tile (0,0) de z=1.
        let c = m.cor_media(120.0, -60.0, 5.0);
        assert_eq!(c, [0.62, 0.60, 0.58]);
    }

    #[test]
    fn o_classificador_de_vegetacao_separa_verde_de_todo_o_resto() {
        assert!(e_vegetacao(60.0, 120.0, 40.0), "copa de arvore");
        assert!(e_vegetacao(90.0, 140.0, 70.0), "grama clara");
        assert!(!e_vegetacao(180.0, 90.0, 60.0), "telha ceramica");
        assert!(!e_vegetacao(200.0, 200.0, 200.0), "laje clara");
        assert!(!e_vegetacao(60.0, 60.0, 60.0), "asfalto");
        assert!(!e_vegetacao(220.0, 210.0, 170.0), "areia");
        // Sombra quase preta: G domina proporcionalmente mas e escuro demais
        // para afirmar vegetacao — o piso >24 evita descartar sombra de telhado.
        assert!(!e_vegetacao(10.0, 20.0, 5.0), "sombra escura");
    }
}

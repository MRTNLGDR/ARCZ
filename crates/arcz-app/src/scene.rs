//! Carregamento da cena: da configuracao ate a malha pronta para a GPU.

use std::time::Instant;

use arcz_geo::{EnuFrame, GeoBBox, Geodetic};
use arcz_model::{Model, PlacedModel};
use arcz_terrain::mosaic::{fetch_height_mosaic, fetch_image_mosaic};
use arcz_terrain::{HeightMosaic, ImageMosaic, TerrainMesh, TileCache};

use crate::config::Config;

pub struct Scene {
    pub frame: EnuFrame,
    pub bbox: GeoBBox,
    pub mesh: TerrainMesh,
    pub imagery: ImageMosaic,
    /// Modelo do usuario, ja georreferenciado. `None` quando `--modelo` nao foi dado.
    pub modelo: Option<PlacedModel>,
    /// Geometria original do modelo, para o preview reposicionar sem reabrir o
    /// arquivo. Guardada so quando o servidor de preview vai subir.
    pub fonte_modelo: Option<arcz_model::FonteGeometria>,
    /// Placement efetivamente usado (ja resolvido a partir de CLI/KMZ/centro).
    pub placement: arcz_model::Placement,
    /// Altitude do terreno sob o modelo, em metros.
    pub solo_modelo_m: f64,
    pub relatorio: Relatorio,
    /// DEM carregado, para o `Renderer` amostrar a altitude sob objetos
    /// adicionados em runtime (biblioteca, arrasta-e-solta).
    pub dem: HeightMosaic,
    /// Lado do quadrado da area carregada, em metros (do `--lado`).
    pub lado_m: f64,
}

impl Scene {
    /// Amostra a altitude do terreno (em metros) na coordenada geodetica.
    /// Usado quando o usuario adiciona um objeto novo no meio da cena: precisa
    /// saber onde o terreno esta embaixo dele pra assentar a base.
    pub fn altura_no_terreno(&self, lon_deg: f64, lat_deg: f64) -> f64 {
        self.dem.sample_geodetic(lon_deg, lat_deg) as f64
    }
}

/// Dados do modelo importado, para o relatorio.
#[derive(Debug, Clone)]
pub struct RelatorioModelo {
    pub arquivo: String,
    pub triangulos: usize,
    pub tamanho_arquivo_m: [f32; 3],
    pub tamanho_real_m: [f32; 3],
    pub escala: f32,
    pub heading_deg: f64,
    pub lat: f64,
    pub lon: f64,
    pub altitude_base_m: f64,
    pub aviso_unidade: Option<&'static str>,
    /// Aviso quando o georreferenciamento do KMZ diverge da area carregada.
    pub aviso_kmz: Option<String>,
    pub primitivas_ignoradas: usize,
    pub materiais: usize,
    pub submeshes: usize,
    pub texturas: usize,
    pub mb_de_textura: f64,
}

/// O que realmente aconteceu no carregamento. Impresso no console e usado nos testes.
#[derive(Debug, Clone)]
pub struct Relatorio {
    pub tiles_dem: u64,
    pub tiles_imagery: u64,
    pub dem_ausentes: u32,
    pub imagery_ausentes: u32,
    pub zoom_dem: u8,
    pub zoom_imagery: u8,
    /// Altitude no ponto central pedido. E o numero conferivel contra fonte externa.
    pub altitude_centro_m: f32,
    /// Extremos **dentro da area pedida** (nao do mosaico inteiro de tiles).
    pub altura_min_m: f32,
    pub altura_max_m: f32,
    /// Pixels de batimetria elevados ao nivel do mar.
    pub pixels_achatados: u32,
    /// Aviso quando a imagery nao tem resolucao para a area pedida.
    pub aviso_imagery: Option<String>,
    pub vertices: usize,
    pub triangulos: usize,
    pub extensao_horizontal_m: f32,
    /// Maior |coordenada| de vertice, em metros. Prova o quao longe da origem a
    /// cena chega — e portanto quanta precisao de `f32` sobra.
    pub maior_coordenada_m: f32,
    /// Resolucao efetiva do `f32` nessa magnitude, em metros.
    pub ulp_m: f64,
    pub segundos: f64,
    pub atribuicoes: Vec<String>,
    pub modelo: Option<RelatorioModelo>,
}

impl Relatorio {
    pub fn imprimir(&self) {
        println!("--- ARCZ / Fatia 0 -------------------------------------------");
        println!(
            "DEM       z{:<3} {:>4} tiles ({} ausentes)",
            self.zoom_dem, self.tiles_dem, self.dem_ausentes
        );
        println!(
            "Imagery   z{:<3} {:>4} tiles ({} ausentes)",
            self.zoom_imagery, self.tiles_imagery, self.imagery_ausentes
        );
        println!(
            "Relevo    {:.1} m .. {:.1} m  (desnivel {:.1} m) | centro a {:.1} m",
            self.altura_min_m,
            self.altura_max_m,
            self.altura_max_m - self.altura_min_m,
            self.altitude_centro_m
        );
        if let Some(a) = &self.aviso_imagery {
            println!("ATENCAO   {a}");
        }
        if self.pixels_achatados > 0 {
            println!(
                "Mar       {} pixels de batimetria elevados a 0 m (use --batimetria para manter)",
                self.pixels_achatados
            );
        }
        println!(
            "Malha     {} vertices, {} triangulos, {:.0} m de lado",
            self.vertices, self.triangulos, self.extensao_horizontal_m
        );
        println!(
            "Precisao  vertice mais distante {:.0} m da origem ENU; ulp do f32 = {:.4} mm",
            self.maior_coordenada_m,
            self.ulp_m * 1000.0
        );
        if let Some(m) = &self.modelo {
            println!("--- Modelo do usuario ----------------------------------------");
            println!("Arquivo   {}", m.arquivo);
            println!(
                "Geometria {} triangulos{}",
                m.triangulos,
                if m.primitivas_ignoradas > 0 {
                    format!(
                        " ({} primitivas nao-triangulares ignoradas)",
                        m.primitivas_ignoradas
                    )
                } else {
                    String::new()
                }
            );
            println!(
                "Arquivo   {:.2} x {:.2} x {:.2} unidades  (escala {})",
                m.tamanho_arquivo_m[0], m.tamanho_arquivo_m[1], m.tamanho_arquivo_m[2], m.escala
            );
            println!(
                "Real      {:.2} m (L) x {:.2} m (A) x {:.2} m (P)",
                m.tamanho_real_m[0], m.tamanho_real_m[1], m.tamanho_real_m[2]
            );
            println!(
                "Materiais {} materiais, {} submeshes, {} texturas ({:.1} MB em VRAM)",
                m.materiais, m.submeshes, m.texturas, m.mb_de_textura
            );
            println!(
                "Posicao   lat {:.6}  lon {:.6}  rumo {:.1}°  base a {:.1} m",
                m.lat, m.lon, m.heading_deg, m.altitude_base_m
            );
            if let Some(aviso) = m.aviso_unidade {
                println!("ATENCAO   {aviso}");
                println!(
                    "          Ajuste com --modelo-escala se a altura real acima estiver errada."
                );
            }
            if let Some(aviso) = &m.aviso_kmz {
                println!("ATENCAO   {aviso}");
            }
        }
        println!("Tempo     {:.2} s", self.segundos);
        for a in &self.atribuicoes {
            println!("Credito   {a}");
        }
        println!("--------------------------------------------------------------");
    }
}

pub async fn carregar(cfg: &Config) -> anyhow::Result<Scene> {
    let inicio = Instant::now();

    let bbox = cfg
        .bbox()
        .map_err(|e| anyhow::anyhow!("area invalida: {e}"))?;
    let centro = bbox.center();

    let cache = TileCache::new(TileCache::default_root())?;
    log::info!("cache de tiles em {}", cache.root().display());

    // DEM e imagery sao independentes: baixam em paralelo.
    let (dem, imagery) = tokio::try_join!(
        fetch_height_mosaic(&cache, cfg.dem, &bbox, cfg.zoom_dem),
        fetch_image_mosaic(
            &cache,
            cfg.imagery,
            &bbox,
            cfg.zoom_imagery,
            cfg.aceitar_licenca
        ),
    )?;

    let mut dem: HeightMosaic = dem;
    let mut pixels_achatados = 0;
    if !cfg.batimetria {
        pixels_achatados = dem.achatar_batimetria(0.0);
    }
    dem.escalar_alturas(cfg.exagero_vertical);

    // A origem do quadro fica no centro da area, na altura do terreno ali. Assim a
    // cena inteira fica simetrica em torno de zero e a camera nunca precisa rebasear
    // dentro de uma regiao de ate 32 km.
    let alt_centro = dem.sample_geodetic(centro.lon_deg, centro.lat_deg) as f64;
    let frame = EnuFrame::new(Geodetic::new(centro.lon_deg, centro.lat_deg, alt_centro));

    let mesh = arcz_terrain::mesh::build(&bbox, cfg.grid_n, &dem, &imagery, &frame); // Avisa quando a imagery nao tem resolucao para a area. Sem isso, o terreno
                                                                                     // chapado parece defeito de render — e so falta de pixel na fonte.
    let aviso_imagery = imagery.aviso_de_resolucao(&bbox, cfg.imagery);
    if let Some(a) = &aviso_imagery {
        log::warn!("{a}");
    }

    // Extremos medidos nos vertices da malha — ou seja, dentro da area pedida.
    // `dem.min_max()` cobriria o mosaico inteiro de tiles, que e bem maior que a
    // bbox e reportaria um desnivel que o usuario nunca ve na tela.
    let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
    let mut maior = 0.0_f32;
    for v in &mesh.vertices {
        lo = lo.min(v.position[1]);
        hi = hi.max(v.position[1]);
        for c in v.position {
            maior = maior.max(c.abs());
        }
    }
    // position[1] e altura relativa a origem do quadro; soma de volta para virar
    // altitude absoluta.
    let alt_centro_f32 = alt_centro as f32;
    let (lo, hi) = (lo + alt_centro_f32, hi + alt_centro_f32);

    // --- modelo do usuario ----------------------------------------------------
    let mut modelo = None;
    let mut rel_modelo = None;
    let mut aviso_kmz: Option<String> = None;
    let mut fonte_modelo = None;
    let mut placement_usado = cfg.modelo_placement;
    let mut solo_modelo = alt_centro;
    if let Some(caminho) = &cfg.modelo {
        let m = Model::load(caminho)?;

        // Precedencia: --modelo-lat/lon explicito > KMZ > centro da area.
        // O explicito ganha do KMZ de proposito: o KMZ pode estar geolocalizado
        // errado (foi o caso do Zenite) e o operador precisa poder sobrepor.
        let geo = match &cfg.modelo_kmz {
            Some(k) => Some(arcz_model::Georreferencia::load(k)?),
            None => None,
        };

        let mut p = cfg.modelo_placement;
        p.lat_deg = cfg
            .modelo_lat
            .or(geo.map(|g| g.lat_deg))
            .unwrap_or(centro.lat_deg);
        p.lon_deg = cfg
            .modelo_lon
            .or(geo.map(|g| g.lon_deg))
            .unwrap_or(centro.lon_deg);

        if let Some(g) = geo {
            // Rumo do KMZ so vale se o usuario nao passou --modelo-heading.
            if cfg.modelo_placement.heading_deg == 0.0 {
                p.heading_deg = g.heading_deg;
            }
            if let Some(aviso) = g.conferir_contra(centro.lat_deg, centro.lon_deg, 500.0) {
                log::warn!("{aviso}");
                aviso_kmz = Some(aviso);
            }
        }

        if !bbox.contains(Geodetic::new(p.lon_deg, p.lat_deg, 0.0)) {
            log::warn!(
                "o modelo esta em {:.5}, {:.5}, fora da area carregada — ele vai aparecer \
                 flutuando fora do terreno. Aumente --lado ou corrija --modelo-lat/lon.",
                p.lat_deg,
                p.lon_deg
            );
        }

        // Colhidos antes de `place`, que consome o modelo (evita clonar 130 MB).
        let tamanho_arquivo = m.size();
        let aviso_unidade = m.suspeita_de_unidade();
        let primitivas_ignoradas = m.primitivas_ignoradas;
        let materiais = m.materiais.len();
        let texturas = m.texturas.len();
        let bytes_textura = m.bytes_de_textura();

        // Altitude do terreno exatamente sob o modelo, nao no centro da cena.
        let solo = dem.sample_geodetic(p.lon_deg, p.lat_deg) as f64;
        solo_modelo = solo;
        placement_usado = p;
        // Vertices no espaco do arquivo: e o que vai para a GPU. A posicao no mundo
        // vem da matriz de modelo, entao mover o objeto nao toca na malha.
        fonte_modelo = Some(arcz_model::FonteGeometria::from_model(&m));
        let posto = arcz_model::place(m, &frame, &p, solo);

        rel_modelo = Some(RelatorioModelo {
            arquivo: caminho.display().to_string(),
            triangulos: posto.triangle_count(),
            tamanho_arquivo_m: tamanho_arquivo,
            tamanho_real_m: posto.tamanho_real_m,
            escala: p.escala,
            heading_deg: p.heading_deg,
            lat: p.lat_deg,
            lon: p.lon_deg,
            altitude_base_m: posto.altitude_base_m,
            aviso_unidade,
            aviso_kmz: aviso_kmz.take(),
            primitivas_ignoradas,
            materiais,
            submeshes: posto.submeshes.len(),
            texturas,
            mb_de_textura: bytes_textura as f64 / (1024.0 * 1024.0),
        });
        modelo = Some(posto);
    }

    let relatorio = Relatorio {
        tiles_dem: dem.range().count(),
        tiles_imagery: imagery.range().count(),
        dem_ausentes: dem.tiles_ausentes,
        imagery_ausentes: imagery.tiles_ausentes,
        zoom_dem: dem.range().z,
        zoom_imagery: imagery.range().z,
        altitude_centro_m: alt_centro_f32,
        altura_min_m: lo,
        altura_max_m: hi,
        pixels_achatados,
        aviso_imagery,
        vertices: mesh.vertices.len(),
        triangulos: mesh.triangle_count(),
        extensao_horizontal_m: mesh.horizontal_extent(),
        maior_coordenada_m: maior,
        ulp_m: (maior.max(1.0) as f64) * f32::EPSILON as f64,
        segundos: inicio.elapsed().as_secs_f64(),
        atribuicoes: vec![
            cfg.dem.atribuicao().to_string(),
            cfg.imagery.atribuicao().to_string(),
        ],
        modelo: rel_modelo,
    };

    Ok(Scene {
        frame,
        bbox,
        mesh,
        imagery,
        modelo,
        fonte_modelo,
        placement: placement_usado,
        solo_modelo_m: solo_modelo,
        relatorio,
        dem,
        lado_m: cfg.lado_m,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relatorio_imprime_sem_panicar() {
        let r = Relatorio {
            tiles_dem: 4,
            tiles_imagery: 1,
            dem_ausentes: 0,
            imagery_ausentes: 0,
            zoom_dem: 12,
            zoom_imagery: 8,
            altitude_centro_m: 760.0,
            altura_min_m: 0.0,
            altura_max_m: 812.0,
            pixels_achatados: 1234,
            aviso_imagery: None,
            vertices: 100,
            triangulos: 162,
            extensao_horizontal_m: 8000.0,
            maior_coordenada_m: 4000.0,
            ulp_m: 4000.0 * f32::EPSILON as f64,
            segundos: 1.5,
            atribuicoes: vec!["teste".into()],
            modelo: Some(RelatorioModelo {
                arquivo: "predio.glb".into(),
                triangulos: 2,
                tamanho_arquivo_m: [2000.0, 5000.0, 0.0],
                tamanho_real_m: [20.0, 50.0, 0.0],
                escala: 0.01,
                heading_deg: 37.0,
                lat: -23.55,
                lon: -46.63,
                altitude_base_m: 760.0,
                aviso_unidade: Some("teste de aviso"),
                aviso_kmz: Some("KMZ 65 km fora".into()),
                primitivas_ignoradas: 1,
                materiais: 39,
                submeshes: 38,
                texturas: 37,
                mb_de_textura: 512.5,
            }),
        };
        r.imprimir();
        // ulp em 4 km tem que ser submilimetrico — se nao for, a Fatia 0 falhou.
        assert!(r.ulp_m < 1e-3, "ulp de {} m", r.ulp_m);
    }
}

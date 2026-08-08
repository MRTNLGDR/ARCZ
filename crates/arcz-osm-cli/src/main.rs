//! CLI headless: gera o entorno OSM (predios, vias, superficies, vegetacao)
//! de uma area como `.glb`, sem GPU, sem janela, sem servidor persistente.
//!
//! Existe para o visualizador web (`servidor.py`) poder chamar a mesma
//! pipeline que ja roda em producao no editor nativo
//! (`arcz-app::renderer::carregar_entorno`, chamada pela rota `/entorno` do
//! `--serve`) sem precisar inicializar wgpu/winit/tiny_http/rusqlite so para
//! gerar uma malha que nunca toca a GPU. `arcz-osm`/`arcz-geo`/`arcz-terrain`
//! nao dependem de nada disso — daqui pra baixo e so CPU e rede.
//!
//! Mesma ordem de chamadas do `carregar_entorno`: busca no Overpass, recorta,
//! adensa (opcional), amostra a cor da propria ortofoto nao entra aqui (o CLI
//! nao carrega imagery — o Cesium ja mostra o satelite por cima), gera as
//! malhas contra o DEM real e exporta.
//!
//! Uso:
//! ```text
//! arcz-osm-cli --lat -27.1545 --lon -48.5022 --lado 150 \
//!     --saida entorno.glb --cache-dem cache_dem --cache-overpass cache_overpass \
//!     [--adensar]
//! ```
//!
//! Sucesso: escreve `--saida` e imprime **uma linha** de JSON no stdout com
//! as contagens (predios_osm, predios_gerados, vias, superficies, arvores,
//! malhas, triangulos, atribuicao) — mesmo formato do `RelatorioEntorno` que
//! a rota `/entorno` ja devolve. Logs vao para stderr, entao o stdout fica
//! limpo para quem so quer a ultima linha. Falha: nada no stdout, mensagem em
//! stderr, saida com codigo != 0.

use std::path::PathBuf;

use arcz_geo::{Enu, EnuFrame, GeoBBox, Geodetic};
use arcz_osm::{
    malha, procedural, Camadas, ClienteOverpass, Opcoes, PontoGeo, RegrasUrbanas, Terreno,
    TerrenoPlano,
};
use arcz_terrain::{DemSource, HeightMosaic, TileCache};

/// Zoom do DEM usado pelo app nativo (`crates/arcz-app/src/config.rs`,
/// `Config::default().zoom_dem`). Mantido igual para nao perder detalhe de
/// terreno em relacao ao visualizador nativo.
const ZOOM_DEM: u8 = 14;

struct Args {
    lat: f64,
    lon: f64,
    lado: f64,
    adensar: bool,
    saida: PathBuf,
    cache_dem: PathBuf,
    cache_overpass: PathBuf,
}

fn ajuda() -> ! {
    eprintln!(
        "uso: arcz-osm-cli --lat N --lon N --lado METROS --saida ARQUIVO.glb \
         --cache-dem DIR --cache-overpass DIR [--adensar]"
    );
    std::process::exit(2);
}

fn parse_args() -> Args {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut lat: Option<f64> = None;
    let mut lon: Option<f64> = None;
    let mut lado: Option<f64> = None;
    let mut adensar = false;
    let mut saida: Option<PathBuf> = None;
    let mut cache_dem: Option<PathBuf> = None;
    let mut cache_overpass: Option<PathBuf> = None;

    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "--lat" => {
                i += 1;
                lat = Some(
                    argv.get(i)
                        .unwrap_or_else(|| ajuda())
                        .parse()
                        .unwrap_or_else(|_| ajuda()),
                );
            }
            "--lon" => {
                i += 1;
                lon = Some(
                    argv.get(i)
                        .unwrap_or_else(|| ajuda())
                        .parse()
                        .unwrap_or_else(|_| ajuda()),
                );
            }
            "--lado" => {
                i += 1;
                lado = Some(
                    argv.get(i)
                        .unwrap_or_else(|| ajuda())
                        .parse()
                        .unwrap_or_else(|_| ajuda()),
                );
            }
            "--adensar" => adensar = true,
            "--saida" => {
                i += 1;
                saida = Some(PathBuf::from(
                    argv.get(i).unwrap_or_else(|| ajuda()).as_str(),
                ));
            }
            "--cache-dem" => {
                i += 1;
                cache_dem = Some(PathBuf::from(
                    argv.get(i).unwrap_or_else(|| ajuda()).as_str(),
                ));
            }
            "--cache-overpass" => {
                i += 1;
                cache_overpass = Some(PathBuf::from(
                    argv.get(i).unwrap_or_else(|| ajuda()).as_str(),
                ));
            }
            "-h" | "--help" => ajuda(),
            outro => {
                eprintln!("flag desconhecida: {outro}");
                ajuda();
            }
        }
        i += 1;
    }

    Args {
        lat: lat.unwrap_or_else(|| ajuda()),
        lon: lon.unwrap_or_else(|| ajuda()),
        lado: lado.unwrap_or_else(|| ajuda()),
        adensar,
        saida: saida.unwrap_or_else(|| ajuda()),
        cache_dem: cache_dem.unwrap_or_else(|| ajuda()),
        cache_overpass: cache_overpass.unwrap_or_else(|| ajuda()),
    }
}

/// Centro geografico de um contorno, como `(lat, lon)`.
///
/// Copiado de `crates/arcz-app/src/renderer.rs::centro_geo` — funcao pura de
/// poucas linhas, nao vale expor so por isto no `arcz-osm`.
fn centro_geo(contorno: &[PontoGeo]) -> Option<(f64, f64)> {
    if contorno.is_empty() {
        return None;
    }
    let n = contorno.len() as f64;
    Some((
        contorno.iter().map(|p| p.lat).sum::<f64>() / n,
        contorno.iter().map(|p| p.lon).sum::<f64>() / n,
    ))
}

/// Adapta um `HeightMosaic` (Terrarium) para a trait `Terreno` do `arcz-osm`.
///
/// Mesma logica de `TerrenoDoDem` em `crates/arcz-app/src/renderer.rs`, so
/// que amostrando um mosaico proprio em vez do `Scene` do editor (o CLI nao
/// tem editor nem GPU).
struct TerrenoDoMosaico<'a> {
    mosaico: &'a HeightMosaic,
    frame: &'a EnuFrame,
}

impl Terreno for TerrenoDoMosaico<'_> {
    fn altura(&self, leste: f64, norte: f64) -> f64 {
        let g = self.frame.enu_to_geodetic(Enu::new(leste, norte, 0.0));
        self.mosaico.sample_geodetic(g.lon_deg, g.lat_deg) as f64
    }
}

async fn carregar_dem(cache_dir: &std::path::Path, bbox: &GeoBBox) -> anyhow::Result<HeightMosaic> {
    let cache = TileCache::new(cache_dir)?;
    let mosaico =
        arcz_terrain::mosaic::fetch_height_mosaic(&cache, DemSource::PADRAO, bbox, ZOOM_DEM)
            .await?;
    Ok(mosaico)
}

async fn executar() -> anyhow::Result<()> {
    let args = parse_args();

    let centro = Geodetic::new(args.lon, args.lat, 0.0);
    let bbox = GeoBBox::around(centro, args.lado)?;
    let frame = EnuFrame::new(centro);

    log::info!(
        "bbox {:.5},{:.5} .. {:.5},{:.5}  (lado {} m)",
        bbox.west,
        bbox.south,
        bbox.east,
        bbox.north,
        args.lado
    );

    // Aponta para um diretorio persistente (nao um tempdir) para o cache
    // sha256-da-query que a propria crate ja faz valer entre execucoes do
    // CLI, nao so dentro de um processo.
    let (mut entorno, origem) = ClienteOverpass::novo(&args.cache_overpass)
        .buscar(&bbox, Camadas::default())
        .await?;
    log::info!("origem OSM: {origem:?} — {}", entorno.resumo());

    let predios_osm = entorno.edificios.len();

    // Recorta ANTES de adensar: o Overpass devolve toda via que toca a bbox,
    // e adensar primeiro loteia a rua alem do terreno — mesma ordem de
    // `carregar_entorno` em crates/arcz-app/src/renderer.rs.
    procedural::recortar(&mut entorno, &frame, args.lado * 0.5);

    let predios_gerados = if args.adensar {
        procedural::adensar(&mut entorno, &frame, RegrasUrbanas::default());
        // Via truncada guarda um ponto alem da divisa (pra rua chegar ate
        // ela); o loteamento ainda pinga sinteticos do lado de fora. Predio
        // real (id >= 0) fica mesmo na borda; o sintetico, que e palpite, e
        // barato de descartar.
        let m = args.lado * 0.5;
        entorno.edificios.retain(|ed| {
            if ed.id >= 0 {
                return true;
            }
            centro_geo(&ed.contorno).is_some_and(|(lat, lon)| {
                let e = frame.geodetic_to_enu(Geodetic::new(lon, lat, 0.0));
                e.e.abs() <= m && e.n.abs() <= m
            })
        });
        entorno.edificios.iter().filter(|e| e.id < 0).count()
    } else {
        0
    };

    // Altura real do terreno. Se o DEM falhar (rede fora, S3 indisponivel),
    // cai pra terreno plano em vez de abortar — a qualidade "nivel Ion" vem
    // das tags OSM (footprint/altura/telhado), nao do relevo por baixo.
    let dem = match carregar_dem(&args.cache_dem, &bbox).await {
        Ok(mosaico) => Some(mosaico),
        Err(e) => {
            log::warn!("DEM indisponivel, usando terreno plano: {e:#}");
            None
        }
    };

    let malhas = match &dem {
        Some(mosaico) => {
            let terreno = TerrenoDoMosaico {
                mosaico,
                frame: &frame,
            };
            malha::gerar(&entorno, &frame, &terreno, Opcoes::default())
        }
        None => malha::gerar(&entorno, &frame, &TerrenoPlano(0.0), Opcoes::default()),
    };
    let triangulos: usize = malhas.iter().map(|m| m.triangulos()).sum();

    let glb = arcz_osm::exportar_glb(&malhas);
    if let Some(pai) = args.saida.parent() {
        if !pai.as_os_str().is_empty() {
            std::fs::create_dir_all(pai)?;
        }
    }
    std::fs::write(&args.saida, &glb)?;

    log::info!(
        "gravado {} ({} bytes): {predios_osm} predios OSM + {predios_gerados} gerados, \
         {} malhas, {triangulos} triangulos",
        args.saida.display(),
        glb.len(),
        malhas.len()
    );

    // Uma linha, no stdout, para o `servidor.py` parsear direto — logs acima
    // foram todos para stderr.
    let relatorio = serde_json::json!({
        "predios_osm": predios_osm,
        "predios_gerados": predios_gerados,
        "vias": entorno.vias.len(),
        "superficies": entorno.superficies.len(),
        "arvores": entorno.arvores.len(),
        "malhas": malhas.len(),
        "triangulos": triangulos,
        "atribuicao": arcz_osm::ATRIBUICAO,
    });
    println!("{relatorio}");
    Ok(())
}

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    if let Err(e) = executar().await {
        eprintln!("arcz-osm-cli: {e:#}");
        std::process::exit(1);
    }
}

//! Aquisicao de terreno: baixa DEM e imagery de fontes publicas, monta mosaicos e
//! gera a malha ja no quadro ENU local.
//!
//! Nenhuma fonte usada por padrao exige chave de API. As que exigem, ou que tem
//! licenca restritiva, estao declaradas explicitamente em [`source::License`] e o
//! chamador precisa aceitar a classe de licenca antes de usar.

pub mod cache;
pub mod mesh;

pub mod mosaic;
pub mod quantized;
pub mod source;

pub use cache::TileCache;
pub use mesh::{TerrainMesh, TerrainVertex};
pub use mosaic::{HeightMosaic, ImageMosaic};
pub use source::{DemSource, ImagerySource, License};

#[derive(Debug, thiserror::Error)]
pub enum TerrainError {
    #[error("tile {0} nao existe na fonte (HTTP 404)")]
    TileAusente(String),
    #[error("HTTP {status} ao buscar {url}")]
    Http { url: String, status: u16 },
    #[error("erro de rede: {0}")]
    Rede(#[from] reqwest::Error),
    #[error("erro de I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("falha ao decodificar imagem: {0}")]
    Imagem(#[from] image::ImageError),
    #[error("tile {id} tem {largura}x{altura}, esperado quadrado de lado potencia de dois")]
    DimensaoInvalida {
        id: String,
        largura: u32,
        altura: u32,
    },
    #[error("tiles da mesma faixa vieram com tamanhos diferentes: {0} e {1}")]
    TamanhoInconsistente(u32, u32),
    #[error("a fonte '{fonte}' e {licenca:?}; passe `aceitar_licenca` para usa-la")]
    LicencaNaoAceita { fonte: String, licenca: License },
    #[error("faixa de {0} tiles excede o limite de {1} — reduza a area ou o zoom")]
    FaixaGrandeDemais(u64, u64),
}

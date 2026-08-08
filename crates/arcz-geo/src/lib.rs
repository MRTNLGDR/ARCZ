//! Base geodesica do ARCZ.
//!
//! Regra inegociavel do projeto: **todo dado geoespacial vive em `f64`**. A conversao
//! para `f32` acontece uma unica vez, no ultimo instante, e somente depois que as
//! coordenadas ja foram rebaixadas para um quadro local (ENU) com origem proxima.
//!
//! O motivo esta documentado e testado em [`enu::tests::f32_em_ecef_perde_precisao_metrica`]:
//! uma coordenada ECEF tipica (~6.4e6 m) perde cerca de meio metro ao ser truncada para
//! `f32`, o que na GPU aparece como vertices tremendo e geometria rasgando. A mesma
//! coordenada expressa em ENU local cabe em `f32` com erro submilimetrico.

pub mod bbox;
pub mod enu;
pub mod sol;
pub mod tiles;
pub mod wgs84;

pub use bbox::GeoBBox;
pub use enu::{Enu, EnuFrame};
pub use sol::{posicao as posicao_solar, InstanteUtc, PosicaoSolar};
pub use tiles::{TileId, TileRange};
pub use wgs84::{Ecef, Geodetic};

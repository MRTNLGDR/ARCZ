//! Entorno procedural do ARCZ a partir do OpenStreetMap.
//!
//! Pega uma bbox, busca os dados do OSM e devolve predios extrudados, faixas de
//! rua, superficies de uso do solo e vegetacao — tudo no quadro ENU do projeto,
//! na escala real, encostado no terreno.
//!
//! ```no_run
//! # async fn exemplo() -> anyhow::Result<()> {
//! use arcz_geo::{EnuFrame, GeoBBox, Geodetic};
//! use arcz_osm::{malha, Camadas, ClienteOverpass, Opcoes, TerrenoPlano};
//!
//! let centro = Geodetic::new(-48.5022653, -27.1544967, 0.0);
//! let bbox = GeoBBox::around(centro, 600.0)?;
//! let frame = EnuFrame::new(centro);
//!
//! let cliente = ClienteOverpass::novo("cache/osm");
//! let (entorno, _origem) = cliente.buscar(&bbox, Camadas::default()).await?;
//! let malhas = malha::gerar(&entorno, &frame, &TerrenoPlano(0.0), Opcoes::default());
//! # Ok(())
//! # }
//! ```
//!
//! ## Proveniencia e licenca
//!
//! Os dados vem do **OpenStreetMap**, sob **ODbL 1.0**. Consequencias praticas:
//!
//! - Toda imagem publicada precisa creditar [`overpass::ATRIBUICAO`].
//! - Redistribuir a **base derivada** (os arquivos de geometria) aciona o
//!   share-alike da ODbL. Renderizacoes sao *produced works* e nao acionam.
//! - O codigo deste crate e do ARCZ e nao herda a ODbL; so os dados herdam.
//!
//! Ver `docs/open-source/OPEN_SOURCE_CATALOG.md`.

pub mod entidade;
pub mod especies;
pub mod gltf_saida;
pub mod malha;
pub mod overpass;
pub mod procedural;
pub mod triangulacao;

pub use entidade::{
    altura_do_edificio, largura_da_via, metros_da_tag, Arvore, ClasseEdificio, ClasseSuperficie,
    ClasseVia, Edificio, Entorno, FonteAltura, PontoGeo, Superficie, Tags, Telhado, Via,
};
pub use gltf_saida::exportar_glb;
pub use especies::{Bioma, Especie, Instancia};
pub use malha::{gerar_edificio, MalhaProcedural, Opcoes, Terreno, TerrenoPlano, Vertice};
pub use procedural::{adensar, gerar_edificacoes, RegrasUrbanas};
pub use overpass::{
    area_km2, interpretar, montar_consulta, Camadas, ClienteOverpass, Origem, OsmError, ATRIBUICAO,
};

/// Registro de proveniencia exigido pelo protocolo de open source do projeto.
pub const PROVENIENCIA: Proveniencia = Proveniencia {
    fonte: "OpenStreetMap via Overpass API",
    licenca: "ODbL 1.0",
    atribuicao_obrigatoria: true,
    share_alike_em_dados: true,
    url: "https://www.openstreetmap.org/copyright",
};

#[derive(Debug, Clone, Copy)]
pub struct Proveniencia {
    pub fonte: &'static str,
    pub licenca: &'static str,
    pub atribuicao_obrigatoria: bool,
    /// A ODbL torna share-alike qualquer **base de dados** derivada que seja
    /// redistribuida. Imagens renderizadas nao sao afetadas.
    pub share_alike_em_dados: bool,
    pub url: &'static str,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_proveniencia_declara_a_odbl() {
        // O prompt mestre proibe ignorar licencas. Como `PROVENIENCIA` e const, a
        // checagem cabe em tempo de compilacao: apagar a obrigacao de atribuicao
        // numa refatoracao passa a nao compilar, em vez de falhar um teste que
        // alguem poderia marcar como `ignore`.
        const _: () = assert!(PROVENIENCIA.atribuicao_obrigatoria);
        const _: () = assert!(PROVENIENCIA.share_alike_em_dados);
        assert_eq!(PROVENIENCIA.licenca, "ODbL 1.0");
        assert!(ATRIBUICAO.contains("OpenStreetMap"));
    }
}

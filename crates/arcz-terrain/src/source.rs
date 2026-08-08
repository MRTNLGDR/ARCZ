//! Fontes publicas de DEM e imagery, com a licenca declarada no tipo.
//!
//! O ARCZ e destinado a uso comercial, entao licenca nao e nota de rodape: e um campo
//! obrigatorio. Uma fonte [`License::NaoComercial`] ou [`License::Restritiva`] so pode
//! ser usada se o chamador aceitar explicitamente (ver `TerrainError::LicencaNaoAceita`).

use arcz_geo::TileId;

/// Classe de licenca de uma fonte de dados.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum License {
    /// Dominio publico ou equivalente. Uso comercial livre.
    DominioPublico,
    /// Uso comercial livre, exigindo credito visivel.
    AtribuicaoObrigatoria,
    /// Proibido uso comercial (ex.: CC-BY-NC-SA).
    NaoComercial,
    /// Termos de servico proprietarios que restringem armazenar/derivar dados.
    Restritiva,
}

impl License {
    /// `true` se a fonte pode entrar num produto comercial sem negociacao.
    pub fn comercialmente_segura(self) -> bool {
        matches!(self, Self::DominioPublico | Self::AtribuicaoObrigatoria)
    }
}

/// Fonte de modelo digital de elevacao.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemSource {
    /// AWS Terrain Tiles (`elevation-tiles-prod`), codificacao *terrarium*.
    ///
    /// Mosaico global de SRTM / GMTED2010 / NED / dados nacionais, servido como PNG
    /// sem chave de API. ~30 m onde ha SRTM. E a unica fonte de DEM global,
    /// comercialmente utilizavel e sem cadastro que existe hoje — por isso e o padrao.
    AwsTerrarium,
}

impl DemSource {
    pub const PADRAO: Self = Self::AwsTerrarium;

    pub fn nome(self) -> &'static str {
        match self {
            Self::AwsTerrarium => "AWS Terrain Tiles (terrarium)",
        }
    }

    pub fn license(self) -> License {
        match self {
            Self::AwsTerrarium => License::AtribuicaoObrigatoria,
        }
    }

    pub fn atribuicao(self) -> &'static str {
        match self {
            Self::AwsTerrarium => {
                "Elevation data: Mapzen/AWS Terrain Tiles (SRTM, GMTED2010, NED e outros)"
            }
        }
    }

    /// Maior zoom com dado real. Acima disso a fonte apenas reamostra.
    pub fn zoom_maximo(self) -> u8 {
        match self {
            Self::AwsTerrarium => 15,
        }
    }

    pub fn url(self, id: TileId) -> String {
        match self {
            Self::AwsTerrarium => format!(
                "https://s3.amazonaws.com/elevation-tiles-prod/terrarium/{}/{}/{}.png",
                id.z, id.x, id.y
            ),
        }
    }
}

/// Fonte de imagem aerea/satelite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImagerySource {
    /// NASA GIBS — Blue Marble com relevo sombreado. Dominio publico (NASA).
    ///
    /// Resolucao baixa (500 m, ate z=8). E o padrao porque e a unica opcao sem chave
    /// **e** sem restricao comercial. Serve para validar o pipeline; nao serve para
    /// render final.
    NasaBlueMarble,
    /// EOX Sentinel-2 cloudless 2020. Resolucao 10 m, ate z=14.
    ///
    /// **CC-BY-NC-SA 4.0 — proibido uso comercial.** A EOX vende uma licenca comercial
    /// separada. Use so para desenvolvimento e avaliacao.
    EoxS2Cloudless,
    /// Esri World Imagery. Alta resolucao (ate ~z=19 em areas urbanas).
    ///
    /// **Termos de servico da Esri restringem uso fora de aplicacoes Esri.**
    /// Disponivel aqui para inspecao visual durante o desenvolvimento.
    EsriWorldImagery,
}

impl ImagerySource {
    /// Padrao seguro: a unica fonte que nao impede uso comercial.
    pub const PADRAO: Self = Self::NasaBlueMarble;

    pub fn nome(self) -> &'static str {
        match self {
            Self::NasaBlueMarble => "NASA GIBS BlueMarble_ShadedRelief_Bathymetry",
            Self::EoxS2Cloudless => "EOX Sentinel-2 cloudless 2020",
            Self::EsriWorldImagery => "Esri World Imagery",
        }
    }

    pub fn license(self) -> License {
        match self {
            Self::NasaBlueMarble => License::DominioPublico,
            Self::EoxS2Cloudless => License::NaoComercial,
            Self::EsriWorldImagery => License::Restritiva,
        }
    }

    pub fn atribuicao(self) -> &'static str {
        match self {
            Self::NasaBlueMarble => "Imagery: NASA EOSDIS GIBS / Blue Marble",
            Self::EoxS2Cloudless => {
                "Sentinel-2 cloudless 2020 by EOX IT Services GmbH (Copernicus Sentinel data 2020)"
            }
            Self::EsriWorldImagery => "Imagery: Esri, Maxar, Earthstar Geographics",
        }
    }

    pub fn zoom_maximo(self) -> u8 {
        match self {
            Self::NasaBlueMarble => 8,
            Self::EoxS2Cloudless => 14,
            Self::EsriWorldImagery => 19,
        }
    }

    pub fn url(self, id: TileId) -> String {
        match self {
            Self::NasaBlueMarble => format!(
                "https://gibs.earthdata.nasa.gov/wmts/epsg3857/best/\
                 BlueMarble_ShadedRelief_Bathymetry/default/default/\
                 GoogleMapsCompatible_Level8/{}/{}/{}.jpeg",
                id.z, id.y, id.x
            ),
            Self::EoxS2Cloudless => format!(
                "https://tiles.maps.eox.at/wmts/1.0.0/s2cloudless-2020_3857/default/\
                 GoogleMapsCompatible/{}/{}/{}.jpg",
                id.z, id.y, id.x
            ),
            Self::EsriWorldImagery => format!(
                "https://services.arcgisonline.com/ArcGIS/rest/services/World_Imagery/\
                 MapServer/tile/{}/{}/{}",
                id.z, id.y, id.x
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_usam_a_ordem_de_eixo_certa() {
        let id = TileId::new(7, 40, 65);

        // XYZ: .../{z}/{x}/{y}.png
        assert!(DemSource::AwsTerrarium.url(id).ends_with("/7/40/65.png"));

        // WMTS e Esri: .../{z}/{y}/{x} — trocar isso da imagem espelhada e e o erro
        // mais comum ao adicionar uma fonte nova.
        assert!(ImagerySource::NasaBlueMarble
            .url(id)
            .ends_with("/7/65/40.jpeg"));
        assert!(ImagerySource::EoxS2Cloudless
            .url(id)
            .ends_with("/7/65/40.jpg"));
        assert!(ImagerySource::EsriWorldImagery
            .url(id)
            .ends_with("/7/65/40"));
    }

    #[test]
    fn urls_nao_tem_espaco_de_quebra_de_linha() {
        // As strings sao escritas com continuacao `\` no fonte; um espaco vazado ali
        // vira URL invalida em runtime.
        let id = TileId::new(3, 1, 2);
        for u in [
            DemSource::AwsTerrarium.url(id),
            ImagerySource::NasaBlueMarble.url(id),
            ImagerySource::EoxS2Cloudless.url(id),
            ImagerySource::EsriWorldImagery.url(id),
        ] {
            assert!(!u.contains(' '), "URL com espaco: {u}");
            assert!(u.starts_with("https://"), "URL sem esquema: {u}");
        }
    }

    #[test]
    fn os_padroes_sao_comercialmente_seguros() {
        assert!(DemSource::PADRAO.license().comercialmente_segura());
        assert!(ImagerySource::PADRAO.license().comercialmente_segura());
    }

    #[test]
    fn fontes_de_alta_resolucao_estao_marcadas_como_nao_seguras() {
        // Guarda contra alguem "promover" essas fontes a padrao sem tratar a licenca.
        assert!(!ImagerySource::EoxS2Cloudless
            .license()
            .comercialmente_segura());
        assert!(!ImagerySource::EsriWorldImagery
            .license()
            .comercialmente_segura());
    }
}

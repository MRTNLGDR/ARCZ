//! Leitura do georreferenciamento de arquivos KML/KMZ exportados do SketchUp.
//!
//! Quando o SketchUp exporta um KMZ, ele grava a posicao e o rumo exatos com que o
//! modelo esta geolocalizado:
//!
//! ```xml
//! <Model>
//!   <Location><latitude>..</latitude><longitude>..</longitude><altitude>..</altitude></Location>
//!   <Orientation><heading>..</heading><tilt>..</tilt><roll>..</roll></Orientation>
//!   <Scale><x>1</x><y>1</y><z>1</z></Scale>
//! </Model>
//! ```
//!
//! Isso substitui o alinhamento no olho: em vez de arrastar o modelo comparando com a
//! ortofoto, o proprio arquivo diz onde ele fica. **Vale o que o SketchUp tiver
//! configurado** — se o modelo foi geolocalizado no lugar errado, o KMZ repete o erro
//! fielmente. Por isso [`Georreferencia::conferir_contra`] existe.

use std::io::Read;
use std::path::Path;

/// Posicao e rumo lidos de um KML/KMZ.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Georreferencia {
    pub lat_deg: f64,
    pub lon_deg: f64,
    pub alt_m: f64,
    /// Rumo horario a partir do norte, em graus.
    pub heading_deg: f64,
    pub tilt_deg: f64,
    pub roll_deg: f64,
    pub escala: [f64; 3],
}

#[derive(Debug, thiserror::Error)]
pub enum KmzError {
    #[error("falha ao ler o arquivo: {0}")]
    Io(#[from] std::io::Error),
    #[error("KMZ invalido: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("o KMZ nao contem nenhum .kml")]
    SemKml,
    #[error("o KML nao tem <Location> com <latitude> e <longitude>")]
    SemLocation,
    #[error("valor nao numerico em <{0}>: {1:?}")]
    ValorInvalido(&'static str, String),
}

impl Georreferencia {
    /// Le `.kmz` (ZIP com um .kml dentro) ou `.kml` direto.
    pub fn load(caminho: impl AsRef<Path>) -> Result<Self, KmzError> {
        let caminho = caminho.as_ref();
        let ehkmz = caminho
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("kmz"));

        let xml = if ehkmz {
            extrair_kml_do_kmz(caminho)?
        } else {
            std::fs::read_to_string(caminho)?
        };
        Self::from_kml(&xml)
    }

    pub fn from_kml(xml: &str) -> Result<Self, KmzError> {
        // O KML pode ter varios <LookAt> (camera das cenas) antes do <Model>. O que
        // interessa e o <Location> dentro de <Model>; buscar "latitude" solto pegaria
        // a camera do tour e posicionaria o predio no lugar errado.
        let bloco = recortar(xml, "<Model", "</Model>").unwrap_or(xml);
        let location = recortar(bloco, "<Location", "</Location>").ok_or(KmzError::SemLocation)?;

        let lat = numero(location, "latitude")?.ok_or(KmzError::SemLocation)?;
        let lon = numero(location, "longitude")?.ok_or(KmzError::SemLocation)?;
        let alt = numero(location, "altitude")?.unwrap_or(0.0);

        let orientacao = recortar(bloco, "<Orientation", "</Orientation>").unwrap_or("");
        let escala_txt = recortar(bloco, "<Scale", "</Scale>").unwrap_or("");

        Ok(Self {
            lat_deg: lat,
            lon_deg: lon,
            alt_m: alt,
            heading_deg: numero(orientacao, "heading")?.unwrap_or(0.0),
            tilt_deg: numero(orientacao, "tilt")?.unwrap_or(0.0),
            roll_deg: numero(orientacao, "roll")?.unwrap_or(0.0),
            escala: [
                numero(escala_txt, "x")?.unwrap_or(1.0),
                numero(escala_txt, "y")?.unwrap_or(1.0),
                numero(escala_txt, "z")?.unwrap_or(1.0),
            ],
        })
    }

    /// Distancia horizontal aproximada, em metros, ate outra coordenada.
    ///
    /// Serve para flagrar o caso real que aconteceu no Zenite: o KMZ apontava para
    /// Gaspar, a 65 km de Bombinhas, porque o modelo foi geolocalizado no lugar
    /// errado dentro do SketchUp.
    pub fn distancia_ate(&self, lat_deg: f64, lon_deg: f64) -> f64 {
        const M_POR_GRAU: f64 = 111_132.0;
        let dlat = (self.lat_deg - lat_deg) * M_POR_GRAU;
        let dlon = (self.lon_deg - lon_deg) * M_POR_GRAU * self.lat_deg.to_radians().cos();
        (dlat * dlat + dlon * dlon).sqrt()
    }

    /// Devolve um aviso se o georreferenciamento estiver longe da coordenada esperada.
    pub fn conferir_contra(&self, lat_deg: f64, lon_deg: f64, tolerancia_m: f64) -> Option<String> {
        let d = self.distancia_ate(lat_deg, lon_deg);
        (d > tolerancia_m).then(|| {
            format!(
                "o KMZ geolocaliza o modelo em {:.6}, {:.6} — {:.0} m ({:.1} km) da coordenada \
                 esperada {lat_deg:.6}, {lon_deg:.6}. Provavelmente o modelo foi geolocalizado \
                 no lugar errado no SketchUp.",
                self.lat_deg,
                self.lon_deg,
                d,
                d / 1000.0
            )
        })
    }
}

fn extrair_kml_do_kmz(caminho: &Path) -> Result<String, KmzError> {
    let arquivo = std::fs::File::open(caminho)?;
    let mut zip = zip::ZipArchive::new(arquivo)?;

    // `doc.kml` e o nome canonico, mas qualquer .kml na raiz serve.
    let indice = (0..zip.len()).find(|&i| {
        zip.by_index(i)
            .ok()
            .map(|e| e.name().to_ascii_lowercase().ends_with(".kml"))
            .unwrap_or(false)
    });

    let mut entrada = zip.by_index(indice.ok_or(KmzError::SemKml)?)?;
    let mut texto = String::new();
    entrada.read_to_string(&mut texto)?;
    Ok(texto)
}

/// Recorta o trecho entre a abertura de `inicio` e o fechamento `fim`.
fn recortar<'a>(texto: &'a str, inicio: &str, fim: &str) -> Option<&'a str> {
    let a = texto.find(inicio)?;
    let resto = &texto[a..];
    let b = resto.find(fim)?;
    Some(&resto[..b])
}

/// Le o conteudo numerico de `<tag>...</tag>`.
///
/// Parser deliberadamente minimo: KML de SketchUp nao usa atributos nem namespace
/// nessas tags, e uma dependencia de XML completo nao se paga aqui.
fn numero(texto: &str, tag: &'static str) -> Result<Option<f64>, KmzError> {
    let abre = format!("<{tag}>");
    let fecha = format!("</{tag}>");
    let Some(a) = texto.find(&abre) else {
        return Ok(None);
    };
    let resto = &texto[a + abre.len()..];
    let Some(b) = resto.find(&fecha) else {
        return Ok(None);
    };
    let bruto = resto[..b].trim();
    bruto
        .parse::<f64>()
        .map(Some)
        .map_err(|_| KmzError::ValorInvalido(tag, bruto.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// O doc.kml real do "ARQ-AP-Zenit Bombinhas - Leao Marinho.kmz", reduzido.
    /// Repare no <LookAt> antes do <Model>: e a armadilha que este parser evita.
    const KML_ZENITE: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="no" ?>
<kml xmlns="http://www.opengis.net/kml/2.2"><Folder><name>ARQ-AP-Zenit Bombinhas</name>
<LookAt><heading>76.270918191028045</heading><tilt>70.666375361662276</tilt>
<latitude>-26.983000030858463</latitude><longitude>-49.099703302266988</longitude>
<range>60.000000000010566</range><altitude>25.764131470101233</altitude></LookAt>
<Placemark><name>Model</name><Model><altitudeMode>absolute</altitudeMode>
<Location><latitude>-26.983333333333334</latitude><longitude>-49.100000000000001</longitude>
<altitude>0</altitude></Location>
<Orientation><heading>59.981632572390822</heading><tilt>0</tilt><roll>0</roll></Orientation>
<Scale><x>1</x><y>1</y><z>1</z></Scale>
<Link><href>models/untitled.dae</href></Link></Model></Placemark></Folder></kml>"#;

    #[test]
    fn le_o_georreferenciamento_do_zenite() {
        let g = Georreferencia::from_kml(KML_ZENITE).unwrap();

        assert!(
            (g.lat_deg + 26.983_333_333).abs() < 1e-9,
            "lat = {}",
            g.lat_deg
        );
        assert!((g.lon_deg + 49.1).abs() < 1e-9, "lon = {}", g.lon_deg);
        assert_eq!(g.alt_m, 0.0);
        assert!((g.heading_deg - 59.981_632_572).abs() < 1e-9);
        assert_eq!(g.tilt_deg, 0.0);
        assert_eq!(g.roll_deg, 0.0);
        assert_eq!(g.escala, [1.0, 1.0, 1.0]);
    }

    /// A regressao mais perigosa: pegar a camera do tour em vez da posicao do modelo.
    /// As duas ficam a ~40 m uma da outra, o que passaria despercebido na tela.
    #[test]
    fn ignora_o_lookat_e_usa_o_location_do_model() {
        let g = Georreferencia::from_kml(KML_ZENITE).unwrap();
        // Latitude e rumo do <LookAt>, que NAO podem ter sido usados.
        let lat_lookat: f64 = "-26.983000030858463".parse().unwrap();
        let heading_lookat: f64 = "76.270918191028045".parse().unwrap();
        assert_ne!(
            g.lat_deg, lat_lookat,
            "pegou a latitude do <LookAt> em vez da do <Model>"
        );
        assert_ne!(g.heading_deg, heading_lookat);
    }

    #[test]
    fn detecta_o_georreferenciamento_errado_do_arquivo_real() {
        let g = Georreferencia::from_kml(KML_ZENITE).unwrap();
        // Bombinhas, segundo o Google Maps.
        let aviso = g.conferir_contra(-27.154_496_7, -48.502_265_3, 200.0);
        let aviso = aviso.expect("deveria acusar os ~65 km de erro");
        assert!(aviso.contains("km"), "{aviso}");

        let d = g.distancia_ate(-27.154_496_7, -48.502_265_3);
        assert!(
            (60_000.0..75_000.0).contains(&d),
            "distancia ate Bombinhas: {d} m"
        );
    }

    #[test]
    fn nao_avisa_quando_o_georreferenciamento_esta_certo() {
        let g = Georreferencia::from_kml(KML_ZENITE).unwrap();
        assert!(g.conferir_contra(-26.983_333_333, -49.1, 200.0).is_none());
    }

    #[test]
    fn campos_opcionais_caem_no_padrao() {
        let kml = "<Model><Location><latitude>-10.5</latitude><longitude>-40.25</longitude>\
                   </Location></Model>";
        let g = Georreferencia::from_kml(kml).unwrap();
        assert_eq!((g.lat_deg, g.lon_deg), (-10.5, -40.25));
        assert_eq!(g.alt_m, 0.0);
        assert_eq!(g.heading_deg, 0.0);
        assert_eq!(g.escala, [1.0, 1.0, 1.0]);
    }

    #[test]
    fn le_escala_diferente_de_um() {
        let kml = "<Model><Location><latitude>1</latitude><longitude>2</longitude></Location>\
                   <Scale><x>2.5</x><y>2.5</y><z>2.5</z></Scale></Model>";
        assert_eq!(
            Georreferencia::from_kml(kml).unwrap().escala,
            [2.5, 2.5, 2.5]
        );
    }

    #[test]
    fn sem_location_e_erro_explicito() {
        let e = Georreferencia::from_kml("<kml><Folder/></kml>").unwrap_err();
        assert!(matches!(e, KmzError::SemLocation), "{e:?}");
    }

    #[test]
    fn valor_nao_numerico_e_erro_explicito() {
        let kml = "<Model><Location><latitude>abc</latitude><longitude>2</longitude>\
                   </Location></Model>";
        let e = Georreferencia::from_kml(kml).unwrap_err();
        assert!(matches!(e, KmzError::ValorInvalido("latitude", _)), "{e:?}");
    }

    #[test]
    fn distancia_bate_com_deslocamento_conhecido() {
        let g = Georreferencia::from_kml(
            "<Model><Location><latitude>0</latitude><longitude>0</longitude></Location></Model>",
        )
        .unwrap();
        // 1 grau de latitude ~ 111.132 km.
        let d = g.distancia_ate(1.0, 0.0);
        assert!((d - 111_132.0).abs() < 10.0, "{d}");
    }
}

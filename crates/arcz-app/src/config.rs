//! Configuracao da regiao a carregar, vinda da linha de comando.

use std::path::PathBuf;

use arcz_geo::{GeoBBox, Geodetic};
use arcz_model::Placement;
use arcz_terrain::{DemSource, ImagerySource};

/// Politica financeira obrigatoria do ARCZ: local-first, offline-first e gratuito por padrao.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoliticaCustos {
    pub paid_services_enabled: bool,
    pub cloud_processing_enabled: bool,
    pub automatic_api_purchase: bool,
    pub automatic_subscription: bool,
    pub automatic_model_purchase: bool,
    pub maximum_unapproved_cost_cents: u64,
}

// Escrito a mao de proposito, e nao derivado: cada campo aqui e uma decisao de
// nao gastar o dinheiro do usuario, e precisa ficar legivel na revisao. Derivar
// tambem faria qualquer campo novo cair em `false`/`0` silenciosamente, o que
// para uma politica de custos e o padrao certo — mas por acidente, nao por
// escolha registrada.
#[allow(clippy::derivable_impls)]
impl Default for PoliticaCustos {
    fn default() -> Self {
        Self {
            paid_services_enabled: false,
            cloud_processing_enabled: false,
            automatic_api_purchase: false,
            automatic_subscription: false,
            automatic_model_purchase: false,
            maximum_unapproved_cost_cents: 0,
        }
    }
}

impl PoliticaCustos {
    pub fn e_totalmente_gratuita(&self) -> bool {
        !self.paid_services_enabled
            && !self.cloud_processing_enabled
            && !self.automatic_api_purchase
            && !self.automatic_subscription
            && !self.automatic_model_purchase
            && self.maximum_unapproved_cost_cents == 0
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub centro: Geodetic,
    pub lado_m: f64,
    pub politica_custos: PoliticaCustos,
    /// Modelo 3D do usuario (.gltf/.glb) a inserir na cena.
    pub modelo: Option<PathBuf>,
    /// Coordenada do modelo. `None` = centro da area carregada.
    pub modelo_lat: Option<f64>,
    pub modelo_lon: Option<f64>,
    /// KMZ/KML de onde tirar lat/lon/rumo do modelo, em vez de digitar.
    pub modelo_kmz: Option<PathBuf>,
    /// Rumo, escala e assentamento. Lat/lon aqui sao preenchidos em `scene::carregar`.
    pub modelo_placement: Placement,
    pub zoom_dem: u8,
    pub zoom_imagery: u8,
    pub grid_n: u32,
    pub dem: DemSource,
    pub imagery: ImagerySource,
    pub aceitar_licenca: bool,
    pub headless: bool,
    pub exagero_vertical: f32,
    /// Mantem a batimetria (profundidade do mar) do DEM em vez de achatar no zero.
    pub batimetria: bool,
    /// Grava um PNG da cena em vez de (ou alem de) abrir a janela.
    pub png: Option<PathBuf>,
    pub png_largura: u32,
    pub png_altura: u32,
    /// Sobrescritas da camera. `None` = enquadramento automatico.
    pub cam_pitch_deg: Option<f64>,
    pub cam_yaw_deg: Option<f64>,
    pub cam_dist_m: Option<f64>,
    /// Sobe o servidor de preview ao vivo nesta porta.
    pub serve: Option<u16>,
    /// Operacao de catalogo de projetos; encerra sem abrir a cena.
    pub comando_projeto: Option<crate::workspace::ComandoProjeto>,
    /// Projeto ao qual a sessao esta ligada; habilita autosave no preview.
    pub projeto_slug: Option<String>,
    /// Pasta com `.glb`/`.gltf` para popular a cena. Cada item vira um objeto
    /// no `Editor`. Se setado, tem precedencia sobre `--modelo` (que era 1 unico).
    pub biblioteca: Option<PathBuf>,
    /// Abre um projeto `.arcz` salvo (substitui o carregamento padrao de DEM/imagery).
    pub abrir: Option<PathBuf>,
    /// Salva o estado atual num projeto `.arcz` e sai (sem abrir janela).
    pub salvar: Option<PathBuf>,
    /// Empreendimento (`.json`) com as plantas mobiliadas a instanciar na cena.
    pub mobiliar: Option<PathBuf>,
    /// Raiz da biblioteca de moveis usada por `--mobiliar`.
    pub biblioteca_raiz: PathBuf,
    /// Restringe `--mobiliar` a um pavimento (nome exato). Util para caber na
    /// memoria: o predio inteiro sao ~1.800 moveis.
    pub mobiliar_pavimento: Option<String>,
    /// Teto de moveis instanciados por `--mobiliar`. 0 = sem limite.
    pub mobiliar_limite: usize,
    /// Altura do plano de corte da VISTA, em metros acima da base do modelo.
    /// `None` = sem corte. Nao altera a geometria: e um corte de camera.
    pub corte_m: Option<f32>,
    /// Espessura da linha escura logo abaixo do corte, em metros.
    pub corte_linha_m: f32,
    /// Estilo de desenho da vista.
    pub estilo: Estilo,
}

/// Como a vista e desenhada. Nao muda a cena, so o shader.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Estilo {
    /// Iluminacao solar e ceu atmosferico.
    #[default]
    Foto,
    /// Planta humanizada: luz zenital, cor chapada, contorno e fundo branco.
    Planta,
    /// Sketch: branco com traco preto.
    Sketch,
}

impl Estilo {
    pub fn de_texto(s: &str) -> Option<Self> {
        match s {
            "foto" => Some(Self::Foto),
            "planta" | "humanizada" => Some(Self::Planta),
            "sketch" | "linha" => Some(Self::Sketch),
            _ => None,
        }
    }

    /// Valor que vai para `Globais::vista.z`.
    pub fn codigo(self) -> f32 {
        match self {
            Self::Foto => 0.0,
            Self::Planta => 1.0,
            Self::Sketch => 2.0,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            // Centro padrao: Zenite by Salinet, Bombinhas/SC — o local real do
            // projeto. Coordenada tirada do Google Maps.
            //
            // ATENCAO: o KMZ exportado do SketchUp aponta para -26.983333,
            // -49.100000 (regiao de Gaspar/Blumenau), ~65 km daqui. A geolocalizacao
            // do arquivo .skp esta errada; a referencia correta e esta.
            centro: Geodetic::new(-48.502_265_3, -27.154_496_7, 0.0),
            lado_m: 3_000.0,
            politica_custos: PoliticaCustos::default(),
            modelo: None,
            modelo_lat: None,
            modelo_lon: None,
            modelo_kmz: None,
            modelo_placement: Placement::default(),
            zoom_dem: 14,
            zoom_imagery: 8,
            grid_n: 513,
            dem: DemSource::PADRAO,
            imagery: ImagerySource::PADRAO,
            aceitar_licenca: false,
            headless: false,
            exagero_vertical: 1.0,
            batimetria: false,
            png: None,
            png_largura: 1600,
            png_altura: 900,
            cam_pitch_deg: None,
            cam_yaw_deg: None,
            cam_dist_m: None,
            serve: None,
            comando_projeto: None,
            projeto_slug: None,
            biblioteca: None,
            abrir: None,
            salvar: None,
            mobiliar: None,
            biblioteca_raiz: PathBuf::from("biblioteca"),
            mobiliar_pavimento: None,
            mobiliar_limite: 0,
            corte_m: None,
            corte_linha_m: 0.06,
            estilo: Estilo::Foto,
        }
    }
}

impl Config {
    /// Empacota corte + estilo no formato que o shader espera (`Globais::vista`).
    pub fn vista(&self) -> [f32; 4] {
        [
            self.corte_m.unwrap_or(0.0),
            if self.corte_m.is_some() { 1.0 } else { 0.0 },
            self.estilo.codigo(),
            self.corte_linha_m.max(0.001),
        ]
    }

    pub fn bbox(&self) -> Result<GeoBBox, arcz_geo::bbox::BBoxError> {
        GeoBBox::around(self.centro, self.lado_m)
    }

    /// Parser minimo de argumentos. Sem dependencia de crate de CLI enquanto a
    /// superficie for esta — trocar por `clap` quando passar de ~10 flags.
    pub fn from_args<I: IntoIterator<Item = String>>(args: I) -> Result<Self, String> {
        let mut cfg = Config::default();
        let args: Vec<String> = args.into_iter().skip(1).collect();
        let mut i = 0;

        while i < args.len() {
            let arg = args[i].as_str();
            // Le o proximo argumento como valor da flag atual.
            let valor = |i: &mut usize| -> Result<String, String> {
                *i += 1;
                args.get(*i)
                    .cloned()
                    .ok_or_else(|| format!("faltou o valor de {}", args[*i - 1]))
            };

            match arg {
                "--lat" => cfg.centro.lat_deg = num(&valor(&mut i)?)?,
                "--lon" => cfg.centro.lon_deg = num(&valor(&mut i)?)?,
                "--lado" => cfg.lado_m = num(&valor(&mut i)?)?,
                "--zoom-dem" => cfg.zoom_dem = num(&valor(&mut i)?)?,
                "--zoom-img" => cfg.zoom_imagery = num(&valor(&mut i)?)?,
                "--grid" => cfg.grid_n = num(&valor(&mut i)?)?,
                "--exagero" => cfg.exagero_vertical = num(&valor(&mut i)?)?,
                "--headless" => cfg.headless = true,
                "--batimetria" => cfg.batimetria = true,
                "--aceito-licenca" => cfg.aceitar_licenca = true,
                "--serve" => cfg.serve = Some(num(&valor(&mut i)?)?),
                "--projeto" => cfg.projeto_slug = Some(valor(&mut i)?),
                "--projetos" => {
                    cfg.comando_projeto = Some(crate::workspace::ComandoProjeto::Listar)
                }
                "--lixeira" => {
                    cfg.comando_projeto = Some(crate::workspace::ComandoProjeto::Lixeira)
                }
                "--excluir-projeto" => {
                    cfg.comando_projeto =
                        Some(crate::workspace::ComandoProjeto::Excluir(valor(&mut i)?))
                }
                "--restaurar-projeto" => {
                    cfg.comando_projeto =
                        Some(crate::workspace::ComandoProjeto::Restaurar(valor(&mut i)?))
                }
                "--novo" => {
                    cfg.comando_projeto =
                        Some(crate::workspace::ComandoProjeto::Novo(valor(&mut i)?))
                }
                "--duplicar-projeto" => {
                    let a = valor(&mut i)?;
                    cfg.comando_projeto = Some(crate::workspace::ComandoProjeto::Duplicar(
                        a,
                        valor(&mut i)?,
                    ))
                }
                "--renomear-projeto" => {
                    let a = valor(&mut i)?;
                    cfg.comando_projeto = Some(crate::workspace::ComandoProjeto::Renomear(
                        a,
                        valor(&mut i)?,
                    ))
                }
                "--snapshot" => {
                    let a = valor(&mut i)?;
                    cfg.comando_projeto = Some(crate::workspace::ComandoProjeto::Snapshot(
                        a,
                        valor(&mut i)?,
                    ))
                }
                "--restaurar-snapshot" => {
                    let a = valor(&mut i)?;
                    cfg.comando_projeto = Some(crate::workspace::ComandoProjeto::RestaurarSnapshot(
                        a,
                        valor(&mut i)?,
                    ))
                }
                "--recuperar" => {
                    cfg.comando_projeto =
                        Some(crate::workspace::ComandoProjeto::Recuperar(valor(&mut i)?))
                }
                "--esvaziar-lixeira" => {
                    cfg.comando_projeto = Some(crate::workspace::ComandoProjeto::EsvaziarLixeira)
                }
                "--snapshots" => {
                    cfg.comando_projeto =
                        Some(crate::workspace::ComandoProjeto::Snapshots(valor(&mut i)?))
                }
                "--biblioteca" => cfg.biblioteca = Some(PathBuf::from(valor(&mut i)?)),
                "--mobiliar" => cfg.mobiliar = Some(PathBuf::from(valor(&mut i)?)),
                "--biblioteca-raiz" => cfg.biblioteca_raiz = PathBuf::from(valor(&mut i)?),
                "--mobiliar-pavimento" => cfg.mobiliar_pavimento = Some(valor(&mut i)?),
                "--mobiliar-limite" => cfg.mobiliar_limite = num(&valor(&mut i)?)?,
                "--corte" => cfg.corte_m = Some(num(&valor(&mut i)?)?),
                "--corte-linha" => cfg.corte_linha_m = num(&valor(&mut i)?)?,
                "--estilo" => {
                    let v = valor(&mut i)?;
                    cfg.estilo = Estilo::de_texto(&v).ok_or_else(|| {
                        format!("--estilo aceita foto, planta ou sketch (veio '{v}')")
                    })?;
                }
                "--abrir" => cfg.abrir = Some(PathBuf::from(valor(&mut i)?)),
                "--salvar" => cfg.salvar = Some(PathBuf::from(valor(&mut i)?)),
                "--cam-pitch" => cfg.cam_pitch_deg = Some(num(&valor(&mut i)?)?),
                "--cam-yaw" => cfg.cam_yaw_deg = Some(num(&valor(&mut i)?)?),
                "--cam-dist" => cfg.cam_dist_m = Some(num(&valor(&mut i)?)?),
                "--png" => cfg.png = Some(PathBuf::from(valor(&mut i)?)),
                "--png-largura" => cfg.png_largura = num(&valor(&mut i)?)?,
                "--png-altura" => cfg.png_altura = num(&valor(&mut i)?)?,
                "--modelo" => cfg.modelo = Some(PathBuf::from(valor(&mut i)?)),
                "--modelo-kmz" => cfg.modelo_kmz = Some(PathBuf::from(valor(&mut i)?)),
                "--modelo-lat" => cfg.modelo_lat = Some(num(&valor(&mut i)?)?),
                "--modelo-lon" => cfg.modelo_lon = Some(num(&valor(&mut i)?)?),
                "--modelo-heading" => cfg.modelo_placement.heading_deg = num(&valor(&mut i)?)?,
                "--modelo-escala" => cfg.modelo_placement.escala = num(&valor(&mut i)?)?,
                "--modelo-offset" => cfg.modelo_placement.offset_vertical_m = num(&valor(&mut i)?)?,
                "--modelo-leste" => cfg.modelo_placement.offset_leste_m = num(&valor(&mut i)?)?,
                "--modelo-norte" => cfg.modelo_placement.offset_norte_m = num(&valor(&mut i)?)?,
                "--modelo-nao-assentar" => cfg.modelo_placement.assentar_no_terreno = false,
                "--imagery" => {
                    let v = valor(&mut i)?;
                    cfg.imagery = match v.as_str() {
                        "gibs" | "bluemarble" => ImagerySource::NasaBlueMarble,
                        "s2" | "s2cloudless" => ImagerySource::EoxS2Cloudless,
                        "esri" => ImagerySource::EsriWorldImagery,
                        outro => return Err(format!("fonte de imagery desconhecida: {outro}")),
                    };
                }
                "-h" | "--help" => return Err(AJUDA.to_string()),
                outro => return Err(format!("argumento desconhecido: {outro}\n\n{AJUDA}")),
            }
            i += 1;
        }

        if cfg.lado_m <= 0.0 {
            return Err("--lado tem que ser positivo".into());
        }
        if cfg.grid_n < 2 {
            return Err("--grid tem que ser >= 2".into());
        }
        if cfg.modelo_placement.escala <= 0.0 || !cfg.modelo_placement.escala.is_finite() {
            return Err("--modelo-escala tem que ser positivo".into());
        }
        if cfg.modelo.is_none() && (cfg.modelo_lat.is_some() || cfg.modelo_lon.is_some()) {
            return Err("--modelo-lat/--modelo-lon sem --modelo nao fazem nada".into());
        }
        if cfg.png_largura < 16 || cfg.png_altura < 16 {
            return Err("--png-largura/--png-altura minimos: 16".into());
        }
        Ok(cfg)
    }
}

fn num<T: std::str::FromStr>(s: &str) -> Result<T, String> {
    s.parse()
        .map_err(|_| format!("valor numerico invalido: {s}"))
}

pub const AJUDA: &str = "\
arcz — carrega uma regiao real (DEM + imagery) e exibe o terreno

USO:
    arcz [OPCOES]

OPCOES:
    --lat <graus>       latitude do centro         (padrao: -27.1544967, Bombinhas)
    --lon <graus>       longitude do centro        (padrao: -48.5022653, Bombinhas)
    --lado <metros>     lado da area quadrada      (padrao: 3000)
    --zoom-dem <z>      zoom dos tiles de DEM      (padrao: 14, max 15)
    --zoom-img <z>      zoom dos tiles de imagery  (padrao: 8)
    --grid <n>          vertices por eixo          (padrao: 513)
    --exagero <f>       exagero vertical do relevo (padrao: 1.0)
    --batimetria        mantem a profundidade do mar (padrao: achata no nivel 0)
    --imagery <fonte>   gibs | s2 | esri           (padrao: gibs)
    --aceito-licenca    libera fontes nao-comerciais/restritivas
    --headless          so baixa e valida, sem abrir janela
    --png <arquivo>     grava um PNG da cena (nao precisa de janela)
    --png-largura <px>  padrao 1600
    --png-altura <px>   padrao 900
    --serve <porta>     preview ao vivo no navegador (ex: --serve 8080)

GESTAO DE PROJETOS:
    --projeto <slug>              liga a sessao a um projeto (ativa autosave)
    --projetos                    lista os projetos salvos
    --snapshots <slug>            lista os snapshots de um projeto
    --lixeira                     lista o que foi excluido (recuperavel)
    --excluir-projeto <slug>      manda para a lixeira (NAO apaga)
    --restaurar-projeto <slug>    traz de volta da lixeira
    --novo <nome>                 cria projeto com a regiao atual (--lat/--lon/--lado)
    --duplicar-projeto <s> <nome> copia sob outro nome
    --renomear-projeto <s> <nome> renomeia
    --snapshot <slug> <rotulo>    grava um snapshot do estado salvo
    --restaurar-snapshot <s> <carimbo>  volta para um snapshot (reversivel)
    --recuperar <slug>            promove o autosave apos uma queda
    --esvaziar-lixeira            APAGA de vez o que esta na lixeira
    --biblioteca <pasta> varre uma pasta com .glb/.gltf e adiciona cada um como objeto
                     (precedencia sobre --modelo: use uma ou outra)
    --mobiliar <json>   instancia as plantas mobiliadas do empreendimento na cena
    --biblioteca-raiz <pasta>     onde estao os moveis (padrao: biblioteca)
    --mobiliar-pavimento <nome>   so este pavimento (ex.: Pavimento tipo 4)
    --mobiliar-limite <n>         teto de moveis instanciados (0 = sem limite)
    --corte <metros>    corta a VISTA acima desta altura (o modelo fica intacto)
    --corte-linha <m>   espessura do traco escuro no plano de corte (padrao 0,06)
    --estilo <e>        foto (padrao) | planta (humanizada) | sketch (linha)
    --abrir <arcz>      abre um projeto salvo (substitui centro/lado/zoom/hora do DEM)
    --salvar <arcz>     salva o estado atual em um projeto e sai (sem abrir janela)
    --cam-pitch <graus> elevacao da camera (90 = vista de cima / nadir)
    --cam-yaw <graus>   rotacao da camera
    --cam-dist <metros> distancia ao alvo
    -h, --help          mostra esta ajuda

MODELO 3D (.gltf / .glb):
    --modelo <arquivo>        insere o modelo na cena, em escala real
    --modelo-kmz <arquivo>    le lat/lon/rumo de um .kmz ou .kml do SketchUp
    --modelo-lat <graus>      onde ancorar          (padrao: centro da area)
    --modelo-lon <graus>      onde ancorar          (padrao: centro da area)
    --modelo-heading <graus>  rumo horario do norte (padrao: 0)
    --modelo-escala <f>       1.0 = arquivo em metros; use 0.01 se estiver em cm
    --modelo-offset <metros>  desloca na vertical apos assentar (negativo enterra)
    --modelo-leste <metros>   ajuste fino para leste  (negativo = oeste)
    --modelo-norte <metros>   ajuste fino para norte  (negativo = sul)
    --modelo-nao-assentar     mantem a altitude do arquivo em vez de pousar no terreno

    O modelo e ancorado pelo CENTRO DA PLANTA, na base. A escala nao e adivinhada:
    o app avisa se as dimensoes parecerem centimetros, mas quem decide e voce.

LICENCAS:
    gibs  NASA GIBS Blue Marble        dominio publico, ate z=8  (padrao seguro)
    s2    EOX Sentinel-2 cloudless     CC-BY-NC-SA: NAO COMERCIAL, ate z=14
    esri  Esri World Imagery           termos restritivos, ate z=19
";

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Config, String> {
        let mut v = vec!["arcz".to_string()];
        v.extend(args.iter().map(|s| s.to_string()));
        Config::from_args(v)
    }

    #[test]
    fn padrao_e_valido_e_comercialmente_seguro() {
        let c = Config::default();
        assert!(c.bbox().is_ok());
        assert!(c.imagery.license().comercialmente_segura());
        assert!(c.dem.license().comercialmente_segura());
        assert!(!c.aceitar_licenca);
    }

    #[test]
    fn le_coordenadas_e_area() {
        let c = parse(&["--lat", "-25.4289", "--lon", "-49.2717", "--lado", "4000"]).unwrap();
        assert!((c.centro.lat_deg + 25.4289).abs() < 1e-9);
        assert!((c.centro.lon_deg + 49.2717).abs() < 1e-9);
        assert!((c.lado_m - 4000.0).abs() < 1e-9);
    }

    #[test]
    fn seleciona_fonte_de_imagery() {
        assert_eq!(
            parse(&["--imagery", "esri"]).unwrap().imagery,
            ImagerySource::EsriWorldImagery
        );
        assert_eq!(
            parse(&["--imagery", "s2"]).unwrap().imagery,
            ImagerySource::EoxS2Cloudless
        );
        assert!(parse(&["--imagery", "bing"]).is_err());
    }

    #[test]
    fn rejeita_entradas_invalidas_em_vez_de_seguir() {
        assert!(parse(&["--lado", "0"]).is_err());
        assert!(parse(&["--grid", "1"]).is_err());
        assert!(parse(&["--lat"]).is_err(), "flag sem valor deveria falhar");
        assert!(parse(&["--turbo"]).is_err());
        assert!(parse(&["--lat", "abc"]).is_err());
    }

    #[test]
    fn help_devolve_o_texto_de_ajuda() {
        let e = parse(&["--help"]).unwrap_err();
        assert!(e.contains("USO:"), "{e}");
        for flag in ["--serve", "--projeto", "--restaurar-projeto"] {
            assert!(
                e.lines().any(|linha| linha.trim_start().starts_with(flag)),
                "{flag} deve comecar em sua propria linha:\n{e}"
            );
        }
    }

    #[test]
    fn le_abrir_e_salvar() {
        let c = parse(&["--abrir", "proj.arcz", "--salvar", "out.arcz"]).unwrap();
        assert_eq!(c.abrir.as_deref(), Some(std::path::Path::new("proj.arcz")));
        assert_eq!(c.salvar.as_deref(), Some(std::path::Path::new("out.arcz")));
    }

    #[test]
    fn le_a_biblioteca() {
        let c = parse(&["--biblioteca", "./moveis"]).unwrap();
        assert_eq!(
            c.biblioteca.as_deref(),
            Some(std::path::Path::new("./moveis"))
        );
    }

    #[test]
    fn biblioteca_e_opcional() {
        let c = parse(&[]).unwrap();
        assert!(c.biblioteca.is_none());
    }

    #[test]
    fn le_os_parametros_do_modelo() {
        let c = parse(&[
            "--modelo",
            "predio.glb",
            "--modelo-lat",
            "-23.55",
            "--modelo-lon",
            "-46.63",
            "--modelo-heading",
            "37.5",
            "--modelo-escala",
            "0.01",
            "--modelo-offset",
            "-2",
        ])
        .unwrap();

        assert_eq!(
            c.modelo.as_deref(),
            Some(std::path::Path::new("predio.glb"))
        );
        assert!((c.modelo_lat.unwrap() + 23.55).abs() < 1e-9);
        assert!((c.modelo_lon.unwrap() + 46.63).abs() < 1e-9);
        assert!((c.modelo_placement.heading_deg - 37.5).abs() < 1e-9);
        assert!((c.modelo_placement.escala - 0.01).abs() < 1e-9);
        assert!((c.modelo_placement.offset_vertical_m + 2.0).abs() < 1e-9);
        assert!(c.modelo_placement.assentar_no_terreno);
    }

    #[test]
    fn configura_a_saida_em_png() {
        let c = parse(&[
            "--png",
            "saida.png",
            "--png-largura",
            "3840",
            "--png-altura",
            "2160",
        ])
        .unwrap();
        assert_eq!(c.png.as_deref(), Some(std::path::Path::new("saida.png")));
        assert_eq!((c.png_largura, c.png_altura), (3840, 2160));

        assert!(parse(&["--png", "a.png", "--png-largura", "4"]).is_err());
    }

    #[test]
    fn sem_modelo_a_cena_e_so_terreno() {
        let c = parse(&[]).unwrap();
        assert!(c.modelo.is_none());
    }

    #[test]
    fn rejeita_parametros_de_modelo_incoerentes() {
        // Escala invalida colapsaria ou explodiria a geometria silenciosamente.
        assert!(parse(&["--modelo", "a.glb", "--modelo-escala", "0"]).is_err());
        assert!(parse(&["--modelo", "a.glb", "--modelo-escala", "-1"]).is_err());
        // Coordenada de modelo sem modelo e quase sempre erro de digitacao.
        assert!(parse(&["--modelo-lat", "-23.5"]).is_err());
    }

    #[test]
    fn nao_assentar_e_opt_in() {
        assert!(
            parse(&["--modelo", "a.glb"])
                .unwrap()
                .modelo_placement
                .assentar_no_terreno
        );
        assert!(
            !parse(&["--modelo", "a.glb", "--modelo-nao-assentar"])
                .unwrap()
                .modelo_placement
                .assentar_no_terreno
        );
    }

    #[test]
    fn licenca_so_e_aceita_explicitamente() {
        assert!(!parse(&["--imagery", "s2"]).unwrap().aceitar_licenca);
        assert!(
            parse(&["--imagery", "s2", "--aceito-licenca"])
                .unwrap()
                .aceitar_licenca
        );
    }

    #[test]
    fn nenhuma_operacao_padrao_pode_realizar_chamada_paga() {
        let c = Config::default();
        assert!(c.politica_custos.e_totalmente_gratuita());
        assert!(!c.politica_custos.paid_services_enabled);
        assert!(!c.politica_custos.cloud_processing_enabled);
        assert!(!c.politica_custos.automatic_api_purchase);
        assert!(!c.politica_custos.automatic_subscription);
        assert!(!c.politica_custos.automatic_model_purchase);
        assert_eq!(c.politica_custos.maximum_unapproved_cost_cents, 0);
    }
}

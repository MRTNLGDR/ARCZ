//! Formato de projeto: o que persiste entre sessoes.
//!
//! Guarda **referencias** aos arquivos de modelo mais a transformacao de cada objeto,
//! nunca a geometria. Um `.arcz` de uma cena com o Zenite e cem moveis tem alguns KB;
//! embutir as malhas o levaria a centenas de MB e tornaria salvar inviavel.
//!
//! O campo `versao` existe desde o primeiro dia para que projetos antigos possam ser
//! migrados em vez de recusados.
//!
//! **Estado:** o formato esta completo e coberto por 8 testes (ciclo salvar/abrir,
//! transformacao preservada, versao futura recusada, JSON corrompido, arquivos
//! ausentes). Ainda **nao esta ligado a UI** — faltam os botoes de salvar/abrir no
//! preview. O `allow` abaixo evita que o `clippy -D warnings` barre o build por
//! codigo ainda nao chamado.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use arcz_model::Placement;
use serde::{Deserialize, Serialize};

/// Versao do formato. Subir sempre que a estrutura mudar de forma incompativel.
///
/// Historico:
/// - 1: lancamento inicial. 8 testes.
/// - 2: campo `pai` no `ObjetoSalvo` (hierarquia pai/filho). 9 testes.
///   Migracao de projetos v1: campo ausente vira `None` (via serde default).
pub const VERSAO_FORMATO: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Projeto {
    pub versao: u32,
    pub nome: String,
    /// Regiao carregada.
    pub lat: f64,
    pub lon: f64,
    pub lado_m: f64,
    pub zoom_dem: u8,
    pub zoom_imagery: u8,
    /// Momento simulado do sol.
    pub mes: u32,
    pub dia: u32,
    pub hora: f64,
    pub objetos: Vec<ObjetoSalvo>,
    /// Camaras gravadas, no mesmo formato que o preview usa.
    #[serde(default)]
    pub cameras: Vec<serde_json::Value>,
}

/// Um objeto da cena, como referencia + transformacao.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObjetoSalvo {
    pub id: u32,
    pub nome: String,
    /// `Some(id_pai)` se faz parte de um grupo. Projetos v1 (sem o campo) sao
    /// lidos com `None` automaticamente — `#[serde(default)]` faz isso.
    #[serde(default)]
    pub pai: Option<u32>,
    /// Caminho do arquivo de origem. Relativo a raiz do projeto quando possivel.
    pub arquivo: PathBuf,
    pub visivel: bool,
    pub lat: f64,
    pub lon: f64,
    pub heading_deg: f64,
    pub escala: f32,
    pub offset_leste_m: f32,
    pub offset_norte_m: f32,
    pub offset_vertical_m: f32,
    pub assentar_no_terreno: bool,
}

impl ObjetoSalvo {
    pub fn placement(&self) -> Placement {
        Placement {
            lat_deg: self.lat,
            lon_deg: self.lon,
            heading_deg: self.heading_deg,
            escala: self.escala,
            offset_leste_m: self.offset_leste_m,
            offset_norte_m: self.offset_norte_m,
            offset_vertical_m: self.offset_vertical_m,
            assentar_no_terreno: self.assentar_no_terreno,
        }
    }

    pub fn de_placement(
        id: u32,
        nome: String,
        arquivo: PathBuf,
        p: &Placement,
        visivel: bool,
    ) -> Self {
        Self::de_placement_com_pai(id, nome, arquivo, p, visivel, None)
    }

    /// Mesma coisa, mas com hierarquia. `pai = None` significa raiz.
    pub fn de_placement_com_pai(
        id: u32,
        nome: String,
        arquivo: PathBuf,
        p: &Placement,
        visivel: bool,
        pai: Option<u32>,
    ) -> Self {
        Self {
            id,
            nome,
            pai,
            arquivo,
            visivel,
            lat: p.lat_deg,
            lon: p.lon_deg,
            heading_deg: p.heading_deg,
            escala: p.escala,
            offset_leste_m: p.offset_leste_m,
            offset_norte_m: p.offset_norte_m,
            offset_vertical_m: p.offset_vertical_m,
            assentar_no_terreno: p.assentar_no_terreno,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProjetoErro {
    #[error("erro de I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON invalido: {0}")]
    Json(#[from] serde_json::Error),
    #[error("projeto na versao {encontrada}; esta build entende ate a {suportada}")]
    VersaoFutura { encontrada: u32, suportada: u32 },
}

impl Projeto {
    /// Grava de forma atomica: escreve num temporario e renomeia.
    ///
    /// Sem isso, uma queda de energia no meio do save deixaria o projeto truncado —
    /// e o usuario perderia tudo com o arquivo ainda existindo, que e pior do que
    /// nao ter salvo.
    pub fn salvar(&self, caminho: &Path) -> Result<(), ProjetoErro> {
        if let Some(pai) = caminho.parent() {
            if !pai.as_os_str().is_empty() {
                std::fs::create_dir_all(pai)?;
            }
        }
        let texto = serde_json::to_string_pretty(self)?;
        let tmp = caminho.with_extension(format!("tmp{}", std::process::id()));
        std::fs::write(&tmp, texto)?;

        if let Err(e) = std::fs::rename(&tmp, caminho) {
            // No Windows o rename falha se o destino existe e esta aberto.
            let _ = std::fs::remove_file(&tmp);
            return Err(e.into());
        }
        Ok(())
    }

    pub fn abrir(caminho: &Path) -> Result<Self, ProjetoErro> {
        let texto = std::fs::read_to_string(caminho)?;
        let p: Projeto = serde_json::from_str(&texto)?;
        if p.versao > VERSAO_FORMATO {
            return Err(ProjetoErro::VersaoFutura {
                encontrada: p.versao,
                suportada: VERSAO_FORMATO,
            });
        }
        Ok(p)
    }

    /// Objetos cujo arquivo de origem sumiu. Avisar e melhor que abrir uma cena
    /// pela metade sem explicar o porque.
    pub fn arquivos_ausentes(&self) -> Vec<&ObjetoSalvo> {
        self.objetos
            .iter()
            .filter(|o| !o.arquivo.is_file())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn projeto_exemplo() -> Projeto {
        Projeto {
            versao: VERSAO_FORMATO,
            nome: "Projeto de Exemplo".into(),
            lat: -27.154_496_7,
            lon: -48.502_265_3,
            lado_m: 400.0,
            zoom_dem: 14,
            zoom_imagery: 18,
            mes: 3,
            dia: 21,
            hora: 15.0,
            objetos: vec![
                ObjetoSalvo::de_placement(
                    0,
                    "Edificio Exemplo".into(),
                    PathBuf::from("fixtures/example-building.glb"),
                    &Placement {
                        lat_deg: -27.154_496_7,
                        lon_deg: -48.502_265_3,
                        heading_deg: 59.98,
                        escala: 1.0,
                        offset_leste_m: -17.5,
                        offset_norte_m: 4.25,
                        ..Default::default()
                    },
                    true,
                ),
                ObjetoSalvo::de_placement(
                    1,
                    "Arvore".into(),
                    PathBuf::from("modelos/tree.glb"),
                    &Placement::default(),
                    false,
                ),
            ],
            cameras: Vec::new(),
        }
    }

    #[test]
    fn salvar_e_abrir_preserva_tudo() {
        let dir = std::env::temp_dir().join(format!("arcz-proj-{}", std::process::id()));
        let arq = dir.join("teste.arcz");
        let p = projeto_exemplo();

        p.salvar(&arq).unwrap();
        let lido = Projeto::abrir(&arq).unwrap();

        assert_eq!(lido.nome, p.nome);
        assert_eq!(
            lido.objetos, p.objetos,
            "os objetos nao sobreviveram ao ciclo"
        );
        assert!((lido.lat - p.lat).abs() < 1e-12);
        assert!((lido.hora - p.hora).abs() < 1e-12);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_transformacao_sobrevive_ao_ciclo_completo() {
        // E o requisito central: fechar e reabrir nao pode mover nada.
        let dir = std::env::temp_dir().join(format!("arcz-proj2-{}", std::process::id()));
        let arq = dir.join("t.arcz");
        projeto_exemplo().salvar(&arq).unwrap();

        let lido = Projeto::abrir(&arq).unwrap();
        let pl = lido.objetos[0].placement();

        assert!((pl.heading_deg - 59.98).abs() < 1e-9);
        assert!((pl.offset_leste_m + 17.5).abs() < 1e-6);
        assert!((pl.offset_norte_m - 4.25).abs() < 1e-6);
        assert!((pl.lat_deg + 27.154_496_7).abs() < 1e-12);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn visibilidade_persiste() {
        let dir = std::env::temp_dir().join(format!("arcz-proj3-{}", std::process::id()));
        let arq = dir.join("t.arcz");
        projeto_exemplo().salvar(&arq).unwrap();

        let lido = Projeto::abrir(&arq).unwrap();
        assert!(lido.objetos[0].visivel);
        assert!(!lido.objetos[1].visivel, "objeto oculto voltou visivel");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn salvar_nao_deixa_temporario_para_tras() {
        let dir = std::env::temp_dir().join(format!("arcz-proj4-{}", std::process::id()));
        let arq = dir.join("t.arcz");
        projeto_exemplo().salvar(&arq).unwrap();
        projeto_exemplo().salvar(&arq).unwrap(); // sobrescreve

        let sobras: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().contains("tmp"))
            .collect();
        assert!(sobras.is_empty(), "temporarios vazados: {sobras:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn projeto_de_versao_futura_e_recusado_com_mensagem_clara() {
        // Abrir calado um formato que nao se entende corrompe o arquivo no proximo save.
        let dir = std::env::temp_dir().join(format!("arcz-proj5-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let arq = dir.join("futuro.arcz");

        let mut p = projeto_exemplo();
        p.versao = VERSAO_FORMATO + 7;
        std::fs::write(&arq, serde_json::to_string(&p).unwrap()).unwrap();

        let e = Projeto::abrir(&arq).unwrap_err();
        assert!(
            matches!(e, ProjetoErro::VersaoFutura { .. }),
            "esperava recusa de versao, veio {e:?}"
        );
        assert!(e.to_string().contains("versao"), "{e}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn json_corrompido_vira_erro_e_nao_panico() {
        let dir = std::env::temp_dir().join(format!("arcz-proj6-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let arq = dir.join("ruim.arcz");
        std::fs::write(&arq, "{ isto nao e json").unwrap();

        assert!(matches!(
            Projeto::abrir(&arq).unwrap_err(),
            ProjetoErro::Json(_)
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn arquivos_ausentes_sao_detectados() {
        let p = projeto_exemplo();
        // Nenhum dos caminhos de exemplo existe de verdade.
        assert_eq!(p.arquivos_ausentes().len(), 2);
    }

    #[test]
    fn cameras_faltando_no_json_nao_impedem_a_abertura() {
        // `#[serde(default)]` protege projetos gravados antes do campo existir.
        let dir = std::env::temp_dir().join(format!("arcz-proj7-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let arq = dir.join("antigo.arcz");

        let json = r#"{"versao":1,"nome":"antigo","lat":0,"lon":0,"lado_m":400,
          "zoom_dem":14,"zoom_imagery":18,"mes":3,"dia":21,"hora":15,"objetos":[]}"#;
        std::fs::write(&arq, json).unwrap();

        let p = Projeto::abrir(&arq).unwrap();
        assert!(p.cameras.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn projeto_v1_abre_como_v2_pai_none() {
        // Adicionado no bump v1 -> v2: campo `pai` faltando no JSON deve virar None.
        let dir = std::env::temp_dir().join(format!("arcz-proj8-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let arq = dir.join("v1.arcz");

        let json = r#"{"versao":1,"nome":"v1","lat":-27.15,"lon":-48.50,"lado_m":400,
          "zoom_dem":14,"zoom_imagery":18,"mes":3,"dia":21,"hora":15,"objetos":[
            {"id":0,"nome":"zenite","arquivo":"z.glb","visivel":true,
             "lat":-27.15,"lon":-48.50,"heading_deg":0.0,"escala":1.0,
             "offset_leste_m":0.0,"offset_norte_m":0.0,"offset_vertical_m":0.0,
             "assentar_no_terreno":true}]}"#;
        std::fs::write(&arq, json).unwrap();

        let p = Projeto::abrir(&arq).unwrap();
        assert!(p.objetos[0].pai.is_none(), "v1 sem campo pai -> None");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hierarquia_pai_filho_sobrevive_ao_ciclo() {
        let dir = std::env::temp_dir().join(format!("arcz-proj9-{}", std::process::id()));
        let arq = dir.join("hier.arcz");

        let raiz = ObjetoSalvo::de_placement_com_pai(
            0,
            "raiz".into(),
            PathBuf::from("r.glb"),
            &Placement::default(),
            true,
            None,
        );
        let filho = ObjetoSalvo::de_placement_com_pai(
            1,
            "filho".into(),
            PathBuf::from("f.glb"),
            &Placement::default(),
            true,
            Some(0),
        );
        let p = Projeto {
            versao: VERSAO_FORMATO,
            nome: "h".into(),
            lat: 0.0,
            lon: 0.0,
            lado_m: 400.0,
            zoom_dem: 14,
            zoom_imagery: 18,
            mes: 3,
            dia: 21,
            hora: 15.0,
            objetos: vec![raiz.clone(), filho.clone()],
            cameras: Vec::new(),
        };
        p.salvar(&arq).unwrap();
        let lido = Projeto::abrir(&arq).unwrap();
        assert!(lido.objetos[0].pai.is_none());
        assert_eq!(lido.objetos[1].pai, Some(0));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

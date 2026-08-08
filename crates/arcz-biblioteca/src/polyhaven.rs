//! Download dos assets CC0 do Poly Haven.
//!
//! O Poly Haven publica tudo em CC0 e expoe uma API publica sem chave:
//! `GET https://api.polyhaven.com/files/{slug}` devolve, por formato e resolucao, a
//! URL do arquivo principal e a lista de dependencias (`.bin` e texturas). Baixamos
//! o glTF porque e o formato que o loader do ARCZ ja le
//! (`arcz_model::Model::load` usa `gltf::import`, que resolve buffer e imagem
//! externos relativos ao arquivo).
//!
//! Cuidados que valem repetir: **escrita atomica** (temporario + rename, para nunca
//! deixar textura truncada), **anti path-escape** (o nome do arquivo dependente vem
//! do servidor, entao nao pode escapar da pasta do item) e **cache** (item ja
//! baixado nao volta pela rede).

use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::catalogo::{Fonte, Item};

/// Resolucao de textura pedida ao Poly Haven.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolucao {
    /// ~350 kB por modelo. Suficiente para mobiliario visto de longe.
    R1k,
    /// ~1,3 MB por modelo. Padrao: le bem em close de interior.
    R2k,
    /// ~5 MB por modelo. So para peca heroi em primeiro plano.
    R4k,
}

impl Resolucao {
    pub fn chave(self) -> &'static str {
        match self {
            Self::R1k => "1k",
            Self::R2k => "2k",
            Self::R4k => "4k",
        }
    }

    pub fn de_texto(s: &str) -> Option<Self> {
        match s {
            "1k" => Some(Self::R1k),
            "2k" => Some(Self::R2k),
            "4k" => Some(Self::R4k),
            _ => None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BibliotecaError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("rede: {0}")]
    Rede(#[from] reqwest::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("glb: {0}")]
    Glb(#[from] crate::parametrico::GlbError),
    #[error("o item '{0}' nao vem do Poly Haven")]
    NaoRemoto(String),
    #[error("'{slug}' nao tem glTF na resolucao {res}")]
    SemResolucao { slug: String, res: &'static str },
    #[error("caminho '{0}' tenta escapar da pasta do item")]
    CaminhoInvalido(String),
    #[error("HTTP {status} em {url}")]
    Http { status: u16, url: String },
}

/// Resposta de `/files/{slug}`, so a parte que interessa.
#[derive(Debug, Deserialize)]
struct RespostaArquivos {
    #[serde(default)]
    gltf: std::collections::HashMap<String, std::collections::HashMap<String, ArquivoGltf>>,
}

#[derive(Debug, Deserialize)]
struct ArquivoGltf {
    url: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    include: std::collections::HashMap<String, Dependencia>,
}

#[derive(Debug, Deserialize)]
struct Dependencia {
    url: String,
    #[serde(default)]
    size: u64,
}

/// O que foi baixado (ou reaproveitado) para um item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relatorio {
    pub chave: String,
    pub modelo: PathBuf,
    pub arquivos: usize,
    pub bytes: u64,
    /// `true` se ja estava em disco e nada foi pela rede.
    pub reaproveitado: bool,
}

/// URL da API de arquivos de um asset.
pub fn url_api(slug: &str) -> String {
    format!("https://api.polyhaven.com/files/{slug}")
}

/// Pagina publica do asset, usada no manifesto de licenca.
pub fn url_pagina(slug: &str) -> String {
    format!("https://polyhaven.com/a/{slug}")
}

/// Resolve `relativo` dentro de `pasta`, recusando qualquer coisa que escape.
///
/// O nome vem do servidor (`include`), entao trata-se de entrada nao confiavel:
/// `../../.ssh/id_rsa` nao pode virar caminho valido.
pub fn destino_seguro(pasta: &Path, relativo: &str) -> Result<PathBuf, BibliotecaError> {
    let invalido = relativo.is_empty()
        || relativo.starts_with('/')
        || relativo.starts_with('\\')
        || relativo.contains(':')
        || relativo
            .split(['/', '\\'])
            .any(|p| p == ".." || p == "." || p.is_empty());
    if invalido {
        return Err(BibliotecaError::CaminhoInvalido(relativo.to_string()));
    }
    let mut destino = pasta.to_path_buf();
    for parte in relativo.split(['/', '\\']) {
        destino.push(parte);
    }
    Ok(destino)
}

/// Baixador de itens do catalogo.
pub struct Baixador {
    client: reqwest::Client,
    raiz: PathBuf,
    resolucao: Resolucao,
}

impl Baixador {
    pub fn novo(raiz: impl Into<PathBuf>, resolucao: Resolucao) -> Result<Self, BibliotecaError> {
        let raiz = raiz.into();
        std::fs::create_dir_all(&raiz)?;
        let client = reqwest::Client::builder()
            .user_agent(concat!("arcz-biblioteca/", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(120))
            .build()?;
        Ok(Self {
            client,
            raiz,
            resolucao,
        })
    }

    pub fn raiz(&self) -> &Path {
        &self.raiz
    }

    /// Pasta do item dentro da biblioteca.
    pub fn pasta_do_item(&self, item: &Item) -> PathBuf {
        self.raiz.join(item.chave)
    }

    /// Baixa um item do Poly Haven. Item ja completo em disco nao vai pela rede.
    ///
    /// "Completo" e o marcador [`MARCADOR`], nao a presenca do `.gltf`: um download
    /// interrompido depois do `.gltf` e antes da textura deixava a pasta com o
    /// modelo mas sem o `.bin`, e a passada seguinte dava o item por pronto. Foi um
    /// erro real (`banco-modular`, queda de conexao), e o `.gltf` sozinho abria com
    /// "arquivo nao encontrado" so na hora de montar a cena.
    pub async fn baixar(&self, item: &Item) -> Result<Relatorio, BibliotecaError> {
        let Fonte::PolyHaven { slug } = item.fonte else {
            return Err(BibliotecaError::NaoRemoto(item.chave.to_string()));
        };
        let pasta = self.pasta_do_item(item);

        if let (true, Some(existente)) = (completo(&pasta), modelo_existente(&pasta)) {
            return Ok(Relatorio {
                chave: item.chave.to_string(),
                modelo: existente,
                arquivos: 0,
                bytes: 0,
                reaproveitado: true,
            });
        }

        let url = url_api(slug);
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(BibliotecaError::Http {
                status: resp.status().as_u16(),
                url,
            });
        }
        let arquivos: RespostaArquivos = resp.json().await?;

        // Nem todo asset e publicado nas tres resolucoes (`decorative_book_set_01`
        // so tem 1k). Cair para a proxima e melhor que devolver o item vazio.
        let (res, entrada) = ordem_de_resolucao(self.resolucao)
            .into_iter()
            .find_map(|r| {
                arquivos
                    .gltf
                    .get(r.chave())
                    .and_then(|m| m.get("gltf"))
                    .map(|e| (r, e))
            })
            .ok_or_else(|| BibliotecaError::SemResolucao {
                slug: slug.to_string(),
                res: self.resolucao.chave(),
            })?;
        if res != self.resolucao {
            log::info!(
                "{slug}: sem glTF em {}, usando {}",
                self.resolucao.chave(),
                res.chave()
            );
        }

        std::fs::create_dir_all(&pasta)?;

        let nome_principal = nome_do_url(&entrada.url);
        let destino_principal = destino_seguro(&pasta, &nome_principal)?;
        let mut bytes = self
            .baixar_arquivo(&entrada.url, &destino_principal, entrada.size)
            .await?;
        let mut n = 1usize;

        for (relativo, dep) in &entrada.include {
            let destino = destino_seguro(&pasta, relativo)?;
            bytes += self.baixar_arquivo(&dep.url, &destino, dep.size).await?;
            n += 1;
        }

        escrever_licenca(&pasta, item, slug)?;
        // Marcador por ultimo: so existe se TODOS os arquivos chegaram.
        std::fs::write(pasta.join(MARCADOR), res.chave())?;

        Ok(Relatorio {
            chave: item.chave.to_string(),
            modelo: destino_principal,
            arquivos: n,
            bytes,
            reaproveitado: false,
        })
    }

    /// Baixa um arquivo. `tamanho_esperado` vem da API: 0 significa "nao informado".
    /// Divergencia nao aborta (a API as vezes desatualiza o campo), mas fica no log
    /// — download truncado silencioso e o pior modo de falha aqui.
    async fn baixar_arquivo(
        &self,
        url: &str,
        destino: &Path,
        tamanho_esperado: u64,
    ) -> Result<u64, BibliotecaError> {
        if let Some(pai) = destino.parent() {
            std::fs::create_dir_all(pai)?;
        }

        // Ate 3 tentativas: baixar 60 arquivos seguidos do mesmo CDN esbarra em
        // conexao derrubada no meio, e perder um item inteiro por causa disso e caro.
        let mut ultima: Option<BibliotecaError> = None;
        let mut corpo = None;
        for tentativa in 1..=TENTATIVAS {
            match self.tentar(url).await {
                Ok(bytes) => {
                    corpo = Some(bytes);
                    break;
                }
                Err(e) => {
                    log::warn!("{url}: tentativa {tentativa}/{TENTATIVAS} falhou: {e}");
                    ultima = Some(e);
                    tokio::time::sleep(std::time::Duration::from_millis(400 * tentativa as u64))
                        .await;
                }
            }
        }
        let corpo = match corpo {
            Some(c) => c,
            None => return Err(ultima.expect("erro registrado na ultima tentativa")),
        };
        if tamanho_esperado > 0 && corpo.len() as u64 != tamanho_esperado {
            log::warn!(
                "{url}: baixou {} bytes, a API dizia {tamanho_esperado}",
                corpo.len()
            );
        }
        // Temporario + rename: interrupcao nunca deixa arquivo pela metade.
        let tmp = destino.with_extension("parcial");
        std::fs::write(&tmp, &corpo)?;
        std::fs::rename(&tmp, destino)?;
        Ok(corpo.len() as u64)
    }

    async fn tentar(&self, url: &str) -> Result<bytes::Bytes, BibliotecaError> {
        let resp = self.client.get(url).send().await?;
        if !resp.status().is_success() {
            return Err(BibliotecaError::Http {
                status: resp.status().as_u16(),
                url: url.to_string(),
            });
        }
        Ok(resp.bytes().await?)
    }
}

/// Quantas vezes tentar cada arquivo antes de desistir.
const TENTATIVAS: usize = 3;

/// Resolucao pedida primeiro, depois as outras da maior para a menor.
///
/// A ordem importa: se o item nao tem 2k, cair para 1k mantem o arquivo leve;
/// cair para 4k so acontece quando e a unica que existe.
pub fn ordem_de_resolucao(pedida: Resolucao) -> Vec<Resolucao> {
    let mut ordem = vec![pedida];
    for r in [Resolucao::R2k, Resolucao::R1k, Resolucao::R4k] {
        if r != pedida {
            ordem.push(r);
        }
    }
    ordem
}

/// Arquivo escrito ao fim de um download bem-sucedido. A presenca dele e o unico
/// sinal confiavel de que a pasta do item esta inteira.
pub const MARCADOR: &str = ".completo";

/// `true` se o item ja foi baixado por inteiro alguma vez.
pub fn completo(pasta: &Path) -> bool {
    pasta.join(MARCADOR).is_file()
}

/// Procura um `.gltf`/`.glb` ja baixado na pasta do item.
pub fn modelo_existente(pasta: &Path) -> Option<PathBuf> {
    let entradas = std::fs::read_dir(pasta).ok()?;
    let mut achados: Vec<PathBuf> = entradas
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            matches!(
                p.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.to_ascii_lowercase())
                    .as_deref(),
                Some("gltf") | Some("glb")
            )
        })
        .collect();
    achados.sort();
    achados.into_iter().next()
}

fn nome_do_url(url: &str) -> String {
    url.rsplit('/').next().unwrap_or("modelo.gltf").to_string()
}

fn escrever_licenca(pasta: &Path, item: &Item, slug: &str) -> Result<(), std::io::Error> {
    let texto = format!(
        "Item: {} ({})\nLicenca: {}\nOrigem: {}\nBaixado por: arcz-biblioteca\n\n\
         CC0 1.0 Universal: o autor abriu mao dos direitos patrimoniais. Uso comercial\n\
         livre, sem necessidade de credito. Detalhes: https://creativecommons.org/publicdomain/zero/1.0/\n",
        item.nome,
        item.chave,
        item.licenca.texto(),
        url_pagina(slug),
    );
    std::fs::write(pasta.join("LICENCA.txt"), texto)
}

/// Linha do manifesto da biblioteca.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct LinhaManifesto {
    pub chave: String,
    pub nome: String,
    pub origem: String,
    pub licenca: String,
    pub modelo: String,
    pub sha256: String,
    pub bytes: u64,
}

/// Monta a linha de manifesto de um arquivo ja em disco.
pub fn linha_manifesto(item: &Item, modelo: &Path) -> Result<LinhaManifesto, std::io::Error> {
    let dados = std::fs::read(modelo)?;
    let sha = Sha256::digest(&dados);
    let origem = match item.fonte {
        Fonte::PolyHaven { slug } => url_pagina(slug),
        Fonte::Parametrica(p) => format!("arcz://parametrico/{}", p.nome_arquivo()),
    };
    Ok(LinhaManifesto {
        chave: item.chave.to_string(),
        nome: item.nome.to_string(),
        origem,
        licenca: item.licenca.texto().to_string(),
        modelo: modelo.display().to_string(),
        sha256: dados_hex(&sha),
        bytes: dados.len() as u64,
    })
}

fn dados_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_da_api_e_da_pagina() {
        assert_eq!(
            url_api("sofa_02"),
            "https://api.polyhaven.com/files/sofa_02"
        );
        assert_eq!(url_pagina("sofa_02"), "https://polyhaven.com/a/sofa_02");
    }

    #[test]
    fn destino_aceita_caminho_relativo_normal() {
        let pasta = Path::new("/tmp/bib/sofa");
        let d = destino_seguro(pasta, "textures/sofa_02_diff_2k.jpg").unwrap();
        assert!(
            d.ends_with("textures/sofa_02_diff_2k.jpg")
                || d.ends_with("textures\\sofa_02_diff_2k.jpg")
        );
    }

    #[test]
    fn destino_recusa_escapar_da_pasta() {
        let pasta = Path::new("/tmp/bib/sofa");
        for ruim in [
            "../fora.txt",
            "textures/../../fora.txt",
            "/etc/passwd",
            "\\\\servidor\\share",
            "C:/Windows/system32/x.dll",
            "",
        ] {
            assert!(
                destino_seguro(pasta, ruim).is_err(),
                "deveria recusar: {ruim:?}"
            );
        }
    }

    #[test]
    fn nome_do_arquivo_sai_do_url() {
        assert_eq!(
            nome_do_url(
                "https://dl.polyhaven.org/file/ph-assets/Models/gltf/2k/sofa_02/sofa_02_2k.gltf"
            ),
            "sofa_02_2k.gltf"
        );
    }

    #[test]
    fn ordem_de_resolucao_comeca_pela_pedida_e_cobre_todas() {
        assert_eq!(
            ordem_de_resolucao(Resolucao::R1k),
            vec![Resolucao::R1k, Resolucao::R2k, Resolucao::R4k]
        );
        assert_eq!(
            ordem_de_resolucao(Resolucao::R2k),
            vec![Resolucao::R2k, Resolucao::R1k, Resolucao::R4k]
        );
        assert_eq!(ordem_de_resolucao(Resolucao::R4k).len(), 3);
    }

    #[test]
    fn resolucao_vai_e_volta() {
        for r in [Resolucao::R1k, Resolucao::R2k, Resolucao::R4k] {
            assert_eq!(Resolucao::de_texto(r.chave()), Some(r));
        }
        assert_eq!(Resolucao::de_texto("8k"), None);
    }

    #[test]
    fn resposta_da_api_e_desserializada() {
        // Recorte real de /files/sofa_02.
        let json = r#"{
            "gltf": { "2k": { "gltf": {
                "url": "https://dl.polyhaven.org/file/ph-assets/Models/gltf/2k/sofa_02/sofa_02_2k.gltf",
                "size": 4183,
                "include": {
                    "sofa_02.bin": { "url": "https://x/sofa_02.bin", "size": 74864 },
                    "textures/sofa_02_diff_2k.jpg": { "url": "https://x/d.jpg", "size": 323319 }
                }
            } } }
        }"#;
        let r: RespostaArquivos = serde_json::from_str(json).unwrap();
        let e = &r.gltf["2k"]["gltf"];
        assert_eq!(e.size, 4183);
        assert_eq!(e.include.len(), 2);
    }

    #[test]
    fn item_parametrico_nao_pode_ser_baixado() {
        let item = crate::catalogo::por_chave("cama-casal").unwrap();
        let dir = std::env::temp_dir().join("arcz-bib-teste-naoremoto");
        let b = Baixador::novo(&dir, Resolucao::R2k).unwrap();
        let erro = futures_bloqueante(b.baixar(item));
        assert!(matches!(erro, Err(BibliotecaError::NaoRemoto(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Executa um future ate o fim num runtime de teste de uma thread.
    fn futures_bloqueante<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(f)
    }

    #[test]
    fn manifesto_registra_sha256_do_arquivo() {
        let dir = std::env::temp_dir().join("arcz-bib-teste-manifesto");
        std::fs::create_dir_all(&dir).unwrap();
        let modelo = dir.join("cama.glb");
        crate::parametrico::escrever_glb(
            &crate::parametrico::Peca::CamaCasal.malha(),
            "cama",
            &modelo,
        )
        .unwrap();

        let item = crate::catalogo::por_chave("cama-casal").unwrap();
        let l = linha_manifesto(item, &modelo).unwrap();
        assert_eq!(l.chave, "cama-casal");
        assert_eq!(l.sha256.len(), 64);
        assert!(l.bytes > 100);
        assert!(l.origem.starts_with("arcz://parametrico/"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pasta_sem_marcador_nao_conta_como_completa() {
        // Reproduz o `banco-modular`: o .gltf chegou, o .bin nao. Sem o marcador,
        // a proxima passada tem que baixar de novo em vez de dar por pronto.
        let dir = std::env::temp_dir().join("arcz-bib-teste-incompleto");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("x.gltf"), b"{}").unwrap();
        assert!(modelo_existente(&dir).is_some());
        assert!(
            !completo(&dir),
            "pasta sem marcador nao pode contar como completa"
        );

        std::fs::write(dir.join(MARCADOR), b"2k").unwrap();
        assert!(completo(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn modelo_existente_encontra_gltf_na_pasta() {
        let dir = std::env::temp_dir().join("arcz-bib-teste-existente");
        std::fs::create_dir_all(&dir).unwrap();
        assert!(modelo_existente(&dir).is_none());
        std::fs::write(dir.join("x.gltf"), b"{}").unwrap();
        assert!(modelo_existente(&dir).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

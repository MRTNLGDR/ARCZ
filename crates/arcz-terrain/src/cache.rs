//! Cache de tiles em disco, enderecado por conteudo da URL.
//!
//! Mesmo padrao do blob store do ZENITE: SHA-256 da chave, sharding por prefixo para
//! nao criar diretorio com centenas de milhares de entradas, e escrita atomica via
//! arquivo temporario + rename. Um processo morto no meio do download nunca deixa
//! tile truncado no cache.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::TerrainError;

/// Cache de tiles com download sob demanda.
#[derive(Debug, Clone)]
pub struct TileCache {
    root: PathBuf,
    client: reqwest::Client,
}

impl TileCache {
    /// Abre (criando se preciso) um cache em `root`.
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, TerrainError> {
        let root = root.into();
        std::fs::create_dir_all(&root)?;

        let client = reqwest::Client::builder()
            .user_agent(concat!("arcz/", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        Ok(Self { root, client })
    }

    /// Diretorio de cache padrao do usuario.
    pub fn default_root() -> PathBuf {
        // LOCALAPPDATA no Windows, XDG_CACHE_HOME/HOME no resto. Nunca relativo ao cwd
        // — esse foi um bug real no ZENITE (spec 10.6) e nao vamos repetir.
        let base = std::env::var_os("LOCALAPPDATA")
            .or_else(|| std::env::var_os("XDG_CACHE_HOME"))
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
            .unwrap_or_else(std::env::temp_dir);
        base.join("ARCZ").join("tiles")
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Caminho onde a URL e (ou seria) armazenada.
    pub fn path_for(&self, url: &str) -> PathBuf {
        let digest = Sha256::digest(url.as_bytes());
        let hex = hex_lower(&digest);
        // Dois niveis de sharding: 256 x 256 diretorios no pior caso.
        self.root
            .join(&hex[0..2])
            .join(&hex[2..4])
            .join(format!("{hex}.bin"))
    }

    /// `true` se a URL ja esta em cache.
    pub fn is_cached(&self, url: &str) -> bool {
        self.path_for(url).is_file()
    }

    /// Devolve os bytes da URL, baixando na primeira vez.
    ///
    /// HTTP 404 vira [`TerrainError::TileAusente`] em vez de erro generico: faltar tile
    /// e situacao normal (oceano, borda de cobertura) e o chamador precisa distinguir
    /// isso de uma falha de rede.
    pub async fn get(&self, url: &str) -> Result<Vec<u8>, TerrainError> {
        let path = self.path_for(url);
        if let Ok(bytes) = tokio::fs::read(&path).await {
            if !bytes.is_empty() {
                return Ok(bytes);
            }
        }

        let resp = self.client.get(url).send().await?;
        let status = resp.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(TerrainError::TileAusente(url.to_string()));
        }
        if !status.is_success() {
            return Err(TerrainError::Http {
                url: url.to_string(),
                status: status.as_u16(),
            });
        }

        let bytes = resp.bytes().await?.to_vec();
        self.store(&path, &bytes).await?;
        Ok(bytes)
    }

    /// Grava com rename atomico: nenhum leitor ve conteudo parcial.
    async fn store(&self, path: &Path, bytes: &[u8]) -> Result<(), TerrainError> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // O nome do temporario inclui o pid para dois processos nao colidirem.
        let tmp = path.with_extension(format!("tmp{}", std::process::id()));
        {
            let mut f = tokio::fs::File::create(&tmp).await?;
            f.write_all(bytes).await?;
            f.sync_all().await?;
        }

        match tokio::fs::rename(&tmp, path).await {
            Ok(()) => Ok(()),
            Err(e) => {
                // No Windows o rename falha se o destino ja existe e esta aberto.
                // Se o destino ja e valido, outro processo ganhou a corrida — tudo bem.
                let _ = tokio::fs::remove_file(&tmp).await;
                if path.is_file() {
                    Ok(())
                } else {
                    Err(e.into())
                }
            }
        }
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    const DIGITOS: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(DIGITOS[(b >> 4) as usize] as char);
        s.push(DIGITOS[(b & 0x0f) as usize] as char);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache_temporario(nome: &str) -> (TileCache, PathBuf) {
        let dir = std::env::temp_dir().join(format!("arcz-test-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        (TileCache::new(&dir).unwrap(), dir)
    }

    #[test]
    fn hex_confere_com_vetor_conhecido() {
        // SHA-256 da string vazia.
        assert_eq!(
            hex_lower(&Sha256::digest(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn caminhos_sao_deterministicos_e_shardados() {
        let (c, dir) = cache_temporario("path");

        let url = "https://exemplo/1/2/3.png";
        let p1 = c.path_for(url);
        let p2 = c.path_for(url);
        assert_eq!(p1, p2);
        assert_ne!(p1, c.path_for("https://exemplo/1/2/4.png"));

        // .../root/aa/bb/<hash>.bin
        let rel = p1.strip_prefix(&dir).unwrap();
        assert_eq!(
            rel.components().count(),
            3,
            "esperado sharding em 2 niveis: {rel:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nenhum_caminho_escapa_da_raiz() {
        let (c, dir) = cache_temporario("escape");
        // A chave e hasheada, entao nem `..` nem barra na URL viram caminho.
        for url in ["../../etc/passwd", "..\\..\\windows", "https://a/../../b"] {
            let p = c.path_for(url);
            assert!(p.starts_with(&dir), "{p:?} escapou de {dir:?}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn store_e_atomico_e_reutiliza_o_cache() {
        let (c, dir) = cache_temporario("store");
        let url = "https://exemplo/tile.png";
        let path = c.path_for(url);

        c.store(&path, b"conteudo-de-teste").await.unwrap();

        assert!(c.is_cached(url));
        assert_eq!(std::fs::read(&path).unwrap(), b"conteudo-de-teste");

        // Nenhum arquivo temporario sobrou.
        let sobras: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().contains("tmp"))
            .collect();
        assert!(sobras.is_empty(), "temporarios vazados: {sobras:?}");

        // Regravar por cima nao corrompe.
        c.store(&path, b"outro-conteudo").await.unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"outro-conteudo");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn diretorio_padrao_e_absoluto() {
        let p = TileCache::default_root();
        assert!(p.is_absolute(), "cache padrao relativo ao cwd: {p:?}");
        assert!(
            p.ends_with("ARCZ/tiles") || p.ends_with("ARCZ\\tiles"),
            "{p:?}"
        );
    }
}

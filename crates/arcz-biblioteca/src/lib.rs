//! Biblioteca de mobiliario e decoracao do ARCZ.
//!
//! Tres partes:
//!
//! - [`catalogo`] — a curadoria: que item existe, que papel cumpre na planta, de onde
//!   vem e sob que licenca.
//! - [`polyhaven`] — download dos assets CC0 fotorreais.
//! - [`parametrico`] — geracao das pecas que nenhum acervo CC0 cobre (cama, armario,
//!   bancada, louca de banheiro), na medida exata da planta.
//!
//! O resultado em disco e uma pasta por item, com o modelo, as texturas, a licenca e
//! um `manifesto.json` na raiz. E exatamente o formato que
//! `arcz_app::cena::varrer_biblioteca` ja sabe ler.

pub mod catalogo;
pub mod parametrico;
pub mod polyhaven;

use std::path::{Path, PathBuf};

pub use catalogo::{Ambiente, Fonte, Item, Licenca, Papel, CATALOGO};
pub use parametrico::Peca;
pub use polyhaven::{Baixador, BibliotecaError, Relatorio, Resolucao};

/// Raiz padrao da biblioteca dentro do projeto.
pub fn raiz_padrao() -> PathBuf {
    PathBuf::from("biblioteca")
}

/// Resultado de [`montar`].
#[derive(Debug, Default)]
pub struct Resumo {
    pub gerados: Vec<String>,
    pub baixados: Vec<Relatorio>,
    pub falhas: Vec<(String, String)>,
    pub bytes: u64,
}

impl Resumo {
    pub fn total(&self) -> usize {
        self.gerados.len() + self.baixados.len()
    }
}

/// Monta a biblioteca em `raiz`: gera as pecas parametricas e baixa as remotas.
///
/// `filtro` limita a um ambiente (util para montar so o que a recepcao precisa).
/// Item que ja existe em disco nao e refeito nem rebaixado.
pub async fn montar(
    raiz: &Path,
    resolucao: Resolucao,
    filtro: Option<Ambiente>,
    somente_locais: bool,
) -> Result<Resumo, BibliotecaError> {
    let baixador = Baixador::novo(raiz, resolucao)?;
    let mut resumo = Resumo::default();
    let mut manifesto = Vec::new();

    for item in CATALOGO {
        if let Some(amb) = filtro {
            if !item.ambientes.contains(&amb) {
                continue;
            }
        }

        let pasta = raiz.join(item.chave);
        let modelo = match item.fonte {
            Fonte::Parametrica(peca) => {
                let destino = pasta.join(format!("{}.glb", peca.nome_arquivo()));
                if !destino.exists() {
                    parametrico::escrever_glb(&peca.malha(), item.nome, &destino)?;
                    escrever_licenca_local(&pasta, item)?;
                    resumo.gerados.push(item.chave.to_string());
                }
                destino
            }
            Fonte::PolyHaven { .. } => {
                if somente_locais {
                    continue;
                }
                match baixador.baixar(item).await {
                    Ok(rel) => {
                        resumo.bytes += rel.bytes;
                        let modelo = rel.modelo.clone();
                        resumo.baixados.push(rel);
                        modelo
                    }
                    Err(e) => {
                        resumo.falhas.push((item.chave.to_string(), e.to_string()));
                        continue;
                    }
                }
            }
        };

        match polyhaven::linha_manifesto(item, &modelo) {
            Ok(l) => manifesto.push(l),
            Err(e) => resumo.falhas.push((item.chave.to_string(), e.to_string())),
        }
    }

    let json = serde_json::to_vec_pretty(&manifesto)?;
    std::fs::write(raiz.join("manifesto.json"), json)?;
    Ok(resumo)
}

fn escrever_licenca_local(pasta: &Path, item: &Item) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(pasta)?;
    let texto = format!(
        "Item: {} ({})\nLicenca: {}\nOrigem: gerado pelo ARCZ (crate arcz-biblioteca)\n\n\
         Geometria propria, sem dependencia de terceiros. Pode ser usada, alterada e\n\
         distribuida sem restricao.\n",
        item.nome,
        item.chave,
        item.licenca.texto()
    );
    std::fs::write(pasta.join("LICENCA.txt"), texto)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn montar_so_locais_gera_todas_as_pecas_parametricas() {
        let dir = std::env::temp_dir().join("arcz-bib-teste-montar");
        let _ = std::fs::remove_dir_all(&dir);

        let resumo = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(montar(&dir, Resolucao::R1k, None, true))
            .unwrap();

        let esperadas = CATALOGO
            .iter()
            .filter(|i| matches!(i.fonte, Fonte::Parametrica(_)))
            .count();
        assert_eq!(resumo.gerados.len(), esperadas);
        assert!(resumo.baixados.is_empty(), "somente_locais nao pode baixar");
        assert!(resumo.falhas.is_empty(), "falhas: {:?}", resumo.falhas);

        // Manifesto so lista o que existe em disco.
        let manifesto: Vec<polyhaven::LinhaManifesto> =
            serde_json::from_slice(&std::fs::read(dir.join("manifesto.json")).unwrap()).unwrap();
        assert_eq!(manifesto.len(), esperadas);
        for l in &manifesto {
            assert!(Path::new(&l.modelo).exists(), "{} nao existe", l.modelo);
            assert_eq!(l.sha256.len(), 64);
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn montar_duas_vezes_nao_regera() {
        let dir = std::env::temp_dir().join("arcz-bib-teste-idempotente");
        let _ = std::fs::remove_dir_all(&dir);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        let a = rt
            .block_on(montar(&dir, Resolucao::R1k, None, true))
            .unwrap();
        let b = rt
            .block_on(montar(&dir, Resolucao::R1k, None, true))
            .unwrap();
        assert!(!a.gerados.is_empty());
        assert!(
            b.gerados.is_empty(),
            "segunda passada regerou: {:?}",
            b.gerados
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn filtro_por_ambiente_reduz_o_conjunto() {
        let dir = std::env::temp_dir().join("arcz-bib-teste-filtro");
        let _ = std::fs::remove_dir_all(&dir);

        let resumo = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(montar(&dir, Resolucao::R1k, Some(Ambiente::Rooftop), true))
            .unwrap();

        // Rooftop tem espreguicadeira, guarda-sol e churrasqueira parametricos.
        assert!(resumo.gerados.contains(&"guarda-sol".to_string()));
        assert!(!resumo.gerados.contains(&"vaso-sanitario".to_string()));

        let _ = std::fs::remove_dir_all(&dir);
    }
}

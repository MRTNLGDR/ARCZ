//! Monta a biblioteca de mobiliario do ARCZ em disco.
//!
//! ```text
//! arcz-biblioteca [--raiz biblioteca] [--res 1k|2k|4k] [--ambiente apartamento]
//!                 [--somente-locais] [--listar]
//! ```

use std::path::PathBuf;

use arcz_biblioteca::{catalogo, montar, Ambiente, Fonte, Resolucao, CATALOGO};

#[tokio::main]
async fn main() {
    let mut raiz = PathBuf::from("biblioteca");
    let mut res = Resolucao::R2k;
    let mut ambiente: Option<Ambiente> = None;
    let mut somente_locais = false;
    let mut listar = false;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--raiz" => {
                i += 1;
                raiz = PathBuf::from(args.get(i).cloned().unwrap_or_default());
            }
            "--res" => {
                i += 1;
                match args.get(i).and_then(|s| Resolucao::de_texto(s)) {
                    Some(r) => res = r,
                    None => {
                        eprintln!("--res aceita 1k, 2k ou 4k");
                        std::process::exit(2);
                    }
                }
            }
            "--ambiente" => {
                i += 1;
                ambiente = match args.get(i).map(|s| s.as_str()) {
                    Some("apartamento") => Some(Ambiente::Apartamento),
                    Some("recepcao") => Some(Ambiente::Recepcao),
                    Some("cafeteria") => Some(Ambiente::Cafeteria),
                    Some("mercado") => Some(Ambiente::Mercado),
                    Some("rooftop") => Some(Ambiente::Rooftop),
                    outro => {
                        eprintln!("--ambiente invalido: {outro:?}");
                        std::process::exit(2);
                    }
                };
            }
            "--somente-locais" => somente_locais = true,
            "--listar" => listar = true,
            "--ajuda" | "-h" | "--help" => {
                println!(
                    "arcz-biblioteca [--raiz <pasta>] [--res 1k|2k|4k] \
                     [--ambiente apartamento|recepcao|cafeteria|mercado|rooftop] \
                     [--somente-locais] [--listar]"
                );
                return;
            }
            outro => {
                eprintln!("argumento desconhecido: {outro}");
                std::process::exit(2);
            }
        }
        i += 1;
    }

    if listar {
        println!("{:<26} {:<34} {:<10} AMBIENTES", "CHAVE", "NOME", "ORIGEM");
        for item in CATALOGO {
            if let Some(a) = ambiente {
                if !item.ambientes.contains(&a) {
                    continue;
                }
            }
            let origem = match item.fonte {
                Fonte::PolyHaven { .. } => "polyhaven",
                Fonte::Parametrica(_) => "arcz",
            };
            println!(
                "{:<26} {:<34} {:<10} {:?}",
                item.chave, item.nome, origem, item.ambientes
            );
        }
        let remotos = CATALOGO.iter().filter(|i| i.remoto()).count();
        println!(
            "\n{} itens ({} do Poly Haven CC0, {} gerados pelo ARCZ)",
            CATALOGO.len(),
            remotos,
            CATALOGO.len() - remotos
        );
        return;
    }

    println!(
        "montando biblioteca em {} (resolucao {}){}",
        raiz.display(),
        res.chave(),
        if somente_locais { ", sem rede" } else { "" }
    );

    match montar(&raiz, res, ambiente, somente_locais).await {
        Ok(r) => {
            println!(
                "\n{} itens prontos: {} gerados, {} baixados ({:.1} MB)",
                r.total(),
                r.gerados.len(),
                r.baixados.len(),
                r.bytes as f64 / 1e6
            );
            let reaproveitados = r.baixados.iter().filter(|b| b.reaproveitado).count();
            if reaproveitados > 0 {
                println!("{reaproveitados} ja estavam em disco (nao foram pela rede)");
            }
            if !r.falhas.is_empty() {
                eprintln!("\n{} falha(s):", r.falhas.len());
                for (chave, erro) in &r.falhas {
                    eprintln!("  {chave}: {erro}");
                }
                std::process::exit(1);
            }
            println!("manifesto: {}", raiz.join("manifesto.json").display());
            println!(
                "papeis cobertos: {}",
                catalogo::CATALOGO
                    .iter()
                    .map(|i| format!("{:?}", i.papel))
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
            );
        }
        Err(e) => {
            eprintln!("falhou: {e}");
            std::process::exit(1);
        }
    }
}

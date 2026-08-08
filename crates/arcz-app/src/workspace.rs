//! Gestao de projetos: catalogo, lixeira, snapshots e autosave.
//!
//! Layout em disco:
//!
//! ```text
//! <raiz>/
//!   projetos/<slug>/
//!     projeto.arcz          <- o projeto
//!     autosave.arcz         <- gravado periodicamente, nunca sobrescreve o de cima
//!     thumb.png             <- miniatura para a tela inicial
//!     snapshots/
//!       20260730-073000_antes-de-mobiliar.arcz
//!   lixeira/<slug>__<carimbo>/   <- projeto excluido, recuperavel
//! ```
//!
//! Duas regras que valem para o modulo inteiro:
//!
//! 1. **Excluir nunca apaga.** Move para a lixeira. So `esvaziar_lixeira` apaga de
//!    verdade, e ela e uma chamada separada e explicita.
//! 2. **Autosave e um arquivo a parte.** Escrever por cima do `projeto.arcz` faria
//!    o autosave destruir justamente o estado que o usuario queria preservar.

use std::path::{Path, PathBuf};

use crate::projeto::{Projeto, ProjetoErro};

/// Nome de arquivo seguro a partir de um titulo qualquer.
///
/// Sem isto, um projeto chamado `../../etc` viraria escrita fora da raiz.
pub fn slug(nome: &str) -> String {
    let limpo: String = nome
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();

    // Colapsa separadores repetidos e apara as pontas.
    let mut out = String::with_capacity(limpo.len());
    let mut anterior_hifen = false;
    for c in limpo.chars() {
        if c == '-' {
            if !anterior_hifen && !out.is_empty() {
                out.push('-');
            }
            anterior_hifen = true;
        } else {
            out.push(c);
            anterior_hifen = false;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }

    if out.is_empty() {
        "projeto".to_string()
    } else {
        out.chars().take(64).collect()
    }
}

/// Resumo de um projeto para a tela inicial, sem abrir o arquivo inteiro.
#[derive(Debug, Clone, PartialEq)]
pub struct ResumoProjeto {
    pub slug: String,
    pub nome: String,
    pub caminho: PathBuf,
    pub bytes: u64,
    /// Segundos desde a época Unix. `0` se o sistema não informar.
    pub modificado_em: u64,
    pub tem_miniatura: bool,
    /// `true` quando ha autosave mais novo que o projeto — indica queda.
    pub recuperavel: bool,
    pub snapshots: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResumoSnapshot {
    pub arquivo: PathBuf,
    pub rotulo: String,
    pub carimbo: String,
    pub bytes: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceErro {
    #[error("erro de I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("projeto: {0}")]
    Projeto(#[from] ProjetoErro),
    #[error("nao existe projeto com o identificador '{0}'")]
    NaoEncontrado(String),
    #[error("ja existe um projeto chamado '{0}'")]
    JaExiste(String),
    #[error("nada para recuperar em '{0}'")]
    SemRecuperacao(String),
}

/// A pasta de trabalho onde os projetos vivem.
pub struct Workspace {
    raiz: PathBuf,
}

impl Workspace {
    /// Abre (criando se preciso) um workspace em `raiz`.
    pub fn new(raiz: impl Into<PathBuf>) -> Result<Self, WorkspaceErro> {
        let raiz = raiz.into();
        std::fs::create_dir_all(raiz.join("projetos"))?;
        std::fs::create_dir_all(raiz.join("lixeira"))?;
        Ok(Self { raiz })
    }

    /// Pasta padrao do usuario. Nunca relativa ao diretorio de execucao.
    pub fn raiz_padrao() -> PathBuf {
        let base = std::env::var_os("LOCALAPPDATA")
            .or_else(|| std::env::var_os("XDG_DATA_HOME"))
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
            .unwrap_or_else(std::env::temp_dir);
        base.join("ARCZ")
    }

    pub fn raiz(&self) -> &Path {
        &self.raiz
    }
    pub fn pasta_de(&self, slug: &str) -> PathBuf {
        self.raiz.join("projetos").join(slug)
    }
    pub fn arquivo_de(&self, slug: &str) -> PathBuf {
        self.pasta_de(slug).join("projeto.arcz")
    }
    fn autosave_de(&self, slug: &str) -> PathBuf {
        self.pasta_de(slug).join("autosave.arcz")
    }
    fn pasta_snapshots(&self, slug: &str) -> PathBuf {
        self.pasta_de(slug).join("snapshots")
    }

    /// Cria um projeto novo. Falha se o identificador ja existir.
    pub fn criar(&self, projeto: &Projeto) -> Result<String, WorkspaceErro> {
        let s = slug(&projeto.nome);
        if self.arquivo_de(&s).exists() {
            return Err(WorkspaceErro::JaExiste(s));
        }
        std::fs::create_dir_all(self.pasta_snapshots(&s))?;
        projeto.salvar(&self.arquivo_de(&s))?;
        Ok(s)
    }

    /// Grava por cima do projeto existente.
    pub fn salvar(&self, slug: &str, projeto: &Projeto) -> Result<(), WorkspaceErro> {
        if !self.pasta_de(slug).is_dir() {
            return Err(WorkspaceErro::NaoEncontrado(slug.to_string()));
        }
        projeto.salvar(&self.arquivo_de(slug))?;
        // Salvar com sucesso torna o autosave obsoleto: manter faria o app
        // oferecer "recuperar" um estado mais velho que o salvo.
        let _ = std::fs::remove_file(self.autosave_de(slug));
        Ok(())
    }

    pub fn abrir(&self, slug: &str) -> Result<Projeto, WorkspaceErro> {
        let arq = self.arquivo_de(slug);
        if !arq.is_file() {
            return Err(WorkspaceErro::NaoEncontrado(slug.to_string()));
        }
        Ok(Projeto::abrir(&arq)?)
    }

    /// Grava o autosave num arquivo separado, sem tocar no projeto.
    pub fn autosave(&self, slug: &str, projeto: &Projeto) -> Result<(), WorkspaceErro> {
        if !self.pasta_de(slug).is_dir() {
            return Err(WorkspaceErro::NaoEncontrado(slug.to_string()));
        }
        projeto.salvar(&self.autosave_de(slug))?;
        Ok(())
    }

    /// `true` quando ha trabalho nao salvo a recuperar.
    ///
    /// O sinal e a **existencia** do autosave, nao a comparacao de datas: `salvar`
    /// apaga o arquivo, entao ele so sobrevive se o app caiu antes de salvar.
    /// Comparar horario de modificacao falharia quando salvar e autossalvar
    /// acontecem dentro do mesmo segundo — a granularidade do sistema de arquivos
    /// nao e confiavel para isso.
    pub fn tem_recuperacao(&self, slug: &str) -> bool {
        self.autosave_de(slug).is_file()
    }

    /// Promove o autosave a projeto. Antes disso, guarda o atual como snapshot —
    /// recuperar nunca pode destruir o que estava salvo.
    pub fn recuperar(&self, slug: &str) -> Result<Projeto, WorkspaceErro> {
        if !self.tem_recuperacao(slug) {
            return Err(WorkspaceErro::SemRecuperacao(slug.to_string()));
        }
        let auto = Projeto::abrir(&self.autosave_de(slug))?;

        if self.arquivo_de(slug).is_file() {
            if let Ok(atual) = Projeto::abrir(&self.arquivo_de(slug)) {
                let _ = self.criar_snapshot(slug, "antes-da-recuperacao", &atual);
            }
        }
        self.salvar(slug, &auto)?;
        Ok(auto)
    }

    /// Grava um snapshot nomeado. `carimbo` vem do chamador para o modulo nao
    /// depender de relogio (e para os testes serem determinísticos).
    pub fn criar_snapshot_em(
        &self,
        slug: &str,
        carimbo: &str,
        rotulo: &str,
        projeto: &Projeto,
    ) -> Result<PathBuf, WorkspaceErro> {
        let pasta = self.pasta_snapshots(slug);
        std::fs::create_dir_all(&pasta)?;
        let arq = pasta.join(format!("{carimbo}_{}.arcz", self::slug(rotulo)));
        projeto.salvar(&arq)?;
        Ok(arq)
    }

    /// Igual, carimbando com o relogio do sistema.
    pub fn criar_snapshot(
        &self,
        slug: &str,
        rotulo: &str,
        projeto: &Projeto,
    ) -> Result<PathBuf, WorkspaceErro> {
        self.criar_snapshot_em(slug, &carimbo_agora(), rotulo, projeto)
    }

    /// Snapshots do mais novo para o mais velho.
    pub fn snapshots(&self, slug: &str) -> Vec<ResumoSnapshot> {
        let Ok(dir) = std::fs::read_dir(self.pasta_snapshots(slug)) else {
            return Vec::new();
        };
        let mut out: Vec<ResumoSnapshot> = dir
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "arcz"))
            .map(|e| {
                let p = e.path();
                let nome = p.file_stem().unwrap_or_default().to_string_lossy();
                let (carimbo, rotulo) = nome.split_once('_').unwrap_or(("", nome.as_ref()));
                ResumoSnapshot {
                    bytes: e.metadata().map(|m| m.len()).unwrap_or(0),
                    carimbo: carimbo.to_string(),
                    rotulo: rotulo.to_string(),
                    arquivo: p,
                }
            })
            .collect();
        // O carimbo e AAAAMMDD-hhmmss, entao ordem lexicografica = ordem cronologica.
        out.sort_by(|a, b| b.carimbo.cmp(&a.carimbo));
        out
    }

    /// Volta o projeto para um snapshot, guardando o estado atual antes.
    pub fn restaurar_snapshot(&self, slug: &str, arquivo: &Path) -> Result<Projeto, WorkspaceErro> {
        let alvo = Projeto::abrir(arquivo)?;
        if let Ok(atual) = Projeto::abrir(&self.arquivo_de(slug)) {
            // Restaurar tambem e reversivel.
            let _ = self.criar_snapshot(slug, "antes-de-restaurar", &atual);
        }
        self.salvar(slug, &alvo)?;
        Ok(alvo)
    }

    /// Copia o projeto inteiro sob um nome novo.
    pub fn duplicar(&self, slug: &str, novo_nome: &str) -> Result<String, WorkspaceErro> {
        let mut p = self.abrir(slug)?;
        p.nome = novo_nome.to_string();
        self.criar(&p)
    }

    /// Renomeia. O identificador em disco acompanha o nome novo.
    pub fn renomear(&self, slug: &str, novo_nome: &str) -> Result<String, WorkspaceErro> {
        let mut p = self.abrir(slug)?;
        let novo = self::slug(novo_nome);
        if novo != slug && self.pasta_de(&novo).exists() {
            return Err(WorkspaceErro::JaExiste(novo));
        }
        p.nome = novo_nome.to_string();

        if novo == slug {
            self.salvar(slug, &p)?;
            return Ok(novo);
        }
        std::fs::rename(self.pasta_de(slug), self.pasta_de(&novo))?;
        p.salvar(&self.arquivo_de(&novo))?;
        Ok(novo)
    }

    /// Move para a lixeira. **Nao apaga.**
    pub fn excluir(&self, slug: &str) -> Result<PathBuf, WorkspaceErro> {
        let origem = self.pasta_de(slug);
        if !origem.is_dir() {
            return Err(WorkspaceErro::NaoEncontrado(slug.to_string()));
        }
        // O carimbo evita colisao quando o mesmo projeto e excluido duas vezes.
        let destino = self
            .raiz
            .join("lixeira")
            .join(format!("{slug}__{}", carimbo_agora()));
        std::fs::create_dir_all(destino.parent().unwrap())?;
        std::fs::rename(&origem, &destino)?;
        Ok(destino)
    }

    pub fn lixeira(&self) -> Vec<ResumoProjeto> {
        let Ok(dir) = std::fs::read_dir(self.raiz.join("lixeira")) else {
            return Vec::new();
        };
        dir.flatten()
            .filter(|e| e.path().is_dir())
            .filter_map(|e| self.resumo_da_pasta(&e.path()))
            .collect()
    }

    /// Traz de volta da lixeira.
    pub fn restaurar_da_lixeira(&self, pasta: &Path) -> Result<String, WorkspaceErro> {
        if !pasta.is_dir() {
            return Err(WorkspaceErro::NaoEncontrado(pasta.display().to_string()));
        }
        let nome = pasta.file_name().unwrap_or_default().to_string_lossy();
        let s = nome.split("__").next().unwrap_or(&nome).to_string();

        let destino = self.pasta_de(&s);
        if destino.exists() {
            return Err(WorkspaceErro::JaExiste(s));
        }
        std::fs::rename(pasta, &destino)?;
        Ok(s)
    }

    /// Apaga a lixeira de verdade. Chamada separada e explicita, de proposito.
    pub fn esvaziar_lixeira(&self) -> Result<usize, WorkspaceErro> {
        let mut n = 0;
        if let Ok(dir) = std::fs::read_dir(self.raiz.join("lixeira")) {
            for e in dir.flatten() {
                if e.path().is_dir() {
                    std::fs::remove_dir_all(e.path())?;
                    n += 1;
                }
            }
        }
        Ok(n)
    }

    /// Catalogo para a tela inicial, do mais recente para o mais antigo.
    pub fn listar(&self) -> Vec<ResumoProjeto> {
        let Ok(dir) = std::fs::read_dir(self.raiz.join("projetos")) else {
            return Vec::new();
        };
        let mut out: Vec<ResumoProjeto> = dir
            .flatten()
            .filter(|e| e.path().is_dir())
            .filter_map(|e| self.resumo_da_pasta(&e.path()))
            .collect();
        out.sort_by(|a, b| b.modificado_em.cmp(&a.modificado_em));
        out
    }

    fn resumo_da_pasta(&self, pasta: &Path) -> Option<ResumoProjeto> {
        let arq = pasta.join("projeto.arcz");
        if !arq.is_file() {
            return None;
        }
        let s = pasta.file_name()?.to_string_lossy().to_string();
        let meta = std::fs::metadata(&arq).ok();

        // Le so o campo `nome`; abrir o projeto inteiro por item deixaria a tela
        // inicial lenta com dezenas de projetos.
        let nome = std::fs::read_to_string(&arq)
            .ok()
            .and_then(|t| nome_do_json(&t))
            .unwrap_or_else(|| s.clone());

        Some(ResumoProjeto {
            bytes: meta.as_ref().map(|m| m.len()).unwrap_or(0),
            modificado_em: modificado_em(&arq).unwrap_or(0),
            tem_miniatura: pasta.join("thumb.png").is_file(),
            recuperavel: self.tem_recuperacao(&s),
            snapshots: self.snapshots(&s).len(),
            slug: s,
            nome,
            caminho: arq,
        })
    }
}

/// Extrai `"nome"` de um JSON sem desserializar o documento inteiro.
fn nome_do_json(texto: &str) -> Option<String> {
    let i = texto.find("\"nome\"")?;
    let resto = &texto[i + 6..];
    let a = resto.find('"')? + 1;
    let b = resto[a..].find('"')? + a;
    Some(resto[a..b].to_string())
}

fn modificado_em(p: &Path) -> Option<u64> {
    std::fs::metadata(p)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// Carimbo `AAAAMMDD-hhmmss` em UTC, calculado sem dependencia de lib de data.
fn carimbo_agora() -> String {
    let s = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (dias, resto) = (s / 86_400, s % 86_400);
    let (h, mi, sec) = (resto / 3600, (resto % 3600) / 60, resto % 60);
    let (ano, mes, dia) = civil_de_dias(dias as i64);
    format!("{ano:04}{mes:02}{dia:02}-{h:02}{mi:02}{sec:02}")
}

/// Dias desde 1970-01-01 -> (ano, mes, dia). Algoritmo de Howard Hinnant.
fn civil_de_dias(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Comandos de gestao de projeto expostos na linha de comando.
///
/// Cada um imprime o resultado e devolve `true` quando o programa deve encerrar
/// sem abrir a cena (sao operacoes de catalogo, nao de render).
pub fn executar_comando(
    cmd: &ComandoProjeto,
    novo_projeto: impl FnOnce(&str) -> Projeto,
) -> Result<bool, WorkspaceErro> {
    let w = Workspace::new(Workspace::raiz_padrao())?;

    match cmd {
        ComandoProjeto::Novo(nome) => {
            let s = w.criar(&novo_projeto(nome))?;
            println!("Projeto '{nome}' criado como '{s}'.");
            println!("  {}", w.arquivo_de(&s).display());
            Ok(true)
        }
        ComandoProjeto::Duplicar(slug, nome) => {
            let novo = w.duplicar(slug, nome)?;
            println!("'{slug}' duplicado como '{novo}'.");
            Ok(true)
        }
        ComandoProjeto::Renomear(slug, nome) => {
            let novo = w.renomear(slug, nome)?;
            println!("'{slug}' renomeado para '{novo}'.");
            Ok(true)
        }
        ComandoProjeto::Snapshot(slug, rotulo) => {
            let p = w.abrir(slug)?;
            let arq = w.criar_snapshot(slug, rotulo, &p)?;
            println!("Snapshot de '{slug}' gravado:");
            println!("  {}", arq.display());
            Ok(true)
        }
        ComandoProjeto::RestaurarSnapshot(slug, carimbo) => {
            let alvo = w
                .snapshots(slug)
                .into_iter()
                .find(|s| s.carimbo.starts_with(carimbo.as_str()))
                .ok_or_else(|| WorkspaceErro::NaoEncontrado(carimbo.clone()))?;
            w.restaurar_snapshot(slug, &alvo.arquivo)?;
            println!("'{slug}' restaurado para o snapshot {}.", alvo.carimbo);
            println!("O estado anterior virou snapshot — a restauracao e reversivel.");
            Ok(true)
        }
        ComandoProjeto::Recuperar(slug) => {
            let p = w.recuperar(slug)?;
            println!("Trabalho nao salvo de '{slug}' recuperado: {}", p.nome);
            println!("O estado que estava salvo virou snapshot.");
            Ok(true)
        }
        ComandoProjeto::EsvaziarLixeira => {
            let n = w.esvaziar_lixeira()?;
            println!("{n} projeto(s) apagado(s) definitivamente da lixeira.");
            Ok(true)
        }
        ComandoProjeto::Listar => {
            let lista = w.listar();
            if lista.is_empty() {
                println!("Nenhum projeto em {}", w.raiz().display());
            } else {
                println!("Projetos em {}:", w.raiz().display());
                for p in &lista {
                    println!(
                        "  {:<28} {:>8} KB  {} snapshot(s){}",
                        p.slug,
                        p.bytes / 1024,
                        p.snapshots,
                        if p.recuperavel {
                            "  [ha trabalho nao salvo a recuperar]"
                        } else {
                            ""
                        }
                    );
                }
            }
            Ok(true)
        }
        ComandoProjeto::Lixeira => {
            let lista = w.lixeira();
            if lista.is_empty() {
                println!("Lixeira vazia.");
            } else {
                println!("Na lixeira (recuperavel):");
                for p in &lista {
                    println!("  {}  ({})", p.slug, p.caminho.display());
                }
            }
            Ok(true)
        }
        ComandoProjeto::Excluir(slug) => {
            let destino = w.excluir(slug)?;
            println!("'{slug}' foi para a lixeira, nao foi apagado:");
            println!("  {}", destino.display());
            println!("Use --lixeira para ver e --restaurar para trazer de volta.");
            Ok(true)
        }
        ComandoProjeto::Restaurar(slug) => {
            let alvo = w
                .lixeira()
                .into_iter()
                .find(|p| p.slug.starts_with(slug.as_str()))
                .ok_or_else(|| WorkspaceErro::NaoEncontrado(slug.clone()))?;
            let pasta = alvo.caminho.parent().unwrap_or(&alvo.caminho).to_path_buf();
            let voltou = w.restaurar_da_lixeira(&pasta)?;
            println!("'{voltou}' restaurado da lixeira.");
            Ok(true)
        }
        ComandoProjeto::Snapshots(slug) => {
            let lista = w.snapshots(slug);
            if lista.is_empty() {
                println!("'{slug}' nao tem snapshots.");
            } else {
                println!("Snapshots de '{slug}' (mais novo primeiro):");
                for s in &lista {
                    println!("  {}  {:<24} {:>7} KB", s.carimbo, s.rotulo, s.bytes / 1024);
                }
            }
            Ok(true)
        }
    }
}

/// Operacao de catalogo pedida na linha de comando.
#[derive(Debug, Clone, PartialEq)]
pub enum ComandoProjeto {
    Listar,
    Lixeira,
    Excluir(String),
    Restaurar(String),
    Snapshots(String),
    /// Cria um projeto com a regiao que estiver na configuracao atual.
    Novo(String),
    Duplicar(String, String),
    Renomear(String, String),
    /// Snapshot do estado salvo de um projeto.
    Snapshot(String, String),
    RestaurarSnapshot(String, String),
    /// Promove o autosave, se houver.
    Recuperar(String),
    EsvaziarLixeira,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projeto::VERSAO_FORMATO;

    fn ws(nome: &str) -> (Workspace, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "arcz-ws-{nome}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        (Workspace::new(&dir).unwrap(), dir)
    }

    fn proj(nome: &str) -> Projeto {
        Projeto {
            versao: VERSAO_FORMATO,
            nome: nome.into(),
            lat: -27.1544967,
            lon: -48.5022653,
            lado_m: 400.0,
            zoom_dem: 14,
            zoom_imagery: 18,
            mes: 3,
            dia: 21,
            hora: 15.0,
            objetos: Vec::new(),
            cameras: Vec::new(),
        }
    }

    #[test]
    fn slug_gera_nome_de_arquivo_seguro() {
        assert_eq!(slug("Zênite by Salinet"), "z-nite-by-salinet");
        assert_eq!(slug("  Projeto   Novo  "), "projeto-novo");
        // A defesa que importa: travessia de caminho nao sobrevive.
        for perigoso in ["../../etc/passwd", "..\\..\\windows", "C:/Windows"] {
            let s = slug(perigoso);
            assert!(
                !s.contains('/') && !s.contains('\\') && !s.contains(".."),
                "{s}"
            );
        }
        assert_eq!(
            slug("!@#$"),
            "projeto",
            "nome so de simbolos precisa de fallback"
        );
        assert!(slug(&"a".repeat(200)).len() <= 64);
    }

    #[test]
    fn criar_listar_e_abrir() {
        let (w, dir) = ws("crud");
        let s = w.criar(&proj("Projeto Exemplo")).unwrap();

        let lista = w.listar();
        assert_eq!(lista.len(), 1);
        assert_eq!(lista[0].nome, "Projeto Exemplo");
        assert_eq!(lista[0].slug, s);
        assert!(lista[0].bytes > 0);

        assert_eq!(w.abrir(&s).unwrap().nome, "Projeto Exemplo");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn criar_duas_vezes_o_mesmo_nome_e_recusado() {
        let (w, dir) = ws("dup");
        w.criar(&proj("Igual")).unwrap();
        assert!(matches!(
            w.criar(&proj("Igual")),
            Err(WorkspaceErro::JaExiste(_))
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn excluir_move_para_a_lixeira_sem_apagar() {
        // A regra mais importante do modulo: um clique nunca destroi trabalho.
        let (w, dir) = ws("lixeira");
        let s = w.criar(&proj("Para Excluir")).unwrap();

        let destino = w.excluir(&s).unwrap();
        assert!(w.listar().is_empty(), "sumiu da lista");
        assert!(destino.is_dir(), "mas continua existindo em disco");
        assert_eq!(w.lixeira().len(), 1);

        let voltou = w.restaurar_da_lixeira(&destino).unwrap();
        assert_eq!(voltou, s);
        assert_eq!(w.listar().len(), 1);
        assert_eq!(w.abrir(&s).unwrap().nome, "Para Excluir");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn esvaziar_lixeira_e_a_unica_coisa_que_apaga() {
        let (w, dir) = ws("esvaziar");
        let s = w.criar(&proj("Descartavel")).unwrap();
        let destino = w.excluir(&s).unwrap();

        assert_eq!(w.esvaziar_lixeira().unwrap(), 1);
        assert!(!destino.exists());
        assert!(w.lixeira().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn restaurar_da_lixeira_nao_atropela_projeto_existente() {
        let (w, dir) = ws("colisao");
        let s = w.criar(&proj("Mesmo Nome")).unwrap();
        let destino = w.excluir(&s).unwrap();
        // Cria outro com o mesmo nome enquanto o primeiro esta na lixeira.
        w.criar(&proj("Mesmo Nome")).unwrap();

        assert!(matches!(
            w.restaurar_da_lixeira(&destino),
            Err(WorkspaceErro::JaExiste(_))
        ));
        assert!(destino.is_dir(), "o da lixeira tem que continuar la");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn duplicar_cria_copia_independente() {
        let (w, dir) = ws("duplicar");
        let s = w.criar(&proj("Original")).unwrap();
        let copia = w.duplicar(&s, "Copia").unwrap();

        assert_ne!(copia, s);
        assert_eq!(w.abrir(&copia).unwrap().nome, "Copia");
        assert_eq!(w.abrir(&s).unwrap().nome, "Original", "o original mudou");

        // Mexer na copia nao afeta o original.
        let mut p = w.abrir(&copia).unwrap();
        p.lado_m = 9999.0;
        w.salvar(&copia, &p).unwrap();
        assert_eq!(w.abrir(&s).unwrap().lado_m, 400.0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn renomear_move_a_pasta_e_atualiza_o_nome() {
        let (w, dir) = ws("renomear");
        let s = w.criar(&proj("Nome Velho")).unwrap();
        let novo = w.renomear(&s, "Nome Novo").unwrap();

        assert_ne!(novo, s);
        assert!(!w.pasta_de(&s).exists(), "a pasta antiga ficou para tras");
        assert_eq!(w.abrir(&novo).unwrap().nome, "Nome Novo");
        assert_eq!(w.listar().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn autosave_nao_sobrescreve_o_projeto() {
        // Se o autosave gravasse por cima, ele destruiria o estado salvo — que e
        // exatamente o que o usuario quer preservar.
        let (w, dir) = ws("autosave");
        let s = w.criar(&proj("Trabalho")).unwrap();

        let mut rascunho = w.abrir(&s).unwrap();
        rascunho.lado_m = 1234.0;
        w.autosave(&s, &rascunho).unwrap();

        assert_eq!(w.abrir(&s).unwrap().lado_m, 400.0, "o projeto foi alterado");
        assert!(w.tem_recuperacao(&s));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recuperar_promove_o_autosave_e_guarda_o_anterior() {
        let (w, dir) = ws("recuperar");
        let s = w.criar(&proj("Caiu")).unwrap();

        let mut rascunho = w.abrir(&s).unwrap();
        rascunho.lado_m = 777.0;
        w.autosave(&s, &rascunho).unwrap();

        let recuperado = w.recuperar(&s).unwrap();
        assert_eq!(recuperado.lado_m, 777.0);
        assert_eq!(w.abrir(&s).unwrap().lado_m, 777.0);
        // O estado que existia antes virou snapshot: recuperar tambem e reversivel.
        assert!(!w.snapshots(&s).is_empty());
        // E o autosave sumiu, para nao oferecer recuperacao de novo.
        assert!(!w.tem_recuperacao(&s));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn salvar_descarta_o_autosave_obsoleto() {
        let (w, dir) = ws("salvar-limpa");
        let s = w.criar(&proj("X")).unwrap();
        w.autosave(&s, &proj("X")).unwrap();
        assert!(w.tem_recuperacao(&s));

        w.salvar(&s, &proj("X")).unwrap();
        assert!(!w.tem_recuperacao(&s), "apos salvar nao ha o que recuperar");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn snapshots_ficam_do_mais_novo_para_o_mais_velho() {
        let (w, dir) = ws("snapshots");
        let s = w.criar(&proj("Com Historico")).unwrap();

        // Carimbos explicitos: sem depender do relogio o teste fica deterministico.
        w.criar_snapshot_em(&s, "20260730-080000", "primeiro", &proj("v1"))
            .unwrap();
        w.criar_snapshot_em(&s, "20260730-090000", "segundo", &proj("v2"))
            .unwrap();
        w.criar_snapshot_em(&s, "20260730-100000", "terceiro", &proj("v3"))
            .unwrap();

        let lista = w.snapshots(&s);
        assert_eq!(lista.len(), 3);
        assert_eq!(lista[0].rotulo, "terceiro", "o mais novo tem que vir antes");
        assert_eq!(lista[2].rotulo, "primeiro");
        assert_eq!(lista[0].carimbo, "20260730-100000");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn restaurar_snapshot_volta_o_estado_e_e_reversivel() {
        // O fluxo dos passos 18-20 da entrega obrigatoria.
        let (w, dir) = ws("restaurar");
        let s = w.criar(&proj("Projeto")).unwrap();

        let mut v1 = w.abrir(&s).unwrap();
        v1.lado_m = 100.0;
        w.salvar(&s, &v1).unwrap();
        let snap = w
            .criar_snapshot_em(&s, "20260730-080000", "marco", &v1)
            .unwrap();

        // Altera depois do snapshot.
        let mut v2 = w.abrir(&s).unwrap();
        v2.lado_m = 500.0;
        w.salvar(&s, &v2).unwrap();
        assert_eq!(w.abrir(&s).unwrap().lado_m, 500.0);

        // Restaura.
        let voltou = w.restaurar_snapshot(&s, &snap).unwrap();
        assert_eq!(voltou.lado_m, 100.0);
        assert_eq!(w.abrir(&s).unwrap().lado_m, 100.0);
        // O estado de 500 virou snapshot antes de ser trocado.
        assert!(w.snapshots(&s).len() >= 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn o_resumo_conta_snapshots_e_marca_recuperacao() {
        let (w, dir) = ws("resumo");
        let s = w.criar(&proj("Com Tudo")).unwrap();
        w.criar_snapshot_em(&s, "20260730-080000", "a", &proj("a"))
            .unwrap();
        w.autosave(&s, &proj("rascunho")).unwrap();

        let r = &w.listar()[0];
        assert_eq!(r.snapshots, 1);
        assert!(r.recuperavel);
        assert!(!r.tem_miniatura);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn operacoes_em_projeto_inexistente_devolvem_erro_claro() {
        let (w, dir) = ws("ausente");
        assert!(matches!(
            w.abrir("nao-existe"),
            Err(WorkspaceErro::NaoEncontrado(_))
        ));
        assert!(matches!(
            w.excluir("nao-existe"),
            Err(WorkspaceErro::NaoEncontrado(_))
        ));
        assert!(matches!(
            w.autosave("nao-existe", &proj("x")),
            Err(WorkspaceErro::NaoEncontrado(_))
        ));
        assert!(matches!(
            w.recuperar("nao-existe"),
            Err(WorkspaceErro::SemRecuperacao(_))
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_raiz_padrao_e_absoluta() {
        let p = Workspace::raiz_padrao();
        assert!(p.is_absolute(), "raiz relativa ao cwd: {p:?}");
        assert!(p.ends_with("ARCZ"));
    }

    #[test]
    fn o_carimbo_tem_formato_ordenavel() {
        let c = carimbo_agora();
        assert_eq!(c.len(), 15, "AAAAMMDD-hhmmss: {c}");
        assert_eq!(&c[8..9], "-");
        assert!(c.chars().filter(|c| c.is_ascii_digit()).count() == 14);
        // Ano plausivel: pega erro grosseiro na conversao de epoch.
        let ano: i32 = c[0..4].parse().unwrap();
        assert!((2020..2100).contains(&ano), "ano {ano}");
    }

    #[test]
    fn a_conversao_de_data_bate_com_epocas_conhecidas() {
        assert_eq!(civil_de_dias(0), (1970, 1, 1));
        assert_eq!(civil_de_dias(19_723), (2024, 1, 1));
        // 2024 e bissexto: 29 de fevereiro tem que existir.
        assert_eq!(civil_de_dias(19_782), (2024, 2, 29));
    }

    #[test]
    fn o_nome_e_lido_sem_desserializar_o_projeto_inteiro() {
        assert_eq!(
            nome_do_json(r#"{"versao":1,"nome":"Zenite","lat":0}"#).as_deref(),
            Some("Zenite")
        );
        assert_eq!(nome_do_json("{}"), None);
        assert_eq!(nome_do_json("lixo"), None);
    }
}

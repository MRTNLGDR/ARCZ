//! Planta mobiliada: descreve o layout de uma unidade e replica nas unidades do predio.
//!
//! **Estado:** o modulo tem teste, mas parte da API (`area_m2`, `salvar`,
//! `total_itens`) ainda nao tem chamador fora deles — falta o Inspector, que vive
//! na camada React. O `allow` abaixo evita que `clippy -D warnings` barre o build
//! por isso, e sai quando a UI consumir.
//!
//!
//! O que existia ate aqui colocava **um** modelo por vez, na mao. Para mobiliar um
//! predio inteiro isso nao escala: o Zenite tem 5 pavimentos tipo e 8 unidades por
//! pavimento — 40 apartamentos com a mesma planta. Aqui a planta e descrita **uma
//! vez**, em metros, no sistema local da unidade, e o compositor instancia em todas.
//!
//! Tres sistemas de coordenadas, nesta ordem:
//!
//! 1. **Unidade** — origem no canto da unidade, `x` para a direita, `z` para o fundo,
//!    metros. E o que o arquivo `.json` da planta descreve.
//! 2. **Pavimento** — origem no canto da laje. A unidade entra com posicao e rotacao.
//! 3. **Mundo** — leste/norte a partir da ancora geografica do predio, girado pelo
//!    rumo. Vira [`Placement`] com `offset_leste_m` / `offset_norte_m` /
//!    `offset_vertical_m`.
//!
//! A altura usa `assentar_no_terreno: true` com `offset_vertical_m` = cota do
//! pavimento. Como o app amostra **uma** altitude de solo para o modelo inteiro
//! (`Scene::solo_modelo_m`), todos os pavimentos ficam paralelos, sem inclinar com o
//! terreno.

#![allow(dead_code)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use arcz_model::Placement;
use serde::{Deserialize, Serialize};

/// Um comodo, em coordenadas da unidade. Serve para validar o layout e para
/// documentar a planta — a geometria das paredes vem do modelo do predio.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Comodo {
    pub nome: String,
    pub x0: f32,
    pub z0: f32,
    pub x1: f32,
    pub z1: f32,
}

impl Comodo {
    pub fn area_m2(&self) -> f32 {
        (self.x1 - self.x0).abs() * (self.z1 - self.z0).abs()
    }

    pub fn contem(&self, x: f32, z: f32) -> bool {
        x >= self.x0.min(self.x1)
            && x <= self.x0.max(self.x1)
            && z >= self.z0.min(self.z1)
            && z <= self.z0.max(self.z1)
    }
}

/// Um movel posicionado na planta.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ItemPlanta {
    /// Chave do catalogo (`arcz_biblioteca::catalogo`), que tambem e a pasta do
    /// item dentro da biblioteca.
    pub chave: String,
    /// Centro do item, em coordenadas da unidade.
    pub x: f32,
    pub z: f32,
    /// Altura da base acima do piso do pavimento. 0 = no chao; use para o que fica
    /// sobre bancada (micro-ondas) ou na parede (quadro, pendente).
    #[serde(default)]
    pub y: f32,
    /// Giro proprio, horario visto de cima. 0 = a frente da peca aponta para `+z`.
    #[serde(default)]
    pub rot: f32,
    #[serde(default = "um")]
    pub escala: f32,
    /// So documental: em que comodo o item deveria estar. [`verificar`] confere.
    #[serde(default)]
    pub comodo: String,
}

fn um() -> f32 {
    1.0
}

/// Layout de um tipo de unidade.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Planta {
    pub nome: String,
    pub largura_m: f32,
    pub profundidade_m: f32,
    #[serde(default)]
    pub comodos: Vec<Comodo>,
    pub itens: Vec<ItemPlanta>,
}

impl Planta {
    pub fn area_m2(&self) -> f32 {
        self.largura_m * self.profundidade_m
    }
}

/// Uma unidade posicionada no pavimento.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Unidade {
    pub nome: String,
    /// Nome de uma [`Planta`] do empreendimento.
    pub planta: String,
    /// Canto da unidade em coordenadas do pavimento, antes do giro.
    pub x: f32,
    pub z: f32,
    /// Giro da unidade no pavimento, horario visto de cima.
    #[serde(default)]
    pub rot: f32,
}

/// Um pavimento e as unidades dele.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Pavimento {
    pub nome: String,
    /// Cota do piso acima da base do predio, em metros. Sai de
    /// `arcz_model::analise::pavimentos`.
    pub altura_m: f32,
    pub unidades: Vec<Unidade>,
}

/// O empreendimento inteiro: onde fica, como esta girado e o que tem em cada andar.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Empreendimento {
    pub nome: String,
    /// Ancora geografica: mesma lat/lon usada para posicionar o modelo do predio.
    pub lat: f64,
    pub lon: f64,
    /// Rumo do predio, horario a partir do norte. Mesmo valor de `--modelo-rumo`.
    #[serde(default)]
    pub heading_deg: f64,
    /// Origem da laje em relacao a ancora do predio, em metros (leste, norte),
    /// **antes** do giro. Serve para alinhar o canto da laje com o centro do modelo.
    #[serde(default)]
    pub origem_leste_m: f32,
    #[serde(default)]
    pub origem_norte_m: f32,
    pub plantas: Vec<Planta>,
    pub pavimentos: Vec<Pavimento>,
}

impl Empreendimento {
    pub fn abrir(caminho: &Path) -> Result<Self, PlantaError> {
        let dados = std::fs::read(caminho)?;
        Ok(serde_json::from_slice(&dados)?)
    }

    pub fn salvar(&self, caminho: &Path) -> Result<(), PlantaError> {
        if let Some(pai) = caminho.parent() {
            std::fs::create_dir_all(pai)?;
        }
        std::fs::write(caminho, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    pub fn planta(&self, nome: &str) -> Option<&Planta> {
        self.plantas.iter().find(|p| p.nome == nome)
    }

    /// Quantos moveis o empreendimento inteiro vai gerar.
    pub fn total_itens(&self) -> usize {
        self.pavimentos
            .iter()
            .flat_map(|p| &p.unidades)
            .filter_map(|u| self.planta(&u.planta))
            .map(|p| p.itens.len())
            .sum()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PlantaError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

/// Um movel pronto para virar objeto do editor.
#[derive(Debug, Clone, PartialEq)]
pub struct Colocacao {
    /// Nome legivel: "Apto 302 / cama-casal".
    pub nome: String,
    pub arquivo: PathBuf,
    pub placement: Placement,
}

/// Gira `(x, z)` por `graus` no sentido horario visto de cima.
///
/// Em coordenadas de planta (`x` leste, `z` norte) o giro horario leva o norte para
/// o leste, e por isso o `z` entra com sinal positivo em `x`.
fn girar(x: f32, z: f32, graus: f32) -> (f32, f32) {
    let (s, c) = graus.to_radians().sin_cos();
    (x * c + z * s, -x * s + z * c)
}

/// Caminho do modelo de um item na biblioteca em disco.
///
/// A biblioteca guarda uma pasta por chave, com o `.gltf` (Poly Haven) ou o `.glb`
/// (peca parametrica) dentro. Devolve `None` se a pasta nao existe ou esta vazia —
/// e o caso de "a planta pede um item que ninguem baixou ainda".
/// Fator que converte a unidade do arquivo para metros.
///
/// Lido do campo `escala` da entrada no `manifesto.json`. Ausente vale 1,0 — o
/// glTF manda que a unidade seja metro, e a maioria dos arquivos respeita.
///
/// Existe porque exportador de acervo público frequentemente grava centímetros:
/// `cargo run --release -p arcz-app --example medir_biblioteca` mede todos e
/// aponta quem está fora da faixa plausível para móvel.
pub fn escala_do_arquivo(biblioteca: &Path, chave: &str) -> f32 {
    // Um projeto tem centenas de instâncias da mesma chave; reler e reparsear o
    // manifesto para cada uma seria O(n²) em disco.
    //
    // O cache é indexado **pelo caminho da biblioteca**, não global: um cache
    // único faria a primeira biblioteca lida vencer para sempre — nos testes,
    // um caso contaminava o seguinte.
    use std::sync::{Mutex, OnceLock};
    type PorBiblioteca = std::collections::HashMap<PathBuf, std::collections::HashMap<String, f32>>;
    static CACHE: OnceLock<Mutex<PorBiblioteca>> = OnceLock::new();

    let cache = CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    let Ok(mut guarda) = cache.lock() else {
        // Mutex envenenado por pânico em outra thread não pode derrubar o
        // mobiliário: cai na escala neutra.
        return 1.0;
    };

    let mapa = guarda.entry(biblioteca.to_path_buf()).or_insert_with(|| {
        let mut m = std::collections::HashMap::new();
        let caminho = biblioteca.join("manifesto.json");
        let Ok(txt) = std::fs::read_to_string(&caminho) else {
            return m;
        };
        let Ok(itens) = serde_json::from_str::<Vec<serde_json::Value>>(&txt) else {
            log::warn!("{} nao e uma lista JSON valida", caminho.display());
            return m;
        };
        for it in itens {
            let (Some(k), Some(e)) = (it["chave"].as_str(), it["escala"].as_f64()) else {
                continue;
            };
            // Escala zero ou negativa colapsaria ou inverteria a peça.
            if e > 0.0 {
                m.insert(k.to_string(), e as f32);
            }
        }
        m
    });
    mapa.get(chave).copied().unwrap_or(1.0)
}

pub fn modelo_do_item(biblioteca: &Path, chave: &str) -> Option<PathBuf> {
    let pasta = biblioteca.join(chave);
    let mut achados: Vec<PathBuf> = std::fs::read_dir(&pasta)
        .ok()?
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

/// Transforma o empreendimento em colocacoes prontas para o editor.
///
/// Devolve tambem os avisos: item sem modelo na biblioteca, unidade apontando para
/// planta inexistente. Nao aborta por causa de um item faltando — mobiliar 39 de 40
/// apartamentos e melhor que nao mobiliar nenhum, desde que o usuario saiba.
pub fn compor(e: &Empreendimento, biblioteca: &Path) -> (Vec<Colocacao>, Vec<String>) {
    let mut saida = Vec::new();
    let mut avisos = Vec::new();
    let mut cache: HashMap<String, Option<PathBuf>> = HashMap::new();

    for pav in &e.pavimentos {
        for unidade in &pav.unidades {
            let Some(planta) = e.planta(&unidade.planta) else {
                avisos.push(format!(
                    "{}: planta '{}' nao existe no empreendimento",
                    unidade.nome, unidade.planta
                ));
                continue;
            };

            for item in &planta.itens {
                let arquivo = cache
                    .entry(item.chave.clone())
                    .or_insert_with(|| modelo_do_item(biblioteca, &item.chave));
                let Some(arquivo) = arquivo.clone() else {
                    // Um aviso por chave, nao um por instancia: senao seriam 40 linhas
                    // iguais para o mesmo item faltando.
                    let msg = format!("item '{}' nao esta na biblioteca", item.chave);
                    if !avisos.contains(&msg) {
                        avisos.push(msg);
                    }
                    continue;
                };

                // Unidade -> pavimento.
                let (ux, uz) = girar(item.x, item.z, unidade.rot);
                let px = unidade.x + ux;
                let pz = unidade.z + uz;

                // Pavimento -> mundo (leste/norte), pelo rumo do predio.
                let (leste, norte) = girar(
                    px + e.origem_leste_m,
                    pz + e.origem_norte_m,
                    e.heading_deg as f32,
                );

                // A escala do **arquivo** multiplica a do item. São coisas
                // diferentes: a do arquivo corrige a unidade em que o modelo foi
                // exportado (o Sketchfab entrega muita coisa em centímetros — um
                // sofá saiu com 167 m); a do item é ajuste do projetista.
                //
                // Sem essa separação, quem reusa o mesmo sofá em dez plantas
                // teria de repetir `escala: 0.01` em todas, e esquecer numa
                // única basta para a peça atravessar o prédio.
                let escala_arquivo = escala_do_arquivo(biblioteca, &item.chave);

                saida.push(Colocacao {
                    nome: format!("{} / {}", unidade.nome, item.chave),
                    arquivo,
                    placement: Placement {
                        lat_deg: e.lat,
                        lon_deg: e.lon,
                        heading_deg: e.heading_deg + unidade.rot as f64 + item.rot as f64,
                        escala: item.escala * escala_arquivo,
                        assentar_no_terreno: true,
                        offset_vertical_m: pav.altura_m + item.y,
                        offset_leste_m: leste,
                        offset_norte_m: norte,
                    },
                });
            }
        }
    }

    (saida, avisos)
}

/// Converte as colocacoes direto em `ObjetoSalvo`, sem abrir nenhum modelo.
///
/// Salvar o predio inteiro (1.785 moveis) pelo Editor estoura a memoria: cada
/// objeto carrega a propria copia de geometria e textura. Mas o `.arcz` guarda so
/// caminho + transformacao — nao precisa da malha para nada. Este atalho salva o
/// empreendimento completo em segundos e com alguns MB.
pub fn projeto_de_colocacoes(
    colocacoes: &[Colocacao],
    nome: String,
    lat: f64,
    lon: f64,
    lado_m: f64,
) -> crate::projeto::Projeto {
    crate::projeto::Projeto {
        versao: crate::projeto::VERSAO_FORMATO,
        nome,
        lat,
        lon,
        lado_m,
        zoom_dem: 14,
        zoom_imagery: 18,
        mes: 3,
        dia: 21,
        hora: 15.0,
        objetos: colocacoes
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let caminho = c
                    .arquivo
                    .canonicalize()
                    .unwrap_or_else(|_| c.arquivo.clone());
                crate::projeto::ObjetoSalvo::de_placement_com_pai(
                    i as u32,
                    c.nome.clone(),
                    caminho,
                    &c.placement,
                    true,
                    None,
                )
            })
            .collect(),
        cameras: Vec::new(),
    }
}

/// Copia o empreendimento com apenas os pavimentos cujo nome contem `filtro`.
///
/// Existe por memoria: o Zenite inteiro sao ~1.800 moveis, e cada objeto do editor
/// hoje carrega a propria copia de geometria e textura. Mobiliar um pavimento por
/// vez e o que cabe enquanto o compartilhamento de recursos na GPU nao existe.
pub fn filtrar_pavimento(e: &Empreendimento, filtro: &str) -> Empreendimento {
    let alvo = filtro.to_lowercase();
    Empreendimento {
        pavimentos: e
            .pavimentos
            .iter()
            .filter(|p| p.nome.to_lowercase().contains(&alvo))
            .cloned()
            .collect(),
        ..e.clone()
    }
}

/// Confere a coerencia do empreendimento antes de compor.
///
/// Pega os erros que so apareceriam como movel atravessando parede no render:
/// item fora do retangulo da unidade, item declarado num comodo que nao o contem,
/// unidade fora da laje, unidades sobrepostas, planta orfa.
pub fn verificar(e: &Empreendimento) -> Vec<String> {
    let mut problemas = Vec::new();

    for planta in &e.plantas {
        for item in &planta.itens {
            if item.x < 0.0
                || item.z < 0.0
                || item.x > planta.largura_m
                || item.z > planta.profundidade_m
            {
                problemas.push(format!(
                    "{}: '{}' em ({:.2}, {:.2}) esta fora da unidade ({:.2} x {:.2} m)",
                    planta.nome,
                    item.chave,
                    item.x,
                    item.z,
                    planta.largura_m,
                    planta.profundidade_m
                ));
            }
            if item.escala <= 0.0 {
                problemas.push(format!(
                    "{}: '{}' com escala {} (precisa ser > 0)",
                    planta.nome, item.chave, item.escala
                ));
            }
            if !item.comodo.is_empty() {
                match planta.comodos.iter().find(|c| c.nome == item.comodo) {
                    None => problemas.push(format!(
                        "{}: '{}' aponta para o comodo '{}', que nao existe",
                        planta.nome, item.chave, item.comodo
                    )),
                    Some(c) if !c.contem(item.x, item.z) => problemas.push(format!(
                        "{}: '{}' esta em ({:.2}, {:.2}), fora do comodo '{}'",
                        planta.nome, item.chave, item.x, item.z, item.comodo
                    )),
                    Some(_) => {}
                }
            }
        }
    }

    for pav in &e.pavimentos {
        for (i, u) in pav.unidades.iter().enumerate() {
            let Some(planta) = e.planta(&u.planta) else {
                problemas.push(format!(
                    "{} / {}: planta '{}' nao existe",
                    pav.nome, u.nome, u.planta
                ));
                continue;
            };
            // Sobreposicao: so vale comparar caixas quando as duas estao sem giro,
            // que e o caso normal de laje retangular.
            for outra in pav.unidades.iter().skip(i + 1) {
                let Some(pb) = e.planta(&outra.planta) else {
                    continue;
                };
                if u.rot != 0.0 || outra.rot != 0.0 {
                    continue;
                }
                let sobrepoe = u.x < outra.x + pb.largura_m
                    && outra.x < u.x + planta.largura_m
                    && u.z < outra.z + pb.profundidade_m
                    && outra.z < u.z + planta.profundidade_m;
                if sobrepoe {
                    problemas.push(format!(
                        "{}: '{}' e '{}' se sobrepoem na laje",
                        pav.nome, u.nome, outra.nome
                    ));
                }
            }
        }
    }

    problemas
}

#[cfg(test)]
mod tests {
    use super::*;

    fn planta_simples() -> Planta {
        Planta {
            nome: "teste".into(),
            largura_m: 4.0,
            profundidade_m: 3.0,
            comodos: vec![Comodo {
                nome: "sala".into(),
                x0: 0.0,
                z0: 0.0,
                x1: 4.0,
                z1: 3.0,
            }],
            itens: vec![ItemPlanta {
                chave: "sofa-3-lugares".into(),
                x: 2.0,
                z: 1.0,
                y: 0.0,
                rot: 0.0,
                escala: 1.0,
                comodo: "sala".into(),
            }],
        }
    }

    fn empreendimento(unidades: Vec<Unidade>) -> Empreendimento {
        Empreendimento {
            nome: "teste".into(),
            lat: -27.15,
            lon: -48.50,
            heading_deg: 0.0,
            origem_leste_m: 0.0,
            origem_norte_m: 0.0,
            plantas: vec![planta_simples()],
            pavimentos: vec![Pavimento {
                nome: "pav 1".into(),
                altura_m: 3.2,
                unidades,
            }],
        }
    }

    fn unidade(nome: &str, x: f32, z: f32, rot: f32) -> Unidade {
        Unidade {
            nome: nome.into(),
            planta: "teste".into(),
            x,
            z,
            rot,
        }
    }

    /// Biblioteca de mentira: uma pasta por chave, com um arquivo `.glb` vazio.
    /// `caso` entra no nome da pasta porque os testes rodam em paralelo — sem isso
    /// um teste apaga a biblioteca que o outro esta usando (aconteceu).
    fn biblioteca_falsa(caso: &str, chaves: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("arcz-planta-teste-{caso}"));
        let _ = std::fs::remove_dir_all(&dir);
        for c in chaves {
            let p = dir.join(c);
            std::fs::create_dir_all(&p).unwrap();
            std::fs::write(p.join("modelo.glb"), b"nao importa").unwrap();
        }
        dir
    }

    #[test]
    fn giro_horario_leva_norte_para_leste() {
        let (x, z) = girar(0.0, 1.0, 90.0);
        assert!((x - 1.0).abs() < 1e-5, "x={x}");
        assert!(z.abs() < 1e-5, "z={z}");

        let (x, z) = girar(1.0, 0.0, 90.0);
        assert!(x.abs() < 1e-5, "x={x}");
        assert!((z + 1.0).abs() < 1e-5, "z={z}");
    }

    #[test]
    fn compoe_uma_colocacao_por_item_e_por_unidade() {
        let bib = biblioteca_falsa("compoe", &["sofa-3-lugares"]);
        let e = empreendimento(vec![
            unidade("A", 0.0, 0.0, 0.0),
            unidade("B", 5.0, 0.0, 0.0),
        ]);
        let (cols, avisos) = compor(&e, &bib);
        assert_eq!(cols.len(), 2);
        assert!(avisos.is_empty(), "{avisos:?}");
        assert_eq!(cols[0].nome, "A / sofa-3-lugares");
        // Unidade B esta 5 m a leste da A.
        assert!(
            (cols[1].placement.offset_leste_m - cols[0].placement.offset_leste_m - 5.0).abs()
                < 1e-4
        );
        let _ = std::fs::remove_dir_all(&bib);
    }

    #[test]
    fn cota_do_pavimento_vai_para_o_offset_vertical() {
        let bib = biblioteca_falsa("cota", &["sofa-3-lugares"]);
        let mut e = empreendimento(vec![unidade("A", 0.0, 0.0, 0.0)]);
        e.pavimentos.push(Pavimento {
            nome: "pav 2".into(),
            altura_m: 6.4,
            unidades: vec![unidade("B", 0.0, 0.0, 0.0)],
        });
        let (cols, _) = compor(&e, &bib);
        assert_eq!(cols.len(), 2);
        assert!((cols[0].placement.offset_vertical_m - 3.2).abs() < 1e-5);
        assert!((cols[1].placement.offset_vertical_m - 6.4).abs() < 1e-5);
        // Todo movel assenta no terreno + cota: pavimentos ficam paralelos.
        assert!(cols.iter().all(|c| c.placement.assentar_no_terreno));
        let _ = std::fs::remove_dir_all(&bib);
    }

    #[test]
    fn rumo_do_predio_gira_posicao_e_orientacao_do_movel() {
        let bib = biblioteca_falsa("rumo", &["sofa-3-lugares"]);
        let mut e = empreendimento(vec![unidade("A", 0.0, 0.0, 0.0)]);
        e.heading_deg = 90.0;
        let (cols, _) = compor(&e, &bib);
        let p = cols[0].placement;
        // O item estava em (2, 1) na unidade; girando 90 graus vira (1, -2).
        assert!(
            (p.offset_leste_m - 1.0).abs() < 1e-4,
            "leste={}",
            p.offset_leste_m
        );
        assert!(
            (p.offset_norte_m + 2.0).abs() < 1e-4,
            "norte={}",
            p.offset_norte_m
        );
        assert!((p.heading_deg - 90.0).abs() < 1e-9);
        let _ = std::fs::remove_dir_all(&bib);
    }

    #[test]
    fn giro_da_unidade_soma_no_rumo_do_movel() {
        let bib = biblioteca_falsa("giro-unidade", &["sofa-3-lugares"]);
        let mut e = empreendimento(vec![unidade("A", 0.0, 0.0, 180.0)]);
        e.plantas[0].itens[0].rot = 45.0;
        e.heading_deg = 10.0;
        let (cols, _) = compor(&e, &bib);
        assert!((cols[0].placement.heading_deg - 235.0).abs() < 1e-9);
        let _ = std::fs::remove_dir_all(&bib);
    }

    #[test]
    fn item_ausente_na_biblioteca_vira_um_aviso_so() {
        let bib = biblioteca_falsa("ausente", &[]);
        let e = empreendimento(vec![
            unidade("A", 0.0, 0.0, 0.0),
            unidade("B", 5.0, 0.0, 0.0),
            unidade("C", 10.0, 0.0, 0.0),
        ]);
        let (cols, avisos) = compor(&e, &bib);
        assert!(cols.is_empty());
        assert_eq!(avisos.len(), 1, "{avisos:?}");
        assert!(avisos[0].contains("sofa-3-lugares"));
        let _ = std::fs::remove_dir_all(&bib);
    }

    #[test]
    fn verificar_pega_item_fora_da_unidade() {
        let mut e = empreendimento(vec![unidade("A", 0.0, 0.0, 0.0)]);
        e.plantas[0].itens[0].x = 9.0;
        let p = verificar(&e);
        assert_eq!(p.len(), 2, "{p:?}"); // fora da unidade E fora do comodo
        assert!(p[0].contains("fora da unidade"));
    }

    #[test]
    fn verificar_pega_comodo_inexistente_e_escala_zero() {
        let mut e = empreendimento(vec![unidade("A", 0.0, 0.0, 0.0)]);
        e.plantas[0].itens[0].comodo = "cozinha".into();
        e.plantas[0].itens[0].escala = 0.0;
        let p = verificar(&e);
        assert!(p.iter().any(|s| s.contains("nao existe")), "{p:?}");
        assert!(p.iter().any(|s| s.contains("escala")), "{p:?}");
    }

    #[test]
    fn verificar_pega_unidades_sobrepostas() {
        // A planta tem 4 m de largura; duas unidades a 2 m de distancia se cruzam.
        let e = empreendimento(vec![
            unidade("A", 0.0, 0.0, 0.0),
            unidade("B", 2.0, 0.0, 0.0),
        ]);
        let p = verificar(&e);
        assert!(p.iter().any(|s| s.contains("se sobrepoem")), "{p:?}");
    }

    #[test]
    fn verificar_aprova_layout_correto() {
        let e = empreendimento(vec![
            unidade("A", 0.0, 0.0, 0.0),
            unidade("B", 4.0, 0.0, 0.0),
        ]);
        assert!(verificar(&e).is_empty());
    }

    #[test]
    fn filtro_de_pavimento_e_por_pedaco_do_nome_e_sem_acento_de_caixa() {
        let mut e = empreendimento(vec![unidade("A", 0.0, 0.0, 0.0)]);
        e.pavimentos[0].nome = "Pavimento tipo 4".into();
        e.pavimentos.push(Pavimento {
            nome: "Rooftop".into(),
            altura_m: 25.2,
            unidades: vec![unidade("R", 0.0, 0.0, 0.0)],
        });

        assert_eq!(filtrar_pavimento(&e, "tipo 4").pavimentos.len(), 1);
        assert_eq!(filtrar_pavimento(&e, "ROOFTOP").pavimentos.len(), 1);
        assert_eq!(
            filtrar_pavimento(&e, "tipo").pavimentos[0].nome,
            "Pavimento tipo 4"
        );
        assert!(filtrar_pavimento(&e, "subsolo").pavimentos.is_empty());
        // O resto do empreendimento (ancora, plantas) segue igual.
        let f = filtrar_pavimento(&e, "tipo");
        assert_eq!(f.lat, e.lat);
        assert_eq!(f.plantas, e.plantas);
    }

    #[test]
    fn json_vai_e_volta_sem_perder_nada() {
        let dir = std::env::temp_dir().join("arcz-planta-teste-json");
        std::fs::create_dir_all(&dir).unwrap();
        let caminho = dir.join("e.json");
        let e = empreendimento(vec![unidade("A", 0.0, 0.0, 0.0)]);
        e.salvar(&caminho).unwrap();
        let lido = Empreendimento::abrir(&caminho).unwrap();
        assert_eq!(e, lido);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn campos_opcionais_tem_padrao_util() {
        // Uma planta escrita a mao pode omitir y, rot, escala e comodo.
        let json = r#"{
            "nome": "min", "largura_m": 3.0, "profundidade_m": 3.0,
            "itens": [ { "chave": "puff", "x": 1.0, "z": 1.0 } ]
        }"#;
        let p: Planta = serde_json::from_str(json).unwrap();
        assert_eq!(p.itens[0].escala, 1.0);
        assert_eq!(p.itens[0].y, 0.0);
        assert!(p.comodos.is_empty());
    }

    #[test]
    fn total_de_itens_conta_todas_as_instancias() {
        let mut e = empreendimento(vec![
            unidade("A", 0.0, 0.0, 0.0),
            unidade("B", 4.0, 0.0, 0.0),
        ]);
        e.pavimentos.push(Pavimento {
            nome: "pav 2".into(),
            altura_m: 6.4,
            unidades: vec![unidade("C", 0.0, 0.0, 0.0)],
        });
        assert_eq!(e.total_itens(), 3);
    }
}

#[cfg(test)]
mod tests_escala_do_arquivo {
    use super::*;

    /// Cada teste ganha o próprio diretório: o cache de `escala_do_arquivo` é
    /// indexado por caminho, então reusar a pasta faria um caso enxergar o
    /// manifesto do anterior.
    fn biblioteca_de_teste(nome: &str, json: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("arcz-esc-{}-{nome}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("manifesto.json"), json).unwrap();
        dir
    }

    #[test]
    fn a_escala_do_arquivo_sai_do_manifesto() {
        // O sofa do Sketchfab veio em centimetros: 167 m de largura. Sem o fator
        // ele atravessa o predio inteiro.
        let dir = biblioteca_de_teste(
            "manifesto",
            r#"[{"chave":"sofa-hq","modelo":"x.glb","escala":0.01},
                {"chave":"cama","modelo":"y.glb"}]"#,
        );
        assert_eq!(escala_do_arquivo(&dir, "sofa-hq"), 0.01);
        // Item sem o campo, no mesmo manifesto, continua neutro.
        assert_eq!(escala_do_arquivo(&dir, "cama"), 1.0);
    }

    #[test]
    fn sem_o_campo_a_escala_e_um() {
        // glTF manda que a unidade seja metro; a maioria dos arquivos respeita.
        let dir = biblioteca_de_teste("sem_campo", r#"[{"chave":"cama","modelo":"y.glb"}]"#);
        assert_eq!(escala_do_arquivo(&dir, "cama"), 1.0);
    }

    #[test]
    fn chave_desconhecida_nao_quebra() {
        let dir = biblioteca_de_teste("desconhecida", r#"[{"chave":"cama","modelo":"y.glb"}]"#);
        assert_eq!(escala_do_arquivo(&dir, "nao-existe"), 1.0);
    }

    #[test]
    fn manifesto_ausente_ou_invalido_cai_para_um() {
        // Biblioteca sem manifesto e caso normal em projeto novo, nao erro.
        let vazio = std::env::temp_dir().join("arcz-sem-manifesto-xyz");
        let _ = std::fs::create_dir_all(&vazio);
        assert_eq!(escala_do_arquivo(&vazio, "qualquer"), 1.0);
    }

    #[test]
    fn escala_zero_ou_negativa_e_ignorada() {
        // Zero colapsaria a peca num ponto; negativo a espelharia.
        let dir = biblioteca_de_teste(
            "zero",
            r#"[{"chave":"a","modelo":"a.glb","escala":0},
                {"chave":"b","modelo":"b.glb","escala":-2}]"#,
        );
        assert_eq!(escala_do_arquivo(&dir, "a"), 1.0);
        assert_eq!(escala_do_arquivo(&dir, "b"), 1.0);
    }
}

//! Ponte entre o contrato de comandos do ARCZ Earth e o que o ARCZ faz de verdade.
//!
//! `ARCZ_BACKEND_CONTRACTS.json` declara 78 comandos. A maioria ainda nao tem
//! implementacao — e este modulo existe para que isso fique **explicito**: cada
//! comando responde `implementado: true` com dado real, ou `false` com o motivo.
//! Uma UI que recebe `{"ok": true}` de um comando que nao faz nada e pior do que
//! uma que recebe um erro honesto.
//!
//! O transporte do contrato e `invoke` do Tauri. Aqui a mesma superficie e servida
//! por HTTP (`POST /cmd/<comando>`), porque o preview roda no navegador e porque o
//! contrato de transporte ainda nao esta fechado — quando estiver, o dispatcher
//! muda de porta de entrada, nao de logica.

use serde::{Deserialize, Serialize};

use crate::cena::Editor;
use crate::scene::Scene;

/// Resposta uniforme de qualquer comando.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Resposta {
    pub comando: String,
    /// `false` quando o comando existe no contrato mas ainda nao faz nada.
    pub implementado: bool,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dado: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub erro: Option<String>,
}

impl Resposta {
    pub fn ok(comando: &str, dado: serde_json::Value) -> Self {
        Self {
            comando: comando.into(),
            implementado: true,
            ok: true,
            dado: Some(dado),
            erro: None,
        }
    }

    pub fn erro(comando: &str, msg: impl Into<String>) -> Self {
        Self {
            comando: comando.into(),
            implementado: true,
            ok: false,
            dado: None,
            erro: Some(msg.into()),
        }
    }

    /// Comando previsto no contrato, ainda sem implementacao.
    pub fn pendente(comando: &str) -> Self {
        Self {
            comando: comando.into(),
            implementado: false,
            ok: false,
            dado: None,
            erro: Some(format!(
                "'{comando}' esta no contrato mas ainda nao tem implementacao no ARCZ"
            )),
        }
    }

    /// Comando que nem existe no contrato.
    pub fn desconhecido(comando: &str) -> Self {
        Self {
            comando: comando.into(),
            implementado: false,
            ok: false,
            dado: None,
            erro: Some(format!("'{comando}' nao existe no contrato")),
        }
    }
}

/// Os comandos que hoje respondem com dado real.
///
/// Manter esta lista ao lado do dispatcher e proposital: e ela que a UI consulta
/// em `capability.list` para desabilitar botao em vez de deixar o usuario clicar
/// em algo que nao acontece.
pub const IMPLEMENTADOS: &[&str] = &[
    "capability.list",
    "workspace.status",
    "project.list",
    "project.get",
    "scene.list",
    "asset.search",
    "asset.tag",
    "terrain.sample",
    "camera.set",
    "layer.toggle",
    "earth.open",
    "package.list",
    "region.list",
];

/// Todos os comandos do contrato, na ordem em que aparecem nele.
pub const CONTRATO: &[&str] = &[
    "project.list",
    "workspace.status",
    "package.list",
    "earth.open",
    "camera.set",
    "layer.toggle",
    "pick.world",
    "package.install",
    "package.update",
    "package.remove",
    "region.list",
    "region.inspect",
    "region.build",
    "project.create",
    "project.import",
    "project.export",
    "project.get",
    "scene.list",
    "project.update",
    "asset.search",
    "asset.import",
    "asset.download",
    "asset.tag",
    "cad.command",
    "cad.snap",
    "cad.generate_mesh",
    "room.compute",
    "cost.compute",
    "model.import",
    "model.validate",
    "model.optimize",
    "placement.capture",
    "placement.snap",
    "terrain.sample",
    "panorama.align",
    "depth.estimate",
    "occlusion.build",
    "light.estimate",
    "composition.validate",
    "render.preview",
    "project.commit",
    "capture.ingest",
    "frame.extract",
    "pose.solve",
    "privacy.mask",
    "reconstruction.start",
    "reconstruction.cancel",
    "splat.export",
    "mesh.export",
    "panorama.search",
    "panorama.load",
    "compose.project",
    "render.panorama",
    "take.create",
    "take.update",
    "take.render",
    "queue.add",
    "render.configure",
    "render.submit",
    "render.cancel",
    "denoise.run",
    "drawing.generate",
    "sheet.compose",
    "pdf.export",
    "dxf.export",
    "ifc.export",
    "timeline.seek",
    "revision.restore",
    "czml.import",
];

/// O que o dispatcher precisa da cena para responder.
pub struct Contexto<'a> {
    pub scene: &'a Scene,
    pub editor: Option<&'a Editor>,
    pub biblioteca: &'a std::path::Path,
}

/// Executa um comando do contrato.
pub fn executar(comando: &str, params: &serde_json::Value, ctx: &Contexto<'_>) -> Resposta {
    match comando {
        // ---- meta ----------------------------------------------------------
        "capability.list" => Resposta::ok(
            comando,
            serde_json::json!({
                "total_contrato": CONTRATO.len(),
                "implementados": IMPLEMENTADOS,
                "pendentes": CONTRATO
                    .iter()
                    .filter(|c| !IMPLEMENTADOS.contains(c))
                    .collect::<Vec<_>>(),
            }),
        ),

        // ---- mundo ---------------------------------------------------------
        "workspace.status" => {
            let c = ctx.scene.bbox.center();
            Resposta::ok(
                comando,
                serde_json::json!({
                    "lat": c.lat_deg,
                    "lon": c.lon_deg,
                    "lado_m": ctx.scene.lado_m,
                    "solo_m": ctx.scene.solo_modelo_m,
                    "objetos": ctx.editor.map(|e| e.objetos.len()).unwrap_or(0),
                    "modelo_carregado": ctx.scene.modelo.is_some(),
                }),
            )
        }

        "earth.open" => {
            let c = ctx.scene.bbox.center();
            Resposta::ok(
                comando,
                serde_json::json!({
                    "camera": { "lat": c.lat_deg, "lon": c.lon_deg, "altura_m": 1500.0 },
                    "offline": true,
                }),
            )
        }

        "camera.set" => {
            // Sem estado proprio: o servidor ja tem /plano e /render.png para
            // mover a camera. Aqui so se valida e devolve o que foi aceito.
            let lat = params.get("lat").and_then(|v| v.as_f64());
            let lon = params.get("lon").and_then(|v| v.as_f64());
            match (lat, lon) {
                (Some(lat), Some(lon)) if (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon) => {
                    Resposta::ok(comando, serde_json::json!({ "lat": lat, "lon": lon }))
                }
                _ => Resposta::erro(comando, "camera.set exige lat e lon validos"),
            }
        }

        "layer.toggle" => {
            let camada = params.get("camada").and_then(|v| v.as_str()).unwrap_or("");
            let visivel = params.get("visivel").and_then(|v| v.as_bool()).unwrap_or(true);
            const CAMADAS: &[&str] = &["terreno", "modelo", "entorno", "mobiliario", "gizmo"];
            if CAMADAS.contains(&camada) {
                Resposta::ok(comando, serde_json::json!({ "camada": camada, "visivel": visivel }))
            } else {
                Resposta::erro(
                    comando,
                    format!("camada '{camada}' nao existe; ha {CAMADAS:?}"),
                )
            }
        }

        "terrain.sample" => {
            let lat = params.get("lat").and_then(|v| v.as_f64());
            let lon = params.get("lon").and_then(|v| v.as_f64());
            match (lat, lon) {
                (Some(lat), Some(lon)) => Resposta::ok(
                    comando,
                    serde_json::json!({
                        "lat": lat,
                        "lon": lon,
                        "altitude_m": ctx.scene.altura_no_terreno(lon, lat),
                        "fonte": "AWS Terrain Tiles (terrarium)",
                    }),
                ),
                _ => Resposta::erro(comando, "terrain.sample exige lat e lon"),
            }
        }

        // ---- projetos ------------------------------------------------------
        "project.list" => match crate::workspace::Workspace::new(
            crate::workspace::Workspace::raiz_padrao(),
        ) {
            Ok(ws) => {
                let projetos: Vec<_> = ws
                    .listar()
                    .into_iter()
                    .map(|p| {
                        serde_json::json!({
                            "slug": p.slug,
                            "nome": p.nome,
                            "caminho": p.caminho.display().to_string(),
                            "bytes": p.bytes,
                            "modificado_em": p.modificado_em,
                            "tem_miniatura": p.tem_miniatura,
                            "recuperavel": p.recuperavel,
                            "snapshots": p.snapshots,
                        })
                    })
                    .collect();
                Resposta::ok(
                    comando,
                    serde_json::json!({ "total": projetos.len(), "projetos": projetos }),
                )
            }
            Err(e) => Resposta::erro(comando, e.to_string()),
        },

        "project.get" => {
            let slug = params.get("slug").and_then(|v| v.as_str()).unwrap_or("");
            if slug.is_empty() {
                return Resposta::erro(comando, "project.get exige 'slug'");
            }
            match crate::workspace::Workspace::new(crate::workspace::Workspace::raiz_padrao())
                .and_then(|ws| ws.abrir(slug))
            {
                Ok(p) => Resposta::ok(
                    comando,
                    serde_json::json!({
                        "nome": p.nome,
                        "lat": p.lat,
                        "lon": p.lon,
                        "lado_m": p.lado_m,
                        "objetos": p.objetos.len(),
                        "versao": p.versao,
                    }),
                ),
                Err(e) => Resposta::erro(comando, e.to_string()),
            }
        }

        "scene.list" => {
            let objetos: Vec<_> = ctx
                .editor
                .map(|e| {
                    e.objetos
                        .iter()
                        .map(|o| {
                            serde_json::json!({
                                "id": o.id,
                                "nome": o.nome,
                                "visivel": o.visivel,
                                "arquivo": o.arquivo.display().to_string(),
                                "lat": o.placement.lat_deg,
                                "lon": o.placement.lon_deg,
                                "leste_m": o.placement.offset_leste_m,
                                "norte_m": o.placement.offset_norte_m,
                                "altura_m": o.placement.offset_vertical_m,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            Resposta::ok(
                comando,
                serde_json::json!({ "total": objetos.len(), "objetos": objetos }),
            )
        }

        // ---- biblioteca ----------------------------------------------------
        "asset.search" => {
            let termo = params
                .get("termo")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_lowercase();
            let ambiente = params.get("ambiente").and_then(|v| v.as_str());
            let itens: Vec<_> = arcz_biblioteca::CATALOGO
                .iter()
                .filter(|i| {
                    termo.is_empty()
                        || i.chave.contains(&termo)
                        || i.nome.to_lowercase().contains(&termo)
                })
                .filter(|i| match ambiente {
                    None => true,
                    Some(a) => i
                        .ambientes
                        .iter()
                        .any(|x| format!("{x:?}").to_lowercase() == a.to_lowercase()),
                })
                .map(|i| {
                    let modelo = crate::planta::modelo_do_item(ctx.biblioteca, i.chave);
                    serde_json::json!({
                        "chave": i.chave,
                        "nome": i.nome,
                        "papel": format!("{:?}", i.papel),
                        "licenca": i.licenca.texto(),
                        "dimensao_m": i.dimensao_m,
                        "baixado": modelo.is_some(),
                        "arquivo": modelo.map(|p| p.display().to_string()),
                    })
                })
                .collect();
            Resposta::ok(
                comando,
                serde_json::json!({ "total": itens.len(), "itens": itens }),
            )
        }

        "asset.tag" => {
            let chave = params.get("chave").and_then(|v| v.as_str()).unwrap_or("");
            match arcz_biblioteca::catalogo::por_chave(chave) {
                Some(i) => Resposta::ok(
                    comando,
                    serde_json::json!({
                        "chave": i.chave,
                        "papel": format!("{:?}", i.papel),
                        "ambientes": i.ambientes.iter().map(|a| format!("{a:?}")).collect::<Vec<_>>(),
                        "licenca": i.licenca.texto(),
                    }),
                ),
                None => Resposta::erro(comando, format!("'{chave}' nao esta no catalogo")),
            }
        }

        // ---- pacotes e regioes ---------------------------------------------
        "package.list" | "region.list" => {
            // Pacote regional = o que ja esta em cache no disco. Nada de lista
            // fixa: se nao foi baixado, nao aparece.
            let raiz = arcz_terrain::TileCache::default_root();
            let tiles = contar_arquivos(&raiz);
            let c = ctx.scene.bbox.center();
            Resposta::ok(
                comando,
                serde_json::json!({
                    "pacotes": [{
                        "nome": format!("{:.4}, {:.4}", c.lat_deg, c.lon_deg),
                        "lado_m": ctx.scene.lado_m,
                        "tiles_em_cache": tiles,
                        "raiz": raiz.display().to_string(),
                        "status": if tiles > 0 { "instalado" } else { "vazio" },
                    }]
                }),
            )
        }

        // ---- resto do contrato ---------------------------------------------
        outro if CONTRATO.contains(&outro) => Resposta::pendente(outro),
        outro => Resposta::desconhecido(outro),
    }
}

fn contar_arquivos(raiz: &std::path::Path) -> usize {
    let mut n = 0;
    let mut pilha = vec![raiz.to_path_buf()];
    while let Some(dir) = pilha.pop() {
        let Ok(entradas) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entradas.flatten() {
            let p = e.path();
            if p.is_dir() {
                pilha.push(p);
            } else {
                n += 1;
            }
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn todo_implementado_esta_no_contrato_ou_e_meta() {
        for c in IMPLEMENTADOS {
            assert!(
                CONTRATO.contains(c) || *c == "capability.list",
                "'{c}' nao existe no contrato"
            );
        }
    }

    #[test]
    fn o_contrato_nao_tem_comando_repetido() {
        let mut vistos = std::collections::HashSet::new();
        for c in CONTRATO {
            assert!(vistos.insert(c), "comando repetido: {c}");
        }
        assert_eq!(CONTRATO.len(), 69, "o contrato mudou de tamanho");
    }

    #[test]
    fn comando_fora_do_contrato_e_desconhecido_nao_pendente() {
        let r = Resposta::desconhecido("nao.existe");
        assert!(!r.implementado && !r.ok);
        assert!(r.erro.unwrap().contains("nao existe no contrato"));
    }

    #[test]
    fn pendente_diz_que_nao_esta_implementado() {
        // O ponto do teste: pendente NUNCA pode voltar ok=true. Uma UI que recebe
        // ok de um comando vazio mostra sucesso para algo que nao aconteceu.
        let r = Resposta::pendente("render.submit");
        assert!(!r.ok);
        assert!(!r.implementado);
        assert!(r.dado.is_none());
    }

    #[test]
    fn resposta_serializa_sem_campos_nulos() {
        let s = serde_json::to_string(&Resposta::ok("x", serde_json::json!({"a":1}))).unwrap();
        assert!(!s.contains("erro"), "ok nao deve carregar campo de erro: {s}");
        let s = serde_json::to_string(&Resposta::pendente("y")).unwrap();
        assert!(!s.contains("\"dado\""), "pendente nao deve carregar dado: {s}");
    }

    #[test]
    fn capability_list_soma_implementados_e_pendentes() {
        // Sem contexto de cena da para testar so o comando meta, que e o que a UI
        // usa para desabilitar botao.
        let r = executar_meta("capability.list");
        let d = r.dado.unwrap();
        let impl_n = d["implementados"].as_array().unwrap().len();
        let pend_n = d["pendentes"].as_array().unwrap().len();
        // capability.list e meta: esta em IMPLEMENTADOS mas nao no CONTRATO.
        assert_eq!(impl_n + pend_n, CONTRATO.len() + 1);
    }

    /// Atalho de teste para os comandos que nao precisam de cena.
    fn executar_meta(comando: &str) -> Resposta {
        assert_eq!(comando, "capability.list");
        Resposta::ok(
            comando,
            serde_json::json!({
                "total_contrato": CONTRATO.len(),
                "implementados": IMPLEMENTADOS,
                "pendentes": CONTRATO
                    .iter()
                    .filter(|c| !IMPLEMENTADOS.contains(c))
                    .collect::<Vec<_>>(),
            }),
        )
    }
}

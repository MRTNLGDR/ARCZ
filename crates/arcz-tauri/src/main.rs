//! Casca Tauri do ARCZ.
//!
//! Este binario existe para **medir o risco do ADR-0002** antes que a UI seja
//! construida em cima: ele sobe uma janela Tauri, cria a superficie wgpu sobre o
//! handle dela, desenha um quadro solido e relata o que aconteceu.
//!
//! O que se quer saber e uma coisa so: a superficie nativa e o webview convivem
//! na mesma janela, e em que ordem? Se nao conviverem, o plano B do ADR-0002 e
//! janela separada — e e melhor descobrir agora que depois do Outliner pronto.

mod renderer_bridge;
mod superficie;

use std::sync::Arc;

use renderer_bridge::{ui_to_renderer, RendererBridgeState};
use superficie::{RelatorioSuperficie, Superficie};
use tauri::{Manager, WebviewWindow};

/// Cor de prova do teste de composicao.
const AZUL: wgpu::Color = wgpu::Color {
    r: 0.05,
    g: 0.10,
    b: 0.22,
    a: 1.0,
};

/// Resultado do teste de composicao, devolvido a UI.
#[tauri::command]
fn superficie_status(estado: tauri::State<'_, EstadoApp>) -> RelatorioSuperficie {
    estado.relatorio.clone()
}

/// Onde o engine deve desenhar, em pixels fisicos. Chamado pela UI no mount e a
/// cada mudanca de layout — ver `docs/integrations/UI_ENGINE_CONTRACT.md`.
#[tauri::command]
fn viewport_area(x: u32, y: u32, largura: u32, altura: u32) -> bool {
    log::info!("viewport_area: {largura}x{altura} em ({x}, {y})");
    true
}

struct EstadoApp {
    relatorio: RelatorioSuperficie,
    superficie: std::sync::Mutex<Option<Superficie>>,
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let bridge_state = Arc::new(RendererBridgeState::new());

    tauri::Builder::default()
        .manage(bridge_state)
        .invoke_handler(tauri::generate_handler![superficie_status, viewport_area, ui_to_renderer])

        .setup(|app| {
            let janela: WebviewWindow = app
                .get_webview_window("main")
                .ok_or("janela 'main' nao encontrada")?;

            let tamanho = janela.inner_size()?;
            let janela = Arc::new(janela);

            let (relatorio, superficie) =
                match Superficie::nova(janela.clone(), tamanho.width, tamanho.height) {
                    Ok(mut s) => {
                        // Azul-escuro: se aparecer na janela, a superficie esta por
                        // cima do webview. Se nao aparecer, esta por baixo — e o dado
                        // que decide o layout da UI.
                        let _ = s.limpar(AZUL);
                        (s.relatorio.clone(), Some(s))
                    }
                    Err(e) => {
                        log::error!("falha ao criar a superficie: {e:#}");
                        (RelatorioSuperficie::falha(e), None)
                    }
                };

            println!();
            println!("=== ADR-0002 / teste de composicao ===");
            if relatorio.ok {
                println!("  superficie wgpu criada sobre a janela Tauri");
                println!("  adaptador : {}", relatorio.adaptador);
                println!("  backend   : {}", relatorio.backend);
                println!("  formato   : {}", relatorio.formato);
                println!("  tamanho   : {}x{}", relatorio.largura, relatorio.altura);
                println!();
                println!("  OLHE A JANELA:");
                println!("    - fundo AZUL-ESCURO  -> superficie por cima do webview (ideal)");
                println!("    - texto HTML visivel -> superficie por baixo (usar plano B)");
            } else {
                println!("  FALHOU: {}", relatorio.erro.clone().unwrap_or_default());
                println!("  Plano B do ADR-0002: janela nativa separada para o viewport.");
            }
            println!();

            app.manage(EstadoApp {
                relatorio,
                superficie: std::sync::Mutex::new(superficie),
            });

            // Reconfigura a surface quando a janela muda de tamanho. Sem isso a
            // imagem estica e o wgpu passa a reclamar de tamanho incompativel.
            let alca = app.handle().clone();
            janela.on_window_event(move |evento| {
                if let tauri::WindowEvent::Resized(t) = evento {
                    // O `State` vive so dentro deste escopo; travar o mutex a partir
                    // dele exige manter o guarda na mesma expressao.
                    if let Ok(mut guarda) = alca.state::<EstadoApp>().superficie.lock() {
                        if let Some(s) = guarda.as_mut() {
                            s.redimensionar(t.width, t.height);
                            let _ = s.limpar(AZUL);
                        }
                    }
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("erro ao subir a janela Tauri");
}

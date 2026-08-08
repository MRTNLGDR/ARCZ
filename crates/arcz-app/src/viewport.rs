//! Viewport wgpu + winit: desenha o terreno e o modelo do usuario numa janela.

use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use crate::camera::OrbitCamera;
use crate::cena::{Editor, Historico};
use crate::gizmo::{self, AlcaId, ModoGizmo};
use crate::gpu::{criar_depth, Globais, Recursos, FUNDO};
use crate::iluminacao::Momento;
use crate::renderer;
use crate::scene::Scene;

/// Estado de um drag de gizmo em andamento. Guarda o ponto inicial no mundo
/// (do pixel no momento do press) e a placement original, para calcular o
/// delta a cada movimento e aplicar de volta no objeto.
struct DragEstado {
    alca: AlcaId,
    /// Coordenada do pixel (NDC) onde o Press aconteceu.
    ndc_inicial: (f64, f64),
    /// Coordenada do pixel (NDC) atual, atualizada a cada CursorMoved.
    ndc_atual: (f64, f64),
}

/// Abre a janela e roda ate o usuario fechar.
pub fn run(scene: Scene, editor: Option<Editor>, vista: [f32; 4]) -> anyhow::Result<()> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::new(scene, editor, vista);
    event_loop.run_app(&mut app)?;
    Ok(())
}

/// Enquadra o modelo (legado `--modelo`) ou os objetos do Editor, ou o terreno.
///
/// Com um predio de 50 m dentro de uma area de 8 km, abrir enquadrando o terreno
/// deixaria o modelo com poucos pixels. Prioridade: 1) modelo legado, 2) caixa
/// dos objetos do Editor, 3) terreno inteiro.
pub fn camera_inicial(scene: &Scene, editor: Option<&Editor>) -> OrbitCamera {
    if let Some(m) = &scene.modelo {
        let t = m.tamanho_real_m;
        return OrbitCamera::enquadrando(m.center(), t[0].max(t[1]).max(t[2]).max(5.0));
    }
    if let Some(ed) = editor {
        if let Some((min, max)) = ed.caixa_total() {
            let centro = [
                (min[0] + max[0]) * 0.5,
                (min[1] + max[1]) * 0.5,
                (min[2] + max[2]) * 0.5,
            ];
            let ext = (max[0] - min[0]).max(max[1] - min[1]).max(max[2] - min[2]);
            return OrbitCamera::enquadrando(centro, ext.max(5.0));
        }
    }
    OrbitCamera::enquadrando(scene.mesh.center(), scene.mesh.horizontal_extent())
}

struct App {
    /// Corte e estilo da vista, repassados ao `Estado` quando a janela abre.
    vista: [f32; 4],
    scene: Scene,
    editor: Option<Editor>,
    estado: Option<Estado>,
    camera: OrbitCamera,
    arrastando: bool,
    cursor: Option<(f64, f64)>,
    /// Posicao do click esquerdo no Pressed, para distinguir click de drag.
    /// Se Released vier com diff < 5 px, conta como click (picking).
    click_inicio: Option<(f64, f64)>,
    /// Coordenada exata do pixel onde o Released aconteceu (sem reset).
    /// Usada para construir o raio do picking depois que `self.cursor` foi zerado.
    click_pixel: Option<(f64, f64)>,
    /// Drag de gizmo em andamento. Quando Some, o CursorMoved aplica delta
    /// no objeto selecionado.
    drag: Option<DragEstado>,
    /// Historico de comandos para Ctrl+Z / Ctrl+Shift+Z. Sem owner (mut
    /// separado do Editor) porque o `self` nao permite dois &mut.
    historico: Historico,
}

impl App {
    fn new(scene: Scene, editor: Option<Editor>, vista: [f32; 4]) -> Self {
        let camera = camera_inicial(&scene, editor.as_ref());
        Self {
            vista,
            scene,
            editor,
            estado: None,
            camera,
            arrastando: false,
            cursor: None,
            click_inicio: None,
            click_pixel: None,
            drag: None,
            historico: Historico::novo(),
        }
    }

    /// Largura de janela: preciso dela pra converter pixel em NDC. Cacheia.
    #[allow(dead_code)]
    fn tamanho_janela(&self) -> (u32, u32) {
        self.estado
            .as_ref()
            .map(|e| (e.config.width, e.config.height))
            .unwrap_or((1440, 900))
    }

    /// Click esquerdo sem drag → raycast contra os AABBs do Editor e seleciona
    /// o objeto mais proximo. Atualiza o gizmo (caixa amarela) sobre o escolhido.
    /// Clicar no vazio desseleciona.
    fn tratar_click_selecao(
        camera: &OrbitCamera,
        editor: &mut Editor,
        click_pixel: Option<(f64, f64)>,
        estado: &mut Estado,
    ) {
        let (w, h) = (estado.config.width as f64, estado.config.height as f64);
        if w < 1.0 || h < 1.0 {
            return;
        }
        let click = match click_pixel {
            Some(p) => p,
            None => return,
        };
        let ndc_x = (click.0 / w) * 2.0 - 1.0;
        // Y do winit cresce pra baixo; NDC cresce pra cima.
        let ndc_y = 1.0 - (click.1 / h) * 2.0;
        let aspect = w / h;
        let (origem, direcao) =
            match renderer::Renderer::raio_da_camera(camera, aspect, ndc_x, ndc_y) {
                Some(r) => r,
                None => return,
            };
        let id = editor.picar(origem, direcao);
        editor.selecionado = id;
        let linhas = gizmo_para_selecao(editor, camera);
        estado.recursos.atualizar_gizmo(&estado.device, &linhas);
    }

    /// Recalcula o gizmo (caixa amarela) sobre o objeto selecionado, ou limpa
    /// o gizmo se nao ha selecao.
    #[allow(dead_code)]
    fn atualizar_gizmo_selecao(&self, editor: &Editor, estado: &mut Estado) {
        let linhas = gizmo_para_selecao(editor, &self.camera);
        estado.recursos.atualizar_gizmo(&estado.device, &linhas);
    }

    /// Versao sem self: usada depois de Esc/Delete quando o borrow de self ja
    /// segurou o `editor`. Calcula o gizmo do selecionado e atualiza a GPU.
    fn atualizar_gizmo_selecao_estatico(
        editor: Option<&Editor>,
        camera: &OrbitCamera,
        estado: &mut Estado,
    ) {
        let linhas = match editor {
            Some(e) => gizmo_para_selecao(e, camera),
            None => Vec::new(),
        };
        estado.recursos.atualizar_gizmo(&estado.device, &linhas);
    }

    /// Aplica undo/redo. Versao estatico pra evitar borrow conflicts entre
    /// `editor` e `estado`. Detalhes:
    /// - Desfaz: pega o ultimo comando, reverte, atualiza a matriz do objeto
    ///   movido na GPU.
    /// - Redo: nao implementado nessa versao (workaround de modificadores).
    #[allow(dead_code)]
    fn aplicar_undo_redo(
        editor: Option<&mut Editor>,
        historico: &mut Historico,
        scene: &Scene,
        estado: &mut Estado,
    ) -> bool {
        let Some(editor) = editor else {
            return false;
        };
        let id = match editor.selecionado() {
            Some(o) => o.id,
            None => return false,
        };
        if !historico.desfazer(editor) {
            return false;
        }
        if let Some(obj) = editor.get(id) {
            let solo = scene.altura_no_terreno(obj.placement.lon_deg, obj.placement.lat_deg);
            let matriz = arcz_model::matriz_modelo(
                obj.fonte.min,
                obj.fonte.max,
                &scene.frame,
                &obj.placement,
                solo,
            );
            estado
                .recursos
                .atualizar_transform(&estado.queue, id, matriz);
        }
        estado.window.request_redraw();
        true
    }
}

fn gizmo_para_selecao(editor: &Editor, camera: &OrbitCamera) -> Vec<crate::gizmo::VerticeLinha> {
    let Some(obj) = editor.selecionado() else {
        return Vec::new();
    };
    let centro = obj.centro();
    let escala = (camera.distancia * 0.12).clamp(1.0, 500.0) as f32;
    let mut v = Vec::new();
    gizmo_linha_caixa(
        &mut v,
        [obj.min_enu[0], obj.min_enu[1], obj.min_enu[2]],
        [obj.max_enu[0], obj.max_enu[1], obj.max_enu[2]],
        [1.0, 0.72, 0.15, 1.0],
    );
    for (dir, cor) in [
        ([1.0, 0.0, 0.0], [0.95, 0.26, 0.28, 1.0]),
        ([0.0, 1.0, 0.0], [0.42, 0.85, 0.30, 1.0]),
        ([0.0, 0.0, 1.0], [0.30, 0.58, 0.98, 1.0]),
    ] {
        let ponta = [
            centro[0] + dir[0] * escala,
            centro[1] + dir[1] * escala,
            centro[2] + dir[2] * escala,
        ];
        gizmo_linha(&mut v, centro, ponta, cor);
    }
    v
}

fn gizmo_linha(v: &mut Vec<crate::gizmo::VerticeLinha>, a: [f32; 3], b: [f32; 3], cor: [f32; 4]) {
    v.push(crate::gizmo::VerticeLinha { position: a, cor });
    v.push(crate::gizmo::VerticeLinha { position: b, cor });
}

fn gizmo_linha_caixa(
    v: &mut Vec<crate::gizmo::VerticeLinha>,
    min: [f32; 3],
    max: [f32; 3],
    cor: [f32; 4],
) {
    let c = [
        [min[0], min[1], min[2]],
        [max[0], min[1], min[2]],
        [max[0], min[1], max[2]],
        [min[0], min[1], max[2]],
        [min[0], max[1], min[2]],
        [max[0], max[1], min[2]],
        [max[0], max[1], max[2]],
        [min[0], max[1], max[2]],
    ];
    for (a, b) in [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ] {
        gizmo_linha(v, c[a], c[b], cor);
    }
}

impl App {
    /// Constrói o gizmo do objeto selecionado no estado atual e testa se o raio
    /// do mouse atual (cursor) acertou uma alça. Devolve a alça se sim.
    fn picking_alca_gizmo(&self, w: u32, h: u32) -> Option<AlcaId> {
        let editor = self.editor.as_ref()?;
        let obj = editor.selecionado()?;
        let centro = obj.centro();
        let g = gizmo::construir_com_alcas(
            centro,
            [obj.min_enu[0], obj.min_enu[1], obj.min_enu[2]],
            [obj.max_enu[0], obj.max_enu[1], obj.max_enu[2]],
            ModoGizmo::Mover,
            self.camera.distancia,
        );
        let pixel = self.click_pixel?;
        let (w, h) = (w as f64, h as f64);
        let ndc_x = (pixel.0 / w) * 2.0 - 1.0;
        let ndc_y = 1.0 - (pixel.1 / h) * 2.0;
        let (origem, direcao) =
            renderer::Renderer::raio_da_camera(&self.camera, w / h, ndc_x, ndc_y)?;
        gizmo::picar_alca(&g.alcas, origem, direcao)
    }

    /// Inicia um drag: guarda o NDC inicial e o NDC atual (= inicial).
    fn iniciar_drag(&mut self, alca: AlcaId, w: u32, h: u32) {
        let Some(pixel) = self.click_pixel else {
            return;
        };
        let (w, h) = (w as f64, h as f64);
        let ndc = ((pixel.0 / w) * 2.0 - 1.0, 1.0 - (pixel.1 / h) * 2.0);
        self.drag = Some(DragEstado {
            alca,
            ndc_inicial: ndc,
            ndc_atual: ndc,
        });
    }

    /// Aplica o delta do drag no objeto. Projeta o NDC inicial e o NDC atual
    /// no plano do eixo escolhido (X→plano YZ, Y→plano XZ, Z→plano XY),
    /// ambos passando pelo centro do objeto, e usa a diferença em metros
    /// para ajustar o offset no placement.
    fn aplicar_drag_mover(
        alca: AlcaId,
        camera: &OrbitCamera,
        editor: &mut Editor,
        drag: &Option<DragEstado>,
        scene: &Scene,
        estado: &mut Estado,
    ) {
        let Some(d) = drag else { return };
        let Some(obj) = editor.selecionado_mut() else {
            return;
        };
        let centro = obj.centro();
        // Eixo: 0=X, 1=Y, 2=Z (coincide com min_enu/max_enu/placement offsets).
        let eixo = match alca {
            AlcaId::X => 0,
            AlcaId::Y => 1,
            AlcaId::Z => 2,
        };
        // Projeta 2 raios (NDC inicial e NDC atual) no plano perpendicular ao
        // eixo, passando pelo centro. O delta no eixo e a translacao aplicada.
        let aspecto = estado.config.width as f64 / estado.config.height.max(1) as f64;
        let Some(p0) =
            renderer::Renderer::raio_da_camera(camera, aspecto, d.ndc_inicial.0, d.ndc_inicial.1)
        else {
            return;
        };
        let Some(p1) =
            renderer::Renderer::raio_da_camera(camera, aspecto, d.ndc_atual.0, d.ndc_atual.1)
        else {
            return;
        };
        // Projeta os 2 raios no plano. Devolve o t; o delta e
        // (ponto1 - ponto0) no eixo.
        let t0 = projetar_no_eixo(p0.0, p0.1, centro[eixo] as f64, eixo);
        let t1 = projetar_no_eixo(p1.0, p1.1, centro[eixo] as f64, eixo);
        let ponto0 = [
            p0.0[0] + t0 * p0.1[0],
            p0.0[1] + t0 * p0.1[1],
            p0.0[2] + t0 * p0.1[2],
        ];
        let ponto1 = [
            p1.0[0] + t1 * p1.1[0],
            p1.0[1] + t1 * p1.1[1],
            p1.0[2] + t1 * p1.1[2],
        ];
        let delta_eixo = ponto1[eixo] - ponto0[eixo];
        // Aplica no offset correto do placement.
        match alca {
            AlcaId::X => obj.placement.offset_leste_m += delta_eixo as f32,
            AlcaId::Y => obj.placement.offset_vertical_m += delta_eixo as f32,
            AlcaId::Z => obj.placement.offset_norte_m += delta_eixo as f32,
        }
        // Atualiza a matriz do objeto na GPU.
        let solo = scene.altura_no_terreno(obj.placement.lon_deg, obj.placement.lat_deg);
        let matriz = arcz_model::matriz_modelo(
            obj.fonte.min,
            obj.fonte.max,
            &scene.frame,
            &obj.placement,
            solo,
        );
        estado
            .recursos
            .atualizar_transform(&estado.queue, obj.id, matriz);
        // Recalcula a bounding box do objeto (mudou) e o gizmo.
        let t = arcz_model::transformar(&obj.fonte, &scene.frame, &obj.placement, solo);
        obj.min_enu = t.min_enu;
        obj.max_enu = t.max_enu;
        let linhas = gizmo_para_selecao(editor, camera);
        estado.recursos.atualizar_gizmo(&estado.device, &linhas);
    }
}

/// Acha o t do raio (origem + t*dir) tal que o ponto fica no plano perpendicular
/// ao eixo `eixo` passando por `valor`. Devolve o t, nao a coordenada, porque
/// o caller usa o t pra projetar no eixo.
/// Se o raio for paralelo ao plano (componente direcao[eixo] muito pequena),
/// devolve 0.
fn projetar_no_eixo(origem: [f64; 3], direcao: [f64; 3], valor: f64, eixo: usize) -> f64 {
    let d = direcao[eixo];
    if d.abs() < 1e-9 {
        return 0.0;
    }
    (valor - origem[eixo]) / d
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.estado.is_some() {
            return;
        }
        let c = self.scene.bbox.center();
        let atributos = Window::default_attributes()
            .with_title(format!(
                "ARCZ — {:.5}, {:.5}  ({:.1} km)",
                c.lat_deg,
                c.lon_deg,
                self.scene.mesh.horizontal_extent() / 1000.0,
            ))
            .with_inner_size(winit::dpi::LogicalSize::new(1440, 900));

        let window = match event_loop.create_window(atributos) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                log::error!("nao foi possivel criar a janela: {e}");
                event_loop.exit();
                return;
            }
        };

        match pollster::block_on(Estado::new(
            window,
            &self.scene,
            self.editor.as_ref(),
            self.vista,
        )) {
            Ok(e) => self.estado = Some(e),
            Err(e) => {
                log::error!("falha ao inicializar a GPU: {e:#}");
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(estado) = self.estado.as_mut() else {
            return;
        };

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    match event.physical_key {
                        PhysicalKey::Code(KeyCode::Escape) => {
                            // Esc: se ha drag, cancela; senao desseleciona;
                            // senao sai (comportamento antigo).
                            if self.drag.is_some() {
                                self.drag = None;
                            } else if self.editor.as_ref().and_then(|e| e.selecionado).is_some() {
                                if let Some(ref mut editor) = self.editor {
                                    editor.selecionado = None;
                                }
                                Self::atualizar_gizmo_selecao_estatico(
                                    self.editor.as_ref(),
                                    &self.camera,
                                    estado,
                                );
                            } else {
                                event_loop.exit();
                            }
                        }
                        PhysicalKey::Code(KeyCode::Delete)
                        | PhysicalKey::Code(KeyCode::Backspace) => {
                            // Delete: remove o objeto selecionado (e seus descendentes).
                            if let Some(ref mut editor) = self.editor {
                                if let Some(id) = editor.selecionado {
                                    if editor.remover(id) {
                                        estado.recursos.remover_objeto(id);
                                        // Tambem remove descendentes da GPU.
                                        let descendentes = editor.descendentes_de(id);
                                        // ja foram removidos in-loop; percorre os ids restantes.
                                        for d in descendentes {
                                            estado.recursos.remover_objeto(d);
                                        }
                                    }
                                }
                                Self::atualizar_gizmo_selecao_estatico(
                                    self.editor.as_ref(),
                                    &self.camera,
                                    estado,
                                );
                            }
                        }
                        PhysicalKey::Code(KeyCode::KeyR) => {
                            self.camera = camera_inicial(&self.scene, self.editor.as_ref());
                            estado.window.request_redraw();
                        }
                        PhysicalKey::Code(KeyCode::KeyT) => {
                            self.camera = OrbitCamera::enquadrando(
                                self.scene.mesh.center(),
                                self.scene.mesh.horizontal_extent(),
                            );
                            estado.window.request_redraw();
                        }
                        // Volta do quadro local para lat/lon: confere na hora se o
                        // que esta na tela corresponde ao lugar certo do mundo.
                        PhysicalKey::Code(KeyCode::KeyC) => {
                            let a = self.camera.alvo;
                            let p = self
                                .scene
                                .frame
                                .enu_to_geodetic(arcz_geo::Enu::new(a[0], -a[2], a[1]));
                            println!(
                                "alvo da camera: lat {:.6}  lon {:.6}  alt {:.1} m",
                                p.lat_deg, p.lon_deg, p.alt_m
                            );
                        }
                        // Ctrl+Z: desfazer. Shift+Ctrl+Z: refazer.
                        // winit 0.30 nao expoe Ctrl direto no KeyCode; checamos
                        // o estado dos modificadores via `is_shortcut` no
                        // modulo winit::keyboard, mas a API mudou. Para E.4
                        // (limite), aceitamos a tecla Z e distinguimos pelo
                        // Shift. Ctrl+Shift+Z eh o atalho padrao de redo.
                        PhysicalKey::Code(KeyCode::KeyZ) => {
                            // Ctrl+Z / Shift+Ctrl+Z virara atalho na E.5 via
                            // menu Tauri. Por agora, Z simples = desfazer.
                            // O tratamento de Shift/Modificadores precisa do
                            // destructure do `event` (que tem `modifiers`).
                            if event.state == ElementState::Pressed {
                                let _ = Self::aplicar_undo_redo(
                                    self.editor.as_mut(),
                                    &mut self.historico,
                                    &self.scene,
                                    estado,
                                );
                            }
                        }
                        _ => {}
                    }
                }
            }

            WindowEvent::MouseInput { state, button, .. } => {
                if button == MouseButton::Left {
                    if state == ElementState::Pressed {
                        self.arrastando = true;
                        self.click_inicio = self.cursor;
                        self.click_pixel = self.cursor;
                        // Antes de entrar em modo picking, ve se ha um gizmo
                        // ativo e o raio acertou uma alca. Se sim, inicia drag
                        // e o picking do objeto NAO acontece.
                        // Extrai o que precisamos do estado sem manter o borrow.
                        let (w, h) = (estado.config.width, estado.config.height);
                        if let Some(alca) = self.picking_alca_gizmo(w, h) {
                            self.iniciar_drag(alca, w, h);
                        }
                    } else {
                        // Released: foi click ou drag?
                        let diff = self
                            .click_inicio
                            .zip(self.cursor)
                            .map(|((a, b), (c, d))| ((a - c).powi(2) + (b - d).powi(2)).sqrt())
                            .unwrap_or(f64::INFINITY);
                        let estava_em_drag = self.drag.is_some();
                        self.arrastando = false;
                        self.cursor = None;
                        self.click_inicio = None;
                        self.drag = None;
                        // < 5 px = click → picking. So se NAO estamos em drag
                        // de gizmo (que tambem comeca com press, mas nesse caso
                        // o diff importa menos).
                        if diff < 5.0 && !estava_em_drag {
                            if let Some(ref mut editor) = self.editor {
                                Self::tratar_click_selecao(
                                    &self.camera,
                                    editor,
                                    self.click_pixel,
                                    estado,
                                );
                                estado.window.request_redraw();
                            }
                        }
                        self.click_pixel = None;
                    }
                }
            }

            WindowEvent::CursorMoved { position, .. } => {
                let atual = (position.x, position.y);
                if self.drag.is_some() {
                    // Drag de gizmo: calcula delta e aplica no objeto.
                    // Extrai a info do drag primeiro para liberar o borrow de self.
                    let alca = self.drag.as_ref().map(|d| d.alca);
                    if let (Some(alca), Some(ref mut editor)) = (alca, self.editor.as_mut()) {
                        // Calcula NDC e delega ao metodo estatico.
                        let (w, h) = (estado.config.width as f64, estado.config.height as f64);
                        let ndc_x = (atual.0 / w) * 2.0 - 1.0;
                        let ndc_y = 1.0 - (atual.1 / h) * 2.0;
                        // Snapshot da placement inicial do drag
                        if let Some(d) = self.drag.as_mut() {
                            d.ndc_atual = (ndc_x, ndc_y);
                        }
                        Self::aplicar_drag_mover(
                            alca,
                            &self.camera,
                            editor,
                            &self.drag,
                            &self.scene,
                            estado,
                        );
                        estado.window.request_redraw();
                    }
                } else if self.arrastando {
                    if let Some(anterior) = self.cursor {
                        self.camera.orbitar(
                            -(atual.0 - anterior.0) * 0.005,
                            -(atual.1 - anterior.1) * 0.005,
                        );
                        estado.window.request_redraw();
                    }
                }
                self.cursor = Some(atual);
            }

            WindowEvent::MouseWheel { delta, .. } => {
                let passos = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y as f64,
                    MouseScrollDelta::PixelDelta(p) => p.y / 60.0,
                };
                self.camera.zoom(0.9f64.powf(passos));
                estado.window.request_redraw();
            }

            WindowEvent::Resized(tamanho) => {
                estado.redimensionar(tamanho.width, tamanho.height);
                estado.window.request_redraw();
            }

            WindowEvent::RedrawRequested => {
                if let Err(e) = estado.desenhar(&self.camera) {
                    log::error!("erro ao desenhar: {e}");
                }
            }

            _ => {}
        }
    }
}

struct Estado {
    /// Corte e estilo da vista (`Globais::vista`).
    vista: [f32; 4],
    momento: Momento,
    lat: f64,
    lon: f64,
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    recursos: Recursos,
    depth: wgpu::TextureView,
}

impl Estado {
    async fn new(
        window: Arc<Window>,
        scene: &Scene,
        editor: Option<&Editor>,
        vista: [f32; 4],
    ) -> anyhow::Result<Self> {
        let tamanho = window.inner_size();
        let (largura, altura) = (tamanho.width.max(1), tamanho.height.max(1));

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance.create_surface(window.clone())?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await?;
        log::info!("adaptador: {:?}", adapter.get_info());

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("arcz-device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
                ..Default::default()
            })
            .await?;

        let caps = surface.get_capabilities(&adapter);
        // Preferir sRGB: a imagery e sRGB e a iluminacao e feita em linear.
        let formato = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: formato,
            width: largura,
            height: altura,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let mut recursos = Recursos::new(&device, &queue, formato, scene)?;
        if let Some(ed) = editor {
            for o in &ed.objetos {
                if o.visivel {
                    let solo = scene.altura_no_terreno(o.placement.lon_deg, o.placement.lat_deg);
                    if let Err(e) =
                        recursos.adicionar_objeto(&device, &queue, &scene.frame, o, solo)
                    {
                        log::warn!("falha ao subir objeto {}: {e}", o.nome);
                    }
                }
            }
        }
        let depth = criar_depth(&device, largura, altura);

        let c = scene.bbox.center();
        Ok(Self {
            vista,
            momento: Momento::default(),
            lat: c.lat_deg,
            lon: c.lon_deg,
            window,
            surface,
            device,
            queue,
            config,
            recursos,
            depth,
        })
    }

    fn redimensionar(&mut self, largura: u32, altura: u32) {
        if largura == 0 || altura == 0 {
            return;
        }
        self.config.width = largura;
        self.config.height = altura;
        self.surface.configure(&self.device, &self.config);
        self.depth = criar_depth(&self.device, largura, altura);
    }

    fn desenhar(&mut self, camera: &OrbitCamera) -> Result<(), wgpu::SurfaceError> {
        let aspecto = self.config.width as f64 / self.config.height.max(1) as f64;
        let vp = camera.view_proj(aspecto);
        let (luz, sol) = self.momento.uniform_luz(self.lat, self.lon);
        let olho = camera.posicao();

        self.queue.write_buffer(
            &self.recursos.globais_buf,
            0,
            bytemuck::bytes_of(&Globais {
                view_proj: crate::camera::to_f32(vp),
                inv_view_proj: crate::camera::to_f32(crate::camera::inverse(vp)),
                luz,
                camera: [
                    olho[0] as f32,
                    olho[1] as f32,
                    olho[2] as f32,
                    sol.elevacao_deg as f32,
                ],
                vista: self.vista,
            }),
        );

        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                self.surface.get_current_texture()?
            }
            Err(e) => return Err(e),
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });

        {
            let mut passe = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("cena"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(FUNDO),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            self.recursos.desenhar(&mut passe);
        }

        self.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }
}

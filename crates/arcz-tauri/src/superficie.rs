//! Superficie wgpu sobre a janela do Tauri.
//!
//! E a peca que o ADR-0002 depende: em vez de o Rust mandar JPEG para o webview
//! (teto medido de ~22 fps), o wgpu desenha direto na janela, como ja faz com
//! `winit` no `arcz-app`. O `wgpu` aceita qualquer alvo que implemente
//! `HasWindowHandle + HasDisplayHandle`, e a `WebviewWindow` do Tauri implementa
//! os dois — por isso a troca nao exige tocar em `gpu.rs` nem nos shaders.
//!
//! O risco real que este modulo existe para medir **nao e criar a superficie**, e
//! sim a composicao: a superficie nativa e o webview disputam a mesma janela, e a
//! ordem de empilhamento varia por plataforma.

use std::sync::Arc;

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

/// O que se descobriu ao tentar subir a superficie. Reportado a UI para a decisao
/// de layout ser tomada com dado, nao com suposicao.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RelatorioSuperficie {
    pub ok: bool,
    pub adaptador: String,
    pub backend: String,
    pub formato: String,
    pub largura: u32,
    pub altura: u32,
    /// Mensagem de erro quando `ok == false`.
    pub erro: Option<String>,
}

pub struct Superficie {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub relatorio: RelatorioSuperficie,
}

impl Superficie {
    /// Cria a superficie sobre `alvo`, que precisa viver tanto quanto ela.
    ///
    /// `alvo` e generico de proposito: assim o modulo e testavel com uma janela
    /// qualquer e nao amarra o codigo ao tipo concreto do Tauri.
    pub fn nova<T>(alvo: Arc<T>, largura: u32, altura: u32) -> anyhow::Result<Self>
    where
        T: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static,
    {
        pollster::block_on(Self::criar(alvo, largura, altura))
    }

    async fn criar<T>(alvo: Arc<T>, largura: u32, altura: u32) -> anyhow::Result<Self>
    where
        T: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static,
    {
        let largura = largura.max(1);
        let altura = altura.max(1);

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance.create_surface(alvo)?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await?;
        let info = adapter.get_info();

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("arcz-tauri-device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
                ..Default::default()
            })
            .await?;

        let caps = surface.get_capabilities(&adapter);
        // sRGB: a imagery e sRGB e a iluminacao e calculada em linear.
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

        let relatorio = RelatorioSuperficie {
            ok: true,
            adaptador: info.name.clone(),
            backend: format!("{:?}", info.backend),
            formato: format!("{formato:?}"),
            largura,
            altura,
            erro: None,
        };
        log::info!(
            "superficie wgpu na janela Tauri: {} / {:?} / {formato:?} / {largura}x{altura}",
            info.name,
            info.backend
        );

        Ok(Self {
            surface,
            device,
            queue,
            config,
            relatorio,
        })
    }

    pub fn redimensionar(&mut self, largura: u32, altura: u32) {
        if largura == 0 || altura == 0 {
            return;
        }
        self.config.width = largura;
        self.config.height = altura;
        self.surface.configure(&self.device, &self.config);
        self.relatorio.largura = largura;
        self.relatorio.altura = altura;
    }

    /// Desenha um quadro solido. Serve so para provar que a superficie aparece na
    /// janela e por cima (ou por baixo) do webview — a medicao de composicao.
    pub fn limpar(&mut self, cor: wgpu::Color) -> Result<(), wgpu::SurfaceError> {
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
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("prova-de-composicao"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(cor),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        self.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }
}

impl RelatorioSuperficie {
    pub fn falha(motivo: impl std::fmt::Display) -> Self {
        Self {
            ok: false,
            adaptador: String::new(),
            backend: String::new(),
            formato: String::new(),
            largura: 0,
            altura: 0,
            erro: Some(motivo.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn o_relatorio_de_falha_carrega_o_motivo() {
        let r = RelatorioSuperficie::falha("sem adaptador compativel");
        assert!(!r.ok);
        assert_eq!(r.erro.as_deref(), Some("sem adaptador compativel"));
        assert_eq!(r.largura, 0);
    }

    #[test]
    fn o_relatorio_serializa_para_a_ui() {
        // A UI decide o layout com base nisto; o contrato de nomes tem que valer.
        let r = RelatorioSuperficie {
            ok: true,
            adaptador: "NVIDIA GeForce RTX 4090 Laptop GPU".into(),
            backend: "Vulkan".into(),
            formato: "Bgra8UnormSrgb".into(),
            largura: 1600,
            altura: 900,
            erro: None,
        };
        let j = serde_json::to_string(&r).unwrap();
        for campo in ["ok", "adaptador", "backend", "formato", "largura", "altura"] {
            assert!(j.contains(campo), "faltou {campo} em {j}");
        }
    }

    #[test]
    fn redimensionar_para_zero_e_ignorado() {
        // Minimizar a janela manda 0x0; reconfigurar a surface com isso e erro de
        // validacao do wgpu e derruba o app.
        let mut r = RelatorioSuperficie {
            ok: true,
            adaptador: String::new(),
            backend: String::new(),
            formato: String::new(),
            largura: 800,
            altura: 600,
            erro: None,
        };
        // Reproduz a guarda de `redimensionar` sem precisar de GPU no teste.
        let aplicar = |r: &mut RelatorioSuperficie, w: u32, h: u32| {
            if w == 0 || h == 0 {
                return;
            }
            r.largura = w;
            r.altura = h;
        };
        aplicar(&mut r, 0, 600);
        assert_eq!((r.largura, r.altura), (800, 600), "0 de largura passou");
        aplicar(&mut r, 1024, 768);
        assert_eq!((r.largura, r.altura), (1024, 768));
    }
}

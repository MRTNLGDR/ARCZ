//! Renderizador offscreen persistente.
//!
//! Diferente de um render de uma vez so, este mantem `device`, `queue` e os recursos
//! vivos entre quadros. Reenviar as texturas do modelo (201 MB no Zenite) a cada
//! ajuste tornaria o preview inutilizavel; aqui so o buffer de vertices e reescrito
//! quando o modelo se move, e mexer na camera nao toca em nada alem do uniform.
//!
//! A posicao do modelo vem de uma matriz em `group(2)`: mover, girar ou escalar
//! reescreve 64 bytes, nao a malha.

use std::path::Path;

use crate::camera::OrbitCamera;
use crate::cena::Editor;
use crate::gpu::{criar_depth, Globais, Recursos, FUNDO};
use crate::iluminacao::Momento;
use crate::scene::Scene;

/// wgpu exige que cada linha copiada de textura para buffer seja multipla disto.
const ALINHAMENTO_LINHA: u32 = 256;

/// Assenta o entorno no DEM real, ponto a ponto.
///
/// Antes disto tudo assentava numa cota unica, e numa encosta como a de Jose
/// Amandio as casas ficavam visivelmente boiando sobre o relevo — o defeito
/// aparecia de imediato na primeira vista aerea.
struct TerrenoDoDem<'a>(&'a Scene, &'a arcz_geo::EnuFrame);

impl arcz_osm::Terreno for TerrenoDoDem<'_> {
    fn altura(&self, leste: f64, norte: f64) -> f64 {
        let g = self
            .1
            .enu_to_geodetic(arcz_geo::Enu::new(leste, norte, 0.0));
        self.0.altura_no_terreno(g.lon_deg, g.lat_deg)
    }
}

/// Centro geografico de um anel, como `(lat, lon)`.
fn centro_geo(anel: &[arcz_osm::PontoGeo]) -> Option<(f64, f64)> {
    if anel.is_empty() {
        return None;
    }
    let n = anel.len() as f64;
    Some((
        anel.iter().map(|p| p.lat).sum::<f64>() / n,
        anel.iter().map(|p| p.lon).sum::<f64>() / n,
    ))
}

/// Clareia e dessatura uma cor, aproximando parede a partir da cobertura.
fn clarear(c: [f32; 3], t: f32) -> [f32; 3] {
    let luz = (c[0] + c[1] + c[2]) / 3.0;
    let alvo = (luz * 0.5 + 0.5).min(0.92);
    [
        c[0] + (alvo - c[0]) * t,
        c[1] + (alvo - c[1]) * t,
        c[2] + (alvo - c[2]) * t,
    ]
}

/// Busca o entorno no OSM, adensa, colore pela ortofoto e instala no `Editor`.
///
/// Fica fora do `impl Renderer` para poder ser chamada com o runtime tokio do
/// servidor, que e quem tem o executor async.
pub async fn carregar_entorno(
    r: &mut Renderer,
    editor: &mut Editor,
    cena: &Scene,
    frame: &arcz_geo::EnuFrame,
    solo_m: f64,
    lado_m: f64,
    adensar: bool,
) -> anyhow::Result<RelatorioEntorno> {
    use arcz_osm::{malha, procedural, Camadas, ClienteOverpass, Opcoes, RegrasUrbanas};

    let centro = frame.origin_geodetic();
    let bbox = arcz_geo::GeoBBox::around(centro, lado_m)?;

    let cache = std::env::var("ARCZ_CACHE").unwrap_or_else(|_| "cache/osm".into());
    let (mut entorno, _) = ClienteOverpass::novo(cache)
        .buscar(&bbox, Camadas::default())
        .await?;

    let predios_osm = entorno.edificios.len();

    // Recorta ANTES de adensar. O Overpass devolve cada via inteira assim que
    // ela toca a bbox; adensar primeiro loteava a rua ate quilometros alem do
    // terreno e as casas ficavam boiando no vazio depois da borda.
    procedural::recortar(&mut entorno, frame, lado_m * 0.5);

    let predios_gerados = if adensar {
        let _n = procedural::adensar(&mut entorno, frame, RegrasUrbanas::default());
        // As vias truncadas guardam um ponto alem da divisa (para a rua chegar
        // ate ela), entao o loteamento ainda pinga alguns lotes do lado de fora
        // do terreno — casas boiando no vazio. Predio mapeado pelo OSM (id >= 0)
        // fica mesmo na borda; o sintetico, que e palpite, e barato de descartar.
        let m = lado_m * 0.5;
        entorno.edificios.retain(|ed| {
            if ed.id >= 0 {
                return true;
            }
            centro_geo(&ed.contorno).is_some_and(|(lat, lon)| {
                let e = frame.geodetic_to_enu(arcz_geo::Geodetic::new(lon, lat, 0.0));
                e.e.abs() <= m && e.n.abs() <= m
            })
        });
        entorno.edificios.iter().filter(|e| e.id < 0).count()
    } else {
        0
    };

    // Cada edificacao herda a cor dominante da ortofoto no seu proprio lote. E o
    // que tira o bairro do branco de maquete: telhas ficam com o tom de telha,
    // lajes com o tom de laje. A parede sai mais clara e menos saturada que a
    // cobertura, que e o que se ve de fato numa vista aerea.
    for ed in &mut entorno.edificios {
        let Some(c) = centro_geo(&ed.contorno) else {
            continue;
        };
        let raio = 6.0 + ed.altura_m * 0.2;
        let cob = cena.imagery.cor_media(c.1, c.0, raio);
        ed.cor_telhado = Some(cob);
        ed.cor_parede = Some(clarear(cob, 0.45));
    }

    let malhas = malha::gerar(
        &entorno,
        frame,
        &TerrenoDoDem(cena, frame),
        Opcoes::default(),
    );
    let triangulos = malhas.iter().map(|m| m.triangulos()).sum();

    // O entorno entra como objeto do `Editor`, nao direto na GPU. E o que o
    // torna clicavel e manipulavel pelo gizmo — ver `entorno.rs`.
    let enviadas = crate::entorno::instalar(editor, &malhas, frame);

    r.recursos.limpar_entorno();
    for o in editor
        .objetos
        .iter()
        .filter(|o| crate::entorno::e_do_entorno(o.id))
    {
        if let Err(e) = r.adicionar_objeto(o, solo_m) {
            log::warn!("falha ao subir {}: {e}", o.nome);
        }
    }

    log::info!(
        "entorno: {predios_osm} do OSM + {predios_gerados} gerados, {enviadas} objetos, {triangulos} triangulos"
    );

    Ok(RelatorioEntorno {
        predios_osm,
        predios_gerados,
        vias: entorno.vias.len(),
        superficies: entorno.superficies.len(),
        arvores: entorno.arvores.len(),
        malhas: enviadas,
        triangulos,
        atribuicao: arcz_osm::ATRIBUICAO,
    })
}

/// Resultado de carregar o entorno, para a UI relatar sem adivinhar.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RelatorioEntorno {
    pub predios_osm: usize,
    pub predios_gerados: usize,
    pub vias: usize,
    pub superficies: usize,
    pub arvores: usize,
    pub malhas: usize,
    pub triangulos: usize,
    /// ODbL: precisa aparecer em qualquer imagem publicada.
    pub atribuicao: &'static str,
}

pub struct Renderer {
    /// Corte e estilo da vista (`Globais::vista`). Ver `Config::vista`.
    pub vista: [f32; 4],
    /// Local e momento usados para posicionar o Sol. Mudam sem recriar recursos.
    pub momento: Momento,
    pub lat: f64,
    pub lon: f64,
    /// Quadro geodetico para converter lat/lon de objetos adicionados em runtime
    /// em coordenadas de render. Vem do `Scene` carregado.
    frame: arcz_geo::EnuFrame,
    device: wgpu::Device,
    queue: wgpu::Queue,
    recursos: Recursos,
    formato: wgpu::TextureFormat,
    /// Alvo e profundidade sao recriados so quando a resolucao muda.
    alvo: Option<(
        u32,
        u32,
        wgpu::Texture,
        wgpu::TextureView,
        wgpu::TextureView,
    )>,
}

impl Renderer {
    pub fn new(scene: &Scene, editor: Option<&Editor>) -> anyhow::Result<Self> {
        pollster::block_on(Self::criar(scene, editor))
    }

    async fn criar(scene: &Scene, editor: Option<&Editor>) -> anyhow::Result<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        // Sem `compatible_surface`: nao ha janela envolvida.
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await?;
        log::info!("renderer no adaptador: {}", adapter.get_info().name);

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("arcz-renderer"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
                ..Default::default()
            })
            .await?;

        let formato = wgpu::TextureFormat::Rgba8UnormSrgb;
        let mut recursos = Recursos::new(&device, &queue, formato, scene)?;

        // Se o Editor foi passado, faz upload de cada objeto visivel. Cada um
        // ganha seu proprio transform_buf + bind_group (E.1 do Nucleo do Editor).
        if let Some(ed) = editor {
            for o in &ed.objetos {
                if o.visivel {
                    // Solo de cada objeto: amostra o DEM na sua posicao geografica.
                    let solo = scene.altura_no_terreno(o.placement.lon_deg, o.placement.lat_deg);
                    if let Err(e) =
                        recursos.adicionar_objeto(&device, &queue, &scene.frame, o, solo)
                    {
                        log::warn!("falha ao subir objeto {}: {e}", o.nome);
                    }
                }
            }
        }

        let c = scene.bbox.center();
        Ok(Self {
            vista: [0.0, 0.0, 0.0, 0.06],
            momento: Momento::default(),
            lat: c.lat_deg,
            lon: c.lon_deg,
            frame: scene.frame,
            device,
            queue,
            recursos,
            formato,
            alvo: None,
        })
    }

    /// Move/gira/escala um objeto. Custa 64 bytes, nao a malha inteira.
    pub fn atualizar_transform(&mut self, id: u32, modelo: [[f32; 4]; 4]) {
        self.recursos.atualizar_transform(&self.queue, id, modelo);
    }

    /// Adiciona um objeto em runtime (apos `Renderer::new`).
    /// Usado quando o usuario arrasta um asset da biblioteca pro viewport.
    /// `solo_m` e a altitude do terreno sob o objeto — o caller amostra o DEM.
    #[allow(dead_code)] // sera usado pela UI Tauri+React na E.5
    pub fn adicionar_objeto(
        &mut self,
        obj: &crate::cena::Objeto,
        solo_m: f64,
    ) -> anyhow::Result<()> {
        self.recursos
            .adicionar_objeto(&self.device, &self.queue, &self.frame, obj, solo_m)
    }

    /// Descarta o entorno da GPU, preservando o modelo do usuario.
    pub fn limpar_entorno(&mut self) {
        self.recursos.limpar_entorno();
    }

    /// Remove um objeto em runtime.
    #[allow(dead_code)]
    pub fn remover_objeto(&mut self, id: u32) {
        self.recursos.remover_objeto(id);
    }

    /// Atualiza as linhas do gizmo mostrado por cima da cena.
    pub fn atualizar_gizmo(&mut self, linhas: &[crate::gizmo::VerticeLinha]) {
        self.recursos.atualizar_gizmo(&self.device, linhas);
    }

    /// Renderiza e devolve os pixels RGBA8 sem padding.
    pub fn render_rgba(
        &mut self,
        camera: &OrbitCamera,
        largura: u32,
        altura: u32,
        mostrar_modelo: bool,
    ) -> anyhow::Result<Vec<u8>> {
        self.render_rgba_camadas(
            camera,
            largura,
            altura,
            mostrar_modelo,
            crate::gpu::Camadas::TUDO,
        )
    }

    /// Igual, escolhendo o que entra no quadro.
    ///
    /// Com `Camadas::SO_PROJETO` o fundo sai **transparente**, para o quadro ser
    /// composto sobre uma foto do local. E por isso que a cor de limpeza muda
    /// junto: limpar com o azul do ceu deixaria uma moldura solida em volta do
    /// predio, e nenhum alpha salvaria isso depois.
    pub fn render_rgba_camadas(
        &mut self,
        camera: &OrbitCamera,
        largura: u32,
        altura: u32,
        mostrar_modelo: bool,
        camadas: crate::gpu::Camadas,
    ) -> anyhow::Result<Vec<u8>> {
        let largura = largura.max(1);
        let altura = altura.max(1);
        self.garantir_alvo(largura, altura);
        let (_, _, alvo, view, depth) = self.alvo.as_ref().expect("alvo recem-criado");

        let aspecto = largura as f64 / altura as f64;
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

        let bytes_por_linha_real = largura * 4;
        let bytes_por_linha = bytes_por_linha_real.div_ceil(ALINHAMENTO_LINHA) * ALINHAMENTO_LINHA;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("leitura"),
            size: (bytes_por_linha as u64) * (altura as u64),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("quadro"),
            });
        {
            let mut passe = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("cena"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(if camadas.ceu {
                            FUNDO
                        } else {
                            // Preto com alpha zero: o que nao for desenhado sai
                            // transparente de verdade no PNG.
                            wgpu::Color {
                                r: 0.0,
                                g: 0.0,
                                b: 0.0,
                                a: 0.0,
                            }
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            self.recursos
                .desenhar_completo(&mut passe, mostrar_modelo, camadas);
        }

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: alvo,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_por_linha),
                    rows_per_image: Some(altura),
                },
            },
            wgpu::Extent3d {
                width: largura,
                height: altura,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));

        let (tx, rx) = std::sync::mpsc::channel();
        buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })?;
        rx.recv()??;

        // Remove o padding de cada linha.
        let dados = buffer.slice(..).get_mapped_range();
        let mut pixels = Vec::with_capacity((bytes_por_linha_real as usize) * (altura as usize));
        for y in 0..altura as usize {
            let inicio = y * bytes_por_linha as usize;
            pixels.extend_from_slice(&dados[inicio..inicio + bytes_por_linha_real as usize]);
        }
        drop(dados);
        buffer.unmap();

        Ok(pixels)
    }

    /// Renderiza e codifica em JPEG.
    ///
    /// Usado durante a interacao. PNG e sem perdas e comprimir 1,5 MP custa mais que
    /// desenhar o quadro — era o gargalo que fazia o preview parecer travado. JPEG a
    /// 82% sai varias vezes mais rapido e a diferenca nao aparece enquanto se arrasta.
    pub fn render_jpeg(
        &mut self,
        camera: &OrbitCamera,
        largura: u32,
        altura: u32,
        mostrar_modelo: bool,
        qualidade: u8,
    ) -> anyhow::Result<Vec<u8>> {
        let pixels = self.render_rgba(camera, largura, altura, mostrar_modelo)?;
        let img = image::RgbaImage::from_raw(largura.max(1), altura.max(1), pixels)
            .ok_or_else(|| anyhow::anyhow!("buffer de pixels com tamanho inesperado"))?;

        let mut saida = std::io::Cursor::new(Vec::new());
        // JPEG nao tem canal alfa; a cena e opaca, entao descartar e correto.
        let rgb = image::DynamicImage::ImageRgba8(img).to_rgb8();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut saida, qualidade.clamp(30, 95))
            .encode_image(&rgb)?;
        Ok(saida.into_inner())
    }

    /// Constroi o raio (origem, direcao) que sai da camera e passa pelo pixel
    /// em coordenadas NDC. Usado pelo picking: testa o raio contra os AABBs
    /// dos objetos do Editor para descobrir qual foi clicado.
    ///
    /// `aspecto` = largura_janela / altura_janela.
    /// `ndc_x`, `ndc_y` em [-1, 1] (-1 = esquerda/baixo, +1 = direita/topo).
    /// Devolve `None` se a inversa da view_proj for degenerada (caso patologico).
    pub fn raio_da_camera(
        camera: &OrbitCamera,
        aspecto: f64,
        ndc_x: f64,
        ndc_y: f64,
    ) -> Option<([f64; 3], [f64; 3])> {
        let inv = crate::camera::inverse(camera.view_proj(aspecto));
        let despro = |z: f64| -> Option<[f64; 3]> {
            let p = crate::camera::transform(inv, [ndc_x, ndc_y, z]);
            (p[3].abs() > 1e-12).then(|| [p[0] / p[3], p[1] / p[3], p[2] / p[3]])
        };
        let perto = despro(0.0)?;
        let longe = despro(1.0)?;
        let dir = [
            longe[0] - perto[0],
            longe[1] - perto[1],
            longe[2] - perto[2],
        ];
        let norma = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
        if norma < 1e-12 {
            return None;
        }
        Some((perto, [dir[0] / norma, dir[1] / norma, dir[2] / norma]))
    }

    /// Onde o raio que passa pelo pixel corta o plano horizontal `altura_y`.
    ///
    /// E a conta que transforma "arrastei 40 px na tela" em "movi o predio 6,3 m para
    /// o nordeste". Sem ela, arrastar geometria seria chute proporcional ao zoom.
    /// Devolve `None` se o raio for paralelo ao plano ou o cruzar atras da camera.
    pub fn raio_no_plano(
        camera: &OrbitCamera,
        aspecto: f64,
        ndc_x: f64,
        ndc_y: f64,
        altura_y: f64,
    ) -> Option<[f64; 2]> {
        let inv = crate::camera::inverse(camera.view_proj(aspecto));

        let despro = |z: f64| -> Option<[f64; 3]> {
            let p = crate::camera::transform(inv, [ndc_x, ndc_y, z]);
            (p[3].abs() > 1e-12).then(|| [p[0] / p[3], p[1] / p[3], p[2] / p[3]])
        };
        let perto = despro(0.0)?;
        let longe = despro(1.0)?;

        let dir = [
            longe[0] - perto[0],
            longe[1] - perto[1],
            longe[2] - perto[2],
        ];
        // Raio paralelo ao plano: nao ha intersecao util.
        if dir[1].abs() < 1e-9 {
            return None;
        }
        let t = (altura_y - perto[1]) / dir[1];
        if t < 0.0 {
            return None;
        }
        Some([perto[0] + dir[0] * t, perto[2] + dir[2] * t])
    }

    /// Renderiza e codifica em PNG.
    pub fn render_png(
        &mut self,
        camera: &OrbitCamera,
        largura: u32,
        altura: u32,
        mostrar_modelo: bool,
    ) -> anyhow::Result<Vec<u8>> {
        let pixels = self.render_rgba(camera, largura, altura, mostrar_modelo)?;
        let img = image::RgbaImage::from_raw(largura.max(1), altura.max(1), pixels)
            .ok_or_else(|| anyhow::anyhow!("buffer de pixels com tamanho inesperado"))?;

        let mut png = std::io::Cursor::new(Vec::new());
        img.write_to(&mut png, image::ImageFormat::Png)?;
        Ok(png.into_inner())
    }

    pub fn salvar_png(
        &mut self,
        camera: &OrbitCamera,
        largura: u32,
        altura: u32,
        destino: &Path,
    ) -> anyhow::Result<()> {
        let png = self.render_png(camera, largura, altura, true)?;
        if let Some(pai) = destino.parent() {
            if !pai.as_os_str().is_empty() {
                std::fs::create_dir_all(pai)?;
            }
        }
        std::fs::write(destino, png)?;
        Ok(())
    }

    fn garantir_alvo(&mut self, largura: u32, altura: u32) {
        if let Some((w, h, _, _, _)) = &self.alvo {
            if *w == largura && *h == altura {
                return;
            }
        }
        let tamanho = wgpu::Extent3d {
            width: largura,
            height: altura,
            depth_or_array_layers: 1,
        };
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("alvo-offscreen"),
            size: tamanho,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.formato,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        let depth = criar_depth(&self.device, largura, altura);
        self.alvo = Some((largura, altura, tex, view, depth));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn padding_de_linha_respeita_o_alinhamento_do_wgpu() {
        // Copiar textura para buffer com bytes_per_row nao alinhado e erro de
        // validacao do wgpu; o calculo abaixo e o que evita isso.
        for largura in [1u32, 100, 640, 1920, 3840, 7680] {
            let real = largura * 4;
            let alinhado = real.div_ceil(ALINHAMENTO_LINHA) * ALINHAMENTO_LINHA;
            assert_eq!(alinhado % ALINHAMENTO_LINHA, 0, "largura {largura}");
            assert!(alinhado >= real, "largura {largura}: {alinhado} < {real}");
            assert!(
                alinhado - real < ALINHAMENTO_LINHA,
                "padding exagerado em {largura}"
            );
        }
    }

    #[test]
    fn o_centro_da_tela_cai_no_alvo_da_camera() {
        // O alvo esta no plano y=0, entao o raio central tem que voltar (0, 0).
        let c = OrbitCamera::enquadrando([0.0, 0.0, 0.0], 200.0);
        let p = Renderer::raio_no_plano(&c, 16.0 / 9.0, 0.0, 0.0, 0.0).unwrap();
        assert!(p[0].abs() < 0.5 && p[1].abs() < 0.5, "centro caiu em {p:?}");
    }

    #[test]
    fn arrastar_para_a_direita_move_o_ponto_para_a_direita_da_camera() {
        // Camera olhando do sul (yaw=0): +NDC x tem que virar +leste no mundo.
        let mut c = OrbitCamera::enquadrando([0.0, 0.0, 0.0], 200.0);
        c.yaw = 0.0;
        c.pitch = 0.7;

        let centro = Renderer::raio_no_plano(&c, 1.0, 0.0, 0.0, 0.0).unwrap();
        let direita = Renderer::raio_no_plano(&c, 1.0, 0.5, 0.0, 0.0).unwrap();
        assert!(
            direita[0] > centro[0] + 1.0,
            "direita da tela deveria ir para o leste: {centro:?} -> {direita:?}"
        );
    }

    #[test]
    fn o_plano_respeita_a_altura_pedida() {
        // Desprojetar no plano y=50 tem que dar um ponto diferente de y=0, e o
        // deslocamento cresce com a altura (a camera esta inclinada).
        let c = OrbitCamera::enquadrando([0.0, 0.0, 0.0], 300.0);
        let baixo = Renderer::raio_no_plano(&c, 1.0, 0.3, -0.4, 0.0).unwrap();
        let alto = Renderer::raio_no_plano(&c, 1.0, 0.3, -0.4, 50.0).unwrap();
        assert!(
            (baixo[0] - alto[0]).abs() > 1.0 || (baixo[1] - alto[1]).abs() > 1.0,
            "a altura do plano nao mudou nada: {baixo:?} vs {alto:?}"
        );
    }

    #[test]
    fn raio_paralelo_ao_plano_nao_devolve_ponto() {
        // Camera exatamente no horizonte olhando reto: o raio central nunca cruza
        // o plano. Sem esta guarda o resultado seria infinito.
        let mut c = OrbitCamera::enquadrando([0.0, 0.0, 0.0], 100.0);
        c.pitch = 0.0;
        c.alvo[1] = 0.0;
        assert!(Renderer::raio_no_plano(&c, 1.0, 0.0, 0.0, 0.0).is_none());
    }

    #[test]
    fn larguras_ja_alinhadas_nao_ganham_padding() {
        // 1920*4 = 7680, que ja e multiplo de 256.
        assert_eq!(
            (1920u32 * 4).div_ceil(ALINHAMENTO_LINHA) * ALINHAMENTO_LINHA,
            7680
        );
    }

    // === testes de raio_da_camera (picking 3D) ================================

    #[test]
    fn raio_da_camera_devolve_origem_e_direcao_normalizada() {
        let c = OrbitCamera::enquadrando([0.0, 0.0, 0.0], 200.0);
        let (origem, dir) = Renderer::raio_da_camera(&c, 1.0, 0.0, 0.0).unwrap();
        // A origem tem que estar perto da posicao da camera (na frente do alvo).
        // A direcao tem que ser unitaria.
        let n = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
        assert!((n - 1.0).abs() < 1e-9, "direcao nao-unitaria: {dir:?}");
        // A direcao tem que apontar do olho para o alvo, nao paralelo.
        assert!(dir[1].abs() > 0.01 || dir[2].abs() > 0.01);
        // Origem tem que ser finita e perto da camera.
        assert!(origem.iter().all(|c| c.is_finite()));
    }

    #[test]
    fn clicar_na_esquerda_e_na_direita_da_direcoes_diferentes() {
        // O mesmo pixel +X e -X nao podem dar o mesmo raio, senao picking nao funciona.
        let c = OrbitCamera::enquadrando([0.0, 0.0, 0.0], 200.0);
        let (_, esq) = Renderer::raio_da_camera(&c, 1.0, -0.5, 0.0).unwrap();
        let (_, dir) = Renderer::raio_da_camera(&c, 1.0, 0.5, 0.0).unwrap();
        // Os raios devem divergir: produto interno < 1 (nao identicos).
        let dot = esq[0] * dir[0] + esq[1] * dir[1] + esq[2] * dir[2];
        assert!(
            dot < 0.999,
            "esquerda e direita dao o mesmo raio (dot={dot})"
        );
    }
}

//! Recursos de GPU compartilhados entre o viewport interativo e o render offscreen.
//!
//! Os dois caminhos usam exatamente o mesmo pipeline, os mesmos buffers e o mesmo
//! shader. Isso e proposital: um PNG gerado pelo `--png` tem que mostrar a mesma
//! imagem da janela, senao o preview nao serve para validar nada.

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::scene::Scene;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct GpuVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

/// group(0): camera e luz, um por quadro.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct Globais {
    pub view_proj: [[f32; 4]; 4],
    /// Inversa da view_proj: o shader de ceu reconstroi a direcao do raio com ela.
    pub inv_view_proj: [[f32; 4]; 4],
    /// xyz = direcao PARA o Sol; w = fracao ambiente.
    pub luz: [f32; 4],
    /// xyz = posicao da camera em ENU; w = elevacao solar em graus.
    pub camera: [f32; 4],
    /// Corte e estilo da VISTA — nao mexem na geometria, so no que e desenhado.
    ///
    /// x = altura do plano de corte em ENU (metros); y = 1 quando o corte esta ativo;
    /// z = estilo (0 foto, 1 planta humanizada, 2 sketch); w = espessura da linha de
    /// corte em metros.
    pub vista: [f32; 4],
}

/// group(2): matriz do objeto. Trocar 64 bytes move a malha inteira.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct TransformUniform {
    pub modelo: [[f32; 4]; 4],
}

impl TransformUniform {
    pub fn identidade() -> Self {
        let mut m = [[0.0f32; 4]; 4];
        for (k, col) in m.iter_mut().enumerate() {
            col[k] = 1.0;
        }
        Self { modelo: m }
    }
}

/// group(1): material, trocado a cada submesh.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct MaterialUniform {
    pub base_color: [f32; 4],
    /// x = tem textura (0/1); y = corte de alpha; z, w = reservados.
    pub flags: [f32; 4],
}

pub const FORMATO_PROFUNDIDADE: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// O que entra no quadro.
///
/// Existe para a composição sobre foto: sobrepor o empreendimento a uma imagem
/// do local exige um quadro **só com o projeto**, sobre fundo transparente.
/// Céu e terreno tapariam a foto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Camadas {
    pub ceu: bool,
    pub terreno: bool,
}

impl Camadas {
    pub const TUDO: Self = Self {
        ceu: true,
        terreno: true,
    };

    /// Só o projeto, para compor sobre uma imagem de fundo.
    ///
    /// O terreno fica de fora por padrão porque na composição quem faz o papel
    /// do chão é a própria foto. Quando o relevo à frente precisa recortar o
    /// prédio, ligue `terreno` de novo.
    pub const SO_PROJETO: Self = Self {
        ceu: false,
        terreno: false,
    };
}

/// Uma chamada de desenho: faixa de indices + o bind group do seu material.
pub struct Lote {
    pub bind_group: wgpu::BindGroup,
    pub offset: u32,
    pub count: u32,
}

pub struct MalhaGpu {
    pub vertex_buf: wgpu::Buffer,
    pub index_buf: wgpu::Buffer,
    pub lotes: Vec<Lote>,
}

/// Um objeto da cena na GPU: malha + a sua propria matriz.
///
/// Cada objeto tem buffer de transformacao proprio, entao mover um nao mexe nos
/// outros — e mover custa 64 bytes, nao a malha.
pub struct ObjetoGpu {
    pub id: u32,
    pub malha: MalhaGpu,
    pub transform_buf: wgpu::Buffer,
    pub bind_transform: wgpu::BindGroup,
    pub visivel: bool,
}

/// Tudo que e preciso para desenhar a cena, independente do destino.
pub struct Recursos {
    pub pipeline_ceu: wgpu::RenderPipeline,
    pub pipeline_terreno: wgpu::RenderPipeline,
    pub pipeline_modelo: wgpu::RenderPipeline,
    pub pipeline_gizmo: wgpu::RenderPipeline,
    /// Linhas do gizmo, regeradas a cada mudanca de selecao ou de camera.
    pub gizmo: Option<(wgpu::Buffer, u32)>,
    /// Terreno usa identidade e nunca muda.
    pub bind_transform_terreno: wgpu::BindGroup,
    /// Objetos editaveis da cena, cada um com a sua matriz.
    pub objetos: Vec<ObjetoGpu>,
    /// Guardados para criar objetos em runtime a partir da biblioteca. Ainda nao
    /// consumidos: `adicionar_objeto` e o proximo passo desta fatia.
    #[allow(dead_code)]
    pub layout_material: wgpu::BindGroupLayout,
    #[allow(dead_code)]
    pub layout_transform: wgpu::BindGroupLayout,
    #[allow(dead_code)]
    pub sampler_repeat: wgpu::Sampler,
    #[allow(dead_code)]
    pub textura_branca: wgpu::TextureView,
    /// Limite de dimensao de textura da GPU atual — usado por `adicionar_objeto`
    /// pra downscale automatico de texturas de modelos grandes.
    #[allow(dead_code)]
    pub limite_textura: u32,
    pub globais_buf: wgpu::Buffer,
    pub globais_bind: wgpu::BindGroup,
    pub terreno: MalhaGpu,
    /// Quantas texturas foram enviadas e quantos bytes ocupam na VRAM.
    pub texturas_enviadas: usize,
    pub bytes_de_textura: usize,
}

impl Recursos {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        formato_alvo: wgpu::TextureFormat,
        scene: &Scene,
    ) -> anyhow::Result<Self> {
        let limite = device.limits().max_texture_dimension_2d;

        // --- layouts ---------------------------------------------------------
        let layout_globais = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globais-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let layout_material = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("material-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let globais_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globais"),
            size: std::mem::size_of::<Globais>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globais_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globais-bind"),
            layout: &layout_globais,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globais_buf.as_entire_binding(),
            }],
        });

        // group(2): transformacao do objeto. Dois bind groups, dois buffers — o do
        // terreno e identidade e nunca muda; o do modelo e reescrito ao mover.
        let layout_transform = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("transform-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let identidade_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("transform-identidade"),
            contents: bytemuck::bytes_of(&TransformUniform::identidade()),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let bind_transform = |rotulo: &str, buf: &wgpu::Buffer| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(rotulo),
                layout: &layout_transform,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buf.as_entire_binding(),
                }],
            })
        };
        let bind_transform_terreno = bind_transform("transform-terreno", &identidade_buf);

        // Repetir e o padrao do glTF; texturas de fachada dependem disso.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sampler-repeat"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let sampler_borda = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sampler-clamp"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let branca = criar_textura_rgba(device, queue, "branca-1x1", 1, 1, &[255, 255, 255, 255]);

        // --- terreno ----------------------------------------------------------
        let (iw, ih) = (scene.imagery.width(), scene.imagery.height());
        if iw > limite || ih > limite {
            anyhow::bail!(
                "mosaico de imagery {iw}x{ih} excede o limite de textura da GPU ({limite}). \
                 Reduza --zoom-img ou --lado."
            );
        }
        let view_imagery =
            criar_textura_rgba(device, queue, "imagery", iw, ih, &scene.imagery.rgba);

        let vertices_terreno: Vec<GpuVertex> = scene
            .mesh
            .vertices
            .iter()
            .map(|v| GpuVertex {
                position: v.position,
                normal: v.normal,
                uv: v.uv,
            })
            .collect();

        let terreno = MalhaGpu {
            vertex_buf: buffer_vertices(device, "terreno", &vertices_terreno),
            index_buf: buffer_indices(device, "terreno", &scene.mesh.indices),
            lotes: vec![Lote {
                bind_group: bind_material(
                    device,
                    &layout_material,
                    "terreno-material",
                    &uniform_material(
                        device,
                        "terreno",
                        MaterialUniform {
                            base_color: [1.0, 1.0, 1.0, 1.0],
                            flags: [1.0, 0.0, 0.0, 0.0],
                        },
                    ),
                    &view_imagery,
                    &sampler_borda,
                ),
                offset: 0,
                count: scene.mesh.indices.len() as u32,
            }],
        };

        // --- modelo do usuario --------------------------------------------------
        let mut texturas_enviadas = 0;
        let mut bytes_de_textura = 0;

        let modelo = match &scene.modelo {
            None => None,
            Some(m) => {
                let vistas: Vec<wgpu::TextureView> = m
                    .texturas
                    .iter()
                    .map(|t| {
                        texturas_enviadas += 1;
                        bytes_de_textura += t.bytes();
                        criar_textura_rgba(
                            device,
                            queue,
                            &t.nome,
                            t.largura.min(limite).max(1),
                            t.altura.min(limite).max(1),
                            &t.rgba,
                        )
                    })
                    .collect();

                // Vertices no espaco do arquivo: a posicao no mundo vem da matriz
                // em group(2). Assim mover o objeto nao reenvia a malha.
                let origem: &[arcz_model::ModelVertex] = match &scene.fonte_modelo {
                    Some(f) => &f.vertices,
                    None => &m.vertices,
                };
                let vs: Vec<GpuVertex> = origem
                    .iter()
                    .map(|v| GpuVertex {
                        position: v.position,
                        normal: v.normal,
                        uv: v.uv,
                    })
                    .collect();

                let lotes = m
                    .submeshes
                    .iter()
                    .map(|s| {
                        let mat = &m.materiais[s.material];
                        let tex = mat.textura.and_then(|i| vistas.get(i));
                        let uniform = uniform_material(
                            device,
                            &mat.nome,
                            MaterialUniform {
                                base_color: mat.base_color,
                                flags: [
                                    if tex.is_some() { 1.0 } else { 0.0 },
                                    // Corte de alpha so em material transparente; num
                                    // material opaco o alpha da textura costuma ser lixo
                                    // e recortaria a fachada inteira.
                                    if mat.transparente { 0.35 } else { 0.0 },
                                    0.0,
                                    0.0,
                                ],
                            },
                        );
                        Lote {
                            bind_group: bind_material(
                                device,
                                &layout_material,
                                &mat.nome,
                                &uniform,
                                tex.unwrap_or(&branca),
                                &sampler,
                            ),
                            offset: s.offset,
                            count: s.count,
                        }
                    })
                    .collect();

                Some(MalhaGpu {
                    vertex_buf: buffer_vertices(device, "modelo", &vs),
                    index_buf: buffer_indices(device, "modelo", &m.indices),
                    lotes,
                })
            }
        };

        // O modelo carregado por `--modelo` vira o objeto 0 da cena. Objetos da
        // biblioteca entram depois por `adicionar_objeto`.
        let mut objetos = Vec::new();
        if let Some(malha) = modelo {
            // A malha vai para a GPU no espaco do ARQUIVO, entao o transform do
            // objeto 0 tem que ser a MESMA matriz que `arcz_model::place` usa.
            // Com identidade aqui o predio era desenhado nas coordenadas cruas do
            // .glb — deslocado do lugar onde a cena diz que ele esta (no Zenite,
            // 5,6 m a oeste e 24 m ao sul). Bug real: obrigava a alinhar o predio
            // na mao e jogava para fora qualquer objeto posicionado em relacao a ele.
            let matriz = match &scene.fonte_modelo {
                Some(f) => arcz_model::matriz_modelo(
                    f.min,
                    f.max,
                    &scene.frame,
                    &scene.placement,
                    scene.solo_modelo_m,
                ),
                None => TransformUniform::identidade().modelo,
            };
            let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("transform-obj0"),
                contents: bytemuck::bytes_of(&TransformUniform { modelo: matriz }),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("transform-obj0-bind"),
                layout: &layout_transform,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buf.as_entire_binding(),
                }],
            });
            objetos.push(ObjetoGpu {
                id: 0,
                malha,
                transform_buf: buf,
                bind_transform: bind,
                visivel: true,
            });
        }

        // --- pipelines ----------------------------------------------------------
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cena"),
            source: wgpu::ShaderSource::Wgsl(include_str!("terrain.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cena-pipeline-layout"),
            bind_group_layouts: &[&layout_globais, &layout_material, &layout_transform],
            push_constant_ranges: &[],
        });

        let criar_pipeline = |rotulo: &str, cull: Option<wgpu::Face>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(rotulo),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<GpuVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2],
                    }],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: formato_alvo,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: cull,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: FORMATO_PROFUNDIDADE,
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::Less,
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            })
        };

        // Terreno: winding garantido por teste, entao descarta as costas.
        let pipeline_terreno = criar_pipeline("terreno-pipeline", Some(wgpu::Face::Back));
        // Modelo do usuario: winding vem do arquivo e costuma ser inconsistente
        // (espelhamento, normais invertidas no exportador). Culling faria paredes
        // inteiras sumirem sem nenhum erro — desenha os dois lados.
        let pipeline_modelo = criar_pipeline("modelo-pipeline", None);

        // --- ceu -----------------------------------------------------------------
        // So group(0): o ceu nao usa material nem textura.
        let shader_ceu = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ceu"),
            source: wgpu::ShaderSource::Wgsl(include_str!("sky.wgsl").into()),
        });
        let layout_ceu = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ceu-pipeline-layout"),
            bind_group_layouts: &[&layout_globais],
            push_constant_ranges: &[],
        });
        let pipeline_ceu = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ceu-pipeline"),
            layout: Some(&layout_ceu),
            vertex: wgpu::VertexState {
                module: &shader_ceu,
                entry_point: Some("vs_sky"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader_ceu,
                entry_point: Some("fs_sky"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: formato_alvo,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: FORMATO_PROFUNDIDADE,
                // Nao escreve profundidade e passa em qualquer teste: o ceu e o
                // fundo, e toda a geometria desenha por cima dele.
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // --- gizmo ---------------------------------------------------------------
        let shader_gizmo = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gizmo"),
            source: wgpu::ShaderSource::Wgsl(include_str!("gizmo.wgsl").into()),
        });
        let layout_gizmo = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gizmo-pipeline-layout"),
            bind_group_layouts: &[&layout_globais],
            push_constant_ranges: &[],
        });
        let pipeline_gizmo = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("gizmo-pipeline"),
            layout: Some(&layout_gizmo),
            vertex: wgpu::VertexState {
                module: &shader_gizmo,
                entry_point: Some("vs_gizmo"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<crate::gizmo::VerticeLinha>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader_gizmo,
                entry_point: Some("fs_gizmo"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: formato_alvo,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: FORMATO_PROFUNDIDADE,
                // Sempre por cima: um gizmo escondido atras do proprio objeto que
                // manipula seria inutil. Todo software 3D faz assim.
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let recursos = Self {
            pipeline_ceu,
            pipeline_gizmo,
            gizmo: None,
            bind_transform_terreno,
            objetos,
            layout_material,
            layout_transform,
            sampler_repeat: sampler,
            textura_branca: branca,
            limite_textura: limite,
            pipeline_terreno,
            pipeline_modelo,
            globais_buf,
            globais_bind,
            terreno,
            texturas_enviadas,
            bytes_de_textura,
        };
        log::info!(
            "GPU: {} draw calls, {} texturas do modelo ({:.1} MB) + imagery {}x{}",
            recursos.draw_calls(),
            recursos.texturas_enviadas,
            recursos.bytes_de_textura as f64 / (1024.0 * 1024.0),
            iw,
            ih
        );
        Ok(recursos)
    }

    /// Numero de chamadas de desenho por quadro. Sobe com a quantidade de materiais
    /// distintos — e o primeiro indicador de que o modelo precisa de atlas/merge.
    pub fn draw_calls(&self) -> usize {
        self.terreno.lotes.len()
            + self
                .objetos
                .iter()
                .map(|o| o.malha.lotes.len())
                .sum::<usize>()
    }

    /// Emite os draw calls. Compartilhado entre janela e offscreen.
    pub fn desenhar(&self, passe: &mut wgpu::RenderPass<'_>) {
        self.desenhar_com(passe, true);
    }

    /// Igual, mas permite omitir o modelo.
    ///
    /// Esconder o modelo e a forma mais direta de conferir alinhamento: alternar
    /// entre com e sem mostra na hora se a geometria esta cobrindo a rua certa
    /// da ortofoto.
    pub fn desenhar_com(&self, passe: &mut wgpu::RenderPass<'_>, mostrar_modelo: bool) {
        self.desenhar_completo(passe, mostrar_modelo, Camadas::TUDO);
    }

    /// Desenha escolhendo o que entra no quadro.
    ///
    /// Serve a composição sobre foto: para sobrepor o empreendimento a uma
    /// imagem do local, o quadro tem de sair **só com o projeto**, sobre fundo
    /// transparente — céu e terreno taparia a foto.
    ///
    /// O terreno continua desenhável separadamente porque ele às vezes é
    /// necessário mesmo na composição: sem ele o prédio não recebe a sombra
    /// projetada no chão nem o recorte do relevo à frente.
    pub fn desenhar_completo(
        &self,
        passe: &mut wgpu::RenderPass<'_>,
        mostrar_modelo: bool,
        camadas: Camadas,
    ) {
        passe.set_bind_group(0, &self.globais_bind, &[]);

        // Ceu primeiro: nao escreve profundidade, entao serve de fundo.
        if camadas.ceu {
            passe.set_pipeline(&self.pipeline_ceu);
            passe.draw(0..3, 0..1);
        }

        if !camadas.terreno {
            if mostrar_modelo {
                passe.set_pipeline(&self.pipeline_modelo);
                for o in self.objetos.iter().filter(|o| o.visivel) {
                    passe.set_bind_group(2, &o.bind_transform, &[]);
                    desenhar_malha(passe, &o.malha);
                }
            }
            self.desenhar_gizmo(passe);
            return;
        }

        passe.set_pipeline(&self.pipeline_terreno);
        passe.set_bind_group(2, &self.bind_transform_terreno, &[]);
        desenhar_malha(passe, &self.terreno);

        if mostrar_modelo {
            passe.set_pipeline(&self.pipeline_modelo);
            for o in self.objetos.iter().filter(|o| o.visivel) {
                passe.set_bind_group(2, &o.bind_transform, &[]);
                desenhar_malha(passe, &o.malha);
            }
        }

        self.desenhar_gizmo(passe);
    }

    /// Gizmo por ultimo, sempre visivel.
    fn desenhar_gizmo(&self, passe: &mut wgpu::RenderPass<'_>) {
        if let Some((buf, n)) = &self.gizmo {
            passe.set_pipeline(&self.pipeline_gizmo);
            passe.set_vertex_buffer(0, buf.slice(..));
            passe.draw(0..*n, 0..1);
        }
    }

    /// Move/gira/escala um objeto trocando so a matriz — 64 bytes em vez da malha.
    pub fn atualizar_transform(&self, queue: &wgpu::Queue, id: u32, modelo: [[f32; 4]; 4]) {
        if let Some(o) = self.objetos.iter().find(|o| o.id == id) {
            queue.write_buffer(
                &o.transform_buf,
                0,
                bytemuck::bytes_of(&TransformUniform { modelo }),
            );
        }
    }

    /// Liga/desliga a visibilidade de um objeto. Usado pelo Outliner (proximo passo).
    #[allow(dead_code)]
    pub fn definir_visivel(&mut self, id: u32, visivel: bool) {
        if let Some(o) = self.objetos.iter_mut().find(|o| o.id == id) {
            o.visivel = visivel;
        }
    }

    /// Substitui as linhas do gizmo. Passar vazio esconde o gizmo.
    pub fn atualizar_gizmo(
        &mut self,
        device: &wgpu::Device,
        linhas: &[crate::gizmo::VerticeLinha],
    ) {
        if linhas.is_empty() {
            self.gizmo = None;
            return;
        }
        // Recriar o buffer e barato aqui: sao algumas centenas de vertices, contra os
        // quase um milhao do modelo.
        let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("gizmo-linhas"),
            contents: bytemuck::cast_slice(linhas),
            usage: wgpu::BufferUsages::VERTEX,
        });
        self.gizmo = Some((buf, linhas.len() as u32));
    }

    /// Adiciona um objeto do `Editor` aos recursos de GPU. Faz upload da geometria
    /// (vertices + indices + texturas) e da matriz de transformacao.
    ///
    /// Idempotente quanto ao id: se ja existe um objeto com esse id, nao faz nada.
    /// `frame` e `solo_m` vem do `Scene` carregado e dao a posicao geografica do
    /// objeto no mundo.
    #[allow(dead_code)] // sera usado pela UI Tauri+React na E.5
    pub fn adicionar_objeto(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &arcz_geo::EnuFrame,
        obj: &crate::cena::Objeto,
        solo_m: f64,
    ) -> anyhow::Result<()> {
        if self.objetos.iter().any(|o| o.id == obj.id) {
            // Id ja existe. Nao duplica — o caller deveria ter atualizado.
            return Ok(());
        }

        // Calcula a matriz de mundo. Vertices continuam no espaco do arquivo;
        // o shader aplica a matriz via group(2).
        let matriz =
            arcz_model::matriz_modelo(obj.fonte.min, obj.fonte.max, frame, &obj.placement, solo_m);

        // Upload dos vertices (espaco do arquivo).
        let vertices_gpu: Vec<GpuVertex> = obj
            .fonte
            .vertices
            .iter()
            .map(|v| GpuVertex {
                position: v.position,
                normal: v.normal,
                uv: v.uv,
            })
            .collect();

        // Upload das texturas (com downscale pelo limite da GPU).
        let limite = self.limite_textura;
        let vistas: Vec<wgpu::TextureView> = obj
            .texturas
            .iter()
            .map(|t| {
                self.texturas_enviadas += 1;
                self.bytes_de_textura += t.bytes();
                criar_textura_rgba(
                    device,
                    queue,
                    &t.nome,
                    t.largura.min(limite).max(1),
                    t.altura.min(limite).max(1),
                    &t.rgba,
                )
            })
            .collect();

        // Um lote por submesh (material). Texturas ausentes caem na branca 1x1.
        let sampler = &self.sampler_repeat;
        let branca = &self.textura_branca;
        let lotes: Vec<Lote> = obj
            .submeshes
            .iter()
            .map(|s| {
                let mat = &obj.materiais[s.material];
                let tex = mat.textura.and_then(|i| vistas.get(i));
                let uniform = uniform_material(
                    device,
                    &mat.nome,
                    MaterialUniform {
                        base_color: mat.base_color,
                        flags: [
                            if tex.is_some() { 1.0 } else { 0.0 },
                            if mat.transparente { 0.35 } else { 0.0 },
                            0.0,
                            0.0,
                        ],
                    },
                );
                Lote {
                    bind_group: bind_material(
                        device,
                        &self.layout_material,
                        &mat.nome,
                        &uniform,
                        tex.unwrap_or(branca),
                        sampler,
                    ),
                    offset: s.offset,
                    count: s.count,
                }
            })
            .collect();

        let malha = MalhaGpu {
            vertex_buf: buffer_vertices(device, &format!("obj{}-vertices", obj.id), &vertices_gpu),
            index_buf: buffer_indices(device, &format!("obj{}", obj.id), &obj.indices),
            lotes,
        };

        // Transform buffer ja inicializado com a matriz (evita 1 write_buffer no
        // primeiro frame).
        let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("transform-obj{}", obj.id)),
            contents: bytemuck::bytes_of(&TransformUniform { modelo: matriz }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("transform-obj{}-bind", obj.id)),
            layout: &self.layout_transform,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buf.as_entire_binding(),
            }],
        });

        self.objetos.push(ObjetoGpu {
            id: obj.id,
            malha,
            transform_buf: buf,
            bind_transform: bind,
            visivel: obj.visivel,
        });

        Ok(())
    }

    /// Remove um objeto pelo id. Nao faz nada se nao existir.
    #[allow(dead_code)]
    pub fn remover_objeto(&mut self, id: u32) {
        self.objetos.retain(|o| o.id != id);
    }

    /// Faixa de ids reservada ao entorno procedural.
    ///
    /// Objetos do usuario usam ids pequenos e crescentes. Separar a faixa deixa
    /// `limpar_entorno` remover so o que veio do OSM, sem tocar no que o usuario
    /// carregou ou posicionou.
    pub const ID_BASE_ENTORNO: u32 = 1_000_000;

    /// Descarta o entorno, preservando os objetos do usuario.
    pub fn limpar_entorno(&mut self) {
        self.objetos.retain(|o| o.id < Self::ID_BASE_ENTORNO);
    }
}

fn desenhar_malha(passe: &mut wgpu::RenderPass<'_>, malha: &MalhaGpu) {
    passe.set_vertex_buffer(0, malha.vertex_buf.slice(..));
    passe.set_index_buffer(malha.index_buf.slice(..), wgpu::IndexFormat::Uint32);
    for lote in &malha.lotes {
        passe.set_bind_group(1, &lote.bind_group, &[]);
        passe.draw_indexed(lote.offset..lote.offset + lote.count, 0, 0..1);
    }
}

fn buffer_vertices(device: &wgpu::Device, rotulo: &str, v: &[GpuVertex]) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(&format!("{rotulo}-vertices")),
        contents: bytemuck::cast_slice(v),
        usage: wgpu::BufferUsages::VERTEX,
    })
}

fn buffer_indices(device: &wgpu::Device, rotulo: &str, i: &[u32]) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(&format!("{rotulo}-indices")),
        contents: bytemuck::cast_slice(i),
        usage: wgpu::BufferUsages::INDEX,
    })
}

fn uniform_material(device: &wgpu::Device, rotulo: &str, m: MaterialUniform) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(&format!("mat-{rotulo}")),
        contents: bytemuck::bytes_of(&m),
        usage: wgpu::BufferUsages::UNIFORM,
    })
}

fn bind_material(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    rotulo: &str,
    uniform: &wgpu::Buffer,
    textura: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(&format!("bind-{rotulo}")),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(textura),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

fn criar_textura_rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    rotulo: &str,
    largura: u32,
    altura: u32,
    rgba: &[u8],
) -> wgpu::TextureView {
    let tamanho = wgpu::Extent3d {
        width: largura,
        height: altura,
        depth_or_array_layers: 1,
    };
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(rotulo),
        size: tamanho,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    // Se o buffer nao bate com as dimensoes (textura recortada pelo limite da GPU),
    // completa em branco em vez de deixar o wgpu abortar.
    let esperado = (largura as usize) * (altura as usize) * 4;
    let dados: std::borrow::Cow<[u8]> = if rgba.len() == esperado {
        std::borrow::Cow::Borrowed(rgba)
    } else {
        let mut v = rgba.to_vec();
        v.resize(esperado, 255);
        std::borrow::Cow::Owned(v)
    };

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &dados,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(largura * 4),
            rows_per_image: Some(altura),
        },
        tamanho,
    );
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}

pub fn criar_depth(device: &wgpu::Device, largura: u32, altura: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width: largura.max(1),
                height: altura.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMATO_PROFUNDIDADE,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

/// Cor de fundo, igual nos dois caminhos de render.
pub const FUNDO: wgpu::Color = wgpu::Color {
    r: 0.05,
    g: 0.07,
    b: 0.10,
    a: 1.0,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_do_vertice_bate_com_o_shader() {
        // O shader declara vec3/vec3/vec2 em offsets 0/12/24, stride 32.
        assert_eq!(std::mem::size_of::<GpuVertex>(), 32);
        assert_eq!(std::mem::offset_of!(GpuVertex, position), 0);
        assert_eq!(std::mem::offset_of!(GpuVertex, normal), 12);
        assert_eq!(std::mem::offset_of!(GpuVertex, uv), 24);
    }

    #[test]
    fn uniforms_respeitam_o_alinhamento_de_16_bytes() {
        // WGSL exige tamanho multiplo de 16 em uniform buffer.
        assert_eq!(std::mem::size_of::<Globais>() % 16, 0);
        assert_eq!(std::mem::size_of::<MaterialUniform>() % 16, 0);
        // 2 matrizes (64 cada) + 3 vec4 (16 cada).
        assert_eq!(std::mem::size_of::<Globais>(), 176);
        assert_eq!(std::mem::size_of::<MaterialUniform>(), 32);
    }

    /// Os dois shaders declaram a mesma struct `Globais`. Divergir nao gera erro de
    /// compilacao — gera leitura do campo errado, com sintoma sutil (cena escura).
    /// Este teste ja pegou exatamente esse bug uma vez.
    #[test]
    fn os_shaders_concordam_com_o_layout_de_globais() {
        let terreno = include_str!("terrain.wgsl");
        let ceu = include_str!("sky.wgsl");

        for campo in ["view_proj", "inv_view_proj", "luz", "camera", "vista"] {
            assert!(
                terreno.contains(&format!("{campo}:")),
                "terrain.wgsl nao declara {campo}"
            );
            assert!(
                ceu.contains(&format!("{campo}:")),
                "sky.wgsl nao declara {campo}"
            );
        }

        // A ordem tambem importa: os campos sao lidos por offset.
        let ordem = |src: &str| {
            ["view_proj", "inv_view_proj", "luz", "camera", "vista"]
                .iter()
                .map(|c| src.find(&format!("{c}:")).unwrap_or(usize::MAX))
                .collect::<Vec<_>>()
        };
        let o = ordem(terreno);
        assert!(
            o.windows(2).all(|p| p[0] < p[1]),
            "terrain.wgsl declara Globais fora de ordem"
        );
        let o = ordem(ceu);
        assert!(
            o.windows(2).all(|p| p[0] < p[1]),
            "sky.wgsl declara Globais fora de ordem"
        );
    }

    #[test]
    fn o_shader_declara_os_dois_grupos_de_bind() {
        let src = include_str!("terrain.wgsl");
        for esperado in [
            "fn vs_main",
            "fn fs_main",
            "@group(0) @binding(0)",
            "@group(1) @binding(0)",
            "@group(1) @binding(1)",
            "@group(1) @binding(2)",
            "@location(0) position",
            "@location(1) normal",
            "@location(2) uv",
        ] {
            assert!(src.contains(esperado), "shader nao declara {esperado}");
        }
    }

    #[test]
    fn o_shader_usa_iluminacao_de_dois_lados() {
        // Geometria importada tem normais invertidas com frequencia; sem abs() as
        // paredes viradas ao contrario ficam pretas.
        let src = include_str!("terrain.wgsl");
        assert!(
            src.contains("abs(dot(n, l))"),
            "iluminacao nao e de dois lados"
        );
        assert!(
            src.contains("discard"),
            "sem corte de alpha para folhagem/vidro"
        );
    }
}

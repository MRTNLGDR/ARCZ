//! Servidor de preview: pagina com controles que re-renderiza a cada ajuste.
//!
//! Existe para resolver o alinhamento do modelo sobre a ortofoto. Comparar um PNG
//! estatico com um print do Google e adivinhar offsets nao converge; aqui os
//! controles mexem em lat/lon/rumo/offset e a imagem responde na hora, com a
//! ortofoto real por baixo.
//!
//! Single-thread de proposito: ha uma GPU e um `Renderer`, e servir dois quadros ao
//! mesmo tempo nao aceleraria nada. Uma requisicao por vez mantem o codigo sem
//! `Mutex` e sem estado compartilhado.

use std::io::Cursor;

use arcz_model::{FonteGeometria, Placement};
use tiny_http::{Header, Method, Response, Server};

use crate::camera::OrbitCamera;
use crate::cena::Editor;
use crate::config::Config;
use crate::renderer::Renderer;
use crate::scene::Scene;

/// Onde as camaras gravadas ficam. Arquivo, e nao localStorage, para o render em
/// lote da Fatia 4 poder consumir a mesma lista.
const ARQUIVO_CAMERAS: &str = "preview/cameras.json";

/// Parametros que a pagina manda por query string.
#[derive(Debug, Clone, Copy)]
struct Params {
    lat: f64,
    lon: f64,
    heading: f64,
    leste: f32,
    norte: f32,
    vertical: f32,
    escala: f32,
    pitch: f64,
    yaw: f64,
    dist: f64,
    /// Deslocamento do ponto observado, em metros. E o "pan" da camera.
    alvo_leste: f64,
    alvo_norte: f64,
    /// Altura do ponto observado acima do centro do modelo, em metros.
    ///
    /// Sem isto o alvo ficava grudado no meio da caixa e nao havia como pôr o
    /// olho na altura de quem anda: o modo caminhar sairia flutuando na metade
    /// da altura do predio.
    alvo_vertical: f64,
    largura: u32,
    altura: u32,
    mostrar_modelo: bool,
    /// Momento simulado, para posicionar o Sol.
    mes: u32,
    dia: u32,
    hora: f64,
    /// Gizmo a desenhar sobre o objeto selecionado. None = modo camera.
    gizmo: Option<crate::gizmo::ModoGizmo>,
    /// Snap pedido pelo cliente. `None` = arrasto livre.
    snap: Option<crate::gizmo::SnappingConfig>,
    /// Altura do plano de corte, em metros acima da base do modelo.
    ///
    /// `None` = sem corte. É o que permite ver o mobiliário: os móveis ficam
    /// dentro de um prédio fechado de 936 mil triângulos, e sem cortar a
    /// cobertura não há como olhar para dentro.
    corte_m: Option<f32>,
}

impl Params {
    fn dos_padroes(scene: &Scene, camera: &OrbitCamera, p: &Placement) -> Self {
        Self {
            lat: p.lat_deg,
            lon: p.lon_deg,
            heading: p.heading_deg,
            leste: p.offset_leste_m,
            norte: p.offset_norte_m,
            vertical: p.offset_vertical_m,
            escala: p.escala,
            pitch: camera.pitch.to_degrees(),
            yaw: camera.yaw.to_degrees(),
            dist: camera.distancia,
            alvo_leste: 0.0,
            alvo_norte: 0.0,
            alvo_vertical: 0.0,
            largura: 1280,
            altura: 800,
            mostrar_modelo: true,
            mes: 3,
            dia: 21,
            hora: 15.0,
            gizmo: None,
            snap: None,
            corte_m: None,
        }
        .com_centro_se_zerado(scene)
    }

    fn com_centro_se_zerado(mut self, scene: &Scene) -> Self {
        if self.lat == 0.0 && self.lon == 0.0 {
            let c = scene.bbox.center();
            self.lat = c.lat_deg;
            self.lon = c.lon_deg;
        }
        self
    }

    /// Sobrescreve o que vier na query string; o resto fica no padrao.
    fn aplicar_query(mut self, query: &str) -> Self {
        for par in query.split('&') {
            let Some((chave, valor)) = par.split_once('=') else {
                continue;
            };
            match chave {
                "lat" => set(&mut self.lat, valor),
                "lon" => set(&mut self.lon, valor),
                "heading" => set(&mut self.heading, valor),
                "leste" => set(&mut self.leste, valor),
                "norte" => set(&mut self.norte, valor),
                "vertical" => set(&mut self.vertical, valor),
                "escala" => set(&mut self.escala, valor),
                "pitch" => set(&mut self.pitch, valor),
                "yaw" => set(&mut self.yaw, valor),
                "dist" => set(&mut self.dist, valor),
                "alvo_leste" => set(&mut self.alvo_leste, valor),
                "alvo_norte" => set(&mut self.alvo_norte, valor),
                "alvo_vertical" => set(&mut self.alvo_vertical, valor),
                // Teto de resolucao: um pedido de 20000px travaria a GPU.
                "w" => set_limitado(&mut self.largura, valor, 64, 3840),
                "h" => set_limitado(&mut self.altura, valor, 64, 2160),
                "modelo" => self.mostrar_modelo = valor != "0",
                "mes" => set(&mut self.mes, valor),
                "dia" => set(&mut self.dia, valor),
                "hora" => set(&mut self.hora, valor),
                // Altura do corte em metros. Vazio ou <= 0 desliga.
                "corte" => {
                    self.corte_m = valor.parse::<f32>().ok().filter(|v| *v > 0.0);
                }

                // Passo do snap em metros. 0 desliga; ausente mantem livre.
                "snap" => {
                    if let Ok(v) = valor.parse::<f32>() {
                        self.snap = (v > 0.0).then_some(crate::gizmo::SnappingConfig {
                            grid_snap_m: Some(v),
                            // O snap angular acompanha o de grade: quem pede
                            // posicao alinhada quase sempre quer rumo alinhado.
                            angle_snap_deg: Some(15.0),
                            terrain_snap: false,
                        });
                    }
                }

                "gizmo" => {
                    self.gizmo = match valor {
                        "mover" => Some(crate::gizmo::ModoGizmo::Mover),
                        "girar" => Some(crate::gizmo::ModoGizmo::Girar),
                        "escalar" => Some(crate::gizmo::ModoGizmo::Escalar),
                        _ => None,
                    }
                }
                _ => {}
            }
        }
        self
    }

    fn placement(&self, base: &Placement) -> Placement {
        let p = Placement {
            lat_deg: self.lat,
            lon_deg: self.lon,
            heading_deg: self.heading,
            escala: if self.escala > 0.0 { self.escala } else { 1.0 },
            offset_leste_m: self.leste,
            offset_norte_m: self.norte,
            offset_vertical_m: self.vertical,
            assentar_no_terreno: base.assentar_no_terreno,
        };

        // O snap e aplicado **aqui**, no Rust, e nao no JavaScript do preview.
        // Antes o navegador arredondava antes de mandar, o que fazia a UI ser a
        // dona da regra: outro cliente (CLI, render em lote, a UI Tauri) pegaria
        // valores nao alinhados. O nucleo e a fonte autoritativa, entao o
        // arredondamento acontece do lado do servidor e o cliente so informa o
        // passo desejado.
        match self.snap {
            Some(cfg) => crate::gizmo::aplicar_snapping_placement(p, &cfg),
            None => p,
        }
    }
}

fn set<T: std::str::FromStr>(destino: &mut T, valor: &str) {
    if let Ok(v) = valor.parse::<T>() {
        *destino = v;
    }
}

fn set_limitado(destino: &mut u32, valor: &str, min: u32, max: u32) {
    if let Ok(v) = valor.parse::<u32>() {
        *destino = v.clamp(min, max);
    }
}

/// Ate este nivel o terreno e anunciado para o mundo inteiro (plano fora da
/// regiao carregada). Cobre o globo visto de longe sem baixar DEM de lugar
/// nenhum.
const NIVEL_BASE_TERRENO: u8 = 5;

/// Detalhe maximo anunciado dentro da regiao.
///
/// O DEM tem ~30 m de resolucao, entao acima de certo nivel nao ha informacao
/// nova a acrescentar — mas PARAR de anunciar faz o Cesium parar de refinar o
/// terreno, e com ele a imagery grudada nele. Era o "limite de zoom": ao
/// aproximar do predio a foto aerea congelava borrada. Anunciar ate 18 mantem o
/// refinamento; os tiles extras sao interpolados do mesmo DEM, o que custa
/// pouco e nao inventa relevo que nao existe.
const NIVEL_MAX_TERRENO: u8 = 18;

/// Estado que sobrevive entre requisicoes.
struct Estado {
    cfg: Config,
    scene: Scene,
    fonte: Option<FonteGeometria>,
    renderer: Renderer,
    editor: Option<Editor>,
    rt: tokio::runtime::Runtime,
    /// Ultimo placement enviado a GPU. Retransformar 936 mil vertices e reenviar
    /// 30 MB a cada quadro era o gargalo real do preview — girar a camera nao
    /// muda a geometria, entao so refaz quando o placement muda de fato.
    ultimo_placement: Option<Placement>,
    /// Caixa envolvente do modelo ja transformado, para enquadrar sem refazer a conta.
    caixa_modelo: Option<([f32; 3], [f32; 3])>,
    /// Projeto ligado a sessao. Quando presente, cada mudanca de posicao dispara
    /// autosave — o trabalho sobrevive a uma queda sem o usuario ter salvo.
    projeto: Option<(crate::workspace::Workspace, String)>,
    /// Banco autoritativo do projeto (`project.sqlite`). `None` quando a sessao
    /// nao esta ligada a um projeto — render em lote, por exemplo.
    store: Option<crate::db::SafeProjectStore>,
    /// Numero do proximo comando no diario. Cresce a cada gravacao.
    sequencia: std::cell::Cell<u64>,
    /// Alvo da camera, travado na primeira posicao do modelo.
    ///
    /// `None` = recalcular no proximo quadro. E o que o botao "Enquadrar" faz, e
    /// o que acontece ao trocar de area.
    alvo_travado: Option<[f32; 3]>,

    /// DEM da regiao, para o terreno `quantized-mesh` do globo.
    ///
    /// Carregado na primeira requisicao de `/terreno/`, e nao na subida: quem
    /// nunca abre o globo nao deve pagar o download. O `Option` externo marca
    /// "ainda nao tentei"; o interno, "tentei e nao veio" — sem os dois, uma
    /// falha de rede faria o servidor retentar a cada tile.
    dem_globo: Option<Option<arcz_terrain::HeightMosaic>>,
}

impl Estado {
    /// DEM da regiao, baixando na primeira chamada.
    fn dem_do_globo(&mut self) -> Option<&arcz_terrain::HeightMosaic> {
        if self.dem_globo.is_none() {
            let bbox = self.cfg.bbox().ok();
            self.dem_globo = Some(bbox.and_then(|bbox| {
                let cache = arcz_terrain::TileCache::new(arcz_terrain::TileCache::default_root()).ok()?;
                let fonte = self.cfg.dem;
                let zoom = self.cfg.zoom_dem;
                match self.rt.block_on(arcz_terrain::mosaic::fetch_height_mosaic(
                    &cache, fonte, &bbox, zoom,
                )) {
                    Ok(mut m) => {
                        // Mesmo tratamento do terreno 3D: sem achatar, a
                        // batimetria abre um buraco de mil metros no mar em
                        // volta da praia.
                        if !self.cfg.batimetria {
                            m.achatar_batimetria(0.0);
                        }
                        Some(m)
                    }
                    Err(e) => {
                        log::warn!("globo sem relevo: {e}");
                        None
                    }
                }
            }));
        }
        self.dem_globo.as_ref().and_then(|d| d.as_ref())
    }

    /// Recarrega a cena com uma area diferente e reconstroi os recursos de GPU.
    ///
    /// Custa ~1 s (baixa tiles novos; o cache em disco cobre repeticoes), entao so e
    /// chamado quando o tamanho da area ou o zoom muda de fato — nunca ao mexer na
    /// camera.
    fn recarregar_area(&mut self, lado_m: f64, zoom_img: Option<u8>) -> anyhow::Result<()> {
        let mut cfg = self.cfg.clone();
        cfg.lado_m = lado_m.clamp(50.0, 20_000.0);
        if let Some(z) = zoom_img {
            cfg.zoom_imagery = z.clamp(1, 19);
        }

        let scene = self.rt.block_on(crate::scene::carregar(&cfg))?;
        self.renderer = Renderer::new(&scene, self.editor.as_ref())?;
        self.fonte = scene.fonte_modelo.clone();
        self.cfg = cfg;
        self.scene = scene;
        // Recursos novos: a geometria precisa ser reenviada.
        self.ultimo_placement = None;
        self.caixa_modelo = None;
        self.alvo_travado = None;
        Ok(())
    }
}

/// Grava o autosave do projeto ligado a sessao, se houver.
///
/// Falha de escrita **nao** interrompe a edicao: perder um autosave e um
/// contratempo, travar o app no meio de um arrasto e pior. O erro vai para o log.
fn autossalvar(estado: &Estado, placement: &Placement) {
    let Some((w, slug)) = &estado.projeto else {
        return;
    };

    // O `.arcz` so existe depois do primeiro salvamento explicito. Quando ainda
    // nao existe, `abrir` falha — e isso **nao** pode impedir a gravacao no
    // SQLite, que e o formato autoritativo. Antes desta separacao o diario
    // ficava vazio em todo projeto novo, sem nenhum aviso.
    match w.abrir(slug) {
        Ok(mut p) => {
            // Reflete a posicao atual no objeto correspondente antes de gravar.
            if let Some(o) = p.objetos.first_mut() {
                o.lat = placement.lat_deg;
                o.lon = placement.lon_deg;
                o.heading_deg = placement.heading_deg;
                o.escala = placement.escala;
                o.offset_leste_m = placement.offset_leste_m;
                o.offset_norte_m = placement.offset_norte_m;
                o.offset_vertical_m = placement.offset_vertical_m;
            }
            if let Err(e) = w.autosave(slug, &p) {
                log::warn!("autosave de '{slug}' falhou: {e}");
            }
        }
        Err(e) => log::debug!("'{slug}' ainda sem projeto.arcz ({e}); segue no sqlite"),
    }

    // Alem do `.arcz`, grava no `project.sqlite` — que e o formato autoritativo
    // pedido pela spec: transacional, com WAL e diario de comandos. Os dois
    // convivem de proposito nesta fase: o `.arcz` continua sendo o que abre a
    // sessao, e o SQLite acumula o historico que ainda nao e lido de volta.
    // Registrar isso aqui em vez de depois evita perder o diario das edicoes
    // feitas enquanto o carregamento nao existe.
    if let Some(store) = &estado.store {
        let no = crate::cena::SceneNode::do_placement(slug, placement);
        if let Err(e) = store.salvar_cena(std::slice::from_ref(&no)) {
            log::warn!("project.sqlite: falha ao gravar a cena: {e}");
        }
        let entrada = crate::cena::JournalEntry {
            sequence_id: estado.sequencia.get(),
            // Sem relogio no `Estado`; o carimbo vem do proprio store na
            // gravacao. Aqui fica o que identifica o comando.
            timestamp: String::new(),
            command_name: "TransformNodeCommand".into(),
            payload_json: serde_json::json!({
                "lat": placement.lat_deg,
                "lon": placement.lon_deg,
                "heading_deg": placement.heading_deg,
                "escala": placement.escala,
                "offset_leste_m": placement.offset_leste_m,
                "offset_norte_m": placement.offset_norte_m,
                "offset_vertical_m": placement.offset_vertical_m,
            }),
        };
        if let Err(e) = store.registrar_journal(std::slice::from_ref(&entrada)) {
            log::warn!("project.sqlite: falha ao gravar o diario: {e}");
        }
        estado.sequencia.set(estado.sequencia.get() + 1);
    }
}

/// Sobe o servidor e bloqueia. `Ctrl+C` encerra.
pub fn servir(
    porta: u16,
    cfg: Config,
    mut scene: Scene,
    editor: Option<Editor>,
) -> anyhow::Result<()> {
    let servidor = Server::http(("127.0.0.1", porta))
        .map_err(|e| anyhow::anyhow!("nao consegui abrir a porta {porta}: {e}"))?;

    let renderer = Renderer::new(&scene, editor.as_ref())?;
    let fonte = scene.fonte_modelo.clone();

    // Sessao ligada a um projeto: habilita autosave a cada mudanca de posicao,
    // para o trabalho sobreviver a uma queda sem o usuario ter salvo.
    let cfg_projeto = match &cfg.projeto_slug {
        Some(slug) => {
            match crate::workspace::Workspace::new(crate::workspace::Workspace::raiz_padrao()) {
                Ok(w) => {
                    println!("  Autosave ligado ao projeto '{slug}'");
                    Some((w, slug.clone()))
                }
                Err(e) => {
                    log::warn!("autosave desligado: {e}");
                    None
                }
            }
        }
        None => None,
    };

    // Banco autoritativo do projeto, ao lado do `.arcz`. Falha ao abrir nao
    // impede editar: perder o diario e ruim, mas travar a sessao por causa dele
    // e pior. O erro vai para o log e o `.arcz` segue como antes.
    let store = cfg_projeto.as_ref().and_then(|(w, slug)| {
        // `pasta_de`, e nao `raiz().join(slug)`: os projetos ficam sob
        // `projetos/<slug>`, e o caminho errado so aparece em runtime.
        let pasta = w.pasta_de(slug);
        // O SQLite nao cria o diretorio; sem isto o `abrir` falha com "unable to
        // open database file" na primeira sessao de um projeto novo.
        if let Err(e) = std::fs::create_dir_all(&pasta) {
            log::warn!("nao criei {}: {e}", pasta.display());
        }
        let caminho = pasta.join("project.sqlite");
        match crate::db::SafeProjectStore::abrir(&caminho) {
            Ok(s) => {
                log::info!("project.sqlite em {}", caminho.display());
                Some(s)
            }
            Err(e) => {
                log::warn!("nao abri {}: {e}", caminho.display());
                None
            }
        }
    });

    // Retoma a posicao gravada na sessao anterior. O `.arcz` ja restaurava o
    // modelo; o SQLite passa a ser lido tambem, e ele e a fonte autoritativa —
    // e o que tem o diario. Gravar sem nunca ler deixava o banco crescendo sem
    // servir para nada.
    let mut placement_retomado = None;
    let mut sequencia_inicial = 1u64;
    if let Some(s) = &store {
        match s.carregar_cena() {
            Ok(nos) if !nos.is_empty() => {
                placement_retomado = nos.iter().find_map(|n| n.para_placement());
                if placement_retomado.is_some() {
                    println!("  Posicao retomada de project.sqlite ({} no(s))", nos.len());
                }
                // O diario continua de onde parou, em vez de sobrescrever as
                // entradas da sessao anterior.
                sequencia_inicial = s.proxima_sequencia().unwrap_or(1);
            }
            Ok(_) => {}
            Err(e) => log::warn!("project.sqlite: falha ao ler a cena: {e}"),
        }
    }
    if let Some(p) = placement_retomado {
        scene.placement = p;
    }

    let mut estado = Estado {
        cfg,
        scene,
        fonte,
        renderer,
        editor,
        rt: tokio::runtime::Runtime::new()?,
        ultimo_placement: None,
        caixa_modelo: None,
        projeto: cfg_projeto,
        store,
        sequencia: std::cell::Cell::new(sequencia_inicial),
        dem_globo: None,
        alvo_travado: None,
    };

    println!();
    println!("  Preview ao vivo:  http://127.0.0.1:{porta}");
    println!("  Ctrl+C para parar");
    println!();

    for requisicao in servidor.incoming_requests() {
        let url = requisicao.url().to_string();
        let (rota, query) = url.split_once('?').unwrap_or((url.as_str(), ""));
        let post = *requisicao.method() == Method::Post;

        // POST /cameras precisa consumir o corpo antes de responder.
        let mut corpo = String::new();
        let mut requisicao = requisicao;
        if post {
            let _ = requisicao.as_reader().read_to_string(&mut corpo);
        }

        // O modelo do usuario sai por streaming, ANTES do `match`.
        //
        // O `match` devolve `Response<Cursor<Vec<u8>>>`, o que obriga o corpo
        // inteiro a estar na memoria. Para o Zenite isso e 124 MB por pedido, e
        // o navegador pede o arquivo a cada troca de motor: passava de meio giga
        // e o processo morria sem panic nenhum no log — foi o que derrubou o
        // servidor. Aqui o `File` e o proprio corpo da resposta e o tiny_http o
        // copia em blocos para o socket.
        if !post && rota == "/modelo.glb" {
            match modelo_em_streaming(&estado) {
                Ok(r) => {
                    let _ = requisicao.respond(r);
                }
                Err(e) => {
                    log::warn!("/modelo.glb: {e}");
                    let _ = requisicao.respond(
                        Response::from_string(e).with_status_code(tiny_http::StatusCode(404)),
                    );
                }
            }
            continue;
        }

        let resultado = match (post, rota) {
            (false, "/" | "/index.html") => Ok(pagina()),
            (false, "/cesium" | "/cesium/index.html") => Ok(Response::from_string(pagina_cesium()).with_header(Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap())),
            (false, "/estado.json") => Ok(json(estado_json(&estado))),
            (false, "/cameras") => Ok(json(ler_cameras())),
            (true, "/cameras") => match gravar_cameras(&corpo) {
                Ok(()) => Ok(json("{\"ok\":true}".into())),
                Err(e) => Err(format!("nao consegui gravar {ARQUIVO_CAMERAS}: {e}")),
            },
            // Busca o entorno no OpenStreetMap, adensa e envia a GPU.
            //
            // Sincrono de proposito: a UI precisa saber quando o bairro esta na
            // cena para so entao pedir o proximo quadro. A primeira chamada
            // custa segundos (rede + Overpass); as seguintes leem do cache.
            // Tira o entorno GIS da cena e deixa so o projeto.
            //
            // Nao apaga nada de disco: o entorno e regeravel a partir do OSM em
            // segundos, e o cache local ja tem a resposta. Chamar `/entorno` de
            // novo o traz de volta identico — a geracao e deterministica.
            (false, "/entorno/limpar") => {
                let antes = estado
                    .editor
                    .as_ref()
                    .map(|e| {
                        e.objetos
                            .iter()
                            .filter(|o| crate::entorno::e_do_entorno(o.id))
                            .count()
                    })
                    .unwrap_or(0);
                if let Some(ed) = estado.editor.as_mut() {
                    crate::entorno::remover(ed);
                }
                estado.renderer.limpar_entorno();
                log::info!("entorno removido: {antes} objeto(s)");
                Ok(json(format!(
                    r#"{{"ok":true,"removidos":{antes},"nota":"chame /entorno para trazer de volta"}}"#
                )))
            }

            // Grava a posicao atual do modelo, sem esperar o autosave.
            //
            // O autosave ja dispara a cada movimento, mas ele e implicito: nao
            // ha como saber que gravou. Este e o botao explicito — devolve a
            // posicao gravada para a UI poder confirmar em vez de prometer.
            (false, "/salvar") => {
                let padroes = Params::dos_padroes(
                    &estado.scene,
                    &crate::viewport::camera_inicial(&estado.scene, estado.editor.as_ref()),
                    &estado.scene.placement,
                );
                let p = padroes.aplicar_query(query).placement(&estado.scene.placement);
                estado.scene.placement = p;
                autossalvar(&estado, &p);

                let onde = match &estado.projeto {
                    Some((w, slug)) => format!("{}", w.pasta_de(slug).display()),
                    None => String::new(),
                };
                let tem_projeto = !onde.is_empty();
                Ok(json(format!(
                    concat!(
                        r#"{{"ok":{},"lat":{:.7},"lon":{:.7},"heading":{:.3},"escala":{:.4},"#,
                        r#""leste":{:.2},"norte":{:.2},"vertical":{:.2},"onde":"{}"}}"#
                    ),
                    tem_projeto,
                    p.lat_deg,
                    p.lon_deg,
                    p.heading_deg,
                    p.escala,
                    p.offset_leste_m,
                    p.offset_norte_m,
                    p.offset_vertical_m,
                    onde.replace('\\', "/").replace('"', "'"),
                )))
            }

            (false, "/entorno") => {
                // Regra maxima do projeto: nada custa dinheiro sem autorizacao.
                // O entorno so usa Overpass e tiles publicos gratuitos, mas a
                // checagem fica aqui para que ligar uma fonte paga no futuro
                // esbarre nela em vez de cobrar em silencio.
                if !estado.cfg.politica_custos.e_totalmente_gratuita() {
                    Err("politica de custos nao esta em modo gratuito; \
                         o entorno usa apenas fontes publicas e nao foi executado"
                        .to_string())
                } else {
                    let lado = query
                        .split('&')
                        .find_map(|kv| kv.strip_prefix("lado="))
                        .and_then(|v| v.parse::<f64>().ok())
                        .unwrap_or(estado.cfg.lado_m)
                        .clamp(100.0, 5_000.0);
                    let adensar = !query.contains("adensar=0");

                    // EnuFrame e Copy e solo e f64: copiar destrava o emprestimo
                    // simultaneo de `scene` (imutavel) e `renderer` (mutavel).
                    let frame = estado.scene.frame;
                    let solo = estado.scene.solo_modelo_m;
                    // O entorno vira objeto editavel, entao precisa de um Editor.
                    // Uma sessao sem editor (render em lote) ganha um aqui.
                    if estado.editor.is_none() {
                        estado.editor = Some(Editor::default());
                    }
                    let Estado {
                        rt,
                        renderer,
                        editor,
                        scene,
                        ..
                    } = &mut estado;
                    let editor = editor.as_mut().expect("editor recem-criado");
                    let r = rt.block_on(crate::renderer::carregar_entorno(
                        renderer, editor, scene, &frame, solo, lado, adensar,
                    ));
                    match r {
                        Ok(rel) => Ok(json(
                            serde_json::to_string(&rel).unwrap_or_else(|_| "{}".into()),
                        )),
                        Err(e) => Err(format!("falha ao carregar o entorno: {e:#}")),
                    }
                }
            }
            (false, "/area") => {
                let p = QueryArea::de(query);
                match estado.recarregar_area(p.lado, p.zoom_img) {
                    Ok(()) => Ok(json(estado_json(&estado))),
                    Err(e) => Err(format!("falha ao recarregar a area: {e:#}")),
                }
            }
            // Onde o pixel (ndc_x, ndc_y) toca o plano horizontal na altura pedida.
            // O cliente chama no mousedown e a cada movimento; a diferenca entre as
            // duas respostas e o deslocamento em metros a aplicar no modelo.
            // Qual objeto esta sob o cursor. A UI chama no clique e usa o id
            // devolvido para marcar a selecao no Outliner e no Inspector.
            (false, "/picar") => {
                let padroes = Params::dos_padroes(
                    &estado.scene,
                    &crate::viewport::camera_inicial(&estado.scene, estado.editor.as_ref()),
                    &estado.scene.placement,
                );
                let p = padroes.aplicar_query(query);
                Ok(json(picar_json(&estado, &p, query)))
            }
            (false, "/plano") => {
                let padroes = Params::dos_padroes(
                    &estado.scene,
                    &crate::viewport::camera_inicial(&estado.scene, estado.editor.as_ref()),
                    &estado.scene.placement,
                );
                let p = padroes.aplicar_query(query);
                Ok(json(plano_json(&estado, &p, query)))
            }
            (false, "/render.jpg") => {
                let padroes = Params::dos_padroes(
                    &estado.scene,
                    &crate::viewport::camera_inicial(&estado.scene, estado.editor.as_ref()),
                    &estado.scene.placement,
                );
                let p = padroes.aplicar_query(query);
                let q = valor_de(query, "q")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(82);
                match renderizar_com(&mut estado, &p, Formato::Jpeg(q)) {
                    Ok(bytes) => Ok(Response::from_data(bytes).with_header(
                        Header::from_bytes(&b"Content-Type"[..], &b"image/jpeg"[..]).unwrap(),
                    )),
                    Err(e) => Err(format!("erro ao renderizar: {e:#}")),
                }
            }
            // O projeto recortado, sobre fundo transparente.
            //
            // E a peca que falta para sobrepor o empreendimento a uma foto do
            // local: o PNG sai so com a geometria do projeto e alpha zero no
            // resto, entao qualquer editor — ou a propria UI — compoe por cima
            // da imagem sem recorte manual.
            (false, "/composicao.png") => {
                let padroes = Params::dos_padroes(
                    &estado.scene,
                    &crate::viewport::camera_inicial(&estado.scene, estado.editor.as_ref()),
                    &estado.scene.placement,
                );
                let p = padroes.aplicar_query(query);
                match renderizar_com(&mut estado, &p, Formato::Composicao) {
                    Ok(bytes) => Ok(Response::from_data(bytes).with_header(
                        Header::from_bytes(&b"Content-Type"[..], &b"image/png"[..]).unwrap(),
                    )),
                    Err(e) => Err(format!("falha ao compor: {e:#}")),
                }
            }

            (false, "/render.png") => {
                let padroes = Params::dos_padroes(
                    &estado.scene,
                    &crate::viewport::camera_inicial(&estado.scene, estado.editor.as_ref()),
                    &estado.scene.placement,
                );
                let p = padroes.aplicar_query(query);
                match renderizar(&mut estado, &p) {
                    Ok(png) => Ok(Response::from_data(png).with_header(
                        Header::from_bytes(&b"Content-Type"[..], &b"image/png"[..]).unwrap(),
                    )),
                    Err(e) => {
                        log::error!("falha ao renderizar: {e:#}");
                        Err(format!("erro ao renderizar: {e:#}"))
                    }
                }
            }
            (false, "/outliner") => {
                let ed = estado.editor.get_or_insert_with(Editor::default);
                ed.sincronizar_nodes();
                let arvore = crate::cena::OutlinerService::construir_arvore(&ed.nodes);
                Ok(json(serde_json::to_string(&arvore).unwrap_or_else(|_| "[]".into())))
            }
            (false, "/inspector") => {
                let ed = estado.editor.get_or_insert_with(Editor::default);
                ed.sincronizar_nodes();
                let node_id = valor_de(query, "id");
                let node = node_id.and_then(|id| ed.nodes.iter().find(|n| n.id == id))
                    .or_else(|| ed.nodes.first());
                if let Some(n) = node {
                    let payload = crate::cena::InspectorService::extrair_payload(n);
                    Ok(json(serde_json::to_string(&payload).unwrap_or_else(|_| "{}".into())))
                } else {
                    Err("nenhum no selecionado ou disponivel na cena".into())
                }
            }
            (true, "/inspector") => {
                let ed = estado.editor.get_or_insert_with(Editor::default);
                ed.sincronizar_nodes();
                if let Ok(novo_payload) = serde_json::from_str::<crate::cena::InspectorPayload>(&corpo) {
                    let id = novo_payload.id.clone();
                    if let Some(pos) = ed.nodes.iter().position(|n| n.id == id) {
                        let mut node = ed.nodes[pos].clone();
                        let mut bus = std::mem::take(&mut ed.bus);
                        crate::cena::InspectorService::aplicar_edicao(&mut bus, ed, &mut node, novo_payload);
                        ed.nodes[pos] = node;
                        ed.bus = bus;
                        Ok(json("{\"ok\":true}".into()))
                    } else {
                        Err("no nao encontrado".into())
                    }
                } else {
                    Err("payload JSON do inspector invalido".into())
                }
            }
            (true, "/undo") => {
                let ed = estado.editor.get_or_insert_with(Editor::default);
                let mut bus = std::mem::take(&mut ed.bus);
                let desfez = bus.desfazer(ed);
                ed.bus = bus;
                Ok(json(format!("{{\"ok\":true,\"desfez\":{desfez}}}")))
            }
            (true, "/redo") => {
                let ed = estado.editor.get_or_insert_with(Editor::default);
                let mut bus = std::mem::take(&mut ed.bus);
                let refez = bus.refazer(ed);
                ed.bus = bus;
                Ok(json(format!("{{\"ok\":true,\"refez\":{refez}}}")))
            }
            (true, "/gis/ingest") => {
                let ed = estado.editor.get_or_insert_with(Editor::default);
                let req: crate::gis_worker::GisIngestRequest = serde_json::from_str(&corpo)
                    .unwrap_or_else(|_| crate::gis_worker::GisIngestRequest {
                        center_lat: estado.scene.bbox.center().lat_deg,
                        center_lon: estado.scene.bbox.center().lon_deg,
                        radius_m: 600.0,
                    });
                let worker = crate::gis_worker::GisContextWorker::novo("cache/gis");
                match estado.rt.block_on(worker.ingest_bbox(req)) {
                    Ok(res) => {
                        for n in &res.nodes {
                            if !ed.nodes.iter().any(|existente| existente.id == n.id) {
                                ed.nodes.push(n.clone());
                            }
                        }
                        Ok(json(serde_json::to_string(&res).unwrap_or_else(|_| "{}".into())))
                    }
                    Err(e) => Err(format!("falha no ingest GIS: {e:#}")),
                }
            }
            (true, "/reconstruct/ingest") => {
                let ed = estado.editor.get_or_insert_with(Editor::default);
                let req: crate::reconstruct_worker::IngestRealityAssetRequest = serde_json::from_str(&corpo)
                    .unwrap_or_else(|_| crate::reconstruct_worker::IngestRealityAssetRequest {
                        file_path: "cache/scan.ply".into(),
                        name: "Digitalizacao de Campo".into(),
                        asset_kind: crate::reconstruct_worker::RealityAssetKind::PointCloud,
                        georeference: None,
                    });
                let worker = crate::reconstruct_worker::ReconstructWorker::novo("cache/reconstruct");
                match worker.processar_asset(req) {
                    Ok(res) => {
                        ed.nodes.push(res.node.clone());
                        Ok(json(serde_json::to_string(&res).unwrap_or_else(|_| "{}".into())))
                    }
                    Err(e) => Err(format!("falha na reconstrucao: {e:#}")),
                }
            }
            (true, "/streetview/ingest") => {
                let ed = estado.editor.get_or_insert_with(Editor::default);
                let c = estado.scene.bbox.center();
                let worker = crate::streetview_worker::StreetViewWorker::novo("cache/panoramax");
                let item = crate::streetview_worker::PanoramaItem {
                    id: format!("px_{}", std::process::id()),
                    provider: crate::streetview_worker::PanoramaProvider::Panoramax,
                    georeference: crate::cena::Georeference64 {
                        latitude: c.lat_deg,
                        longitude: c.lon_deg,
                        altitude: 2.0,
                        heading: 0.0,
                    },
                    image_url: Some("https://panoramax.ign.fr/api/v1/items/demo/sd.jpg".into()),
                    embed_url: None,
                    capture_date: Some("2026-01-01".into()),
                    license: "CC-BY-SA-4.0".into(),
                };
                let res = worker.processar_panoramas(vec![item]);
                for n in &res.nodes {
                    ed.nodes.push(n.clone());
                }
                Ok(json(serde_json::to_string(&res).unwrap_or_else(|_| "{}".into())))
            }
            (true, "/cad/ingest") => {
                let ed = estado.editor.get_or_insert_with(Editor::default);
                let req: crate::cad_worker::IngestCadRequest = serde_json::from_str(&corpo)
                    .unwrap_or_else(|_| crate::cad_worker::IngestCadRequest {
                        file_path: "projetos/planta_executiva.dxf".into(),
                        format: crate::cad_worker::CadFormat::Dxf,
                        unit_scale: 0.001,
                        georeference: None,
                    });
                let worker = crate::cad_worker::CadWorker::novo();
                match worker.processar_cad(req) {
                    Ok(res) => {
                        for n in &res.nodes {
                            ed.nodes.push(n.clone());
                        }
                        Ok(json(serde_json::to_string(&res).unwrap_or_else(|_| "{}".into())))
                    }
                    Err(e) => Err(format!("falha ao ingestar CAD: {e:#}")),
                }
            }
            (true, "/archviz/instantiate") => {
                let ed = estado.editor.get_or_insert_with(Editor::default);
                let req: crate::archviz_worker::InstantiateAssetRequest = serde_json::from_str(&corpo)
                    .unwrap_or_else(|_| crate::archviz_worker::InstantiateAssetRequest {
                        asset_id: "cadeira_eames".into(),
                        name: "Cadeira Eames Archviz".into(),
                        category: crate::archviz_worker::ArchvizCategory::Furniture,
                        position: [0.0, 0.0, 0.0],
                        rotation_euler: [0.0, 0.0, 0.0],
                        scale: [1.0, 1.0, 1.0],
                        material_overrides: vec!["couro_preto".into()],
                    });
                let worker = crate::archviz_worker::ArchvizWorker::novo();
                match worker.instanciar_asset(req) {
                    Ok(res) => {
                        ed.nodes.push(res.node.clone());
                        Ok(json(serde_json::to_string(&res).unwrap_or_else(|_| "{}".into())))
                    }
                    Err(e) => Err(format!("falha ao instanciar asset archviz: {e:#}")),
                }
            }
            // Catalogo da biblioteca, para o painel de arrastar-e-soltar.
            //
            // Devolve a dimensao real de cada peca junto do nome. Sem ela o
            // usuario arrasta um sofa achando que tem 2 m e descobre que tem 167
            // — foi exatamente o que aconteceu com dois modelos do Sketchfab.
            (false, "/biblioteca") => {
                let raiz = std::path::Path::new("biblioteca");
                let itens: Vec<serde_json::Value> =
                    match std::fs::read_to_string(raiz.join("manifesto.json")) {
                        Ok(t) => serde_json::from_str(&t).unwrap_or_default(),
                        Err(_) => Vec::new(),
                    };

                let mut saida = Vec::with_capacity(itens.len());
                for it in &itens {
                    let Some(chave) = it["chave"].as_str() else {
                        continue;
                    };
                    // Medir abrindo o arquivo custaria segundos para 115 pecas a
                    // cada abertura do painel. O tamanho vem do cache do
                    // catalogo quando existir; senao a UI mostra "—".
                    let escala = it["escala"].as_f64().unwrap_or(1.0);
                    saida.push(serde_json::json!({
                        "chave": chave,
                        "nome": it["nome"].as_str().unwrap_or(chave),
                        "licenca": it["licenca"].as_str().unwrap_or("?"),
                        "escala": escala,
                        "categoria": categoria_do_item(chave),
                    }));
                }
                saida.sort_by(|a, b| {
                    let ca = a["categoria"].as_str().unwrap_or("");
                    let cb = b["categoria"].as_str().unwrap_or("");
                    ca.cmp(cb)
                        .then_with(|| a["nome"].as_str().cmp(&b["nome"].as_str()))
                });
                Ok(json(serde_json::to_string(&saida).unwrap_or_else(|_| "[]".into())))
            }

            // Solta uma peca da biblioteca na cena, na posicao do cursor.
            //
            // `leste`/`norte` vem do raio da UI contra o plano do terreno — a
            // mesma conta que o arrasto do gizmo usa, para a peca cair onde o
            // cursor estava e nao no centro da cena.
            (false, "/adicionar") => {
                let chave = valor_de(query, "chave").unwrap_or("").to_string();
                if chave.is_empty() || chave.contains("..") || chave.contains('/') {
                    Err("chave invalida".to_string())
                } else {
                    let leste = valor_de(query, "leste")
                        .and_then(|v| v.parse::<f32>().ok())
                        .unwrap_or(0.0);
                    let norte = valor_de(query, "norte")
                        .and_then(|v| v.parse::<f32>().ok())
                        .unwrap_or(0.0);
                    let vertical = valor_de(query, "vertical")
                        .and_then(|v| v.parse::<f32>().ok())
                        .unwrap_or(0.0);
                    let rot = valor_de(query, "rot")
                        .and_then(|v| v.parse::<f64>().ok())
                        .unwrap_or(0.0);

                    let biblioteca = std::path::Path::new("biblioteca");
                    match crate::planta::modelo_do_item(biblioteca, &chave) {
                        None => Err(format!("'{chave}' nao esta na biblioteca")),
                        Some(arquivo) => {
                            let escala = crate::planta::escala_do_arquivo(biblioteca, &chave);
                            let p = Placement {
                                lat_deg: estado.scene.placement.lat_deg,
                                lon_deg: estado.scene.placement.lon_deg,
                                heading_deg: rot,
                                escala,
                                assentar_no_terreno: true,
                                offset_vertical_m: vertical,
                                offset_leste_m: leste,
                                offset_norte_m: norte,
                            };
                            let scene_frame = estado.scene.frame;
                            let solo = estado.scene.solo_modelo_m;
                            let ed = estado.editor.get_or_insert_with(Editor::default);
                            match arcz_model::Model::load(&arquivo) {
                                Err(e) => Err(format!("nao abri '{chave}': {e}")),
                                Ok(model) => {
                                    let id = ed.adicionar_com_arquivo(
                                        chave.clone(),
                                        model,
                                        p,
                                        None,
                                        arquivo,
                                    );
                                    match id {
                                        None => Err("nao consegui adicionar".to_string()),
                                        Some(id) => {
                                            if let Some(o) = ed.get_mut(id) {
                                                o.transformar(&scene_frame, solo);
                                            }
                                            // Sobe para a GPU agora: sem isto a
                                            // peca so apareceria na proxima
                                            // recarga da area.
                                            let novo = ed.get(id).cloned();
                                            if let Some(o) = novo {
                                                if let Err(e) =
                                                    estado.renderer.adicionar_objeto(&o, solo)
                                                {
                                                    log::warn!("GPU: {e}");
                                                }
                                            }
                                            log::info!("adicionado '{chave}' como id {id}");
                                            Ok(json(format!(
                                                r#"{{"ok":true,"id":{id},"chave":"{chave}","escala":{escala}}}"#
                                            )))
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            (false, "/arquivos") => {
                let mut lista = Vec::new();
                if let Ok(entries) = std::fs::read_dir("cache") {
                    for entry in entries.flatten() {
                        if let Ok(meta) = entry.metadata() {
                            if meta.is_file() {
                                lista.push(serde_json::json!({
                                    "nome": entry.file_name().to_string_lossy().to_string(),
                                    "tamanho": meta.len(),
                                    "caminho": entry.path().to_string_lossy().to_string()
                                }));
                            }
                        }
                    }
                }
                Ok(json(serde_json::to_string(&lista).unwrap_or_else(|_| "[]".into())))
            }
            (true, "/upload") => {
                let _ = std::fs::create_dir_all("cache");
                let filename = valor_de(query, "nome").unwrap_or("modelo_uploaded.obj").to_string();
                let dest = std::path::Path::new("cache").join(&filename);
                match std::fs::write(&dest, corpo.as_bytes()) {
                    Ok(()) => {
                        let ed = estado.editor.get_or_insert_with(Editor::default);
                        let node_id = format!("uploaded_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs());
                        let mut node = crate::cena::SceneNode::novo(node_id, filename.clone(), crate::cena::NodeType::Building);
                        node.confidence = crate::cena::NodeConfidence::Observed;
                        node.asset_ref = Some(dest.to_string_lossy().to_string());
                        ed.nodes.push(node);
                        Ok(json(format!("{{\"ok\":true,\"arquivo\":\"{}\"}}", filename)))
                    }
                    Err(e) => Err(format!("erro ao gravar arquivo upload: {e:#}")),
                }
            }
            (true, "/raw_gis/ingest") => {
                let ed = estado.editor.get_or_insert_with(Editor::default);
                let center = estado.scene.bbox.center();
                let req: crate::raw_gis_worker::IngestRawGisRequest = serde_json::from_str(&corpo)
                    .unwrap_or(crate::raw_gis_worker::IngestRawGisRequest {
                        bbox_latlon: [center.lat_deg - 0.01, center.lon_deg - 0.01, center.lat_deg + 0.01, center.lon_deg + 0.01],
                        zoom: 15,
                        tile_kind: crate::raw_gis_worker::GisTileKind::SatelliteImagery,
                    });
                let worker = crate::raw_gis_worker::RawGisWorker::novo("cache/raw_gis");
                match worker.processar_raw_gis(req) {
                    Ok(res) => {
                        for n in &res.nodes {
                            ed.nodes.push(n.clone());
                        }
                        Ok(json(serde_json::to_string(&res).unwrap_or_else(|_| "{}".into())))
                    }
                    Err(e) => Err(format!("falha ao ingestar tiles raw GIS: {e:#}")),
                }
            }
            (true, "/sync/offline") => {
                let ed = estado.editor.get_or_insert_with(Editor::default);
                let c = estado.scene.bbox.center();
                let worker = crate::offline_sync_worker::OfflineGisSyncWorker::novo("cache/gis_offline", crate::offline_sync_worker::AutoSyncConfig::default());
                match worker.executar_sincronizacao([c.lat_deg - 0.01, c.lon_deg - 0.01, c.lat_deg + 0.01, c.lon_deg + 0.01]) {
                    Ok(report) => {
                        let nos = worker.gerar_nos_cena(c.lat_deg, c.lon_deg);
                        for n in nos {
                            if !ed.nodes.iter().any(|existente| existente.id == n.id) {
                                ed.nodes.push(n);
                            }
                        }
                        Ok(json(serde_json::to_string(&report).unwrap_or_else(|_| "{}".into())))
                    }
                    Err(e) => Err(format!("falha na sincronizacao offline: {e:#}")),
                }
            }
            (false, "/cesium/czml") => {
                let ed = estado.editor.get_or_insert_with(Editor::default);
                ed.sincronizar_nodes();
                let worker = crate::cesium_worker::CesiumWorker::novo();
                match worker.exportar_czml(&ed.nodes) {
                    Ok(res) => Ok(json(res.czml_json)),
                    Err(e) => Err(format!("falha ao exportar CZML CesiumJS: {e:#}")),
                }
            }
            (false, "/cesium/tileset.json") => {
                let ed = estado.editor.get_or_insert_with(Editor::default);
                ed.sincronizar_nodes();
                let worker = crate::cesium_worker::CesiumWorker::novo();
                match worker.exportar_czml(&ed.nodes) {
                    Ok(res) => Ok(json(res.tileset_json)),
                    Err(e) => Err(format!("falha ao exportar 3D Tileset CesiumJS: {e:#}")),
                }
            }
            // Entorno como glTF binario, para o CesiumJS consumir.
            //
            // O Cesium resolve globo, terreno, streaming, camera e picking — nao
            // vale reescrever nada disso. Mas ele **consome** glTF e 3D Tiles;
            // nao os gera. O caminho OSM -> footprint -> extrusao -> telhado ->
            // cor da ortofoto nao existe pronto em lugar nenhum, e e o que o
            // `arcz-osm` faz. Esta rota e a ponte entre os dois.
            (false, "/entorno.glb") => {
                let malhas = estado
                    .editor
                    .as_ref()
                    .map(crate::entorno::malhas_do_editor)
                    .unwrap_or_default();
                if malhas.is_empty() {
                    Err("nenhum entorno na cena; chame /entorno antes".to_string())
                } else {
                    let glb = arcz_osm::exportar_glb(&malhas);
                    log::info!(
                        "entorno.glb: {} malhas, {} KB",
                        malhas.len(),
                        glb.len() / 1024
                    );
                    Ok(Response::from_data(glb).with_header(
                        Header::from_bytes(&b"Content-Type"[..], &b"model/gltf-binary"[..])
                            .unwrap(),
                    ))
                }
            }

            // ---- CesiumJS vendorizado, servido do disco ----------------------
            //
            // Servido pelo proprio Rust, e nao por CDN: o ARCZ e offline-first, e
            // um globo que so abre com internet contraria a regra R001 do chain.
            // Relevo real no globo, gerado aqui do nosso proprio DEM.
            //
            // O formato `quantized-mesh` e aberto; o que a Cesium cobra e o
            // *servico* que o distribui. Produzindo o mesmo formato a partir do
            // AWS Terrain Tiles, o globo ganha relevo sem custo nem cadastro.
            // O modelo do usuario, para o globo.
            //
            // Ate aqui so o entorno ia para o Cesium: trocar de motor mostrava a
            // cidade sem o empreendimento, que e justamente o que interessa. O
            // arquivo vai cru; quem posiciona e a matriz ENU montada no HTML,
            // com o mesmo placement do render nativo.

            // Placement e camera atuais, para o globo espelhar o render nativo.
            (false, "/sincronia.json") => {
                let pl = &estado.scene.placement;
                let cam = crate::viewport::camera_inicial(&estado.scene, estado.editor.as_ref());
                let padroes = Params::dos_padroes(&estado.scene, &cam, pl);
                let p = padroes.aplicar_query(query);
                Ok(json(format!(
                    "{{\"lat\":{:.9},\"lon\":{:.9},\"heading\":{:.4},\"escala\":{:.6},\
                     \"leste\":{:.4},\"norte\":{:.4},\"vertical\":{:.4},\
                     \"solo_m\":{:.3},\"tem_modelo\":{}}}",
                    p.lat,
                    p.lon,
                    p.heading,
                    p.escala,
                    p.leste,
                    p.norte,
                    p.vertical,
                    estado.scene.solo_modelo_m,
                    estado.cfg.modelo.is_some(),
                )))
            }

            // Enche as piscinas do modelo.
            //
            // Um clique em vez de arrastar um plano azul ate parecer certo: o
            // revestimento diz onde a piscina esta e qual o tamanho dela, e a
            // lamina entra no mesmo quadro do predio — cai no lugar exato.
            (false, "/agua") => {
                // Reler o arquivo em vez de usar a geometria ja na GPU: o
                // detector precisa dos NOMES dos materiais, que a malha
                // enviada a placa nao carrega.
                match estado
                    .cfg
                    .modelo
                    .clone()
                    .ok_or_else(|| "nenhum modelo carregado".to_string())
                    .and_then(|c| {
                        arcz_model::Model::load(&c).map_err(|e| format!("nao reli o modelo: {e}"))
                    }) {
                    Err(e) => Err(e),
                    Ok(model) => {
                        let laminas = crate::agua::detectar(&model);
                        if laminas.is_empty() {
                            // Dizer "nao achei" e melhor que inserir um plano
                            // aleatorio e deixar o usuario descobrir depois.
                            Ok(json(
                                "{\"ok\":false,\"piscinas\":0,\"aviso\":\"nenhum revestimento de piscina no modelo\"}"
                                    .to_string(),
                            ))
                        } else {
                            let area: f32 = laminas.iter().map(|l| l.area()).sum();
                            let m = crate::agua::malha(&laminas);
                            // Mesmo placement do predio: e o que garante que a
                            // lamina acompanhe rumo, escala e posicao dele.
                            let p = estado.scene.placement.clone();
                            let scene_frame = estado.scene.frame;
                            let solo = estado.scene.solo_modelo_m;
                            let ed = estado.editor.get_or_insert_with(Editor::default);
                            // Remove a lamina anterior: chamar duas vezes
                            // empilharia aguas coplanares e daria z-fighting.
                            let antigas: Vec<_> = ed
                                .objetos
                                .iter()
                                .filter(|o| o.nome == "agua-piscina")
                                .map(|o| o.id)
                                .collect();
                            for id in &antigas {
                                ed.remover(*id);
                                estado.renderer.remover_objeto(*id);
                            }
                            match ed.adicionar("agua-piscina".to_string(), m, p, None) {
                                None => Err("nao consegui inserir a lamina".to_string()),
                                Some(id) => {
                                    if let Some(o) = ed.get_mut(id) {
                                        o.transformar(&scene_frame, solo);
                                    }
                                    if let Some(o) = ed.get(id).cloned() {
                                        if let Err(e) = estado.renderer.adicionar_objeto(&o, solo) {
                                            log::warn!("GPU: {e}");
                                        }
                                    }
                                    log::info!(
                                        "agua: {} piscina(s), {area:.1} m2, id {id}",
                                        laminas.len()
                                    );
                                    Ok(json(format!(
                                        "{{\"ok\":true,\"piscinas\":{},\"area_m2\":{area:.1},\"id\":{id}}}",
                                        laminas.len()
                                    )))
                                }
                            }
                        }
                    }
                }
            }

            (false, "/terreno/layer.json") => {
                let cobertura = estado.dem_do_globo().map(|d| d.bounds());
                Ok(json(arcz_terrain::quantized::layer_json(
                    cobertura,
                    NIVEL_BASE_TERRENO,
                    NIVEL_MAX_TERRENO,
                )))
            }
            (false, caminho) if caminho.starts_with("/terreno/") => {
                match tile_do_caminho(caminho, "/terreno/", ".terrain") {
                    Some((z, x, y)) => {
                        // Fora da regiao carregada o tile sai plano: sem o
                        // recorte, `sample_geodetic` grampearia na borda e
                        // esticaria o relevo do litoral pelo oceano inteiro.
                        let b = arcz_terrain::quantized::bounds_do_tile(z, x, y);
                        let dem = estado.dem_do_globo().filter(|d| {
                            let c = d.bounds();
                            b.west < c.east && b.east > c.west && b.south < c.north && b.north > c.south
                        });
                        let bytes = arcz_terrain::quantized::codificar(z, x, y, dem);
                        Ok(Response::from_data(bytes)
                            .with_header(
                                Header::from_bytes(
                                    &b"Content-Type"[..],
                                    &b"application/vnd.quantized-mesh"[..],
                                )
                                .unwrap(),
                            )
                            .with_header(
                                Header::from_bytes(&b"Cache-Control"[..], &b"max-age=3600"[..])
                                    .unwrap(),
                            ))
                    }
                    None => Err(format!("tile de terreno invalido: {caminho}")),
                }
            }

            // Imagem aerea do globo, dos mesmos tiles que o terreno 3D usa.
            (false, caminho) if caminho.starts_with("/imagery/") => {
                match tile_do_caminho(caminho, "/imagery/", ".jpg") {
                    Some((z, x, y)) => {
                        match servir_imagery(&estado.rt, estado.cfg.imagery, z, x, y) {
                            Some(r) => Ok(r),
                            None => Err(format!("tile de imagem indisponivel: {caminho}")),
                        }
                    }
                    None => Err(format!("tile de imagem invalido: {caminho}")),
                }
            }

            (false, caminho) if caminho.starts_with("/vendor/cesium/") => {
                match servir_vendor(caminho) {
                    Some(r) => Ok(r),
                    None => Err(format!("nao encontrado: {caminho}")),
                }
            }

            // ---- UI do ARCZ Earth Desktop, servida pelo proprio Rust ---------
            (false, "/earth" | "/earth/" | "/earth/index.html") => Ok(html(UI_INDEX)),
            (false, "/earth/app.js") => Ok(script(UI_APP)),
            // data.js = a referencia da UI + um bootstrap que troca o mock pelo
            // que o servidor sabe de verdade. A UI nao precisa saber a diferenca;
            // o que nao existe de verdade fica visivelmente vazio, nao inventado.
            (false, "/earth/data.js") => {
                Ok(script(&format!("{UI_DATA}
{}", BOOTSTRAP_VIVO)))
            }
            (false, "/earth/styles.css") => Ok(estilo(UI_CSS)),

            // ---- dispatcher do contrato de comandos --------------------------
            (true, r) if r.starts_with("/cmd/") => {
                let nome = &r["/cmd/".len()..];
                let params: serde_json::Value =
                    serde_json::from_str(&corpo).unwrap_or(serde_json::Value::Null);
                let ctx = crate::comandos::Contexto {
                    scene: &estado.scene,
                    editor: estado.editor.as_ref(),
                    biblioteca: &estado.cfg.biblioteca_raiz,
                };
                let r = crate::comandos::executar(nome, &params, &ctx);
                Ok(json(serde_json::to_string(&r).unwrap_or_else(|_| "{}".into())))
            }
            (false, "/cmd") => {
                let ctx = crate::comandos::Contexto {
                    scene: &estado.scene,
                    editor: estado.editor.as_ref(),
                    biblioteca: &estado.cfg.biblioteca_raiz,
                };
                let r = crate::comandos::executar("capability.list", &serde_json::Value::Null, &ctx);
                Ok(json(serde_json::to_string(&r).unwrap_or_else(|_| "{}".into())))
            }

            (false, "/earth/scene") => {
                let mut engine = arcz_earth::ArczEarthEngine::novo("cache/earth");
                let scene = engine.inicializar_globo_offline();
                Ok(json(serde_json::to_string(&scene).unwrap_or_else(|_| "{}".into())))
            }
            (false, "/earth/take") => {
                let engine = arcz_earth::ArczEarthEngine::novo("cache/earth");
                let c = estado.scene.bbox.center();
                let bands = engine.renderizar_take(&arcz_earth::CameraPosition {
                    longitude: c.lon_deg,
                    latitude: c.lat_deg,
                    height: 1500.0,
                });
                Ok(json(serde_json::to_string(&bands).unwrap_or_else(|_| "[]".into())))
            }
            _ => Err("rota desconhecida".to_string()),
        };

        let envio = match resultado {
            Ok(r) => requisicao.respond(r),
            Err(msg) => requisicao.respond(Response::from_string(msg).with_status_code(404)),
        };
        if let Err(e) = envio {
            log::warn!("cliente desconectou: {e}");
        }
    }

    Ok(())
}

struct QueryArea {
    lado: f64,
    zoom_img: Option<u8>,
}

impl QueryArea {
    fn de(query: &str) -> Self {
        let mut lado = 400.0;
        let mut zoom_img = None;
        for par in query.split('&') {
            if let Some((c, v)) = par.split_once('=') {
                match c {
                    "lado" => set(&mut lado, v),
                    "zoom_img" => zoom_img = v.parse().ok(),
                    _ => {}
                }
            }
        }
        Self { lado, zoom_img }
    }
}

/// Formato de saida do quadro.
#[derive(Debug, Clone, Copy)]
enum Formato {
    Png,
    /// Qualidade JPEG. Usado durante a interacao, onde velocidade vale mais.
    Jpeg(u8),
    /// So o projeto, sobre fundo transparente, para compor sobre uma foto do
    /// local. Sempre PNG: JPEG nao tem canal alpha.
    Composicao,
}

fn valor_de<'a>(query: &'a str, chave: &str) -> Option<&'a str> {
    query
        .split('&')
        .filter_map(|p| p.split_once('='))
        .find(|(c, _)| *c == chave)
        .map(|(_, v)| v)
}

fn plano_json(estado: &Estado, p: &Params, query: &str) -> String {
    let ndc_x = valor_de(query, "nx")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    let ndc_y = valor_de(query, "ny")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);

    // `camera_de` precisa de &mut so por causa do renderer; aqui refazemos a conta
    // sem tocar em GPU.
    let mut alvo = estado.scene.mesh.center();
    let mut extensao = estado.scene.mesh.horizontal_extent();
    let mut base_y = 0.0_f64;
    if let Some(f) = &estado.fonte {
        let t = arcz_model::transformar(
            f,
            &estado.scene.frame,
            &p.placement(&estado.scene.placement),
            estado.scene.solo_modelo_m,
        );
        alvo = [
            (t.min_enu[0] + t.max_enu[0]) * 0.5,
            (t.min_enu[1] + t.max_enu[1]) * 0.5,
            (t.min_enu[2] + t.max_enu[2]) * 0.5,
        ];
        extensao = (t.max_enu[0] - t.min_enu[0])
            .max(t.max_enu[1] - t.min_enu[1])
            .max(t.max_enu[2] - t.min_enu[2]);
        base_y = t.min_enu[1] as f64;
    }

    let mut camera = OrbitCamera::enquadrando(alvo, extensao);
    camera.alvo[0] += p.alvo_leste;
    camera.alvo[2] -= p.alvo_norte;
    camera.alvo[1] += p.alvo_vertical;
    camera.pitch = p.pitch.to_radians().clamp(-1.553, 1.553);
    camera.yaw = p.yaw.to_radians();
    if p.dist > 0.0 {
        camera.distancia = p.dist;
        // Em modo caminhar a camera chega a menos de um metro da parede. O
        // `near` calculado pela extensao da cena (0,5 m) recortaria tudo a
        // frente, e a tela ficaria vazia justamente ao entrar no ambiente.
        camera.near = (p.dist * 0.05).clamp(0.02, camera.near);
    }

    let aspecto = p.largura as f64 / p.altura.max(1) as f64;
    match Renderer::raio_no_plano(&camera, aspecto, ndc_x, ndc_y, base_y) {
        // x = leste, z = -norte, entao norte = -z.
        Some(q) => format!(
            "{{\"ok\":true,\"leste\":{:.4},\"norte\":{:.4}}}",
            q[0], -q[1]
        ),
        None => "{\"ok\":false}".to_string(),
    }
}

/// Qual objeto está sob o pixel (`nx`, `ny`) em NDC.
///
/// O raio é construído com a mesma câmera do quadro desenhado — não com uma
/// reconstruída por aproximação. Se as duas divergirem, o clique acerta um
/// objeto diferente do que está sob o cursor, e o erro é invisível: o usuário
/// só percebe que "às vezes seleciona errado".
///
/// Testa contra a caixa envolvente, não contra os triângulos. Para escolher
/// **qual** objeto foi clicado a caixa basta, e percorrer 936 mil triângulos
/// por clique custaria mais que desenhar o quadro.
fn picar_json(estado: &Estado, p: &Params, query: &str) -> String {
    let Some(editor) = &estado.editor else {
        return "{\"ok\":false,\"motivo\":\"sem editor nesta sessao\"}".to_string();
    };

    let ndc_x = valor_de(query, "nx")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    let ndc_y = valor_de(query, "ny")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);

    let camera = camera_do_quadro(estado, p);
    let aspecto = p.largura as f64 / p.altura.max(1) as f64;
    // Camera degenerada (matriz nao inversivel) nao pode virar panico no meio
    // de um clique; devolve "nada sob o cursor".
    let Some((origem, direcao)) = Renderer::raio_da_camera(&camera, aspecto, ndc_x, ndc_y) else {
        return "{\"ok\":false}".to_string();
    };

    match editor.picar(origem, direcao) {
        Some(id) => {
            let o = editor.get(id);
            let nome = o.map(|o| o.nome.clone()).unwrap_or_default();
            let (min, max) = o.map(|o| (o.min_enu, o.max_enu)).unwrap_or_default();
            // Origem do dado, para o Inspector mostrar a proveniencia certa: o
            // entorno vem de GIS, o resto e do usuario.
            let origem_dado = if crate::entorno::e_do_entorno(id) {
                "gis"
            } else {
                "usuario"
            };
            format!(
                concat!(
                    "{{\"ok\":true,\"id\":{},\"nome\":\"{}\",\"origem\":\"{}\",",
                    "\"leste\":{:.2},\"norte\":{:.2},\"altura\":{:.2},",
                    "\"largura\":{:.2},\"profundidade\":{:.2}}}"
                ),
                id,
                nome.replace('"', "'"),
                origem_dado,
                (min[0] + max[0]) * 0.5,
                -(min[2] + max[2]) * 0.5,
                max[1] - min[1],
                max[0] - min[0],
                max[2] - min[2],
            )
        }
        None => "{\"ok\":false}".to_string(),
    }
}

/// Câmera exatamente igual à usada para desenhar o quadro atual.
///
/// Extraída de `renderizar_com` e `plano_json`, que a montavam cada um por si.
/// Duas cópias da mesma conta divergem na primeira mudança, e o sintoma seria
/// clique desalinhado do desenho.
fn camera_do_quadro(estado: &Estado, p: &Params) -> OrbitCamera {
    let mut alvo = estado.scene.mesh.center();
    let mut extensao = estado.scene.mesh.horizontal_extent();

    if let Some((min, max)) = estado.caixa_modelo {
        // A EXTENSAO vem da caixa do modelo: e ela que dita a distancia de
        // enquadramento.
        extensao = (max[0] - min[0]).max(max[1] - min[1]).max(max[2] - min[2]);
    }
    // O ALVO vem do valor travado. Usar o centro da caixa aqui era o bug de "o
    // mapa se mexe junto": a caixa acompanha o placement, entao arrastar o
    // predio levava a camera junto — o predio ficava cravado no meio da tela e
    // o mapa e que deslizava por baixo.
    if let Some(t) = estado.alvo_travado {
        alvo = t;
    }

    let mut camera = OrbitCamera::enquadrando(alvo, extensao);
    camera.alvo[0] += p.alvo_leste;
    camera.alvo[2] -= p.alvo_norte;
    camera.alvo[1] += p.alvo_vertical;
    camera.pitch = p.pitch.to_radians().clamp(-1.553, 1.553);
    camera.yaw = p.yaw.to_radians();
    if p.dist > 0.0 {
        camera.distancia = p.dist;
        // Em modo caminhar a camera chega a menos de um metro da parede. O
        // `near` calculado pela extensao da cena (0,5 m) recortaria tudo a
        // frente, e a tela ficaria vazia justamente ao entrar no ambiente.
        camera.near = (p.dist * 0.05).clamp(0.02, camera.near);
    }
    camera
}

fn renderizar(estado: &mut Estado, p: &Params) -> anyhow::Result<Vec<u8>> {
    renderizar_com(estado, p, Formato::Png)
}

fn renderizar_com(estado: &mut Estado, p: &Params, formato: Formato) -> anyhow::Result<Vec<u8>> {
    // Corte da vista, por quadro. Sem ele o mobiliário fica invisível: os
    // móveis estão dentro de um prédio fechado de 936 mil triângulos, e a
    // cobertura tapa tudo. Esconder o modelo não resolve — o mesmo interruptor
    // apaga os móveis junto.
    //
    // O plano vem em metros acima da **base do modelo**, não em cota absoluta:
    // é assim que o usuário raciocina ("mostre até 3 m") e não muda de sentido
    // quando o prédio sobe ou desce no terreno.
    estado.renderer.vista = match p.corte_m {
        Some(altura) => {
            let base = estado
                .caixa_modelo
                .map(|(min, _)| min[1])
                .unwrap_or(estado.scene.solo_modelo_m as f32);
            [base + altura, 1.0, estado.cfg.estilo.codigo(), estado.cfg.corte_linha_m.max(0.001)]
        }
        None => estado.cfg.vista(),
    };

    let mut alvo = estado.scene.mesh.center();
    let mut extensao = estado.scene.mesh.horizontal_extent();

    if let Some(f) = &estado.fonte {
        let placement = p.placement(&estado.scene.placement);

        // Só refaz a geometria quando o modelo realmente se moveu. Girar a câmera
        // não muda vértice nenhum, e transformar 936 mil deles + subir 30 MB para a
        // GPU a cada quadro era o que segurava o preview em ~150 ms.
        if estado.ultimo_placement != Some(placement) {
            // 64 bytes de matriz. Antes isto retransformava 936 mil vertices e
            // subia 30 MB — era o que fazia mover parecer travado.
            let m = arcz_model::matriz_modelo(
                f.min,
                f.max,
                &estado.scene.frame,
                &placement,
                estado.scene.solo_modelo_m,
            );
            estado.renderer.atualizar_transform(0, m);
            let caixa = arcz_model::caixa_transformada(f.min, f.max, m);
            // Trava o alvo da camera na PRIMEIRA posicao vista.
            //
            // Antes o alvo era recalculado do centro da caixa a cada quadro, e
            // como a caixa acompanha o placement, arrastar o predio arrastava a
            // camera junto: o predio ficava cravado no meio da tela e o mapa e
            // que deslizava por baixo. Travando aqui, mover o modelo move o
            // modelo — que e o que o gizmo promete.
            if estado.alvo_travado.is_none() {
                estado.alvo_travado = Some([
                    (caixa.0[0] + caixa.1[0]) * 0.5,
                    (caixa.0[1] + caixa.1[1]) * 0.5,
                    (caixa.0[2] + caixa.1[2]) * 0.5,
                ]);
            }
            estado.caixa_modelo = Some(caixa);
            estado.ultimo_placement = Some(placement);
            autossalvar(estado, &placement);
        }

        if let Some((min, max)) = estado.caixa_modelo {
            // A extensao pode acompanhar a caixa — ela so dita a distancia.
            extensao = (max[0] - min[0]).max(max[1] - min[1]).max(max[2] - min[2]);
        }
        // O alvo, nao: fica onde travou, para o arrasto mover o modelo e nao a
        // camera.
        if let Some(t) = estado.alvo_travado {
            alvo = t;
        }
    }

    // Sol conforme a data e hora pedidas. So mexe em uniform, nao recria recurso.
    estado.renderer.momento = crate::iluminacao::Momento {
        mes: p.mes.clamp(1, 12),
        dia: p.dia.clamp(1, 31),
        hora_local: p.hora.clamp(0.0, 23.99),
        ..crate::iluminacao::Momento::default()
    };

    let mut camera = OrbitCamera::enquadrando(alvo, extensao);
    // Pan: em coordenadas de render o norte e -Z.
    camera.alvo[0] += p.alvo_leste;
    camera.alvo[2] -= p.alvo_norte;
    camera.alvo[1] += p.alvo_vertical;
    camera.pitch = p.pitch.to_radians().clamp(-1.553, 1.553);
    camera.yaw = p.yaw.to_radians();
    if p.dist > 0.0 {
        camera.distancia = p.dist;
        // Em modo caminhar a camera chega a menos de um metro da parede. O
        // `near` calculado pela extensao da cena (0,5 m) recortaria tudo a
        // frente, e a tela ficaria vazia justamente ao entrar no ambiente.
        camera.near = (p.dist * 0.05).clamp(0.02, camera.near);
    }
    // O far plane vem do enquadramento inicial; com pan e zoom manual ele pode ficar
    // curto e cortar o terreno. Recalcula pela distancia efetiva.
    camera.far = (camera.distancia * 20.0).max(extensao as f64 * 20.0);

    // Gizmo do objeto selecionado. Em modo camera nao ha o que manipular.
    let linhas = match (p.gizmo, estado.caixa_modelo) {
        (Some(modo), Some((min, max))) if p.mostrar_modelo => {
            let centro = [
                (min[0] + max[0]) * 0.5,
                (min[1] + max[1]) * 0.5,
                (min[2] + max[2]) * 0.5,
            ];
            crate::gizmo::construir(centro, min, max, modo, camera.distancia)
        }
        _ => Vec::new(),
    };
    estado.renderer.atualizar_gizmo(&linhas);

    match formato {
        Formato::Png => estado
            .renderer
            .render_png(&camera, p.largura, p.altura, p.mostrar_modelo),
        Formato::Jpeg(q) => {
            estado
                .renderer
                .render_jpeg(&camera, p.largura, p.altura, p.mostrar_modelo, q)
        }
        // Só o projeto, sobre fundo transparente, para compor sobre uma foto do
        // local. JPEG não serve aqui: o formato não tem canal alpha, e o fundo
        // viraria preto sólido em volta do prédio.
        Formato::Composicao => {
            let rgba = estado.renderer.render_rgba_camadas(
                &camera,
                p.largura,
                p.altura,
                p.mostrar_modelo,
                crate::gpu::Camadas::SO_PROJETO,
            )?;
            let mut png = Vec::new();
            let img = image::RgbaImage::from_raw(p.largura, p.altura, rgba)
                .ok_or_else(|| anyhow::anyhow!("buffer RGBA nao bate com {}x{}", p.largura, p.altura))?;
            image::DynamicImage::ImageRgba8(img)
                .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)?;
            Ok(png)
        }
    }
}

fn estado_json(estado: &Estado) -> String {
    let c = estado.scene.bbox.center();
    let r = &estado.scene.relatorio;
    let m = r.modelo.as_ref();
    format!(
        concat!(
            "{{\"lat\":{:.7},\"lon\":{:.7},\"lado\":{:.0},\"zoom_img\":{},\"zoom_dem\":{},",
            "\"extensao\":{:.0},\"triangulos_terreno\":{},\"tem_modelo\":{},",
            "\"modelo_lat\":{:.7},\"modelo_lon\":{:.7},\"modelo_heading\":{:.3},",
            "\"modelo_triangulos\":{},\"modelo_largura\":{:.2},\"modelo_altura\":{:.2},",
            "\"modelo_profundidade\":{:.2},\"escala\":{:.4},\"aviso\":{}}}"
        ),
        c.lat_deg,
        c.lon_deg,
        estado.cfg.lado_m,
        r.zoom_imagery,
        r.zoom_dem,
        r.extensao_horizontal_m,
        r.triangulos,
        estado.scene.modelo.is_some(),
        // Posicao **efetiva**, nao a do relatorio de carga. O relatorio descreve
        // o que veio do arquivo; depois de retomar do `project.sqlite` os dois
        // divergem, e a UI abriria mostrando a posicao antiga.
        estado.scene.placement.lat_deg,
        estado.scene.placement.lon_deg,
        estado.scene.placement.heading_deg,
        m.map_or(0, |m| m.triangulos),
        m.map_or(0.0, |m| m.tamanho_real_m[0]),
        m.map_or(0.0, |m| m.tamanho_real_m[1]),
        m.map_or(0.0, |m| m.tamanho_real_m[2]),
        estado.scene.placement.escala,
        match m.and_then(|m| m.aviso_kmz.as_deref()) {
            Some(a) => format!("\"{}\"", a.replace('"', "'")),
            None => "null".to_string(),
        }
    )
}

/// Abre o `.glb` do usuário como corpo de resposta, sem lê-lo para a memória.
///
/// O tiny_http aceita qualquer `Read` como corpo e copia em blocos para o
/// socket. Com um modelo de 124 MB pedido várias vezes, a diferença entre isto
/// e `fs::read` é o processo continuar de pé.
fn modelo_em_streaming(estado: &Estado) -> Result<Response<std::fs::File>, String> {
    use std::io::{Read, Seek};

    let original = estado
        .cfg
        .modelo
        .as_ref()
        .ok_or_else(|| "nenhum modelo carregado".to_string())?;

    // Serve a versao LEVE, gerada uma vez e guardada em cache.
    //
    // O arquivo do Zenite tem 130 MB porque o export do SketchUp repete cada
    // vertice por face e guarda textura de 2048 px. Nenhuma das duas coisas
    // aparece na tela do globo, e as duas aparecem no relogio.
    let leve = crate::otimizar::caminho_no_cache(original);
    let caminho = if leve.exists() {
        leve
    } else {
        match arcz_model::Model::load(original)
            .map_err(|e| format!("{e}"))
            .and_then(|m| crate::otimizar::gerar(&m, &leve).map_err(|e| format!("{e}")))
        {
            Ok(g) => {
                log::info!(
                    "modelo leve: {} -> {} vertices ({} texturas reduzidas), {:.1} MB -> {:.1} MB",
                    g.vertices_antes,
                    g.vertices_depois,
                    g.texturas_reduzidas,
                    std::fs::metadata(original).map(|m| m.len()).unwrap_or(0) as f64 / 1e6,
                    std::fs::metadata(&leve).map(|m| m.len()).unwrap_or(0) as f64 / 1e6,
                );
                leve
            }
            Err(e) => {
                // Falhou a otimizacao: serve o original. Um globo lento e melhor
                // que um globo vazio.
                log::warn!("nao otimizei o modelo ({e}); servindo o original");
                original.clone()
            }
        }
    };

    let mut f = std::fs::File::open(&caminho).map_err(|e| format!("nao abri o modelo: {e}"))?;

    // `.gltf` separado não serve: o Cesium buscaria o `.bin` e as texturas em
    // caminhos que esta rota não expõe. Confere só os quatro bytes do magic.
    let mut magic = [0u8; 4];
    f.read_exact(&mut magic)
        .map_err(|e| format!("modelo ilegivel: {e}"))?;
    if &magic != b"glTF" {
        return Err("o modelo precisa ser .glb para ir ao globo".to_string());
    }
    f.rewind().map_err(|e| format!("nao reposicionei: {e}"))?;

    let tamanho = f.metadata().ok().map(|m| m.len() as usize);
    Ok(Response::new(
        tiny_http::StatusCode(200),
        vec![
            Header::from_bytes(&b"Content-Type"[..], &b"model/gltf-binary"[..]).unwrap(),
            // Deixa o navegador reusar entre trocas de motor, em vez de rebaixar
            // 124 MB toda vez.
            Header::from_bytes(&b"Cache-Control"[..], &b"max-age=3600"[..]).unwrap(),
        ],
        f,
        tamanho,
        None,
    ))
}

/// Analisa `/terreno/{z}/{x}/{y}.terrain` e `/imagery/{z}/{x}/{y}.jpg`.
fn tile_do_caminho(caminho: &str, prefixo: &str, ext: &str) -> Option<(u8, u32, u32)> {
    let rel = caminho.strip_prefix(prefixo)?.strip_suffix(ext)?;
    let mut p = rel.split('/');
    let (z, x, y) = (p.next()?, p.next()?, p.next()?);
    if p.next().is_some() {
        return None;
    }
    // Zoom acima de 22 estouraria o `2^(z+1)` do esquema geografico.
    let z: u8 = z.parse().ok().filter(|&z| z <= 22)?;
    Some((z, x.parse().ok()?, y.parse().ok()?))
}

/// Baixa (ou lê do cache) um tile de imagem aérea e devolve os bytes crus.
///
/// O globo do Cesium sem imagem é uma esfera azul lisa — foi isso que o preview
/// mostrava. Em vez de ligar o serviço pago do Cesium ion, o ARCZ serve pela
/// própria porta os mesmos tiles que já usa no terreno 3D. Depois do primeiro
/// acesso tudo vem do cache em disco, então o globo funciona offline.
fn servir_imagery(
    rt: &tokio::runtime::Runtime,
    fonte: arcz_terrain::ImagerySource,
    z: u8,
    x: u32,
    y: u32,
) -> Option<Response<Cursor<Vec<u8>>>> {
    let cache = arcz_terrain::TileCache::new(arcz_terrain::TileCache::default_root()).ok()?;
    let url = fonte.url(arcz_geo::TileId::new(z, x, y));
    let bytes = rt.block_on(cache.get(&url)).ok()?;
    // O tipo real varia por fonte (JPEG no Esri, PNG no GIBS). O navegador
    // decide pelo conteúdo, mas errar o cabeçalho quebra o cache do Cesium.
    let tipo: &[u8] = if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        b"image/png"
    } else {
        b"image/jpeg"
    };
    Some(
        Response::from_data(bytes)
            .with_header(Header::from_bytes(&b"Content-Type"[..], tipo).unwrap())
            .with_header(
                Header::from_bytes(&b"Cache-Control"[..], &b"max-age=86400"[..]).unwrap(),
            ),
    )
}

/// Serve um arquivo do CesiumJS vendorizado em `vendor/cesium`.
///
/// Recusa qualquer caminho com `..`: sem essa guarda, um pedido como
/// `/vendor/cesium/../../../../etc/passwd` sairia do diretório e serviria
/// arquivo arbitrário do disco.
fn servir_vendor(caminho: &str) -> Option<Response<std::io::Cursor<Vec<u8>>>> {
    let rel = caminho.strip_prefix("/vendor/cesium/")?;
    if rel.contains("..") || rel.starts_with('/') || rel.contains('\\') {
        log::warn!("caminho recusado: {caminho}");
        return None;
    }
    // O ZIP oficial traz `Build/Cesium/...`, e a cópia manteve o nível
    // `Cesium/`. A URL fica curta (`/vendor/cesium/Cesium.js`) e o disco resolve
    // o nível extra aqui, para não vazar a forma do pacote para o HTML.
    let disco = std::path::Path::new("vendor/cesium/Cesium").join(rel);
    let bytes = std::fs::read(&disco).ok()?;

    // O navegador recusa executar JS servido como texto simples, e a folha de
    // estilo é ignorada em silêncio se o tipo estiver errado.
    let tipo: &[u8] = match disco.extension().and_then(|e| e.to_str()) {
        Some("js") | Some("mjs") => b"application/javascript; charset=utf-8",
        Some("css") => b"text/css; charset=utf-8",
        Some("json") => b"application/json; charset=utf-8",
        Some("wasm") => b"application/wasm",
        Some("png") => b"image/png",
        Some("jpg") | Some("jpeg") => b"image/jpeg",
        Some("svg") => b"image/svg+xml",
        Some("gif") => b"image/gif",
        Some("ktx2") => b"image/ktx2",
        Some("glb") => b"model/gltf-binary",
        Some("gltf") => b"model/gltf+json",
        Some("xml") => b"text/xml; charset=utf-8",
        Some("html") => b"text/html; charset=utf-8",
        _ => b"application/octet-stream",
    };
    Some(
        Response::from_data(bytes)
            .with_header(Header::from_bytes(&b"Content-Type"[..], tipo).unwrap()),
    )
}

/// Agrupa a peça por tipo, a partir da chave.
///
/// A biblioteca não guarda categoria — o nome da pasta é o que há. Uma lista
/// plana de 115 itens é inutilizável para arrastar, então o agrupamento é
/// deduzido aqui em vez de exigir recatalogar tudo à mão.
fn categoria_do_item(chave: &str) -> &'static str {
    let k = chave.to_ascii_lowercase();
    // A ordem importa: "mesa-cadeiras" é mesa, não cadeira.
    const REGRAS: &[(&str, &str)] = &[
        ("carro", "Veículos"),
        ("cama", "Dormitório"),
        ("criado", "Dormitório"),
        ("guarda-roupa", "Dormitório"),
        ("comoda", "Dormitório"),
        ("sofa", "Estar"),
        ("poltrona", "Estar"),
        ("puff", "Estar"),
        ("rack", "Estar"),
        ("almofada", "Estar"),
        ("tapete", "Estar"),
        ("mesa", "Mesas"),
        ("cadeira", "Assentos"),
        ("banqueta", "Assentos"),
        ("banco", "Assentos"),
        ("cozinha", "Cozinha"),
        ("cooktop", "Cozinha"),
        ("geladeira", "Cozinha"),
        ("micro-ondas", "Cozinha"),
        ("bancada", "Cozinha"),
        ("vaso-sanitario", "Banheiro"),
        ("cuba", "Banheiro"),
        ("box-chuveiro", "Banheiro"),
        ("espelho", "Banheiro"),
        ("planta", "Vegetação"),
        ("vaso-ceramica", "Vegetação"),
        ("floreira", "Vegetação"),
        ("coqueiro", "Vegetação"),
        ("samambaia", "Vegetação"),
        ("suculenta", "Vegetação"),
        ("fern", "Vegetação"),
        ("potted", "Vegetação"),
        ("planter", "Vegetação"),
        ("calathea", "Vegetação"),
        ("pachira", "Vegetação"),
        ("arvore", "Vegetação"),
        ("tree", "Vegetação"),
        ("espreguicadeira", "Externo"),
        ("guarda-sol", "Externo"),
        ("ombrelone", "Externo"),
        ("churrasqueira", "Externo"),
        ("outdoor", "Externo"),
        ("paineis-solares", "Externo"),
        ("gondola", "Comércio"),
        ("prateleira", "Comércio"),
        ("balcao", "Comércio"),
        ("caixa-registradora", "Comércio"),
        ("manequim", "Comércio"),
        ("arara", "Comércio"),
        ("luminaria", "Iluminação"),
        ("pendente", "Iluminação"),
        ("lareira", "Iluminação"),
        ("quadro", "Decoração"),
        ("relogio", "Decoração"),
        ("livros", "Decoração"),
        ("jogo-cha", "Decoração"),
        ("estante", "Armários"),
        ("armario", "Armários"),
        ("gaveteiro", "Armários"),
    ];
    for (termo, cat) in REGRAS {
        if k.contains(termo) {
            return cat;
        }
    }
    "Outros"
}

fn ler_cameras() -> String {
    std::fs::read_to_string(ARQUIVO_CAMERAS).unwrap_or_else(|_| "[]".to_string())
}

fn gravar_cameras(corpo: &str) -> std::io::Result<()> {
    // Validacao minima: tem que ser um array JSON. Sem isso um POST torto
    // corromperia a lista que o render em lote vai consumir.
    let t = corpo.trim();
    if !t.starts_with('[') || !t.ends_with(']') {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "a lista de cameras precisa ser um array JSON",
        ));
    }
    if let Some(pai) = std::path::Path::new(ARQUIVO_CAMERAS).parent() {
        std::fs::create_dir_all(pai)?;
    }
    std::fs::write(ARQUIVO_CAMERAS, t)
}

fn json(corpo: String) -> Response<Cursor<Vec<u8>>> {
    Response::from_data(corpo.into_bytes()).with_header(
        Header::from_bytes(
            &b"Content-Type"[..],
            &b"application/json; charset=utf-8"[..],
        )
        .unwrap(),
    )
}

fn pagina() -> Response<Cursor<Vec<u8>>> {
    Response::from_data(PAGINA.as_bytes().to_vec()).with_header(
        Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap(),
    )
}

const PAGINA: &str = include_str!("preview.html");

// UI do ARCZ Earth Desktop. Embutida no binario: o app roda offline, entao nao
// pode depender de arquivo solto ao lado do executavel.
const UI_INDEX: &str = include_str!("ui/index.html");
const UI_APP: &str = include_str!("ui/app.js");
const UI_DATA: &str = include_str!("ui/data.js");
const UI_CSS: &str = include_str!("ui/styles.css");

/// Troca os dados de exemplo da UI pelos reais, antes de `app.js` rodar.
///
/// XHR sincrono de proposito: `data.js` roda antes de `app.js`, que le
/// `ARCZ_DATA` na primeira linha. Sem bloquear aqui, a UI montaria com o mock e
/// so depois receberia o dado real — e o usuario veria numero falso piscar.
const BOOTSTRAP_VIVO: &str = r#"
(function () {
  function cmd(nome, params) {
    try {
      var x = new XMLHttpRequest();
      x.open("POST", "/cmd/" + nome, false);
      x.send(JSON.stringify(params || {}));
      var r = JSON.parse(x.responseText);
      return r && r.ok ? r.dado : null;
    } catch (e) {
      console.warn("cmd " + nome + " falhou:", e);
      return null;
    }
  }

  var D = window.ARCZ_DATA;
  if (!D) return;

  var st = cmd("workspace.status");
  var proj = cmd("project.list");
  var pac = cmd("package.list");
  var cap = cmd("capability.list");
  var cena = cmd("scene.list");

  D.vivo = { status: st, capacidades: cap, cena: cena };

  // Projetos: o catalogo real do workspace. Vazio e vazio — nao se inventa.
  if (proj && Array.isArray(proj.projetos)) {
    D.projects = proj.projetos.map(function (p) {
      return {
        name: p.nome,
        local: st ? st.lat.toFixed(4) + " · " + st.lon.toFixed(4) : "—",
        type: "ARCZ",
        status: p.recuperavel ? "Recuperavel" : "Salvo",
        size: (p.bytes / 1e6).toFixed(1) + " MB",
        updated: "—",
        thumb: p.tem_miniatura ? "sun" : ""
      };
    });
  }

  if (pac && Array.isArray(pac.pacotes)) {
    D.packages = pac.pacotes.map(function (p) {
      return {
        name: p.nome,
        version: "cache",
        size: p.tiles_em_cache + " tiles",
        status: p.status === "instalado" ? "Instalado" : "Vazio",
        progress: p.tiles_em_cache > 0 ? 100 : 0
      };
    });
  }

  // Rodape: o que esta na cena agora, medido, nao escrito a mao.
  if (st) {
    D.rodape = st.objetos + " objetos · " + (st.modelo_carregado ? "modelo carregado" : "sem modelo") +
      " · solo " + st.solo_m.toFixed(1) + " m · lado " + st.lado_m + " m";
  }
  if (cap) {
    D.contrato = cap.implementados.length + "/" + cap.total_contrato + " comandos ligados";
  }
  console.info("ARCZ vivo:", D.rodape, "|", D.contrato);
})();
"#;

fn html(corpo: &str) -> Response<Cursor<Vec<u8>>> {
    Response::from_string(corpo).with_header(
        Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap(),
    )
}

fn script(corpo: &str) -> Response<Cursor<Vec<u8>>> {
    Response::from_string(corpo).with_header(
        Header::from_bytes(
            &b"Content-Type"[..],
            &b"application/javascript; charset=utf-8"[..],
        )
        .unwrap(),
    )
}

fn estilo(corpo: &str) -> Response<Cursor<Vec<u8>>> {
    Response::from_string(corpo).with_header(
        Header::from_bytes(&b"Content-Type"[..], &b"text/css; charset=utf-8"[..]).unwrap(),
    )
}

fn pagina_cesium() -> String {
    r#"<!doctype html>
<html lang="pt-BR">
<head>
<meta charset="utf-8">
<title>ARCZ — Visualizador CesiumJS Sandcastle 3D</title>
<link rel="stylesheet" href="https://cesium.com/downloads/cesiumjs/releases/1.115/Build/Cesium/Widgets/widgets.css">
<script src="https://cesium.com/downloads/cesiumjs/releases/1.115/Build/Cesium/Cesium.js"></script>
<style>
  html, body, #cesiumContainer { width: 100%; height: 100%; margin: 0; padding: 0; overflow: hidden; background: #000; }
  #toolbar { position: absolute; top: 10px; left: 10px; z-index: 99; background: rgba(13, 16, 22, 0.85); padding: 10px; border-radius: 6px; border: 1px solid #303a4b; color: #fff; font-family: monospace; }
  select, button { background: #1f2733; color: #fff; border: 1px solid #4d8dfd; padding: 6px 10px; border-radius: 4px; cursor: pointer; margin-bottom: 6px; width: 100%; }
</style>
</head>
<body>
<div id="toolbar">
  <b style="color:#4d8dfd;">ARCZ CesiumJS 3D Viewer</b>
  <div style="margin-top:8px;">Provedor de Terreno:</div>
  <select id="terrainSelect" onchange="trocarTerreno(this.value)">
    <option value="ellipsoid">EllipsoidTerrainProvider</option>
    <option value="custom">CustomHeightmapTerrainProvider (Onda Senoidal)</option>
    <option value="world">Cesium World Terrain</option>
  </select>
  <button onclick="carregarCZML()">🔄 Recarregar Stream CZML</button>
  <button onclick="lookAtEverest()">⛰️ Ir para Monte Everest</button>
</div>
<div id="cesiumContainer"></div>

<script>
let viewer;
let customHeightmapProvider;

window.onload = function() {
  viewer = new Cesium.Viewer("cesiumContainer", {
    baseLayerPicker: true,
    geocoder: false,
    timeline: true,
    animation: true
  });

  viewer.scene.globe.enableLighting = true;
  viewer.clock.currentTime = Cesium.JulianDate.fromIso8601("2023-01-01T00:00:00");

  const customHeightmapWidth = 32;
  const customHeightmapHeight = 32;
  customHeightmapProvider = new Cesium.CustomHeightmapTerrainProvider({
    width: customHeightmapWidth,
    height: customHeightmapHeight,
    callback: function (x, y, level) {
      const width = customHeightmapWidth;
      const height = customHeightmapHeight;
      const buffer = new Float32Array(width * height);
      for (let yy = 0; yy < height; yy++) {
        for (let xx = 0; xx < width; xx++) {
          const v = (y + yy / (height - 1)) / Math.pow(2, level);
          buffer[yy * width + xx] = 4000 * (Math.sin(8000 * v) * 0.5 + 0.5);
        }
      }
      return buffer;
    }
  });

  carregarCZML();
};

function trocarTerreno(tipo) {
  if (tipo === "ellipsoid") {
    viewer.terrainProvider = new Cesium.EllipsoidTerrainProvider();
  } else if (tipo === "custom") {
    viewer.terrainProvider = customHeightmapProvider;
  } else if (tipo === "world") {
    Cesium.Terrain.fromWorldTerrain().then(tp => viewer.scene.setTerrain(tp));
  }
}

function carregarCZML() {
  viewer.dataSources.add(Cesium.CzmlDataSource.load("/cesium/czml"));
}

function lookAtEverest() {
  const target = new Cesium.Cartesian3(300770.508, 5634912.131, 2978152.286);
  const offset = new Cesium.Cartesian3(6344.974, -793.341, 2499.950);
  viewer.camera.lookAt(target, offset);
  viewer.camera.lookAtTransform(Cesium.Matrix4.IDENTITY);
}
</script>
</body>
</html>"#.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Params {
        Params {
            lat: -27.15,
            lon: -48.5,
            heading: 0.0,
            leste: 0.0,
            norte: 0.0,
            vertical: 0.0,
            escala: 1.0,
            pitch: 35.0,
            yaw: -30.0,
            dist: 200.0,
            alvo_leste: 0.0,
            alvo_norte: 0.0,
            alvo_vertical: 0.0,
            largura: 1280,
            altura: 800,
            mostrar_modelo: true,
            mes: 3,
            dia: 21,
            hora: 15.0,
            gizmo: None,
            snap: None,
            corte_m: None,
        }
    }

    #[test]
    fn query_sobrescreve_apenas_o_que_veio() {
        let p = base().aplicar_query("heading=60&leste=-18");
        assert!((p.heading - 60.0).abs() < 1e-9);
        assert!((p.leste + 18.0).abs() < 1e-6);
        assert!((p.lat + 27.15).abs() < 1e-9);
        assert_eq!(p.largura, 1280);
    }

    #[test]
    fn valor_invalido_e_ignorado_em_vez_de_quebrar() {
        let p = base().aplicar_query("heading=abc&leste=&norte=7");
        assert_eq!(p.heading, 0.0);
        assert_eq!(p.leste, 0.0);
        assert!((p.norte - 7.0).abs() < 1e-6);
    }

    #[test]
    fn resolucao_e_limitada_para_nao_travar_a_gpu() {
        let p = base().aplicar_query("w=99999&h=1");
        assert_eq!(p.largura, 3840);
        assert_eq!(p.altura, 64);
    }

    #[test]
    fn query_vazia_ou_lixo_nao_altera_nada() {
        for q in ["", "&&", "semigual", "=semchave"] {
            let p = base().aplicar_query(q);
            assert_eq!(p.largura, 1280, "query {q:?}");
            assert!((p.dist - 200.0).abs() < 1e-9, "query {q:?}");
        }
    }

    #[test]
    fn pan_do_alvo_e_lido() {
        let p = base().aplicar_query("alvo_leste=25.5&alvo_norte=-40");
        assert!((p.alvo_leste - 25.5).abs() < 1e-9);
        assert!((p.alvo_norte + 40.0).abs() < 1e-9);
    }

    #[test]
    fn modelo_pode_ser_escondido_para_conferir_alinhamento() {
        assert!(base().aplicar_query("").mostrar_modelo);
        assert!(!base().aplicar_query("modelo=0").mostrar_modelo);
        assert!(base().aplicar_query("modelo=1").mostrar_modelo);
    }

    #[test]
    fn placement_reflete_os_params() {
        let p = base().aplicar_query("heading=45&leste=3&norte=-4&vertical=2&escala=0.01");
        let pl = p.placement(&Placement::default());
        assert!((pl.heading_deg - 45.0).abs() < 1e-9);
        assert!((pl.offset_leste_m - 3.0).abs() < 1e-6);
        assert!((pl.offset_norte_m + 4.0).abs() < 1e-6);
        assert!((pl.offset_vertical_m - 2.0).abs() < 1e-6);
        assert!((pl.escala - 0.01).abs() < 1e-6);
    }

    #[test]
    fn sem_snap_o_placement_passa_intacto() {
        // Arrasto livre e o padrao: nenhum arredondamento sem pedido explicito.
        let pl = base()
            .aplicar_query("leste=3.37&norte=-4.82&heading=47.3")
            .placement(&Placement::default());
        assert!((pl.offset_leste_m - 3.37).abs() < 1e-6);
        assert!((pl.offset_norte_m + 4.82).abs() < 1e-6);
        assert!((pl.heading_deg - 47.3).abs() < 1e-9);
    }

    #[test]
    fn o_snap_alinha_no_servidor_e_nao_no_navegador() {
        // O passo vem do cliente, mas quem arredonda e o Rust. Antes disto a UI
        // era a dona da regra, e um pedido pela CLI ou pelo render em lote
        // produzia posicao desalinhada.
        let pl = base()
            .aplicar_query("snap=0.5&leste=3.37&norte=-4.82&vertical=1.11")
            .placement(&Placement::default());
        assert_eq!(pl.offset_leste_m, 3.5);
        assert_eq!(pl.offset_norte_m, -5.0);
        assert_eq!(pl.offset_vertical_m, 1.0);
    }

    #[test]
    fn o_snap_de_grade_traz_junto_o_snap_de_rumo() {
        // Quem alinha a posicao quase sempre quer o rumo alinhado tambem;
        // deixar o rumo solto produz fileiras de casas tortas.
        let pl = base()
            .aplicar_query("snap=1&heading=47.3")
            .placement(&Placement::default());
        assert_eq!(pl.heading_deg, 45.0);
    }

    #[test]
    fn snap_zero_desliga_o_alinhamento() {
        let pl = base()
            .aplicar_query("snap=0&leste=3.37")
            .placement(&Placement::default());
        assert!((pl.offset_leste_m - 3.37).abs() < 1e-6);
    }

    #[test]
    fn snap_invalido_nao_derruba_a_requisicao() {
        // Query malformada nao pode virar erro 500 no meio de um arrasto.
        for q in ["snap=abc", "snap=", "snap=-2"] {
            let pl = base().aplicar_query(q).placement(&Placement::default());
            assert!(pl.escala > 0.0, "query {q}");
        }
    }

    #[test]
    fn escala_zero_ou_negativa_cai_para_um() {
        // Escala 0 colapsaria a geometria num ponto.
        for q in ["escala=0", "escala=-3"] {
            let pl = base().aplicar_query(q).placement(&Placement::default());
            assert_eq!(pl.escala, 1.0, "query {q}");
        }
    }

    #[test]
    fn area_e_limitada_a_faixa_util() {
        let q = QueryArea::de("lado=999999&zoom_img=99");
        assert!(
            (q.lado - 999999.0).abs() < 1.0,
            "o clamp e aplicado no recarregar"
        );
        assert_eq!(q.zoom_img, Some(99), "o clamp de zoom tambem");

        // Sem query, cai no padrao.
        let d = QueryArea::de("");
        assert!((d.lado - 400.0).abs() < 1e-9);
        assert!(d.zoom_img.is_none());
    }

    #[test]
    fn cameras_invalidas_sao_recusadas() {
        assert!(gravar_cameras("nao sou json").is_err());
        assert!(gravar_cameras("{\"a\":1}").is_err());
    }

    #[test]
    fn a_pagina_declara_todos_os_controles_que_o_servidor_le() {
        // Guarda contra adicionar um parametro no servidor e esquecer da pagina.
        for campo in [
            "lat", "lon", "heading", "leste", "norte", "vertical", "escala", "pitch", "yaw", "dist",
        ] {
            assert!(
                PAGINA.contains(&format!("id=\"{campo}\"")),
                "a pagina nao tem controle para {campo}"
            );
        }
        for rota in ["/render.png", "/estado.json", "/cameras", "/area"] {
            assert!(PAGINA.contains(rota), "a pagina nao usa a rota {rota}");
        }
    }
}

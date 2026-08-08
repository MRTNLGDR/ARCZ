//! ARCZ — Nucleo do Editor.
//!
//! Carrega uma regiao real (DEM + imagery de satelite) + opcionalmente objetos
//! de uma biblioteca ou projeto salvo, monta a cena editavel e exibe no viewport.
//! Suporta salvar/carregar projetos `.arcz` (formato versionado JSON).

mod archviz_worker;
mod cad_worker;
mod agua;
mod camera;
mod cena;
mod comandos;
mod cesium_worker;
mod config;
mod db;
mod entorno;
mod gis_worker;
mod gizmo;
mod gpu;
mod iluminacao;
mod offline_sync_worker;
mod otimizar;
mod planta;
mod projeto;
mod raw_gis_worker;
mod reconstruct_worker;
mod renderer;
mod scene;
mod server;
mod streetview_worker;
mod viewport;
mod workspace;

use config::Config;

/// Instancia as plantas mobiliadas do empreendimento no editor.
///
/// Cada arquivo e aberto **uma vez** e reaproveitado nas 40 instancias — sem isso,
/// mobiliar um pavimento reabriria o mesmo `.gltf` oito vezes. A copia por objeto
/// continua existindo (`Objeto` guarda a propria geometria); o ganho aqui e de
/// tempo de import, nao de memoria.
fn mobiliar(cfg: &Config, caminho: &std::path::Path, ed: &mut cena::Editor) -> anyhow::Result<()> {
    let emp = planta::Empreendimento::abrir(caminho)
        .map_err(|e| anyhow::anyhow!("falha ao ler {}: {e}", caminho.display()))?;

    let emp = match &cfg.mobiliar_pavimento {
        Some(filtro) => {
            let f = planta::filtrar_pavimento(&emp, filtro);
            if f.pavimentos.is_empty() {
                anyhow::bail!(
                    "nenhum pavimento com '{filtro}'. Ha: {}",
                    emp.pavimentos
                        .iter()
                        .map(|p| p.nome.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            f
        }
        None => emp,
    };

    let problemas = planta::verificar(&emp);
    for p in problemas.iter().take(20) {
        eprintln!("PLANTA: {p}");
    }
    if problemas.len() > 20 {
        eprintln!("PLANTA: ... e mais {}", problemas.len() - 20);
    }

    let (mut colocacoes, avisos) = planta::compor(&emp, &cfg.biblioteca_raiz);
    for a in &avisos {
        eprintln!("AVISO: {a}");
    }
    if cfg.mobiliar_limite > 0 && colocacoes.len() > cfg.mobiliar_limite {
        eprintln!(
            "Mobiliario: {} itens compostos, cortando em {} por --mobiliar-limite",
            colocacoes.len(),
            cfg.mobiliar_limite
        );
        colocacoes.truncate(cfg.mobiliar_limite);
    }

    let mut cache: std::collections::HashMap<std::path::PathBuf, arcz_model::Model> =
        std::collections::HashMap::new();
    let mut postos = 0usize;
    let mut falhas = 0usize;

    for c in &colocacoes {
        let modelo = match cache.entry(c.arquivo.clone()) {
            std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
            std::collections::hash_map::Entry::Vacant(e) => {
                match arcz_model::Model::load(e.key()) {
                    Ok(m) => e.insert(m),
                    Err(err) => {
                        eprintln!("AVISO: {} nao abriu: {err}", c.arquivo.display());
                        falhas += 1;
                        continue;
                    }
                }
            }
        };
        if ed
            .adicionar_com_arquivo(
                c.nome.clone(),
                modelo.clone(),
                c.placement,
                None,
                c.arquivo.clone(),
            )
            .is_some()
        {
            postos += 1;
        } else {
            falhas += 1;
        }
    }

    if std::env::var("ARCZ_DEBUG_PLANTA").is_ok() {
        for o in ed.objetos.iter().take(3) {
            eprintln!(
                "DEBUG {}: enu x {:.2}..{:.2} y {:.2}..{:.2} z {:.2}..{:.2} | leste {:.2} norte {:.2} vert {:.2}",
                o.nome,
                o.min_enu[0],
                o.max_enu[0],
                o.min_enu[1],
                o.max_enu[1],
                o.min_enu[2],
                o.max_enu[2],
                o.placement.offset_leste_m,
                o.placement.offset_norte_m,
                o.placement.offset_vertical_m
            );
        }
    }
    eprintln!(
        "Mobiliario: {} pavimento(s), {} unidade(s), {postos} movel(is) instanciado(s) \
         de {} arquivo(s) distinto(s){}",
        emp.pavimentos.len(),
        emp.pavimentos
            .iter()
            .map(|p| p.unidades.len())
            .sum::<usize>(),
        cache.len(),
        if falhas > 0 {
            format!(", {falhas} falha(s)")
        } else {
            String::new()
        }
    );
    Ok(())
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let mut cfg = match Config::from_args(std::env::args()) {
        Ok(c) => c,
        Err(msg) => {
            let pediu_ajuda = msg.starts_with("arcz \u{2014}");
            if pediu_ajuda {
                println!("{msg}");
                return Ok(());
            }
            eprintln!("erro: {msg}");
            std::process::exit(2);
        }
    };

    if !cfg.imagery.license().comercialmente_segura() {
        eprintln!(
            "AVISO: '{}' e {:?}. {}",
            cfg.imagery.nome(),
            cfg.imagery.license(),
            cfg.imagery.atribuicao()
        );
        eprintln!("       Nao use as imagens resultantes em produto comercial.");
    }

    // --abrir: parseia o projeto PRIMEIRO para sobrescrever centro/lado/zoom.
    let mut projeto_aberto = None;
    if let Some(caminho) = &cfg.abrir {
        let p = projeto::Projeto::abrir(caminho)
            .map_err(|e| anyhow::anyhow!("falha ao abrir projeto {}: {e}", caminho.display()))?;
        eprintln!(
            "Projeto '{}' aberto: {} objeto(s), v{}",
            p.nome,
            p.objetos.len(),
            p.versao
        );
        cfg.centro = arcz_geo::Geodetic::new(p.lon, p.lat, 0.0);
        cfg.lado_m = p.lado_m;
        cfg.zoom_dem = p.zoom_dem;
        cfg.zoom_imagery = p.zoom_imagery;
        projeto_aberto = Some(p);
    } // Operacoes de catalogo encerram antes de carregar terreno ou modelo.
    if let Some(cmd) = &cfg.comando_projeto {
        workspace::executar_comando(cmd, |nome| projeto::Projeto {
            versao: projeto::VERSAO_FORMATO,
            nome: nome.to_string(),
            lat: cfg.centro.lat_deg,
            lon: cfg.centro.lon_deg,
            lado_m: cfg.lado_m,
            zoom_dem: cfg.zoom_dem,
            zoom_imagery: cfg.zoom_imagery,
            mes: 3,
            dia: 21,
            hora: 15.0,
            objetos: Vec::new(),
            cameras: Vec::new(),
        })?;
        return Ok(());
    }

    // --mobiliar + --salvar: grava o .arcz sem abrir modelo nenhum. E o unico jeito
    // de salvar o predio inteiro (1.785 moveis) — pelo Editor a memoria estoura.
    if let (Some(caminho), Some(destino)) = (&cfg.mobiliar, &cfg.salvar) {
        let emp = planta::Empreendimento::abrir(caminho)
            .map_err(|e| anyhow::anyhow!("falha ao ler {}: {e}", caminho.display()))?;
        let emp = match &cfg.mobiliar_pavimento {
            Some(f) => planta::filtrar_pavimento(&emp, f),
            None => emp,
        };
        for p in planta::verificar(&emp).iter().take(20) {
            eprintln!("PLANTA: {p}");
        }
        let (colocacoes, avisos) = planta::compor(&emp, &cfg.biblioteca_raiz);
        for a in &avisos {
            eprintln!("AVISO: {a}");
        }
        let p = planta::projeto_de_colocacoes(
            &colocacoes,
            emp.nome.clone(),
            emp.lat,
            emp.lon,
            cfg.lado_m,
        );
        p.salvar(destino)
            .map_err(|e| anyhow::anyhow!("falha ao salvar {}: {e}", destino.display()))?;
        println!(
            "Projeto '{}' salvo em {} — {} movel(is) de {} unidade(s) em {} pavimento(s)",
            p.nome,
            destino.display(),
            p.objetos.len(),
            emp.pavimentos
                .iter()
                .map(|x| x.unidades.len())
                .sum::<usize>(),
            emp.pavimentos.len()
        );
        return Ok(());
    }

    let rt = tokio::runtime::Runtime::new()?;
    let scene = rt.block_on(scene::carregar(&cfg))?;
    scene.relatorio.imprimir();

    // Constroi o Editor: do projeto aberto OU da biblioteca OU vazio.
    let editor = if let Some(p) = projeto_aberto.as_ref() {
        let (ed, erros) = cena::editor_de_projeto(p);
        if !erros.is_empty() {
            eprintln!(
                "AVISO: {} objeto(s) do projeto nao puderam ser carregados:",
                erros.len()
            );
            for (path, msg) in erros.iter().take(10) {
                eprintln!("  - {}: {}", path.display(), msg);
            }
        }
        Some(ed)
    } else if let Some(raiz) = &cfg.biblioteca {
        let centro = scene.bbox.center();
        let (mut ed, erros) =
            cena::editor_de_biblioteca(raiz, centro.lat_deg, centro.lon_deg, 0.0, 1.0, 500);
        if !erros.is_empty() {
            eprintln!(
                "AVISO: {} arquivo(s) da biblioteca nao puderam ser carregados:",
                erros.len()
            );
            for (path, msg) in erros.iter().take(10) {
                eprintln!("  - {}: {}", path.display(), msg);
            }
            if erros.len() > 10 {
                eprintln!("  ... e mais {}", erros.len() - 10);
            }
        }
        ed.selecionado = ed.objetos.first().map(|o| o.id);
        Some(ed)
    } else {
        None
    };
    let mut editor = editor;

    // O modelo de `--modelo` e desenhado por um caminho proprio (`scene.modelo`),
    // fora do `Editor`. Isso o deixava invisivel para `Editor::picar`: o clique
    // selecionava o entorno inteiro e nunca o predio do projeto — justo o objeto
    // que o usuario mais quer manipular. Registra-lo aqui o torna clicavel pela
    // mesma rota, sem duplicar geometria: a fonte e a mesma que o renderer usa.
    if let (Some(fonte), Some(caminho)) = (&scene.fonte_modelo, cfg.modelo.as_ref()) {
        let ed = editor.get_or_insert_with(cena::Editor::default);
        let ja_tem = ed.objetos.iter().any(|o| o.arquivo == *caminho);
        if !ja_tem {
            let nome = caminho
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Modelo".into());
            ed.registrar_fonte(nome, fonte.clone(), scene.placement, caminho.clone());
            if let Some(o) = ed.objetos.last_mut() {
                o.transformar(&scene.frame, scene.solo_modelo_m);
            }
        }
    }

    // --mobiliar: instancia as plantas mobiliadas por cima do que ja esta na cena.
    if let Some(caminho) = &cfg.mobiliar {
        let ed = editor.get_or_insert_with(cena::Editor::default);
        mobiliar(&cfg, caminho, ed)?;

        if std::env::var("ARCZ_DEBUG_PLANTA").is_ok() {
            if let Some(m) = &scene.modelo {
                eprintln!(
                    "DEBUG predio ENU: x {:.2}..{:.2} y {:.2}..{:.2} z {:.2}..{:.2}",
                    m.min_enu[0],
                    m.max_enu[0],
                    m.min_enu[1],
                    m.max_enu[1],
                    m.min_enu[2],
                    m.max_enu[2]
                );
            }
            for o in ed.objetos.iter_mut().take(6) {
                o.transformar(&scene.frame, scene.solo_modelo_m);
                eprintln!(
                    "DEBUG {}: ENU x {:.2}..{:.2} y {:.2}..{:.2} z {:.2}..{:.2}",
                    o.nome,
                    o.min_enu[0],
                    o.max_enu[0],
                    o.min_enu[1],
                    o.max_enu[1],
                    o.min_enu[2],
                    o.max_enu[2]
                );
            }
        }
    }

    if let Some(ed) = &editor {
        eprintln!("Cena: {} objeto(s) na cena editavel", ed.objetos.len());
    }

    // --salvar: depois de carregar, salva o estado num .arcz e sai.
    if let Some(destino) = &cfg.salvar {
        if let Some(ed) = &editor {
            let p = cena::projeto_de_editor(ed, "sem-titulo".into(), &scene);
            p.salvar(destino)
                .map_err(|e| anyhow::anyhow!("falha ao salvar {}: {e}", destino.display()))?;
            eprintln!(
                "Projeto salvo em {} ({} objeto(s))",
                destino.display(),
                p.objetos.len()
            );
        } else {
            eprintln!("AVISO: --salvar sem objetos na cena, salvando projeto vazio");
            let p = projeto::Projeto {
                versao: projeto::VERSAO_FORMATO,
                nome: "vazio".into(),
                lat: scene.bbox.center().lat_deg,
                lon: scene.bbox.center().lon_deg,
                lado_m: scene.lado_m,
                zoom_dem: cfg.zoom_dem,
                zoom_imagery: cfg.zoom_imagery,
                mes: 3,
                dia: 21,
                hora: 15.0,
                objetos: Vec::new(),
                cameras: Vec::new(),
            };
            p.salvar(destino)
                .map_err(|e| anyhow::anyhow!("falha ao salvar {}: {e}", destino.display()))?;
        }
        return Ok(());
    }

    if let Some(porta) = cfg.serve {
        return server::servir(porta, cfg, scene, editor);
    }

    if let Some(destino) = &cfg.png {
        let mut camera = viewport::camera_inicial(&scene, editor.as_ref());
        if let Some(p) = cfg.cam_pitch_deg {
            camera.pitch = p.to_radians().clamp(-1.553, 1.553);
        }
        if let Some(y) = cfg.cam_yaw_deg {
            camera.yaw = y.to_radians();
        }
        if let Some(d) = cfg.cam_dist_m {
            camera.distancia = d.max(1.0);
        }
        let mut r = renderer::Renderer::new(&scene, editor.as_ref())?;
        r.vista = cfg.vista();
        r.salvar_png(&camera, cfg.png_largura, cfg.png_altura, destino)?;
        println!(
            "PNG       {} ({}x{})",
            destino.display(),
            cfg.png_largura,
            cfg.png_altura
        );
    }

    if cfg.headless || cfg.png.is_some() {
        return Ok(());
    }

    println!(
        "Controles: arrastar = orbitar | roda = zoom | R = enquadrar modelo | \
         T = enquadrar terreno | C = coordenada do alvo | Delete = remover | \
         Esc = desselecionar | Ctrl+D = duplicar | ESC duplo = sair"
    );
    viewport::run(scene, editor, cfg.vista())
}

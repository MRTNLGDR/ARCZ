//! Converte as malhas do OSM em objetos editaveis da cena.
//!
//! ## Por que isto existe
//!
//! A primeira versao mandou o entorno direto para a GPU (`Recursos::objetos`),
//! pulando o `Editor`. O bairro aparecia, mas `Editor::picar` so percorre
//! `Editor::objetos` — entao clicar num predio do bairro nao acertava nada e o
//! gizmo continuava preso no modelo importado. O sintoma que o Lucas relatou foi
//! exatamente esse: *"quando movo, rotaciono ou posiciono, so o modelo se move,
//! nao o mapa"*.
//!
//! ## O truque que evita reescrever o gerador
//!
//! As malhas do `arcz-osm` nascem em **coordenadas de mundo**, e um `Objeto`
//! espera geometria no espaco do arquivo, posicionada por `Placement`. Em vez de
//! reescrever o gerador para produzir espaco local, aproveita-se a forma da
//! matriz:
//!
//! ```text
//! matriz = T(destino) * R(rumo) * S(escala) * T(-ancora)
//! ```
//!
//! A ancora e o centro XZ da caixa da fonte, na base. Se o `Placement` apontar
//! para **a propria posicao geografica desse centro**, com rumo 0 e escala 1,
//! entao `T(destino) == T(ancora)` e a matriz e a identidade: a geometria fica
//! exatamente onde o gerador a colocou.
//!
//! O ganho e que, a partir dai, ela e um objeto como qualquer outro — mexer no
//! `Placement` move, gira e escala de verdade, e picking, gizmo, undo/redo,
//! Outliner e salvamento passam a funcionar sem nenhum caso especial.

use arcz_geo::{Enu, EnuFrame};
use arcz_model::{FonteGeometria, Material, ModelVertex, Placement, Submesh};
use arcz_osm::MalhaProcedural;

use crate::cena::{Editor, Objeto};

/// Primeiro id da faixa reservada ao entorno.
///
/// Objetos do usuario usam ids pequenos e crescentes. A faixa separada deixa
/// `remover_entorno` limpar so o que veio do OSM.
pub const ID_BASE: u32 = 1_000_000;

/// Converte uma malha procedural num objeto editavel, ancorado no proprio centro.
pub fn objeto_de_malha(m: &MalhaProcedural, frame: &EnuFrame, id: u32) -> Objeto {
    let vertices: Vec<ModelVertex> = m
        .vertices
        .iter()
        .map(|v| ModelVertex {
            position: v.pos,
            normal: v.normal,
            uv: v.uv,
        })
        .collect();

    let (min, max) = m.envolvente();

    // Centro XZ da caixa, na base — a mesma ancora que `matriz_modelo` usa.
    let centro_x = (min[0] + max[0]) * 0.5;
    let centro_z = (min[2] + max[2]) * 0.5;

    // Render (x=leste, y=cima, z=-norte) de volta para ENU e dai para geodetico.
    let g = frame.enu_to_geodetic(Enu::new(centro_x as f64, -centro_z as f64, 0.0));

    let material = Material {
        nome: m.nome.clone(),
        base_color: [m.cor[0], m.cor[1], m.cor[2], 1.0],
        textura: None,
        metallic: 0.0,
        // Alvenaria e asfalto sao foscos; deixar liso daria plastico brilhante.
        roughness: 0.85,
        transparente: false,
    };

    Objeto {
        id,
        nome: m.nome.clone(),
        pai: None,
        fonte: FonteGeometria { vertices, min, max },
        indices: m.indices.clone(),
        submeshes: vec![Submesh {
            material: 0,
            offset: 0,
            count: m.indices.len() as u32,
        }],
        materiais: vec![material],
        texturas: Vec::new(),
        placement: Placement {
            lat_deg: g.lat_deg,
            lon_deg: g.lon_deg,
            heading_deg: 0.0,
            escala: 1.0,
            // A geometria ja esta na cota certa: assentar de novo a moveria.
            assentar_no_terreno: false,
            offset_vertical_m: 0.0,
            offset_leste_m: 0.0,
            offset_norte_m: 0.0,
        },
        visivel: true,
        selecionavel: true,
        bloqueado: false,
        // O entorno nao vem de arquivo. O caminho vazio sinaliza isso ao
        // salvamento, que precisa regerar em vez de recarregar do disco.
        arquivo: std::path::PathBuf::new(),
        min_enu: min,
        max_enu: max,
        // O entorno nasce em coordenadas de mundo, entao a matriz e identidade
        // — e por isso o objeto fica exatamente onde o gerador o colocou.
        matriz: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    }
}

/// Substitui o entorno do editor pelas malhas dadas. Devolve quantos objetos
/// entraram.
pub fn instalar(editor: &mut Editor, malhas: &[MalhaProcedural], frame: &EnuFrame) -> usize {
    remover(editor);
    for (i, m) in malhas.iter().enumerate() {
        if m.indices.is_empty() {
            continue;
        }
        editor
            .objetos
            .push(objeto_de_malha(m, frame, ID_BASE + i as u32));
    }
    editor.objetos.iter().filter(|o| e_do_entorno(o.id)).count()
}

/// Descarta o entorno, preservando os objetos do usuario.
pub fn remover(editor: &mut Editor) {
    editor.objetos.retain(|o| !e_do_entorno(o.id));
    if editor.selecionado.is_some_and(e_do_entorno) {
        editor.selecionado = None;
    }
}

pub fn e_do_entorno(id: u32) -> bool {
    id >= ID_BASE
}

/// Reconstitui as malhas do entorno a partir dos objetos do `Editor`.
///
/// Serve à exportação glTF: o gerador roda uma vez e o resultado vira objeto
/// editável; exportar precisa do caminho de volta. Reler do OSM daria geometria
/// diferente da que está na tela se o usuário tiver movido algo.
///
/// A cor vem do material, que é onde ela ficou ao virar objeto.
pub fn malhas_do_editor(editor: &Editor) -> Vec<MalhaProcedural> {
    editor
        .objetos
        .iter()
        .filter(|o| e_do_entorno(o.id) && o.visivel)
        .map(|o| MalhaProcedural {
            nome: o.nome.clone(),
            // O tipo de origem não sobrevive à conversão para `Objeto` e não é
            // usado na exportação; o que importa lá é a geometria e a cor.
            origem: arcz_osm::malha::Origem::Edificio(o.id as i64),
            cor: o
                .materiais
                .first()
                .map(|m| [m.base_color[0], m.base_color[1], m.base_color[2]])
                .unwrap_or([0.75, 0.75, 0.75]),
            vertices: o
                .fonte
                .vertices
                .iter()
                .map(|v| arcz_osm::Vertice {
                    pos: v.position,
                    normal: v.normal,
                    uv: v.uv,
                })
                .collect(),
            indices: o.indices.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcz_geo::Geodetic;
    use arcz_osm::{malha, Edificio, Entorno, FonteAltura, PontoGeo, Telhado};

    fn frame() -> EnuFrame {
        EnuFrame::new(Geodetic::new(-48.5022653, -27.1544967, 0.0))
    }

    fn malha_de_um_predio() -> MalhaProcedural {
        let (lat, lon) = (-27.1544967, -48.5022653);
        let d = 0.0002;
        let ed = Edificio {
            id: 1,
            nome: Some("Bloco".into()),
            classe: arcz_osm::ClasseEdificio::Apartamentos,
            contorno: vec![
                PontoGeo { lat, lon },
                PontoGeo { lat, lon: lon + d },
                PontoGeo {
                    lat: lat + d,
                    lon: lon + d,
                },
                PontoGeo { lat: lat + d, lon },
            ],
            altura_m: 24.0,
            base_m: 0.0,
            fonte_altura: FonteAltura::Medida,
            telhado: Telhado::Plano,
            cor_parede: None,
            cor_telhado: None,
        };
        malha::gerar_edificio(&ed, &frame(), &arcz_osm::TerrenoPlano(7.0)).unwrap()
    }

    #[test]
    fn a_matriz_do_objeto_de_entorno_e_a_identidade() {
        // O teste central do modulo. Se a matriz nao sair identidade, o bairro
        // inteiro salta de lugar no instante em que vira objeto editavel.
        let f = frame();
        let m = malha_de_um_predio();
        let o = objeto_de_malha(&m, &f, ID_BASE);

        let matriz = arcz_model::matriz_modelo(o.fonte.min, o.fonte.max, &f, &o.placement, 0.0);

        for (i, linha) in matriz.iter().enumerate() {
            for (j, v) in linha.iter().enumerate() {
                let esperado = if i == j { 1.0 } else { 0.0 };
                assert!(
                    (v - esperado).abs() < 1e-3,
                    "matriz[{i}][{j}] = {v}, esperava {esperado}"
                );
            }
        }
    }

    #[test]
    fn a_geometria_nao_se_move_ao_virar_objeto() {
        // Verificacao independente da anterior: compara a caixa antes e depois.
        let f = frame();
        let m = malha_de_um_predio();
        let (min_antes, max_antes) = m.envolvente();

        let mut o = objeto_de_malha(&m, &f, ID_BASE);
        o.transformar(&f, 0.0);

        for k in 0..3 {
            assert!(
                (o.min_enu[k] - min_antes[k]).abs() < 0.01,
                "min[{k}] moveu de {} para {}",
                min_antes[k],
                o.min_enu[k]
            );
            assert!(
                (o.max_enu[k] - max_antes[k]).abs() < 0.01,
                "max[{k}] moveu de {} para {}",
                max_antes[k],
                o.max_enu[k]
            );
        }
    }

    #[test]
    fn o_predio_do_entorno_pode_ser_clicado() {
        // A queixa original: clicar no bairro nao selecionava nada porque
        // `picar` so ve `Editor::objetos`.
        let f = frame();
        let m = malha_de_um_predio();
        let mut ed = Editor::default();
        assert_eq!(instalar(&mut ed, std::slice::from_ref(&m), &f), 1);

        // Raio vindo de cima, no centro do predio.
        let (min, max) = m.envolvente();
        let centro = [
            ((min[0] + max[0]) * 0.5) as f64,
            (max[1] + 50.0) as f64,
            ((min[2] + max[2]) * 0.5) as f64,
        ];
        let alvo = ed.picar(centro, [0.0, -1.0, 0.0]);
        assert_eq!(alvo, Some(ID_BASE), "o raio de cima nao acertou o predio");
    }

    #[test]
    fn mover_o_placement_move_o_predio_do_entorno() {
        // O que o Lucas pediu: o gizmo tem de mexer no bairro, nao so no modelo.
        let f = frame();
        let m = malha_de_um_predio();
        let mut o = objeto_de_malha(&m, &f, ID_BASE);
        o.transformar(&f, 0.0);
        let antes = o.centro();

        o.placement.offset_leste_m = 30.0;
        o.transformar(&f, 0.0);
        let depois = o.centro();

        assert!(
            (depois[0] - antes[0] - 30.0).abs() < 0.1,
            "mover 30 m para leste deslocou {} m",
            depois[0] - antes[0]
        );
        assert!(
            (depois[2] - antes[2]).abs() < 0.1,
            "o deslocamento para leste mexeu no norte"
        );
    }

    #[test]
    fn girar_o_placement_gira_o_predio_do_entorno() {
        let f = frame();
        let m = malha_de_um_predio();
        let mut o = objeto_de_malha(&m, &f, ID_BASE);
        o.transformar(&f, 0.0);
        let antes = o.tamanho();

        // 90° troca as dimensoes horizontais de uma planta retangular.
        o.placement.heading_deg = 90.0;
        o.transformar(&f, 0.0);
        let depois = o.tamanho();

        assert!(
            (depois[0] - antes[2]).abs() < 0.5 && (depois[2] - antes[0]).abs() < 0.5,
            "girar 90° nao trocou as dimensoes: {antes:?} -> {depois:?}"
        );
        assert!(
            (depois[1] - antes[1]).abs() < 0.01,
            "girar no plano mudou a altura"
        );
    }

    #[test]
    fn escalar_o_placement_escala_o_predio_do_entorno() {
        let f = frame();
        let mut o = objeto_de_malha(&malha_de_um_predio(), &f, ID_BASE);
        o.transformar(&f, 0.0);
        let antes = o.tamanho();

        o.placement.escala = 2.0;
        o.transformar(&f, 0.0);
        let depois = o.tamanho();

        for k in 0..3 {
            assert!(
                (depois[k] - antes[k] * 2.0).abs() < 0.1,
                "eixo {k} escalou de {} para {} (esperava o dobro)",
                antes[k],
                depois[k]
            );
        }
    }

    #[test]
    fn instalar_troca_o_entorno_sem_tocar_nos_objetos_do_usuario() {
        let f = frame();
        let m = malha_de_um_predio();
        let mut ed = Editor::default();

        // Objeto do usuario, com id fora da faixa do entorno.
        let mut meu = objeto_de_malha(&m, &f, 7);
        meu.nome = "Zenite".into();
        ed.objetos.push(meu);

        instalar(&mut ed, &[m.clone(), m.clone()], &f);
        assert_eq!(ed.objetos.len(), 3);

        // Reinstalar troca so o entorno.
        assert_eq!(instalar(&mut ed, std::slice::from_ref(&m), &f), 1);
        assert_eq!(ed.objetos.len(), 2);
        assert!(
            ed.objetos.iter().any(|o| o.nome == "Zenite"),
            "o objeto do usuario sumiu"
        );

        remover(&mut ed);
        assert_eq!(ed.objetos.len(), 1);
        assert_eq!(ed.objetos[0].nome, "Zenite");
    }

    #[test]
    fn remover_limpa_a_selecao_presa_no_entorno() {
        // Sem isto o gizmo continuaria desenhado sobre um objeto que nao existe.
        let f = frame();
        let mut ed = Editor::default();
        instalar(&mut ed, &[malha_de_um_predio()], &f);
        ed.selecionado = Some(ID_BASE);

        remover(&mut ed);
        assert_eq!(ed.selecionado, None);
    }

    #[test]
    fn a_faixa_de_ids_separa_entorno_de_usuario() {
        assert!(!e_do_entorno(0));
        assert!(!e_do_entorno(999_999));
        assert!(e_do_entorno(ID_BASE));
    }

    #[test]
    fn malhas_vazias_nao_viram_objeto() {
        let f = frame();
        let vazia = MalhaProcedural {
            nome: "vazia".into(),
            origem: arcz_osm::malha::Origem::Vegetacao,
            cor: [0.0, 0.0, 0.0],
            vertices: Vec::new(),
            indices: Vec::new(),
        };
        let mut ed = Editor::default();
        assert_eq!(instalar(&mut ed, &[vazia], &f), 0);
    }

    #[test]
    fn um_bairro_inteiro_vira_objetos_clicaveis() {
        // Escala realista: o recorte de Bombinhas gera centenas de malhas.
        let f = frame();
        let (lat, lon) = (-27.1544967, -48.5022653);
        let entorno = Entorno {
            vias: vec![arcz_osm::Via {
                id: 1,
                nome: Some("Rua".into()),
                classe: arcz_osm::ClasseVia::Local,
                largura_m: 6.0,
                eixo: vec![
                    PontoGeo {
                        lat,
                        lon: lon - 0.002,
                    },
                    PontoGeo {
                        lat,
                        lon: lon + 0.002,
                    },
                ],
            }],
            ..Default::default()
        };
        let mut entorno = entorno;
        let gerados =
            arcz_osm::procedural::adensar(&mut entorno, &f, arcz_osm::RegrasUrbanas::default());
        assert!(gerados > 10);

        let malhas = malha::gerar(
            &entorno,
            &f,
            &arcz_osm::TerrenoPlano(0.0),
            arcz_osm::Opcoes::default(),
        );
        let mut ed = Editor::default();
        let n = instalar(&mut ed, &malhas, &f);
        assert_eq!(n, malhas.len());

        // Cada predio tem de ser alcancavel por um raio vindo de cima.
        let mut acertos = 0;
        for o in ed.objetos.iter().filter(|o| e_do_entorno(o.id)) {
            let c = o.centro();
            if ed
                .picar(
                    [c[0] as f64, (o.max_enu[1] + 100.0) as f64, c[2] as f64],
                    [0.0, -1.0, 0.0],
                )
                .is_some()
            {
                acertos += 1;
            }
        }
        assert_eq!(acertos, n, "nem todo objeto do entorno e clicavel");
    }
}

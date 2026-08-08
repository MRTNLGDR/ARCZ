//! Catalogo curado de mobiliario e decoracao para o ARCZ.
//!
//! Cada item declara a **licenca no tipo**, pelo mesmo motivo de
//! `arcz_terrain::source`: o ARCZ e destinado a uso comercial, entao licenca nao e
//! nota de rodape. Nada entra aqui sem ser CC0 (dominio publico) ou gerado pelo
//! proprio ARCZ.
//!
//! Duas origens:
//!
//! - [`Fonte::PolyHaven`] — modelos fotorreais escaneados/modelados, CC0, baixados sob
//!   demanda ([`crate::polyhaven`]). Sao as pecas "heroi": sofa, poltrona, mesa,
//!   luminaria, planta.
//! - [`Fonte::Parametrica`] — pecas que **nao existem** em nenhum acervo CC0 fotorreal
//!   (cama moderna, guarda-roupa, bancada, loucas de banheiro, TV, tapete). Sao
//!   geradas em glTF pelo [`crate::parametrico`] com a medida exata da planta e cor
//!   neutra. Melhor um volume neutro na medida certa do que um modelo bonito na
//!   medida errada — a planta manda.

use crate::parametrico::Peca;

/// Classe de licenca de um asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Licenca {
    /// CC0 / dominio publico. Uso comercial livre, sem credito obrigatorio.
    Cc0,
    /// Gerado pelo proprio ARCZ. Sem terceiro envolvido.
    Proprio,
}

impl Licenca {
    /// `true` se pode entrar num produto comercial sem negociacao nem credito.
    pub fn comercialmente_livre(self) -> bool {
        matches!(self, Self::Cc0 | Self::Proprio)
    }

    pub fn texto(self) -> &'static str {
        match self {
            Self::Cc0 => "CC0 1.0 (dominio publico) — Poly Haven",
            Self::Proprio => "Gerado pelo ARCZ (parametrico)",
        }
    }
}

/// De onde o arquivo do item vem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fonte {
    /// Slug do asset em <https://polyhaven.com/a/{slug}>.
    PolyHaven { slug: &'static str },
    /// Peca gerada localmente em glTF.
    Parametrica(Peca),
}

/// Onde o item e usado no empreendimento.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ambiente {
    /// Interior das unidades (sala, quartos, cozinha, banhos, sacada).
    Apartamento,
    /// Hall, recepcao e circulacao do embasamento.
    Recepcao,
    /// Cafeteria do terreo (a "sceno" da fachada).
    Cafeteria,
    /// Mercado/loja do terreo (a "BRZZ" da fachada).
    Mercado,
    /// Rooftop: convivio, gourmet, piscina e deck.
    Rooftop,
}

/// Papel do item na planta. E por ele que a planta pede mobilia:
/// a planta diz "aqui vai um [`Papel::SofaTresLugares`]", nao "aqui vai sofa_02".
/// Trocar o modelo depois nao mexe na planta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Papel {
    // dormir
    CamaCasal,
    CamaSolteiro,
    Criado,
    GuardaRoupa,
    // estar
    SofaTresLugares,
    SofaDoisLugares,
    Poltrona,
    Puff,
    MesaCentro,
    MesaLateral,
    Rack,
    Tv,
    Tapete,
    Estante,
    // jantar / cozinha
    MesaJantar,
    CadeiraJantar,
    BancadaCozinha,
    Cooktop,
    Geladeira,
    MicroOndas,
    Banqueta,
    // banho
    VasoSanitario,
    CubaBanheiro,
    BoxChuveiro,
    // decoracao
    Vaso,
    PlantaVaso,
    Quadro,
    Espelho,
    Luminaria,
    LuminariaPendente,
    Almofadas,
    Livros,
    Relogio,
    // externo / comum
    BalcaoRecepcao,
    Poltronalounge,
    MesaExterna,
    Espreguicadeira,
    GuardaSol,
    Floreira,
    Banco,
    Churrasqueira,
    // comercial
    BalcaoCafe,
    Caixa,
    JogoDeCha,
    PrateleiraLoja,
}

/// Um item do catalogo.
#[derive(Debug, Clone, Copy)]
pub struct Item {
    /// Chave estavel, usada nas plantas e no nome da pasta em disco.
    pub chave: &'static str,
    pub nome: &'static str,
    pub papel: Papel,
    pub fonte: Fonte,
    pub licenca: Licenca,
    pub ambientes: &'static [Ambiente],
    /// Medida nominal (largura, altura, profundidade) em metros. Serve para a planta
    /// conferir se o item cabe no comodo e para calcular a escala quando o arquivo
    /// vem em outra unidade.
    pub dimensao_m: [f32; 3],
}

impl Item {
    /// Nome da pasta do item dentro da biblioteca.
    pub fn pasta(&self) -> String {
        self.chave.to_string()
    }

    /// `true` se o arquivo precisa ser baixado da internet.
    pub fn remoto(&self) -> bool {
        matches!(self.fonte, Fonte::PolyHaven { .. })
    }
}

use Ambiente::*;
use Papel::*;

const AP: &[Ambiente] = &[Apartamento];
const AP_REC: &[Ambiente] = &[Apartamento, Recepcao];
const AP_ROOF: &[Ambiente] = &[Apartamento, Rooftop];
const REC: &[Ambiente] = &[Recepcao];
const CAFE: &[Ambiente] = &[Cafeteria];
const ROOF: &[Ambiente] = &[Rooftop];
const TODOS: &[Ambiente] = &[Apartamento, Recepcao, Cafeteria, Mercado, Rooftop];

/// Item vindo do Poly Haven (CC0).
const fn ph(
    chave: &'static str,
    nome: &'static str,
    slug: &'static str,
    papel: Papel,
    ambientes: &'static [Ambiente],
    dimensao_m: [f32; 3],
) -> Item {
    Item {
        chave,
        nome,
        papel,
        fonte: Fonte::PolyHaven { slug },
        licenca: Licenca::Cc0,
        ambientes,
        dimensao_m,
    }
}

/// Peca gerada pelo ARCZ.
const fn par(
    chave: &'static str,
    nome: &'static str,
    peca: Peca,
    papel: Papel,
    ambientes: &'static [Ambiente],
    dimensao_m: [f32; 3],
) -> Item {
    Item {
        chave,
        nome,
        papel,
        fonte: Fonte::Parametrica(peca),
        licenca: Licenca::Proprio,
        ambientes,
        dimensao_m,
    }
}

/// O catalogo. Curadoria: linhas retas, madeira clara/media, tecido cinza e off-white
/// — a mesma paleta neutra dos renders do Zenite.
pub const CATALOGO: &[Item] = &[
    // ---------------- estar ----------------
    ph(
        "sofa-3-lugares",
        "Sofa 3 lugares",
        "sofa_02",
        SofaTresLugares,
        AP,
        [2.2, 0.85, 0.9],
    ),
    ph(
        "sofa-2-lugares",
        "Sofa 2 lugares",
        "sofa_03",
        SofaDoisLugares,
        AP,
        [1.7, 0.85, 0.9],
    ),
    ph(
        "sofa-modular",
        "Sofa modular",
        "Sofa_01",
        SofaTresLugares,
        AP_REC,
        [2.4, 0.8, 0.95],
    ),
    ph(
        "poltrona-moderna",
        "Poltrona moderna",
        "modern_arm_chair_01",
        Poltrona,
        AP_REC,
        [0.75, 0.8, 0.8],
    ),
    ph(
        "poltrona-mid-century",
        "Poltrona mid-century",
        "mid_century_lounge_chair",
        Poltrona,
        AP_REC,
        [0.8, 0.78, 0.85],
    ),
    ph(
        "poltrona-braco",
        "Poltrona de braco",
        "ArmChair_01",
        Poltrona,
        AP,
        [0.85, 0.85, 0.9],
    ),
    ph("puff", "Puff", "Ottoman_01", Puff, AP, [0.6, 0.42, 0.6]),
    ph(
        "mesa-centro-1",
        "Mesa de centro retangular",
        "modern_coffee_table_01",
        MesaCentro,
        AP,
        [1.1, 0.4, 0.6],
    ),
    ph(
        "mesa-centro-2",
        "Mesa de centro baixa",
        "modern_coffee_table_02",
        MesaCentro,
        AP,
        [1.2, 0.35, 0.65],
    ),
    ph(
        "mesa-centro-redonda",
        "Mesa de centro redonda",
        "coffee_table_round_01",
        MesaCentro,
        AP_REC,
        [0.8, 0.4, 0.8],
    ),
    ph(
        "mesa-lateral",
        "Mesa lateral",
        "side_table_01",
        MesaLateral,
        AP,
        [0.45, 0.5, 0.45],
    ),
    ph(
        "mesa-lateral-alta",
        "Mesa lateral alta",
        "side_table_tall_01",
        MesaLateral,
        AP,
        [0.4, 0.65, 0.4],
    ),
    ph(
        "almofadas",
        "Almofadas",
        "throw_pillows_01",
        Almofadas,
        AP,
        [0.5, 0.15, 0.5],
    ),
    ph(
        "livros",
        "Livros decorativos",
        "book_encyclopedia_set_01",
        Livros,
        AP_REC,
        [0.3, 0.12, 0.2],
    ),
    ph(
        "estante",
        "Estante",
        "Shelf_01",
        Estante,
        AP,
        [0.9, 1.8, 0.35],
    ),
    ph(
        "armario-madeira",
        "Armario de madeira",
        "modern_wooden_cabinet",
        Rack,
        AP,
        [1.6, 0.6, 0.45],
    ),
    ph(
        "gaveteiro",
        "Gaveteiro",
        "drawer_cabinet",
        Rack,
        AP,
        [1.0, 0.8, 0.45],
    ),
    // ---------------- jantar / cozinha ----------------
    ph(
        "mesa-jantar",
        "Mesa de jantar",
        "dining_table",
        MesaJantar,
        AP,
        [1.8, 0.75, 0.9],
    ),
    ph(
        "cadeira-jantar",
        "Cadeira de jantar",
        "dining_chair_02",
        CadeiraJantar,
        AP,
        [0.45, 0.9, 0.5],
    ),
    ph(
        "mesa-redonda",
        "Mesa redonda",
        "round_wooden_table_01",
        MesaJantar,
        &[Apartamento, Cafeteria, Rooftop],
        [1.0, 0.75, 1.0],
    ),
    ph(
        "mesa-redonda-2",
        "Mesa redonda pequena",
        "round_wooden_table_02",
        MesaJantar,
        CAFE,
        [0.8, 0.74, 0.8],
    ),
    ph(
        "banqueta-alta",
        "Banqueta alta",
        "bar_chair_round_01",
        Banqueta,
        &[Apartamento, Cafeteria, Rooftop],
        [0.4, 1.05, 0.4],
    ),
    ph(
        "banqueta-metal",
        "Banqueta de metal",
        "metal_stool_01",
        Banqueta,
        CAFE,
        [0.35, 0.75, 0.35],
    ),
    ph(
        "cooktop",
        "Cooktop / fogao",
        "electric_stove",
        Cooktop,
        AP,
        [0.6, 0.9, 0.6],
    ),
    ph(
        "micro-ondas",
        "Micro-ondas",
        "vintage_microwave",
        MicroOndas,
        AP,
        [0.5, 0.3, 0.38],
    ),
    ph(
        "jogo-cha",
        "Jogo de cha",
        "tea_set_01",
        JogoDeCha,
        &[Apartamento, Cafeteria],
        [0.35, 0.15, 0.3],
    ),
    // ---------------- dormir (parametrico: nao ha equivalente CC0 fotorreal) --------
    par(
        "cama-casal",
        "Cama de casal",
        Peca::CamaCasal,
        CamaCasal,
        AP,
        [1.70, 1.00, 2.10],
    ),
    par(
        "cama-solteiro",
        "Cama de solteiro",
        Peca::CamaSolteiro,
        CamaSolteiro,
        AP,
        [1.00, 1.00, 2.00],
    ),
    par(
        "guarda-roupa",
        "Guarda-roupa",
        Peca::GuardaRoupa,
        GuardaRoupa,
        AP,
        [2.00, 2.40, 0.64],
    ),
    ph(
        "criado-mudo",
        "Criado-mudo",
        "ClassicNightstand_01",
        Criado,
        AP,
        [0.45, 0.55, 0.4],
    ),
    ph(
        "criado-mudo-2",
        "Criado-mudo pintado",
        "painted_wooden_nightstand",
        Criado,
        AP,
        [0.5, 0.6, 0.4],
    ),
    ph(
        "relogio-mesa",
        "Relogio de mesa",
        "alarm_clock_01",
        Relogio,
        AP,
        [0.12, 0.14, 0.08],
    ),
    // ---------------- banho / cozinha construida (parametrico) ----------------
    par(
        "bancada-cozinha",
        "Bancada de cozinha com cuba",
        Peca::BancadaCozinha,
        BancadaCozinha,
        AP,
        [2.44, 2.20, 0.64],
    ),
    par(
        "geladeira",
        "Geladeira",
        Peca::Geladeira,
        Geladeira,
        AP,
        [0.7, 1.85, 0.7],
    ),
    par(
        "rack-tv",
        "Painel de TV",
        Peca::RackTv,
        Tv,
        AP,
        [1.80, 1.60, 0.38],
    ),
    par(
        "tapete",
        "Tapete",
        Peca::Tapete,
        Tapete,
        AP_REC,
        [2.4, 0.02, 1.7],
    ),
    par(
        "vaso-sanitario",
        "Vaso sanitario",
        Peca::VasoSanitario,
        VasoSanitario,
        AP,
        [0.38, 0.78, 0.66],
    ),
    par(
        "cuba-banheiro",
        "Bancada com cuba",
        Peca::CubaBanheiro,
        CubaBanheiro,
        AP,
        [0.94, 1.65, 0.53],
    ),
    par(
        "box-chuveiro",
        "Box de vidro",
        Peca::BoxChuveiro,
        BoxChuveiro,
        AP,
        [0.9, 2.0, 0.9],
    ),
    // ---------------- decoracao ----------------
    ph(
        "planta-vaso-1",
        "Planta em vaso (grande)",
        "potted_plant_01",
        PlantaVaso,
        TODOS,
        [0.6, 1.3, 0.6],
    ),
    ph(
        "planta-vaso-2",
        "Planta em vaso (media)",
        "potted_plant_02",
        PlantaVaso,
        TODOS,
        [0.5, 0.9, 0.5],
    ),
    ph(
        "suculenta",
        "Suculenta",
        "potted_plant_04",
        PlantaVaso,
        TODOS,
        [0.2, 0.25, 0.2],
    ),
    ph(
        "vaso-ceramica-1",
        "Vaso de ceramica",
        "ceramic_vase_01",
        Vaso,
        AP_REC,
        [0.2, 0.3, 0.2],
    ),
    ph(
        "vaso-ceramica-2",
        "Vaso de ceramica alto",
        "ceramic_vase_03",
        Vaso,
        AP_REC,
        [0.22, 0.42, 0.22],
    ),
    ph(
        "quadro-1",
        "Quadro emoldurado",
        "hanging_picture_frame_01",
        Quadro,
        AP_REC,
        [0.6, 0.8, 0.04],
    ),
    ph(
        "quadro-2",
        "Quadro emoldurado (paisagem)",
        "hanging_picture_frame_02",
        Quadro,
        AP_REC,
        [0.9, 0.6, 0.04],
    ),
    ph(
        "espelho",
        "Espelho",
        "ornate_mirror_01",
        Espelho,
        AP,
        [0.7, 1.1, 0.05],
    ),
    ph(
        "relogio-parede",
        "Relogio de parede",
        "wall_clock",
        Relogio,
        AP_REC,
        [0.3, 0.3, 0.05],
    ),
    ph(
        "luminaria-teto",
        "Luminaria de teto",
        "modern_ceiling_lamp_01",
        LuminariaPendente,
        AP_REC,
        [0.4, 0.35, 0.4],
    ),
    ph(
        "pendente-cafe",
        "Pendente industrial",
        "hanging_industrial_lamp",
        LuminariaPendente,
        CAFE,
        [0.3, 0.45, 0.3],
    ),
    ph(
        "luminaria-mesa",
        "Luminaria de mesa",
        "desk_lamp_arm_01",
        Luminaria,
        AP,
        [0.4, 0.5, 0.2],
    ),
    // ---------------- recepcao ----------------
    par(
        "balcao-recepcao",
        "Balcao de recepcao",
        Peca::BalcaoRecepcao,
        BalcaoRecepcao,
        REC,
        [3.08, 1.10, 1.39],
    ),
    ph(
        "mesa-escritorio",
        "Mesa de escritorio",
        "metal_office_desk",
        BalcaoRecepcao,
        REC,
        [1.6, 0.75, 0.8],
    ),
    ph(
        "banco-modular",
        "Banco modular",
        "modular_street_seating",
        Banco,
        &[Recepcao, Rooftop],
        [1.8, 0.45, 0.6],
    ),
    // ---------------- cafeteria / loja ----------------
    ph(
        "balcao-cafe",
        "Balcao de cafe",
        "CoffeeCart_01",
        BalcaoCafe,
        CAFE,
        [1.4, 1.1, 0.7],
    ),
    ph(
        "caixa-registradora",
        "Caixa registradora",
        "CashRegister_01",
        Caixa,
        &[Cafeteria, Mercado],
        [0.4, 0.3, 0.35],
    ),
    ph(
        "prateleira-loja",
        "Prateleira de loja",
        "steel_frame_shelves_03",
        PrateleiraLoja,
        &[Mercado, Cafeteria],
        [1.2, 1.9, 0.45],
    ),
    // ---------------- rooftop / externo ----------------
    ph(
        "mesa-cadeiras-externa",
        "Conjunto mesa e cadeiras externo",
        "outdoor_table_chair_set_01",
        MesaExterna,
        ROOF,
        [1.6, 0.75, 1.0],
    ),
    ph(
        "banco-madeira",
        "Banco de madeira",
        "painted_wooden_bench",
        Banco,
        ROOF,
        [1.5, 0.85, 0.6],
    ),
    ph(
        "floreira-1",
        "Floreira",
        "planter_box_01",
        Floreira,
        &[Rooftop, Recepcao],
        [1.0, 0.5, 0.4],
    ),
    ph(
        "floreira-2",
        "Floreira redonda",
        "planter_box_02",
        Floreira,
        &[Rooftop, Recepcao],
        [0.7, 0.55, 0.7],
    ),
    ph(
        "floreira-3",
        "Floreira alta",
        "planter_box_03",
        Floreira,
        &[Rooftop, Recepcao],
        [0.6, 0.8, 0.6],
    ),
    par(
        "espreguicadeira",
        "Espreguicadeira",
        Peca::Espreguicadeira,
        Espreguicadeira,
        ROOF,
        [0.7, 0.75, 2.0],
    ),
    par(
        "guarda-sol",
        "Guarda-sol",
        Peca::GuardaSol,
        GuardaSol,
        ROOF,
        [2.6, 2.4, 2.6],
    ),
    par(
        "churrasqueira",
        "Bancada gourmet com churrasqueira",
        Peca::Churrasqueira,
        Churrasqueira,
        ROOF,
        [3.0, 1.1, 0.7],
    ),
    ph(
        "poltrona-lounge-externa",
        "Poltrona lounge externa",
        "GreenChair_01",
        Poltronalounge,
        AP_ROOF,
        [0.7, 0.8, 0.75],
    ),
];

/// Procura um item pela chave.
pub fn por_chave(chave: &str) -> Option<&'static Item> {
    CATALOGO.iter().find(|i| i.chave == chave)
}

/// Todos os itens que servem para um papel.
pub fn por_papel(papel: Papel) -> impl Iterator<Item = &'static Item> {
    CATALOGO.iter().filter(move |i| i.papel == papel)
}

/// Todos os itens usados num ambiente.
pub fn por_ambiente(amb: Ambiente) -> impl Iterator<Item = &'static Item> {
    CATALOGO.iter().filter(move |i| i.ambientes.contains(&amb))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn chaves_sao_unicas() {
        let mut vistas = HashSet::new();
        for item in CATALOGO {
            assert!(vistas.insert(item.chave), "chave duplicada: {}", item.chave);
        }
    }

    #[test]
    fn toda_licenca_e_comercialmente_livre() {
        for item in CATALOGO {
            assert!(
                item.licenca.comercialmente_livre(),
                "{} tem licenca que impede uso comercial",
                item.chave
            );
        }
    }

    #[test]
    fn dimensoes_sao_plausiveis() {
        // Nada com 0 m nem maior que um comodo inteiro: pega erro de digitacao
        // (0.9 virando 9.0) antes de a planta colocar um sofa de 9 metros na sala.
        for item in CATALOGO {
            for (eixo, v) in item.dimensao_m.iter().enumerate() {
                assert!(
                    *v > 0.005 && *v < 6.0,
                    "{}: dimensao[{eixo}] = {v} m fora do plausivel",
                    item.chave
                );
            }
        }
    }

    #[test]
    fn todo_item_declara_ao_menos_um_ambiente() {
        for item in CATALOGO {
            assert!(!item.ambientes.is_empty(), "{} sem ambiente", item.chave);
        }
    }

    #[test]
    fn chave_serve_de_nome_de_pasta() {
        // Sem espaco, sem acento, sem separador de caminho: a chave vira diretorio.
        for item in CATALOGO {
            assert!(
                item.chave
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "chave invalida como pasta: {}",
                item.chave
            );
        }
    }

    #[test]
    fn os_papeis_essenciais_do_apartamento_tem_item() {
        // Se algum desses ficar sem item, a planta nao consegue mobiliar a unidade.
        for papel in [
            CamaCasal,
            Criado,
            GuardaRoupa,
            SofaTresLugares,
            MesaCentro,
            MesaJantar,
            CadeiraJantar,
            BancadaCozinha,
            Geladeira,
            Tv,
            VasoSanitario,
            CubaBanheiro,
            BoxChuveiro,
        ] {
            assert!(
                por_papel(papel).next().is_some(),
                "nenhum item para o papel {papel:?}"
            );
        }
    }

    #[test]
    fn busca_por_chave_e_por_ambiente() {
        assert_eq!(por_chave("sofa-3-lugares").unwrap().papel, SofaTresLugares);
        assert!(por_chave("nao-existe").is_none());
        assert!(por_ambiente(Rooftop).count() >= 5);
    }

    #[test]
    fn itens_parametricos_nao_sao_remotos() {
        for item in CATALOGO {
            match item.fonte {
                Fonte::Parametrica(_) => assert!(!item.remoto()),
                Fonte::PolyHaven { slug } => {
                    assert!(item.remoto());
                    assert!(!slug.is_empty());
                }
            }
        }
    }
}

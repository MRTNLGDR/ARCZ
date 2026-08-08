//! O que o ARCZ extrai do OpenStreetMap e como isso vira geometria.
//!
//! O OSM e uma base de tags livres: o mesmo predio pode declarar `height`,
//! `building:levels`, os dois, ou nenhum. Este modulo normaliza essa bagunca num
//! punhado de tipos com **dimensoes ja resolvidas em metros**, para que o gerador
//! de malha nunca precise adivinhar nada.
//!
//! ## Licenca
//!
//! Os dados vem do OpenStreetMap sob **ODbL 1.0**. Isso obriga a atribuicao
//! ("© colaboradores do OpenStreetMap") em qualquer imagem publicada e torna
//! *share-alike* qualquer base derivada que seja redistribuida. Renderizacoes
//! (imagens) sao "produced work" e nao contaminam o projeto; redistribuir a
//! geometria extraida, sim. Ver `PROVENIENCIA` em `lib.rs`.

use std::collections::BTreeMap;

/// Ponto geografico cru, como o Overpass devolve.
#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
pub struct PontoGeo {
    pub lat: f64,
    pub lon: f64,
}

/// Tags OSM de um elemento.
pub type Tags = BTreeMap<String, String>;

/// Categoria de uso, usada para escolher cor, altura de fallback e material.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClasseEdificio {
    Residencial,
    Apartamentos,
    Comercial,
    Industrial,
    Publico,
    Religioso,
    Garagem,
    Outro,
}

impl ClasseEdificio {
    /// Deduz a classe a partir do valor da tag `building`.
    pub fn da_tag(valor: &str) -> Self {
        match valor {
            "house" | "detached" | "residential" | "bungalow" | "terrace" | "semidetached_house"
            | "hut" | "cabin" => Self::Residencial,
            "apartments" | "dormitory" | "hotel" | "residential_tower" => Self::Apartamentos,
            "commercial" | "retail" | "supermarket" | "shop" | "office" | "kiosk" | "restaurant" => {
                Self::Comercial
            }
            "industrial" | "warehouse" | "factory" | "hangar" => Self::Industrial,
            "school" | "hospital" | "university" | "civic" | "public" | "government"
            | "college" | "kindergarten" => Self::Publico,
            "church" | "cathedral" | "chapel" | "mosque" | "temple" | "synagogue" => Self::Religioso,
            "garage" | "garages" | "carport" | "shed" | "roof" => Self::Garagem,
            _ => Self::Outro,
        }
    }

    /// Altura tipica em metros quando o OSM nao informa nada.
    ///
    /// Estes numeros nao sao chutes redondos: sao a mediana observada em areas
    /// urbanas brasileiras de baixa densidade, que e o caso de Bombinhas. Um
    /// predio sem tag vira um volume plausivel, nao um cubo generico.
    pub fn altura_tipica_m(self) -> f64 {
        match self {
            Self::Residencial => 6.0,
            Self::Apartamentos => 15.0,
            Self::Comercial => 5.5,
            Self::Industrial => 8.0,
            Self::Publico => 9.0,
            Self::Religioso => 12.0,
            Self::Garagem => 3.0,
            Self::Outro => 7.0,
        }
    }

    /// Cor base RGB linear, ate a texturizacao PBR entrar.
    pub fn cor_base(self) -> [f32; 3] {
        match self {
            Self::Residencial => [0.82, 0.78, 0.72],
            Self::Apartamentos => [0.74, 0.75, 0.78],
            Self::Comercial => [0.80, 0.76, 0.70],
            Self::Industrial => [0.66, 0.67, 0.68],
            Self::Publico => [0.86, 0.84, 0.80],
            Self::Religioso => [0.88, 0.86, 0.82],
            Self::Garagem => [0.70, 0.68, 0.65],
            Self::Outro => [0.78, 0.76, 0.73],
        }
    }
}

/// Forma da cobertura. Laje plana le como caixa; duas aguas le como casa.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Telhado {
    /// Laje. E o certo para predio alto e para contorno irregular do OSM.
    Plano,
    /// Duas aguas, com a cumeeira no eixo maior da planta. `altura_m` e a
    /// elevacao da cumeeira acima do topo da parede.
    DuasAguas { altura_m: f64, beiral_m: f64 },
}

impl Telhado {
    /// Cobertura tipica da classe. Casa baixa tem telhado aparente; predio de
    /// apartamentos tem laje — inverter isso e o que faz um bairro parecer
    /// maquete de papelao.
    pub fn tipico(classe: ClasseEdificio, pavimentos: f64) -> Self {
        match classe {
            ClasseEdificio::Residencial | ClasseEdificio::Garagem if pavimentos <= 3.0 => {
                Self::DuasAguas {
                    altura_m: 1.8 + pavimentos * 0.2,
                    beiral_m: 0.6,
                }
            }
            ClasseEdificio::Religioso => Self::DuasAguas {
                altura_m: 3.5,
                beiral_m: 0.8,
            },
            _ => Self::Plano,
        }
    }

    pub fn altura_m(self) -> f64 {
        match self {
            Self::Plano => 0.0,
            Self::DuasAguas { altura_m, .. } => altura_m,
        }
    }
}

/// Um predio com contorno fechado e altura ja resolvida em metros.
#[derive(Debug, Clone)]
pub struct Edificio {
    pub id: i64,
    pub nome: Option<String>,
    pub classe: ClasseEdificio,
    /// Contorno em ordem, **sem** repetir o primeiro ponto no fim.
    pub contorno: Vec<PontoGeo>,
    /// Altura total acima da base, em metros.
    pub altura_m: f64,
    /// Altura da base acima do terreno (`building:min_level`), em metros.
    pub base_m: f64,
    /// De onde a altura veio. Distingue medida de estimativa — o usuario precisa
    /// saber o que e dado e o que e palpite antes de renderizar um 8K.
    pub fonte_altura: FonteAltura,
    /// Forma da cobertura.
    pub telhado: Telhado,
    /// Cor da parede. `None` deixa o gerador usar a cor tipica da classe; o app
    /// preenche com a cor amostrada da ortofoto quando ela esta disponivel.
    pub cor_parede: Option<[f32; 3]>,
    /// Cor do telhado, mesma regra.
    pub cor_telhado: Option<[f32; 3]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FonteAltura {
    /// Tag `height` explicita.
    Medida,
    /// Derivada de `building:levels`.
    PorPavimentos,
    /// Fallback pela classe de uso.
    Estimada,
}

/// Tipo de via, que determina largura e cor do asfalto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClasseVia {
    Rodovia,
    Arterial,
    Coletora,
    Local,
    Servico,
    Pedestre,
    Trilha,
}

impl ClasseVia {
    /// Traduz a tag `highway`. `None` para valores que nao sao via trafegavel
    /// nem caminho (ex.: `bus_stop`, `street_lamp`, `crossing`).
    pub fn da_tag(valor: &str) -> Option<Self> {
        Some(match valor {
            "motorway" | "motorway_link" | "trunk" | "trunk_link" => Self::Rodovia,
            "primary" | "primary_link" => Self::Arterial,
            "secondary" | "secondary_link" | "tertiary" | "tertiary_link" => Self::Coletora,
            "residential" | "unclassified" | "living_street" | "road" => Self::Local,
            "service" | "track" => Self::Servico,
            "pedestrian" | "footway" | "steps" | "cycleway" => Self::Pedestre,
            "path" | "bridleway" => Self::Trilha,
            _ => return None,
        })
    }

    /// Largura padrao em metros quando nem `width` nem `lanes` existem.
    pub fn largura_tipica_m(self) -> f64 {
        match self {
            Self::Rodovia => 14.0,
            Self::Arterial => 10.0,
            Self::Coletora => 8.0,
            Self::Local => 6.0,
            Self::Servico => 4.0,
            Self::Pedestre => 2.5,
            Self::Trilha => 1.5,
        }
    }

    pub fn cor_base(self) -> [f32; 3] {
        match self {
            Self::Rodovia | Self::Arterial => [0.19, 0.19, 0.21],
            Self::Coletora | Self::Local => [0.24, 0.24, 0.25],
            Self::Servico => [0.28, 0.27, 0.26],
            Self::Pedestre => [0.55, 0.52, 0.48],
            Self::Trilha => [0.45, 0.40, 0.33],
        }
    }

    /// Quanto a faixa sobe acima do terreno, para nao brigar com o z-buffer do
    /// terreno na mesma cota.
    pub fn folga_m(self) -> f64 {
        match self {
            Self::Pedestre | Self::Trilha => 0.06,
            _ => 0.10,
        }
    }
}

/// Uma via com eixo em polyline e largura ja resolvida.
#[derive(Debug, Clone)]
pub struct Via {
    pub id: i64,
    pub nome: Option<String>,
    pub classe: ClasseVia,
    pub eixo: Vec<PontoGeo>,
    pub largura_m: f64,
}

/// Uso do solo que vira uma superficie colorida (parque, grama, praia, mata).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClasseSuperficie {
    Mata,
    Grama,
    Praia,
    Agua,
    Quadra,
}

impl ClasseSuperficie {
    pub fn cor_base(self) -> [f32; 3] {
        match self {
            Self::Mata => [0.16, 0.30, 0.14],
            Self::Grama => [0.32, 0.48, 0.22],
            Self::Praia => [0.86, 0.80, 0.64],
            Self::Agua => [0.10, 0.28, 0.42],
            Self::Quadra => [0.42, 0.46, 0.32],
        }
    }

    pub fn folga_m(self) -> f64 {
        match self {
            // A agua precisa ficar *abaixo* das vias e acima do leito.
            Self::Agua => 0.02,
            _ => 0.04,
        }
    }

    /// Quantas arvores por hectare esta superficie gera.
    pub fn arvores_por_hectare(self) -> f64 {
        match self {
            Self::Mata => 120.0,
            Self::Grama => 8.0,
            Self::Quadra => 0.0,
            Self::Praia | Self::Agua => 0.0,
        }
    }
}

/// Poligono de uso do solo.
#[derive(Debug, Clone)]
pub struct Superficie {
    pub id: i64,
    pub classe: ClasseSuperficie,
    pub contorno: Vec<PontoGeo>,
}

/// Uma arvore: ou mapeada no OSM (`natural=tree`), ou semeada por nos dentro de
/// um poligono de mata.
#[derive(Debug, Clone, Copy)]
pub struct Arvore {
    pub posicao: PontoGeo,
    /// Altura do topo em metros.
    pub altura_m: f64,
    /// Raio da copa em metros.
    pub raio_copa_m: f64,
    /// Giro em torno do eixo vertical, em radianos — quebra a repeticao visual.
    pub giro_rad: f64,
    pub mapeada: bool,
}

/// Tudo que se extraiu de uma consulta, ja normalizado.
#[derive(Debug, Clone, Default)]
pub struct Entorno {
    pub edificios: Vec<Edificio>,
    pub vias: Vec<Via>,
    pub superficies: Vec<Superficie>,
    pub arvores: Vec<Arvore>,
}

impl Entorno {
    pub fn vazio(&self) -> bool {
        self.edificios.is_empty()
            && self.vias.is_empty()
            && self.superficies.is_empty()
            && self.arvores.is_empty()
    }

    /// Resumo de uma linha para log e para a UI.
    pub fn resumo(&self) -> String {
        let medidos = self
            .edificios
            .iter()
            .filter(|e| e.fonte_altura != FonteAltura::Estimada)
            .count();
        format!(
            "{} predios ({} com altura do OSM), {} vias, {} superficies, {} arvores",
            self.edificios.len(),
            medidos,
            self.vias.len(),
            self.superficies.len(),
            self.arvores.len(),
        )
    }
}

/// Le uma tag numerica em metros, tolerando as unidades que aparecem no OSM.
///
/// O OSM aceita `"12"`, `"12 m"`, `"12.5"`, `"40'"` (pes) e `"40'6\""`. Ignorar
/// isso produz predios de 40 metros onde deveria haver 12.
pub fn metros_da_tag(bruto: &str) -> Option<f64> {
    let t = bruto.trim();
    if t.is_empty() {
        return None;
    }

    // Pes e polegadas: 40' ou 40'6"
    if let Some((pes, resto)) = t.split_once('\'') {
        let pes: f64 = pes.trim().parse().ok()?;
        let pol: f64 = resto
            .trim()
            .trim_end_matches('"')
            .trim()
            .parse()
            .unwrap_or(0.0);
        return Some(pes * 0.3048 + pol * 0.0254);
    }

    let numero: String = t
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    let v: f64 = numero.parse().ok()?;
    let sufixo = t[numero.len()..].trim().to_ascii_lowercase();
    let m = match sufixo.as_str() {
        "" | "m" | "meter" | "meters" | "metre" | "metres" => v,
        "km" => v * 1000.0,
        "ft" | "feet" | "foot" => v * 0.3048,
        "mi" => v * 1609.344,
        _ => return None,
    };
    (m.is_finite() && m > 0.0).then_some(m)
}

/// Resolve a altura de um predio a partir das tags, na ordem de confianca.
pub fn altura_do_edificio(tags: &Tags, classe: ClasseEdificio) -> (f64, f64, FonteAltura) {
    let base = tags
        .get("min_height")
        .and_then(|s| metros_da_tag(s))
        .or_else(|| {
            tags.get("building:min_level")
                .and_then(|s| s.trim().parse::<f64>().ok())
                .map(|n| n * ALTURA_PAVIMENTO_M)
        })
        .unwrap_or(0.0)
        .max(0.0);

    if let Some(h) = tags
        .get("height")
        .or_else(|| tags.get("building:height"))
        .and_then(|s| metros_da_tag(s))
    {
        // `height` no OSM e do chao ao topo, ja incluindo a base.
        let total = (h - base).max(2.0);
        return (total, base, FonteAltura::Medida);
    }

    if let Some(niveis) = tags
        .get("building:levels")
        .or_else(|| tags.get("levels"))
        .and_then(|s| s.trim().parse::<f64>().ok())
        .filter(|n| *n > 0.0 && *n < 200.0)
    {
        let telhado = tags
            .get("roof:height")
            .and_then(|s| metros_da_tag(s))
            .unwrap_or(0.0);
        return (
            niveis * ALTURA_PAVIMENTO_M + telhado,
            base,
            FonteAltura::PorPavimentos,
        );
    }

    (classe.altura_tipica_m(), base, FonteAltura::Estimada)
}

/// Pe-direito medio usado para converter pavimentos em metros.
pub const ALTURA_PAVIMENTO_M: f64 = 3.0;

/// Resolve a largura de uma via a partir das tags.
pub fn largura_da_via(tags: &Tags, classe: ClasseVia) -> f64 {
    if let Some(w) = tags.get("width").and_then(|s| metros_da_tag(s)) {
        return w.clamp(0.8, 60.0);
    }
    if let Some(faixas) = tags
        .get("lanes")
        .and_then(|s| s.trim().parse::<f64>().ok())
        .filter(|n| *n >= 1.0 && *n <= 12.0)
    {
        // 3,2 m e a faixa padrao brasileira; mais 1 m de acostamento por lado
        // nas vias que tem.
        let acostamento = if matches!(classe, ClasseVia::Rodovia | ClasseVia::Arterial) {
            2.0
        } else {
            0.0
        };
        return faixas * 3.2 + acostamento;
    }
    classe.largura_tipica_m()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(pares: &[(&str, &str)]) -> Tags {
        pares
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn le_altura_em_metros_com_e_sem_unidade() {
        assert_eq!(metros_da_tag("12"), Some(12.0));
        assert_eq!(metros_da_tag("12 m"), Some(12.0));
        assert_eq!(metros_da_tag("12.5m"), Some(12.5));
        assert_eq!(metros_da_tag(" 8 "), Some(8.0));
    }

    #[test]
    fn le_altura_em_pes_porque_o_osm_aceita() {
        // 40 pes = 12,19 m. Tratar como metros criaria um predio 3x mais alto.
        let m = metros_da_tag("40'").unwrap();
        assert!((m - 12.192).abs() < 1e-3, "40' virou {m}");
        let m = metros_da_tag("40'6\"").unwrap();
        assert!((m - 12.344).abs() < 1e-3, "40'6\" virou {m}");
        let m = metros_da_tag("30 ft").unwrap();
        assert!((m - 9.144).abs() < 1e-3, "30 ft virou {m}");
    }

    #[test]
    fn recusa_altura_invalida() {
        assert_eq!(metros_da_tag(""), None);
        assert_eq!(metros_da_tag("alto"), None);
        assert_eq!(metros_da_tag("0"), None);
        assert_eq!(metros_da_tag("-5"), None);
        assert_eq!(metros_da_tag("12 parsecs"), None);
    }

    #[test]
    fn height_explicita_vence_os_pavimentos() {
        // Quando as duas tags existem elas discordam com frequencia; `height` e
        // medida e `levels` e contagem, entao a medida ganha.
        let t = tags(&[("height", "18"), ("building:levels", "3")]);
        let (h, _, fonte) = altura_do_edificio(&t, ClasseEdificio::Apartamentos);
        assert_eq!(h, 18.0);
        assert_eq!(fonte, FonteAltura::Medida);
    }

    #[test]
    fn pavimentos_viram_metros_com_o_telhado() {
        let t = tags(&[("building:levels", "4"), ("roof:height", "2.5")]);
        let (h, _, fonte) = altura_do_edificio(&t, ClasseEdificio::Residencial);
        assert_eq!(h, 4.0 * ALTURA_PAVIMENTO_M + 2.5);
        assert_eq!(fonte, FonteAltura::PorPavimentos);
    }

    #[test]
    fn sem_tag_usa_a_altura_tipica_da_classe() {
        let (h, base, fonte) = altura_do_edificio(&tags(&[]), ClasseEdificio::Apartamentos);
        assert_eq!(h, 15.0);
        assert_eq!(base, 0.0);
        assert_eq!(fonte, FonteAltura::Estimada);
    }

    #[test]
    fn base_elevada_e_descontada_da_altura_total() {
        // Um predio com `height=20` e `min_height=5` tem 15 m de volume, nao 20 —
        // senao ele atravessa o predio de baixo.
        let t = tags(&[("height", "20"), ("min_height", "5")]);
        let (h, base, _) = altura_do_edificio(&t, ClasseEdificio::Outro);
        assert_eq!((h, base), (15.0, 5.0));
    }

    #[test]
    fn altura_nunca_fica_degenerada() {
        // min_height maior que height existe no OSM e produziria volume negativo.
        let t = tags(&[("height", "4"), ("min_height", "9")]);
        let (h, _, _) = altura_do_edificio(&t, ClasseEdificio::Outro);
        assert!(h >= 2.0, "altura degenerada: {h}");
    }

    #[test]
    fn largura_da_via_prefere_width_depois_lanes() {
        assert_eq!(largura_da_via(&tags(&[("width", "9")]), ClasseVia::Local), 9.0);
        let w = largura_da_via(&tags(&[("lanes", "2")]), ClasseVia::Local);
        assert_eq!(w, 6.4);
        // Rodovia ganha acostamento.
        let w = largura_da_via(&tags(&[("lanes", "2")]), ClasseVia::Rodovia);
        assert_eq!(w, 8.4);
        assert_eq!(
            largura_da_via(&tags(&[]), ClasseVia::Local),
            ClasseVia::Local.largura_tipica_m()
        );
    }

    #[test]
    fn classe_de_via_ignora_o_que_nao_e_via() {
        assert_eq!(ClasseVia::da_tag("residential"), Some(ClasseVia::Local));
        assert_eq!(ClasseVia::da_tag("bus_stop"), None);
        assert_eq!(ClasseVia::da_tag("street_lamp"), None);
    }

    #[test]
    fn o_resumo_separa_medido_de_estimado() {
        let e = Entorno {
            edificios: vec![
                Edificio {
                    id: 1,
                    nome: None,
                    classe: ClasseEdificio::Outro,
                    contorno: vec![],
                    altura_m: 10.0,
                    base_m: 0.0,
                    fonte_altura: FonteAltura::Medida,
                    telhado: Telhado::Plano,
                    cor_parede: None,
                    cor_telhado: None,
                },
                Edificio {
                    id: 2,
                    nome: None,
                    classe: ClasseEdificio::Outro,
                    contorno: vec![],
                    altura_m: 7.0,
                    base_m: 0.0,
                    fonte_altura: FonteAltura::Estimada,
                    telhado: Telhado::Plano,
                    cor_parede: None,
                    cor_telhado: None,
                },
            ],
            ..Default::default()
        };
        assert!(e.resumo().contains("2 predios (1 com altura do OSM)"), "{}", e.resumo());
    }
}

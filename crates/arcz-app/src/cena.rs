//! Cena editavel: varios objetos, selecao por clique e transformacao por gizmo.
//!
//! Substitui o modelo unico e fixo das fatias anteriores. Cada objeto guarda a
//! geometria original mais um [`Placement`]; mover, girar ou escalar so troca o
//! placement, e a geometria e reprocessada apenas para aquele objeto.
//!
//! **Estado:** o modulo esta coberto por testes (picking por raio, hierarquia
//! pai/filho, ciclo de vida, undo/redo, varredura da biblioteca). Parte da API
//! ainda nao tem chamador fora dos testes — o `Historico` e os `Comando` existem
//! e funcionam, mas ainda nao ha botao de desfazer na interface web; o viewport
//! nativo ja os usa. O `allow` abaixo evita que `clippy -D warnings` barre o
//! build por isso, e deve sair quando a UI consumir tudo.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use arcz_geo::EnuFrame;
use arcz_model::{FonteGeometria, Material, Model, Placement, Submesh, Textura};

use crate::projeto;

/// Identificador estavel de um objeto na cena.
pub type ObjetoId = u32;

/// Identificador canonico em String para o Scene Graph autoritativo.
pub type NodeId = String;

/// Tipos suportados no registro autoritativo de nos da cena.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum NodeType {
    Root,
    Building,
    Site,
    Parcel,
    Road,
    Terrain,
    Vegetation,
    Water,
    Furniture,
    Light,
    Camera,
    Vehicle,
    PointCloud,
    GaussianSplat,
    RealityMesh,
    Panorama,
    GisObject,
    CadObject,
    BimObject,
    GenericModel,
}

impl NodeType {
    /// Inverso de `format!("{:?}")`, que e como o tipo vai para o SQLite.
    ///
    /// Sem isto a leitura devolvia `Building` para tudo, e um terreno voltava
    /// do banco como edificacao — silenciosamente, porque o campo era lido e
    /// descartado. Valor desconhecido cai em `GenericModel` em vez de perder o
    /// no: um tipo novo gravado por versao mais nova nao pode apagar dados.
    pub fn do_texto(s: &str) -> Self {
        match s {
            "Root" => Self::Root,
            "Building" => Self::Building,
            "Site" => Self::Site,
            "Parcel" => Self::Parcel,
            "Road" => Self::Road,
            "Terrain" => Self::Terrain,
            "Vegetation" => Self::Vegetation,
            "Water" => Self::Water,
            "Furniture" => Self::Furniture,
            "Light" => Self::Light,
            "Camera" => Self::Camera,
            "Vehicle" => Self::Vehicle,
            "PointCloud" => Self::PointCloud,
            "GaussianSplat" => Self::GaussianSplat,
            "RealityMesh" => Self::RealityMesh,
            "Panorama" => Self::Panorama,
            "GisObject" => Self::GisObject,
            "CadObject" => Self::CadObject,
            "BimObject" => Self::BimObject,
            _ => Self::GenericModel,
        }
    }
}

/// Matriz de cores e niveis de confianca da reconstrucao (GREEN / BLUE / YELLOW / RED).
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq)]
pub enum NodeConfidence {
    Observed,      // Green - 1.0
    GisDerived,    // Blue - 0.8
    Reconstructed, // Yellow - 0.6
    Inferred,      // Red - 0.3
}

impl NodeConfidence {
    pub fn value(&self) -> f64 {
        match self {
            Self::Observed => 1.0,
            Self::GisDerived => 0.8,
            Self::Reconstructed => 0.6,
            Self::Inferred => 0.3,
        }
    }

    pub fn color_code(&self) -> &'static str {
        match self {
            Self::Observed => "GREEN",
            Self::GisDerived => "BLUE",
            Self::Reconstructed => "YELLOW",
            Self::Inferred => "RED",
        }
    }

    pub fn from_value(val: f64) -> Self {
        if val >= 0.9 {
            Self::Observed
        } else if val >= 0.7 {
            Self::GisDerived
        } else if val >= 0.5 {
            Self::Reconstructed
        } else {
            Self::Inferred
        }
    }
}

/// Transformacao 64-bit no espaco local/mundo para evitar jitter no renderer.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct Transform64 {
    pub position: [f64; 3],
    pub rotation: [f64; 4],
    pub scale: [f64; 3],
}

impl Default for Transform64 {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        }
    }
}

impl Transform64 {
    pub fn to_renderer_f32(&self) -> ([f32; 3], [f32; 4], [f32; 3]) {
        (
            [
                self.position[0] as f32,
                self.position[1] as f32,
                self.position[2] as f32,
            ],
            [
                self.rotation[0] as f32,
                self.rotation[1] as f32,
                self.rotation[2] as f32,
                self.rotation[3] as f32,
            ],
            [
                self.scale[0] as f32,
                self.scale[1] as f32,
                self.scale[2] as f32,
            ],
        )
    }
}

/// Georreferenciamento WGS84/ENU em f64.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct Georeference64 {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: f64,
    pub heading: f64,
}

/// No autoritativo do Scene Graph do ARCZ.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SceneNode {
    pub id: NodeId,
    pub parent_id: Option<NodeId>,
    pub node_type: NodeType,
    pub name: String,
    pub transform: Transform64,
    pub georeference: Option<Georeference64>,
    pub visibility: bool,
    pub locked: bool,
    pub selectable: bool,
    pub layer: String,
    pub material_refs: Vec<String>,
    pub asset_ref: Option<String>,
    pub source: String,
    pub confidence: NodeConfidence,
    pub created_at: String,
    pub updated_at: String,
    pub revision: u64,
    pub metadata: serde_json::Value,
}

impl SceneNode {
    pub fn novo(id: impl Into<String>, name: impl Into<String>, node_type: NodeType) -> Self {
        Self {
            id: id.into(),
            parent_id: None,
            node_type,
            name: name.into(),
            transform: Transform64::default(),
            georeference: None,
            visibility: true,
            locked: false,
            selectable: true,
            layer: "default".to_string(),
            material_refs: Vec::new(),
            asset_ref: None,
            source: "observed".to_string(),
            confidence: NodeConfidence::Observed,
            created_at: "2026-07-30T00:00:00Z".to_string(),
            updated_at: "2026-07-30T00:00:00Z".to_string(),
            revision: 1,
            metadata: serde_json::Value::Object(serde_json::Map::new()),
        }
    }

    /// Nó autoritativo a partir da posição atual de um modelo importado.
    ///
    /// A georreferência guarda lat/lon/altitude/rumo em `f64`, e o `transform`
    /// guarda o ajuste fino local em metros. São coisas diferentes de propósito:
    /// somar o offset na latitude perderia precisão no mesmo lugar onde o resto
    /// do projeto trabalha para não perder.
    pub fn do_placement(id: impl Into<String>, p: &arcz_model::Placement) -> Self {
        let mut no = Self::novo(id, "Modelo", NodeType::Building);
        no.georeference = Some(Georeference64 {
            latitude: p.lat_deg,
            longitude: p.lon_deg,
            altitude: p.offset_vertical_m as f64,
            heading: p.heading_deg,
        });
        no.transform = Transform64 {
            position: [
                p.offset_leste_m as f64,
                p.offset_vertical_m as f64,
                -(p.offset_norte_m as f64),
            ],
            rotation: quaternion_de_rumo(p.heading_deg),
            scale: [p.escala as f64; 3],
        };
        no
    }
}

impl SceneNode {
    /// Reconstitui a posição a partir do nó gravado. Inverso de `do_placement`.
    ///
    /// Devolve `None` quando o nó não tem georreferência: sem lat/lon não há
    /// onde colocá-lo, e inventar o centro da área seria pior que ignorar.
    pub fn para_placement(&self) -> Option<arcz_model::Placement> {
        let g = self.georeference.as_ref()?;
        Some(arcz_model::Placement {
            lat_deg: g.latitude,
            lon_deg: g.longitude,
            heading_deg: g.heading,
            escala: self.transform.scale[0] as f32,
            offset_leste_m: self.transform.position[0] as f32,
            // O eixo Z do render aponta para o sul; o offset norte é o negativo.
            offset_norte_m: -self.transform.position[2] as f32,
            offset_vertical_m: g.altitude as f32,
            // Quem gravou já tinha assentado; reassentar moveria o modelo na
            // reabertura, que é exatamente o que a persistência deve evitar.
            assentar_no_terreno: false,
        })
    }
}

/// Quaternion de uma rotação de `graus` em torno do eixo vertical.
///
/// O rumo do ARCZ é horário a partir do norte; o quaternion gira em torno de
/// +Y no espaço de render, onde -Z aponta para o norte. Daí o sinal negativo.
fn quaternion_de_rumo(graus: f64) -> [f64; 4] {
    let meio = -graus.to_radians() * 0.5;
    [0.0, meio.sin(), 0.0, meio.cos()]
}

/// Um nó da árvore do Outliner pronto para renderização na UI.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct OutlinerNode {
    pub id: NodeId,
    pub parent_id: Option<NodeId>,
    pub name: String,
    pub node_type: NodeType,
    pub confidence_color: String,
    pub confidence_value: f64,
    pub visibility: bool,
    pub locked: bool,
    pub selectable: bool,
    pub children: Vec<OutlinerNode>,
}

pub struct OutlinerService;

impl OutlinerService {
    /// Constrói a árvore hierárquica completa de OutlinerNode a partir dos SceneNodes autoritativos.
    pub fn construir_arvore(nodes: &[SceneNode]) -> Vec<OutlinerNode> {
        fn construir_filhos(parent_id: Option<&str>, nodes: &[SceneNode]) -> Vec<OutlinerNode> {
            nodes
                .iter()
                .filter(|n| n.parent_id.as_deref() == parent_id)
                .map(|n| OutlinerNode {
                    id: n.id.clone(),
                    parent_id: n.parent_id.clone(),
                    name: n.name.clone(),
                    node_type: n.node_type,
                    confidence_color: n.confidence.color_code().to_string(),
                    confidence_value: n.confidence.value(),
                    visibility: n.visibility,
                    locked: n.locked,
                    selectable: n.selectable,
                    children: construir_filhos(Some(&n.id), nodes),
                })
                .collect()
        }

        construir_filhos(None, nodes)
    }

    /// Valida reparentamento sem criar ciclo na hierarquia (retorna true se valido).
    pub fn validar_reparentamento(
        node_id: &str,
        novo_pai_id: Option<&str>,
        nodes: &[SceneNode],
    ) -> bool {
        let Some(novo_pai) = novo_pai_id else {
            return true;
        };
        if node_id == novo_pai {
            return false;
        }

        let mut atual = Some(novo_pai.to_string());
        while let Some(pid) = atual {
            if pid == node_id {
                return false; // Ciclo detectado!
            }
            atual = nodes
                .iter()
                .find(|n| n.id == pid)
                .and_then(|n| n.parent_id.clone());
        }
        true
    }
}

/// Payload completo de propriedades editaveis do painel Inspector.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct InspectorPayload {
    pub id: NodeId,
    pub name: String,
    pub layer: String,
    pub node_type: NodeType,
    pub position: [f64; 3],
    pub rotation: [f64; 4],
    pub scale: [f64; 3],
    pub georeference: Option<Georeference64>,
    pub visibility: bool,
    pub locked: bool,
    pub selectable: bool,
    pub confidence_color: String,
    pub confidence_value: f64,
    pub material_refs: Vec<String>,
    pub metadata: serde_json::Value,
}

pub struct InspectorService;

impl InspectorService {
    /// Extrai o payload de inspeção a partir do nó autoritativo da cena.
    pub fn extrair_payload(node: &SceneNode) -> InspectorPayload {
        InspectorPayload {
            id: node.id.clone(),
            name: node.name.clone(),
            layer: node.layer.clone(),
            node_type: node.node_type,
            position: node.transform.position,
            rotation: node.transform.rotation,
            scale: node.transform.scale,
            georeference: node.georeference.clone(),
            visibility: node.visibility,
            locked: node.locked,
            selectable: node.selectable,
            confidence_color: node.confidence.color_code().to_string(),
            confidence_value: node.confidence.value(),
            material_refs: node.material_refs.clone(),
            metadata: node.metadata.clone(),
        }
    }

    /// Aplica as mutações editadas no Inspector roteando através do CommandBus para registro de Undo/Redo e Journal.
    pub fn aplicar_edicao(
        bus: &mut CommandBus,
        editor: &mut Editor,
        node: &mut SceneNode,
        novo_payload: InspectorPayload,
    ) {
        if node.name != novo_payload.name {
            let cmd = Comando::RenomearNode {
                id: node.id.clone(),
                antes: node.name.clone(),
                depois: novo_payload.name.clone(),
            };
            bus.executar(cmd, editor);
            node.name = novo_payload.name;
        }

        if node.transform.position != novo_payload.position
            || node.transform.rotation != novo_payload.rotation
            || node.transform.scale != novo_payload.scale
        {
            let antes = node.transform.clone();
            let depois = Transform64 {
                position: novo_payload.position,
                rotation: novo_payload.rotation,
                scale: novo_payload.scale,
            };
            let cmd = Comando::TransformarNode {
                id: node.id.clone(),
                antes,
                depois: depois.clone(),
            };
            bus.executar(cmd, editor);
            node.transform = depois;
        }

        if node.visibility != novo_payload.visibility {
            let cmd = Comando::VisibilidadeNode {
                id: node.id.clone(),
                visivel: novo_payload.visibility,
            };
            bus.executar(cmd, editor);
            node.visibility = novo_payload.visibility;
        }

        if node.material_refs != novo_payload.material_refs {
            let cmd = Comando::MaterialNode {
                id: node.id.clone(),
                materiais_anteriores: node.material_refs.clone(),
                materiais_novos: novo_payload.material_refs.clone(),
            };
            bus.executar(cmd, editor);
            node.material_refs = novo_payload.material_refs;
        }

        node.layer = novo_payload.layer;
        node.georeference = novo_payload.georeference;
        node.locked = novo_payload.locked;
        node.selectable = novo_payload.selectable;
        node.metadata = novo_payload.metadata;
    }
}

/// Historico de comandos para undo/redo.
///
/// Cada `Comando` representa uma mutacao no `Editor` (mover, rotacionar, etc.)
/// com `aplicar` e `reverter` definidos. O historico empilha os comandos
/// executados e, no `Ctrl+Z`, chama `reverter` do topo e move pra pilha de
/// redo. `Ctrl+Shift+Z` faz o contrario.
///
/// Limitacao da Fase 2: comandos cobrem as operacoes base (mover, deletar,
/// adicionar). Operacoes mais complexas (girar, escalar, hierarquia) serao
/// adicionadas quando essas features forem entregues.
#[derive(Default)]
pub struct Historico {
    feitos: Vec<Comando>,
    refeitos: Vec<Comando>,
}

impl Historico {
    pub fn novo() -> Self {
        Self::default()
    }

    /// Executa o comando no editor e empilha no historico. Limpa a pilha de
    /// redo (fazer uma nova acao descarta o que estava desfeito).
    pub fn executar(&mut self, mut cmd: Comando, editor: &mut Editor) {
        cmd.aplicar(editor);
        self.feitos.push(cmd);
        self.refeitos.clear();
    }

    /// Desfaz o ultimo comando. Devolve true se algo foi desfeito.
    pub fn desfazer(&mut self, editor: &mut Editor) -> bool {
        let Some(mut cmd) = self.feitos.pop() else {
            return false;
        };
        cmd.reverter(editor);
        self.refeitos.push(cmd);
        true
    }

    /// Refaz o ultimo comando desfeito. Devolve true se algo foi refeito.
    pub fn refazer(&mut self, editor: &mut Editor) -> bool {
        let Some(mut cmd) = self.refeitos.pop() else {
            return false;
        };
        cmd.aplicar(editor);
        self.feitos.push(cmd);
        true
    }

    pub fn tamanho_feitos(&self) -> usize {
        self.feitos.len()
    }
    pub fn tamanho_refeitos(&self) -> usize {
        self.refeitos.len()
    }
}

/// Registro de evento persistente para o journal de comandos.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct JournalEntry {
    pub sequence_id: u64,
    pub timestamp: String,
    pub command_name: String,
    pub payload_json: serde_json::Value,
}

/// Barramento de comandos autoritativos e transacionais.
pub struct CommandBus {
    pub historico: Historico,
    pub journal: Vec<JournalEntry>,
    pub sequence_counter: u64,
    pub preview_transform: Option<(ObjetoId, Placement)>,
}

impl Default for CommandBus {
    fn default() -> Self {
        Self {
            historico: Historico::novo(),
            journal: Vec::new(),
            sequence_counter: 0,
            preview_transform: None,
        }
    }
}

impl CommandBus {
    pub fn novo() -> Self {
        Self::default()
    }

    /// Executa um comando tipado, registra no journal e empilha no historico.
    pub fn executar(&mut self, mut cmd: Comando, editor: &mut Editor) {
        cmd.aplicar(editor);
        self.sequence_counter += 1;
        self.journal.push(JournalEntry {
            sequence_id: self.sequence_counter,
            timestamp: "2026-07-30T00:00:00Z".to_string(),
            command_name: cmd.nome(),
            payload_json: cmd.payload(),
        });
        self.historico.executar(cmd, editor);
    }

    /// Inicia arraste contínuo no gizmo sem comprometer transação no histórico.
    pub fn iniciar_arraste_preview(&mut self, id: ObjetoId, placement_inicial: Placement) {
        self.preview_transform = Some((id, placement_inicial));
    }

    /// Atualiza apenas o estado visual de preview durante o movimento do mouse.
    pub fn atualizar_arraste_preview(&self, editor: &mut Editor, placement_temporario: Placement) {
        if let Some((id, _)) = self.preview_transform {
            if let Some(obj) = editor.get_mut(id) {
                obj.placement = placement_temporario;
            }
        }
    }

    /// Finaliza o arraste do gizmo e compromete UMA ÚNICA transação no histórico.
    pub fn finalizar_arraste_comprometer(
        &mut self,
        editor: &mut Editor,
        placement_final: Placement,
    ) {
        if let Some((id, placement_inicial)) = self.preview_transform.take() {
            if placement_inicial != placement_final {
                let cmd = Comando::Mover {
                    id,
                    antes: placement_inicial,
                    depois: placement_final,
                };
                self.executar(cmd, editor);
            }
        }
    }

    pub fn desfazer(&mut self, editor: &mut Editor) -> bool {
        self.historico.desfazer(editor)
    }

    pub fn refazer(&mut self, editor: &mut Editor) -> bool {
        self.historico.refazer(editor)
    }
}

/// Um comando reversivel no `Editor`. Cada variante tem `aplicar` e `reverter`
/// simetricos — desfazer de um aplicar devolve o estado original.
pub enum Comando {
    /// Moveu um objeto (mudou o placement). Guarda o anterior pra desfazer.
    Mover {
        id: ObjetoId,
        antes: Placement,
        depois: Placement,
    },
    /// Adicionou um objeto (com nome, Model carregado, caminho). O reverter
    /// recria o objeto e remove.
    Adicionar {
        id: ObjetoId,
        nome: String,
        caminho: PathBuf,
        placement: Placement,
        visivel: bool,
    },
    /// Removeu um objeto. O reverter recria com o mesmo id.
    Remover { objeto: Objeto },
    /// Criacao transacional de no autoritativo.
    CriarNode { node: SceneNode },
    /// Remocao de no autoritativo.
    RemoverNode { node: SceneNode },
    /// Transformacao de no autoritativo.
    TransformarNode {
        id: NodeId,
        antes: Transform64,
        depois: Transform64,
    },
    /// Renomear no autoritativo.
    RenomearNode {
        id: NodeId,
        antes: String,
        depois: String,
    },
    /// Reagrupar no (mudança de pai).
    ReagruparNode {
        id: NodeId,
        pai_anterior: Option<NodeId>,
        pai_novo: Option<NodeId>,
    },
    /// Visibilidade do no.
    VisibilidadeNode { id: NodeId, visivel: bool },
    /// Alteracao de materiais.
    MaterialNode {
        id: NodeId,
        materiais_anteriores: Vec<String>,
        materiais_novos: Vec<String>,
    },
}

impl Comando {
    pub fn nome(&self) -> String {
        match self {
            Comando::Mover { .. } => "Mover".to_string(),
            Comando::Adicionar { .. } => "Adicionar".to_string(),
            Comando::Remover { .. } => "Remover".to_string(),
            Comando::CriarNode { .. } => "CriarNode".to_string(),
            Comando::RemoverNode { .. } => "RemoverNode".to_string(),
            Comando::TransformarNode { .. } => "TransformarNode".to_string(),
            Comando::RenomearNode { .. } => "RenomearNode".to_string(),
            Comando::ReagruparNode { .. } => "ReagruparNode".to_string(),
            Comando::VisibilidadeNode { .. } => "VisibilidadeNode".to_string(),
            Comando::MaterialNode { .. } => "MaterialNode".to_string(),
        }
    }

    pub fn payload(&self) -> serde_json::Value {
        match self {
            Comando::Mover { id, antes, depois } => {
                serde_json::json!({
                    "id": id,
                    "antes": { "leste": antes.offset_leste_m, "norte": antes.offset_norte_m, "escala": antes.escala },
                    "depois": { "leste": depois.offset_leste_m, "norte": depois.offset_norte_m, "escala": depois.escala }
                })
            }
            Comando::Adicionar {
                id,
                nome,
                caminho,
                placement,
                visivel,
            } => {
                serde_json::json!({
                    "id": id, "nome": nome, "caminho": caminho,
                    "placement": { "leste": placement.offset_leste_m, "norte": placement.offset_norte_m, "escala": placement.escala },
                    "visivel": visivel
                })
            }
            Comando::Remover { objeto } => {
                serde_json::json!({ "id": objeto.id, "nome": objeto.nome })
            }
            Comando::CriarNode { node } => serde_json::to_value(node).unwrap_or_default(),
            Comando::RemoverNode { node } => serde_json::json!({ "id": node.id }),
            Comando::TransformarNode { id, antes, depois } => {
                serde_json::json!({ "id": id, "antes": antes, "depois": depois })
            }
            Comando::RenomearNode { id, antes, depois } => {
                serde_json::json!({ "id": id, "antes": antes, "depois": depois })
            }
            Comando::ReagruparNode {
                id,
                pai_anterior,
                pai_novo,
            } => {
                serde_json::json!({ "id": id, "pai_anterior": pai_anterior, "pai_novo": pai_novo })
            }
            Comando::VisibilidadeNode { id, visivel } => {
                serde_json::json!({ "id": id, "visivel": visivel })
            }
            Comando::MaterialNode {
                id,
                materiais_anteriores,
                materiais_novos,
            } => {
                serde_json::json!({ "id": id, "materiais_anteriores": materiais_anteriores, "materiais_novos": materiais_novos })
            }
        }
    }

    pub fn aplicar(&mut self, editor: &mut Editor) {
        match self {
            Comando::Mover { id, depois, .. } => {
                if let Some(o) = editor.get_mut(*id) {
                    o.placement = *depois;
                }
            }
            Comando::Adicionar {
                id,
                nome,
                caminho,
                placement,
                visivel,
            } => {
                let caminho = caminho.clone();
                if let Ok(model) = arcz_model::Model::load(&caminho) {
                    if let Some(new_id) =
                        editor.adicionar_com_arquivo(nome.clone(), model, *placement, None, caminho)
                    {
                        if let Some(o) = editor.get_mut(new_id) {
                            o.visivel = *visivel;
                            o.id = *id;
                        }
                    }
                }
            }
            Comando::Remover { objeto } => {
                editor.remover(objeto.id);
            }
            Comando::CriarNode { .. } | Comando::RemoverNode { .. } => {}
            Comando::TransformarNode { id, depois, .. } => {
                if let Ok(num_id) = id.parse::<u32>() {
                    if let Some(o) = editor.get_mut(num_id) {
                        o.placement.offset_leste_m = depois.position[0] as f32;
                        o.placement.offset_norte_m = depois.position[1] as f32;
                        o.placement.offset_vertical_m = depois.position[2] as f32;
                    }
                }
            }
            Comando::RenomearNode { id, depois, .. } => {
                if let Ok(num_id) = id.parse::<u32>() {
                    if let Some(o) = editor.get_mut(num_id) {
                        o.nome = depois.clone();
                    }
                }
            }
            Comando::ReagruparNode { id, pai_novo, .. } => {
                if let Ok(num_id) = id.parse::<u32>() {
                    let pai_num = pai_novo.as_ref().and_then(|p| p.parse::<u32>().ok());
                    if let Some(o) = editor.get_mut(num_id) {
                        o.pai = pai_num;
                    }
                }
            }
            Comando::VisibilidadeNode { id, visivel } => {
                if let Ok(num_id) = id.parse::<u32>() {
                    if let Some(o) = editor.get_mut(num_id) {
                        o.visivel = *visivel;
                    }
                }
            }
            Comando::MaterialNode { .. } => {}
        }
    }

    pub fn reverter(&mut self, editor: &mut Editor) {
        match self {
            Comando::Mover { id, antes, .. } => {
                if let Some(o) = editor.get_mut(*id) {
                    o.placement = *antes;
                }
            }
            Comando::Adicionar { id, .. } => {
                editor.remover(*id);
            }
            Comando::Remover { objeto } => {
                if let Ok(model) = arcz_model::Model::load(&objeto.arquivo) {
                    if let Some(new_id) = editor.adicionar_com_arquivo(
                        objeto.nome.clone(),
                        model,
                        objeto.placement,
                        objeto.pai,
                        objeto.arquivo.clone(),
                    ) {
                        if let Some(o) = editor.get_mut(new_id) {
                            o.visivel = objeto.visivel;
                            o.id = objeto.id;
                        }
                    }
                }
            }
            Comando::CriarNode { .. } | Comando::RemoverNode { .. } => {}
            Comando::TransformarNode { id, antes, .. } => {
                if let Ok(num_id) = id.parse::<u32>() {
                    if let Some(o) = editor.get_mut(num_id) {
                        o.placement.offset_leste_m = antes.position[0] as f32;
                        o.placement.offset_norte_m = antes.position[1] as f32;
                        o.placement.offset_vertical_m = antes.position[2] as f32;
                    }
                }
            }
            Comando::RenomearNode { id, antes, .. } => {
                if let Ok(num_id) = id.parse::<u32>() {
                    if let Some(o) = editor.get_mut(num_id) {
                        o.nome = antes.clone();
                    }
                }
            }
            Comando::ReagruparNode {
                id, pai_anterior, ..
            } => {
                if let Ok(num_id) = id.parse::<u32>() {
                    let pai_num = pai_anterior.as_ref().and_then(|p| p.parse::<u32>().ok());
                    if let Some(o) = editor.get_mut(num_id) {
                        o.pai = pai_num;
                    }
                }
            }
            Comando::VisibilidadeNode { id, visivel } => {
                if let Ok(num_id) = id.parse::<u32>() {
                    if let Some(o) = editor.get_mut(num_id) {
                        o.visivel = !*visivel;
                    }
                }
            }
            Comando::MaterialNode { .. } => {}
        }
    }
}

/// Estado universal de selecao da cena, sincronizado com o Outliner e Inspector.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SelectionState {
    pub selected_node_id: Option<NodeId>,
    pub selected_objeto_id: Option<ObjetoId>,
    pub highlight_outline: bool,
    pub outliner_sync: bool,
    pub inspector_sync: bool,
}

impl SelectionState {
    pub fn selecionar_objeto(&mut self, id: ObjetoId) {
        self.selected_objeto_id = Some(id);
        self.selected_node_id = Some(id.to_string());
        self.highlight_outline = true;
        self.outliner_sync = true;
        self.inspector_sync = true;
    }

    pub fn selecionar_node(&mut self, node_id: NodeId) {
        self.selected_node_id = Some(node_id.clone());
        self.selected_objeto_id = node_id.parse::<u32>().ok();
        self.highlight_outline = true;
        self.outliner_sync = true;
        self.inspector_sync = true;
    }

    pub fn limpar(&mut self) {
        self.selected_node_id = None;
        self.selected_objeto_id = None;
        self.highlight_outline = false;
        self.outliner_sync = true;
        self.inspector_sync = true;
    }
}

/// Um objeto colocavel e editavel.
#[derive(Clone)]
pub struct Objeto {
    pub id: ObjetoId,
    pub nome: String,
    /// `Some(id_pai)` se o objeto faz parte de um grupo. A transformacao do pai
    /// e propagada: o eixo do objeto eh o do mundo (placement do pai) e o offset
    /// local dele se aplica depois. `None` = raiz.
    pub pai: Option<ObjetoId>,
    /// Vertices no espaco do arquivo. A posicao no mundo vem do `placement`.
    pub fonte: FonteGeometria,
    pub indices: Vec<u32>,
    pub submeshes: Vec<Submesh>,
    pub materiais: Vec<Material>,
    pub texturas: Vec<Textura>,
    pub placement: Placement,
    pub visivel: bool,
    pub selecionavel: bool,
    pub bloqueado: bool,
    /// Caminho do arquivo de origem. Usado pelo save/load (.arcz).
    pub arquivo: PathBuf,
    /// Caixa envolvente em coordenadas de render, recalculada a cada transformacao.
    pub min_enu: [f32; 3],
    pub max_enu: [f32; 3],
    /// Matriz de modelo efetiva (`T · R · S`), em coluna-maior.
    ///
    /// Guardada aqui porque o picking precisa da **inversa** para levar o raio
    /// ao espaco do arquivo. Deduzi-la das caixas nao funciona: a caixa de
    /// mundo e a envolvente do modelo ja girado, e com rumo 59,98 graus ela e
    /// bem maior que o modelo.
    pub matriz: [[f32; 4]; 4],
}

impl Objeto {
    /// Caminho do arquivo de origem (util para `projeto::ObjetoSalvo::arquivo`).
    pub fn arquivo_path(&self) -> PathBuf {
        self.arquivo.clone()
    }
}

impl Objeto {
    pub fn centro(&self) -> [f32; 3] {
        [
            (self.min_enu[0] + self.max_enu[0]) * 0.5,
            (self.min_enu[1] + self.max_enu[1]) * 0.5,
            (self.min_enu[2] + self.max_enu[2]) * 0.5,
        ]
    }

    pub fn tamanho(&self) -> [f32; 3] {
        [
            self.max_enu[0] - self.min_enu[0],
            self.max_enu[1] - self.min_enu[1],
            self.max_enu[2] - self.min_enu[2],
        ]
    }

    pub fn triangulos(&self) -> usize {
        self.indices.len() / 3
    }

    /// Distância até o triângulo mais próximo atingido pelo raio.
    ///
    /// Os vértices da fonte estão no espaço do arquivo. Em vez de transformar
    /// os 936 mil do modelo a cada clique, o **raio** é levado para o espaço do
    /// arquivo pela inversa da matriz — uma inversão contra centenas de
    /// milhares de transformações. O parâmetro `t` é o mesmo nos dois espaços,
    /// porque a transformação é afim.
    pub fn intersecao_triangulos(&self, origem: [f64; 3], direcao: [f64; 3]) -> Option<f64> {
        // Usa a matriz **real** do objeto, não uma escala deduzida das caixas.
        // A caixa de mundo é a envolvente do modelo já girado: com rumo 59,98°
        // ela é bem maior que o modelo, e a escala deduzida dela encolhia o raio
        // para dentro. O sintoma era o clique atravessar o prédio.
        let inv = inverter_afim(self.matriz)?;
        let leva = |p: [f64; 3], w: f64| -> [f64; 3] {
            let mut q = [0.0f64; 3];
            for (k, item) in q.iter_mut().enumerate() {
                // Coluna-maior, como o resto do pipeline.
                *item = inv[0][k] as f64 * p[0]
                    + inv[1][k] as f64 * p[1]
                    + inv[2][k] as f64 * p[2]
                    + inv[3][k] as f64 * w;
            }
            q
        };
        let o = leva(origem, 1.0);
        // Direção é vetor: leva com w = 0, sem a translação.
        let d = leva(direcao, 0.0);

        let mut melhor = f64::INFINITY;
        for tri in self.indices.chunks_exact(3) {
            let (a, b, c) = (
                self.fonte.vertices.get(tri[0] as usize)?,
                self.fonte.vertices.get(tri[1] as usize)?,
                self.fonte.vertices.get(tri[2] as usize)?,
            );
            if let Some(t) = raio_triangulo(o, d, a.position, b.position, c.position) {
                if t < melhor {
                    melhor = t;
                }
            }
        }
        melhor.is_finite().then_some(melhor)
    }

    /// Recalcula a geometria no mundo e devolve os vertices para a GPU.
    pub fn transformar(&mut self, frame: &EnuFrame, solo_m: f64) -> Vec<arcz_model::ModelVertex> {
        let t = arcz_model::transformar(&self.fonte, frame, &self.placement, solo_m);
        // Mantida em sincronia com a geometria: um clique com matriz velha acerta
        // onde o objeto estava, nao onde esta.
        self.matriz = arcz_model::matriz_modelo(
            self.fonte.min,
            self.fonte.max,
            frame,
            &self.placement,
            solo_m,
        );
        self.min_enu = t.min_enu;
        self.max_enu = t.max_enu;
        t.vertices
    }
}

/// A cena editavel.
#[derive(Default)]
pub struct Editor {
    pub objetos: Vec<Objeto>,
    pub selecionado: Option<ObjetoId>,
    proximo_id: ObjetoId,
    pub nodes: Vec<SceneNode>,
    pub bus: CommandBus,
    pub selection: SelectionState,
}

impl Editor {
    /// Sincroniza a lista de objetos legados com os nós autoritativos do Scene Graph.
    pub fn sincronizar_nodes(&mut self) {
        for obj in &self.objetos {
            let node_id = obj.id.to_string();
            if !self.nodes.iter().any(|n| n.id == node_id) {
                let mut node = SceneNode::novo(node_id, obj.nome.clone(), NodeType::Building);
                node.parent_id = obj.pai.map(|p| p.to_string());
                node.visibility = obj.visivel;
                node.locked = obj.bloqueado;
                node.selectable = obj.selecionavel;
                node.confidence = NodeConfidence::Observed;
                node.transform.position = [
                    obj.placement.offset_leste_m as f64,
                    obj.placement.offset_norte_m as f64,
                    obj.placement.offset_vertical_m as f64,
                ];
                self.nodes.push(node);
            }
        }
    }
    /// Insere um modelo ja carregado e devolve o id atribuido.
    ///
    /// `pai` (opcional) torna o objeto filho de outro. O id do pai tem que existir;
    /// um pai invalido faz a insercao falhar (`None`).
    pub fn adicionar(
        &mut self,
        nome: String,
        model: Model,
        placement: Placement,
        pai: Option<ObjetoId>,
    ) -> Option<ObjetoId> {
        self.adicionar_com_arquivo(nome, model, placement, pai, PathBuf::new())
    }

    /// Mesma coisa, mas com o caminho do arquivo preservado para o save/load.
    pub fn adicionar_com_arquivo(
        &mut self,
        nome: String,
        model: Model,
        placement: Placement,
        pai: Option<ObjetoId>,
        arquivo: PathBuf,
    ) -> Option<ObjetoId> {
        if let Some(pid) = pai {
            if !self.objetos.iter().any(|o| o.id == pid) {
                return None;
            }
        }
        let id = self.proximo_id;
        self.proximo_id += 1;

        self.objetos.push(Objeto {
            id,
            nome,
            pai,
            fonte: FonteGeometria::from_model(&model),
            indices: model.indices,
            submeshes: model.submeshes,
            materiais: model.materiais,
            texturas: model.texturas,
            placement,
            visivel: true,
            selecionavel: true,
            bloqueado: false,
            arquivo,
            min_enu: [0.0; 3],
            max_enu: [0.0; 3],
            // Identidade até a primeira `transformar`, que a preenche com a
            // matriz efetiva. Antes disso a caixa também é zero, então o objeto
            // não é alcançável pelo picking de qualquer forma.
            matriz: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        });
        Some(id)
    }

    /// Registra geometria já carregada como objeto editável.
    ///
    /// Diferente de `adicionar`, não lê o arquivo: reaproveita a `FonteGeometria`
    /// que o renderer já tem em memória. Serve para o modelo de `--modelo`, que
    /// é desenhado por caminho próprio mas precisa existir no `Editor` para ser
    /// clicável — sem duplicar 936 mil vértices.
    ///
    /// Não sobe materiais nem texturas: o objeto existe para picking e
    /// transformação, e quem desenha continua sendo o caminho original.
    pub fn registrar_fonte(
        &mut self,
        nome: String,
        fonte: FonteGeometria,
        placement: Placement,
        arquivo: PathBuf,
    ) -> ObjetoId {
        let id = self.proximo_id;
        self.proximo_id += 1;
        // Um triângulo por face não existe aqui: os índices vêm do próprio
        // arquivo e o picking precisa deles. Sem índices o objeto seria
        // invisível ao clique, que é justamente o que se quer evitar.
        let indices: Vec<u32> = (0..fonte.vertices.len() as u32).collect();
        self.objetos.push(Objeto {
            id,
            nome,
            pai: None,
            fonte,
            indices,
            submeshes: Vec::new(),
            materiais: Vec::new(),
            texturas: Vec::new(),
            placement,
            visivel: true,
            selecionavel: true,
            bloqueado: false,
            arquivo,
            min_enu: [0.0; 3],
            max_enu: [0.0; 3],
            matriz: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        });
        id
    }

    /// Versao simplificada que aceita `pai = None` e propaga o erro como panic
    /// em testes; o chamador real (main.rs / load de projeto) usa o retorno
    /// `Option<ObjetoId>`.
    #[cfg(test)]
    pub fn adicionar_raiz(&mut self, nome: String, model: Model, placement: Placement) -> ObjetoId {
        self.adicionar(nome, model, placement, None)
            .expect("falha inesperada ao adicionar raiz")
    }

    pub fn remover(&mut self, id: ObjetoId) -> bool {
        let antes = self.objetos.len();
        if self.ancestrais_de(id).contains(&id) {
            return false;
        }
        let mut a_remover: Vec<ObjetoId> = vec![id];
        let mut i = 0;
        while i < a_remover.len() {
            let descendentes = self.descendentes_de(a_remover[i]);
            for d in descendentes {
                if !a_remover.contains(&d) {
                    a_remover.push(d);
                }
            }
            i += 1;
        }
        self.objetos.retain(|o| !a_remover.contains(&o.id));
        if self.selecionado.is_some_and(|s| a_remover.contains(&s)) {
            self.selecionado = None;
        }
        self.objetos.len() != antes
    }

    /// Retorna uma COPIA do objeto removido (util pro Comando::Remover que
    /// precisa do objeto inteiro pra desfazer).
    pub fn remover_retornando(&mut self, id: ObjetoId) -> Option<Objeto> {
        let pos = self.objetos.iter().position(|o| o.id == id)?;
        let obj = self.objetos.remove(pos);
        if self.selecionado == Some(id) {
            self.selecionado = None;
        }
        Some(obj)
    }

    /// Cadeia de ancestrais de um objeto, do pai imediato ate a raiz.
    /// O proprio id NAO aparece (a menos que haja ciclo, caso em que aparece —
    /// o `remover` usa isso pra detectar).
    pub fn ancestrais_de(&self, id: ObjetoId) -> Vec<ObjetoId> {
        let mut out = Vec::new();
        let mut atual = self.get(id).and_then(|o| o.pai);
        while let Some(pid) = atual {
            if out.contains(&pid) || pid == id {
                out.push(pid);
                break;
            }
            out.push(pid);
            atual = self.get(pid).and_then(|o| o.pai);
        }
        out
    }

    /// Filhos diretos de um objeto (nao recursivo).
    pub fn filhos_de(&self, id: ObjetoId) -> Vec<ObjetoId> {
        self.objetos
            .iter()
            .filter(|o| o.pai == Some(id))
            .map(|o| o.id)
            .collect()
    }

    /// Todos os descendentes em largura (filhos, netos, bisnetos...).
    pub fn descendentes_de(&self, id: ObjetoId) -> Vec<ObjetoId> {
        let mut out = Vec::new();
        let mut fila: Vec<ObjetoId> = self.filhos_de(id);
        while let Some(f) = fila.pop() {
            if out.contains(&f) {
                continue;
            }
            out.push(f);
            for neto in self.filhos_de(f) {
                fila.push(neto);
            }
        }
        out
    }

    /// Lista os objetos em ordem topologica (raiz primeiro, filhos depois).
    /// Util para o renderizador: garante que o pai ja foi desenhado quando o
    /// filho for desenhado (embora na pratica a ordem de draw nao importe
    /// enquanto o depth test esta ligado).
    pub fn lista_por_hierarquia(&self) -> Vec<ObjetoId> {
        let mut out = Vec::with_capacity(self.objetos.len());
        for o in &self.objetos {
            if o.pai.is_none() {
                out.push(o.id);
                for d in self.descendentes_de(o.id) {
                    if !out.contains(&d) {
                        out.push(d);
                    }
                }
            }
        }
        out
    }

    pub fn get(&self, id: ObjetoId) -> Option<&Objeto> {
        self.objetos.iter().find(|o| o.id == id)
    }

    pub fn get_mut(&mut self, id: ObjetoId) -> Option<&mut Objeto> {
        self.objetos.iter_mut().find(|o| o.id == id)
    }

    pub fn selecionado(&self) -> Option<&Objeto> {
        self.selecionado.and_then(|id| self.get(id))
    }

    pub fn selecionado_mut(&mut self) -> Option<&mut Objeto> {
        let id = self.selecionado?;
        self.get_mut(id)
    }

    /// Qual objeto o raio atinge primeiro. `None` se nenhum.
    ///
    /// Testa contra a caixa envolvente, nao contra os triangulos: para escolher
    /// **qual** objeto foi clicado a caixa basta, e testar 900 mil triangulos por
    /// clique custaria mais que renderizar o quadro.
    pub fn picar(&self, origem: [f64; 3], direcao: [f64; 3]) -> Option<ObjetoId> {
        let mut melhor: Option<(f64, ObjetoId)> = None;
        for o in self
            .objetos
            .iter()
            .filter(|o| o.visivel && o.selecionavel && !o.bloqueado)
        {
            // A caixa e so a rejeicao barata. Quem decide e o triangulo.
            //
            // Malha agregada quebra o teste por caixa: as vias do entorno vem
            // agrupadas por classe, e a caixa de "Vias — local" mede 715 x 805 m
            // e engloba o bairro inteiro, inclusive o ar acima dos predios.
            // Qualquer raio entra nela antes de chegar a qualquer coisa, e o
            // clique selecionava sempre a rua — em cima do predio inclusive.
            if intersecao_aabb(origem, direcao, o.min_enu, o.max_enu).is_none() {
                continue;
            }
            if let Some(t) = o.intersecao_triangulos(origem, direcao) {
                if melhor.is_none_or(|(td, _)| t < td) {
                    melhor = Some((t, o.id));
                }
            }
        }
        melhor.map(|(_, id)| id)
    }

    /// Mapeia o raio de picking diretamente para o NodeId autoritativo.
    pub fn picar_node(&self, origem: [f64; 3], direcao: [f64; 3]) -> Option<NodeId> {
        self.picar(origem, direcao).map(|id| id.to_string())
    }

    /// Caixa envolvente de todos os objetos visiveis, para enquadrar a cena.
    pub fn caixa_total(&self) -> Option<([f32; 3], [f32; 3])> {
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        let mut algum = false;
        for o in self.objetos.iter().filter(|o| o.visivel) {
            algum = true;
            for k in 0..3 {
                min[k] = min[k].min(o.min_enu[k]);
                max[k] = max[k].max(o.max_enu[k]);
            }
        }
        algum.then_some((min, max))
    }
}

/// Inversa de uma matriz afim (rotação, escala e translação).
///
/// Não serve para projeção — e não precisa: a matriz de modelo do ARCZ é sempre
/// `T · R · S`. Inverter a 3×3 e reaplicar à translação é muito mais barato e
/// numericamente estável que uma inversão 4×4 genérica.
fn inverter_afim(m: [[f32; 4]; 4]) -> Option<[[f32; 4]; 4]> {
    let a = [
        [m[0][0] as f64, m[1][0] as f64, m[2][0] as f64],
        [m[0][1] as f64, m[1][1] as f64, m[2][1] as f64],
        [m[0][2] as f64, m[1][2] as f64, m[2][2] as f64],
    ];
    let det = a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1])
        - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
        + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]);
    // Escala zero colapsa o objeto: não há inversa, e o clique só o ignora.
    if det.abs() < 1e-15 {
        return None;
    }
    let id = 1.0 / det;
    let i = [
        [
            (a[1][1] * a[2][2] - a[1][2] * a[2][1]) * id,
            (a[0][2] * a[2][1] - a[0][1] * a[2][2]) * id,
            (a[0][1] * a[1][2] - a[0][2] * a[1][1]) * id,
        ],
        [
            (a[1][2] * a[2][0] - a[1][0] * a[2][2]) * id,
            (a[0][0] * a[2][2] - a[0][2] * a[2][0]) * id,
            (a[0][2] * a[1][0] - a[0][0] * a[1][2]) * id,
        ],
        [
            (a[1][0] * a[2][1] - a[1][1] * a[2][0]) * id,
            (a[0][1] * a[2][0] - a[0][0] * a[2][1]) * id,
            (a[0][0] * a[1][1] - a[0][1] * a[1][0]) * id,
        ],
    ];
    let t = [m[3][0] as f64, m[3][1] as f64, m[3][2] as f64];
    let nt = [
        -(i[0][0] * t[0] + i[0][1] * t[1] + i[0][2] * t[2]),
        -(i[1][0] * t[0] + i[1][1] * t[1] + i[1][2] * t[2]),
        -(i[2][0] * t[0] + i[2][1] * t[1] + i[2][2] * t[2]),
    ];
    Some([
        [i[0][0] as f32, i[1][0] as f32, i[2][0] as f32, 0.0],
        [i[0][1] as f32, i[1][1] as f32, i[2][1] as f32, 0.0],
        [i[0][2] as f32, i[1][2] as f32, i[2][2] as f32, 0.0],
        [nt[0] as f32, nt[1] as f32, nt[2] as f32, 1.0],
    ])
}

/// Interseção raio–triângulo por Möller–Trumbore.
///
/// Sem `culling`: uma parede vista por dentro (corte, câmera dentro do prédio)
/// precisa continuar clicável, e o winding do modelo importado nem sempre é
/// consistente.
fn raio_triangulo(
    origem: [f64; 3],
    direcao: [f64; 3],
    a: [f32; 3],
    b: [f32; 3],
    c: [f32; 3],
) -> Option<f64> {
    let a = [a[0] as f64, a[1] as f64, a[2] as f64];
    let e1 = [b[0] as f64 - a[0], b[1] as f64 - a[1], b[2] as f64 - a[2]];
    let e2 = [c[0] as f64 - a[0], c[1] as f64 - a[1], c[2] as f64 - a[2]];

    let cruz = |u: [f64; 3], v: [f64; 3]| {
        [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ]
    };
    let escalar = |u: [f64; 3], v: [f64; 3]| u[0] * v[0] + u[1] * v[1] + u[2] * v[2];

    let h = cruz(direcao, e2);
    let det = escalar(e1, h);
    // Raio paralelo ao plano do triângulo.
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let s = [origem[0] - a[0], origem[1] - a[1], origem[2] - a[2]];
    let u = escalar(s, h) * inv;
    if !(-1e-9..=1.0 + 1e-9).contains(&u) {
        return None;
    }
    let q = cruz(s, e1);
    let v = escalar(direcao, q) * inv;
    if v < -1e-9 || u + v > 1.0 + 1e-9 {
        return None;
    }
    let t = escalar(e2, q) * inv;
    // Atrás da câmera não conta.
    (t > 1e-9).then_some(t)
}

/// Distancia ate a entrada do raio na caixa, pelo metodo dos slabs.
///
/// Devolve `None` se o raio erra a caixa ou so a atinge para tras.
pub fn intersecao_aabb(
    origem: [f64; 3],
    direcao: [f64; 3],
    min: [f32; 3],
    max: [f32; 3],
) -> Option<f64> {
    let mut t_entra = f64::NEG_INFINITY;
    let mut t_sai = f64::INFINITY;

    for k in 0..3 {
        let (lo, hi) = (min[k] as f64, max[k] as f64);
        if direcao[k].abs() < 1e-12 {
            // Raio paralelo a este par de faces: so passa se ja estiver entre elas.
            if origem[k] < lo || origem[k] > hi {
                return None;
            }
            continue;
        }
        let inv = 1.0 / direcao[k];
        let mut t0 = (lo - origem[k]) * inv;
        let mut t1 = (hi - origem[k]) * inv;
        if t0 > t1 {
            std::mem::swap(&mut t0, &mut t1);
        }
        t_entra = t_entra.max(t0);
        t_sai = t_sai.min(t1);
        if t_entra > t_sai {
            return None;
        }
    }

    // Caixa inteiramente atras da camera.
    if t_sai < 0.0 {
        return None;
    }
    // Camera dentro da caixa: a "entrada" e a propria origem.
    Some(t_entra.max(0.0))
}

/// Um item da biblioteca de assets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemBiblioteca {
    pub nome: String,
    pub caminho: PathBuf,
    /// Pasta imediatamente acima, usada para agrupar na interface.
    pub categoria: String,
}

/// Constroi um `Editor` a partir de um `Projeto` salvo. Cada `ObjetoSalvo`
/// vira um `Objeto` no editor, com `Model` carregado do `arquivo`. Arquivos
/// ausentes sao pulados (e reportados em `erros`). E o caminho usado pela
/// E.4 do Nucleo do Editor pra `--abrir projeto.arcz`.
pub fn editor_de_projeto(projeto: &projeto::Projeto) -> (Editor, Vec<(PathBuf, String)>) {
    let mut ed = Editor::default();
    let mut erros = Vec::new();
    for obj in &projeto.objetos {
        match arcz_model::Model::load(&obj.arquivo) {
            Ok(m) => {
                let id = ed.adicionar_com_arquivo(
                    obj.nome.clone(),
                    m,
                    obj.placement(),
                    obj.pai,
                    obj.arquivo.clone(),
                );
                if id.is_none() {
                    erros.push((
                        obj.arquivo.clone(),
                        "adicionar falhou (pai invalido?)".into(),
                    ));
                } else if let Some(_id) = id {
                    // Restaura `visivel`. O `adicionar` cria id proprio;
                    // aqui o `id` salvo eh ignorado (o novo id do Editor pode
                    // ser diferente). Para v1->v2, isso muda os ids em
                    // save->load, mas como `projeto.rs` usa `id: u32` apenas
                    // pra exibicao (nao pra referencia cruzada), e aceitavel.
                    if let Some(o) = ed.objetos.last_mut() {
                        o.visivel = obj.visivel;
                    }
                }
            }
            Err(e) => {
                erros.push((obj.arquivo.clone(), e.to_string()));
            }
        }
    }
    (ed, erros)
}

/// Constroi um `Projeto` a partir de um `Editor` (para `--salvar`).
pub fn projeto_de_editor(
    editor: &Editor,
    nome: String,
    scene: &crate::scene::Scene,
) -> projeto::Projeto {
    let c = scene.bbox.center();
    projeto::Projeto {
        versao: projeto::VERSAO_FORMATO,
        nome,
        lat: c.lat_deg,
        lon: c.lon_deg,
        lado_m: scene.lado_m,
        zoom_dem: 14, // fixo por enquanto; pode ser salvo depois
        zoom_imagery: 18,
        mes: 3,
        dia: 21,
        hora: 15.0,
        objetos: editor
            .objetos
            .iter()
            .map(|o| {
                // Caminho absoluto canonico: o projeto fica independente do cwd
                // e funciona de qualquer lugar. `canonicalize` falha se o
                // arquivo nao existe, e ai caimos pro caminho original
                // (melhor que perder o objeto).
                let caminho = o
                    .arquivo
                    .canonicalize()
                    .unwrap_or_else(|_| o.arquivo.clone());
                projeto::ObjetoSalvo::de_placement_com_pai(
                    o.id,
                    o.nome.clone(),
                    caminho,
                    &o.placement,
                    o.visivel,
                    o.pai,
                )
            })
            .collect(),
        cameras: Vec::new(),
    }
}

/// Constroi um `Editor` a partir de uma pasta de modelos.
///
/// Cada `.glb`/`.gltf` encontrado vira um objeto. `lat`/`lon`/`heading`/`escala`
/// sao aplicados a todos (uniformes). Arquivos que falham ao carregar sao
/// coletados em `erros` para o caller avisar o usuario, em vez de abortar
/// silenciosamente a cena inteira.
pub fn editor_de_biblioteca(
    raiz: &Path,
    lat: f64,
    lon: f64,
    heading: f64,
    escala: f32,
    limite: usize,
) -> (Editor, Vec<(PathBuf, String)>) {
    let itens = varrer_biblioteca(raiz, limite);
    let mut ed = Editor::default();
    let mut erros = Vec::new();
    for item in itens {
        match arcz_model::Model::load(&item.caminho) {
            Ok(m) => {
                let p = Placement {
                    lat_deg: lat,
                    lon_deg: lon,
                    heading_deg: heading,
                    escala,
                    ..Default::default()
                };
                if ed.adicionar(item.nome.clone(), m, p, None).is_none() {
                    erros.push((item.caminho.clone(), "adicionar falhou (inesperado)".into()));
                }
            }
            Err(e) => {
                erros.push((item.caminho, e.to_string()));
            }
        }
    }
    (ed, erros)
}

/// Varre um diretorio atras de modelos carregaveis.
///
/// So lista `.glb`/`.gltf`: sao os formatos que o loader le hoje. Listar `.fbx` e
/// `.obj` que nao abrem daria erro so na hora de inserir, que e tarde demais.
pub fn varrer_biblioteca(raiz: &Path, limite: usize) -> Vec<ItemBiblioteca> {
    let mut achados = Vec::new();
    let mut pilha = vec![raiz.to_path_buf()];

    while let Some(dir) = pilha.pop() {
        if achados.len() >= limite {
            break;
        }
        let Ok(entradas) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entradas.flatten() {
            let p = e.path();
            if p.is_dir() {
                pilha.push(p);
                continue;
            }
            let ext = p
                .extension()
                .and_then(|x| x.to_str())
                .map(|x| x.to_ascii_lowercase());
            if !matches!(ext.as_deref(), Some("glb") | Some("gltf")) {
                continue;
            }
            achados.push(ItemBiblioteca {
                nome: p
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("sem-nome")
                    .to_string(),
                categoria: dir
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string(),
                caminho: p,
            });
            if achados.len() >= limite {
                break;
            }
        }
    }

    achados.sort_by(|a, b| (&a.categoria, &a.nome).cmp(&(&b.categoria, &b.nome)));
    achados
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caixa_unitaria() -> ([f32; 3], [f32; 3]) {
        ([-1.0, -1.0, -1.0], [1.0, 1.0, 1.0])
    }

    #[test]
    fn raio_de_frente_atinge_a_caixa() {
        let (min, max) = caixa_unitaria();
        let t = intersecao_aabb([0.0, 0.0, -10.0], [0.0, 0.0, 1.0], min, max).unwrap();
        assert!((t - 9.0).abs() < 1e-9, "entrada em t={t}, esperado 9");
    }

    #[test]
    fn raio_que_passa_ao_lado_nao_atinge() {
        let (min, max) = caixa_unitaria();
        assert!(intersecao_aabb([5.0, 5.0, -10.0], [0.0, 0.0, 1.0], min, max).is_none());
    }

    #[test]
    fn caixa_atras_da_camera_nao_conta() {
        // Clicar nao pode selecionar o que esta atras de voce.
        let (min, max) = caixa_unitaria();
        assert!(intersecao_aabb([0.0, 0.0, 10.0], [0.0, 0.0, 1.0], min, max).is_none());
    }

    #[test]
    fn camera_dentro_da_caixa_conta_como_zero() {
        let (min, max) = caixa_unitaria();
        let t = intersecao_aabb([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], min, max).unwrap();
        assert_eq!(t, 0.0);
    }

    #[test]
    fn raio_paralelo_dentro_do_intervalo_atinge() {
        // Direcao sem componente em Y, mas a origem esta na faixa de Y da caixa.
        let (min, max) = caixa_unitaria();
        assert!(intersecao_aabb([-10.0, 0.5, 0.0], [1.0, 0.0, 0.0], min, max).is_some());
        // Fora da faixa de Y: erra.
        assert!(intersecao_aabb([-10.0, 9.0, 0.0], [1.0, 0.0, 0.0], min, max).is_none());
    }

    #[test]
    fn raio_diagonal_atinge_o_canto() {
        let (min, max) = caixa_unitaria();
        let d = 1.0 / 3.0_f64.sqrt();
        assert!(intersecao_aabb([-5.0, -5.0, -5.0], [d, d, d], min, max).is_some());
    }

    fn objeto_em(id: ObjetoId, min: [f32; 3], max: [f32; 3]) -> Objeto {
        let v0 = arcz_model::ModelVertex {
            position: [min[0], min[1], min[2]],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 0.0],
        };
        let v1 = arcz_model::ModelVertex {
            position: [max[0], min[1], min[2]],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 0.0],
        };
        let v2 = arcz_model::ModelVertex {
            position: [max[0], max[1], max[2]],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 1.0],
        };
        let v3 = arcz_model::ModelVertex {
            position: [min[0], max[1], max[2]],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 1.0],
        };
        Objeto {
            id,
            nome: format!("obj{id}"),
            pai: None,
            fonte: FonteGeometria {
                vertices: vec![v0, v1, v2, v3],
                min,
                max,
            },
            indices: vec![0, 1, 2, 0, 2, 3],
            submeshes: Vec::new(),
            materiais: Vec::new(),
            texturas: Vec::new(),
            placement: Placement::default(),
            visivel: true,
            selecionavel: true,
            bloqueado: false,
            arquivo: PathBuf::new(),
            min_enu: min,
            max_enu: max,
            matriz: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    #[test]
    fn o_clique_seleciona_o_objeto_mais_proximo() {
        let mut ed = Editor::default();
        // Dois cubos alinhados no eixo Z; o raio vem de -Z.
        ed.objetos
            .push(objeto_em(1, [-1.0, -1.0, 8.0], [1.0, 1.0, 10.0]));
        ed.objetos
            .push(objeto_em(2, [-1.0, -1.0, -2.0], [1.0, 1.0, 0.0]));

        let atingido = ed.picar([0.0, 0.0, -20.0], [0.0, 0.0, 1.0]);
        assert_eq!(atingido, Some(2), "deveria pegar o mais perto da camera");
    }

    #[test]
    fn objeto_invisivel_nao_e_selecionavel() {
        let mut ed = Editor::default();
        let mut o = objeto_em(1, [-1.0, -1.0, -1.0], [1.0, 1.0, 1.0]);
        o.visivel = false;
        ed.objetos.push(o);
        assert_eq!(ed.picar([0.0, 0.0, -10.0], [0.0, 0.0, 1.0]), None);
    }

    #[test]
    fn clique_no_vazio_nao_seleciona_nada() {
        let mut ed = Editor::default();
        ed.objetos
            .push(objeto_em(1, [-1.0, -1.0, -1.0], [1.0, 1.0, 1.0]));
        assert_eq!(ed.picar([50.0, 50.0, -10.0], [0.0, 0.0, 1.0]), None);
    }

    #[test]
    fn remover_limpa_a_selecao_do_objeto_removido() {
        let mut ed = Editor::default();
        ed.objetos.push(objeto_em(7, [0.0; 3], [1.0; 3]));
        ed.selecionado = Some(7);

        assert!(ed.remover(7));
        assert_eq!(
            ed.selecionado, None,
            "selecao ficou apontando para objeto morto"
        );
        assert!(
            !ed.remover(7),
            "remover duas vezes deveria falhar na segunda"
        );
    }

    #[test]
    fn remover_nao_mexe_na_selecao_de_outro_objeto() {
        let mut ed = Editor::default();
        ed.objetos.push(objeto_em(1, [0.0; 3], [1.0; 3]));
        ed.objetos.push(objeto_em(2, [0.0; 3], [1.0; 3]));
        ed.selecionado = Some(2);
        ed.remover(1);
        assert_eq!(ed.selecionado, Some(2));
    }

    #[test]
    fn ids_nunca_sao_reaproveitados() {
        // Reaproveitar id faria a selecao apontar para o objeto errado depois de
        // remover e adicionar.
        let mut ed = Editor {
            proximo_id: 0,
            ..Default::default()
        };
        let a = ed.adicionar_raiz(
            "a".into(),
            Model::from_glb_slice(&glb_min()).unwrap(),
            Placement::default(),
        );
        ed.remover(a);
        let b = ed.adicionar_raiz(
            "b".into(),
            Model::from_glb_slice(&glb_min()).unwrap(),
            Placement::default(),
        );
        assert_ne!(a, b, "o id foi reciclado");
    }

    #[test]
    fn caixa_total_cobre_todos_os_visiveis() {
        let mut ed = Editor::default();
        ed.objetos
            .push(objeto_em(1, [-5.0, 0.0, -5.0], [-1.0, 3.0, -1.0]));
        ed.objetos
            .push(objeto_em(2, [1.0, -2.0, 1.0], [6.0, 2.0, 8.0]));

        let (min, max) = ed.caixa_total().unwrap();
        assert_eq!(min, [-5.0, -2.0, -5.0]);
        assert_eq!(max, [6.0, 3.0, 8.0]);

        // Sem objetos visiveis nao ha caixa.
        ed.objetos.iter_mut().for_each(|o| o.visivel = false);
        assert!(ed.caixa_total().is_none());
    }

    #[test]
    fn a_biblioteca_so_lista_formatos_que_o_loader_abre() {
        let dir = std::env::temp_dir().join(format!("arcz-bib-{}", std::process::id()));
        let sub = dir.join("moveis");
        std::fs::create_dir_all(&sub).unwrap();
        for arquivo in ["cadeira.glb", "mesa.gltf", "sofa.fbx", "leia.txt"] {
            std::fs::write(sub.join(arquivo), b"x").unwrap();
        }

        let itens = varrer_biblioteca(&dir, 100);
        let nomes: Vec<&str> = itens.iter().map(|i| i.nome.as_str()).collect();

        assert!(nomes.contains(&"cadeira") && nomes.contains(&"mesa"));
        assert!(
            !nomes.contains(&"sofa"),
            "listou .fbx, que o loader nao abre — o erro apareceria so ao inserir"
        );
        assert!(!nomes.contains(&"leia"));
        assert_eq!(itens[0].categoria, "moveis");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_varredura_respeita_o_limite() {
        let dir = std::env::temp_dir().join(format!("arcz-bib2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..20 {
            std::fs::write(dir.join(format!("m{i}.glb")), b"x").unwrap();
        }
        assert_eq!(varrer_biblioteca(&dir, 5).len(), 5);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // === testes de hierarquia pai/filho (E.1) =================================

    fn ed_com_3_objetos() -> (Editor, ObjetoId, ObjetoId, ObjetoId) {
        let mut ed = Editor::default();
        let a = ed
            .adicionar(
                "raiz".into(),
                Model::from_glb_slice(&glb_min()).unwrap(),
                Placement::default(),
                None,
            )
            .unwrap();
        let b = ed
            .adicionar(
                "filho1".into(),
                Model::from_glb_slice(&glb_min()).unwrap(),
                Placement::default(),
                Some(a),
            )
            .unwrap();
        let c = ed
            .adicionar(
                "filho2".into(),
                Model::from_glb_slice(&glb_min()).unwrap(),
                Placement::default(),
                Some(a),
            )
            .unwrap();
        (ed, a, b, c)
    }

    #[test]
    fn adicionar_com_pai_invalido_retorna_none() {
        let mut ed = Editor::default();
        ed.adicionar_raiz(
            "a".into(),
            Model::from_glb_slice(&glb_min()).unwrap(),
            Placement::default(),
        );
        let resultado = ed.adicionar(
            "b".into(),
            Model::from_glb_slice(&glb_min()).unwrap(),
            Placement::default(),
            Some(999),
        );
        assert!(resultado.is_none(), "pai 999 nao existe; deveria falhar");
        assert_eq!(ed.objetos.len(), 1, "objeto nao foi inserido");
    }

    #[test]
    fn ancestrais_de_sobe_da_folha_ate_a_raiz() {
        let (ed, a, b, c) = ed_com_3_objetos();
        assert_eq!(ed.ancestrais_de(c), vec![a]);
        assert_eq!(ed.ancestrais_de(b), vec![a]);
        assert!(ed.ancestrais_de(a).is_empty());
    }

    #[test]
    fn ancestrais_de_detecta_ciclo_e_nao_loopa() {
        // Constroi ciclo artificial. O `adicionar` nao cria isso, mas montamos
        // direto pra testar a guarda.
        let mut ed = Editor::default();
        ed.objetos.push(objeto_em(1, [0.0; 3], [1.0; 3]));
        ed.objetos.push(objeto_em(2, [0.0; 3], [1.0; 3]));
        ed.objetos[0].pai = Some(2);
        ed.objetos[1].pai = Some(1);
        let anc = ed.ancestrais_de(1);
        assert!(anc.contains(&2));
        assert!(anc.contains(&1), "o ciclo faz o proprio id aparecer");
        assert!(anc.len() <= 4, "ciclo nao foi cortado: {anc:?}");
    }

    #[test]
    fn filhos_de_retorna_apenas_diretos() {
        let (ed, a, b, c) = ed_com_3_objetos();
        let mut filhos = ed.filhos_de(a);
        filhos.sort();
        assert_eq!(filhos, vec![b, c]);
        assert!(ed.filhos_de(b).is_empty());
    }

    #[test]
    fn descendentes_de_expande_recursivo() {
        let (mut ed, a, b, c) = ed_com_3_objetos();
        let d = ed
            .adicionar(
                "neto".into(),
                Model::from_glb_slice(&glb_min()).unwrap(),
                Placement::default(),
                Some(b),
            )
            .unwrap();
        let mut desc = ed.descendentes_de(a);
        desc.sort();
        let mut esperado = vec![b, c, d];
        esperado.sort();
        assert_eq!(desc, esperado);
    }

    #[test]
    fn remover_pai_arrasta_todos_os_descendentes() {
        let (mut ed, a, b, _c) = ed_com_3_objetos();
        ed.adicionar(
            "neto".into(),
            Model::from_glb_slice(&glb_min()).unwrap(),
            Placement::default(),
            Some(b),
        )
        .unwrap();
        assert_eq!(ed.objetos.len(), 4);
        ed.remover(a);
        assert_eq!(ed.objetos.len(), 0, "tudo foi junto com o pai");
        assert!(ed.selecionado.is_none());
    }

    #[test]
    fn lista_por_hierarquia_coloca_raiz_primeiro() {
        let (ed, a, b, c) = ed_com_3_objetos();
        let lista = ed.lista_por_hierarquia();
        assert_eq!(lista[0], a);
        assert!(lista.contains(&b));
        assert!(lista.contains(&c));
    }

    #[test]
    fn objeto_pai_default_e_none() {
        let (ed, a, _b, _c) = ed_com_3_objetos();
        assert!(ed.get(a).unwrap().pai.is_none());
    }

    // === testes de historico (undo/redo) =====================================

    use super::{Comando, Historico};

    fn ed_com_1_objeto() -> (Editor, ObjetoId) {
        let mut ed = Editor::default();
        let id = ed
            .adicionar(
                "x".into(),
                Model::from_glb_slice(&glb_min()).unwrap(),
                Placement::default(),
                None,
            )
            .unwrap();
        (ed, id)
    }

    #[test]
    fn historico_novo_comeca_vazio() {
        let h = Historico::novo();
        assert_eq!(h.tamanho_feitos(), 0);
        assert_eq!(h.tamanho_refeitos(), 0);
    }

    #[test]
    fn desfazer_sem_historico_retorna_false() {
        let (mut ed, _id) = ed_com_1_objeto();
        let mut h = Historico::novo();
        assert!(!h.desfazer(&mut ed));
        assert!(!h.refazer(&mut ed));
    }

    #[test]
    fn mover_e_desfazer_volta_ao_estado_anterior() {
        let (mut ed, id) = ed_com_1_objeto();
        let mut h = Historico::novo();
        let antes = ed.get(id).unwrap().placement;
        let depois = Placement {
            offset_leste_m: 10.0,
            ..antes
        };
        h.executar(Comando::Mover { id, antes, depois }, &mut ed);
        assert!((ed.get(id).unwrap().placement.offset_leste_m - 10.0).abs() < 1e-6);
        assert!(h.desfazer(&mut ed));
        assert!((ed.get(id).unwrap().placement.offset_leste_m - 0.0).abs() < 1e-6);
    }

    #[test]
    fn refazer_reaplica_o_movimento() {
        let (mut ed, id) = ed_com_1_objeto();
        let mut h = Historico::novo();
        let antes = ed.get(id).unwrap().placement;
        let depois = Placement {
            offset_leste_m: 5.0,
            ..antes
        };
        h.executar(Comando::Mover { id, antes, depois }, &mut ed);
        h.desfazer(&mut ed);
        assert!(h.refazer(&mut ed));
        assert!((ed.get(id).unwrap().placement.offset_leste_m - 5.0).abs() < 1e-6);
    }

    #[test]
    fn nova_acao_limpa_a_pilha_de_redo() {
        let (mut ed, id) = ed_com_1_objeto();
        let mut h = Historico::novo();
        let antes = ed.get(id).unwrap().placement;
        let depois = Placement {
            offset_leste_m: 3.0,
            ..antes
        };
        h.executar(Comando::Mover { id, antes, depois }, &mut ed);
        h.desfazer(&mut ed);
        assert_eq!(h.tamanho_refeitos(), 1);
        // Nova acao descarta o redo.
        h.executar(
            Comando::Mover {
                id,
                antes: ed.get(id).unwrap().placement,
                depois: Placement {
                    offset_leste_m: 7.0,
                    ..antes
                },
            },
            &mut ed,
        );
        assert_eq!(h.tamanho_refeitos(), 0);
    }

    #[test]
    fn remover_e_desfazer_volta_o_objeto() {
        // Para o Comando::Remover funcionar, o Objeto precisa ter um
        // caminho de arquivo valido (o reverter recarrega o Model).
        let dir = std::env::temp_dir().join(format!("arcz-undo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let caminho = dir.join("alvo.glb");
        std::fs::write(&caminho, glb_min()).unwrap();

        let mut ed = Editor::default();
        let id = ed
            .adicionar_com_arquivo(
                "x".into(),
                Model::load(&caminho).unwrap(),
                Placement::default(),
                None,
                caminho.clone(),
            )
            .unwrap();
        let mut h = Historico::novo();
        let obj_copia = ed
            .get(id)
            .cloned()
            .unwrap_or_else(|| panic!("obj nao achado"));
        h.executar(Comando::Remover { objeto: obj_copia }, &mut ed);
        assert!(ed.get(id).is_none(), "objeto nao foi removido");
        // Desfazer: recriar (com novo id, mas mesmo total de objetos).
        assert!(h.desfazer(&mut ed));
        assert_eq!(ed.objetos.len(), 1, "desfazer nao recriou o objeto");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// GLB minimo valido: um triangulo.
    fn glb_min() -> Vec<u8> {
        let pos: [f32; 9] = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        let mut bin = Vec::new();
        for c in pos {
            bin.extend_from_slice(&c.to_le_bytes());
        }
        while bin.len() % 4 != 0 {
            bin.push(0);
        }
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],
            "nodes":[{{"mesh":0}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}}}}]}}],
            "buffers":[{{"byteLength":{}}}],
            "bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}}],
            "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3",
              "min":[0,0,0],"max":[1,1,0]}}]}}"#,
            bin.len()
        );
        let mut jb = json.into_bytes();
        while jb.len() % 4 != 0 {
            jb.push(b' ');
        }
        let total = 12 + 8 + jb.len() + 8 + bin.len();
        let mut glb = Vec::new();
        glb.extend_from_slice(b"glTF");
        glb.extend_from_slice(&2u32.to_le_bytes());
        glb.extend_from_slice(&(total as u32).to_le_bytes());
        glb.extend_from_slice(&(jb.len() as u32).to_le_bytes());
        glb.extend_from_slice(b"JSON");
        glb.extend_from_slice(&jb);
        glb.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        glb.extend_from_slice(&[0x42, 0x49, 0x4E, 0x00]);
        glb.extend_from_slice(&bin);
        glb
    }

    #[test]
    fn scenenode_serializacao_e_deserializacao_sem_perdas() {
        let mut node = SceneNode::novo("node_zenite_01", "Edifício Zênite", NodeType::Building);
        node.georeference = Some(Georeference64 {
            latitude: -27.1432,
            longitude: -48.4901,
            altitude: 15.4,
            heading: 145.0,
        });
        node.transform.position = [10.5, 20.2, 5.0];
        node.confidence = NodeConfidence::Observed;

        let json = serde_json::to_string(&node).unwrap();
        let deserialized: SceneNode = serde_json::from_str(&json).unwrap();
        assert_eq!(node, deserialized);
    }

    #[test]
    fn transform64_converte_para_f32_do_renderer() {
        // Quaternion de 90° em torno de Y. Usa-se a constante, e nao 0.7071
        // digitado: o literal truncado tem erro de 1e-5, que se acumula ao
        // compor rotacoes e faz o objeto derivar depois de alguns giros.
        const R: f64 = std::f64::consts::FRAC_1_SQRT_2;
        let t64 = Transform64 {
            position: [100.5, 200.25, 50.125],
            rotation: [0.0, R, 0.0, R],
            scale: [1.0, 2.0, 1.0],
        };
        let (pos_f32, rot_f32, scale_f32) = t64.to_renderer_f32();
        assert_eq!(pos_f32, [100.5f32, 200.25f32, 50.125f32]);
        assert_eq!(rot_f32, [0.0f32, R as f32, 0.0f32, R as f32]);
        assert_eq!(scale_f32, [1.0f32, 2.0f32, 1.0f32]);
    }

    #[test]
    fn mapeamento_de_confianca_e_codigo_de_cores() {
        assert_eq!(NodeConfidence::Observed.color_code(), "GREEN");
        assert_eq!(NodeConfidence::GisDerived.color_code(), "BLUE");
        assert_eq!(NodeConfidence::Reconstructed.color_code(), "YELLOW");
        assert_eq!(NodeConfidence::Inferred.color_code(), "RED");

        assert_eq!(NodeConfidence::Observed.value(), 1.0);
        assert_eq!(NodeConfidence::Inferred.value(), 0.3);
    }

    #[test]
    fn command_bus_com_arraste_preview_e_journal() {
        let dir = std::env::temp_dir().join(format!("arcz-bus-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let caminho = dir.join("alvo.glb");
        std::fs::write(&caminho, glb_min()).unwrap();

        let mut bus = CommandBus::novo();
        let mut ed = Editor::default();
        let id = ed
            .adicionar_com_arquivo(
                "Cubo Teste".into(),
                Model::load(&caminho).unwrap(),
                Placement::default(),
                None,
                caminho.clone(),
            )
            .unwrap();

        let inicial = Placement::default();
        bus.iniciar_arraste_preview(id, inicial);

        // Movimento intermediario (mouse drag) nao deve sujar o historico
        let temp = Placement {
            offset_leste_m: 5.0,
            ..inicial
        };
        bus.atualizar_arraste_preview(&mut ed, temp);
        assert_eq!(bus.historico.tamanho_feitos(), 0);
        assert_eq!(bus.journal.len(), 0);

        // Final do arraste: compromete 1 unica transacao
        let final_p = Placement {
            offset_leste_m: 10.0,
            ..inicial
        };
        bus.finalizar_arraste_comprometer(&mut ed, final_p);

        assert_eq!(bus.historico.tamanho_feitos(), 1);
        assert_eq!(bus.journal.len(), 1);
        assert_eq!(bus.journal[0].command_name, "Mover");

        // Undo reverte para o inicial
        assert!(bus.desfazer(&mut ed));
        assert_eq!(ed.get(id).unwrap().placement, inicial);

        // Redo reaplica para o final
        assert!(bus.refazer(&mut ed));
        assert_eq!(ed.get(id).unwrap().placement, final_p);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn servico_de_selecao_universal_e_sincronizacao() {
        let dir = std::env::temp_dir().join(format!("arcz-sel-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let caminho = dir.join("alvo.glb");
        std::fs::write(&caminho, glb_min()).unwrap();

        let mut ed = Editor::default();
        let id = ed
            .adicionar_com_arquivo(
                "Entidade Universal".into(),
                Model::load(&caminho).unwrap(),
                Placement::default(),
                None,
                caminho.clone(),
            )
            .unwrap();

        let mut state = SelectionState::default();
        state.selecionar_objeto(id);

        assert_eq!(state.selected_objeto_id, Some(id));
        assert_eq!(state.selected_node_id, Some(id.to_string()));
        assert!(state.highlight_outline);
        assert!(state.outliner_sync);
        assert!(state.inspector_sync);

        // Respeita objetos bloqueados ou nao selecionaveis
        if let Some(obj) = ed.get_mut(id) {
            obj.bloqueado = true;
        }
        assert_eq!(ed.picar_node([0.0, 0.0, -10.0], [0.0, 0.0, 1.0]), None);

        if let Some(obj) = ed.get_mut(id) {
            obj.bloqueado = false;
            obj.selecionavel = false;
        }
        assert_eq!(ed.picar_node([0.0, 0.0, -10.0], [0.0, 0.0, 1.0]), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn outliner_construcao_de_arvore_e_prevencao_de_ciclos() {
        let mut n1 = SceneNode::novo("pai_1", "Pai", NodeType::Building);
        n1.confidence = NodeConfidence::Observed;

        let mut n2 = SceneNode::novo("filho_1", "Filho", NodeType::Furniture);
        n2.parent_id = Some("pai_1".to_string());
        n2.confidence = NodeConfidence::GisDerived;

        let nodes = vec![n1, n2];
        let arvore = OutlinerService::construir_arvore(&nodes);

        assert_eq!(arvore.len(), 1);
        assert_eq!(arvore[0].id, "pai_1");
        assert_eq!(arvore[0].confidence_color, "GREEN");
        assert_eq!(arvore[0].children.len(), 1);
        assert_eq!(arvore[0].children[0].id, "filho_1");
        assert_eq!(arvore[0].children[0].confidence_color, "BLUE");

        // Reparentamento seguro (sem ciclos)
        assert!(OutlinerService::validar_reparentamento(
            "filho_1", None, &nodes
        ));
        // Reparentamento invalido (tentar tornar o pai filho de seu proprio filho -> ciclo)
        assert!(!OutlinerService::validar_reparentamento(
            "pai_1",
            Some("filho_1"),
            &nodes
        ));
    }

    #[test]
    fn inspector_extrai_payload_e_aplica_edicao_com_undo() {
        let mut bus = CommandBus::novo();
        let mut ed = Editor::default();
        let mut node = SceneNode::novo("node_edit", "Edificio Original", NodeType::Building);

        let payload_inicial = InspectorService::extrair_payload(&node);
        assert_eq!(payload_inicial.name, "Edificio Original");

        let mut payload_editado = payload_inicial.clone();
        payload_editado.name = "Edificio Renomeado".to_string();
        payload_editado.position = [10.0, 20.0, 5.0];
        payload_editado.visibility = false;

        InspectorService::aplicar_edicao(&mut bus, &mut ed, &mut node, payload_editado);

        assert_eq!(node.name, "Edificio Renomeado");
        assert_eq!(node.transform.position, [10.0, 20.0, 5.0]);
        assert!(!node.visibility);
        assert!(bus.historico.tamanho_feitos() >= 3);
        assert_eq!(bus.journal.len(), bus.historico.tamanho_feitos());
    }
}

#[cfg(test)]
mod tests_no_do_placement {
    use super::*;
    use arcz_model::Placement;

    fn placement() -> Placement {
        Placement {
            lat_deg: -27.1544967,
            lon_deg: -48.5022653,
            heading_deg: 59.98,
            escala: 1.0,
            offset_leste_m: 12.5,
            offset_norte_m: -7.25,
            offset_vertical_m: 3.0,
            assentar_no_terreno: true,
        }
    }

    #[test]
    fn a_georreferencia_guarda_lat_lon_em_f64() {
        // O ponto do exercicio: lat/lon nao podem passar por f32 em lugar
        // nenhum. Um f32 nesta latitude erra cerca de 1 m.
        let no = SceneNode::do_placement("zenite", &placement());
        let g = no.georeference.expect("sem georreferencia");
        assert_eq!(g.latitude, -27.1544967);
        assert_eq!(g.longitude, -48.5022653);
        assert_eq!(g.heading, 59.98);
    }

    #[test]
    fn o_offset_local_fica_no_transform_e_nao_na_latitude() {
        // Somar o ajuste fino na latitude misturaria metros com graus e perderia
        // precisao exatamente onde o resto do projeto trabalha para nao perder.
        let no = SceneNode::do_placement("zenite", &placement());
        assert_eq!(no.transform.position[0], 12.5);
        assert_eq!(no.transform.position[2], 7.25); // norte negativo vira +Z
        assert_eq!(no.georeference.unwrap().latitude, -27.1544967);
    }

    #[test]
    fn a_escala_vai_para_os_tres_eixos() {
        let mut p = placement();
        p.escala = 2.5;
        let no = SceneNode::do_placement("x", &p);
        assert_eq!(no.transform.scale, [2.5, 2.5, 2.5]);
    }

    #[test]
    fn o_quaternion_do_rumo_e_unitario_e_gira_no_sentido_certo() {
        // Quaternion nao normalizado deforma a malha ao ser aplicado.
        for graus in [0.0, 45.0, 90.0, 180.0, 270.0, 359.9, -30.0] {
            let q = quaternion_de_rumo(graus);
            let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
            assert!((n - 1.0).abs() < 1e-12, "rumo {graus}: norma {n}");
        }
        // Rumo 0 e a identidade.
        assert_eq!(quaternion_de_rumo(0.0), [0.0, 0.0, 0.0, 1.0]);
        // O rumo do ARCZ e horario a partir do norte; no espaco de render isso
        // e giro negativo em torno de +Y, entao o componente y fica negativo.
        assert!(quaternion_de_rumo(90.0)[1] < 0.0);
    }

    #[test]
    fn o_no_nasce_visivel_e_selecionavel() {
        // Um no que nasce oculto ou travado sumiria do Outliner sem explicacao.
        let no = SceneNode::do_placement("x", &placement());
        assert!(no.visibility);
        assert!(no.selectable);
        assert!(!no.locked);
        assert_eq!(no.node_type, NodeType::Building);
    }
}

#[cfg(test)]
mod tests_picking_preciso {
    use super::*;

    #[test]
    fn o_raio_acerta_o_triangulo_de_frente() {
        // Triangulo no plano z=0; raio vindo de +z para -z.
        let t = raio_triangulo(
            [0.25, 0.25, 5.0],
            [0.0, 0.0, -1.0],
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
        );
        assert!(t.is_some());
        assert!((t.unwrap() - 5.0).abs() < 1e-9);
    }

    #[test]
    fn o_raio_erra_fora_do_triangulo() {
        // (0.9, 0.9) esta fora da hipotenusa u+v<=1.
        assert!(raio_triangulo(
            [0.9, 0.9, 5.0],
            [0.0, 0.0, -1.0],
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
        )
        .is_none());
    }

    #[test]
    fn o_triangulo_atras_da_camera_nao_conta() {
        // Sem esta guarda, clicar no ceu selecionaria o que esta atras.
        assert!(raio_triangulo(
            [0.25, 0.25, 5.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
        )
        .is_none());
    }

    #[test]
    fn a_face_de_tras_tambem_e_clicavel() {
        // Camera dentro do predio (corte, vista interna) precisa selecionar a
        // parede vista pelo avesso.
        let t = raio_triangulo(
            [0.25, 0.25, -5.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
        );
        assert!(t.is_some(), "backface deveria ser clicavel");
    }

    #[test]
    fn o_raio_paralelo_ao_plano_nao_acerta() {
        assert!(raio_triangulo(
            [0.25, 0.25, 1.0],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
        )
        .is_none());
    }

    #[test]
    fn inverter_a_identidade_devolve_a_identidade() {
        let id = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let inv = inverter_afim(id).expect("identidade e inversivel");
        for i in 0..4 {
            for j in 0..4 {
                assert!((inv[i][j] - id[i][j]).abs() < 1e-6, "[{i}][{j}]");
            }
        }
    }

    #[test]
    fn a_inversa_desfaz_translacao_rotacao_e_escala() {
        // Rumo de 90 graus em torno de Y, escala 2, transladado.
        let m: [[f32; 4]; 4] = [
            [0.0, 0.0, -2.0, 0.0],
            [0.0, 2.0, 0.0, 0.0],
            [2.0, 0.0, 0.0, 0.0],
            [10.0, 5.0, -3.0, 1.0],
        ];
        let inv = inverter_afim(m).expect("inversivel");

        // Leva um ponto pela matriz e traz de volta pela inversa.
        let aplicar = |mat: [[f32; 4]; 4], p: [f64; 3]| -> [f64; 3] {
            let mut q = [0.0f64; 3];
            for (k, item) in q.iter_mut().enumerate() {
                *item = mat[0][k] as f64 * p[0]
                    + mat[1][k] as f64 * p[1]
                    + mat[2][k] as f64 * p[2]
                    + mat[3][k] as f64;
            }
            q
        };
        let p = [3.0, -7.0, 1.5];
        let ida = aplicar(m, p);
        let volta = aplicar(inv, ida);
        for k in 0..3 {
            assert!(
                (volta[k] - p[k]).abs() < 1e-4,
                "eixo {k}: {} vs {}",
                volta[k],
                p[k]
            );
        }
    }

    #[test]
    fn escala_zero_nao_tem_inversa() {
        // Objeto colapsado nao pode virar panico nem divisao por zero no clique.
        let m = [
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        assert!(inverter_afim(m).is_none());
    }
}

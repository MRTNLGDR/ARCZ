//! Scene Graph e Command Bus autoritativo do ARCZ Earth.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Nível de confiança da informação no Scene Graph.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[derive(Default)]
pub enum NodeConfidence {
    Observed,     // Medido no local / Scanner / Foto (Verde)
    #[default]
    GisDerived,   // Extraído de fonte oficial GIS (Azul)
    Reconstructed,// Reconstruído por SFM/AI (Amarelo)
    Inferred,     // Gerado procedualmente (Vermelho)
}


/// Tipos de nós no Scene Graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NodeType {
    Root,
    Site,
    Building,
    Level,
    Wall,
    Opening,
    Furniture,
    Terrain,
    Vegetation,
    Road,
    Camera,
    Light,
    GenericModel,
}

/// Transformação 64-bit para viewport geoespacial sem jitter.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

/// Georreferência WGS84 em ponto flutuante duplo.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Georeference64 {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: f64,
    pub heading: f64,
}

/// Nó autoritativo da cena no ARCZ.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SceneNode {
    pub id: u64,
    pub parent_id: Option<u64>,
    pub name: String,
    pub node_type: NodeType,
    pub confidence: NodeConfidence,
    pub transform: Transform64,
    pub georeference: Option<Georeference64>,
    pub visible: bool,
    pub locked: bool,
}

impl SceneNode {
    pub fn new(id: u64, name: impl Into<String>, node_type: NodeType) -> Self {
        Self {
            id,
            parent_id: None,
            name: name.into(),
            node_type,
            confidence: NodeConfidence::default(),
            transform: Transform64::default(),
            georeference: None,
            visible: true,
            locked: false,
        }
    }
}

/// Comandos de mutação do Scene Graph (Command Pattern).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Command {
    AddNode(SceneNode),
    RemoveNode { id: u64 },
    UpdateTransform { id: u64, transform: Transform64 },
    RenameNode { id: u64, name: String },
    SetVisibility { id: u64, visible: bool },
}

/// Registro no Journal de operações.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    pub revision: u64,
    pub command: Command,
    pub timestamp_utc: String,
}

/// Barramento de comandos com pilha de Undo/Redo e revision monotônica.
#[derive(Debug, Default)]
pub struct CommandBus {
    pub revision: u64,
    pub nodes: HashMap<u64, SceneNode>,
    pub undo_stack: Vec<Command>,
    pub redo_stack: Vec<Command>,
    pub journal: Vec<JournalEntry>,
}

impl CommandBus {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply(&mut self, cmd: Command) {
        self.revision += 1;
        self.journal.push(JournalEntry {
            revision: self.revision,
            command: cmd.clone(),
            timestamp_utc: "2026-07-30T15:26:00Z".to_string(),
        });

        let inverse = match &cmd {
            Command::AddNode(node) => Command::RemoveNode { id: node.id },
            Command::RemoveNode { id } => {
                if let Some(n) = self.nodes.get(id) {
                    Command::AddNode(n.clone())
                } else {
                    Command::RemoveNode { id: *id }
                }
            }
            Command::UpdateTransform { id, transform: _ } => {
                let prev_t = self.nodes.get(id).map(|n| n.transform.clone()).unwrap_or_default();
                Command::UpdateTransform { id: *id, transform: prev_t }
            }
            Command::RenameNode { id, name: _ } => {
                let prev_name = self.nodes.get(id).map(|n| n.name.clone()).unwrap_or_default();
                Command::RenameNode { id: *id, name: prev_name }
            }
            Command::SetVisibility { id, visible } => Command::SetVisibility { id: *id, visible: !*visible },
        };

        match &cmd {
            Command::AddNode(node) => {
                self.nodes.insert(node.id, node.clone());
            }
            Command::RemoveNode { id } => {
                self.nodes.remove(id);
            }
            Command::UpdateTransform { id, transform } => {
                if let Some(n) = self.nodes.get_mut(id) {
                    n.transform = transform.clone();
                }
            }
            Command::RenameNode { id, name } => {
                if let Some(n) = self.nodes.get_mut(id) {
                    n.name = name.clone();
                }
            }
            Command::SetVisibility { id, visible } => {
                if let Some(n) = self.nodes.get_mut(id) {
                    n.visible = *visible;
                }
            }
        }

        self.undo_stack.push(inverse);
        self.redo_stack.clear();
    }


    pub fn undo(&mut self) -> Option<Command> {
        let inverse = self.undo_stack.pop()?;
        self.revision += 1;
        match &inverse {
            Command::AddNode(node) => {
                self.nodes.insert(node.id, node.clone());
            }
            Command::RemoveNode { id } => {
                self.nodes.remove(id);
            }
            Command::UpdateTransform { id, transform } => {
                if let Some(n) = self.nodes.get_mut(id) {
                    n.transform = transform.clone();
                }
            }
            Command::RenameNode { id, name } => {
                if let Some(n) = self.nodes.get_mut(id) {
                    n.name = name.clone();
                }
            }
            Command::SetVisibility { id, visible } => {
                if let Some(n) = self.nodes.get_mut(id) {
                    n.visible = *visible;
                }
            }
        }
        self.redo_stack.push(inverse.clone());
        Some(inverse)
    }

    pub fn redo(&mut self) -> Option<Command> {
        let cmd = self.redo_stack.pop()?;
        self.apply(cmd.clone());
        Some(cmd)
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_bus_apply_and_undo() {
        let mut bus = CommandBus::new();
        let node = SceneNode::new(1, "Edifício Zênite", NodeType::Building);
        bus.apply(Command::AddNode(node));

        assert_eq!(bus.nodes.len(), 1);
        assert_eq!(bus.revision, 1);

        bus.undo();
        assert_eq!(bus.nodes.len(), 0);
    }
}

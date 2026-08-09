//! Domínio CAD puro do ARCZ Designer.
//!
//! Este crate não conhece React, Cesium, Three.js nem Aedifex. Ele define a
//! representação editável e validada que pode ser consumida por qualquer UI.

use arcz_validation::{signed_area, validate_polygon, GeometryError, Point2};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

pub type NodeId = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Transform {
    pub position: [f64; 3],
    pub rotation: Vec<f64>,
    pub scale: [f64; 3],
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            rotation: vec![0.0; 3],
            scale: [1.0; 3],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CadNodeKind {
    Site,
    SiteBoundary,
    Building,
    Level,
    Wall,
    Door,
    Window,
    Opening,
    Zone,
    Slab,
    Ceiling,
    Roof,
    RoofSegment,
    Stair,
    StairSegment,
    Item,
    Fence,
    Column,
    Elevator,
    StructuralGrid,
    Measurement,
    Terrain,
    Extension(String),
}

impl CadNodeKind {
    /// Parse a persisted discriminator without discarding future/plugin kinds.
    pub fn from_name(value: &str) -> Self {
        match value {
            "site" => Self::Site,
            "site-boundary" => Self::SiteBoundary,
            "building" => Self::Building,
            "level" => Self::Level,
            "wall" => Self::Wall,
            "door" => Self::Door,
            "window" => Self::Window,
            "opening" => Self::Opening,
            "zone" => Self::Zone,
            "slab" => Self::Slab,
            "ceiling" => Self::Ceiling,
            "roof" => Self::Roof,
            "roof-segment" => Self::RoofSegment,
            "stair" => Self::Stair,
            "stair-segment" => Self::StairSegment,
            "item" => Self::Item,
            "fence" => Self::Fence,
            "column" => Self::Column,
            "elevator" => Self::Elevator,
            "structural-grid" => Self::StructuralGrid,
            "measurement" => Self::Measurement,
            "terrain" => Self::Terrain,
            other => Self::Extension(other.to_string()),
        }
    }

    pub fn as_name(&self) -> &str {
        match self {
            Self::Site => "site",
            Self::SiteBoundary => "site-boundary",
            Self::Building => "building",
            Self::Level => "level",
            Self::Wall => "wall",
            Self::Door => "door",
            Self::Window => "window",
            Self::Opening => "opening",
            Self::Zone => "zone",
            Self::Slab => "slab",
            Self::Ceiling => "ceiling",
            Self::Roof => "roof",
            Self::RoofSegment => "roof-segment",
            Self::Stair => "stair",
            Self::StairSegment => "stair-segment",
            Self::Item => "item",
            Self::Fence => "fence",
            Self::Column => "column",
            Self::Elevator => "elevator",
            Self::StructuralGrid => "structural-grid",
            Self::Measurement => "measurement",
            Self::Terrain => "terrain",
            Self::Extension(value) => value.as_str(),
        }
    }
}

impl Serialize for CadNodeKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_name())
    }
}

impl<'de> Deserialize<'de> for CadNodeKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(Self::from_name(&value))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CadNode {
    pub id: NodeId,
    pub kind: CadNodeKind,
    pub parent_id: Option<NodeId>,
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    #[serde(default)]
    pub transform: Transform,
    #[serde(default)]
    pub properties: Map<String, Value>,
    #[serde(default)]
    pub extension_data: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CadDocument {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub revision: u64,
    pub nodes: BTreeMap<NodeId, CadNode>,
    pub root_node_ids: Vec<NodeId>,
    #[serde(default)]
    pub materials: BTreeMap<String, Value>,
    #[serde(default)]
    pub metadata: Map<String, Value>,
}

impl CadDocument {
    pub fn validate(&self) -> Result<ValidationSummary, CadError> {
        if self.schema_version != 1 {
            return Err(CadError::UnsupportedSchema(self.schema_version));
        }
        if self.id.trim().is_empty() || self.name.trim().is_empty() {
            return Err(CadError::RequiredField("id/name"));
        }
        let actual_roots: BTreeSet<&String> = self
            .nodes
            .iter()
            .filter_map(|(id, node)| node.parent_id.is_none().then_some(id))
            .collect();
        let declared_roots: BTreeSet<&String> = self.root_node_ids.iter().collect();
        if actual_roots != declared_roots {
            return Err(CadError::RootMismatch);
        }
        let mut warnings = Vec::new();
        for (key, node) in &self.nodes {
            if key != &node.id {
                return Err(CadError::NodeKeyMismatch(key.clone()));
            }
            if let Some(parent) = &node.parent_id {
                if !self.nodes.contains_key(parent) {
                    return Err(CadError::MissingParent {
                        node: node.id.clone(),
                        parent: parent.clone(),
                    });
                }
            }
            validate_transform(node)?;
            validate_node_geometry(node, &self.nodes, &mut warnings)?;
            validate_ancestry(&node.id, &self.nodes)?;
        }
        Ok(ValidationSummary {
            node_count: self.nodes.len(),
            root_count: self.root_node_ids.len(),
            warnings,
        })
    }

    pub fn insert(&mut self, node: CadNode) -> Result<(), CadError> {
        if self.nodes.contains_key(&node.id) {
            return Err(CadError::DuplicateNode(node.id));
        }
        if let Some(parent) = &node.parent_id {
            if !self.nodes.contains_key(parent) {
                return Err(CadError::MissingParent {
                    node: node.id.clone(),
                    parent: parent.clone(),
                });
            }
        }
        let mut staged = self.clone();
        if node.parent_id.is_none() {
            staged.root_node_ids.push(node.id.clone());
        }
        staged.nodes.insert(node.id.clone(), node);
        staged.validate()?;
        staged.revision = self.revision.saturating_add(1);
        *self = staged;
        Ok(())
    }

    pub fn delete_cascade(&mut self, id: &str) -> Result<Vec<NodeId>, CadError> {
        if !self.nodes.contains_key(id) {
            return Err(CadError::NodeNotFound(id.to_string()));
        }
        let mut deleted = vec![id.to_string()];
        let mut index = 0;
        while index < deleted.len() {
            let parent = deleted[index].clone();
            deleted.extend(
                self.nodes
                    .values()
                    .filter(|node| node.parent_id.as_deref() == Some(parent.as_str()))
                    .map(|node| node.id.clone()),
            );
            index += 1;
        }
        let deleted_set: BTreeSet<&str> = deleted.iter().map(String::as_str).collect();
        let mut staged = self.clone();
        staged
            .nodes
            .retain(|node_id, _| !deleted_set.contains(node_id.as_str()));
        staged
            .root_node_ids
            .retain(|root| !deleted_set.contains(root.as_str()));
        staged.validate()?;
        staged.revision = self.revision.saturating_add(1);
        *self = staged;
        Ok(deleted)
    }

    pub fn create_room(
        &mut self,
        level_id: &str,
        room_id: &str,
        name: &str,
        polygon: Vec<Point2>,
        wall_height: f64,
        wall_thickness: f64,
    ) -> Result<RoomCreation, CadError> {
        let level = self
            .nodes
            .get(level_id)
            .ok_or_else(|| CadError::NodeNotFound(level_id.to_string()))?;
        if !matches!(&level.kind, CadNodeKind::Level) {
            return Err(CadError::WrongParentKind {
                node_kind: "zone".into(),
                expected: "level".into(),
            });
        }
        validate_polygon(&polygon, 0.01)?;
        if wall_height <= 0.0 || wall_thickness <= 0.0 {
            return Err(CadError::NonPositiveDimension);
        }
        let zone_id = room_id.to_string();
        let slab_id = format!("{room_id}:slab");
        let ceiling_id = format!("{room_id}:ceiling");
        let wall_ids: Vec<String> = (0..polygon.len())
            .map(|index| format!("{room_id}:wall:{index}"))
            .collect();
        let all_ids = [zone_id.clone(), slab_id.clone(), ceiling_id.clone()]
            .into_iter()
            .chain(wall_ids.iter().cloned())
            .collect::<Vec<_>>();
        if let Some(duplicate) = all_ids
            .iter()
            .find(|id| self.nodes.contains_key(id.as_str()))
        {
            return Err(CadError::DuplicateNode(duplicate.clone()));
        }
        let mut staged = self.clone();
        let polygon_value = serde_json::to_value(&polygon)?;
        for (id, kind, label) in [
            (&zone_id, CadNodeKind::Zone, name.to_string()),
            (&slab_id, CadNodeKind::Slab, format!("Laje {name}")),
            (&ceiling_id, CadNodeKind::Ceiling, format!("Forro {name}")),
        ] {
            let mut properties = Map::new();
            properties.insert("polygon".into(), polygon_value.clone());
            let node = CadNode {
                id: id.clone(),
                kind,
                parent_id: Some(level_id.to_string()),
                name: label,
                visible: true,
                locked: false,
                transform: Transform::default(),
                properties,
                extension_data: Map::new(),
            };
            staged.nodes.insert(id.clone(), node);
        }

        for (index, wall_id) in wall_ids.iter().enumerate() {
            let mut properties = Map::new();
            properties.insert("start".into(), serde_json::to_value(polygon[index])?);
            properties.insert(
                "end".into(),
                serde_json::to_value(polygon[(index + 1) % polygon.len()])?,
            );
            properties.insert("height".into(), Value::from(wall_height));
            properties.insert("thickness".into(), Value::from(wall_thickness));
            staged.nodes.insert(
                wall_id.clone(),
                CadNode {
                    id: wall_id.clone(),
                    kind: CadNodeKind::Wall,
                    parent_id: Some(level_id.to_string()),
                    name: format!("Parede {}", index + 1),
                    visible: true,
                    locked: false,
                    transform: Transform::default(),
                    properties,
                    extension_data: Map::new(),
                },
            );
        }
        staged.validate()?;
        staged.revision = self.revision.saturating_add(1);
        *self = staged;
        Ok(RoomCreation {
            zone_id,
            slab_id,
            ceiling_id,
            wall_ids,
            area_m2: signed_area(&polygon).abs(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoomCreation {
    pub zone_id: NodeId,
    pub slab_id: NodeId,
    pub ceiling_id: NodeId,
    pub wall_ids: Vec<NodeId>,
    pub area_m2: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValidationSummary {
    pub node_count: usize,
    pub root_count: usize,
    pub warnings: Vec<String>,
}

fn validate_transform(node: &CadNode) -> Result<(), CadError> {
    if node
        .transform
        .position
        .iter()
        .any(|value| !value.is_finite())
        || node
            .transform
            .rotation
            .iter()
            .any(|value| !value.is_finite())
        || node.transform.scale.iter().any(|value| !value.is_finite())
    {
        return Err(CadError::NonFinite(node.id.clone()));
    }
    if !matches!(node.transform.rotation.len(), 3 | 4) {
        return Err(CadError::RotationLength(node.id.clone()));
    }
    if node
        .transform
        .scale
        .iter()
        .any(|value| value.abs() < 1.0e-12)
    {
        return Err(CadError::ZeroScale(node.id.clone()));
    }
    Ok(())
}

fn point2(value: Option<&Value>, field: &str) -> Result<Point2, CadError> {
    let array = value
        .and_then(Value::as_array)
        .ok_or_else(|| CadError::InvalidGeometry(field.to_string()))?;
    if array.len() != 2 {
        return Err(CadError::InvalidGeometry(field.to_string()));
    }
    let x = array[0]
        .as_f64()
        .ok_or_else(|| CadError::InvalidGeometry(field.to_string()))?;
    let y = array[1]
        .as_f64()
        .ok_or_else(|| CadError::InvalidGeometry(field.to_string()))?;
    if !x.is_finite() || !y.is_finite() {
        return Err(CadError::NonFinite(field.to_string()));
    }
    Ok([x, y])
}

fn polygon(value: Option<&Value>, field: &str) -> Result<Vec<Point2>, CadError> {
    let rows = value
        .and_then(Value::as_array)
        .ok_or_else(|| CadError::InvalidGeometry(field.to_string()))?;
    rows.iter()
        .map(|row| point2(Some(row), field))
        .collect::<Result<Vec<_>, _>>()
}

fn positive(properties: &Map<String, Value>, field: &str, default: f64) -> Result<f64, CadError> {
    let value = properties
        .get(field)
        .and_then(Value::as_f64)
        .unwrap_or(default);
    if !value.is_finite() || value <= 0.0 {
        return Err(CadError::NonPositiveDimension);
    }
    Ok(value)
}

fn validate_node_geometry(
    node: &CadNode,
    nodes: &BTreeMap<NodeId, CadNode>,
    warnings: &mut Vec<String>,
) -> Result<(), CadError> {
    match &node.kind {
        CadNodeKind::Wall => {
            let start = point2(node.properties.get("start"), "wall.start")?;
            let end = point2(node.properties.get("end"), "wall.end")?;
            if (end[0] - start[0]).hypot(end[1] - start[1]) < 0.05 {
                return Err(CadError::WallTooShort(node.id.clone()));
            }
            positive(&node.properties, "height", 3.0)?;
            positive(&node.properties, "thickness", 0.15)?;
        }
        CadNodeKind::Door | CadNodeKind::Window => {
            let parent = node
                .parent_id
                .as_ref()
                .and_then(|id| nodes.get(id))
                .ok_or_else(|| CadError::MissingParent {
                    node: node.id.clone(),
                    parent: node.parent_id.clone().unwrap_or_default(),
                })?;
            if parent.kind != CadNodeKind::Wall {
                return Err(CadError::WrongParentKind {
                    node_kind: format!("{:?}", node.kind),
                    expected: "wall".into(),
                });
            }
            let t = node
                .properties
                .get("t")
                .and_then(Value::as_f64)
                .unwrap_or(0.5);
            if !(0.0..=1.0).contains(&t) {
                return Err(CadError::OpeningParameter(node.id.clone()));
            }
            positive(&node.properties, "width", 0.9)?;
            positive(&node.properties, "height", 2.1)?;
        }
        CadNodeKind::Zone
        | CadNodeKind::Slab
        | CadNodeKind::Ceiling
        | CadNodeKind::SiteBoundary
        | CadNodeKind::Opening => {
            let points = polygon(node.properties.get("polygon"), "polygon")?;
            validate_polygon(&points, 0.01)?;
        }
        CadNodeKind::Level => {
            positive(&node.properties, "height", 3.0)?;
        }
        CadNodeKind::Extension(name) => {
            warnings.push(format!(
                "extension node preserved without native validator: {name}"
            ));
        }
        _ => {}
    }
    Ok(())
}

fn validate_ancestry(id: &str, nodes: &BTreeMap<NodeId, CadNode>) -> Result<(), CadError> {
    let mut visited = BTreeSet::new();
    let mut current = Some(id);
    while let Some(node_id) = current {
        if !visited.insert(node_id.to_string()) {
            return Err(CadError::ParentCycle(id.to_string()));
        }
        current = nodes
            .get(node_id)
            .and_then(|node| node.parent_id.as_deref());
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum CadError {
    #[error("unsupported CAD schema version {0}")]
    UnsupportedSchema(u32),
    #[error("required field is missing: {0}")]
    RequiredField(&'static str),
    #[error("declared roots do not match root nodes")]
    RootMismatch,
    #[error("node key does not match node id: {0}")]
    NodeKeyMismatch(String),
    #[error("node already exists: {0}")]
    DuplicateNode(String),
    #[error("node not found: {0}")]
    NodeNotFound(String),
    #[error("parent {parent} for node {node} does not exist")]
    MissingParent { node: String, parent: String },
    #[error("parent cycle detected at {0}")]
    ParentCycle(String),
    #[error("non-finite value at {0}")]
    NonFinite(String),
    #[error("rotation must contain 3 Euler values or 4 quaternion values: {0}")]
    RotationLength(String),
    #[error("scale cannot contain zero: {0}")]
    ZeroScale(String),
    #[error("invalid geometry field: {0}")]
    InvalidGeometry(String),
    #[error("wall is shorter than 5 cm: {0}")]
    WallTooShort(String),
    #[error("dimension must be positive")]
    NonPositiveDimension,
    #[error("opening parameter t must be in [0,1]: {0}")]
    OpeningParameter(String),
    #[error("{node_kind} requires parent kind {expected}")]
    WrongParentKind { node_kind: String, expected: String },
    #[error(transparent)]
    Geometry(#[from] GeometryError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_document() -> CadDocument {
        let site = CadNode {
            id: "site".into(),
            kind: CadNodeKind::Site,
            parent_id: None,
            name: "Lote".into(),
            visible: true,
            locked: false,
            transform: Transform::default(),
            properties: Map::new(),
            extension_data: Map::new(),
        };
        let building = CadNode {
            id: "building".into(),
            kind: CadNodeKind::Building,
            parent_id: Some("site".into()),
            name: "Edificação".into(),
            visible: true,
            locked: false,
            transform: Transform::default(),
            properties: Map::new(),
            extension_data: Map::new(),
        };
        let mut level_properties = Map::new();
        level_properties.insert("height".into(), Value::from(3.0));
        let level = CadNode {
            id: "level".into(),
            kind: CadNodeKind::Level,
            parent_id: Some("building".into()),
            name: "Térreo".into(),
            visible: true,
            locked: false,
            transform: Transform::default(),
            properties: level_properties,
            extension_data: Map::new(),
        };
        CadDocument {
            schema_version: 1,
            id: "doc".into(),
            name: "Teste".into(),
            revision: 0,
            nodes: [
                ("site".into(), site),
                ("building".into(), building),
                ("level".into(), level),
            ]
            .into_iter()
            .collect(),
            root_node_ids: vec!["site".into()],
            materials: BTreeMap::new(),
            metadata: Map::new(),
        }
    }

    #[test]
    fn creates_room_atomically() {
        let mut document = base_document();
        let result = document
            .create_room(
                "level",
                "room-1",
                "Sala",
                vec![[0.0, 0.0], [5.0, 0.0], [5.0, 4.0], [0.0, 4.0]],
                3.0,
                0.15,
            )
            .unwrap();
        assert_eq!(result.wall_ids.len(), 4);
        assert_eq!(result.area_m2, 20.0);
        assert_eq!(document.revision, 1);
        assert!(document.validate().is_ok());
    }

    #[test]
    fn rejects_stale_invalid_geometry() {
        let mut document = base_document();
        let error = document
            .create_room(
                "level",
                "room-1",
                "Inválida",
                vec![[0.0, 0.0], [4.0, 4.0], [0.0, 4.0], [4.0, 0.0]],
                3.0,
                0.15,
            )
            .unwrap_err();
        assert!(matches!(error, CadError::Geometry(_)));
        assert_eq!(document.nodes.len(), 3);
    }
    #[test]
    fn extension_kind_serializes_as_plain_discriminator() {
        let kind = CadNodeKind::Extension("community:facade".into());
        assert_eq!(
            serde_json::to_value(&kind).unwrap(),
            Value::from("community:facade")
        );
        assert_eq!(
            serde_json::from_value::<CadNodeKind>(Value::from("community:facade")).unwrap(),
            kind
        );
    }

    #[test]
    fn failed_insert_does_not_mutate_document() {
        let mut document = base_document();
        let before = document.clone();
        let mut properties = Map::new();
        properties.insert("start".into(), serde_json::json!([0.0, 0.0]));
        properties.insert("end".into(), serde_json::json!([0.0, 0.0]));
        let result = document.insert(CadNode {
            id: "invalid-wall".into(),
            kind: CadNodeKind::Wall,
            parent_id: Some("level".into()),
            name: "Inválida".into(),
            visible: true,
            locked: false,
            transform: Transform::default(),
            properties,
            extension_data: Map::new(),
        });
        assert!(result.is_err());
        assert_eq!(document, before);
    }

    #[test]
    fn duplicate_room_identifiers_do_not_overwrite_existing_nodes() {
        let mut document = base_document();
        document
            .create_room(
                "level",
                "room-duplicate",
                "Sala",
                vec![[0.0, 0.0], [5.0, 0.0], [5.0, 4.0], [0.0, 4.0]],
                3.0,
                0.15,
            )
            .unwrap();
        let before = document.clone();
        let result = document.create_room(
            "level",
            "room-duplicate",
            "Outra",
            vec![[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]],
            3.0,
            0.15,
        );
        assert!(matches!(result, Err(CadError::DuplicateNode(_))));
        assert_eq!(document, before);
    }
}

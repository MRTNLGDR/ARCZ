//! Semântica BIM e quantitativos derivados do documento CAD.
//!
//! Quantitativos são artefatos recalculáveis. Eles nunca substituem geometria
//! nem se tornam a fonte primária do projeto.

use arcz_cad::{CadDocument, CadNode, CadNodeKind, NodeId};
use arcz_validation::{signed_area, Point2};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoomQuantity {
    pub node_id: NodeId,
    pub name: String,
    pub area_m2: f64,
    pub perimeter_m: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WallQuantity {
    pub node_id: NodeId,
    pub length_m: f64,
    pub height_m: f64,
    pub thickness_m: f64,
    pub gross_area_m2: f64,
    pub opening_area_m2: f64,
    pub net_area_m2: f64,
    pub volume_m3: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QuantityReport {
    pub source_revision: u64,
    pub rooms: Vec<RoomQuantity>,
    pub walls: Vec<WallQuantity>,
    pub totals: BTreeMap<String, f64>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CostRule {
    pub code: String,
    pub unit: CostUnit,
    pub unit_cost: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CostUnit {
    SquareMeterWall,
    CubicMeterWall,
    SquareMeterFloor,
    Unit,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CostItem {
    pub code: String,
    pub description: String,
    pub quantity: f64,
    pub unit: CostUnit,
    pub unit_cost: f64,
    pub total: f64,
    pub source_node_ids: Vec<NodeId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CostReport {
    pub source_revision: u64,
    pub currency: String,
    pub items: Vec<CostItem>,
    pub total: f64,
}

pub fn compute_quantities(document: &CadDocument) -> Result<QuantityReport, BimError> {
    document.validate()?;
    let mut rooms = Vec::new();
    let mut walls = Vec::new();
    let mut warnings = Vec::new();

    for node in document.nodes.values() {
        match &node.kind {
            CadNodeKind::Zone => rooms.push(room_quantity(node)?),
            CadNodeKind::Wall => walls.push(wall_quantity(node, document)?),
            CadNodeKind::Extension(kind) => warnings.push(format!(
                "no native quantity rule for extension node {} ({kind})",
                node.id
            )),
            _ => {}
        }
    }

    let totals = [
        ("floor_area_m2".to_string(), rooms.iter().map(|item| item.area_m2).sum()),
        ("wall_net_area_m2".to_string(), walls.iter().map(|item| item.net_area_m2).sum()),
        ("wall_volume_m3".to_string(), walls.iter().map(|item| item.volume_m3).sum()),
    ]
    .into_iter()
    .collect();

    Ok(QuantityReport {
        source_revision: document.revision,
        rooms,
        walls,
        totals,
        warnings,
    })
}

pub fn estimate_costs(
    report: &QuantityReport,
    rules: &[CostRule],
    currency: impl Into<String>,
) -> Result<CostReport, BimError> {
    let mut items = Vec::new();
    for rule in rules {
        if !rule.unit_cost.is_finite() || rule.unit_cost < 0.0 {
            return Err(BimError::InvalidUnitCost(rule.code.clone()));
        }
        let (quantity, description, nodes) = match rule.unit {
            CostUnit::SquareMeterWall => (
                report.walls.iter().map(|item| item.net_area_m2).sum(),
                "Área líquida de paredes".to_string(),
                report.walls.iter().map(|item| item.node_id.clone()).collect(),
            ),
            CostUnit::CubicMeterWall => (
                report.walls.iter().map(|item| item.volume_m3).sum(),
                "Volume de paredes".to_string(),
                report.walls.iter().map(|item| item.node_id.clone()).collect(),
            ),
            CostUnit::SquareMeterFloor => (
                report.rooms.iter().map(|item| item.area_m2).sum(),
                "Área de piso".to_string(),
                report.rooms.iter().map(|item| item.node_id.clone()).collect(),
            ),
            CostUnit::Unit => (
                1.0,
                "Item unitário".to_string(),
                Vec::new(),
            ),
        };
        items.push(CostItem {
            code: rule.code.clone(),
            description,
            quantity,
            unit: rule.unit,
            unit_cost: rule.unit_cost,
            total: quantity * rule.unit_cost,
            source_node_ids: nodes,
        });
    }
    let total = items.iter().map(|item| item.total).sum();
    Ok(CostReport {
        source_revision: report.source_revision,
        currency: currency.into(),
        items,
        total,
    })
}

fn points(node: &CadNode, field: &str) -> Result<Vec<Point2>, BimError> {
    let rows = node
        .properties
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| BimError::MissingGeometry(node.id.clone()))?;
    rows.iter()
        .map(|row| {
            let pair = row
                .as_array()
                .filter(|pair| pair.len() == 2)
                .ok_or_else(|| BimError::MissingGeometry(node.id.clone()))?;
            Ok([
                pair[0]
                    .as_f64()
                    .ok_or_else(|| BimError::MissingGeometry(node.id.clone()))?,
                pair[1]
                    .as_f64()
                    .ok_or_else(|| BimError::MissingGeometry(node.id.clone()))?,
            ])
        })
        .collect()
}

fn room_quantity(node: &CadNode) -> Result<RoomQuantity, BimError> {
    let polygon = points(node, "polygon")?;
    let perimeter_m = (0..polygon.len())
        .map(|index| {
            let a = polygon[index];
            let b = polygon[(index + 1) % polygon.len()];
            (b[0] - a[0]).hypot(b[1] - a[1])
        })
        .sum();
    Ok(RoomQuantity {
        node_id: node.id.clone(),
        name: node.name.clone(),
        area_m2: signed_area(&polygon).abs(),
        perimeter_m,
    })
}

fn wall_quantity(node: &CadNode, document: &CadDocument) -> Result<WallQuantity, BimError> {
    let start = points_from_single(node, "start")?;
    let end = points_from_single(node, "end")?;
    let length_m = (end[0] - start[0]).hypot(end[1] - start[1]);
    let height_m = number(node, "height", 3.0)?;
    let thickness_m = number(node, "thickness", 0.15)?;
    let opening_area_m2 = document
        .nodes
        .values()
        .filter(|child| child.parent_id.as_deref() == Some(node.id.as_str()))
        .filter(|child| matches!(&child.kind, CadNodeKind::Door | CadNodeKind::Window))
        .map(|child| Ok(number(child, "width", 0.9)? * number(child, "height", 2.1)?))
        .collect::<Result<Vec<f64>, BimError>>()?
        .into_iter()
        .sum::<f64>();
    let gross_area_m2 = length_m * height_m;
    let net_area_m2 = (gross_area_m2 - opening_area_m2).max(0.0);
    Ok(WallQuantity {
        node_id: node.id.clone(),
        length_m,
        height_m,
        thickness_m,
        gross_area_m2,
        opening_area_m2,
        net_area_m2,
        volume_m3: net_area_m2 * thickness_m,
    })
}

fn points_from_single(node: &CadNode, field: &str) -> Result<Point2, BimError> {
    let values = node
        .properties
        .get(field)
        .and_then(Value::as_array)
        .filter(|values| values.len() == 2)
        .ok_or_else(|| BimError::MissingGeometry(node.id.clone()))?;
    Ok([
        values[0]
            .as_f64()
            .ok_or_else(|| BimError::MissingGeometry(node.id.clone()))?,
        values[1]
            .as_f64()
            .ok_or_else(|| BimError::MissingGeometry(node.id.clone()))?,
    ])
}

fn number(node: &CadNode, field: &str, default: f64) -> Result<f64, BimError> {
    let value = node.properties.get(field).and_then(Value::as_f64).unwrap_or(default);
    if !value.is_finite() || value <= 0.0 {
        return Err(BimError::InvalidQuantity {
            node: node.id.clone(),
            field: field.to_string(),
        });
    }
    Ok(value)
}

#[derive(Debug, Error)]
pub enum BimError {
    #[error(transparent)]
    Cad(#[from] arcz_cad::CadError),
    #[error("missing geometry for {0}")]
    MissingGeometry(String),
    #[error("invalid quantity field {field} on {node}")]
    InvalidQuantity { node: String, field: String },
    #[error("invalid unit cost rule {0}")]
    InvalidUnitCost(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcz_cad::{CadNode, Transform};
    use serde_json::{Map, Value};

    #[test]
    fn subtracts_openings_from_wall_area() {
        let mut wall_props = Map::new();
        wall_props.insert("start".into(), serde_json::json!([0.0, 0.0]));
        wall_props.insert("end".into(), serde_json::json!([5.0, 0.0]));
        wall_props.insert("height".into(), Value::from(3.0));
        wall_props.insert("thickness".into(), Value::from(0.2));
        let wall = CadNode {
            id: "wall".into(),
            kind: CadNodeKind::Wall,
            parent_id: Some("level".into()),
            name: "Parede".into(),
            visible: true,
            locked: false,
            transform: Transform::default(),
            properties: wall_props,
            extension_data: Map::new(),
        };
        let mut door_props = Map::new();
        door_props.insert("t".into(), Value::from(0.5));
        door_props.insert("width".into(), Value::from(1.0));
        door_props.insert("height".into(), Value::from(2.1));
        let door = CadNode {
            id: "door".into(),
            kind: CadNodeKind::Door,
            parent_id: Some("wall".into()),
            name: "Porta".into(),
            visible: true,
            locked: false,
            transform: Transform::default(),
            properties: door_props,
            extension_data: Map::new(),
        };
        let quantity = wall_quantity(
            &wall,
            &CadDocument {
                schema_version: 1,
                id: "doc".into(),
                name: "doc".into(),
                revision: 1,
                nodes: [("wall".into(), wall.clone()), ("door".into(), door)]
                    .into_iter()
                    .collect(),
                root_node_ids: vec![],
                materials: BTreeMap::new(),
                metadata: Map::new(),
            },
        )
        .unwrap();
        assert!((quantity.net_area_m2 - 12.9).abs() < 1.0e-9);
    }
}

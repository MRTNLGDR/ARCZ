//! Adaptador loss-aware entre o envelope Aedifex e o documento CAD ARCZ.
//!
//! Campos desconhecidos de plugins são preservados em `extension_data`; o
//! adaptador nunca reduz uma cena editável a uma malha derivada.

use arcz_cad::{CadDocument, CadNode, CadNodeKind, Transform};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeoAnchor {
    pub origin_wgs84: [f64; 3],
    pub north_rotation_deg: f64,
    pub region_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImportResult {
    pub document: CadDocument,
    pub source_hash: String,
    pub warnings: Vec<String>,
}

pub fn import_scene(
    source: &Value,
    scene_id: impl Into<String>,
    anchor: Option<GeoAnchor>,
) -> Result<ImportResult, AedifexError> {
    let object = source.as_object().ok_or(AedifexError::SceneNotObject)?;
    let source_nodes = object
        .get("nodes")
        .and_then(Value::as_object)
        .ok_or(AedifexError::NodesNotObject)?;
    let mut nodes = BTreeMap::new();
    let mut warnings = Vec::new();
    for (key, value) in source_nodes {
        let native = value
            .as_object()
            .ok_or_else(|| AedifexError::NodeNotObject(key.clone()))?;
        let id = native
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or(key)
            .to_string();
        if nodes.contains_key(&id) {
            return Err(AedifexError::DuplicateNode(id));
        }
        let kind_text = native
            .get("type")
            .or_else(|| native.get("kind"))
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let kind = CadNodeKind::from_name(kind_text);
        if matches!(&kind, CadNodeKind::Extension(_)) {
            warnings.push(format!("preserved plugin node kind {kind_text}: {id}"));
        }
        let parent_id = native
            .get("parentId")
            .or_else(|| native.get("parent_id"))
            .and_then(Value::as_str)
            .map(ToString::to_string);
        let position = vector3(native.get("position"), [0.0; 3])?;
        let rotation = vector(native.get("rotation"), vec![0.0; 3])?;
        let scale = vector3(native.get("scale"), [1.0; 3])?;
        let mut properties = Map::new();
        for (field, value) in native {
            if ![
                "id", "type", "kind", "parentId", "parent_id", "name", "label", "visible",
                "locked", "position", "rotation", "scale", "transform",
            ]
            .contains(&field.as_str())
            {
                properties.insert(field.clone(), value.clone());
            }
        }
        nodes.insert(
            id.clone(),
            CadNode {
                id,
                kind,
                parent_id,
                name: native
                    .get("name")
                    .or_else(|| native.get("label"))
                    .and_then(Value::as_str)
                    .unwrap_or(kind_text)
                    .to_string(),
                visible: native.get("visible").and_then(Value::as_bool).unwrap_or(true),
                locked: native.get("locked").and_then(Value::as_bool).unwrap_or(false),
                transform: Transform {
                    position,
                    rotation,
                    scale,
                },
                properties,
                extension_data: [("aedifex".to_string(), Value::Object(native.clone()))]
                    .into_iter()
                    .collect(),
            },
        );
    }
    let roots = object
        .get("rootNodeIds")
        .or_else(|| object.get("root_node_ids"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_else(|| {
            nodes
                .values()
                .filter(|node| node.parent_id.is_none())
                .map(|node| node.id.clone())
                .collect()
        });
    let source_hash = hash_value(source)?;
    let mut metadata = Map::new();
    metadata.insert("source_format".into(), Value::from("aedifex"));
    metadata.insert("source_hash".into(), Value::from(source_hash.clone()));
    metadata.insert(
        "compat_aedifex_scene".into(),
        Value::Object(
            object
                .iter()
                .filter(|(key, _)| key.as_str() != "nodes")
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        ),
    );
    if let Some(anchor) = anchor {
        metadata.insert("geo_anchor".into(), serde_json::to_value(anchor)?);
    }
    let document = CadDocument {
        schema_version: 1,
        id: scene_id.into(),
        name: object
            .get("name")
            .or_else(|| object.get("title"))
            .and_then(Value::as_str)
            .unwrap_or("Cena Aedifex")
            .to_string(),
        revision: 0,
        nodes,
        root_node_ids: roots,
        materials: object
            .get("sceneMaterials")
            .or_else(|| object.get("materials"))
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect(),
        metadata,
    };
    document.validate()?;
    Ok(ImportResult {
        document,
        source_hash,
        warnings,
    })
}

pub fn export_scene(document: &CadDocument) -> Result<Value, AedifexError> {
    document.validate()?;
    let mut output = document
        .metadata
        .get("compat_aedifex_scene")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut nodes = Map::new();
    for node in document.nodes.values() {
        let mut native = node
            .extension_data
            .get("aedifex")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        native.insert("id".into(), Value::from(node.id.clone()));
        native.insert("type".into(), Value::from(node.kind.as_name()));
        match &node.parent_id {
            Some(parent) => {
                native.insert("parentId".into(), Value::from(parent.clone()));
            }
            None => {
                native.remove("parentId");
            }
        }
        native.insert("name".into(), Value::from(node.name.clone()));
        native.insert("visible".into(), Value::from(node.visible));
        native.insert("locked".into(), Value::from(node.locked));
        native.insert("position".into(), serde_json::to_value(node.transform.position)?);
        native.insert("rotation".into(), serde_json::to_value(&node.transform.rotation)?);
        native.insert("scale".into(), serde_json::to_value(node.transform.scale)?);
        for (key, value) in &node.properties {
            native.insert(key.clone(), value.clone());
        }
        nodes.insert(node.id.clone(), Value::Object(native));
    }
    output.insert("nodes".into(), Value::Object(nodes));
    output.insert(
        "rootNodeIds".into(),
        serde_json::to_value(&document.root_node_ids)?,
    );
    output.insert(
        "sceneMaterials".into(),
        Value::Object(document.materials.clone().into_iter().collect()),
    );
    output.insert("name".into(), Value::from(document.name.clone()));
    Ok(Value::Object(output))
}

fn vector3(value: Option<&Value>, default: [f64; 3]) -> Result<[f64; 3], AedifexError> {
    let Some(values) = value.and_then(Value::as_array) else {
        return Ok(default);
    };
    if values.len() != 3 {
        return Err(AedifexError::VectorLength);
    }
    let mut output = [0.0; 3];
    for (index, value) in values.iter().enumerate() {
        output[index] = value.as_f64().ok_or(AedifexError::VectorNumber)?;
    }
    Ok(output)
}

fn vector(value: Option<&Value>, default: Vec<f64>) -> Result<Vec<f64>, AedifexError> {
    let Some(values) = value.and_then(Value::as_array) else {
        return Ok(default);
    };
    if !matches!(values.len(), 3 | 4) {
        return Err(AedifexError::VectorLength);
    }
    values
        .iter()
        .map(|value| value.as_f64().ok_or(AedifexError::VectorNumber))
        .collect()
}

fn hash_value(value: &Value) -> Result<String, AedifexError> {
    let raw = serde_json::to_vec(value)?;
    let mut digest = Sha256::new();
    digest.update(raw);
    Ok(format!("{:x}", digest.finalize()))
}

#[derive(Debug, Error)]
pub enum AedifexError {
    #[error("Aedifex scene is not an object")]
    SceneNotObject,
    #[error("Aedifex nodes is not an object")]
    NodesNotObject,
    #[error("Aedifex node is not an object: {0}")]
    NodeNotObject(String),
    #[error("duplicate node: {0}")]
    DuplicateNode(String),
    #[error("vector length is invalid")]
    VectorLength,
    #[error("vector contains a non-number")]
    VectorNumber,
    #[error(transparent)]
    Cad(#[from] arcz_cad::CadError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_payload_roundtrips_without_loss() {
        let source = serde_json::json!({
            "name": "Teste",
            "nodes": {
                "site": {"id":"site","type":"site"},
                "plugin": {
                    "id":"plugin","type":"community:facade","parentId":"site",
                    "pluginPayload":{"louvers":[1,2,3]},"position":[0,0,0]
                }
            },
            "rootNodeIds":["site"],
            "futureField":{"preserve":true}
        });
        let imported = import_scene(&source, "scene", None).unwrap();
        let exported = export_scene(&imported.document).unwrap();
        assert_eq!(exported["futureField"], source["futureField"]);
        assert_eq!(
            exported["nodes"]["plugin"]["pluginPayload"],
            source["nodes"]["plugin"]["pluginPayload"]
        );
    }
}

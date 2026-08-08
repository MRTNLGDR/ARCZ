//! Planet-to-object world authority contracts for ARCZ.

pub mod pipeline;
//!
//! ARCZ never treats the globe renderer as the canonical editable scene. This
//! crate owns the scale hierarchy, world-layer policy and streaming budgets that
//! let one project grow from a chair or parcel to a city, country or planet.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use thiserror::Error;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum WorldScope {
    Object,
    Parcel,
    Block,
    Neighborhood,
    City,
    State,
    Country,
    Continent,
    Planet,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct GeoAnchor {
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    pub altitude_m: f64,
    pub true_north_deg: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CoordinateFrame {
    Wgs84,
    Ecef,
    LocalEnu,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct WorldCellId {
    pub level: u8,
    pub x: u64,
    pub y: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorldLayerKind {
    Terrain,
    Imagery,
    Parcels,
    Buildings,
    Roads,
    Rail,
    Transit,
    Hydrology,
    Vegetation,
    Utilities,
    Atmosphere,
    Simulation,
    Authoring,
    Analysis,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LayerMutability {
    ReadOnlySource,
    DerivedReadOnly,
    CanonicalEditable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorldLayerPolicy {
    pub id: String,
    pub kind: WorldLayerKind,
    pub mutability: LayerMutability,
    pub min_lod: u8,
    pub max_lod: u8,
    pub provenance_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StreamBudget {
    pub max_resident_cells: u32,
    pub max_geometry_bytes: u64,
    pub max_texture_bytes: u64,
    pub max_gpu_bytes: u64,
    pub max_concurrent_jobs: u16,
}

impl Default for StreamBudget {
    fn default() -> Self {
        Self {
            max_resident_cells: 512,
            max_geometry_bytes: 2 * 1024 * 1024 * 1024,
            max_texture_bytes: 4 * 1024 * 1024 * 1024,
            max_gpu_bytes: 8 * 1024 * 1024 * 1024,
            max_concurrent_jobs: 8,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LockedWorldProject {
    pub project_id: String,
    pub scope: WorldScope,
    pub anchor: GeoAnchor,
    pub frame: CoordinateFrame,
    pub source_revision: u64,
    pub selected_cell_ids: Vec<WorldCellId>,
    pub layers: Vec<WorldLayerPolicy>,
    pub budget: StreamBudget,
}

pub fn validate_world_project(project: &LockedWorldProject) -> Result<(), WorldContractError> {
    if project.project_id.trim().is_empty() {
        return Err(WorldContractError::MissingProjectId);
    }
    if !(-90.0..=90.0).contains(&project.anchor.latitude_deg)
        || !(-180.0..=180.0).contains(&project.anchor.longitude_deg)
    {
        return Err(WorldContractError::InvalidAnchor);
    }
    if project.budget.max_resident_cells == 0 || project.budget.max_concurrent_jobs == 0 {
        return Err(WorldContractError::InvalidBudget);
    }

    let mut ids = BTreeSet::new();
    let mut editable_authoring_layers = 0usize;
    for layer in &project.layers {
        if layer.id.trim().is_empty() || !ids.insert(layer.id.clone()) {
            return Err(WorldContractError::DuplicateOrEmptyLayer(layer.id.clone()));
        }
        if layer.min_lod > layer.max_lod {
            return Err(WorldContractError::InvalidLodRange(layer.id.clone()));
        }
        if matches!(layer.mutability, LayerMutability::CanonicalEditable) {
            if !matches!(layer.kind, WorldLayerKind::Authoring) {
                return Err(WorldContractError::EditableSourceLayer(layer.id.clone()));
            }
            editable_authoring_layers += 1;
        }
    }
    if editable_authoring_layers > 1 {
        return Err(WorldContractError::MultipleCanonicalLayers);
    }
    Ok(())
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum WorldContractError {
    #[error("project id is required")]
    MissingProjectId,
    #[error("geographic anchor is outside WGS84 bounds")]
    InvalidAnchor,
    #[error("stream budget must be non-zero")]
    InvalidBudget,
    #[error("world layer id is empty or duplicated: {0}")]
    DuplicateOrEmptyLayer(String),
    #[error("invalid LOD range for layer {0}")]
    InvalidLodRange(String),
    #[error("only the ARCZ authoring layer may be canonical/editable: {0}")]
    EditableSourceLayer(String),
    #[error("a project may have at most one canonical editable authoring layer")]
    MultipleCanonicalLayers,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_editable_source_layers() {
        let project = LockedWorldProject {
            project_id: "p".into(),
            scope: WorldScope::City,
            anchor: GeoAnchor { latitude_deg: 0.0, longitude_deg: 0.0, altitude_m: 0.0, true_north_deg: 0.0 },
            frame: CoordinateFrame::LocalEnu,
            source_revision: 1,
            selected_cell_ids: vec![],
            layers: vec![WorldLayerPolicy {
                id: "terrain".into(), kind: WorldLayerKind::Terrain,
                mutability: LayerMutability::CanonicalEditable,
                min_lod: 0, max_lod: 18, provenance_required: true,
            }],
            budget: StreamBudget::default(),
        };
        assert!(matches!(validate_world_project(&project), Err(WorldContractError::EditableSourceLayer(_))));
    }
}

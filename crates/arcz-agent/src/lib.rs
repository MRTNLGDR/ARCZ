//! Provider-agnostic ARCZ agent action planning.
//! Models propose typed operations; only validated tool calls may mutate projects.

use arcz_plugin_sdk::ToolRequest;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScopeKind { Object, Parcel, Block, Neighborhood, City, State, Country, Continent, Planet }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReferenceAsset {
    pub asset_id: String,
    pub role: String,
    pub sha256: String,
    #[serde(default)] pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentIntent {
    pub request: String,
    pub project_id: String,
    pub expected_revision: u64,
    pub scope: ScopeKind,
    #[serde(default)] pub references: Vec<ReferenceAsset>,
    #[serde(default)] pub constraints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlannedAction {
    pub id: String,
    pub plugin_id: String,
    pub request: ToolRequest,
    #[serde(default)] pub depends_on: Vec<String>,
    pub risk: ActionRisk,
    pub requires_approval: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionRisk { ReadOnly, ReversibleEdit, DestructiveEdit, ExternalEffect }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActionPlan { pub intent: AgentIntent, pub actions: Vec<PlannedAction> }

pub fn validate_plan(plan: &ActionPlan) -> Result<(), AgentPlanError> {
    if plan.intent.request.trim().is_empty() || plan.intent.project_id.trim().is_empty() {
        return Err(AgentPlanError::MissingIntent);
    }
    let mut ids = BTreeSet::new();
    for action in &plan.actions {
        if action.id.trim().is_empty() || action.plugin_id.trim().is_empty() {
            return Err(AgentPlanError::MissingActionIdentity);
        }
        if !ids.insert(action.id.clone()) { return Err(AgentPlanError::DuplicateAction(action.id.clone())); }
        if action.request.project_id != plan.intent.project_id || action.request.expected_revision < plan.intent.expected_revision {
            return Err(AgentPlanError::RevisionOrProjectMismatch(action.id.clone()));
        }
        if !matches!(action.risk, ActionRisk::ReadOnly) && !action.request.dry_run && !action.requires_approval {
            return Err(AgentPlanError::UnsafeMutation(action.id.clone()));
        }
    }
    for action in &plan.actions {
        for dependency in &action.depends_on {
            if !ids.contains(dependency) || dependency == &action.id {
                return Err(AgentPlanError::InvalidDependency { action: action.id.clone(), dependency: dependency.clone() });
            }
        }
    }
    Ok(())
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AgentPlanError {
    #[error("agent intent and project are required")] MissingIntent,
    #[error("action id and plugin id are required")] MissingActionIdentity,
    #[error("duplicate action {0}")] DuplicateAction(String),
    #[error("project/revision mismatch in action {0}")] RevisionOrProjectMismatch(String),
    #[error("unsafe mutation without dry-run or approval: {0}")] UnsafeMutation(String),
    #[error("invalid dependency {dependency} for action {action}")] InvalidDependency { action: String, dependency: String },
}

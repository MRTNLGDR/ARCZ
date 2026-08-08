//! ARCZ plugin ABI/domain contracts.
//! Plugins never own the canonical scene.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use thiserror::Error;

pub const PLUGIN_API_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeKind { BuiltinRust, WasmComponent, LocalProcess, WebSidecar }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Capability(pub String);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_version: u32,
    pub runtime: RuntimeKind,
    pub entrypoint: String,
    #[serde(default)] pub capabilities: Vec<Capability>,
    #[serde(default)] pub reads: Vec<String>,
    #[serde(default)] pub writes: Vec<String>,
    #[serde(default)] pub network: NetworkPolicy,
    #[serde(default)] pub deterministic: bool,
    #[serde(default)] pub gpu_optional: bool,
    #[serde(default)] pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum NetworkPolicy { #[default] None, Localhost, Lan, ExplicitRemote }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolRequest {
    pub tool: String,
    pub project_id: String,
    pub expected_revision: u64,
    pub payload: Value,
    #[serde(default)] pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolResult {
    pub tool: String,
    pub source_revision: u64,
    pub resulting_revision: Option<u64>,
    pub changed_node_ids: Vec<String>,
    pub artifacts: Vec<ArtifactRef>,
    pub diagnostics: Vec<Diagnostic>,
    pub output: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactRef { pub kind: String, pub uri: String, pub sha256: String }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Diagnostic { pub severity: String, pub code: String, pub message: String }

pub fn validate_manifest(manifest: &PluginManifest) -> Result<(), PluginContractError> {
    if manifest.id.trim().is_empty() || manifest.name.trim().is_empty() || manifest.version.trim().is_empty() {
        return Err(PluginContractError::MissingIdentity);
    }
    if manifest.api_version != PLUGIN_API_VERSION { return Err(PluginContractError::UnsupportedApi(manifest.api_version)); }
    if manifest.entrypoint.trim().is_empty() { return Err(PluginContractError::MissingEntrypoint); }
    let mut seen = BTreeSet::new();
    for capability in &manifest.capabilities {
        let value = capability.0.trim();
        if value.is_empty() { return Err(PluginContractError::EmptyCapability); }
        if !seen.insert(value.to_owned()) { return Err(PluginContractError::DuplicateCapability(value.to_owned())); }
    }
    if matches!(manifest.network, NetworkPolicy::ExplicitRemote)
        && !manifest.capabilities.iter().any(|capability| capability.0 == "network.remote") {
        return Err(PluginContractError::RemoteNetworkWithoutCapability);
    }
    Ok(())
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PluginContractError {
    #[error("plugin identity fields are required")] MissingIdentity,
    #[error("unsupported plugin API version {0}")] UnsupportedApi(u32),
    #[error("plugin entrypoint is required")] MissingEntrypoint,
    #[error("plugin capability must not be empty")] EmptyCapability,
    #[error("duplicate plugin capability {0}")] DuplicateCapability(String),
    #[error("explicit remote network requires network.remote capability")] RemoteNetworkWithoutCapability,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_implicit_remote_network() {
        let manifest = PluginManifest {
            id: "x".into(), name: "X".into(), version: "0.1.0".into(), api_version: PLUGIN_API_VERSION,
            runtime: RuntimeKind::LocalProcess, entrypoint: "x".into(), capabilities: vec![], reads: vec![], writes: vec![],
            network: NetworkPolicy::ExplicitRemote, deterministic: false, gpu_optional: true, metadata: Value::Null,
        };
        assert_eq!(validate_manifest(&manifest), Err(PluginContractError::RemoteNetworkWithoutCapability));
    }
}

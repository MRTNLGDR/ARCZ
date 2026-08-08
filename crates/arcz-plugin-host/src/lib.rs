//! Capability-indexed plugin registry and permission gate for ARCZ.
//!
//! The host is intentionally independent from any concrete loader. Native Rust,
//! WASM components, local workers and web sidecars all register through the same
//! contract, while scene mutation remains revision-guarded by the ARCZ kernel.

use arcz_plugin_sdk::{validate_manifest, Capability, PluginContractError, PluginManifest};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PermissionGrant {
    #[serde(default)]
    pub capabilities: BTreeSet<String>,
    #[serde(default)]
    pub readable_domains: BTreeSet<String>,
    #[serde(default)]
    pub writable_domains: BTreeSet<String>,
}

#[derive(Debug, Default)]
pub struct PluginRegistry {
    manifests: BTreeMap<String, PluginManifest>,
    capability_index: BTreeMap<String, BTreeSet<String>>,
}

impl PluginRegistry {
    pub fn register(&mut self, manifest: PluginManifest) -> Result<(), PluginHostError> {
        validate_manifest(&manifest)?;
        if self.manifests.contains_key(&manifest.id) {
            return Err(PluginHostError::DuplicatePlugin(manifest.id));
        }
        let id = manifest.id.clone();
        for Capability(capability) in &manifest.capabilities {
            self.capability_index
                .entry(capability.clone())
                .or_default()
                .insert(id.clone());
        }
        self.manifests.insert(id, manifest);
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&PluginManifest> {
        self.manifests.get(id)
    }

    pub fn providers_for(&self, capability: &str) -> Vec<&PluginManifest> {
        self.capability_index
            .get(capability)
            .into_iter()
            .flatten()
            .filter_map(|id| self.manifests.get(id))
            .collect()
    }

    pub fn authorize(
        &self,
        plugin_id: &str,
        requested_capability: &str,
        grant: &PermissionGrant,
    ) -> Result<(), PluginHostError> {
        let plugin = self
            .manifests
            .get(plugin_id)
            .ok_or_else(|| PluginHostError::UnknownPlugin(plugin_id.into()))?;
        if !plugin
            .capabilities
            .iter()
            .any(|cap| cap.0 == requested_capability)
        {
            return Err(PluginHostError::CapabilityNotDeclared {
                plugin: plugin_id.into(),
                capability: requested_capability.into(),
            });
        }
        if !grant.capabilities.contains(requested_capability) {
            return Err(PluginHostError::CapabilityNotGranted {
                plugin: plugin_id.into(),
                capability: requested_capability.into(),
            });
        }
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.manifests.len()
    }
    pub fn is_empty(&self) -> bool {
        self.manifests.is_empty()
    }
}

#[derive(Debug, Error)]
pub enum PluginHostError {
    #[error(transparent)]
    InvalidManifest(#[from] PluginContractError),
    #[error("plugin already registered: {0}")]
    DuplicatePlugin(String),
    #[error("unknown plugin: {0}")]
    UnknownPlugin(String),
    #[error("plugin {plugin} did not declare capability {capability}")]
    CapabilityNotDeclared { plugin: String, capability: String },
    #[error("plugin {plugin} was not granted capability {capability}")]
    CapabilityNotGranted { plugin: String, capability: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcz_plugin_sdk::{NetworkPolicy, RuntimeKind, PLUGIN_API_VERSION};
    use serde_json::Value;

    fn manifest() -> PluginManifest {
        PluginManifest {
            id: "roads".into(),
            name: "Roads".into(),
            version: "0.1.0".into(),
            api_version: PLUGIN_API_VERSION,
            runtime: RuntimeKind::BuiltinRust,
            entrypoint: "builtin://roads".into(),
            capabilities: vec![Capability("road.generate".into())],
            reads: vec![],
            writes: vec![],
            network: NetworkPolicy::None,
            deterministic: true,
            gpu_optional: true,
            metadata: Value::Null,
        }
    }

    #[test]
    fn indexes_and_authorizes_declared_capability() {
        let mut registry = PluginRegistry::default();
        registry.register(manifest()).unwrap();
        let mut grant = PermissionGrant::default();
        grant.capabilities.insert("road.generate".into());
        assert_eq!(registry.providers_for("road.generate").len(), 1);
        assert!(registry.authorize("roads", "road.generate", &grant).is_ok());
    }
}

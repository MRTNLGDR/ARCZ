//! Archviz & PBR Material Worker: gerenciamento de biblioteca de blocos 3D e materiais PBR.
//!
//! Gerencia biblioteca de mobiliário, vegetação, iluminação, veículos e materiais PBR
//! (Albedo, Normal, Roughness, Metallic, Ambient Occlusion).
//! Instancia elementos na cena como nós autoritativos `SceneNode` com suporte a override de materiais.

use crate::cena::{SceneNode, NodeType, NodeConfidence, Transform64};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ArchvizCategory {
    Furniture,
    Vegetation,
    Light,
    Vehicle,
    PbrMaterial,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PbrMaterial {
    pub id: String,
    pub name: String,
    pub albedo_map: Option<String>,
    pub normal_map: Option<String>,
    pub roughness_metallic_map: Option<String>,
    pub base_color: [f32; 4],
    pub roughness: f32,
    pub metallic: f32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InstantiateAssetRequest {
    pub asset_id: String,
    pub name: String,
    pub category: ArchvizCategory,
    pub position: [f64; 3],
    pub rotation_euler: [f64; 3],
    pub scale: [f64; 3],
    pub material_overrides: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InstantiateAssetResult {
    pub node: SceneNode,
    pub materials_applied: Vec<PbrMaterial>,
}

pub struct ArchvizWorker;

impl ArchvizWorker {
    pub fn novo() -> Self {
        Self
    }

    /// Instancia um bloco ou ativo da biblioteca Archviz como no SceneNode autoritativo.
    pub fn instanciar_asset(&self, req: InstantiateAssetRequest) -> anyhow::Result<InstantiateAssetResult> {
        let node_id = format!("archviz_{}_{}", req.asset_id, std::process::id());

        let node_type = match req.category {
            ArchvizCategory::Furniture => NodeType::Furniture,
            ArchvizCategory::Vegetation => NodeType::Vegetation,
            ArchvizCategory::Light => NodeType::Light,
            ArchvizCategory::Vehicle => NodeType::Vehicle,
            ArchvizCategory::PbrMaterial => NodeType::GenericModel,
        };

        let mut node = SceneNode::novo(node_id, req.name, node_type);
        node.confidence = NodeConfidence::Observed; // GREEN badge (ativo da biblioteca padrão)
        node.layer = format!("Archviz/{:?}", req.category);
        node.source = "ArchvizLibrary".to_string();
        node.asset_ref = Some(format!("library/{}.glb", req.asset_id));
        node.material_refs = req.material_overrides.clone();

        node.transform = Transform64 {
            position: req.position,
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: req.scale,
        };

        node.metadata = serde_json::json!({
            "asset_id": req.asset_id,
            "category": format!("{:?}", req.category),
            "rotation_euler": req.rotation_euler,
        });

        // Materiais PBR associados
        let materials_applied = vec![PbrMaterial {
            id: "mat_pbr_default".to_string(),
            name: "Material PBR Padrão".to_string(),
            albedo_map: Some("textures/wood_albedo.png".to_string()),
            normal_map: Some("textures/wood_normal.png".to_string()),
            roughness_metallic_map: Some("textures/wood_arm.png".to_string()),
            base_color: [1.0, 1.0, 1.0, 1.0],
            roughness: 0.4,
            metallic: 0.0,
        }];

        Ok(InstantiateAssetResult {
            node,
            materials_applied,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instancia_objeto_da_biblioteca_archviz_com_confianca_verde() {
        let worker = ArchvizWorker::novo();
        let req = InstantiateAssetRequest {
            asset_id: "cadeira_design_01".to_string(),
            name: "Cadeira Eames".to_string(),
            category: ArchvizCategory::Furniture,
            position: [2.0, 3.0, 0.0],
            rotation_euler: [0.0, 0.0, 90.0],
            scale: [1.0, 1.0, 1.0],
            material_overrides: vec!["mat_couro_preto".to_string()],
        };

        let result = worker.instanciar_asset(req).unwrap();

        assert_eq!(result.node.confidence, NodeConfidence::Observed);
        assert_eq!(result.node.confidence.color_code(), "GREEN");
        assert_eq!(result.node.node_type, NodeType::Furniture);
        assert_eq!(result.node.material_refs, vec!["mat_couro_preto".to_string()]);
        assert_eq!(result.materials_applied.len(), 1);
    }
}

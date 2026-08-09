//! Archviz & PBR Material Worker.
//!
//! Nada aqui cria asset/material placeholder. Um item só entra na cena quando o
//! arquivo GLTF/GLB existe na biblioteca local e abre como glTF válido. Overrides
//! PBR são carregados de manifests JSON locais; mapas declarados precisam existir
//! dentro da raiz da biblioteca. Ausência é erro explícito.

use crate::cena::{NodeConfidence, NodeType, SceneNode, Transform64};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};

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
    #[serde(default)]
    pub ambient_occlusion_map: Option<String>,
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

pub struct ArchvizWorker {
    library_dir: PathBuf,
}

impl ArchvizWorker {
    /// Biblioteca padrão é local ao ARCZ. `ARCZ_ARCHVIZ_LIBRARY` pode apontar
    /// para outro diretório local explicitamente configurado.
    pub fn novo() -> Self {
        let root = std::env::var_os("ARCZ_ARCHVIZ_LIBRARY")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("resources/archviz"));
        Self { library_dir: root }
    }

    pub fn com_raiz<P: AsRef<Path>>(library_dir: P) -> Self {
        Self {
            library_dir: library_dir.as_ref().to_path_buf(),
        }
    }

    pub fn instanciar_asset(
        &self,
        req: InstantiateAssetRequest,
    ) -> anyhow::Result<InstantiateAssetResult> {
        validate_identifier(&req.asset_id, "asset_id")?;
        for material in &req.material_overrides {
            validate_identifier(material, "material override")?;
        }
        if !req.position.iter().chain(req.scale.iter()).chain(req.rotation_euler.iter()).all(|v| v.is_finite()) {
            anyhow::bail!("transform do asset contém valor não finito");
        }
        if req.scale.iter().any(|value| *value == 0.0) {
            anyhow::bail!("escala zero não é aceita para asset Archviz");
        }

        let asset_path = self.find_asset(&req.asset_id)?;
        // Parse real do container glTF/GLB. Isso detecta arquivo vazio/corrompido
        // sem depender de renderer ou rede.
        gltf::Gltf::open(&asset_path).map_err(|e| {
            anyhow::anyhow!("asset Archviz inválido '{}': {e}", asset_path.display())
        })?;
        let asset_meta = std::fs::metadata(&asset_path)?;
        let asset_hash = sha256_file(&asset_path)?;

        let mut materials_applied = Vec::new();
        for material_id in &req.material_overrides {
            materials_applied.push(self.load_material(material_id)?);
        }

        let node_type = match req.category {
            ArchvizCategory::Furniture => NodeType::Furniture,
            ArchvizCategory::Vegetation => NodeType::Vegetation,
            ArchvizCategory::Light => NodeType::Light,
            ArchvizCategory::Vehicle => NodeType::Vehicle,
            ArchvizCategory::PbrMaterial => NodeType::GenericModel,
        };

        let mut node = SceneNode::novo(format!("archviz_{}", &asset_hash[..16]), req.name, node_type);
        node.confidence = NodeConfidence::Observed;
        node.layer = format!("Archviz/{:?}", req.category);
        node.source = "ArchvizLibrary/local".to_string();
        node.asset_ref = Some(asset_path.display().to_string());
        node.material_refs = req.material_overrides.clone();
        node.transform = Transform64 {
            position: req.position,
            rotation: euler_degrees_to_quaternion(req.rotation_euler),
            scale: req.scale,
        };
        node.metadata = serde_json::json!({
            "asset_id": req.asset_id,
            "asset_sha256": asset_hash,
            "asset_bytes": asset_meta.len(),
            "asset_path": asset_path.display().to_string(),
            "category": format!("{:?}", req.category),
            "rotation_euler_deg": req.rotation_euler,
            "materials_verified": materials_applied.len(),
        });

        Ok(InstantiateAssetResult { node, materials_applied })
    }

    fn find_asset(&self, asset_id: &str) -> anyhow::Result<PathBuf> {
        let root = self.library_dir.resolve_local()?;
        for subdir in ["assets", "models", ""] {
            for ext in ["glb", "gltf"] {
                let candidate = if subdir.is_empty() {
                    root.join(format!("{asset_id}.{ext}"))
                } else {
                    root.join(subdir).join(format!("{asset_id}.{ext}"))
                };
                if candidate.is_file() && !std::fs::symlink_metadata(&candidate)?.file_type().is_symlink() {
                    return Ok(candidate);
                }
            }
        }
        anyhow::bail!(
            "asset Archviz '{}' não existe na biblioteca local '{}'; nenhum placeholder será criado",
            asset_id,
            root.display()
        )
    }

    fn load_material(&self, material_id: &str) -> anyhow::Result<PbrMaterial> {
        let root = self.library_dir.resolve_local()?;
        let path = root.join("materials").join(format!("{material_id}.json"));
        if !path.is_file() || std::fs::symlink_metadata(&path)?.file_type().is_symlink() {
            anyhow::bail!("manifest PBR local ausente: {}", path.display());
        }
        let mut material: PbrMaterial = serde_json::from_slice(&std::fs::read(&path)?)
            .map_err(|e| anyhow::anyhow!("manifest PBR inválido '{}': {e}", path.display()))?;
        if material.id != material_id {
            anyhow::bail!("manifest '{}' declara id '{}'", path.display(), material.id);
        }
        if !(0.0..=1.0).contains(&material.roughness) || !(0.0..=1.0).contains(&material.metallic) {
            anyhow::bail!("roughness/metallic fora de 0..1 em {}", path.display());
        }
        for value in &mut [
            &mut material.albedo_map,
            &mut material.normal_map,
            &mut material.roughness_metallic_map,
            &mut material.ambient_occlusion_map,
        ] {
            if let Some(relative) = value.as_deref() {
                let resolved = resolve_inside(&root, relative)?;
                if !resolved.is_file() || std::fs::symlink_metadata(&resolved)?.file_type().is_symlink() {
                    anyhow::bail!("mapa PBR declarado mas ausente: {}", resolved.display());
                }
                **value = Some(resolved.display().to_string());
            }
        }
        Ok(material)
    }
}

trait LocalRoot {
    fn resolve_local(&self) -> anyhow::Result<PathBuf>;
}

impl LocalRoot for PathBuf {
    fn resolve_local(&self) -> anyhow::Result<PathBuf> {
        let root = std::fs::canonicalize(self).map_err(|e| {
            anyhow::anyhow!("biblioteca Archviz local indisponível '{}': {e}", self.display())
        })?;
        if !root.is_dir() {
            anyhow::bail!("biblioteca Archviz não é diretório: {}", root.display());
        }
        Ok(root)
    }
}

fn resolve_inside(root: &Path, relative: &str) -> anyhow::Result<PathBuf> {
    let raw = Path::new(relative);
    if raw.is_absolute() {
        anyhow::bail!("mapa PBR absoluto não é aceito: {relative}");
    }
    let candidate = root.join(raw);
    let resolved = std::fs::canonicalize(&candidate).map_err(|e| {
        anyhow::anyhow!("mapa PBR não encontrado '{}': {e}", candidate.display())
    })?;
    resolved
        .strip_prefix(root)
        .map_err(|_| anyhow::anyhow!("mapa PBR escapou da biblioteca: {}", resolved.display()))?;
    Ok(resolved)
}

fn validate_identifier(value: &str, label: &str) -> anyhow::Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    {
        anyhow::bail!("{label} inválido: '{value}'");
    }
    Ok(())
}

fn euler_degrees_to_quaternion(euler: [f64; 3]) -> [f64; 4] {
    let [rx, ry, rz] = euler.map(|value| value.to_radians() * 0.5);
    let (sx, cx) = rx.sin_cos();
    let (sy, cy) = ry.sin_cos();
    let (sz, cz) = rz.sin_cos();
    [
        sx * cy * cz - cx * sy * sz,
        cx * sy * cz + sx * cy * sz,
        cx * cy * sz - sx * sy * cz,
        cx * cy * cz + sx * sy * sz,
    ]
}

fn sha256_file(path: &Path) -> anyhow::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instancia_somente_asset_e_material_que_existem_localmente() {
        let base = std::env::temp_dir().join(format!("arcz-archviz-{}", std::process::id()));
        std::fs::create_dir_all(base.join("assets")).unwrap();
        std::fs::create_dir_all(base.join("materials")).unwrap();
        std::fs::write(
            base.join("assets/cadeira.gltf"),
            r#"{"asset":{"version":"2.0"},"scenes":[{}],"scene":0}"#,
        )
        .unwrap();
        std::fs::write(
            base.join("materials/couro.json"),
            r#"{"id":"couro","name":"Couro","albedo_map":null,"normal_map":null,"roughness_metallic_map":null,"ambient_occlusion_map":null,"base_color":[0.04,0.04,0.04,1.0],"roughness":0.55,"metallic":0.0}"#,
        )
        .unwrap();

        let worker = ArchvizWorker::com_raiz(&base);
        let result = worker
            .instanciar_asset(InstantiateAssetRequest {
                asset_id: "cadeira".to_string(),
                name: "Cadeira real".to_string(),
                category: ArchvizCategory::Furniture,
                position: [2.0, 3.0, 0.0],
                rotation_euler: [0.0, 0.0, 90.0],
                scale: [1.0, 1.0, 1.0],
                material_overrides: vec!["couro".to_string()],
            })
            .unwrap();

        assert_eq!(result.materials_applied.len(), 1);
        assert_eq!(result.node.confidence, NodeConfidence::Observed);
        assert!(Path::new(result.node.asset_ref.as_deref().unwrap()).is_file());
        assert_ne!(result.node.transform.rotation, [0.0, 0.0, 0.0, 1.0]);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn asset_ausente_falha_sem_material_default_ficticio() {
        let base = std::env::temp_dir().join(format!("arcz-archviz-missing-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let worker = ArchvizWorker::com_raiz(&base);
        let result = worker.instanciar_asset(InstantiateAssetRequest {
            asset_id: "nao-existe".to_string(),
            name: "nada".to_string(),
            category: ArchvizCategory::Furniture,
            position: [0.0; 3],
            rotation_euler: [0.0; 3],
            scale: [1.0; 3],
            material_overrides: Vec::new(),
        });
        assert!(result.is_err());
        let _ = std::fs::remove_dir_all(&base);
    }
}

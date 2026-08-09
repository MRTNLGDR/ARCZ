//! Reconstruct & Reality Mesh Worker: ingestão de nuvens de pontos e malhas 3D reconstruídas.
//!
//! Este worker não inventa AABB, contagem de pontos, hash ou arquivo. A entrada
//! precisa existir localmente; o conteúdo é inspecionado, SHA-256 é calculado dos
//! bytes reais e uma cópia imutável por hash é materializada no storage do ARCZ.
//! Formatos sem decoder local nesta crate falham explicitamente em vez de retornar
//! um resultado simulado.

use crate::cena::{Georeference64, NodeConfidence, NodeType, SceneNode};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum RealityAssetKind {
    PointCloud,
    GaussianSplat,
    RealityMesh,
    ColmapReconstruction,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IngestRealityAssetRequest {
    pub file_path: String,
    pub name: String,
    pub asset_kind: RealityAssetKind,
    pub georeference: Option<Georeference64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IngestRealityAssetResult {
    pub node: SceneNode,
    pub aabb_min: [f64; 3],
    pub aabb_max: [f64; 3],
    /// Mantido por compatibilidade de contrato. O valor agora é a contagem real
    /// de posições lidas do arquivo, não uma estimativa fixa.
    pub estimated_points_or_vertices: usize,
    pub asset_hash: String,
}

#[derive(Debug, Clone)]
struct GeometryStats {
    min: [f64; 3],
    max: [f64; 3],
    vertices: usize,
}

pub struct ReconstructWorker {
    pub storage_dir: PathBuf,
}

impl ReconstructWorker {
    pub fn novo<P: AsRef<Path>>(storage_dir: P) -> Self {
        Self {
            storage_dir: storage_dir.as_ref().to_path_buf(),
        }
    }

    /// Processa um asset local real e cria o SceneNode autoritativo.
    pub fn processar_asset(
        &self,
        req: IngestRealityAssetRequest,
    ) -> anyhow::Result<IngestRealityAssetResult> {
        let source = Path::new(&req.file_path);
        let meta = std::fs::symlink_metadata(source)
            .map_err(|e| anyhow::anyhow!("asset local inexistente '{}': {e}", source.display()))?;
        if meta.file_type().is_symlink() {
            anyhow::bail!("symlink não é aceito como asset de reconstrução: {}", source.display());
        }
        if !meta.is_file() {
            anyhow::bail!("asset de reconstrução precisa ser arquivo: {}", source.display());
        }

        let ext = source
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        validate_kind_extension(&req.asset_kind, &ext)?;

        let stats = inspect_geometry(source, &ext)?;
        if stats.vertices == 0 {
            anyhow::bail!("asset sem posições 3D utilizáveis: {}", source.display());
        }

        let asset_hash = sha256_file(source)?;
        std::fs::create_dir_all(&self.storage_dir)?;
        let stored_name = if ext.is_empty() {
            asset_hash.clone()
        } else {
            format!("{asset_hash}.{ext}")
        };
        let stored = self.storage_dir.join(stored_name);
        if !stored.exists() {
            std::fs::copy(source, &stored).map_err(|e| {
                anyhow::anyhow!(
                    "falha ao materializar asset '{}' em '{}': {e}",
                    source.display(),
                    stored.display()
                )
            })?;
        } else if sha256_file(&stored)? != asset_hash {
            anyhow::bail!("colisão/integridade inválida no storage: {}", stored.display());
        }

        let node_type = match req.asset_kind {
            RealityAssetKind::PointCloud => NodeType::PointCloud,
            RealityAssetKind::GaussianSplat => NodeType::GaussianSplat,
            RealityAssetKind::RealityMesh | RealityAssetKind::ColmapReconstruction => {
                NodeType::RealityMesh
            }
        };
        let id = format!("reality_{}", &asset_hash[..16]);
        let mut node = SceneNode::novo(id, req.name, node_type);
        node.confidence = NodeConfidence::Observed;
        node.layer = "Reality/Reconstruction".to_string();
        node.source = format!("RealityScan/{}", ext.to_uppercase());
        node.asset_ref = Some(stored.display().to_string());
        node.georeference = req.georeference;
        node.metadata = serde_json::json!({
            "asset_kind": format!("{:?}", req.asset_kind),
            "file_extension": ext,
            "source_bytes": meta.len(),
            "sha256": asset_hash,
            "aabb_min": stats.min,
            "aabb_max": stats.max,
            "positions": stats.vertices,
            "materialized_path": stored.display().to_string(),
        });

        Ok(IngestRealityAssetResult {
            node,
            aabb_min: stats.min,
            aabb_max: stats.max,
            estimated_points_or_vertices: stats.vertices,
            asset_hash,
        })
    }
}

fn validate_kind_extension(kind: &RealityAssetKind, ext: &str) -> anyhow::Result<()> {
    let allowed: &[&str] = match kind {
        RealityAssetKind::PointCloud | RealityAssetKind::GaussianSplat => &["ply"],
        RealityAssetKind::RealityMesh => &["glb", "gltf", "obj"],
        RealityAssetKind::ColmapReconstruction => &["ply", "glb", "gltf", "obj"],
    };
    if allowed.contains(&ext) {
        return Ok(());
    }
    if matches!(ext, "las" | "laz" | "e57") {
        anyhow::bail!(
            "formato .{ext} exige decoder local dedicado ainda não instalado nesta crate; nenhum resultado simulado será produzido"
        );
    }
    anyhow::bail!("formato .{ext} incompatível com {:?}; aceitos: {allowed:?}", kind)
}

fn inspect_geometry(path: &Path, ext: &str) -> anyhow::Result<GeometryStats> {
    match ext {
        "ply" => inspect_ascii_ply(path),
        "obj" => inspect_obj(path),
        "glb" | "gltf" => inspect_gltf(path),
        _ => anyhow::bail!("sem inspetor local para .{ext}"),
    }
}

fn bounds_from_points<I>(points: I) -> anyhow::Result<GeometryStats>
where
    I: IntoIterator<Item = [f64; 3]>,
{
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    let mut vertices = 0usize;
    for point in points {
        if !point.iter().all(|value| value.is_finite()) {
            continue;
        }
        for axis in 0..3 {
            min[axis] = min[axis].min(point[axis]);
            max[axis] = max[axis].max(point[axis]);
        }
        vertices += 1;
    }
    if vertices == 0 {
        anyhow::bail!("nenhuma posição 3D finita encontrada");
    }
    Ok(GeometryStats { min, max, vertices })
}

fn inspect_obj(path: &Path) -> anyhow::Result<GeometryStats> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("OBJ não é texto UTF-8 válido '{}': {e}", path.display()))?;
    let points = text.lines().filter_map(|line| {
        let line = line.trim_start();
        if !line.starts_with("v ") {
            return None;
        }
        let values = line[2..]
            .split_whitespace()
            .take(3)
            .map(str::parse::<f64>)
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        (values.len() == 3).then(|| [values[0], values[1], values[2]])
    });
    bounds_from_points(points)
}

fn inspect_ascii_ply(path: &Path) -> anyhow::Result<GeometryStats> {
    let bytes = std::fs::read(path)?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| anyhow::anyhow!("PLY binário exige decoder local dedicado; nenhum AABB será inventado"))?;
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("ply") {
        anyhow::bail!("cabeçalho PLY inválido: {}", path.display());
    }

    let mut ascii = false;
    let mut vertex_count = None;
    let mut in_vertex = false;
    let mut vertex_properties: Vec<String> = Vec::new();
    for line in &mut lines {
        let line = line.trim();
        if line.starts_with("format ") {
            ascii = line.starts_with("format ascii ");
        } else if let Some(rest) = line.strip_prefix("element ") {
            let mut parts = rest.split_whitespace();
            let name = parts.next().unwrap_or("");
            in_vertex = name == "vertex";
            if in_vertex {
                vertex_count = parts.next().and_then(|value| value.parse::<usize>().ok());
                vertex_properties.clear();
            }
        } else if in_vertex {
            if let Some(rest) = line.strip_prefix("property ") {
                let name = rest.split_whitespace().last().unwrap_or("");
                vertex_properties.push(name.to_string());
            }
        }
        if line == "end_header" {
            break;
        }
    }
    if !ascii {
        anyhow::bail!("PLY binário não é suportado por este inspetor local; instale/ligue decoder real em vez de simular dados");
    }
    let count = vertex_count.ok_or_else(|| anyhow::anyhow!("PLY sem element vertex"))?;
    let x = vertex_properties.iter().position(|name| name == "x").ok_or_else(|| anyhow::anyhow!("PLY sem propriedade x"))?;
    let y = vertex_properties.iter().position(|name| name == "y").ok_or_else(|| anyhow::anyhow!("PLY sem propriedade y"))?;
    let z = vertex_properties.iter().position(|name| name == "z").ok_or_else(|| anyhow::anyhow!("PLY sem propriedade z"))?;
    let mut points = Vec::with_capacity(count);
    for index in 0..count {
        let line = lines.next().ok_or_else(|| anyhow::anyhow!("PLY terminou antes dos {count} vértices: parou em {index}"))?;
        let values = line
            .split_whitespace()
            .map(str::parse::<f64>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow::anyhow!("vértice PLY inválido na linha {}: {e}", index + 1))?;
        let max_index = x.max(y).max(z);
        if values.len() <= max_index {
            anyhow::bail!("vértice PLY {} não contém x/y/z", index + 1);
        }
        points.push([values[x], values[y], values[z]]);
    }
    bounds_from_points(points)
}

fn inspect_gltf(path: &Path) -> anyhow::Result<GeometryStats> {
    let (document, buffers, _) = gltf::import(path)
        .map_err(|e| anyhow::anyhow!("falha ao abrir glTF/GLB '{}': {e}", path.display()))?;
    let mut points = Vec::new();
    for mesh in document.meshes() {
        for primitive in mesh.primitives() {
            let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));
            if let Some(positions) = reader.read_positions() {
                points.extend(positions.map(|p| [p[0] as f64, p[1] as f64, p[2] as f64]));
            }
        }
    }
    bounds_from_points(points)
}

fn sha256_file(path: &Path) -> anyhow::Result<String> {
    let mut file = File::open(path)?;
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
    fn ingere_ply_real_calcula_bounds_hash_e_copia_para_storage() {
        let base = std::env::temp_dir().join(format!("arcz-reconstruct-{}", std::process::id()));
        let source = base.join("scan.ply");
        let storage = base.join("storage");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(
            &source,
            "ply\nformat ascii 1.0\nelement vertex 3\nproperty float x\nproperty float y\nproperty float z\nend_header\n-2 1 4\n5 -3 2\n1 8 9\n",
        )
        .unwrap();

        let worker = ReconstructWorker::novo(&storage);
        let result = worker
            .processar_asset(IngestRealityAssetRequest {
                file_path: source.display().to_string(),
                name: "Scan real".to_string(),
                asset_kind: RealityAssetKind::PointCloud,
                georeference: None,
            })
            .unwrap();

        assert_eq!(result.estimated_points_or_vertices, 3);
        assert_eq!(result.aabb_min, [-2.0, -3.0, 2.0]);
        assert_eq!(result.aabb_max, [5.0, 8.0, 9.0]);
        assert_eq!(result.asset_hash.len(), 64);
        assert!(Path::new(result.node.asset_ref.as_deref().unwrap()).is_file());
        assert_eq!(result.node.confidence, NodeConfidence::Observed);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn recusa_arquivo_inexistente_em_vez_de_simular_resultado() {
        let worker = ReconstructWorker::novo(std::env::temp_dir().join("arcz-no-mock"));
        let result = worker.processar_asset(IngestRealityAssetRequest {
            file_path: "arquivo-que-nao-existe.ply".to_string(),
            name: "não existe".to_string(),
            asset_kind: RealityAssetKind::PointCloud,
            georeference: None,
        });
        assert!(result.is_err());
    }
}

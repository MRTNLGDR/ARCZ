use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type Point2 = [f64; 2];
pub type Point3 = [f32; 3];

#[derive(Debug, Clone, Copy)]
pub struct MeshView<'a> {
    pub positions: &'a [Point3],
    pub normals: &'a [Point3],
    pub uvs: &'a [[f32; 2]],
    pub indices: &'a [u32],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    pub warnings: Vec<String>,
    pub triangles: usize,
    pub bbox_min: Point3,
    pub bbox_max: Point3,
}

pub fn signed_area(polygon: &[Point2]) -> f64 {
    if polygon.len() < 3 {
        return 0.0;
    }
    let mut sum = 0.0;
    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[(i + 1) % polygon.len()];
        sum += a[0] * b[1] - b[0] * a[1];
    }
    sum * 0.5
}

pub fn validate_polygon(polygon: &[Point2], minimum_area: f64) -> Result<(), GeometryError> {
    if polygon.len() < 3 {
        return Err(GeometryError::TooFewVertices);
    }
    for point in polygon {
        if !point[0].is_finite() || !point[1].is_finite() {
            return Err(GeometryError::NonFinite);
        }
    }
    if signed_area(polygon).abs() < minimum_area {
        return Err(GeometryError::AreaTooSmall);
    }
    let n = polygon.len();
    for i in 0..n {
        let a = polygon[i];
        let b = polygon[(i + 1) % n];
        for j in i + 1..n {
            let c = polygon[j];
            let d = polygon[(j + 1) % n];
            if i == j || (i + 1) % n == j || (j + 1) % n == i {
                continue;
            }
            if segments_intersect(a, b, c, d) {
                return Err(GeometryError::SelfIntersection { a: i, b: j });
            }
        }
    }
    Ok(())
}

fn orient(a: Point2, b: Point2, c: Point2) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}
fn segments_intersect(a: Point2, b: Point2, c: Point2, d: Point2) -> bool {
    let o1 = orient(a, b, c);
    let o2 = orient(a, b, d);
    let o3 = orient(c, d, a);
    let o4 = orient(c, d, b);
    ((o1 > 0.0 && o2 < 0.0) || (o1 < 0.0 && o2 > 0.0))
        && ((o3 > 0.0 && o4 < 0.0) || (o3 < 0.0 && o4 > 0.0))
}

pub fn validate_mesh(mesh: MeshView<'_>) -> Result<ValidationReport, GeometryError> {
    if mesh.positions.is_empty() {
        return Err(GeometryError::EmptyMesh);
    }
    if mesh.normals.len() != mesh.positions.len() || mesh.uvs.len() != mesh.positions.len() {
        return Err(GeometryError::AttributeLengthMismatch);
    }
    if !mesh.indices.len().is_multiple_of(3) {
        return Err(GeometryError::IndexCountNotTriangular);
    }
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for (index, position) in mesh.positions.iter().enumerate() {
        for axis in 0..3 {
            if !position[axis].is_finite() || !mesh.normals[index][axis].is_finite() {
                return Err(GeometryError::NonFinite);
            }
            min[axis] = min[axis].min(position[axis]);
            max[axis] = max[axis].max(position[axis]);
        }
        if !mesh.uvs[index][0].is_finite() || !mesh.uvs[index][1].is_finite() {
            return Err(GeometryError::NonFinite);
        }
    }
    let mut warnings = Vec::new();
    for (triangle, indices) in mesh.indices.chunks_exact(3).enumerate() {
        if indices
            .iter()
            .any(|index| *index as usize >= mesh.positions.len())
        {
            return Err(GeometryError::IndexOutOfBounds { triangle });
        }
        let a = mesh.positions[indices[0] as usize];
        let b = mesh.positions[indices[1] as usize];
        let c = mesh.positions[indices[2] as usize];
        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let cross = [
            ab[1] * ac[2] - ab[2] * ac[1],
            ab[2] * ac[0] - ab[0] * ac[2],
            ab[0] * ac[1] - ab[1] * ac[0],
        ];
        let area2 = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
        if area2 < 1.0e-8 {
            warnings.push(format!("triângulo degenerado: {triangle}"));
        }
    }
    Ok(ValidationReport {
        warnings,
        triangles: mesh.indices.len() / 3,
        bbox_min: min,
        bbox_max: max,
    })
}

#[derive(Debug, Error, PartialEq)]
pub enum GeometryError {
    #[error("menos de três vértices")]
    TooFewVertices,
    #[error("valor não finito")]
    NonFinite,
    #[error("área abaixo do mínimo")]
    AreaTooSmall,
    #[error("auto-interseção entre arestas {a} e {b}")]
    SelfIntersection { a: usize, b: usize },
    #[error("malha vazia")]
    EmptyMesh,
    #[error("comprimentos de atributos divergentes")]
    AttributeLengthMismatch,
    #[error("índices não formam triângulos")]
    IndexCountNotTriangular,
    #[error("índice fora do buffer no triângulo {triangle}")]
    IndexOutOfBounds { triangle: usize },
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detecta_bow_tie() {
        assert!(matches!(
            validate_polygon(&[[0., 0.], [1., 1.], [0., 1.], [1., 0.]], 0.0),
            Err(GeometryError::SelfIntersection { .. }) | Err(GeometryError::AreaTooSmall)
        ));
    }
}

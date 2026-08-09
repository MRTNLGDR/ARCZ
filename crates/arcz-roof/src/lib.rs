use arcz_validation::{signed_area, validate_polygon, Point2};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoofKind {
    Flat,
    Shed,
    Gable,
    Hip,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoofRequest {
    pub footprint: Vec<Point2>,
    pub wall_top_m: f64,
    pub kind: RoofKind,
    pub pitch_deg: f64,
    #[serde(default)]
    pub eave_m: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RoofMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoofResult {
    pub mesh: RoofMesh,
    pub requested: RoofKind,
    pub applied: RoofKind,
    pub warnings: Vec<String>,
}

pub fn generate(request: &RoofRequest) -> Result<RoofResult, RoofError> {
    validate_polygon(&request.footprint, 0.25)
        .map_err(|error| RoofError::InvalidFootprint(error.to_string()))?;
    if !request.wall_top_m.is_finite()
        || !request.pitch_deg.is_finite()
        || !(0.0..=75.0).contains(&request.pitch_deg)
        || !request.eave_m.is_finite()
        || !(0.0..=5.0).contains(&request.eave_m)
    {
        return Err(RoofError::InvalidParameter);
    }
    let mut footprint = request.footprint.clone();
    if signed_area(&footprint) < 0.0 {
        footprint.reverse();
    }
    let rectangular = rectangle_bounds_if_axis_aligned(&footprint);
    let expanded = rectangular.map(|bounds| expand_bounds(bounds, request.eave_m));
    let (mesh, applied, warnings) = match request.kind {
        RoofKind::Flat => (flat(&footprint, request.wall_top_m)?, RoofKind::Flat, Vec::new()),
        RoofKind::Hip => match expanded {
            Some(bounds) => (hip(&rectangle_points(bounds), request.wall_top_m, request.pitch_deg)?, RoofKind::Hip, Vec::new()),
            None => {
                let mut warnings = Vec::new();
                if request.eave_m > 0.0 {
                    warnings.push("beiral não aplicado em footprint não retangular; cobertura hip mantém limite seguro".to_owned());
                }
                (hip(&footprint, request.wall_top_m, request.pitch_deg)?, RoofKind::Hip, warnings)
            }
        },
        RoofKind::Shed => match expanded {
            Some(bounds) => (shed(bounds, request.wall_top_m, request.pitch_deg), RoofKind::Shed, Vec::new()),
            None => (flat(&footprint, request.wall_top_m)?, RoofKind::Flat,
                     vec!["shed exige footprint retangular; fallback seguro para cobertura plana".to_owned()]),
        },
        RoofKind::Gable => match expanded {
            Some(bounds) => (gable(bounds, request.wall_top_m, request.pitch_deg), RoofKind::Gable, Vec::new()),
            None => (flat(&footprint, request.wall_top_m)?, RoofKind::Flat,
                     vec!["duas águas exige footprint retangular nesta versão; fallback seguro para cobertura plana".to_owned()]),
        },
    };
    Ok(RoofResult {
        mesh,
        requested: request.kind,
        applied,
        warnings,
    })
}

fn expand_bounds(bounds: [f64; 4], eave: f64) -> [f64; 4] {
    [
        bounds[0] - eave,
        bounds[1] - eave,
        bounds[2] + eave,
        bounds[3] + eave,
    ]
}

fn rectangle_points(bounds: [f64; 4]) -> Vec<Point2> {
    let [min_x, min_y, max_x, max_y] = bounds;
    vec![
        [min_x, min_y],
        [max_x, min_y],
        [max_x, max_y],
        [min_x, max_y],
    ]
}

fn rectangle_bounds_if_axis_aligned(points: &[Point2]) -> Option<[f64; 4]> {
    if points.len() != 4 {
        return None;
    }
    let min_x = points.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
    let max_x = points
        .iter()
        .map(|p| p[0])
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = points.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
    let max_y = points
        .iter()
        .map(|p| p[1])
        .fold(f64::NEG_INFINITY, f64::max);
    let tolerance = ((max_x - min_x).max(max_y - min_y) * 1.0e-6).max(1.0e-7);
    let corners = [
        [min_x, min_y],
        [max_x, min_y],
        [max_x, max_y],
        [min_x, max_y],
    ];
    if points.iter().all(|p| {
        corners
            .iter()
            .any(|c| (p[0] - c[0]).abs() <= tolerance && (p[1] - c[1]).abs() <= tolerance)
    }) {
        Some([min_x, min_y, max_x, max_y])
    } else {
        None
    }
}

fn flat(points: &[Point2], height: f64) -> Result<RoofMesh, RoofError> {
    let triangles = triangulate(points)?;
    let mut mesh = RoofMesh::default();
    for triangle in triangles {
        let a = points[triangle[0]];
        let b = points[triangle[1]];
        let c = points[triangle[2]];
        emit_triangle(
            &mut mesh,
            render(a, height),
            render(b, height),
            render(c, height),
            [
                [a[0] as f32, a[1] as f32],
                [b[0] as f32, b[1] as f32],
                [c[0] as f32, c[1] as f32],
            ],
        );
    }
    Ok(mesh)
}

fn hip(points: &[Point2], wall_top: f64, pitch_deg: f64) -> Result<RoofMesh, RoofError> {
    let center = centroid(points);
    let min_distance = points
        .iter()
        .map(|p| ((p[0] - center[0]).powi(2) + (p[1] - center[1]).powi(2)).sqrt())
        .fold(f64::INFINITY, f64::min);
    let apex = wall_top + min_distance * pitch_deg.to_radians().tan();
    let mut mesh = RoofMesh::default();
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        emit_triangle(
            &mut mesh,
            render(a, wall_top),
            render(b, wall_top),
            render(center, apex),
            [[0., 0.], [1., 0.], [0.5, 1.]],
        );
    }
    Ok(mesh)
}

fn gable(bounds: [f64; 4], wall_top: f64, pitch_deg: f64) -> RoofMesh {
    let [x0, y0, x1, y1] = bounds;
    let width = x1 - x0;
    let depth = y1 - y0;
    let mut mesh = RoofMesh::default();
    if width >= depth {
        let mid = (y0 + y1) * 0.5;
        let ridge = wall_top + depth * 0.5 * pitch_deg.to_radians().tan();
        emit_quad(
            &mut mesh,
            render([x0, y0], wall_top),
            render([x1, y0], wall_top),
            render([x1, mid], ridge),
            render([x0, mid], ridge),
        );
        emit_quad(
            &mut mesh,
            render([x0, mid], ridge),
            render([x1, mid], ridge),
            render([x1, y1], wall_top),
            render([x0, y1], wall_top),
        );
        emit_triangle(
            &mut mesh,
            render([x0, y0], wall_top),
            render([x0, mid], ridge),
            render([x0, y1], wall_top),
            [[0., 0.], [0.5, 1.], [1., 0.]],
        );
        emit_triangle(
            &mut mesh,
            render([x1, y1], wall_top),
            render([x1, mid], ridge),
            render([x1, y0], wall_top),
            [[0., 0.], [0.5, 1.], [1., 0.]],
        );
    } else {
        let mid = (x0 + x1) * 0.5;
        let ridge = wall_top + width * 0.5 * pitch_deg.to_radians().tan();
        emit_quad(
            &mut mesh,
            render([x0, y0], wall_top),
            render([mid, y0], ridge),
            render([mid, y1], ridge),
            render([x0, y1], wall_top),
        );
        emit_quad(
            &mut mesh,
            render([mid, y0], ridge),
            render([x1, y0], wall_top),
            render([x1, y1], wall_top),
            render([mid, y1], ridge),
        );
        emit_triangle(
            &mut mesh,
            render([x0, y0], wall_top),
            render([x1, y0], wall_top),
            render([mid, y0], ridge),
            [[0., 0.], [1., 0.], [0.5, 1.]],
        );
        emit_triangle(
            &mut mesh,
            render([x1, y1], wall_top),
            render([x0, y1], wall_top),
            render([mid, y1], ridge),
            [[0., 0.], [1., 0.], [0.5, 1.]],
        );
    }
    mesh
}

fn shed(bounds: [f64; 4], wall_top: f64, pitch_deg: f64) -> RoofMesh {
    let [x0, y0, x1, y1] = bounds;
    let rise = (x1 - x0) * pitch_deg.to_radians().tan();
    let mut mesh = RoofMesh::default();
    emit_quad(
        &mut mesh,
        render([x0, y0], wall_top),
        render([x1, y0], wall_top + rise),
        render([x1, y1], wall_top + rise),
        render([x0, y1], wall_top),
    );
    mesh
}

fn render(point: Point2, height: f64) -> [f32; 3] {
    [point[0] as f32, height as f32, -point[1] as f32]
}
fn centroid(points: &[Point2]) -> Point2 {
    let n = points.len() as f64;
    [
        points.iter().map(|p| p[0]).sum::<f64>() / n,
        points.iter().map(|p| p[1]).sum::<f64>() / n,
    ]
}

fn emit_quad(mesh: &mut RoofMesh, a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3]) {
    emit_triangle(mesh, a, b, c, [[0., 0.], [1., 0.], [1., 1.]]);
    emit_triangle(mesh, a, c, d, [[0., 0.], [1., 1.], [0., 1.]]);
}
fn emit_triangle(mesh: &mut RoofMesh, a: [f32; 3], b: [f32; 3], c: [f32; 3], uv: [[f32; 2]; 3]) {
    let normal = normal(a, b, c);
    let base = mesh.positions.len() as u32;
    mesh.positions.extend([a, b, c]);
    mesh.normals.extend([normal; 3]);
    mesh.uvs.extend(uv);
    mesh.indices.extend([base, base + 1, base + 2]);
}
fn normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-12);
    [n[0] / l, n[1] / l, n[2] / l]
}

fn triangulate(points: &[Point2]) -> Result<Vec<[usize; 3]>, RoofError> {
    let mut remaining: Vec<usize> = (0..points.len()).collect();
    let mut result = Vec::new();
    let mut guard = 0;
    while remaining.len() > 3 {
        guard += 1;
        if guard > points.len() * points.len() {
            return Err(RoofError::Triangulation);
        }
        let mut ear = None;
        for i in 0..remaining.len() {
            let a = remaining[(i + remaining.len() - 1) % remaining.len()];
            let b = remaining[i];
            let c = remaining[(i + 1) % remaining.len()];
            if cross(points[a], points[b], points[c]) <= 1e-10 {
                continue;
            }
            if remaining.iter().copied().any(|p| {
                p != a && p != b && p != c && inside(points[p], points[a], points[b], points[c])
            }) {
                continue;
            }
            ear = Some((i, [a, b, c]));
            break;
        }
        let Some((index, triangle)) = ear else {
            return Err(RoofError::Triangulation);
        };
        result.push(triangle);
        remaining.remove(index);
    }
    result.push([remaining[0], remaining[1], remaining[2]]);
    Ok(result)
}
fn cross(a: Point2, b: Point2, c: Point2) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}
fn inside(p: Point2, a: Point2, b: Point2, c: Point2) -> bool {
    let c1 = cross(a, b, p);
    let c2 = cross(b, c, p);
    let c3 = cross(c, a, p);
    c1 >= -1e-10 && c2 >= -1e-10 && c3 >= -1e-10
}

#[derive(Debug, Error)]
pub enum RoofError {
    #[error("footprint inválido: {0}")]
    InvalidFootprint(String),
    #[error("parâmetro de telhado inválido")]
    InvalidParameter,
    #[error("triangulação falhou")]
    Triangulation,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gable_retangular_tem_faces() {
        let r = generate(&RoofRequest {
            footprint: vec![[0., 0.], [10., 0.], [10., 6.], [0., 6.]],
            wall_top_m: 3.,
            kind: RoofKind::Gable,
            pitch_deg: 30.,
            eave_m: 0.,
        })
        .unwrap();
        assert_eq!(r.applied, RoofKind::Gable);
        assert!(!r.mesh.indices.is_empty());
    }
    #[test]
    fn beiral_expande_cobertura_retangular() {
        let r = generate(&RoofRequest {
            footprint: vec![[0., 0.], [10., 0.], [10., 6.], [0., 6.]],
            wall_top_m: 3.,
            kind: RoofKind::Gable,
            pitch_deg: 30.,
            eave_m: 0.5,
        })
        .unwrap();
        let min_x = r
            .mesh
            .positions
            .iter()
            .map(|p| p[0])
            .fold(f32::INFINITY, f32::min);
        let max_x = r
            .mesh
            .positions
            .iter()
            .map(|p| p[0])
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(min_x <= -0.49 && max_x >= 10.49);
    }
}

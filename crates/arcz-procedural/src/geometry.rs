use arcz_validation::{signed_area, validate_polygon, Point2};
use crate::ProceduralError;

pub fn ensure_ccw(points: &[Point2]) -> Result<Vec<Point2>, ProceduralError> {
    validate_polygon(points, 0.01).map_err(|error| ProceduralError::InvalidPolygon(error.to_string()))?;
    let mut output = points.to_vec();
    if signed_area(&output) < 0.0 { output.reverse(); }
    Ok(output)
}

pub fn render_point(point: Point2, height: f64) -> Result<[f32; 3], ProceduralError> {
    if !point[0].is_finite() || !point[1].is_finite() || !height.is_finite() {
        return Err(ProceduralError::NonFinite);
    }
    Ok([point[0] as f32, height as f32, -point[1] as f32])
}

pub fn centroid(points: &[Point2]) -> Point2 {
    let factor = 1.0 / points.len() as f64;
    [points.iter().map(|p| p[0]).sum::<f64>() * factor,
     points.iter().map(|p| p[1]).sum::<f64>() * factor]
}

pub fn point_in_polygon(point: Point2, polygon: &[Point2]) -> bool {
    let mut inside = false;
    let mut j = polygon.len() - 1;
    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[j];
        if (a[1] > point[1]) != (b[1] > point[1]) {
            let x = (b[0] - a[0]) * (point[1] - a[1]) / (b[1] - a[1]) + a[0];
            if point[0] < x { inside = !inside; }
        }
        j = i;
    }
    inside
}

pub fn triangulate(points: &[Point2]) -> Result<Vec<[usize; 3]>, ProceduralError> {
    let points = ensure_ccw(points)?;
    let mut remaining: Vec<usize> = (0..points.len()).collect();
    let mut result = Vec::new();
    let maximum_iterations = points.len().saturating_mul(points.len()).max(16);
    let mut iterations = 0;
    while remaining.len() > 3 {
        iterations += 1;
        if iterations > maximum_iterations { return Err(ProceduralError::TriangulationFailed); }
        let mut ear = None;
        for i in 0..remaining.len() {
            let a = remaining[(i + remaining.len() - 1) % remaining.len()];
            let b = remaining[i];
            let c = remaining[(i + 1) % remaining.len()];
            if cross(points[a], points[b], points[c]) <= 1.0e-10 { continue; }
            if remaining.iter().copied().any(|p| p != a && p != b && p != c
                && point_in_triangle(points[p], points[a], points[b], points[c])) { continue; }
            ear = Some((i, [a, b, c]));
            break;
        }
        let Some((index, triangle)) = ear else { return Err(ProceduralError::TriangulationFailed); };
        result.push(triangle);
        remaining.remove(index);
    }
    result.push([remaining[0], remaining[1], remaining[2]]);
    Ok(result)
}

fn cross(a: Point2, b: Point2, c: Point2) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

fn point_in_triangle(p: Point2, a: Point2, b: Point2, c: Point2) -> bool {
    let c1 = cross(a, b, p);
    let c2 = cross(b, c, p);
    let c3 = cross(c, a, p);
    c1 >= -1.0e-10 && c2 >= -1.0e-10 && c3 >= -1.0e-10
}

pub fn axis_aligned_infill(polygon: &[Point2], front_setback: f64, side_setback: f64,
                           maximum_coverage: f64) -> Option<Vec<Point2>> {
    if ensure_ccw(polygon).is_err() || !(0.05..=0.95).contains(&maximum_coverage) { return None; }
    let min_x = polygon.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
    let max_x = polygon.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max);
    let min_y = polygon.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
    let max_y = polygon.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max);
    let center = centroid(polygon);
    let mut width = ((max_x - min_x) - 2.0 * side_setback).max(1.0);
    let mut depth = ((max_y - min_y) - front_setback - side_setback).max(1.0);
    let lot_area = (max_x - min_x).max(0.0) * (max_y - min_y).max(0.0);
    let target_area = lot_area * maximum_coverage;
    if width * depth > target_area && target_area > 0.0 {
        let scale = (target_area / (width * depth)).sqrt();
        width *= scale;
        depth *= scale;
    }
    for _ in 0..24 {
        let rectangle = vec![
            [center[0] - width * 0.5, center[1] - depth * 0.5],
            [center[0] + width * 0.5, center[1] - depth * 0.5],
            [center[0] + width * 0.5, center[1] + depth * 0.5],
            [center[0] - width * 0.5, center[1] + depth * 0.5],
        ];
        if width >= 3.0 && depth >= 3.0 && rectangle.iter().all(|point| point_in_polygon(*point, polygon)) {
            return Some(rectangle);
        }
        width *= 0.9;
        depth *= 0.9;
    }
    None
}

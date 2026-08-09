use arcz_determinism::Seed;
use arcz_validation::{validate_polygon, Point2};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScatterRequest {
    pub polygon: Vec<Point2>,
    pub seed: u64,
    pub target_count: usize,
    pub minimum_distance_m: f64,
    #[serde(default)]
    pub exclusions: Vec<ExclusionCircle>,
    #[serde(default = "default_attempts")]
    pub attempts_per_instance: usize,
    #[serde(default)]
    pub variants: Vec<VariantWeight>,
}
fn default_attempts() -> usize {
    40
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ExclusionCircle {
    pub center: Point2,
    pub radius_m: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantWeight {
    pub id: String,
    pub weight: f64,
    pub scale_min: f64,
    pub scale_max: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instance {
    pub position: Point2,
    pub rotation_rad: f64,
    pub scale: f64,
    pub variant: String,
    pub reason: String,
}

pub fn scatter(request: &ScatterRequest) -> Result<Vec<Instance>, VegetationError> {
    validate_polygon(&request.polygon, 0.25)
        .map_err(|e| VegetationError::InvalidPolygon(e.to_string()))?;
    if request.target_count > 1_000_000
        || !request.minimum_distance_m.is_finite()
        || request.minimum_distance_m < 0.0
        || request.attempts_per_instance == 0
    {
        return Err(VegetationError::InvalidParameter);
    }
    let min_x = request
        .polygon
        .iter()
        .map(|p| p[0])
        .fold(f64::INFINITY, f64::min);
    let max_x = request
        .polygon
        .iter()
        .map(|p| p[0])
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = request
        .polygon
        .iter()
        .map(|p| p[1])
        .fold(f64::INFINITY, f64::min);
    let max_y = request
        .polygon
        .iter()
        .map(|p| p[1])
        .fold(f64::NEG_INFINITY, f64::max);
    let mut rng = Seed(request.seed).rng();
    let mut points: Vec<Point2> = Vec::new();
    let mut result = Vec::new();
    let weights: Vec<_> = request.variants.iter().map(|v| (v, v.weight)).collect();
    let max_attempts = request
        .target_count
        .saturating_mul(request.attempts_per_instance);
    for _ in 0..max_attempts {
        if result.len() >= request.target_count {
            break;
        }
        let p = [rng.range_f64(min_x, max_x), rng.range_f64(min_y, max_y)];
        if !point_in_polygon(p, &request.polygon) {
            continue;
        }
        if request
            .exclusions
            .iter()
            .any(|e| distance(p, e.center) < e.radius_m)
        {
            continue;
        }
        if points
            .iter()
            .any(|q| distance(p, *q) < request.minimum_distance_m)
        {
            continue;
        }
        let variant = rng.choose_weighted(&weights).copied();
        let (id, scale) = match variant {
            Some(v) => (v.id.clone(), rng.range_f64(v.scale_min, v.scale_max)),
            None => ("generic".to_owned(), rng.range_f64(0.85, 1.15)),
        };
        points.push(p);
        result.push(Instance {
            position: p,
            rotation_rad: rng.range_f64(0., std::f64::consts::TAU),
            scale,
            variant: id,
            reason: "inside_mask_and_outside_exclusions".to_owned(),
        });
    }
    Ok(result)
}
fn distance(a: Point2, b: Point2) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
fn point_in_polygon(p: Point2, poly: &[Point2]) -> bool {
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[j];
        let crosses_scanline = (a[1] > p[1]) != (b[1] > p[1]);
        if crosses_scanline {
            let x_intersection = (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0];
            if p[0] < x_intersection {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

#[derive(Debug, Error)]
pub enum VegetationError {
    #[error("polígono inválido: {0}")]
    InvalidPolygon(String),
    #[error("parâmetro de scatter inválido")]
    InvalidParameter,
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scatter_reproduz() {
        let r = ScatterRequest {
            polygon: vec![[0., 0.], [10., 0.], [10., 10.], [0., 10.]],
            seed: 1,
            target_count: 10,
            minimum_distance_m: 1.,
            exclusions: vec![],
            attempts_per_instance: 40,
            variants: vec![],
        };
        assert_eq!(
            scatter(&r)
                .unwrap()
                .iter()
                .map(|x| x.position)
                .collect::<Vec<_>>(),
            scatter(&r)
                .unwrap()
                .iter()
                .map(|x| x.position)
                .collect::<Vec<_>>()
        );
    }
}

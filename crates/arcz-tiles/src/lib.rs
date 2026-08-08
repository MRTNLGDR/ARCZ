use arcz_determinism::{sha256_hex, Seed};
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TileKey {
    pub provider: String,
    pub version: String,
    pub z: u8,
    pub x: u32,
    pub y: u32,
    pub profile_hash: String,
    pub generator_version: String,
    pub seed: u64,
}
impl TileKey {
    pub fn stable_id(&self) -> String {
        sha256_hex(format!(
            "{}|{}|{}/{}/{}|{}|{}|{}",
            self.provider,
            self.version,
            self.z,
            self.x,
            self.y,
            self.profile_hash,
            self.generator_version,
            self.seed
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TileState {
    Missing,
    Queued,
    Fetching,
    Preparing,
    Generating,
    Validating,
    Ready,
    Applying,
    Active,
    Evicting,
    FailedRetryable,
    FailedPermanent,
    Cancelled,
}

impl TileState {
    pub fn can_transition(self, next: Self) -> bool {
        use TileState::*;
        matches!(
            (self, next),
            (Missing, Queued)
                | (Queued, Fetching)
                | (Queued, Preparing)
                | (Queued, Cancelled)
                | (Fetching, Preparing)
                | (Fetching, FailedRetryable)
                | (Fetching, FailedPermanent)
                | (Fetching, Cancelled)
                | (Preparing, Generating)
                | (Preparing, FailedRetryable)
                | (Preparing, FailedPermanent)
                | (Preparing, Cancelled)
                | (Generating, Validating)
                | (Generating, FailedRetryable)
                | (Generating, FailedPermanent)
                | (Generating, Cancelled)
                | (Validating, Ready)
                | (Validating, FailedRetryable)
                | (Validating, FailedPermanent)
                | (Validating, Cancelled)
                | (Ready, Applying)
                | (Ready, Evicting)
                | (Applying, Active)
                | (Applying, FailedRetryable)
                | (Applying, FailedPermanent)
                | (Active, Evicting)
                | (Active, Queued)
                | (Evicting, Missing)
                | (Evicting, Ready)
                | (FailedRetryable, Queued)
                | (FailedRetryable, Cancelled)
                | (Cancelled, Queued)
        ) || self == next
    }

    pub fn transition(self, next: Self) -> Result<Self, TileError> {
        if self.can_transition(next) {
            Ok(next)
        } else {
            Err(TileError::InvalidTransition {
                from: self,
                to: next,
            })
        }
    }
}

#[derive(Debug, Error)]
pub enum TileError {
    #[error("transição inválida de tile: {from:?} -> {to:?}")]
    InvalidTransition { from: TileState, to: TileState },
    #[error("zoom inválido: {0}")]
    InvalidZoom(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Ring {
    Hero,
    Near,
    Medium,
    Distant,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlannedTile {
    pub z: u8,
    pub x: u32,
    pub y: u32,
    pub ring: Ring,
    pub distance_m: f64,
    pub seed: u64,
}

pub fn lon_lat_to_tile(lon: f64, lat: f64, z: u8) -> Result<(u32, u32), TileError> {
    if z > 22 {
        return Err(TileError::InvalidZoom(z));
    }
    let n = (1_u64 << z) as f64;
    let lat = lat
        .clamp(-85.051_128_779_806_6, 85.051_128_779_806_6)
        .to_radians();
    let x = (((lon + 180.0) / 360.0 * n).floor() as i64).clamp(0, n as i64 - 1) as u32;
    let y =
        (((1.0 - lat.tan().asinh() / PI) / 2.0 * n).floor() as i64).clamp(0, n as i64 - 1) as u32;
    Ok((x, y))
}

pub fn tile_center(x: u32, y: u32, z: u8) -> (f64, f64) {
    let n = (1_u64 << z) as f64;
    let lon = (x as f64 + 0.5) / n * 360.0 - 180.0;
    let lat = ((PI * (1.0 - 2.0 * (y as f64 + 0.5) / n)).sinh().atan()).to_degrees();
    (lon, lat)
}

pub fn approximate_distance_m(a: (f64, f64), b: (f64, f64)) -> f64 {
    let mid_lat = ((a.1 + b.1) * 0.5).to_radians();
    let dx = (b.0 - a.0) * 111_132.0 * mid_lat.cos();
    let dy = (b.1 - a.1) * 111_132.0;
    dx.hypot(dy)
}

pub fn plan(
    focus: (f64, f64),
    radius_m: f64,
    z: u8,
    rings: [f64; 4],
    seed: Seed,
) -> Result<Vec<PlannedTile>, TileError> {
    let (cx, cy) = lon_lat_to_tile(focus.0, focus.1, z)?;
    let n = 1_u32 << z;
    let neighbor_x = (cx + 1).min(n - 1);
    let tile_m =
        approximate_distance_m(tile_center(cx, cy, z), tile_center(neighbor_x, cy, z)).max(1.0);
    let span = (radius_m / tile_m).ceil().max(1.0) as i64;
    let mut result = Vec::new();
    for y in (cy as i64 - span).max(0)..=(cy as i64 + span).min(n as i64 - 1) {
        for x in (cx as i64 - span).max(0)..=(cx as i64 + span).min(n as i64 - 1) {
            let distance = approximate_distance_m(focus, tile_center(x as u32, y as u32, z));
            if distance > radius_m + tile_m {
                continue;
            }
            let ring = if distance <= rings[0] {
                Ring::Hero
            } else if distance <= rings[1] {
                Ring::Near
            } else if distance <= rings[2] {
                Ring::Medium
            } else {
                Ring::Distant
            };
            let tile_seed = seed.derive("tile", format!("{z}/{x}/{y}")).0;
            result.push(PlannedTile {
                z,
                x: x as u32,
                y: y as u32,
                ring,
                distance_m: distance,
                seed: tile_seed,
            });
        }
    }
    result.sort_by(|a, b| {
        a.distance_m
            .total_cmp(&b.distance_m)
            .then(a.y.cmp(&b.y))
            .then(a.x.cmp(&b.x))
    });
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn estado_invalido_e_rejeitado() {
        assert!(TileState::Missing.transition(TileState::Active).is_err());
    }
    #[test]
    fn plano_e_estavel() {
        assert_eq!(
            plan(
                (-48.5, -27.15),
                500.0,
                17,
                [100.0, 250.0, 400.0, 600.0],
                Seed(1)
            )
            .unwrap(),
            plan(
                (-48.5, -27.15),
                500.0,
                17,
                [100.0, 250.0, 400.0, 600.0],
                Seed(1)
            )
            .unwrap()
        );
    }
}

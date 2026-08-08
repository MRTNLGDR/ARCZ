use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacadeRequest {
    pub width_m: f64,
    pub floors: u32,
    pub floor_height_m: f64,
    pub preferred_module_width_m: f64,
    #[serde(default = "default_ground")]
    pub ground_floor_height_m: f64,
    #[serde(default)]
    pub balcony_probability: f64,
    #[serde(default)]
    pub commercial_ground_floor: bool,
}
fn default_ground() -> f64 {
    4.0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleKind {
    Wall,
    Window,
    Balcony,
    Shopfront,
    Entrance,
    Service,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacadeModule {
    pub floor: u32,
    pub column: u32,
    pub kind: ModuleKind,
    pub x_m: f64,
    pub base_m: f64,
    pub width_m: f64,
    pub height_m: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacadeLayout {
    pub columns: u32,
    pub module_width_m: f64,
    pub modules: Vec<FacadeModule>,
}

pub fn layout(
    request: &FacadeRequest,
    mut selector: impl FnMut(u32, u32) -> f64,
) -> Result<FacadeLayout, FacadeError> {
    if !request.width_m.is_finite()
        || request.width_m <= 1.0
        || request.floors == 0
        || !request.floor_height_m.is_finite()
        || request.floor_height_m <= 2.0
        || !request.preferred_module_width_m.is_finite()
        || request.preferred_module_width_m <= 0.5
        || !(0.0..=1.0).contains(&request.balcony_probability)
    {
        return Err(FacadeError::InvalidParameter);
    }
    let columns = ((request.width_m / request.preferred_module_width_m).round() as u32).max(1);
    let module_width = request.width_m / columns as f64;
    let mut modules = Vec::with_capacity((columns * request.floors) as usize);
    for floor in 0..request.floors {
        let base = if floor == 0 {
            0.0
        } else {
            request.ground_floor_height_m + (floor - 1) as f64 * request.floor_height_m
        };
        let height = if floor == 0 {
            request.ground_floor_height_m
        } else {
            request.floor_height_m
        };
        for column in 0..columns {
            let kind = if floor == 0 && request.commercial_ground_floor {
                if column == columns / 2 {
                    ModuleKind::Entrance
                } else {
                    ModuleKind::Shopfront
                }
            } else {
                let value = selector(floor, column).clamp(0.0, 1.0);
                if value < request.balcony_probability {
                    ModuleKind::Balcony
                } else if value < 0.92 {
                    ModuleKind::Window
                } else {
                    ModuleKind::Wall
                }
            };
            modules.push(FacadeModule {
                floor,
                column,
                kind,
                x_m: column as f64 * module_width,
                base_m: base,
                width_m: module_width,
                height_m: height,
            });
        }
    }
    Ok(FacadeLayout {
        columns,
        module_width_m: module_width,
        modules,
    })
}

#[derive(Debug, Error)]
pub enum FacadeError {
    #[error("parâmetro de fachada inválido")]
    InvalidParameter,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn terreo_comercial_e_diferenciado() {
        let l = layout(
            &FacadeRequest {
                width_m: 12.,
                floors: 3,
                floor_height_m: 3.,
                preferred_module_width_m: 3.,
                ground_floor_height_m: 4.,
                balcony_probability: 0.5,
                commercial_ground_floor: true,
            },
            |_, _| 0.8,
        )
        .unwrap();
        assert!(l.modules.iter().any(|m| m.kind == ModuleKind::Shopfront));
    }
}

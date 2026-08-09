use serde::{Deserialize, Serialize};
use std::ops::{Add, AddAssign};
use thiserror::Error;

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Resources {
    pub triangles: u64,
    pub instances: u64,
    pub draw_calls: u64,
    pub geometry_mb: f64,
    pub textures_mb: f64,
    pub framebuffer_mb: f64,
    pub materials: u64,
    pub vegetation_overdraw: f64,
    pub cpu_ms: f64,
    pub gpu_upload_ms: f64,
    pub cache_mb: f64,
}

impl Resources {
    pub fn validate(self) -> Result<Self, BudgetError> {
        for (name, value) in [
            ("geometry_mb", self.geometry_mb),
            ("textures_mb", self.textures_mb),
            ("framebuffer_mb", self.framebuffer_mb),
            ("vegetation_overdraw", self.vegetation_overdraw),
            ("cpu_ms", self.cpu_ms),
            ("gpu_upload_ms", self.gpu_upload_ms),
            ("cache_mb", self.cache_mb),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(BudgetError::InvalidResource(name));
            }
        }
        Ok(self)
    }

    pub fn fits_within(self, limit: Self) -> bool {
        self.triangles <= limit.triangles
            && self.instances <= limit.instances
            && self.draw_calls <= limit.draw_calls
            && self.geometry_mb <= limit.geometry_mb
            && self.textures_mb <= limit.textures_mb
            && self.framebuffer_mb <= limit.framebuffer_mb
            && self.materials <= limit.materials
            && self.vegetation_overdraw <= limit.vegetation_overdraw
            && self.cpu_ms <= limit.cpu_ms
            && self.gpu_upload_ms <= limit.gpu_upload_ms
            && self.cache_mb <= limit.cache_mb
    }

    pub fn exceedances(self, limit: Self) -> Vec<Exceedance> {
        let mut result = Vec::new();
        macro_rules! check {
            ($field:ident) => {
                if (self.$field as f64) > (limit.$field as f64) {
                    result.push(Exceedance {
                        resource: stringify!($field).to_owned(),
                        requested: self.$field as f64,
                        limit: limit.$field as f64,
                    });
                }
            };
        }
        check!(triangles);
        check!(instances);
        check!(draw_calls);
        check!(geometry_mb);
        check!(textures_mb);
        check!(framebuffer_mb);
        check!(materials);
        check!(vegetation_overdraw);
        check!(cpu_ms);
        check!(gpu_upload_ms);
        check!(cache_mb);
        result
    }

    pub fn saturating_sub(self, other: Self) -> Self {
        Self {
            triangles: self.triangles.saturating_sub(other.triangles),
            instances: self.instances.saturating_sub(other.instances),
            draw_calls: self.draw_calls.saturating_sub(other.draw_calls),
            geometry_mb: (self.geometry_mb - other.geometry_mb).max(0.0),
            textures_mb: (self.textures_mb - other.textures_mb).max(0.0),
            framebuffer_mb: (self.framebuffer_mb - other.framebuffer_mb).max(0.0),
            materials: self.materials.saturating_sub(other.materials),
            vegetation_overdraw: (self.vegetation_overdraw - other.vegetation_overdraw).max(0.0),
            cpu_ms: (self.cpu_ms - other.cpu_ms).max(0.0),
            gpu_upload_ms: (self.gpu_upload_ms - other.gpu_upload_ms).max(0.0),
            cache_mb: (self.cache_mb - other.cache_mb).max(0.0),
        }
    }
}

impl Add for Resources {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        let mut value = self;
        value += rhs;
        value
    }
}
impl AddAssign for Resources {
    fn add_assign(&mut self, rhs: Self) {
        self.triangles = self.triangles.saturating_add(rhs.triangles);
        self.instances = self.instances.saturating_add(rhs.instances);
        self.draw_calls = self.draw_calls.saturating_add(rhs.draw_calls);
        self.geometry_mb += rhs.geometry_mb;
        self.textures_mb += rhs.textures_mb;
        self.framebuffer_mb += rhs.framebuffer_mb;
        self.materials = self.materials.saturating_add(rhs.materials);
        self.vegetation_overdraw += rhs.vegetation_overdraw;
        self.cpu_ms += rhs.cpu_ms;
        self.gpu_upload_ms += rhs.gpu_upload_ms;
        self.cache_mb += rhs.cache_mb;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Decision {
    Accept,
    Degrade,
    Split,
    Defer,
    Confirm,
    Reject,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Exceedance {
    pub resource: String,
    pub requested: f64,
    pub limit: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evaluation {
    pub requested: Resources,
    pub available: Resources,
    pub decision: Decision,
    pub exceedances: Vec<Exceedance>,
}

pub fn evaluate(requested: Resources, available: Resources) -> Result<Evaluation, BudgetError> {
    requested.validate()?;
    available.validate()?;
    let exceedances = requested.exceedances(available);
    let splittable = exceedances.iter().all(|item| {
        matches!(
            item.resource.as_str(),
            "triangles" | "instances" | "draw_calls" | "geometry_mb" | "textures_mb" | "cache_mb"
        )
    });
    let decision = if exceedances.is_empty() {
        Decision::Accept
    } else if splittable {
        Decision::Split
    } else {
        Decision::Reject
    };
    Ok(Evaluation {
        requested,
        available,
        decision,
        exceedances,
    })
}

#[derive(Debug, Error)]
pub enum BudgetError {
    #[error("recurso inválido: {0}")]
    InvalidResource(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn excedente_geometrico_pode_dividir() {
        let req = Resources {
            triangles: 20,
            ..Default::default()
        };
        let lim = Resources {
            triangles: 10,
            ..Default::default()
        };
        assert_eq!(evaluate(req, lim).unwrap().decision, Decision::Split);
    }
}

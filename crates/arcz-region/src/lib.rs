use arcz_determinism::sha256_hex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionRequest {
    pub region_id: String,
    pub bbox_wgs84: [f64; 4],
    #[serde(default)]
    pub polygon_wgs84: Vec<[f64; 2]>,
    pub focus: GeoPoint,
    pub scale: String,
    pub requested_radius_m: f64,
    pub sources: SourceFlags,
    #[serde(default)]
    pub generation_epoch: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct GeoPoint {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourceFlags {
    pub osm: bool,
    pub overture: bool,
    pub dem: bool,
    pub imagery: bool,
    pub street: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub field: String,
    pub value: Value,
    pub source: String,
    pub source_ref: String,
    pub confidence: f64,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionContext {
    pub schema_version: u32,
    pub region_id: String,
    pub crs_work: String,
    pub origin_wgs84: [f64; 3],
    pub terrain: TerrainContext,
    pub urban: Value,
    pub environment: EnvironmentContext,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub source_packages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerrainContext {
    pub min_m: Option<f64>,
    pub max_m: Option<f64>,
    pub mean_slope_deg: Option<f64>,
    #[serde(default)]
    pub slope_classes: Map<String, Value>,
    pub confidence: f64,
    pub vertical_error_m: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentContext {
    pub biome: String,
    pub climate_profile: String,
    pub soil_profile: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionalProfile {
    pub id: String,
    pub version: String,
    #[serde(flatten)]
    pub body: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposedProfile {
    pub profile: Value,
    pub applied: Vec<String>,
    pub profile_hash: String,
}

impl RegionRequest {
    pub fn validate(&self) -> Result<(), RegionError> {
        let [west, south, east, north] = self.bbox_wgs84;
        if !(west.is_finite() && south.is_finite() && east.is_finite() && north.is_finite()) {
            return Err(RegionError::NonFinite);
        }
        if west >= east || south >= north {
            return Err(RegionError::InvalidBbox);
        }
        if !(-180.0..=180.0).contains(&self.focus.lon)
            || !(-85.051_128_779_806_6..=85.051_128_779_806_6).contains(&self.focus.lat)
        {
            return Err(RegionError::InvalidFocus);
        }
        if !self.requested_radius_m.is_finite() || self.requested_radius_m <= 0.0 {
            return Err(RegionError::InvalidRadius);
        }
        Ok(())
    }
}

pub fn compose_profiles(
    layers: &[Value],
    user_override: Option<&Value>,
) -> Result<ComposedProfile, RegionError> {
    if layers.is_empty() {
        return Err(RegionError::NoProfiles);
    }
    let mut result = Value::Object(Map::new());
    let mut applied = Vec::new();
    for layer in layers {
        let id = layer
            .get("id")
            .and_then(Value::as_str)
            .ok_or(RegionError::ProfileIdentity)?;
        let version = layer
            .get("version")
            .and_then(Value::as_str)
            .ok_or(RegionError::ProfileIdentity)?;
        deep_merge(&mut result, layer.clone());
        applied.push(format!("{id}@{version}"));
    }
    if let Some(value) = user_override {
        deep_merge(&mut result, value.clone());
        applied.push("user_override".to_owned());
    }
    for pointer in [
        "/architecture/building_mix",
        "/roofs/types",
        "/roofs/materials",
        "/facades/materials",
    ] {
        normalize_distribution(&mut result, pointer)?;
    }
    let canonical = canonical_json(&result);
    Ok(ComposedProfile {
        profile: result,
        applied,
        profile_hash: sha256_hex(canonical),
    })
}

fn deep_merge(target: &mut Value, layer: Value) {
    match (target, layer) {
        (Value::Object(base), Value::Object(next)) => {
            for (key, value) in next {
                match base.get_mut(&key) {
                    Some(existing) => deep_merge(existing, value),
                    None => {
                        base.insert(key, value);
                    }
                }
            }
        }
        (target, value) => *target = value,
    }
}

fn normalize_distribution(root: &mut Value, pointer: &str) -> Result<(), RegionError> {
    let Some(value) = root.pointer_mut(pointer) else {
        return Err(RegionError::MissingDistribution(pointer.to_owned()));
    };
    let object = value
        .as_object_mut()
        .ok_or_else(|| RegionError::InvalidDistribution(pointer.to_owned()))?;
    let mut total = 0.0;
    for value in object.values() {
        let weight = value
            .as_f64()
            .ok_or_else(|| RegionError::InvalidDistribution(pointer.to_owned()))?;
        if !weight.is_finite() || weight < 0.0 {
            return Err(RegionError::InvalidDistribution(pointer.to_owned()));
        }
        total += weight;
    }
    if total <= 0.0 {
        return Err(RegionError::InvalidDistribution(pointer.to_owned()));
    }
    for value in object.values_mut() {
        *value = Value::from(value.as_f64().unwrap_or(0.0) / total);
    }
    Ok(())
}

pub fn canonical_json(value: &Value) -> Vec<u8> {
    fn sorted(value: &Value) -> Value {
        match value {
            Value::Object(map) => {
                let mut keys: Vec<_> = map.keys().collect();
                keys.sort();
                let mut output = Map::new();
                for key in keys {
                    output.insert(key.clone(), sorted(&map[key]));
                }
                Value::Object(output)
            }
            Value::Array(values) => Value::Array(values.iter().map(sorted).collect()),
            value => value.clone(),
        }
    }
    serde_json::to_vec(&sorted(value)).expect("JSON profile is serializable")
}

#[derive(Debug, Error)]
pub enum RegionError {
    #[error("coordenada não finita")]
    NonFinite,
    #[error("bbox inválida")]
    InvalidBbox,
    #[error("foco inválido")]
    InvalidFocus,
    #[error("raio inválido")]
    InvalidRadius,
    #[error("nenhum perfil fornecido")]
    NoProfiles,
    #[error("perfil sem id/version")]
    ProfileIdentity,
    #[error("distribuição ausente: {0}")]
    MissingDistribution(String),
    #[error("distribuição inválida: {0}")]
    InvalidDistribution(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normaliza_pesos() {
        let profile = serde_json::json!({"id":"x","version":"1","architecture":{"building_mix":{"a":2,"b":2}},"roofs":{"types":{"flat":1},"materials":{"tile":1}},"facades":{"materials":{"paint":1}}});
        let composed = compose_profiles(&[profile], None).unwrap();
        assert_eq!(
            composed
                .profile
                .pointer("/architecture/building_mix/a")
                .unwrap()
                .as_f64(),
            Some(0.5)
        );
    }
}

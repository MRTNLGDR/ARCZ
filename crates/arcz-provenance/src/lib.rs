//! Rastreabilidade de proveniência e licenças no ARCZ Earth.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum LicenseType {
    ODbL,        // OpenStreetMap
    CC0,         // PolyHaven / Kenney
    CcBy40,      // Creative Commons Attribution 4.0
    Apache20,    // CesiumJS / MapAnything
    MIT,
    Proprietary,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalSourceRecord {
    pub source_id: String,
    pub provider_name: String,
    pub license: LicenseType,
    pub version: String,
    pub checksum_sha256: String,
    pub cache_allowed: bool,
    pub confidence_level: String,
}

impl ExternalSourceRecord {
    pub fn new(
        source_id: impl Into<String>,
        provider_name: impl Into<String>,
        license: LicenseType,
        checksum: impl Into<String>,
    ) -> Self {
        Self {
            source_id: source_id.into(),
            provider_name: provider_name.into(),
            license,
            version: "2026.07".to_string(),
            checksum_sha256: checksum.into(),
            cache_allowed: license != LicenseType::Proprietary,
            confidence_level: "High".to_string(),
        }
    }

    pub fn is_commercially_safe(&self) -> bool {
        matches!(
            self.license,
            LicenseType::ODbL | LicenseType::CC0 | LicenseType::CcBy40 | LicenseType::Apache20 | LicenseType::MIT
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provenance_commercial_safety() {
        let osm_source = ExternalSourceRecord::new("osm-1", "OpenStreetMap", LicenseType::ODbL, "a1b2c3d4");
        assert!(osm_source.is_commercially_safe());

        let unknown_source = ExternalSourceRecord::new("unk-1", "ThirdParty", LicenseType::Proprietary, "12345678");
        assert!(!unknown_source.is_commercially_safe());
    }
}

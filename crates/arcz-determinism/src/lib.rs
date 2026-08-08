//! Determinismo transversal do ARCZ.
//!
//! Nunca use ordem de conclusão assíncrona, endereço de memória ou `rand()` do
//! sistema para derivar geometria. Uma entidade recebe a mesma semente para a
//! mesma cadeia projeto → região → tile → lote → edifício → componente.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Seed(pub u64);

impl Seed {
    pub const ZERO: Self = Self(0);

    pub fn derive(self, namespace: &str, key: impl AsRef<[u8]>) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(self.0.to_le_bytes());
        hasher.update((namespace.len() as u64).to_le_bytes());
        hasher.update(namespace.as_bytes());
        let key = key.as_ref();
        hasher.update((key.len() as u64).to_le_bytes());
        hasher.update(key);
        let digest = hasher.finalize();
        Self(u64::from_le_bytes(
            digest[0..8].try_into().expect("slice length"),
        ))
    }

    pub fn from_parts(parts: &[&[u8]]) -> Self {
        let mut hasher = Sha256::new();
        for part in parts {
            hasher.update((part.len() as u64).to_le_bytes());
            hasher.update(part);
        }
        let digest = hasher.finalize();
        Self(u64::from_le_bytes(
            digest[0..8].try_into().expect("slice length"),
        ))
    }

    pub fn rng(self) -> StableRng {
        StableRng::new(self)
    }
}

/// SplitMix64: pequeno, rápido e com sequência totalmente especificada.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StableRng {
    state: u64,
}

impl StableRng {
    pub fn new(seed: Seed) -> Self {
        Self { state: seed.0 }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub fn next_f64(&mut self) -> f64 {
        // 53 bits de mantissa; intervalo [0, 1).
        ((self.next_u64() >> 11) as f64) * (1.0 / ((1_u64 << 53) as f64))
    }

    pub fn range_f64(&mut self, min: f64, max: f64) -> f64 {
        assert!(min.is_finite() && max.is_finite() && min <= max);
        min + (max - min) * self.next_f64()
    }

    pub fn range_usize(&mut self, start: usize, end_exclusive: usize) -> usize {
        assert!(start < end_exclusive);
        let span = (end_exclusive - start) as u64;
        // Rejeição evita viés de módulo.
        let zone = u64::MAX - (u64::MAX % span);
        loop {
            let value = self.next_u64();
            if value < zone {
                return start + (value % span) as usize;
            }
        }
    }

    pub fn chance(&mut self, probability: f64) -> bool {
        assert!((0.0..=1.0).contains(&probability));
        self.next_f64() < probability
    }

    pub fn choose_weighted<'a, T>(&mut self, choices: &'a [(T, f64)]) -> Option<&'a T> {
        let total: f64 = choices.iter().map(|(_, weight)| weight.max(0.0)).sum();
        if !(total > 0.0) {
            return None;
        }
        let mut cursor = self.range_f64(0.0, total);
        for (value, weight) in choices {
            cursor -= weight.max(0.0);
            if cursor <= 0.0 {
                return Some(value);
            }
        }
        choices.last().map(|(value, _)| value)
    }
}

pub fn sha256_hex(bytes: impl AsRef<[u8]>) -> String {
    let digest = Sha256::digest(bytes.as_ref());
    let mut output = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mesma_cadeia_produz_mesma_semente() {
        let a = Seed(7)
            .derive("region", b"bombinhas")
            .derive("tile", b"18/1/2");
        let b = Seed(7)
            .derive("region", b"bombinhas")
            .derive("tile", b"18/1/2");
        assert_eq!(a, b);
    }

    #[test]
    fn rng_e_reproduzivel() {
        let mut a = Seed(42).rng();
        let mut b = Seed(42).rng();
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }
}

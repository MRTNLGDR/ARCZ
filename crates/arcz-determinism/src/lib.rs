use sha2::{Digest, Sha256};

#[derive(Debug, Clone)]
pub struct DeterministicRng {
    state: u64,
}

impl DeterministicRng {
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0x9e3779b97f4a7c15
            } else {
                seed
            },
        }
    }

    pub fn next_u64(&mut self) -> u64 {
        // SplitMix64. Pequeno, rápido e totalmente especificado para replay.
        self.state = self.state.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }

    pub fn next_f64(&mut self) -> f64 {
        const SCALE: f64 = 1.0 / ((1_u64 << 53) as f64);
        ((self.next_u64() >> 11) as f64) * SCALE
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
        // Preserve the previous semantics for zero/negative/NaN totals while
        // making f64's partial ordering explicit for Rust 1.97 Clippy.
        if total.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
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
    fn replay_is_stable_for_same_seed() {
        let mut a = DeterministicRng::new(7);
        let mut b = DeterministicRng::new(7);
        let left: Vec<_> = (0..20).map(|_| a.next_u64()).collect();
        let right: Vec<_> = (0..20).map(|_| b.next_u64()).collect();
        assert_eq!(left, right);
    }

    #[test]
    fn weighted_choice_rejects_non_positive_total() {
        let mut rng = DeterministicRng::new(1);
        let choices = [("a", 0.0), ("b", -2.0)];
        assert_eq!(rng.choose_weighted(&choices), None);
    }
}

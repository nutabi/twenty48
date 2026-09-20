//! A small seedable random number generator.
//!
//! The engine needs randomness to place new tiles, but pulling in a crate for
//! sixteen lines of arithmetic is not worth it. This is `splitmix64`: fast,
//! good enough for a game, and — more importantly — reproducible from a seed so
//! that games can be replayed and tests can be deterministic.

/// Returns a seed drawn from the system clock.
///
/// Used when a caller starts a game without naming a seed. The value is always
/// reported back, so a clock-seeded game replays exactly like any other.
pub fn entropy_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0x2048, |elapsed| elapsed.as_nanos() as u64)
}

/// A `splitmix64` generator.
#[derive(Debug, Clone)]
pub struct Rng {
    seed: u64,
    state: u64,
}

impl Rng {
    /// Creates a generator that will always produce the same sequence for the
    /// same `seed`.
    pub const fn new(seed: u64) -> Self {
        Self { seed, state: seed }
    }

    /// Returns the seed this generator was created with.
    pub const fn seed(&self) -> u64 {
        self.seed
    }

    /// Returns the next value in the sequence.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Returns the high half of the next value, which is the better-mixed one.
    pub fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    /// Returns a uniformly distributed value in `0..bound`.
    ///
    /// Uses Lemire's multiply-and-reject method, so the result carries no
    /// modulo bias.
    ///
    /// # Panics
    ///
    /// Panics if `bound` is zero.
    pub fn below(&mut self, bound: u32) -> u32 {
        assert!(bound > 0, "bound must be non-zero");

        let mut product = u64::from(self.next_u32()) * u64::from(bound);
        if (product as u32) < bound {
            // The low `2^32 % bound` products are over-represented; redraw
            // until we land outside that window.
            let threshold = bound.wrapping_neg() % bound;
            while (product as u32) < threshold {
                product = u64::from(self.next_u32()) * u64::from(bound);
            }
        }
        (product >> 32) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::Rng;

    #[test]
    fn same_seed_gives_same_sequence() {
        let mut left = Rng::new(42);
        let mut right = Rng::new(42);
        for _ in 0..100 {
            assert_eq!(left.next_u64(), right.next_u64());
        }
    }

    #[test]
    fn the_seed_is_remembered() {
        let mut rng = Rng::new(99);
        rng.next_u64();
        assert_eq!(rng.seed(), 99);
    }

    #[test]
    fn different_seeds_diverge() {
        let mut left = Rng::new(1);
        let mut right = Rng::new(2);
        assert_ne!(left.next_u64(), right.next_u64());
    }

    #[test]
    fn below_stays_in_range_and_covers_it() {
        let mut rng = Rng::new(7);
        let mut seen = [0usize; 10];
        for _ in 0..10_000 {
            let value = rng.below(10);
            assert!(value < 10);
            seen[value as usize] += 1;
        }
        assert!(seen.iter().all(|&count| count > 800), "skewed: {seen:?}");
    }

    #[test]
    fn below_one_is_always_zero() {
        let mut rng = Rng::new(3);
        assert_eq!(rng.below(1), 0);
    }
}

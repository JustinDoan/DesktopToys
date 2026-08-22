//! PCG32 so a roll can be replayed from a seed without pulling in a rand crate.

#[derive(Clone, Debug)]
pub struct Rng {
    state: u64,
    increment: u64,
}

impl Rng {
    pub fn from_seed(seed: u64) -> Self {
        let mut rng = Self {
            state: 0,
            increment: (seed << 1) | 1,
        };
        rng.next_u32();
        rng.state = rng.state.wrapping_add(seed);
        rng.next_u32();
        rng
    }

    pub fn next_u32(&mut self) -> u32 {
        let previous = self.state;
        self.state = previous
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(self.increment);
        let xor_shifted = (((previous >> 18) ^ previous) >> 27) as u32;
        let rotation = (previous >> 59) as u32;
        xor_shifted.rotate_right(rotation)
    }

    /// Uniform in `[0, 1)`.
    pub fn unit(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Uniform in `[-1, 1)`.
    pub fn signed(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }

    pub fn range(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * self.unit()
    }

    pub fn below(&mut self, limit: usize) -> usize {
        if limit == 0 {
            return 0;
        }
        (self.next_u32() as usize) % limit
    }

    pub fn chance(&mut self, probability: f32) -> bool {
        self.unit() < probability
    }
}

#[cfg(test)]
mod tests {
    use super::Rng;

    #[test]
    fn same_seed_replays_the_same_stream() {
        let mut left = Rng::from_seed(0xC0FFEE);
        let mut right = Rng::from_seed(0xC0FFEE);
        for _ in 0..64 {
            assert_eq!(left.next_u32(), right.next_u32());
        }
    }

    #[test]
    fn unit_stays_inside_the_half_open_range() {
        let mut rng = Rng::from_seed(7);
        for _ in 0..10_000 {
            let value = rng.unit();
            assert!((0.0..1.0).contains(&value), "{value} escaped [0, 1)");
        }
    }

    #[test]
    fn distinct_seeds_diverge() {
        let mut left = Rng::from_seed(1);
        let mut right = Rng::from_seed(2);
        assert_ne!(left.next_u32(), right.next_u32());
    }
}

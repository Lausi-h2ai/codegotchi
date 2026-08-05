/// A small deterministic source for cosmetic randomness.
///
/// Important domain transitions must not call this port.
pub trait RandomSource {
    fn next_u64(&mut self) -> u64;

    /// Returns a value in `[0, 1)`.
    fn next_f32(&mut self) -> f32 {
        const MANTISSA_SCALE: f32 = 1.0 / 16_777_216.0;
        (self.next_u64() >> 40) as f32 * MANTISSA_SCALE
    }

    /// Returns a value in `[0, upper_exclusive)`, or zero for an empty range.
    fn next_bounded(&mut self, upper_exclusive: u64) -> u64 {
        if upper_exclusive == 0 {
            0
        } else {
            self.next_u64() % upper_exclusive
        }
    }
}

const ZERO_SEED_STATE: u64 = 0x9E37_79B9_7F4A_7C15;

/// A repeatable xorshift source with a nonzero state for every seed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeededRandomSource {
    state: u64,
}

impl SeededRandomSource {
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { ZERO_SEED_STATE } else { seed },
        }
    }
}

impl RandomSource for SeededRandomSource {
    fn next_u64(&mut self) -> u64 {
        let mut state = self.state;
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        self.state = state;
        state
    }
}

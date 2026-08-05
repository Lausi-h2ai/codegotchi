use thiserror::Error;

const DIGESTION_PER_POOP: u64 = 100;
const WORK_PER_POOP: u64 = 50;

/// A validated pair of point thresholds selected by a poop strategy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PoopGenerationThreshold {
    digestion_points: u64,
    work_points: u64,
}

impl PoopGenerationThreshold {
    pub fn new(digestion_points: u64, work_points: u64) -> Result<Self, PoopThresholdError> {
        if digestion_points == 0 {
            return Err(PoopThresholdError::ZeroDigestionPoints);
        }
        if work_points == 0 {
            return Err(PoopThresholdError::ZeroWorkPoints);
        }
        Ok(Self {
            digestion_points,
            work_points,
        })
    }

    pub fn digestion_points(self) -> u64 {
        self.digestion_points
    }

    pub fn work_points(self) -> u64 {
        self.work_points
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum PoopThresholdError {
    #[error("poop digestion threshold must be greater than zero")]
    ZeroDigestionPoints,
    #[error("poop work threshold must be greater than zero")]
    ZeroWorkPoints,
}

/// Selects a validated threshold without receiving mutable aggregate access.
pub trait PoopGenerationStrategy {
    fn threshold(&self, digestion_points: u64, work_points: u64)
    -> Option<PoopGenerationThreshold>;
}

/// The deterministic threshold strategy for the domain slice.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultPoopGenerationStrategy;

impl PoopGenerationStrategy for DefaultPoopGenerationStrategy {
    fn threshold(
        &self,
        _digestion_points: u64,
        _work_points: u64,
    ) -> Option<PoopGenerationThreshold> {
        // Both domain constants are nonzero; the public constructor validates
        // custom thresholds before they can reach the simulation loop.
        Some(PoopGenerationThreshold {
            digestion_points: DIGESTION_PER_POOP,
            work_points: WORK_PER_POOP,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_strategy_keeps_both_thresholds() {
        let strategy = DefaultPoopGenerationStrategy;
        let threshold = strategy.threshold(0, 0).unwrap();
        assert_eq!(threshold.digestion_points(), DIGESTION_PER_POOP);
        assert_eq!(threshold.work_points(), WORK_PER_POOP);
    }
}

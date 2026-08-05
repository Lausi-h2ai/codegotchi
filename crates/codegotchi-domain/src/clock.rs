use std::sync::{Arc, RwLock};

use chrono::{DateTime, Duration, Utc};

/// Supplies the current UTC time to domain objects.
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

/// A clock backed by the host's UTC wall clock.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// A cloneable, controllable clock for deterministic domain tests and simulations.
#[derive(Clone, Debug)]
pub struct FakeClock {
    now: Arc<RwLock<DateTime<Utc>>>,
}

impl FakeClock {
    pub fn new(now: DateTime<Utc>) -> Self {
        Self {
            now: Arc::new(RwLock::new(now)),
        }
    }

    pub fn set(&self, now: DateTime<Utc>) {
        let mut current = self
            .now
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *current = now;
    }

    /// Moves the clock by a duration, leaving it unchanged if the duration overflows chrono's range.
    pub fn advance(&self, duration: Duration) {
        let mut current = self
            .now
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(next) = current.checked_add_signed(duration) {
            *current = next;
        }
    }
}

impl Clock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        *self
            .now
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl<T> Clock for Arc<T>
where
    T: Clock + ?Sized,
{
    fn now(&self) -> DateTime<Utc> {
        self.as_ref().now()
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone, Utc};

    use super::{Clock, FakeClock, SystemClock};

    #[test]
    fn fake_clock_advances_across_clones() {
        let start = Utc.with_ymd_and_hms(2026, 8, 4, 12, 0, 0).unwrap();
        let clock = FakeClock::new(start);
        let clone = clock.clone();

        clock.advance(Duration::hours(2));

        assert_eq!(clock.now(), start + Duration::hours(2));
        assert_eq!(clone.now(), start + Duration::hours(2));
    }

    #[test]
    fn fake_clock_can_be_moved_backwards_without_panicking() {
        let start = Utc.with_ymd_and_hms(2026, 8, 4, 12, 0, 0).unwrap();
        let clock = FakeClock::new(start);

        clock.advance(Duration::hours(-3));

        assert_eq!(clock.now(), start - Duration::hours(3));
    }

    #[test]
    // This is intentionally a live-clock smoke test; deterministic domain behavior uses FakeClock.
    fn system_clock_returns_a_current_utc_timestamp() {
        let before = Utc::now();
        let now = SystemClock.now();
        let after = Utc::now();

        assert!(now >= before);
        assert!(now <= after);
    }
}

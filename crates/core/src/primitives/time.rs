//! An absolute instant on the UTC timeline.
//!
//! [`UtcTicks`] counts 100 ns ticks since 1601-01-01 UTC. That is the unit
//! PowerKit's history files already store, so adopting it as the neutral type
//! keeps every stored value valid. It is a plain number with a fixed epoch:
//! any platform converts it exactly to and from Unix time, and only a platform
//! crate reads a system clock or a file time.

/// Ticks between 1601-01-01 and 1970-01-01 UTC.
const UNIX_EPOCH_TICKS: i64 = 116_444_736_000_000_000;
const TICKS_PER_MS: i64 = 10_000;

/// 100 ns ticks since 1601-01-01 UTC.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UtcTicks(pub i64);

impl UtcTicks {
    /// From milliseconds since the Unix epoch; saturates at the ends of the range.
    pub const fn from_unix_ms(ms: i64) -> Self {
        Self(
            ms.saturating_mul(TICKS_PER_MS)
                .saturating_add(UNIX_EPOCH_TICKS),
        )
    }

    /// Milliseconds since the Unix epoch, rounded toward the past.
    pub const fn unix_ms(self) -> i64 {
        self.0
            .saturating_sub(UNIX_EPOCH_TICKS)
            .div_euclid(TICKS_PER_MS)
    }

    /// Whole milliseconds from `earlier` to `self`, rounded up; `None` unless
    /// `self` is later.
    pub fn ms_after(self, earlier: Self) -> Option<u64> {
        let ticks = self.0.checked_sub(earlier.0).filter(|ticks| *ticks > 0)?;
        Some((ticks as u64).div_ceil(TICKS_PER_MS as u64))
    }

    /// `self` moved `ms` milliseconds into the future; saturates.
    pub const fn plus_ms(self, ms: u64) -> Self {
        let ticks = if ms > (i64::MAX / TICKS_PER_MS) as u64 {
            i64::MAX
        } else {
            ms as i64 * TICKS_PER_MS
        };
        Self(self.0.saturating_add(ticks))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_time_converts_exactly_in_both_directions() {
        assert_eq!(UtcTicks::from_unix_ms(0).0, UNIX_EPOCH_TICKS);
        assert_eq!(UtcTicks(UNIX_EPOCH_TICKS).unix_ms(), 0);
        // 2026-01-01T00:00:00Z.
        let ms = 1_767_225_600_000;
        assert_eq!(UtcTicks::from_unix_ms(ms).unix_ms(), ms);
        // Before 1970 rounds toward the past, not toward zero.
        assert_eq!(UtcTicks(UNIX_EPOCH_TICKS - 1).unix_ms(), -1);
        assert_eq!(UtcTicks::from_unix_ms(i64::MAX).0, i64::MAX);
    }

    #[test]
    fn remaining_time_rounds_up_and_needs_a_later_instant() {
        let now = UtcTicks(1_000_000);
        assert_eq!(now.ms_after(now), None);
        assert_eq!(UtcTicks(999_999).ms_after(now), None);
        assert_eq!(UtcTicks(1_000_001).ms_after(now), Some(1));
        assert_eq!(UtcTicks(1_010_000).ms_after(now), Some(1));
        assert_eq!(UtcTicks(1_010_001).ms_after(now), Some(2));
        assert_eq!(now.plus_ms(3).0, 1_030_000);
        assert_eq!(now.plus_ms(u64::MAX).0, i64::MAX);
    }
}

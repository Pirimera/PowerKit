//! Platform-independent AlwaysOn session state. Deadlines use monotonic uptime.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Off,
    Indefinite,
    Until(u64),
}
impl Mode {
    pub fn timed(now_ms: u64, duration_ms: u64) -> Self {
        Self::Until(now_ms.saturating_add(duration_ms))
    }
    pub fn expired(self, now_ms: u64) -> bool {
        matches!(self, Self::Until(end) if now_ms >= end)
    }
    pub fn active(self) -> bool {
        self != Self::Off
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deadline_boundary_and_resume_past_deadline() {
        let mode = Mode::timed(100, 1000);
        assert!(!mode.expired(1099));
        assert!(mode.expired(1100));
        assert!(mode.expired(9000));
    }
    #[test]
    fn persistent_modes_do_not_expire() {
        assert!(!Mode::Off.active());
        assert!(Mode::Indefinite.active());
        assert!(!Mode::Indefinite.expired(u64::MAX));
        assert!(!Mode::Off.expired(u64::MAX));
    }
    #[test]
    fn deadline_does_not_wrap() {
        assert_eq!(Mode::timed(u64::MAX - 1, 100), Mode::Until(u64::MAX));
    }
}

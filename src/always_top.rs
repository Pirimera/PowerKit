//! Platform-independent AlwaysTop session. Pins are not persisted across launches.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Pin {
    #[default]
    Off,
    On {
        title: String,
    },
}
impl Pin {
    pub fn on(title: impl Into<String>) -> Self {
        Self::On {
            title: sanitize_title(title.into()),
        }
    }
    pub fn active(&self) -> bool {
        matches!(self, Self::On { .. })
    }
    pub fn title(&self) -> Option<&str> {
        match self {
            Self::Off => None,
            Self::On { title } => Some(title),
        }
    }
    pub fn summary(count: usize, last_title: impl Into<String>) -> Self {
        match count {
            0 => Self::Off,
            1 => Self::on(last_title),
            n => Self::on(format!("{n} pencere")),
        }
    }
}
fn sanitize_title(title: String) -> String {
    let title: String = title
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    const MAX: usize = 32;
    let mut chars = title.chars();
    let mut clipped: String = chars.by_ref().take(MAX).collect();
    if chars.next().is_some() {
        clipped.push('…');
    }
    if clipped.is_empty() {
        "Pencere".into()
    } else {
        clipped
    }
}
pub const MIN_OPACITY: i32 = 20;
pub const MAX_OPACITY: i32 = 100;
pub const OPACITY_STEP: i32 = 10;
pub fn step_opacity(current: i32, delta: i32) -> i32 {
    current
        .saturating_add(delta)
        .clamp(MIN_OPACITY, MAX_OPACITY)
}
pub fn alpha_from_percent(percent: i32) -> u8 {
    ((255 * percent.clamp(MIN_OPACITY, MAX_OPACITY)) / 100) as u8
}
pub fn percent_from_alpha(alpha: u8) -> i32 {
    (i32::from(alpha) * 100) / 255
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_and_control_titles_become_a_visible_label() {
        assert_eq!(
            Pin::on(""),
            Pin::On {
                title: "Pencere".into()
            }
        );
        assert_eq!(
            Pin::on("\u{0}\n\t"),
            Pin::On {
                title: "Pencere".into()
            }
        );
        assert!(!Pin::Off.active());
        assert_eq!(Pin::Off.title(), None);
    }
    #[test]
    fn titles_are_collapsed_and_clipped_for_status() {
        let pin = Pin::on("  Notepad\n  -  untitled  ");
        assert_eq!(pin.title(), Some("Notepad - untitled"));
        assert!(pin.active());
        let long = "A".repeat(40);
        let clipped = Pin::on(long);
        assert_eq!(clipped.title().unwrap().chars().count(), 33);
        assert!(clipped.title().unwrap().ends_with('…'));
    }
    #[test]
    fn opacity_steps_match_powertoys_bounds() {
        assert_eq!(step_opacity(100, -OPACITY_STEP), 90);
        assert_eq!(step_opacity(25, -OPACITY_STEP), MIN_OPACITY);
        assert_eq!(step_opacity(20, OPACITY_STEP), 30);
        assert_eq!(step_opacity(95, OPACITY_STEP), MAX_OPACITY);
        assert_eq!(alpha_from_percent(100), 255);
        assert_eq!(alpha_from_percent(20), 51);
        assert_eq!(percent_from_alpha(255), 100);
        assert_eq!(percent_from_alpha(51), 20);
    }
    #[test]
    fn multiple_pins_use_a_count_label() {
        assert_eq!(Pin::summary(0, "Notepad"), Pin::Off);
        assert_eq!(Pin::summary(1, "Notepad"), Pin::on("Notepad"));
        assert_eq!(Pin::summary(2, "Notepad").title(), Some("2 pencere"));
        assert!(Pin::summary(3, "Explorer").active());
    }
}

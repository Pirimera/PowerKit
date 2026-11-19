//! Platform-independent AlwaysTop session. Pins are not persisted across launches.
//! Hotkey chords live in `hotkey.rs`; the status text is localized in `locale.rs`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Pin {
    #[default]
    Off,
    /// One pinned window. An empty title is shown as the localized "untitled" label.
    On { title: String },
    /// Several pinned windows; the caller formats the count in the current language.
    Several { count: usize },
}
impl Pin {
    pub fn on(title: impl Into<String>) -> Self {
        Self::On {
            title: sanitize_title(title.into()),
        }
    }
    pub fn active(&self) -> bool {
        !matches!(self, Self::Off)
    }
    /// The title of the single pinned window; `None` when nothing or several are pinned.
    pub fn title(&self) -> Option<&str> {
        match self {
            Self::On { title } => Some(title),
            Self::Off | Self::Several { .. } => None,
        }
    }
    pub fn summary(count: usize, last_title: impl Into<String>) -> Self {
        match count {
            0 => Self::Off,
            1 => Self::on(last_title),
            count => Self::Several { count },
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
    clipped
}
pub const MIN_OPACITY: i32 = 20;
pub const MAX_OPACITY: i32 = 100;
pub const OPACITY_STEP: i32 = 10;
pub const MIN_FRAME_THICKNESS: i32 = 1;
pub const MAX_FRAME_THICKNESS: i32 = 32;
pub const DEFAULT_FRAME_THICKNESS: i32 = 4;
pub const MIN_FRAME_OPACITY: i32 = 10;
pub const MAX_FRAME_OPACITY: i32 = 100;
pub const DEFAULT_FRAME_OPACITY: i32 = 100;
pub fn clamp_frame_thickness(value: i32) -> i32 {
    value.clamp(MIN_FRAME_THICKNESS, MAX_FRAME_THICKNESS)
}
pub fn clamp_frame_opacity(value: i32) -> i32 {
    value.clamp(MIN_FRAME_OPACITY, MAX_FRAME_OPACITY)
}
pub fn step_opacity(current: i32, delta: i32) -> i32 {
    current
        .saturating_add(delta)
        .clamp(MIN_OPACITY, MAX_OPACITY)
}
pub fn alpha_from_percent(percent: i32) -> u8 {
    ((255 * percent.clamp(MIN_OPACITY, MAX_OPACITY)) / 100) as u8
}
/// Nearest percent, so `alpha_from_percent` round-trips and 10% steps do not drift.
pub fn percent_from_alpha(alpha: u8) -> i32 {
    (i32::from(alpha) * 100 + 127) / 255
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_titles_stay_empty_for_the_localized_label() {
        assert_eq!(
            Pin::on(""),
            Pin::On {
                title: String::new()
            }
        );
        assert_eq!(
            Pin::on("\u{0}\n\t"),
            Pin::On {
                title: String::new()
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
    fn opacity_steps_do_not_drift_through_alpha() {
        let mut down = Vec::new();
        let mut percent = MAX_OPACITY;
        while percent > MIN_OPACITY {
            percent = percent_from_alpha(alpha_from_percent(step_opacity(percent, -OPACITY_STEP)));
            down.push(percent);
        }
        assert_eq!(down, [90, 80, 70, 60, 50, 40, 30, 20]);
        let mut up = Vec::new();
        while percent < MAX_OPACITY {
            percent = percent_from_alpha(alpha_from_percent(step_opacity(percent, OPACITY_STEP)));
            up.push(percent);
        }
        assert_eq!(up, [30, 40, 50, 60, 70, 80, 90, 100]);
    }
    #[test]
    fn multiple_pins_are_a_count_not_a_title() {
        assert_eq!(Pin::summary(0, "Notepad"), Pin::Off);
        assert_eq!(Pin::summary(1, "Notepad"), Pin::on("Notepad"));
        assert_eq!(Pin::summary(2, "Notepad"), Pin::Several { count: 2 });
        assert_eq!(Pin::summary(2, "Notepad").title(), None);
        assert!(Pin::summary(3, "Explorer").active());
    }
    #[test]
    fn frame_clamps_hold_their_bounds() {
        assert_eq!(clamp_frame_thickness(0), MIN_FRAME_THICKNESS);
        assert_eq!(clamp_frame_thickness(99), MAX_FRAME_THICKNESS);
        assert_eq!(clamp_frame_opacity(0), MIN_FRAME_OPACITY);
    }
}

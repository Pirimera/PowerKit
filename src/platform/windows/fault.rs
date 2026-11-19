//! User-facing failures raised below the UI. A [`Fault`] travels inside an
//! `io::Error` (often from a worker thread) and is worded in the current language
//! only when shown, by [`describe`] with the caller's string catalog.
use crate::locale::Strings;
use std::{error::Error, fmt, io};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Fault {
    SettingsLocation,
    TrayIcon,
    TrayMenu,
    NoRunningInstance,
    ForwardFailed,
    TargetAppeared,
    BackupNotRecycled,
    ImagingUnavailable,
    SourceChanged,
    NoFolder,
    FrameUnavailable,
    ScreenContext,
    MeasureSurface,
    NoDisplay,
    OpacityUnreadable,
}

impl Fault {
    pub(super) fn text(self, t: &Strings) -> &'static str {
        let f = t.faults;
        match self {
            Self::SettingsLocation => f.settings_location,
            Self::TrayIcon => f.tray_icon,
            Self::TrayMenu => f.tray_menu,
            Self::NoRunningInstance => f.no_running_instance,
            Self::ForwardFailed => f.forward_failed,
            Self::TargetAppeared => f.target_appeared,
            Self::BackupNotRecycled => f.backup_not_recycled,
            Self::ImagingUnavailable => f.imaging_unavailable,
            Self::SourceChanged => f.source_changed,
            Self::NoFolder => f.no_folder,
            Self::FrameUnavailable => f.frame_unavailable,
            Self::ScreenContext => f.screen_context,
            Self::MeasureSurface => f.measure_surface,
            Self::NoDisplay => f.no_display,
            Self::OpacityUnreadable => f.opacity_unreadable,
        }
    }
}

/// English, for logs, `POWERKIT_DIAGNOSTIC_FILE` and anything that skips [`describe`].
impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.text(crate::locale::UiLanguage::English.strings()))
    }
}

impl Error for Fault {}

impl From<Fault> for io::Error {
    fn from(fault: Fault) -> Self {
        io::Error::other(fault)
    }
}

/// The error in the language of `t`; OS and other errors keep their own text.
pub(super) fn describe(error: &io::Error, t: &Strings) -> String {
    match error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<Fault>())
    {
        Some(fault) => fault.text(t).to_string(),
        None => error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locale::UiLanguage;

    #[test]
    fn faults_are_worded_by_the_catalog_and_other_errors_pass_through() {
        let error = io::Error::from(Fault::NoDisplay);
        assert_eq!(
            describe(&error, UiLanguage::Turkish.strings()),
            "Ölçülecek ekran bulunamadı"
        );
        assert_eq!(error.to_string(), "No display was found to measure");
        let plain = io::Error::other("access denied");
        assert_eq!(
            describe(&plain, UiLanguage::Arabic.strings()),
            "access denied"
        );
    }
}

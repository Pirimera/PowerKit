//! MediaRenamer domain: formats, capture times, templates, grouping, collision planning and
//! the rename history. Pure; the Windows layer supplies metadata and file-system answers.
pub mod bmff;
pub mod history;
pub mod plan;
pub mod template;
pub mod time;

use crate::media_extensions;
#[cfg(test)]
use std::path::Path;

/// Files per batch; scanning stops here with a visible notice.
pub const MAX_BATCH: usize = 20_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Jpeg,
    Heif,
    Raw,
    Png,
    WebP,
    Video,
    Avi,
    Sidecar,
}
impl Kind {
    pub fn from_extension(extension: &str) -> Option<Self> {
        let lower = extension.to_ascii_lowercase();
        let is = |list: &[&str]| list.contains(&lower.as_str());
        Some(match lower.as_str() {
            _ if is(media_extensions::JPEG) => Self::Jpeg,
            _ if is(media_extensions::HEIF) => Self::Heif,
            _ if is(media_extensions::RAW) => Self::Raw,
            _ if is(media_extensions::VIDEO) => Self::Video,
            "png" => Self::Png,
            "webp" => Self::WebP,
            "avi" => Self::Avi,
            "aae" | "xmp" => Self::Sidecar,
            _ => return None,
        })
    }
    #[cfg(test)]
    pub fn of(path: &Path) -> Option<Self> {
        Self::from_extension(path.extension()?.to_str()?)
    }
    pub fn is_media(self) -> bool {
        self != Self::Sidecar
    }
    /// The Store extension WIC needs for this format, shown when its codec is missing.
    pub fn codec(self) -> Option<&'static str> {
        match self {
            Self::Heif => Some("HEIF Image Extensions"),
            Self::Raw => Some("Raw Image Extension"),
            Self::WebP => Some("WebP Image Extensions"),
            _ => None,
        }
    }
    /// Primary preference inside a group: still images first, then RAW, then video.
    fn rank(self) -> u8 {
        match self {
            Self::Jpeg | Self::Heif => 0,
            Self::Raw => 1,
            Self::Png | Self::WebP => 2,
            Self::Video | Self::Avi => 3,
            Self::Sidecar => 4,
        }
    }
}
/// Extensions for the file picker filter (`*.jpg;*.jpeg;…`), media only.
pub fn picker_pattern() -> String {
    media_extensions::media()
        .map(|extension| format!("*.{extension}"))
        .collect::<Vec<_>>()
        .join(";")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preferences {
    pub template: String,
    pub recurse: bool,
    pub mtime_fallback: bool,
    pub lowercase_ext: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            template: template::DEFAULT.into(),
            recurse: false,
            mtime_fallback: false,
            lowercase_ext: false,
        }
    }
}
impl Preferences {
    /// A stored template that no longer parses falls back to the default instead of
    /// rejecting the whole settings file.
    pub fn set_template(&mut self, value: &str) {
        self.template = if template::Template::parse(value).is_ok() {
            value.into()
        } else {
            template::DEFAULT.into()
        };
    }
    pub fn append_settings(&self, out: &mut String) {
        use std::fmt::Write;
        let _ = write!(
            out,
            "rename_template={}\nrename_recurse={}\nrename_mtime_fallback={}\nrename_lowercase_ext={}\n",
            self.template, self.recurse, self.mtime_fallback, self.lowercase_ext
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn allow_list_is_closed_and_case_insensitive() {
        assert_eq!(Kind::of(Path::new("a.JPG")), Some(Kind::Jpeg));
        assert_eq!(Kind::of(Path::new("a.Cr3")), Some(Kind::Raw));
        assert_eq!(Kind::of(Path::new("a.MOV")), Some(Kind::Video));
        assert_eq!(Kind::of(Path::new("a.xmp")), Some(Kind::Sidecar));
        assert_eq!(Kind::of(Path::new("a.txt")), None);
        assert_eq!(Kind::of(Path::new("noext")), None);
        assert!(picker_pattern().starts_with("*.jpg;"));
        assert!(!picker_pattern().contains("xmp"));
        for extension in media_extensions::media() {
            assert!(Kind::from_extension(extension).is_some_and(Kind::is_media));
            assert!(media_extensions::is_media(&extension.to_uppercase()));
        }
        assert!(!media_extensions::is_media("xmp"));
    }
    #[test]
    fn invalid_stored_template_falls_back() {
        let mut prefs = Preferences::default();
        prefs.set_template("{yyyy}_{nope}");
        assert_eq!(prefs.template, template::DEFAULT);
        prefs.set_template("{yyyy}{MM}");
        assert_eq!(prefs.template, "{yyyy}{MM}");
        let mut out = String::new();
        prefs.append_settings(&mut out);
        assert!(out.starts_with("rename_template={yyyy}{MM}\nrename_recurse=false\n"));
    }
}

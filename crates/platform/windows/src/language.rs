//! The Windows UI language, mapped to the catalogs PowerKit ships.
use powerkit_core::locale::UiLanguage;
use powerkit_core::settings::AppLanguage;

/// The catalog for a stored preference; `System` follows the Windows UI language.
pub fn resolve(preference: AppLanguage) -> UiLanguage {
    UiLanguage::resolve(preference, system())
}

/// A Windows UI language without a catalog falls back to Turkish.
fn system() -> UiLanguage {
    // Linked directly to avoid enabling the windows-sys Globalization feature.
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetUserDefaultUILanguage() -> u16;
    }
    // SAFETY: returns the current user UI language; no pointers involved.
    let langid = unsafe { GetUserDefaultUILanguage() };
    match langid & 0x3ff {
        0x01 => UiLanguage::Arabic,  // LANG_ARABIC
        0x09 => UiLanguage::English, // LANG_ENGLISH
        _ => UiLanguage::Turkish,    // LANG_TURKISH (0x1f) and everything else
    }
}

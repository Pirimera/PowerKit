//! Platform-neutral value types that cross layer boundaries: keys, instants
//! and colors. Platform crates convert them to and from native forms; nothing
//! above a platform crate handles a virtual-key code, `FILETIME` or `COLORREF`.
pub mod color;
pub mod key;
pub mod time;

pub use color::Rgb;
pub use key::KeyCode;
pub use time::UtcTicks;

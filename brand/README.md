# PowerKit brand

Canonical product artwork. The Windows application and Velopack installer consume
the ICO files under [windows](windows). Other platform exports stay here as source
artwork; they are not shipped until those targets exist.

## Mark

The mark is a **P** built from five cells on a 2 × 3 grid. The upper-right cell is
the only accent: **Deniz** `#1C8FA3` on light surfaces, **Buz** `#8FE0EA` on dark
surfaces.

Keep at least **1u** (one cell) of clear space around the mark. Do not rotate,
stretch, add a shadow or outline, or recolor the cells.

Wordmarks in this folder have the letterforms converted to outlines. Brand
typesetting uses Inter Tight (SIL OFL). The native interface keeps the platform
UI font (Segoe UI Variable on Windows).

## Tokens

| Name | Hex | Role |
| --- | --- | --- |
| Petrol | `#0B4A55` | Primary mark cells, app-icon field |
| Derin petrol | `#062F36` | Darkest brand surface |
| Deniz | `#1C8FA3` | Accent cell on light |
| Buz | `#8FE0EA` | Accent cell on dark |
| Kireç | `#F2F6F6` | Light brand surface |

Machine-readable copies: [tokens/colors.json](tokens/colors.json) and
[tokens/colors.css](tokens/colors.css). Status and text colors live in the JSON
file; do not invent replacements for UI chrome. Win32 controls continue to use
system colors and fonts.

## Layout

| Path | Contents |
| --- | --- |
| [logo](logo) | Mark, horizontal and vertical lockups, wordmark · color / mono-dark / mono-black / reverse / mono-white · SVG and 512 px PNG |
| [icon](icon) | App icon master SVG, 16/20/24/32 px grid SVGs, notify-area SVGs |
| [windows](windows) | `PowerKit.ico`, theme and paused tray ICOs, PNG sizes, archived MSIX and Inno rasters |
| [macos](macos) | `PowerKit.icns` and `AppIcon.iconset` (not packaged) |
| [linux](linux) | hicolor PNG/SVG and `powerkit.desktop` (not packaged) |
| [web](web) | Favicon and app icons |
| [social](social) | README header and GitHub social preview |

Tray file names follow the **taskbar theme**, not pixel color: `tray-light` is the
dark glyph for a light taskbar; `tray-dark` is the light glyph for a dark taskbar.
`*-paused` is AlwaysOn off. The Windows adapter selects these at runtime.

## Repository use

- Window and notify-area icons are embedded from `windows/*.ico`.
- `scripts/Package.ps1` passes `windows/PowerKit.ico` to Velopack as `--icon`.
- [README](../README.md) uses [social/readme-header-1280x240.png](social/readme-header-1280x240.png).
- Set the GitHub repository social preview to
  [social/social-preview-1280x640.png](social/social-preview-1280x640.png) in
  repository settings; that upload is not implied by this file.

Do not replace these files with generated copies from user media. Prefer the SVG
masters when producing a new raster size.

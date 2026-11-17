# Roadmap

This is a scope guide, not a delivery schedule.

1. Publish the MSIX through Microsoft Store as the primary channel, then point
   winget at the Store package. The manifest template and local packaging script
   exist; still open are a physical install/uninstall check of the packaged
   startup task, Explorer commands and PowerStore worker, the Partner Center
   identity and an authorized submission workflow.
2. Finish physical acceptance of AlwaysOn, AlwaysTop, ScreenRuler, ImageResizer
   and the Store/winget install and update path.
3. AlwaysTop follow-ups (see [always-top](always-top.md)): behavior-preserving
   `top.rs` → `top/` split; then settings parity (border/corners toggle, excluded
   apps, Game Mode block), hotkey-label and opacity-target fixes. System menu and
   UIAccess stay separate product decisions. Optional extras (click-through, pin
   list) are backlog only.
4. Expand WinUI 3 to settings, dialogs and on-screen toolbars once Store packaging
   supplies the runtime; keep tray, hotkeys and measurement overlays Win32/Direct2D.
5. ImageResizer follow-ups: multi-frame GIF/TIFF, a preset editor and AI upscaling are not implemented. Unpackaged builds register the classic Explorer verb and the per-user sparse package at runtime, and they may remain after deleting the build; the MSIX declares its commands in the manifest.
6. MediaRenamer physical acceptance: HEIC and Live Photo pairs, camera RAW,
   missing codecs, OneDrive online-only files and Explorer multi-select. The
   workspace itself (in-place rename of JPEG, HEIF, RAW, PNG, WebP, MP4, MOV,
   M4V, 3GP and AVI from capture-time templates, with preview, collisions and
   undoable history) is implemented; see [MediaRenamer design](media-renamer.md).
   Later: caret-position token insertion, folder moves and copies.
7. PowerStore: a package manager front end, WinGet first, then Scoop and
   Chocolatey, then developer managers (see [power-store](power-store.md)).
8. PowerMouse: device-independent remapping of Middle, Back, Forward, wheel
   tilt and F13–F24 with single, long-press and gesture bindings (see
   [PowerMouse design](power-mouse.md)). Domain, hooks/injector and the full
   settings page (including tilt, F13–F24 and custom shortcuts) are in; tray
   submenu and physical acceptance remain. Vendor protocols, per-application
   profiles and Left/Right remapping are out of scope.

Before media writes, resolve missing metadata, timezone interpretation, filename
sanitization and collisions; add preview and recovery. Do not modify source media
during parser development.

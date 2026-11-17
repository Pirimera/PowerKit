# Roadmap

This is a scope guide, not a delivery schedule.

1. Migrate distribution to MSIX with Microsoft Store as the primary channel and
   winget pointing at the Store package. Retire GitHub Releases / Velopack as a
   product install path. Local packaging trials use a self-signed test
   certificate; Store publication re-signs the package.
2. Finish physical acceptance of AlwaysOn, AlwaysTop, ScreenRuler, ImageResizer
   and the Store/winget install and update path.
3. AlwaysTop follow-ups (see [always-top](always-top.md)): behavior-preserving
   `top.rs` → `top/` split; then settings parity (border/corners toggle, excluded
   apps, Game Mode block), hotkey-label and opacity-target fixes. System menu and
   UIAccess stay separate product decisions. Optional extras (click-through, pin
   list) are backlog only.
4. Expand WinUI 3 to settings, dialogs and on-screen toolbars once Store packaging
   supplies the runtime; keep tray, hotkeys and measurement overlays Win32/Direct2D.
5. ImageResizer follow-ups: multi-frame GIF/TIFF, a preset editor and AI upscaling are not implemented. The classic Explorer verb and the per-user sparse package are registered at runtime and may remain after uninstall.
6. MediaRenamer physical acceptance: HEIC and Live Photo pairs, camera RAW,
   missing codecs, OneDrive online-only files and Explorer multi-select. The
   workspace itself (in-place rename of JPEG, HEIF, RAW, PNG, WebP, MP4, MOV,
   M4V, 3GP and AVI from capture-time templates, with preview, collisions and
   undoable history) is implemented; see [MediaRenamer design](media-renamer.md).
   Later: caret-position token insertion, folder moves and copies.
7. PowerStore: a package manager front end, WinGet first, then Scoop and
   Chocolatey, then developer managers (see [power-store](power-store.md)).

Before media writes, resolve missing metadata, timezone interpretation, filename
sanitization and collisions; add preview and recovery. Do not modify source media
during parser development. MouseMapper is excluded from the current scope.

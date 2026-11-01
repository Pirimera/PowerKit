# Roadmap

This is a scope guide, not a delivery schedule.

1. Finish physical acceptance of AlwaysOn and the installer/updater.
2. AlwaysTop: native window pinning with an explicit toggle and visible state.
3. ScreenRuler: DPI-aware on-screen measurement.
4. ImageResizer: batch preview, output policy and image quality choices.
5. MediaRenamer: JPG, HEIC, MP4 and MOV metadata templates. Default filename:
   YYYY-MM-DD_HH-mm-ss, with user-selectable metadata fields.

Before media writes, resolve missing metadata, timezone interpretation, filename
sanitization and collisions; add preview and recovery. Do not modify source media
during parser development. MouseMapper is excluded from the current scope.

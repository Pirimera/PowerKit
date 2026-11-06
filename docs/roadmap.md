# Roadmap

This is a scope guide, not a delivery schedule.

1. Finish physical acceptance of AlwaysOn, AlwaysTop, ScreenRuler, ImageResizer and the installer/updater.
2. ImageResizer follow-ups: Windows 11 compact menu, multi-frame GIF/TIFF, a preset editor and AI upscaling are not implemented. The classic Explorer verb is registered at runtime and may remain after uninstall.
3. MediaRenamer: JPG, HEIC, MP4 and MOV metadata templates. Default filename:
   YYYY-MM-DD_HH-mm-ss, with user-selectable metadata fields.

Before media writes, resolve missing metadata, timezone interpretation, filename
sanitization and collisions; add preview and recovery. Do not modify source media
during parser development. MouseMapper is excluded from the current scope.

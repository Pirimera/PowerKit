# Changelog

Notable user-visible changes are recorded here. Release dates are added only after
publication; a source version is not proof of a published release.

## Unreleased

## 0.1.0

First version of PowerKit: seven native Windows 11 tools in one tray application,
for ARM64 and x64, in Turkish, English and Arabic.

- AlwaysOn keeps Windows awake, and optionally the display: indefinitely, for 30, 60 or 120 minutes, for a custom interval, until a chosen date and time, or while a chosen app runs. It has a toggle shortcut, a notice before a timed session ends with a 30-minute extension, power conditions (only when plugged in, stop below a battery level) and `--always-on` command-line control. Every launch starts with AlwaysOn off.
- AlwaysTop pins any number of windows above the others with a colored border, from a shortcut, the tray or its page, and changes a pinned window's transparency in 10% steps. Border color, opacity and thickness, the shortcuts and the pin sound are yours to set. Pins are not saved.
- ScreenRuler measures on-screen pixels from a movable toolbar: a bounds rectangle, or spacing found by edge detection in both directions, horizontally or vertically. A click copies the measurement; extra units, tolerance, continuous capture and the line color are settings.
- ImageResizer resizes one or many images from the Explorer menu, the tray or its page, with four preset sizes and a custom one, fit, fill or stretch, and a preview before anything is written. A copy never overwrites the original; replacing keeps a backup.
- MediaRenamer renames photos and videos in place from their capture time, using a template you can edit. It shows every new name before renaming, keeps Live Photo pairs and sidecar files together, never overwrites a file, and can undo a batch from History, also after a restart.
- PowerStore lists the software installed through WinGet, Scoop, Chocolatey, npm, pip, Cargo, .NET tools, PowerShell and vcpkg in one table, shows available updates, and installs, updates and removes packages one at a time with progress and cancel. It checks for updates in the background and never installs them on its own.
- PowerMouse remaps the Middle, Back and Forward buttons, horizontal scroll and F13–F24 of any mouse to actions, held shortcuts, long presses, hold-and-swipe gestures, typed text or workflows, and adds an Actions Ring of eight slots around the cursor. For an MX Master 4 connected over Bluetooth it also uses the gesture button, mode shift button, haptic panel and thumb wheel, shows the battery, and sets sensor resolution and wheel behavior.
- The settings window has a Home page with a switch for every tool, a page per tool and a Settings page for language, theme, transparency, starting with Windows and the tray menu. The tray menu lists the tools you choose, in your order, with their shortcuts.
- PowerKit is packaged as MSIX for the Microsoft Store, which also delivers updates. Settings are kept in `%LOCALAPPDATA%\PowerKit` and remain after an uninstall.

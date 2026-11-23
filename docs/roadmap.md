# Roadmap

This is a scope guide, not a delivery schedule. It lists what is open; what
each tool does today is in the [README](../README.md) and the design
documents.

## Publication

1. Publish the MSIX through Microsoft Store as the primary channel, then point
   winget at the Store package. The manifest template and the local packaging
   script exist. Still open: the Partner Center submission workflow, the Store
   policy review for PowerStore, and a physical install, update and uninstall
   check of the packaged startup task, Explorer commands and PowerStore
   worker.
2. Finish the physical acceptance in [testing](testing.md) for every tool on
   native ARM64 and native x64. x64 has only been run under emulation.

## Platforms

Windows 11 on ARM64 and x64 is the only supported platform today. macOS and
Linux are planned; nothing for them is implemented, and no build for them
exists.

The workspace is layered for that step (see
[architecture](architecture.md#layers)): the domain logic and the controllers
are platform-neutral, and an operating system or a UI kit is added by
implementing the port traits and the UI contract rather than by rewriting the
application. Before either platform can be claimed:

1. Run the tests of `powerkit-core` and `powerkit-app` on Linux and macOS.
   They type-check there today and have never run off Windows.
2. Choose the second UI kit. WinUI 3 through windows-reactor is Windows only;
   GPUI is the candidate.
3. Decide per tool what the platform offers. Keep-awake, pinned windows,
   screen capture, global shortcuts, input remapping and package managers
   differ on each system, so a platform reports the tools it supports and the
   UI hides the rest. PowerStore's managers (Homebrew, apt, Flatpak and so on)
   and PowerMouse's input layer need their own design first.
4. Choose the distribution channel and package format per platform.

## Tools

- **AlwaysTop** (see [design](always-top.md)): settings parity with PowerToys
  (border and rounded-corner toggles, excluded apps, Game Mode block). The
  title-bar system menu item and UIAccess for elevated windows stay separate
  product decisions. Click-through and a pin list are backlog only.
- **ImageResizer:** multi-frame GIF and TIFF, a preset editor and AI upscaling
  are not implemented. Unpackaged builds register the classic Explorer verb
  and a per-user sparse package at runtime, which may remain after the build
  is deleted; the MSIX declares its commands in the manifest instead.
- **MediaRenamer** (see [design](media-renamer.md)): physical acceptance with
  HEIC and Live Photo pairs, camera RAW, missing codecs, OneDrive online-only
  files and Explorer multi-select. Later: caret-position token insertion,
  folder moves and copies.
- **PowerStore** (see [design](power-store.md)): interactive checks (`msstore`
  install, installers that raise UAC, cancel during an external installer).
  No automatic updates or cloud sync.
- **PowerMouse** (see [design](power-mouse.md)): Bolt and Unifying receivers,
  further verified models, and the checks listed as not verified there. Mice
  only; per-application profiles and Left/Right remapping are out of scope.

No third UI stack is added on Windows: the settings shell and dialogs are
WinUI 3; the tray menu, hotkeys, measurement overlays and the AlwaysTop frame
stay Win32/Direct2D.

Any further tool that edits the user's files follows the MediaRenamer rules:
preview first, collision handling and recovery, and synthetic fixtures rather
than the user's own media during development.

# AlwaysTop

[User guide](README.md) · [Türkçe](tr/always-top.md)

AlwaysTop keeps a window above all the others, so a video, a note or a
terminal stays visible while you work in another application. Any number of
windows can be pinned at once.

## Pin and unpin

| From | How |
| --- | --- |
| Keyboard | Click the window, then press Win+Ctrl+T. The same shortcut unpins it |
| Tray menu | **Always Top** > **Keep window on top** pins the window you used last |
| Always Top page | **Pin or unpin** does the same |

A pinned window gets a colored border. The tray menu and the page show the
title of the pinned window, or how many windows are pinned.

Pinning one window does not unpin the others. Closing a window, or exiting
PowerKit, removes its pin.

## Make a pinned window see-through

Win+Ctrl+- lowers the opacity of the pinned window in front, and Win+Ctrl++
raises it, in steps of 10% between 20% and 100%. The tray menu has the same
commands. Unpinning the window restores its normal opacity.

## Settings

| Setting | What it does |
| --- | --- |
| Activation shortcut | Pins or unpins the window in front. Default Win+Ctrl+T |
| Increase opacity, Decrease opacity | Defaults Win+Ctrl++ and Win+Ctrl+- |
| Use system accent color | The border takes the Windows accent color. Turn it off to choose a **Border color** |
| Border opacity | 10% to 100%. Default 100% |
| Border thickness | 1 to 32 pixels. Default 4 |
| Play sound | Plays a Windows sound when a window is pinned or unpinned and when its opacity changes |

## Good to know

- Pins and window opacity are not saved. After PowerKit restarts no window is
  pinned. The shortcuts and the border settings are saved.
- PowerKit's own windows and the taskbar cannot be pinned.
- A window of an application that runs as administrator cannot be pinned.
- The border is hidden while its window is minimized.

# PowerKit user guide

[Türkçe](tr/README.md)

PowerKit is a set of small Windows 11 tools that live in one tray application.
This guide explains how to use them. The interface is available in Turkish,
English and Arabic; this guide uses the English names.

| Tool | What it does |
| --- | --- |
| [AlwaysOn](always-on.md) | Keeps Windows awake, and optionally the display |
| [AlwaysTop](always-top.md) | Pins windows above the others |
| [ScreenRuler](screen-ruler.md) | Measures pixels on the screen |
| [ImageResizer](image-resizer.md) | Resizes one or many images |
| [MediaRenamer](media-renamer.md) | Renames photos and videos by when they were taken |
| [PowerStore](power-store.md) | Installs, updates and removes software through your package managers |
| [PowerMouse](power-mouse.md) | Remaps mouse buttons and adds an Actions Ring |

## Requirements and installation

- Windows 11, on an ARM64 or x64 computer.
- Windows App Runtime 2.4 or later. The Microsoft Store installs it together
  with PowerKit.

PowerKit is distributed through the Microsoft Store, and through winget, which
installs the same Store package. The Store listing is not published yet. Until
it is, PowerKit can only be built from source; see the
[development guide](../development.md). GitHub Releases is not a download
channel.

Updates come from the Store. PowerKit has no updater of its own.

## The window and the tray icon

PowerKit runs in the background and shows an icon in the notification area
(the system tray). The settings window has a navigation rail on the left:
Home first, then one page per tool, and Settings at the bottom.

- The window's X button hides the window. PowerKit keeps running.
- To quit, choose **Exit PowerKit** in the tray menu or on the Settings page.
- When PowerKit starts with Windows, it stays in the tray and does not open
  the window.
- Starting PowerKit again from the Start menu opens the window of the copy
  that is already running.

## Home

Home lists every tool with a switch. Turning a tool off removes it from the
navigation rail and the tray menu and stops it everywhere in Windows. All tools
are on at first except PowerMouse, which is off until you turn it on.

Home also shows the version, the architecture, whether PowerKit runs from an
MSIX package, where the settings file is, and the release notes.

## Tray menu

Click the tray icon to open the menu. Each tool has one entry:

| Entry | Contents |
| --- | --- |
| Power Store | The number of available updates, **Update all**, **Check for updates**, **Open PowerStore** |
| Always On | The modes described in [AlwaysOn](always-on.md), with a mark on the active one |
| Always Top | **Keep window on top**, the opacity commands and **Play sound** |
| Screen Ruler | Starts or stops measuring |
| Media Renamer | The state of the current preview and **Open MediaRenamer** |
| Image Resizer | **Resize images** |
| Power Mouse | A check mark that turns PowerMouse on or off |
| Shortcuts | The current shortcut of each tool |

Media Renamer and Image Resizer are not listed at first, because they are
mostly started from File Explorer. Choose which entries the menu shows, and in
what order, under **Tray menu** on the Settings page. The menu ends with
**Open PowerKit** and **Exit PowerKit**.

## Settings

The Settings page holds what applies to PowerKit as a whole. Each tool's own
settings are on that tool's page.

| Setting | What it does |
| --- | --- |
| Run at startup | Starts PowerKit when you sign in to Windows. Off by default |
| Language | Windows default, Türkçe, English or العربية |
| Theme | Windows default, Light or Dark |
| Acrylic opacity | How strongly the window's background covers the desktop behind it, from 50% to 100%. The window never becomes fully transparent |
| Show system tray icon | Hides the tray icon when off. To open the window then, start PowerKit again from the Start menu |
| Tray menu | Which tools the tray menu lists, and their order |
| Restore default settings | Resets the settings on this page only. The tools' settings and histories are kept |

## Shortcuts

| Shortcut | Action |
| --- | --- |
| Win+Ctrl+Shift+A | Turn AlwaysOn on or off |
| Win+Ctrl+T | Pin or unpin the window in front |
| Win+Ctrl++ and Win+Ctrl+- | Raise or lower the opacity of a pinned window |
| Win+Ctrl+Shift+M | Start or stop ScreenRuler |

- These are the defaults. Change one on its tool's page: click the shortcut,
  then press the new keys.
- When another application already uses a default shortcut, PowerKit tries the
  same keys with Alt in place of Ctrl, for example Win+Alt+T.
- A shortcut that could not be registered is marked "Inactive" on the page,
  with the reason. Choose different keys for it.

## Command line

`powerkit.exe` accepts these arguments. If PowerKit is already running, the
running copy carries out the request.

| Argument | Effect |
| --- | --- |
| `--background` | Start in the tray without opening the window |
| `--always-on on` | Keep awake indefinitely |
| `--always-on 90` | Keep awake for 90 minutes |
| `--always-on off` | Turn AlwaysOn off |
| `--always-on-pid 1234` | Keep awake while the process with that id runs |
| `--resize <files>` | Open the ImageResizer window with those images |
| `--rename <files or folders>` | Open them as a preview in MediaRenamer |

## Where PowerKit keeps its data

Everything is in `%LOCALAPPDATA%\PowerKit`. The folder remains after an
uninstall; delete it by hand for a clean start.

| File or folder | Contents |
| --- | --- |
| `settings.txt` | All settings |
| `media-renamer-history.txt` | MediaRenamer's rename history, used by Undo |
| `power-store-history.txt` | PowerStore's operation history |
| `power-store-installed.txt` | The last package list, shown first on the next start |
| `icons`, `icon-database.json` | Downloaded package logos |
| `mouse-assets` | Downloaded pictures of supported mice |

PowerKit does not save running sessions: after a restart AlwaysOn is off and no
window is pinned.

These features use the network: PowerStore, through your package managers and
for package logos (the logo download can be turned off in PowerStore's
settings), and PowerMouse, which downloads a picture of a recognized Logitech
mouse.

## Troubleshooting

| Problem | What to do |
| --- | --- |
| "Settings need Windows App Runtime 2.4 or later" | Install or update Windows App Runtime, then restart PowerKit |
| A shortcut does nothing | Open the tool's page. An "Inactive" note under the shortcut means another application uses those keys; record different ones |
| "Settings could not be read; PowerKit started with defaults" | The settings file is damaged. PowerKit keeps the file as it is until you change a setting, so you can copy it first |
| The tray icon is missing | It may be hidden. Start PowerKit again from the Start menu and turn on **Show system tray icon** in Settings. Windows may also keep the icon in the overflow area of the taskbar |
| A tool is missing from the navigation rail | It is turned off. Turn it on from Home |
| AlwaysTop or PowerMouse does nothing in an application that runs as administrator | Windows does not let a normal application control an elevated one, and PowerKit never runs as administrator. Such a window cannot be pinned, and PowerMouse leaves the mouse buttons as they are while it is in front |

## Help and feedback

Report bugs and ask questions through
[GitHub Issues](https://github.com/Pirimera/PowerKit/issues); see
[SUPPORT](../../SUPPORT.md) for what to include. Report security problems as
described in [SECURITY](../../SECURITY.md).

# PowerMouse design

Status: slices 1–4 are implemented (domain, settings, mouse/keyboard hooks,
injector, elevation pass-through, and the settings page covering Middle/Back/
Forward, wheel tilt, F13–F24 rows, custom shortcut capture, and reset). Tray
submenu and physical acceptance remain. This document records the product
decisions and the planned implementation for a device-independent mouse button
remapper.

PowerMouse is a [utility](ui-structure.md): bindings are edited on the tool
page and take effect system-wide while the tool is enabled. Remapping happens
in PowerKit's own user session; there is no service, driver or vendor
software.

## Reference: MX4Lite and OpenLogi

MX4Lite (a personal fork of OpenLogi, MIT OR Apache-2.0) was reviewed as
reference material. It is not a dependency and is not ported:

- MX4Lite diverts two Logitech MX Master 4 controls (gesture button, haptic
  panel) over HID++ `0x1b04` and maps them to six fixed shortcuts. That path
  is Logitech-specific and out of scope for PowerMouse.
- MX4Lite's LocalSystem service plus per-session agent exists to restart a
  hidden agent across user switches. PowerKit already runs in the user
  session, so the service model is not needed.
- OpenLogi's crates bring tokio, async-hid, tarpc and GPUI. PowerKit stays one
  crate, so the useful parts are reimplemented in narrow modules.

Patterns PowerMouse adopts:

| Source | Pattern |
| --- | --- |
| `openlogi-hook` Windows backend | `WH_MOUSE_LL` / `WH_KEYBOARD_LL` on a dedicated thread with its own message pump; ignore `LLMHF_INJECTED`; copy the hook struct and never keep the pointer |
| `openlogi-agent-core` hook runtime | Press admission (first live hold wins), suppressed-press set so a release follows its press disposition, stale-hold recovery after a lost release |
| `openlogi-core` binding model | Single, long-press (500 ms) and hold-and-swipe gesture bindings; swipe thresholds (50 px travel, 40 px deadzone, 160 ms hold before a swipe can commit) |
| MX4Lite `platform.rs` | One `SendInput` batch per action, precise release of keys accepted by a partial batch, refuse when a physical modifier is held or the input desktop is not `Default`, tag injected input in `dwExtraInfo` |
| MX4Lite `engine.rs` | Duplicate press suppression; only explicitly repeatable actions (volume) may repeat or reverse within one hold |

If any OpenLogi source is copied rather than reimplemented, its MIT notice
goes into the [dependency licensing](dependencies.md) notices.

## Scope

In the first release:

- Remap Middle, Back (X1), Forward (X2) and main-wheel tilt left/right on any
  mouse Windows exposes through the standard mouse stack.
- Remap F13–F24, which many mice and macro pads send for extra buttons.
- Per trigger, choose pass-through (default), a single action, a short/long
  press pair, or a hold-and-swipe gesture (click, up, down, left, right).
- Actions from a fixed Windows catalog, a recorded custom keyboard shortcut,
  or a PowerKit command.
- One global on/off switch from the page, the tray and the General card.

Not planned for the first release: Left and Right button remapping, wheel
rotation remapping, per-application profiles, per-device bindings, pointer
speed or DPI changes, macros, typed text, launching programs or commands,
vendor protocols (Logitech HID++, Razer, others) and a tray-free background
mode.

## Triggers

| Trigger | Windows input | Press/release | Gesture |
| --- | --- | --- | --- |
| Middle | `WM_MBUTTONDOWN` / `UP` | Yes | Yes |
| Back | `WM_XBUTTONDOWN` / `UP`, `XBUTTON1` | Yes | Yes |
| Forward | `WM_XBUTTONDOWN` / `UP`, `XBUTTON2` | Yes | Yes |
| Tilt left / right | `WM_MOUSEHWHEEL`, negative / positive | No: a burst of wheel events while tilted | No |
| F13–F24 | `WM_KEYDOWN` / `UP`, `VK_F13`–`VK_F24` | Yes (key repeat ignored) | No |

- Left and Right are never suppressed, so a bad binding cannot make the
  pointer unusable.
- Tilt has no release edge. It supports a single action only, fired once per
  burst; a burst ends after 250 ms without events in the same direction.
- F13–F24 are matched by virtual key regardless of which device sent them;
  the page says so. The keyboard hook is installed only while at least one
  F-key is bound.
- Unbound triggers and every other event pass through untouched.

## Bindings

| Binding | Behavior |
| --- | --- |
| Pass-through | Default. The trigger is not suppressed |
| Single | The action fires on press. A mouse-button action is mirrored: its down is sent on press and its up on release, so dragging still works |
| Long press | The short action fires on a release before 500 ms; the long action fires once when 500 ms elapses and the release is then swallowed |
| Gesture | Hold and move: the first committed direction fires its action once per hold; a release without a swipe fires the click action. Volume up/down may repeat and reverse within one hold |

- A trigger with any binding other than pass-through has its press and its
  matching release suppressed. A release whose press was not suppressed
  (PowerMouse enabled mid-press) passes through.
- The "Do nothing" action disables a trigger.
- With long press and gesture, mouse-button actions become a click
  (down and up together), not a mirrored hold.
- Only one hold is live at a time. A second trigger pressed during a hold is
  handled as its own single press unless it is a gesture trigger, which is
  refused until the first hold ends. A re-press of the held trigger, or any
  press after 10 s, ends a hold whose release was lost.

### Gesture movement

- Movement deltas come from successive `MSLLHOOKSTRUCT.pt` values; the
  thresholds are scaled from 96 DPI to the DPI of the monitor under the
  cursor at press time.
- Proposed: the cursor stays still while a gesture trigger is held
  (`WM_MOUSEMOVE` is suppressed during the hold and deltas are measured
  against the frozen position), matching how a hardware gesture button
  behaves. A gesture binding therefore replaces middle-drag panning for that
  button; the page says so.

## Actions

Each action maps to one Windows input sequence. Labels live in
`src/locale.rs`; storage tokens are English and stable.

| Group | Actions |
| --- | --- |
| Mouse | Left, Right, Middle, Back, Forward click |
| Navigation | Browser back / forward (`VK_BROWSER_BACK` / `FORWARD`), Task View (Win+Tab), previous / next virtual desktop (Win+Ctrl+Left / Right), Show desktop (Win+D) |
| Editing | Copy, Paste, Cut, Undo, Redo (Ctrl+Y), Select all, Find, Save |
| Tabs | New tab, Close tab, Reopen tab, Next tab, Previous tab, Reload |
| Media | Play/pause, Next track, Previous track, Volume up, Volume down, Mute |
| System | Capture region (Win+Shift+S), Lock (`LockWorkStation`, because injected Win+L is ignored by Windows), Do nothing |
| PowerKit | Pin or unpin the last window with AlwaysTop, Start ScreenRuler, Open PowerKit |
| Custom | A recorded shortcut: modifiers plus one virtual key |

- Custom shortcuts are recorded with the existing settings-shell key capture
  (`winui_host::begin_key_capture`, `CAPTURED_KEY`) and stored in the same
  `mods,vk` form as tool hotkeys.
- PowerKit actions are posted to the hidden app window as the existing
  `AppCommand` values and run on the UI thread, exactly as tray commands do.
- An action is skipped, never partially replayed, when a physical modifier is
  already held, the input desktop is not `Default` (lock screen, UAC), or
  `SendInput` accepts only part of the batch; keys accepted by a partial batch
  are released.

## Settings page and tray

Follows the utility skeleton in [Tool UI structure](ui-structure.md):

1. **Status card:** "PowerMouse" toggle (label on the left), a one-line state
   ("Kapalı", "Açık", "Yönetici penceresinde devre dışı") and the
   elevated-window limitation.
2. **Mouse buttons section:** one card row per trigger (Middle, Back, Forward,
   Tilt left, Tilt right). The row shows the binding summary; expanding it
   shows a binding-type choice (pass-through, single, long press, gesture)
   and one action picker per slot. The action picker lists the catalog
   groups and ends with "Kısayol kaydet…", which opens the shared hotkey
   capture.
3. **Extra keys section:** F13–F24 rows only for keys the user added.
   "Ekstra tuş ekle…" waits for the next F13–F24 press and adds its row.
4. A "Varsayılana dön" command that sets every trigger back to pass-through.

Changes apply immediately: the UI thread publishes a new binding snapshot to
the hook thread and writes settings. PowerMouse joins the rail after
ImageResizer. The tray gets a PowerMouse submenu with an "Etkin" check and a
disabled status row; General shows one card with status, Open and an
enable/disable quick action. Strings ship in Turkish, English and Arabic.

## Threading and lifecycle

| Thread | Owns |
| --- | --- |
| UI thread | `Session`: enabled flag, binding snapshot, settings writes, tray and page state, hook thread start/stop, PowerKit actions |
| Hook thread | `SetWindowsHookExW(WH_MOUSE_LL)` and, when needed, `WH_KEYBOARD_LL`; the press state machine; long-press and tilt timers (`SetTimer` thread timers on its own message loop) |
| Injector thread | `SendInput` batches, `LockWorkStation`, mirrored-button bookkeeping |

- The hook callback copies the event, runs the pure state machine and
  returns a pass/suppress decision; it never blocks, allocates on the hot
  path, logs, takes a contended lock or calls `SendInput`. Windows removes a
  low-level hook that exceeds `LowLevelHooksTimeout` without notifying the
  owner, which is why the hook cannot live on the WinUI thread.
- Outputs go to the injector over a channel. Binding snapshots are replaced
  by a thread message to the hook thread, so the callback reads only
  thread-local state.
- The hook ignores events with `LLMHF_INJECTED` / `LLKHF_INJECTED`, so its own
  output (and other injected input) is never remapped.
- Elevated foreground windows: input to a higher-integrity window is not
  remapped, and `SendInput` into it is blocked by UIPI (PowerToys Keyboard
  Manager has the same limit). A foreground WinEvent hook on the UI thread
  publishes an atomic "foreground is elevated" flag; while it is set, every
  trigger passes through so a press is never swallowed without an action.
- Stop paths: disable, binding reset, session lock, suspend, session end and
  PowerKit exit all stop the hook thread (`WM_QUIT`, join), cancel pending
  holds and timers, and have the injector release every mirrored button it
  still holds down. Exit runs this from `WM_DESTROY` before the WinUI window
  closes. Unlock and resume restart the hook only when PowerMouse is enabled.
- PowerMouse never runs elevated, never installs a driver and does not stop
  or restart an AlwaysOn session.

## Persistence

Settings through the existing settings writer (`settings.txt`):

| Key | Meaning |
| --- | --- |
| `mouse_enabled` | `true` / `false`, default `false` |
| `mouse_<trigger>` | Binding for `middle`, `back`, `forward`, `tilt_left`, `tilt_right`, `f13`…`f24`; absent means pass-through |

Binding grammar (each value at most 256 bytes):

```text
binding  = "do:" action
         | "long:" action "|" action
         | "gesture:" action "|" action "|" action "|" action "|" action   ; click|up|down|left|right
action   = token                   ; for example copy, task_view, volume_up, none
         | "keys:" mods "," vk     ; same form as tool hotkeys
```

- A gesture slot may be `none`; tilt and F-key triggers accept only `do:`.
- An unknown trigger, action token or malformed binding makes the file
  invalid under the existing policy: it is preserved until an explicit user
  change replaces it. Older PowerKit builds also treat the new keys as
  invalid, as with every other added setting.
- Not persisted: holds in progress, mirrored buttons, the elevated-foreground
  flag.

## Module layout

- `src/power_mouse.rs`: pure triggers, actions, bindings and their grammar,
  the press state machine (press, release, move, timer, cancel → suppress
  decision plus outputs), swipe accumulator and tilt burst detection.
  Unit-tested without Windows.
- `src/platform/windows/power_mouse/`: `mod.rs` (`Session` on the UI thread),
  `hook.rs` (hook thread and message loop), `inject.rs` (injector thread and
  `INPUT` batches), `elevation.rs` (foreground integrity flag).
- `src/platform/windows/window/pages.rs` or a new `mouse_page.rs` for the page.
- Strings in `src/locale.rs` (or `src/locale/power_mouse.rs` if the catalog
  grows like MediaRenamer's).
- `windows-sys` gains the `Win32_System_Shutdown`,
  `Win32_System_StationsAndDesktops` and `Win32_System_RemoteDesktop` features;
  no new crates.

## Testing

- Unit tests with synthetic event sequences: single, mirrored, long-press
  before and after the threshold, gesture click and each direction, volume
  repeat and reversal, lost release, stale hold, second trigger during a hold,
  release without a suppressed press, tilt bursts, F-key repeat, cancel while
  mirrored buttons are down, DPI threshold scaling, binding grammar round trips
  and rejection of malformed values.
- Windows integration: partial-batch cleanup on `INPUT` construction, and a
  smoke check that starts and stops the hook thread with isolated preferences.
  Real hook behavior cannot be driven by `SendInput`, because injected events
  are ignored by design.
- Physical (record native ARM64, emulated x64 and physical evidence
  separately): a five-button USB mouse, a Bluetooth mouse, a mouse with wheel
  tilt, a device that sends F13–F24, precision-touchpad middle click and
  horizontal scrolling, an elevated window in the foreground, lock/unlock,
  sleep/resume, fast user switching and exit while a mirrored button is held.

## Implementation slices

1. **Domain and settings:** `power_mouse.rs`, grammar, state machine, settings
   keys and tests. Done.
2. **Hook and injector:** hook thread, injector, lifecycle stop paths,
   elevated-foreground pass-through, single bindings for Middle, Back and
   Forward; minimal page with the enable toggle and single-action pickers;
   rail entry and General card. Done.
3. **Long press and gestures** on the page and in the runtime (binding-type
   choice, per-slot pickers, reset to defaults). Done.
4. **Tilt, F13–F24 and custom shortcuts** (tilt rows, add/remove F-keys,
   keyboard hook while F-keys are bound, “Record shortcut…” capture). Done.
5. **Tray submenu** and physical acceptance; finish architecture/README polish.

## Resolved product decisions (2026-11-17)

- Name: PowerMouse.
- Device-independent: standard Windows mouse input only; vendor protocols are
  out of scope.
- First release: single actions, long press, hold-and-swipe gestures, custom
  keyboard shortcuts and F13–F24 triggers.
- Per-application profiles are not in the first release.

## Open questions

- Whether precision-touchpad horizontal scrolling and three-finger middle
  click reach `WH_MOUSE_LL` as ordinary `WM_MOUSEHWHEEL` / `WM_MBUTTON*`. If
  they do, tilt remapping (and middle remapping) would also affect the
  touchpad; tilt may need to ship behind a warning or be dropped.
- Whether freezing the cursor during a gesture hold is preferred over letting
  it move, as OpenLogi's OS-hook gestures do.
- Whether full-screen games or anti-cheat software react to a global
  low-level hook; a "pause in full-screen apps" option may be needed later.

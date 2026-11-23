# AlwaysTop design

Contributor notes for the AlwaysTop tool: PowerToys Always On Top parity, the
`top/` module map, open checks and the optional product backlog. Runtime
behavior is defined in [architecture](architecture.md).

## Contract

AlwaysTop pins eligible top-level windows with `SetProp` plus
`SetWindowPos(HWND_TOPMOST, …)`, keeps any number of pins until each window is
toggled, closed or PowerKit exits, draws a PowerToys-style D2D FrameDrawer ring
behind each pin, steps layered opacity between 20% and 100% in exact 10%
increments, and plays Windows Media cues when sound is enabled. Pins and window
opacity are not persisted. Defaults: Win+Ctrl+T (pin), Win+Ctrl++ / Win+Ctrl+-
(opacity), with Win+Alt fallbacks when those defaults are taken.

| Surface | Role |
| --- | --- |
| `crates/core/src/always_top.rs` | Pure pin session (none, one or a count), opacity/frame clamps |
| `crates/core/src/hotkey.rs` | Shared keyboard chords, defaults and Win+Alt fallback for all tools |
| `crates/app/src/controller/always_top.rs` | Which window a command acts on, pin or unpin, opacity steps, the tray's opacity target and the cues |
| `crates/platform/windows/src/top/` | UI-thread pin ownership, hooks, border host, opacity and smoke (see the module map below) |
| `crates/platform/windows/src/frame.rs` | D2D hollow ring (FrameDrawer port) |

The pin shortcut acts on the current foreground root window. The tray and the
settings page act on the last remembered eligible window, so the user can open
the menu without losing the intended target; the tray opacity items follow that
same window.

Rules the implementation keeps:

- Every window of PowerKit's own process is ineligible, including the unowned
  WinUI settings window.
- A pin is looked up by its exact HWND without a liveness check, so a destroyed
  target loses its pin and ring at once, and a destroyed child window does not
  unpin its root.
- The ring is shown only while its window is visible, not minimized and not
  cloaked. Minimizing hides the ring window and restoring shows the same one.
- Unpinning drops the pin first, then restores the window; releasing all pins
  tries every window and reports the first error.
- The `PowerKit.AlwaysTop.Pinned` property is removed on every unpin and on a
  failed pin, also for a window that was already topmost before pinning.
- The ring repaints only when size, color, opacity, thickness or radius change.
  The accent color is read once and refreshed on `WM_SETTINGCHANGE` and
  `WM_DWMCOLORIZATIONCOLORCHANGED`.
- D2D never runs while the pin state is borrowed: the ring paints with its
  drawer moved out of the pin.
- `top::shutdown` releases the D2D factory after every pin is gone. Leaving
  that to the thread-local destructor at process exit raises an exception in
  KERNELBASE.

## PowerToys Always On Top parity

| Feature | PowerToys | PowerKit | Notes |
| --- | --- | --- | --- |
| Pin / unpin focused window | Yes | Yes | Tray path uses last eligible, not live foreground |
| Multiple simultaneous pins | Yes | Yes | |
| Activation + opacity hotkeys | Yes | Yes | Same default chords and opacity bounds |
| Border color / opacity / thickness | Yes | Yes | Accent or custom RGB |
| Accent vs custom border color | Yes | Yes | |
| Pin / opacity sound | Yes | Yes | Uses global `sound` preference |
| Pinned-window opacity steps | Yes | Yes | Layered style restored on unpin |
| Show border toggle | Yes | No | Border always drawn |
| Rounded corners toggle | Yes | No | Corner radius follows DWM automatically |
| Excluded apps | Yes | No | Not implemented |
| Do not activate in Game Mode | Yes | No | Not implemented |
| Title-bar system menu item | Yes | No | Separate product decision |
| Elevated windows / UIAccess | Partial | No | Separate product decision; Store/package cost is high |

Settings parity, when it is implemented, comes as one change: persist
`enable_frame`, `round_corners`, `excluded_apps` and `block_in_game_mode`, add
the WinUI AlwaysTop cards with Turkish, English and Arabic strings, and match
PowerToys' partial matching for excluded apps. Evaluate UIAccess only with the
Store package identity and an elevation policy.

## Open checks

- Reapplying `HWND_TOPMOST` after a focus change calls `SetWindowPos` without
  `SWP_NOACTIVATE`, synchronously against another process. PowerKit is a
  background process when the hook fires, so the foreground lock should stop
  activation, but that is unverified. Check by hand (pin a window, cover it
  with another topmost window, switch apps) whether it steals focus or blocks
  on a hung window, and change the reapply path only if it does.
- A failing `SetWindowPos` during unpin is not simulated by any test.
- The physical checklist is in [testing](testing.md#alwaystop).

## Module map

`PinGuard`, the `thread_local` `PinState`, HWND restore and border destroy stay
on the UI thread. Submodules reach shared state through `use super::*`, and
only `step_foreground_opacity`, `step_pinned_opacity` and `smoke` are
re-exported beside the `mod.rs` commands, so callers use `top::…`.

| Module | Responsibility |
| --- | --- |
| `mod.rs` | `PinState`, `Tracked`, `PinGuard`, `apply_prefs`, the `Desktop` port for the `powerkit_app` controller, public commands that delegate to it (`toggle`, `reconcile`, `release`, `shutdown`, `listen`), topmost/prop marking and sound cues |
| `eligibility.rs` | Eligible roots, `resolve_target`, shell/skip list, titles |
| `border.rs` | Frame paint, border class/HWND lifecycle, timer, `glue_border_for` / z-order sync |
| `opacity.rs` | Layered apply/restore and step helpers |
| `hook.rs` | WinEvent install/uninstall and event branches |
| `smoke.rs` | HWND smoke paths, kept out of production helpers |

The pin and opacity shortcuts are not registered here. The shared registry in
`crates/app/src/controller/hotkeys.rs` decides the chords, the Win+Alt fallback
for a slot's own default and the labels;
`crates/platform/windows/src/hotkeys.rs` makes the `RegisterHotKey` calls.

## Optional product backlog (beyond PowerToys)

Priority order for later product decisions:

1. **Click-through**: let clicks pass through a translucent pinned window
   (PowerToys does not offer this; high value on a single display).
2. **Pin list / bulk unpin**: tray or settings surface listing open pins.
3. **Excluded-app templates**: starter lists for games and launchers once
   excluded apps exist.
4. **Per-monitor topmost**: optional chord that only raises on the current
   monitor to reduce multi-monitor z-order noise.
5. **AlwaysTop-specific sound**: `pin_sound` separate from the global `sound`
   preference.
6. **Brief border emphasis**: short visual pulse on pin when sound is off.

# AlwaysTop review

Contributor notes for the AlwaysTop mini-app: PowerToys Always On Top
parity, technical debt, the `top/` module map, and optional
product backlog. Runtime behavior is defined in [architecture](architecture.md);
this page records review findings and deferred work. No product code change is
implied by editing this guide alone.

## Current contract

AlwaysTop pins eligible top-level windows with `SetProp` plus
`SetWindowPos(HWND_TOPMOST, …)`, keeps any number of pins until each window is
toggled, closed or PowerKit exits, draws a PowerToys-style D2D FrameDrawer ring
behind each pin, steps layered opacity between 20% and 100% in 10% increments,
and plays Windows Media cues when sound is enabled. Pins and window opacity are
not persisted. Defaults: Win+Ctrl+T (pin), Win+Ctrl++ / Win+Ctrl+- (opacity),
with Win+Alt fallbacks when those defaults are taken.

| Surface | Role |
| --- | --- |
| `src/always_top.rs` | Pure pin summary, hotkey values, opacity/frame clamps |
| `src/platform/windows/top/` | UI-thread pin ownership, hooks, border host, opacity, hotkeys, smoke (see the module map below) |
| `src/platform/windows/frame.rs` | D2D hollow ring (FrameDrawer port) |
| Settings / tray / hotkeys | Prefer preferences; tray pin uses last eligible foreground |

Hotkey pin acts on the current foreground root window. The tray (and settings)
pin action uses the last remembered eligible window so the user can open the
menu without losing the intended target.

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
| Show border toggle | Yes | No | Border always drawn today |
| Rounded corners toggle | Yes | No | Corner radius follows DWM automatically |
| Excluded apps | Yes | No | Intentional gap in architecture |
| Do not activate in Game Mode | Yes | No | Intentional gap |
| Title-bar system menu item | Yes | No | Intentional gap |
| Elevated windows / UIAccess | Partial | No | Intentional; Store/package cost is high |

### Deferred parity (product decision required)

Recommended order when implementing parity (separate change set):

1. Settings parity: persist `enable_frame`, `round_corners`, `excluded_apps`,
   `block_in_game_mode`; WinUI AlwaysTop cards; Turkish / English / Arabic
   strings. Match PowerToys excluded-app partial matching semantics.
2. Small behavior fix before or with that package: tray opacity up/down should
   target the same window as the hotkeys (pinned foreground), not only the last
   entry in the pin vector.
3. Title-bar system menu injection and UIAccess for elevated windows stay
   separate decisions; evaluate UIAccess only with Store package identity and
   elevation policy.

## Technical debt

1. **Resolved: monolithic `top.rs`** — split into `top/` without behavior
   changes (see the module map below).
2. **Resolved: hotkey label mismatch** — settings and tray both show the
   registered chord (including a Win+Alt fallback) through
   `Hotkey::register_with_fallback`, shared with AlwaysOn and ScreenRuler.
3. **Opacity target mismatch** — tray steps the last pin; hotkeys step the
   foreground pin.
4. **Resolved: `install_slot` failure reporting** — a chord that could not be
   registered is no longer shown in the tray; settings then show the stored
   chord without implying it is active elsewhere.
5. **Dual Win32 crates** — `windows_sys` in `top/`, `windows` in `frame.rs`.
6. **100 ms border timer** — continuous poll; prefer event-driven sync with a
   low-frequency backup if profiling warrants it.
7. **Hard-coded shell skip classes** — PowerKit overlay class names; easy to
   miss when adding tools.
8. **Test gaps** — border z-order, minimize/restore, hook paths, opacity target
   consistency and hotkey fallback UX lack automated coverage beyond domain
   clamps and `top::smoke`.

## Module map

`src/platform/windows/top/` replaced the single `top.rs` in a behavior-preserving
split. `PinGuard`, the `thread_local` `PinState`, HWND restore and border destroy
stay on the UI thread; pins and window opacity are still not persisted.
`frame.rs` remains the D2D port. Submodules reach shared state through
`use super::*`, and only `step_foreground_opacity`, `step_pinned_opacity` and
`smoke` are re-exported beside the `mod.rs` commands, so callers keep `top::…`.

| Module | Responsibility |
| --- | --- |
| `mod.rs` | `PinState`, `Tracked`, `PinGuard`, `apply_prefs`, public commands (`toggle`, `reconcile`, `release`, `shutdown`, `listen`), topmost/prop marking and sound cues |
| `eligibility.rs` | Eligible roots, `resolve_target`, shell/skip list, titles |
| `border.rs` | Frame paint, border class/HWND lifecycle, timer, `glue_border_for` / z-order sync |
| `opacity.rs` | Layered apply/restore and step helpers |
| `hotkeys.rs` | `RegisterHotKey` slots, Win+Alt fallbacks, labels |
| `hook.rs` | WinEvent install/uninstall and event branches |
| `smoke.rs` | HWND smoke paths, kept out of production helpers |

## Optional product backlog (beyond PowerToys)

Priority order for later product decisions:

1. **Click-through** — let clicks pass through a translucent pinned window
   (PowerToys does not offer this; high value on a single display).
2. **Pin list / bulk unpin** — tray or settings surface listing open pins.
3. **Excluded-app templates** — starter lists for games and launchers once
   excluded apps exist.
4. **Per-monitor topmost** — optional chord that only raises on the current
   monitor to reduce multi-monitor z-order noise.
5. **AlwaysTop-specific sound** — `pin_sound` separate from the global `sound`
   preference.
6. **Brief border emphasis** — short visual pulse on pin when sound is off.

## Implementation status

As of this review:

- Documentation and design only; no AlwaysTop code, settings keys, locale or
  changelog updates accompany this guide.
- Module split, behavior fixes and PowerToys settings parity wait for an
  explicit implementation task.

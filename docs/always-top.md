# AlwaysTop review

Contributor notes for the AlwaysTop mini-app: PowerToys Always On Top
parity, technical debt, the `top/` module map, and optional
product backlog. Runtime behavior is defined in [architecture](architecture.md);
this page records review findings, the remediation plan and deferred work.

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
| `src/always_top.rs` | Pure pin session (none, one or a count), opacity/frame clamps |
| `src/hotkey.rs` | Shared keyboard chords, defaults and Win+Alt fallback for all tools |
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
2. Small behavior fixes are done: the tray opacity items now follow the window
   the tray pin item targets.
3. Title-bar system menu injection and UIAccess for elevated windows stay
   separate decisions; evaluate UIAccess only with Store package identity and
   elevation policy.

## Technical debt

Resolved earlier: the monolithic `top.rs` split into `top/`, hotkey labels that
showed an unregistered chord, the tray opacity target mismatch and
`install_slot` failure reporting. Mixed `windows_sys` / `windows` use is
repository-wide, not AlwaysTop debt, and is not tracked here.

## Remediation plan

Findings come from a code review of `always_top.rs`, `top/` and `frame.rs`
(2026-11-19). Each item is checked off only when its change and checks land,
with the evidence noted after it (unit, smoke, physical). Items marked
*physical* need a real-window check before they count as done.

### Phase 1 — small correctness fixes

- [x] **P1.1 Exclude PowerKit's own process.** `eligible` checks only the
  hidden app window and windows it owns; the WinUI settings window has no
  owner, so the pin hotkey, the page's pin button or the 1 s
  `remember_foreground` tick can pin or remember it. Reject any window whose
  `GetWindowThreadProcessId` is the current process.
  *Done: `own_process` in `eligibility.rs`; unit test
  `windows_of_this_process_are_never_eligible`. Physical check of the
  settings window still pending.*
- [x] **P1.2 Opacity steps drift.** `percent_from_alpha` truncates (90% →
  alpha 229 → 89%), so steps run 100, 90, 79, 68 … and 89, 99, 100. Round to
  the nearest percent and cover every 10% step in a unit test.
  *Done: rounding in `always_top.rs`; unit test
  `opacity_steps_do_not_drift_through_alpha`; smoke passes.*
- [x] **P1.3 Pinned property leaks.** `PowerKit.AlwaysTop.Pinned` is removed
  only when `set_topmost(false)` runs, which is skipped for a window that was
  already topmost; a failed `set_topmost(true)` also leaves it. Remove the
  property on every unpin and on pin failure.
  *Done: `pin_window` / `unpin_window`; covered by the existing smoke
  pin/unpin path, no dedicated test for the already-topmost case.*
- [x] **P1.4 Per-slot default chord.** `install_slot` treats a chord as a
  default when it matches any of the three defaults; pass each slot's own
  default.
  *Done: `install_slot` takes the slot default; the fallback rule itself is
  unit-tested in `always_top.rs`.*

### Phase 2 — pin and border lifecycle (*physical*)

- [x] **P2.1 Dead pins clean up immediately.** `is_pinned_window` requires a
  live window, so `EVENT_OBJECT_DESTROY` never unpins the destroyed window and
  the `!live` branch in `sync_border` is unreachable; the orphan ring stays up
  to 1 s until `reconcile`. Look pins up by exact HWND without a liveness
  check, and unpin on destroy only for that exact HWND (not a child's root).
  *Done: `tracked` (exact HWND, no liveness) in `mod.rs`; destroy and the ring
  timer's `sync_border` use it. Smoke `ring_lifecycle`: a child's destroy
  keeps the pin, a destroyed target loses its pin and ring. Physical check
  with another app's window pending.*
- [x] **P2.2 Hidden and cloaked pins hide their ring.** Handle
  `EVENT_OBJECT_HIDE`/`SHOW` and `EVENT_OBJECT_CLOAKED`/`UNCLOAKED` so
  close-to-tray apps and virtual-desktop switches do not leave a ring behind.
  *Done: a third WinEvent hook for the cloak pair; `glue_border_for` decides
  ring visibility from `shown` (visible, not minimized, not cloaked). Smoke
  `ring_lifecycle` hides, shows, cloaks and uncloaks a fixture through
  `on_event` (hooks skip PowerKit's own process). Physical close-to-tray and
  virtual-desktop checks pending.*
- [x] **P2.3 Unpin never leaves partial state.** `unpin_window` returns early
  when `SetWindowPos` fails, after the ring and alpha are already gone, and
  `release` stops at the first failure so exit can leave windows topmost.
  Always drop the pin, try every window and report the first error.
  *Done: `take_pin` removes the pin first, `restore_layered` takes the saved
  state, `release` tries every pin. Existing smoke pin/unpin paths pass; a
  failing `SetWindowPos` is not simulated.*
- [ ] **P2.4 Review reapply activation.** Reapplying `HWND_TOPMOST` after a
  focus change calls `SetWindowPos` without `SWP_NOACTIVATE` and synchronously
  against another process. Confirm physically whether it steals focus or can
  block on a hung window; switch the reapply path only if it does.
  *Open: unchanged. PowerKit is a background process when the hook fires, so
  the foreground lock should stop activation, but that is unverified; needs a
  hand check (pin a window, cover it with another topmost window, switch
  apps).*
- [x] **P2.5 Release the D2D factory on shutdown.** Found while extending the
  smoke check: after a ring had been drawn, the thread-local `ID2D1Factory`
  was released by the TLS destructor at process exit and raised exception
  0x87A in KERNELBASE (smoke exit code 2170 after "passed"). `top::shutdown`
  now calls `frame::release_factory` after every pin is gone.
  *Done: release smoke exits 0 three runs in a row; before the fix it exited
  2170 on every run.*

### Phase 3 — border cost

- [x] **P3.1 Cache the accent color.** `frame_paint` creates a WinRT
  `UISettings` on every paint (every 100 ms per pin). Cache it and refresh on
  `WM_SETTINGCHANGE` / `WM_DWMCOLORIZATIONCOLORCHANGED`.
  *Done: `ring_accent` in `border.rs` reads the accent once for rings; both
  broadcasts clear it, and so does creating a ring. The settings page still
  reads the live accent. Smoke passes; a live accent change is not tried yet.*
- [x] **P3.2 Skip unchanged paints.** Rebuild geometry and redraw only when
  size, color, thickness or radius changed; stop re-arming the periodic timer
  on every tick and lengthen it to a low-frequency backup.
  *Done: `PaintKey` in `frame.rs` skips the D2D repaint while size, color,
  opacity, thickness and radius are unchanged; a ring shown again repaints.
  The backup timer is set once at 500 ms; focus restacks a pinned window's ring
  directly, since the timer no longer does it quickly. Unit tests
  `identical_inputs_skip_the_repaint` and
  `first_paint_and_any_changed_input_reach_d2d`. Physical z-order check pending.*
- [x] **P3.3 Reuse the border window.** Minimize/restore destroys and recreates
  the ring HWND and D2D target; hide and show it instead, and register the
  window class once.
  *Done: minimize hides the ring (`hide_border_for`) and restore shows the same
  HWND; the class registers once per thread. Smoke `ring_lifecycle` checks the
  HWND survives hide/show and minimize/restore. Physical minimize check pending.*

### Phase 4 — structure and localization

- [x] **P4.1 `PinState` API.** Replace ~15 repeated
  `pins.iter().find(|pin| pin.hwnd == target)` blocks with `find`/`with_pin`/
  `remove` helpers; do not call `ShowWindow` or D2D while the `RefCell` is
  borrowed; replace the manual `holding` flag with a guard; move the test-only
  `discover` flag out of production state.
  *Done: `with_pin` / `with_pin_mut` replace the lookups; the ring paints with
  its drawer moved out of the pin (`take_drawer` / `put_drawer`), so D2D never
  runs under the borrow; `Holding` clears the re-apply mark on every exit. The
  test-only flag is gone: `resolve_target(app, candidate)` takes the foreground
  window as a parameter, and the smoke check passes null. Smoke exits 0; no
  pin/unpin is driven by hand yet.*
- [x] **P4.2 Shared `hotkey` module.** Move `Hotkey`, its defaults and
  `register_with_fallback` from `always_top.rs` to `src/hotkey.rs`; AlwaysOn,
  ScreenRuler and PowerMouse use it too.
  *Done: `src/hotkey.rs` holds the chord type, the four defaults, labels and the
  Win+Alt fallback; every tool imports `crate::hotkey::Hotkey`. The chord tests
  moved with it.*
- [x] **P4.3 Localize the pin summary.** `Pin::summary` embeds Turkish
  ("{n} pencere", "Pencere"), so English and Arabic tooltips show Turkish.
  Return a count variant and format it in `locale.rs`; also localize the `%{}`
  frame-opacity label.
  *Done: `Pin::Several { count }` and an empty `Pin::On` title carry data only;
  `Strings::always_top_status` formats them for the tooltip, the tray root item
  and the settings page. `Strings::percent` formats the frame opacity: "%50" in
  Turkish, "50%" in English and Arabic. Unit tests
  `always_top_status_and_percent_follow_the_language`.*
- [x] **P4.4 Remove duplicate foreground tracking.** The tray callback and
  `menu()` both call `remember_foreground`.
  *Done: the callback no longer calls it; `menu()` does, once. The 1 s
  reconcile tick and AlwaysOn's bind still call it for their own reasons.*

### Phase 5 — close-out

- [x] **P5.1** Update [architecture](architecture.md), this guide's module map
  and the Unreleased changelog for observable changes.
  *Done for Phases 1–4: architecture rows for `always_top.rs` and `hotkey.rs`,
  the module map above and the changelog bullets are in place.*
- [ ] **P5.2** Record physical evidence for Phase 2 (close, close-to-tray,
  virtual desktop switch, minimize/restore, exit with pins).

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

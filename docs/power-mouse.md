# PowerMouse design

PowerMouse is a [utility](ui-structure.md): bindings are edited on the tool
page and take effect system-wide while the tool is enabled. Remapping happens
in PowerKit's own user session; there is no service or driver, and no vendor
software is required.

## Layers

PowerMouse follows OpenLogi where practical, with one difference: it works
with every mouse, not only Logitech ones.

| Layer | Works with | Provides |
| --- | --- | --- |
| Generic | Any mouse Windows exposes through the standard mouse stack | The triggers, bindings and actions in this document |
| Device adapter (MX Master 4 over Bluetooth; receivers planned) | Selected mice, Logitech HID++ first | Extra triggers (gesture button, mode shift button, haptic panel, thumb wheel), device settings (DPI, SmartShift, wheel) and state (battery, connection) |

- Adapter button events feed the same press state machine; binding types,
  actions and the injector are shared by both layers.
- The generic layer keeps working when no adapter matches or the device is
  disconnected. The page shows the device section only for a connected,
  recognized device.
- Adapter features stay off for a model that was not physically verified. The
  one verified model is MX Master 4 over Bluetooth. A mouse behind a Bolt or
  Unifying receiver is not supported yet. The generic layer's "every mouse"
  claim is likewise physically verified on that one device only; see
  [Verification status](#verification-status).
- While Logi Options+, OpenLogi or MX4Lite is running, the adapter stays off
  and the page says so; the generic layer keeps working.
- In scope: hold-to-talk bindings, typed text, multi-step workflows,
  Easy-Switch, the MX Master 4 haptic panel and an Actions Ring with one
  layout.
- Not offered: mouse settings that Windows Settings already has; see
  [Settings left to Windows](#settings-left-to-windows).
- Out of scope: per-application profiles, keyboards, lights and webcams,
  launching programs, Left and Right button remapping, and wheel rotation
  remapping.

Protocol references: OpenLogi is MIT OR Apache-2.0. Solaar is GPL and may
inform protocol understanding only, never source.

## Device discovery

Discovery finds the connected devices a vendor adapter exists for. It only
identifies them; talking to a device is the adapter's job.

- **Scan:** `discovery.rs` lists the present HID collections
  (`CM_Get_Device_Interface_ListW`) and opens each with zero desired access,
  which is enough to ask for vendor and product id (`HidD_GetAttributes`),
  top-level usage (`HidP_GetCaps`) and product string. A collection that
  cannot be opened or described is skipped.
- **Grouping and classification** (pure, `crates/core/src/power_mouse/device.rs`):
  Logitech collections are grouped into devices by the hardware id of their
  path. A device is kept when it has an HID++ collection and is either a
  receiver (Bolt `C548`, Unifying `C52B`/`C532`) or exposes a mouse
  collection, so Logitech keyboards and other vendors' devices are left out.
  HID++ collections are usage page `FF00` (usage 1 short report, usage 2 long
  report) on receivers and wired devices, and usage page `FF43` on Bluetooth
  LE, where the usage's low byte is a mask of the reports carried.
- **Verified models:** a table by transport and product id. Its one entry is
  MX Master 4 over Bluetooth (`B042`), read from the physical device:
  collections `0001/0002` (mouse), `FF43/0202` (HID++ long report only) and
  `000E/0001`, with an empty product string, so the name comes from the
  table. Adapter features stay off for any model not in the table. A mouse
  behind a receiver would be identified over HID++, never by the receiver's
  id. The Bolt receiver's collections have not been read from hardware.
- **Thread and lifecycle:** `devices.rs` runs the scan on its own thread while
  PowerMouse is running, never on the UI thread, publishes the list and posts
  a command to the PowerKit window when it changed. `WM_DEVICECHANGE`, which
  every top-level window receives, asks for a new scan; a burst of changes is
  scanned once after 400 ms of quiet. Stopping PowerMouse stops the thread
  and clears the list.
- **Page:** a "Devices" section lists each device with its connection. A
  device the adapter drives shows the adapter's state and battery; otherwise
  the row says that the model is not verified or that the mouse behind a
  receiver cannot be identified yet. The section is absent when no such
  device is connected.
- **Limits:** two identical receivers appear as one device. Whether a
  Bluetooth mouse leaving and rejoining produces `WM_DEVICECHANGE` promptly
  is not verified.

## Logitech adapter

The adapter adds device controls as triggers and reads the battery. It runs
for a connected device that is a verified model, connected directly, with a
long-report HID++ collection (`Device::adapter_ready`).

- **Protocol** (pure, `crates/core/src/power_mouse/hidpp.rs`): HID++ 2.0 long reports
  (`0x11`, 20 bytes: device index, feature index, function and software id
  nibbles, 16 payload bytes). A directly connected device is index `0xFF`.
  PowerKit's software id is `0x0D`; replies are matched by device, feature,
  function and software id, error replies (feature index `0xFF`) by the
  echoed request header. Features used: device name and type `0x0005`,
  unified battery `0x1004`, reprogrammable controls `0x1B04`, wireless
  status `0x1D4B`. The framing and feature layouts were confirmed against
  OpenLogi's `openlogi-hidpp` crate (MIT OR Apache-2.0) and reimplemented;
  no source was copied.
- **Channel** (`hidpp_channel.rs`): overlapped `ReadFile`/`WriteFile` on the
  collection, opened shared. It implements the `HidppChannel` port, so the
  protocol code above it (`crates/app/src/controller/power_mouse`) does not know Windows. A channel belongs to the thread that opened it.
  A request waits up to 1.5 s for its reply; events that arrive meanwhile
  are kept, other clients' replies are dropped.
- **Device triggers:** gesture button (control `00C3`, key
  `gesture_button`), mode shift button (`00C4`, `mode_shift`) and haptic
  panel (`01A0`, `haptic_panel`). They take every binding type. A control is
  diverted only while it has a binding; an unbound control keeps its
  function on the device. Raw XY is requested only for a gesture binding, so
  other bindings leave the cursor free while the control is held; the
  device's movement reports then drive the swipe, with no cursor freeze.
- **Thread** (`adapter.rs`): one thread per device owns the channel and the
  waiting. The decisions are `Link` in `crates/app/src/controller/power_mouse/link.rs`:
  it identifies the device, diverts the bound controls, turns the device's
  "held controls" reports into press and release edges and its raw movement
  into gesture travel. The thread sends them to the hook thread, which owns
  the engine. Commands from the UI thread (new bindings, elevated
  foreground, stop) interrupt its wait through an event handle.
- **Elevated windows:** while an elevated window is in the foreground the
  adapter undiverts everything, so the controls do their own function there
  instead of nothing; diversion returns when the window leaves the
  foreground. This was chosen over accepting dead buttons or requesting
  UIAccess.
- **Reconnect and lifecycle:** diversions are volatile on the device. The
  wireless-status event (the device came back and wants reconfiguring)
  clears what the adapter believes is diverted and applies it again. A
  device that stops answering is retried every 10 s. Stopping PowerMouse,
  suspend and exit stop the adapter, which first undiverts its controls and
  releases any still held; resume starts it again. Undiverting stops at the
  first control that gets no answer, so a device that is gone delays the
  stop by one request timeout at most.
- **Other applications:** while Logi Options+, OpenLogi or MX4Lite is
  running, the adapter leaves the device alone and the page says which one
  is running; the generic layer keeps working. The check runs when the
  adapter connects and every two minutes.
- **Battery:** read on connect, on the device's battery event and every two
  minutes. The page shows the level; at 15 % or less while discharging a
  tray notice is shown once, and again only after the battery has recovered.
- **Page:** the device row shows the name the device reports, its
  connection, the adapter state and the battery. A "Device buttons" section
  lists the device triggers the connected device offers, and any that still
  have a binding.

### Device settings and actions

`crates/app/src/controller/power_mouse/device_settings.rs` runs on the adapter thread, on
the adapter's channel.
Each setting is offered only when the device reports the feature.

| Setting or action | Feature | Behavior |
| --- | --- | --- |
| DPI (`mouse_dpi`) | Adjustable DPI `0x2201` | The page offers the presets 400, 800, 1000, 1200, 1600, 2400, 3200 and 4000 that the sensor supports; the first choice leaves the device's own DPI alone and shows it |
| Wheel mode (`mouse_wheel_mode`: `ratchet`, `free_spin`, `smart_shift`) | SmartShift enhanced `0x2111` | Ratchet never releases (release speed `0xFF`); SmartShift is ratchet with the device's default release speed; torque is not changed |
| High-resolution scrolling (`mouse_hires_wheel`) | High-resolution wheel `0x2121` | Only the resolution bit of the wheel mode is changed; the other bits are written back as read |
| Thumb wheel | Thumb wheel `0x2150` | While either direction (`thumb_left`, `thumb_right`) is bound, the wheel is diverted and stops scrolling sideways; its rotation events become the two triggers, one press per uninterrupted turn, single action only. It is given back on stop and while an elevated window is in front |
| Easy-Switch 1–3 (`host_1`…`host_3`) | Change host `0x1814` | The device moves to that paired computer; it sends no reply and then stops answering |
| Haptic tap (`haptic_pulse`) | Haptic feedback `0x19B0` | Plays waveform 4, a short tap |
| Next DPI (`dpi_cycle`) | Adjustable DPI | Steps to the next supported preset, wrapping to the lowest. The step is not stored: the saved DPI, if any, returns on reconnect |

- A setting left on "keep the device's setting" is never written. A set
  value is written when it differs from what the adapter last set, and again
  after the device reconnects.
- DPI and wheel settings are the user's choice and stay on the device when
  PowerMouse stops; "Reset to defaults" only stops managing them.
- Device actions are sent from the injector thread straight to the adapter
  threads and reach every connected device; a device without the feature
  ignores them, and with no adapter running they do nothing. An action fired
  while the device is unreachable is dropped, not replayed later.
- The settings are global keys, not per device: with one verified model
  there is nothing yet to tell apart. They become per-device keys when a
  second model is verified.

## Actions Ring

A ring of eight slots that opens around the cursor; clicking a slot runs
its action. It belongs to the generic layer and works with every mouse.
There is one layout; per-application layouts are out of scope.

- **Opening:** the "Show Actions Ring" action (`actions_ring`), bound to any
  trigger. Firing it while the ring is open closes the ring. The ring does
  not open while an elevated window is in the foreground, where a chosen
  action could not act, or when no slot has an action.
- **Slots as triggers:** each slot is a trigger (`ring_1`…`ring_8`, from the
  top clockwise) that takes a single action, so a slot may hold anything a
  button can: a catalog action, a recorded shortcut, typed text, a workflow
  or a device action. Choosing a slot presses and releases its trigger on
  the hook thread, so the action runs through the same engine and injector;
  a mouse-button action becomes one click. A slot cannot hold the action
  that opens the ring.
- **Window** (`ring_window.rs`, UI thread): a layered, topmost popup with
  `WS_EX_NOACTIVATE`, 340 DIP wide, centred on the cursor and kept inside
  the monitor's work area. It never takes focus, so the foreground
  application is still the target when the action fires. For the same
  reason it reads input by polling every 30 ms rather than by focus: the
  cursor position drives the hover highlight; a left-button press chooses
  the bound slot under the cursor and cancels anywhere else; Esc, a right or
  middle press, and 10 s without a choice cancel. A click the window itself
  receives is handled directly as well, so a click shorter than one poll is
  not lost. Like the tray menu, hotkeys and ScreenRuler overlays, the ring
  is Win32, not WinUI.
- **Drawing** (pure, `crates/core/src/power_mouse/ring.rs`): wedges are rasterized into
  premultiplied BGRA with edges anti-aliased by distance; bound slots are
  dark, the hovered one takes the Windows accent colour and empty ones are
  dimmed and not selectable. Labels are drawn with GDI at 10 pt, wrapped
  onto up to three lines; the ring's alpha is restored afterwards because
  GDI text clears it. A label is the action's name, the recorded keys, or
  the first 18 characters of typed text.
- **Lifecycle:** stopping PowerMouse, suspend and exit close the ring.

## PowerMouse UI and device artwork

The PowerMouse page (`crates/ui/reactor/src/window/mouse_page.rs`) runs as a
full-height split workspace inspired by OpenLogi:

- **Sticky device header:** 48×48 rounded device thumbnail (real
  `front_core.png` render when cached, otherwise a mouse glyph), device name,
  inline transport / adapter-link / battery pills, the global PowerMouse
  switch and a reset icon button.
- **Buttons tab:** interactive mouse stage (`crates/core/src/power_mouse/diagram.rs`)
  with a real `side_core.png` render for supported Logitech models (or a
  sculpted vector mouse silhouette for generic mice), 2-segment 45°-elbow
  leader lines, circular physical anchor dots and floating two-line callout
  cards, plus a secondary trigger strip (`F13–F24` and horizontal scroll when
  not on canvas) and a fixed 328 DIP right-side inspector with a 5-direction
  interactive gesture list.
- **Scrolling & pointer tab:** two-column hardware dashboard when an active
  adapter is connected (live DPI readout, WinUI slider, preset chips, wheel
  mode segmented control and HiRes toggle) above hairline-grouped generic
  scrolling toggles.
- **Actions Ring tab:** centered 8-slot circular radial preview paired with a
  direct slot inspector (immediate action selection without an extra binding
  kind step, plus one-click clear).
- **Off-UI device artwork cache (`crates/platform/windows/src/power_mouse/assets.rs`):**
  when a recognized Logitech device is discovered, `front_core.png` and
  `side_core.png` are fetched off the UI thread from OpenLogi's public asset
  CDN (`https://assets.openlogi.org/v1/devices/{depot}/{file}`, with jsDelivr
  `@logi-assets/pointing` fallback), validated as PNG (`<= 2 MB`), written
  atomically to `%LOCALAPPDATA%\PowerKit\mouse-assets\`, and surfaced to the
  WinUI shell via an atomic revision counter. No Logitech product renders are
  checked into this repository; offline or generic mice fall back smoothly to
  the sculpted silhouette.

## Reference: MX4Lite and OpenLogi

MX4Lite (a personal fork of OpenLogi, MIT OR Apache-2.0) was reviewed as
reference material. It is not a dependency and is not ported:

- MX4Lite diverts two Logitech MX Master 4 controls (gesture button, haptic
  panel) over HID++ `0x1b04` and maps them to six fixed shortcuts. That path
  is Logitech-specific; it belongs to the [Logitech adapter](#logitech-adapter),
  not to the generic layer.
- MX4Lite's LocalSystem service plus per-session agent exists to restart a
  hidden agent across user switches. PowerKit already runs in the user
  session, so the service model is not needed.
- OpenLogi's crates bring tokio, async-hid, tarpc and GPUI. PowerKit takes
  none of them as a dependency; the useful parts are reimplemented in narrow
  modules.

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

Generic layer, as implemented:

- Remap Middle, Back (X1), Forward (X2) and horizontal scroll left/right on
  any mouse Windows exposes through the standard mouse stack.
- Remap F13–F24, which many mice and macro pads send for extra buttons.
- Per trigger, choose pass-through (default), a single action, a hold-down
  (hold-to-talk) action, a short/long press pair, or a hold-and-swipe gesture
  (click, up, down, left, right).
- Actions from a fixed Windows catalog, a recorded custom keyboard shortcut,
  typed text, a multi-step workflow or a PowerKit command.
- One global on/off switch from the page, the tray and the General card.

Not in the generic layer: per-device bindings, DPI and other device settings
(the adapter's job). Out of scope altogether: Left and Right button
remapping, wheel rotation remapping, per-application profiles, launching
programs or commands and a tray-free background mode.

## Triggers

| Trigger | Windows input | Press/release | Gesture |
| --- | --- | --- | --- |
| Middle | `WM_MBUTTONDOWN` / `UP` | Yes | Yes |
| Back | `WM_XBUTTONDOWN` / `UP`, `XBUTTON1` | Yes | Yes |
| Forward | `WM_XBUTTONDOWN` / `UP`, `XBUTTON2` | Yes | Yes |
| Horizontal scroll left / right | `WM_MOUSEHWHEEL`, negative / positive | No: a burst of wheel events while scrolling | No |
| F13–F24 | `WM_KEYDOWN` / `UP`, `VK_F13`–`VK_F24` | Yes (key repeat ignored) | No |
| Gesture button, mode shift button, haptic panel | Not Windows input: reported by the [Logitech adapter](#logitech-adapter) while diverted | Yes | Yes, from the device's raw movement |
| Thumb wheel left / right | Not Windows input: rotation reported by the adapter while the thumb wheel is diverted | No: a burst of rotation events | No |
| Ring slot 1–8 | Not Windows input: chosen in the [Actions Ring](#actions-ring) | Pressed and released at once | No |

- Left and Right are never suppressed, so a bad binding cannot make the
  pointer unusable.
- Horizontal scroll comes from a tilting wheel or a thumb wheel; Windows
  reports both as `WM_MOUSEHWHEEL`, so they are one trigger pair and a bound
  action replaces horizontal scrolling. The storage keys and code identifiers
  are `tilt_left` and `tilt_right`.
- Horizontal scroll has no release edge. It supports a single action only,
  fired once per burst; a burst ends after 250 ms without events in the same
  direction, measured from the last event, so a continuous scroll never
  repeats the action.
- F13–F24 are matched by virtual key regardless of which device sent them;
  the page says so. The keyboard hook is installed only while at least one
  F-key is bound.
- Unbound triggers and every other event pass through untouched.

## Bindings

| Binding | Behavior |
| --- | --- |
| Pass-through | Default. The trigger is not suppressed |
| Single | The action fires on press. A mouse-button action is mirrored: its down is sent on press and its up on release, so dragging still works |
| Hold down | Hold-to-talk: the action's key chord or mouse button goes down on press and stays down until the trigger is released. An action that cannot stay down (a PowerKit command, Lock, typed text, a workflow) fires once on press |
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

### Settings left to Windows

PowerMouse offers only mouse settings that Windows Settings
(Bluetooth & devices > Mouse) does not. Primary button, pointer speed,
enhanced pointer precision, scroll direction, lines per notch and scrolling
inactive windows are left to Windows; the Scrolling and pointer tab links
there. DPI is not Windows' pointer speed: it is the sensor resolution, set on
the device by the adapter, and the page calls it that.

PowerMouse does not touch the vertical wheel at all: scroll direction is a
Windows setting, and there is no Back/Forward-plus-wheel side scroll.

### Gesture movement

- Movement deltas are measured from the last cursor position Windows actually
  applied to each `MSLLHOOKSTRUCT.pt` (`CursorAnchor`); the thresholds are
  scaled from 96 DPI to the DPI of the monitor under the cursor.
- The cursor stays still while a gesture trigger is held: `WM_MOUSEMOVE` is
  suppressed during the hold, and because a swallowed move does not move the
  cursor, the anchor is not advanced past it. Each hook point is then the
  frozen position plus one step, and the steps add up to the travel. A
  gesture binding therefore replaces middle-drag panning for that button; the
  page says so.

## Actions

Each action maps to one Windows input sequence. Labels live in
`crates/core/src/locale.rs`; storage tokens are English and stable.

| Group | Actions |
| --- | --- |
| Mouse | Left, Right, Middle, Back, Forward click |
| Navigation | Browser back / forward (`VK_BROWSER_BACK` / `FORWARD`), Task View (Win+Tab), previous / next virtual desktop (Win+Ctrl+Left / Right), Show desktop (Win+D) |
| Editing | Copy, Paste, Cut, Undo, Redo (Ctrl+Y), Select all, Find, Save |
| Tabs | New tab, Close tab, Reopen tab, Next tab, Previous tab, Reload |
| Media | Play/pause, Next track, Previous track, Volume up, Volume down, Mute |
| System | Capture region (Win+Shift+S), Lock (`LockWorkStation`, because injected Win+L is ignored by Windows), Do nothing |
| PowerKit | Pin or unpin the last window with AlwaysTop, Start ScreenRuler, Open PowerKit, Show Actions Ring |
| Device | Easy-Switch 1–3, Haptic tap, Next DPI; see [Device settings and actions](#device-settings-and-actions) |
| Custom | A recorded shortcut: modifiers plus one virtual key |
| Text | Up to 120 characters. A character the foreground window's keyboard layout types with a plain or Shift key (`VkKeyScanExW`, dead keys excluded) is sent as that key; anything else, and letters while Caps Lock is on, as a Unicode packet (`KEYEVENTF_UNICODE`), one per `SendInput` with 10 ms between them. Windows keeps only the last packet's character per thread, so a burst of packets to a target that reads them late came out as the last character repeated. A line break is sent as Enter and a tab as Tab |
| Workflow | Up to 8 steps run in order; a step is any other action or a pause of at most 5 s |

- Custom shortcuts are recorded with the existing settings-shell key capture
  (`winui_host::begin_key_capture`, `CAPTURED_KEY`) and stored in the same
  `mods,vk` form as tool hotkeys.
- PowerKit actions are posted to the hidden app window as the existing
  `AppCommand` values and run on the UI thread, exactly as tray commands do.
- Text is typed only after Apply on the page; it is refused while a modifier
  is held, because the characters would turn into shortcuts.
- A workflow pause is a deadline on the injector thread, not a sleep, so
  button releases and other outputs are still handled while it waits. While
  a workflow is running or pausing, a second one is not started. Stopping
  PowerMouse drops the steps that have not run.
- An action is skipped, never partially replayed, when a physical modifier is
  already held, the input desktop is not `Default` (lock screen, UAC), or
  `SendInput` accepts only part of the batch; keys accepted by a partial batch
  are released.

## Settings page and tray

Follows the utility skeleton in [Tool UI structure](ui-structure.md), laid out
like a device page in Logi Options+ or OpenLogi: the device first, then tabs,
and on each tab a picture to choose from beside an editor for the choice.
The code is `crates/ui/reactor/src/window/mouse_page.rs`.

1. **Device card:** the name of the connected device an adapter drives (or
   "All mice" when there is none), its connection, adapter state and battery,
   and the main switch with its state as the label on its left. Other
   detected devices follow as one line each.
2. **Tabs:** Buttons, Scrolling and pointer, Actions Ring.
3. **Buttons tab:** on the left a top-view diagram of a mouse, drawn from
   shapes in theme colours, with one numbered hotspot per trigger that has a
   place on a mouse; a dot marks the ones that are bound. The thumb-side
   column grows with the controls the connected device offers. Under the
   diagram the same triggers are listed by group with what each does now, and
   the F13–F24 keys the user added, with "Ekstra tuş ekle…". Choosing a
   hotspot or a row opens that trigger in the editor on the right: the
   binding type, then one action picker per slot. The picker lists the
   catalog and ends with "Kısayol kaydet…" (the shared hotkey capture),
   "Metin yaz…" (a text box with Apply) and "İş akışı…" (a step list: each
   step has its own picker, which offers pauses instead of a nested workflow,
   and a remove button; "Adım ekle" appends a step). Editor fields put the
   label above the control, so the control keeps the column's width.
4. **Scrolling and pointer tab:** the connected device's settings (sensor
   resolution, wheel mode, high-resolution scrolling) or a note that they
   appear when a supported mouse is connected, then a note and a link to
   Windows mouse settings for what PowerMouse leaves to Windows.
5. **Actions Ring tab:** a preview with the eight slots placed around a
   circle as they appear on screen, a line naming the triggers that open the
   ring (or saying that none does yet), and the editor for the chosen slot.
6. A "Varsayılana dön" command at the foot that sets every trigger back to
   pass-through and stops managing the
   device settings.

Changes apply immediately: the UI thread publishes a new binding snapshot to
the hook thread and writes settings. PowerMouse joins the rail after
ImageResizer. The tray lists PowerMouse as one checked item that switches it
on or off, at the root like the other one-command tools. Because that item is the switch, it stays listed while PowerMouse
is off; Settings can hide or reorder it like any tray tool (`power_mouse` in
`tray_tools`, listed by default, last). General shows one card with status, Open and an
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
- Outputs go to the injector over an unbounded channel, so the callback never
  blocks and a button release is never dropped. The injector sends a
  button-up only for a button it actually put down. Binding snapshots are
  replaced by a thread message to the hook thread, which applies the newest
  queued snapshot, so the callback reads only thread-local state.
- The hook ignores events with `LLMHF_INJECTED` / `LLKHF_INJECTED`, so its own
  output (and other injected input) is never remapped.
- Elevated foreground windows: input to a higher-integrity window is not
  remapped, and `SendInput` into it is blocked by UIPI (PowerToys Keyboard
  Manager has the same limit). A foreground WinEvent hook on the UI thread
  publishes an atomic "foreground is elevated" flag; while it is set, every
  trigger passes through so a press is never swallowed without an action.
  The hook also receives PowerKit's own foreground changes, so returning from
  an elevated window to a PowerKit window clears the flag.
- Stop paths: disable, binding reset, session lock, suspend, session end and
  PowerKit exit all stop the hook thread (`WM_QUIT`, join), cancel pending
  holds and timers, and have the injector release every mirrored button it
  still holds down. Exit runs this from `WM_DESTROY` before the WinUI window
  closes. Unlock and resume restart the hook only when PowerMouse is enabled.
- Session lock and user switching arrive as `WM_WTSSESSION_CHANGE`
  (`WTSRegisterSessionNotification` on the app window). A lock and a
  disconnect are tracked apart, because fast user switching sends both, and
  PowerMouse runs again only when the session is unlocked and connected.
  Stopping also stops the device adapters, which give the Logitech buttons
  back, so another user on the same computer gets a working mouse.
- PowerMouse starts when PowerKit starts: binding the app window
  (`power_mouse::listen`) reconciles, because startup applies the
  preferences before the window exists.
- After a crash or hang, Windows Error Reporting starts PowerKit again
  (`recovery.rs`, `RegisterApplicationRestart`). This covers Rust panics too:
  a panic in a window procedure ends in a fail-fast abort, which Windows
  also restarts. The new process carries `--restarted` and waits up to 30 s
  for the crashed one to release the single-instance name. Windows restarts
  only a process that ran for a minute, so a crash at startup does not loop,
  and nothing restarts when error reporting is off by policy.
- PowerMouse never runs elevated, never installs a driver and does not stop
  or restart an AlwaysOn session.

## Persistence

Settings through the existing settings writer (`settings.txt`):

| Key | Meaning |
| --- | --- |
| `mouse_enabled` | `true` / `false`, default `false` |
| `mouse_dpi` | 100–32000; absent leaves the device's DPI alone |
| `mouse_wheel_mode` | `ratchet`, `free_spin` or `smart_shift`; absent leaves the device's mode alone |
| `mouse_hires_wheel` | `true` / `false`; absent leaves the device's wheel resolution alone |
| `mouse_<trigger>` | Binding for `middle`, `back`, `forward`, `tilt_left`, `tilt_right`, `f13`…`f24`, and the device triggers `gesture_button`, `mode_shift`, `haptic_panel`, `thumb_left`, `thumb_right`, and the ring slots `ring_1`…`ring_8`; absent means pass-through (an empty slot) |

Binding grammar (each value at most 1024 bytes):

```text
binding  = "do:" action
         | "hold:" action
         | "long:" action "|" action
         | "gesture:" action "|" action "|" action "|" action "|" action   ; click|up|down|left|right
action   = simple
         | "flow:" [ step *( ">" step ) ]   ; at most 8 steps
step     = simple
         | "wait:" ms              ; 1–5000
simple   = token                   ; for example copy, task_view, volume_up, none
         | "keys:" mods "," vk     ; same form as tool hotkeys
         | "text:" encoded         ; at most 120 characters
```

- `encoded` keeps ASCII letters, digits, `-`, `_` and `.` and writes every
  other UTF-8 byte as `%XX`, so a text can never contain `|`, `>` or a line
  break.
- A gesture slot may be `none`. Horizontal-scroll triggers accept only `do:`;
  F-key triggers accept `do:`, `hold:` and `long:`.
- A binding that cannot be written within the limits is refused when it is
  set, so nothing is accepted that the next save would drop.
- An unknown trigger, action token or malformed binding makes the file
  invalid under the existing policy: it is preserved until an explicit user
  change replaces it.
- Not persisted: holds in progress, mirrored buttons, the elevated-foreground
  flag.

## Module layout

- `crates/core/src/power_mouse.rs`: pure triggers, actions, bindings and their grammar,
  the press state machine (press, release, move, timer, cancel → suppress
  decision plus outputs), swipe accumulator and tilt burst detection.
  Unit-tested without Windows.
- `crates/core/src/power_mouse/device.rs`: pure device model for adapters: HID
  collections, grouping, HID++ classification and the verified-model table.
- `crates/platform/windows/src/power_mouse/`: `mod.rs` (`Session` on the UI thread),
  `hook.rs` (hook thread and message loop), `inject.rs` (injector thread and
  `INPUT` batches), `elevation.rs` (foreground integrity flag),
  `discovery.rs` (read-only HID scan), `devices.rs` (discovery thread and
  the published device list), `hidpp_channel.rs` (HID++ report I/O, the
  `HidppChannel` port), `adapter.rs` (one thread per device: waiting,
  retry, published status), `conflict.rs` (other applications that drive the same
  controls), `ring_window.rs` (the Actions Ring popup) and `probes.rs`
  (ignored tests that query the connected mouse, run by hand).
- `crates/app/src/controller/power_mouse/`: the HID++ conversation above the channel,
  with no Windows dependency: `logitech.rs` (identity, battery and control
  queries), `device_settings.rs` (DPI, wheel, thumb wheel, Easy-Switch and
  haptic requests) and `link.rs` (one connected device: diversion, events,
  status). Tested against a fake mouse.
- `crates/core/src/power_mouse/ring.rs`: pure Actions Ring geometry and rasterizer.
- `crates/core/src/power_mouse/hidpp.rs`: pure HID++ 2.0 framing, reply matching and
  feature decoding.
- `crates/ui/reactor/src/window/mouse_page.rs`: the page.
- `crates/core/src/locale/power_mouse.rs`: page and action picker strings in
  Turkish, English and Arabic.
- No crate was added for PowerMouse: HID discovery, overlapped report I/O and
  the process list come from `windows-sys` features.

## Testing

- Unit tests with synthetic event sequences: single, mirrored, long-press
  before and after the threshold, gesture click and each direction, volume
  repeat and reversal, lost release, stale hold, second trigger during a hold,
  release without a suppressed press, horizontal-scroll bursts including a
  continuous scroll, gesture travel while the cursor is frozen, F-key repeat,
  cancel while mirrored buttons are down, DPI threshold scaling, hold-down
  bindings, text and workflow grammar with their limits, binding
  grammar round trips and rejection of malformed values. Lost release and
  stale hold are not covered yet.
- Windows integration: partial-batch cleanup on `INPUT` construction,
  release bookkeeping for mirrored buttons, newest-snapshot selection,
  workflow scheduling around pauses and Unicode `INPUT` construction. A
  smoke check that starts and stops the hook thread with isolated preferences
  is not written yet.
  Real hook behavior cannot be driven by `SendInput`, because injected events
  are ignored by design.
- Physical (record native ARM64, emulated x64 and physical evidence
  separately): a five-button USB mouse, a Bluetooth mouse, a mouse with wheel
  tilt or a thumb wheel, a device that sends F13–F24, precision-touchpad middle click and
  horizontal scrolling, an elevated window in the foreground, lock/unlock,
  sleep/resume, fast user switching and exit while a mirrored button is held.

## Verification status

Evidence is from one machine: Windows 11 on native ARM64 with a Logitech MX
Master 4 over Bluetooth (product id `B042`), with Logi Options+, OpenLogi and
MX4Lite not running unless a line says otherwise. Later changes are covered
by the automated tests only unless they were tried by hand again; treat this
list as what has been observed, not as a guarantee for the current revision.

Observed with the physical mouse, by the hand-run probes in
`crates/platform/windows/src/power_mouse/probes.rs`:

- HID collections `0001/0002`, `FF43/0202` (HID++ long report only) and
  `000E/0001`, with an empty product string.
- The device answers at index `0xFF`, reports the name "MX Master 4", type
  mouse and its battery level, and lists nine controls (`0050`, `0051`,
  `0052`, `0053`, `0056`, `00C3`, `00C4`, `01A0`, `00D7`; all but left and
  right divertable with raw XY).
- Diverting the gesture button with raw XY reads back as diverted + raw XY,
  and undiverting reads back as neither.
- All six setting features are present: adjustable DPI 200–8000 in steps of
  50, SmartShift enhanced (default release speed 10), high-resolution wheel,
  thumb wheel, change host (three hosts) and haptic feedback. Extended DPI
  and plain SmartShift are absent. Writing the current DPI, wheel mode, wheel
  resolution and thumb-wheel mode back is accepted and reads back unchanged.

Accepted by hand on a packaged build:

- MX Master 4: device, connection and battery on the page; gesture button
  with hold-and-swipe; haptic panel and mode shift button with an action; a
  DPI change and back; the thumb wheel with an action, direction included;
  haptic tap.
- Actions Ring: opening from a bound button, the hover highlight, a click
  running the slot's action in the application underneath, and closing by
  the centre, Esc and a right click.
- Generic layer: hold down, a single action mirroring a mouse button,
  hold-and-swipe on Middle, long press (short and long), typed text with
  Turkish characters, a workflow with a pause, a recorded shortcut,
  horizontal scroll once per burst and the tray switch.
- Lifecycle: an elevated window in front, lock and unlock (also with a
  mirrored button held), sleep and resume, the mouse switched off and on,
  and switching to another user and back.
- With MX4Lite running, the page names it and leaves the device buttons off.
- Crash restart with a debug build: an access violation and a UI-thread
  panic were each followed by a replacement process that took over.

Not verified:

- F13–F24 by hand (no device that sends them was available).
- PowerKit exit while a mirrored button is held.
- Easy-Switch, the low-battery notice, and the device buttons returning to
  the mouse once a rival application stops. Of the rival process names,
  only `mx4-lite.exe` was read from a running installation;
  `logioptionsplus_agent.exe` and `openlogi-agent.exe` were not.
- Whether a Bluetooth mouse leaving and rejoining produces `WM_DEVICECHANGE`
  promptly.
- Crash restart of the packaged build.
- The light theme, the Arabic (right-to-left) layout, x64 builds, a Bolt or
  Unifying receiver, a precision touchpad and any mouse other than MX
  Master 4.

## Product decisions

- Name: PowerMouse.
- The generic layer uses standard Windows mouse input only and works with
  every mouse. It offers single actions, hold-down, long press,
  hold-and-swipe gestures, custom keyboard shortcuts and F13–F24 triggers.
- Device adapters add vendor features for selected mice, Logitech HID++
  first.
- Mice only; no keyboards, lights or webcams.
- Per-application profiles are not wanted.
- Left and Right button remapping and wheel rotation remapping stay excluded.
- With a rival application running, the adapter stays off and the page warns.

## Open questions

- Whether precision-touchpad horizontal scrolling and three-finger middle
  click reach `WH_MOUSE_LL` as ordinary `WM_MOUSEHWHEEL` / `WM_MBUTTON*`. If
  they do, horizontal-scroll remapping (and middle remapping) would also
  affect the touchpad; it may need to ship behind a warning.
- Whether freezing the cursor during a gesture hold is preferred over letting
  it move, as OpenLogi's OS-hook gestures do. Diverted adapter buttons report
  raw movement, so the question applies to the generic layer only.
- Elevated windows and diverted buttons: resolved, see
  [Logitech adapter](#logitech-adapter). OpenLogi's source (installer
  definition not read) runs at normal integrity with no `uiAccess`
  manifest and no elevated-window handling, so its diverted buttons do
  nothing there. UIAccess remains a separate product decision, as for
  AlwaysTop; its acceptance in a Store MSIX is unverified.
- A mouse behind a Bolt or Unifying receiver: the device index is its
  pairing slot and the receiver announces arrivals itself. This needs the
  receiver for verification.
- Whether full-screen games or anti-cheat software react to a global
  low-level hook; a "pause in full-screen apps" option may be needed later.

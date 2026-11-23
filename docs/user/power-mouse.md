# PowerMouse

[User guide](README.md) · [Türkçe](tr/power-mouse.md)

PowerMouse gives the extra buttons of your mouse new jobs: a keyboard
shortcut, a media key, a piece of text, or a ring of actions around the
cursor. It works with every mouse Windows recognizes and needs no software
from the mouse's maker. For a Logitech MX Master 4 connected over Bluetooth it
also uses the mouse's special buttons and settings.

PowerMouse is off at first. Turn it on with the switch at the top of the
Power Mouse page, from Home, or in the tray menu.

## What can be remapped

| Button or key | Notes |
| --- | --- |
| Middle button, Back, Forward | Every binding type |
| Horizontal scroll left and right | From a tilting wheel or a thumb wheel. One action only; it runs once per uninterrupted scroll and replaces horizontal scrolling |
| F13 to F24 | Keys that some mice and macro pads send for extra buttons. Choose **Add extra key…** and press the key first. Single action, Hold down or Long press. They are remapped whichever device sends them |

The left and right buttons and the turning of the wheel cannot be remapped, so
a wrong setting can never leave you without a working mouse.

## Give a button a job

1. On the **Buttons** tab, click a button in the picture or in the list.
2. On the right, choose a **Binding type**.
3. Choose the action for each part of the binding.

Changes apply at once.

| Binding type | Behavior |
| --- | --- |
| Pass through | The button does what it normally does. This is the default |
| Single action | The action runs when you press the button |
| Hold down | The shortcut or button stays pressed for as long as you hold the mouse button, for example for push-to-talk |
| Long press | One action for a short press, another when you hold the button for half a second |
| Hold and swipe | Hold the button and move the mouse up, down, left or right; each direction has its own action, and a press without movement has a fifth. The cursor stays still while you hold the button |

## Actions

| Group | Actions |
| --- | --- |
| Mouse | Left, right and middle click, mouse back, mouse forward |
| Navigation | Browser back and forward, Task View, previous and next virtual desktop, Show desktop |
| Editing | Copy, Paste, Cut, Undo, Redo, Select all, Find, Save |
| Tabs | New tab, Close tab, Reopen tab, Next tab, Previous tab, Reload |
| Media | Play/pause, Next track, Previous track, Volume up, Volume down, Mute |
| System | Capture region, Lock, Do nothing |
| PowerKit | Pin the last window with AlwaysTop, Start ScreenRuler, Open PowerKit, Show Actions Ring |
| Record shortcut… | Any keyboard shortcut you press |
| Type text… | Types up to 120 characters. Enter the text and choose **Apply** |
| Workflow… | Up to 8 steps that run in order. A step is any action above or a wait of at most 5 seconds |

**Do nothing** disables a button.

## Actions Ring

The Actions Ring is a ring of eight slots that opens around the cursor. Click
a slot to run its action.

1. On the **Actions Ring** tab, click a slot and choose its action. A slot
   takes any action from the list above.
2. On the **Buttons** tab, give a button the action **Show Actions Ring**.

The ring closes when you click its centre or outside it, press Esc, or wait 10
seconds. Pressing the button again closes it too.

## MX Master 4

When an MX Master 4 is connected over Bluetooth, the page shows its name,
connection and battery level, and offers more:

| Feature | What you get |
| --- | --- |
| Gesture button, Mode shift button, Haptic panel | Remappable with every binding type. A button without a binding keeps its own function |
| Thumb wheel | Each direction can run an action. The thumb wheel then stops scrolling sideways |
| Sensor resolution (DPI) | Presets between 400 and 4000 |
| Wheel mode | Ratchet, Free spin or SmartShift, which changes to free spin on a fast flick |
| High-resolution scrolling | Smoother scrolling in applications that support it |
| Extra actions | Easy-Switch 1 to 3 moves the mouse to another paired computer, Haptic tap, Next DPI |

- Device settings are on the **Scrolling and pointer** tab. **Keep the
  device's setting** leaves a setting as the mouse has it.
- PowerKit shows a notification when the battery falls to 15%.
- While Logi Options+, OpenLogi or MX4Lite is running, PowerMouse leaves the
  device buttons to that application and says so on the page. The standard
  buttons are still remapped.
- Other Logitech mice, and mice connected through a Bolt or Unifying
  receiver, are listed but their device features stay off. They work as
  ordinary mice with everything described above the MX Master 4 section.

## Settings Windows already has

Scroll direction, pointer speed, the primary button and the number of lines
per wheel notch are in Windows Settings under Bluetooth & devices > Mouse. The
**Scrolling and pointer** tab has a link to that page. PowerMouse does not
duplicate them.

## Good to know

- While a window of an application that runs as administrator is in front,
  PowerMouse does not remap anything, and every button does what it normally
  does. The page shows "Paused for elevated window".
- An action is skipped when you are holding Ctrl, Shift, Alt or the Windows
  key at that moment, because the keys would combine into a different
  shortcut.
- PowerMouse stops while the computer is locked or asleep and continues
  afterwards.
- With **Hold and swipe** on the middle button, dragging with the middle
  button no longer pans in applications that use it for that.
- **Reset to defaults** returns every button to **Pass through** and stops
  managing the device settings. Settings already written to the mouse stay on
  the mouse.
- PowerMouse has no per-application profiles: a button does the same thing in
  every application.
- What has been tried by hand: an MX Master 4 over Bluetooth on Windows 11
  ARM64. Other mice use the same standard Windows input, but have not been
  tried, and neither have F13 to F24 keys or Easy-Switch.

# ScreenRuler

[User guide](README.md) · [Türkçe](tr/screen-ruler.md)

ScreenRuler measures distances on the screen in pixels: the size of a
rectangle you draw, or the gap between two edges that it finds for you.

## Measure

1. Press Win+Ctrl+Shift+M, or choose **Screen Ruler** in the tray menu, or
   **Start measuring** on the Screen Ruler page. A small toolbar opens at the
   top of the main display.
2. Choose a tool on the toolbar.
3. Click to copy the measurement to the clipboard.
4. Press Esc or the toolbar's close button to finish.

| Tool | Shortcut | Measures |
| --- | --- | --- |
| Bounds | Ctrl+1 | The rectangle you drag |
| Spacing | Ctrl+2 | The distance to the nearest edges around the pointer, in both directions |
| Horizontal spacing | Ctrl+3 | The same, left and right only |
| Vertical spacing | Ctrl+4 | The same, up and down only |

- Choosing the active tool again clears it.
- Hold Shift while clicking to leave the measurement on the screen and take
  another one.
- The mouse wheel changes the edge tolerance by 15 until you close the
  toolbar. Raise it when the lines stop too early, on noise or a gradient;
  lower it when they run past a faint edge.
- Drag the dotted grip at the left end of the toolbar to move it. It opens in
  the same place until PowerKit exits.

## Settings

| Setting | What it does |
| --- | --- |
| Measurement shortcut | Default Win+Ctrl+Shift+M |
| Default mode | The tool that is selected when the toolbar opens. Default None |
| Capture the screen continuously while measuring | Follows changing content, such as a video. Off by default; the screen is captured once when measuring starts |
| Per-channel edge detection | Compares red, green and blue separately instead of their total. Off by default |
| Pixel tolerance | How different two pixels must be to count as an edge. Default 30 |
| Unit | Shows inches, centimetres, millimetres or DIP beside the pixel value. Default pixels only |
| Draw feet on cross lines | Draws small end marks on the measuring lines. On by default |
| Line color | Default `#FF4500` |

## Good to know

- A measurement is not saved; the clipboard copy is the result.

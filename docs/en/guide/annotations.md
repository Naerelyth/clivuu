# Annotations & Editing

[User Guide](../README.md) | **English** | [简体中文](../../zh-CN/guide/annotations.md)

Clivuu uses the exact same vector/bitmap annotation engine during interactive [Screenshot Capture](capture.md) and toolbar editing on [Floating Pins](pins.md). During screenshot capture, you can also draw outside the selection box; expanding the selection later reveals those marks, and exporting crops strictly to the final selection area.

## Main Toolbar & Tool Shortcuts

The main toolbar runs at a fixed `32 px` height with a draggable left grip (`Move toolbar`), functional separators, and content-sized `32 px` subbars that expand immediately on tool selection (both the main toolbar and active subbar stay visible while drawing annotations, mosaicking, erasing, or transforming marks, hiding only during capture region selection/move/resize gestures or when toggled with `Space`):

| Tool | Shortcut | Options in Subbar | Behavior Notes |
| :--- | :--- | :--- | :--- |
| **Cursor** | `Ctrl+1` | None | Re-select and move/edit **Text** and **Sequence** marks; drag pins directly or move/expand capture selections |
| **Shapes** | `Ctrl+2` | Width (`1–16`), Fill toggle, Rectangle / Ellipse toggle, Color | Hold `Shift` to constrain squares and circles; movable, resizable, and rotatable while active before deselection, locked once finalized |
| **Arrow** | `Ctrl+3` | Width (`1–16`), 11 arrow styles, 5 line styles, Color | Drag two-point arrows or click-to-draw multi-segment polylines (finish with Right-click, Double-click, or `Enter`; `Esc` cancels); `Shift` snaps to 45° increments, vertices stay adjustable while active and lock once deselected |
| **Pen** | `Ctrl+4` | Width (`1–50`), Color | Freehand vector stroke; committed strokes keep their original width and stay fixed in place |
| **Highlight** | `Ctrl+5` | Width (`4–50`), Freehand / Straight line / Rectangle / Ellipse toggle, Color | Translucent highlighter (default opacity ~32%); hold `Shift` for 45° lines or square/circle regions; freehand strokes stay fixed immediately, geometric highlights lock once deselected |
| **Sequence** | `Ctrl+6` | Next-index setter/stepper (`1–9999`), 5-shape selector, Format selector, Size (`1–50`), Number & Fill colors | Click canvas to stamp numbered step badges with optional arrow leader; remains reselectable and movable at any time |
| **Mosaic** | `Ctrl+7` | Brush width (`4–50`), Freehand / Rectangle toggle, Effect toggle (`Pixelate` / `Blur`), Strength (`3–24`, default `8`) | Conceals sensitive regions; freehand strokes stay fixed immediately, rectangular mosaic is adjustable while active and locks once deselected |
| **Text** | `Ctrl+8` | Font family selector, Size (`1–72`), Bold, Italic, Outline (`None` / `Auto` / `White` / `Black` / `Custom`, width `0–8`, default `1`), Text & Outline colors | Click canvas to type inline directly on the image; remains reselectable, editable, movable, and rotatable at any time |
| **Eraser** | `E` | Brush width (`4–50`), Freehand / Area toggle | Erases annotations while preserving underlying image pixels; freehand strokes stay fixed immediately, rectangular erasers lock once deselected |

> **Finalized annotation locking**: Freehand strokes stay fixed immediately upon release. Non-freehand shapes, arrows, geometric highlights, rectangular mosaics, and rectangular erasers remain movable, resizable, and restyleable while active right after drawing, then lock permanently once deselected (by clicking blank canvas, starting the next mark, switching tools, or right-clicking after finishing any active polyline). Only **Text** and **Sequence** marks can be re-selected, moved, or edited afterward, including with **Cursor** (`Ctrl+1`).

Additional toolbar actions:
- **History & keyboard edits**: `Undo` (`Ctrl+Z`), `Redo` (`Ctrl+Y`), Clear all annotations (`Ctrl+Shift+Z`, irreversible), Delete selected mark (`Delete` / `Backspace`), and adjust brush width, text size, or rectangular mosaic strength (`1` / `[` decrease, `2` / `]` increase).
- **Pin-only geometry tools** (shown when editing a pin): `Crop` (`1px` accent border with circular outer anchors, supporting immediate full-frame edge/corner resize, sub-crop selection, interior move, and 8-way resize while preserving the cropped region's framed desktop position on `Apply changes`), `Rotate right` (`1`), `Rotate left` (`2`), `Flip horizontally` (`3`), `Flip vertically` (`4`). (Selecting another annotation tool exits an unapplied crop.)
- **Utilities**: `Recognize text` (`O`; `Shift+O` quick-copies in capture), `Scrolling capture` (`L`, capture only), `Region recording` (`V`, capture only).
- **Finish**: `Cancel` (`Esc` in capture; on pins, reverts unapplied edits from the current toolbar session and closes the toolbar), `Pin` (`Ctrl+T`, capture) / `Apply changes` (`Enter`/`Esc`/`Space`, pin), `Save image` (`Ctrl+S`), `Copy image` (`Enter` in capture / `Ctrl+C`).

## Arrow Styles & Line Styles

The **Arrow** subbar separates shaft/head geometry from stroke dash style (11 arrow types x 5 line styles):

- **Arrow types**: `Straight`, `StraightBilateral`, `Line`, `LineBilateral`, `Outline`, `Filled`, `DimensionLine`, `Solid`, `SolidBilateral`, `DimensionArrow`, `PlainLine`.
- **Line styles**: `Solid`, `Dash`, `Dot`, `DashDot`, `DashDotDot` (`Filled` draws a solid silhouette without line-dash strokes).

## Sequence Badges

**Sequence** (`Ctrl+6`) stamps numbered step callouts and optional callout leaders onto your image:

- **Formats**: `Decimal` (`1, 2, 3`), `Roman` (`I, II, III`), `LowerAlpha` (`a, b, c`), `UpperAlpha` (`A, B, C`), `Chinese` (`一, 二, 三`).
- **Shapes**: click the badge shape selector in the subbar to choose among 5 badge frames (`FilledCircle`, `BorderedCircle`, `OutlineCircle`, `Text`, `RoundedSquare`). Only `BorderedCircle` exposes an independent number color.
- **Counter control**: press `Z` to increment or `X` to decrement the next badge number without altering badges already placed, or click the subbar next-number button to set or reset the counter. Placing and undoing/redoing badges tracks the counter accurately.
- **Editing a placed badge**: pull out a leader immediately while dragging on placement, or select a placed badge and hover it to show inline `−` / `+` index adjustment, delete, and leader toggle controls. Drag its body to reposition, drag its leader endpoint outward to adjust the notched arrow / short-distance triangle leader (hold `Shift` to snap direction), or drag the tip back inside the badge to hide the leader.

## Mosaic & Blur Controls

- **Geometry modes**: freehand brush stroke or rectangular area drag (geometry cannot be converted after placement; freehand strokes stay fixed in place, while rectangular regions remain movable and resizable until deselected).
- **Effect modes & strength**: click the effect button in the subbar to toggle between `Pixelate` and `Blur`. Both modes share a unified strength range (`3–24`, default `8`) that is preserved when switching between them.
- **Wheel behavior**:
  - In **Freehand** mosaic mode, scrolling the wheel on the canvas adjusts brush width while the strength slider sets pixel/blur intensity.
  - In **Rectangle** mosaic mode, scrolling the wheel on the canvas adjusts intensity by `1` per step and updates the strength slider live.
  - Hovering the **Effect strength** slider in the subbar and scrolling the mouse wheel always steps intensity by `1` with coalesced single-step undo.
- Committed freehand strokes (`Pen`, freehand `Highlight`, freehand `Mosaic`, freehand `Eraser`) retain their original width and stay fixed on the canvas.

## Inline Text Editing & IME Support

Text edits happen directly on the canvas:

- Click anywhere on the canvas with `Text` (`Ctrl+8`) selected to place an insertion cursor. Re-edit an existing text mark by clicking it with `Text` or double-clicking it with `Cursor` (`Ctrl+1`), drag to move, drag corner handles or scroll the wheel inside or outside the frame to resize (`1–72`), drag the spiral handle to rotate (`Shift` snaps in `15°` steps), or click the top-right delete badge to remove it. While typing inline, `Ctrl+Z`, `Ctrl+Y`, and `Ctrl+Shift+Z` undo/redo text inside the box, and `Esc` commits it (when no IME preedit is active).
- Type font names in the Font selector panel to search fonts; when an IME preedit string is uncommitted, pressing `Esc` clears the uncommitted preedit first, and pressing `Esc` again closes the font panel without leaving capture.
- When typing inside any text box, search field, numeric field, or active IBus/Fcitx IME preedit session, normal typing keys (`1–9`, `E`, `Z`, `X`, etc.) go strictly to text input rather than triggering tool switches.

## Colors & Eyedropper Popover

Every tool that takes color (`Shapes`, `Arrow`, `Pen`, `Highlight`, `Sequence`, `Text`) shows `20` inline quick swatches beside a square current-color launcher button in its subbar:

- Clicking an inline quick swatch applies that preset immediately.
- Clicking the square launcher opens the full color panel with `48` basic colors, `16` persistent custom color slots (`Add to custom colors`), a `Pick screen color` eyedropper, an HSV plane with Value and Opacity sliders (when main opacity is supported), numeric `H`/`S`/`V`/`R`/`G`/`B`/`A` and `#RRGGBB`/`#RRGGBBAA` HTML fields with a copy button, and `Cancel`/`OK`.
- Color changes apply to future marks and immediately update any currently selected annotation.

## Related Manuals

- [Screenshot Capture](capture.md)
- [Floating Pins](pins.md)
- [Shortcuts & Mouse Gestures](shortcuts.md)

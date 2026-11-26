# Shortcuts & Mouse Gestures

[User Guide](../README.md) | **English** | [简体中文](../../zh-CN/guide/shortcuts.md)

Clivuu handles interactive shortcuts within active overlays, pins, and editors, while global hotkeys (starting capture, pinning, scrolling, recording) are configured through your desktop environment or window manager by binding shortcuts to `clivuu` CLI commands.

## Global Command Bindings

See [Getting Started](getting-started.md#desktop-integration--hotkeys) and [CLI Reference](../development/cli.md) for commands to bind globally in your desktop settings.

## Annotation Tools & Shared Editing

These shortcuts apply inside both interactive screenshot capture and pin toolbar editing (focused text fields, search entries, popups, and active IME preedit retain native typing priority):

| Action | Shortcut | Notes |
| :--- | :--- | :--- |
| Cursor | `Ctrl+1` | Re-select and move/edit Text and Sequence marks; drag pins or move/extend capture selection |
| Shapes (`Rect` / `Ellipse`) | `Ctrl+2` | Hold `Shift` while drawing to constrain squares and circles (locks once deselected) |
| Arrow | `Ctrl+3` | Drag two-point arrows or click-to-draw multi-segment polylines (Right-click/Double-click/`Enter` finishes, `Esc` cancels); `Shift` snaps to 45° increments |
| Pen | `Ctrl+4` | Freehand stroke (stays fixed in place once committed) |
| Highlight | `Ctrl+5` | Freehand, straight rounded line, rectangle, or ellipse (`Shift` constrains 45° lines or square/circle regions) |
| Sequence | `Ctrl+6` | `Z` / `X` increment/decrement next badge index |
| Mosaic | `Ctrl+7` | Click effect icon to toggle pixelate/blur; wheel changes strength by `1` (Rectangle / hovered slider) or width (Freehand) |
| Text | `Ctrl+8` | Direct canvas input; `Esc` clears uncommitted IME preedit first, then closes open font panel |
| **Eraser** | `E` | Freehand or rectangular area annotation eraser |
| Delete selected mark | `Delete` / `Backspace` | Removes the active selected annotation |
| Step brush / text size / mosaic strength | `Wheel` / `1` / `2` / `[` / `]` | Decreases (`1` / `[`) or increases (`2` / `]`) stroke width, text size, or rectangular mosaic strength |
| Finish polyline / deselect mark | `Right-click` | Finishes an active polyline arrow or deselects the active annotation |
| Constrain geometry | `Shift` | Constrains shapes/highlights to squares/circles, arrows/lines/leaders to `45°`, rotation to `15°`, and resize proportions |
| Undo / Redo | `Ctrl+Z` / `Ctrl+Y` | Stepwise canvas history (inside inline text, applies to text edits) |
| Clear all annotations | `Ctrl+Shift+Z` | Irreversibly removes all annotations on current image (inside inline text, deletes/handles text edits) |
| Save / Quick save | `Ctrl+S` / `Ctrl+Shift+S` | Open Save dialog or write immediately to quick folder |
| Copy image | `Ctrl+C` | Copies annotated image to clipboard (`Enter` also finishes capture) |
| Show / hide toolbar | `Space` | Toggles toolbar visibility (also applies edits on pins) |

## Screenshot Selection & History

| Action | Shortcut | Notes |
| :--- | :--- | :--- |
| Cycle window / screen targets | `Mouse wheel` | Before drawing a selection |
| Toggle screen / full desktop | `Ctrl+A` | Expands or collapses between active screen and combined monitors |
| Restore last selection | `R` | Restores previous capture coordinates |
| Browse region history | `Page Up` / `Page Down` | Steps backward / forward through previous regions |
| Browse capture frames | `,` / `.` | Steps through recent in-memory captures matching current monitor layout |
| Show / hide captured cursor | `` ` `` / `!` | Toggles captured pointer visibility during capture when `capture-cursor=true` (GNOME Wayland and X11) |
| Refresh frozen backdrop | `F5` | Re-acquires static desktop frame keeping selection and edits |
| Move selection by `1` source px | `Arrow keys` | Moves selection box when Cursor mode is active |
| Expand / shrink edge by `1` source px | `Ctrl+Arrow` / `Shift+Arrow` | Expands (`Ctrl`) or contracts (`Shift`) the matching edge |
| Pin capture | `Ctrl+T` | Sends selection to floating desktop pin |
| Recognize text (OCR) | `O` | Toggles in-place text selection on the capture (`Ctrl+A` selects all, `Ctrl+C` copies, `Enter` copies and closes, `Esc` clears selection then exits, double/triple-click selects line/block; quick-copies if `ocr-quick-copy=true`) |
| Quick copy text (OCR) | `Shift+O` | Recognizes selection text, copies immediately, and closes |
| Switch to scrolling capture | `L` | Hands selected rectangle to scrolling capture |
| Switch to region recording | `V` | Hands selected rectangle to screen recording |
| Reset selection / cancel capture | `Right-click` | Finishes polyline / deselects mark / exits OCR, then clears selection; cancels capture when no selection exists |
| Cancel capture | `Esc` | Cancels capture directly (when no popup, inline text, polyline, or OCR mode is active) |

## Floating Pins

Available when a pin has keyboard focus (always on native Wayland/X11, and on GNOME when `pin-gnome-focus=true`; disabled across the entire pin lifecycle if `pin-gnome-focus=false`):

| Action | Shortcut | Notes |
| :--- | :--- | :--- |
| Move pin by `1 px` | `Arrow keys` | Nudge pin position in logical coordinates when no drawing tool is active |
| Rotate `90°` right / left | `1` / `2` | Applies display rotation when no drawing tool is active |
| Flip horizontally / vertically | `3` / `4` | Applies display mirror when no drawing tool is active |
| Toggle grayscale / color inversion | `5` / `6` | Reversible visual and export filters when no drawing tool is active |
| Scale / opacity via keyboard | `+` / `=` / `-` | Shares modifiers configured for wheel zoom/opacity (also numpad `+` / `-`, when enabled) |
| Select all visible pins in group | `Ctrl+A` | Transforms, copies, saves, or closes all active group pins together |
| Replace pin from clipboard | `Ctrl+V` | Replaces content keeping position |
| Copy original text | `Ctrl+Shift+C` | Copies unrasterized source text from text/HTML/color pins |
| Recognize text (OCR) | `O` | Toggles in-place OCR when no drawing tool is active (`Ctrl+C` copy, `Ctrl+A` select all, `Esc` clear/exit, double/triple-click line/block, `Alt`-drag moves pin over text) |
| Open Preferences | `Ctrl+Shift+P` | Opens Clivuu settings |
| Apply toolbar changes | `Enter` / `Esc` / `Space` | Applies edits and hides the pin toolbar (click toolbar `Cancel` to revert unapplied edits) |
| Close pin | `Ctrl+W` / `Esc` (toolbar closed) | Moves pin to recoverable closed list |
| Destroy pin permanently | `Shift+Esc` | Removes pin without recovery |

## Magnifier & Color Sampling

| Action | Shortcut | Notes |
| :--- | :--- | :--- |
| Show / hide magnifier & guides | `Alt` | Holds magnifier visible during capture or on focused pins; toggles crosshair before selection or alignment guides after selection in capture |
| Toggle RGB / HEX display | `Shift` | While magnifier is visible |
| Copy sampled pixel color | `C` | Copies pixel under crosshair (`Shift` selects format) |

## Scrolling Capture, Recording & Playback

- **Scrolling capture**: `Enter` (Pin image), `Ctrl+C` (Copy), `Ctrl+S` / `Ctrl+Shift+S` (Save / Quick save), `Ctrl+R` (Restart in current region), `Esc` (Cancel).
- **Region recording**: `Enter` (Start recording in ready state), `Esc` (Discard in ready state, or open discard confirmation while recording/paused).
- **Recording playback**: `Space` (Play / Pause), `Ctrl+C` (Copy exported file), `Ctrl+S` / `Ctrl+Shift+S` (Save / Quick save), `Esc` (Open close confirmation popup).

## Configurable Pin Mouse Gestures

Under `Preferences -> Control -> Mouse`, you can configure pin mouse and wheel gestures:

| Setting Key | Default | Action |
| :--- | :--- | :--- |
| `pin-mouse-zoom` | `wheel` | Scale pin size (`wheel` or `none`, plus up to 2 modifiers) |
| `pin-mouse-opacity` | `ctrl+wheel` | Adjust pin opacity |
| `pin-mouse-close` | `double-left` | Close pin (`double-left`, `middle`, `right`, or `none`) |
| `pin-mouse-reset` | `middle` | Reset pin size and opacity |
| `pin-mouse-thumbnail` | `shift+double-left` | Toggle thumbnail / full size |
| `pin-mouse-copy-text` | `shift+right` | Copy source text |
| `pin-mouse-destroy` | `ctrl+double-left` | Destroy pin permanently |
| `pin-mouse-zoom-keys` / `pin-mouse-opacity-keys` | `true` / `true` | Allow `+`, `=`, `-` (and numpad `+`/`-`) keys with the same modifier bindings |

Up to two modifiers (`ctrl`, `shift`, `alt`, `super`) are allowed per gesture. Bindings match modifiers strictly, with earlier rows taking precedence, and `Reset settings` restores only `Control -> Mouse` defaults after confirmation.

## Related Manuals

- [Preferences](preferences.md)
- [Floating Pins](pins.md)
- [Configuration Schema](../development/configuration.md)

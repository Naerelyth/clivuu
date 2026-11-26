# Screenshot Capture

[User Guide](../README.md) | **English** | [简体中文](../../zh-CN/guide/capture.md)

Standard screenshot capture grabs a desktop frame, freezes it inside Clivuu's full-desktop overlay, and lets you select, inspect, annotate, pin, recognize text, record, save, or copy a region without changing underlying windows while you work.

## Entry Points

- **Tray**: click `Capture`, `Window capture`, or `Pin recovery & cleanup -> Pin most recent capture`.
- **CLI**:
  - `clivuu snip` (interactive selection)
  - `clivuu snip --window` or `--active-window` (single-frame window capture)
  - `clivuu snip --active-screen` / `--full` / `--last` / `--area X Y W H` / `--size W H`
  - `clivuu snip --delay SECONDS` (delay up to `3600` seconds before acquiring the frame)
  - Optional `-o` (`--output`) destination (`clipboard`, `pin`, `quick-save`, `file-dialog`, `ocr_clipboard`, `printer`, `barcode`, `no-auto-save`, `success`, `silent`, or `FILE.png`/`FILE.jpg`/`FILE.jpeg`) skips interactive editing when paired with non-interactive range flags or applies directly upon finishing capture.

## Selection & Navigation

Once the overlay opens:

- **Freehand region**: click and drag across any screen area.
- **Expand existing selection**: in Cursor mode (`Ctrl+1`), clicking outside an existing selection box extends the closest edges to the pointer.
- **Scroll wheel window/screen cycle**: scrolling the wheel before dragging cycles through candidate window rects and monitor boundaries under the pointer (window detection queries `hyprctl` on Hyprland and `niri msg` on niri; other compositors present monitor boundaries unless started via `--window`).
- **Screen / desktop toggle (`Ctrl+A`)**: toggles selection between the active monitor and the combined full desktop bounding box.
- **Last region (`R`) & region history (`Page Up` / `Page Down`)**: `R` restores the most recent capture rectangle; `Page Up` and `Page Down` cycle through saved region rectangles.
- **Nudge & resize by source pixel**:
  - Unmodified arrow keys move the selection box by `1` source pixel when no drawing tool is active (Cursor mode).
  - `Ctrl + Arrow` expands the edge in that arrow's direction outward by `1` source pixel, and `Shift + Arrow` contracts that same edge inward by `1` source pixel (both work even while a drawing tool is active).
- **Reset selection vs. cancel (`Right-click` / `Esc`)**:
  - `Right-click` cascades: finishes an active polyline arrow, deselects an active annotation, exits interactive OCR mode, resets a confirmed selection (along with its annotations) back to selection mode, or cancels capture when no selection is active (without adding to history).
  - `Esc` cancels capture immediately without adding to history (except when closing an open popover or font search, committing active inline text, cancelling an uncommitted polyline arrow, or stepping out of interactive OCR).

## Overlay HUD & Toolbar Placement

- **Toolbar and subbar placement**: Clivuu reserves vertical space for the active tool's subbar (`4 px` gap plus `32 px` subbar height) before choosing whether to place the `32 px` main toolbar below or above the selection. If the bottom margin fits only the main toolbar and not the subbar, both bars move above the selection together (with the subbar stacked above the main bar) instead of flipping onto the capture border. On tight monitors or fullscreen selections where neither outer side fits, controls fall back inside the selection (`InsideBottom` preferred over `InsideTop`). Dragging the toolbar grip pins a custom position with the subbar anchored below it (flipping above near the bottom screen edge).
- **Top-left size indicator (`W × H px`)**: displays actual captured source pixels above the top-left corner of the selection (hiding at `1 × 1` before dragging when `capture-hide-1x1=true` in Preferences). Once a selection is confirmed, hovering the pointer over the size badge automatically hides it so annotations and top-left pixel checks remain unobstructed; moving away restores it.
- **Bottom-left shortcut key-tips**: when `capture-show-tips=true` (default), contextual shortcuts appear at the bottom-left corner of the active monitor (`12 px` inset) for Hover, Selected, and Editing states. Key-tips hide during drags and active gestures, in OCR mode, and during single-window captures, and automatically hide whenever their bounds overlap a confirmed selection rectangle.

## Pixel Magnifier & Color Sampling

Clivuu displays a pixel-accurate magnifier centered on the pointer hotspot across fractional and mixed-DPI monitors before a selection is created, during initial drags and edge resizing, while hovering stationary inside a confirmed selection in Cursor (`Ctrl+1`) mode (when not over toolbars, popups, OCR overlays, or annotation targets), and whenever `Alt` is held.

- **`Shift` (while magnifier is visible)**: switches the color readout format between `RGB` and `HEX`.
- **`C`**: copies the sampled center pixel color directly to the clipboard while the magnifier is visible.
- **`Alt`**: forces the magnifier visible while held and toggles fullscreen crosshairs (before selection) or alignment guides (after selection).

## Refresh, Capture History & Output Actions

- **Refresh backdrop (`F5`)**: re-acquires a fresh static desktop frame (for standard desktop captures, including cursor state when `capture-cursor=true`) while keeping your current selection rectangle and placed annotations.
- **Browse capture history (`,` and `.`)**: steps backward (`,`) and forward (`.`) through in-memory recent captures matching the current monitor layout (up to `128 MiB`, capped by `capture-history-records`, default `20`).
- **Quick finish actions**:
  - `Enter`, `Ctrl+C`, or double-click inside Cursor mode (outside text marks): copies the cropped result to the clipboard and exits capture.
  - `Ctrl+T`: pins the cropped region directly as a floating desktop pin.
  - `Ctrl+S`: opens the native GTK Save file dialog.
  - `Ctrl+Shift+S`: quick-saves immediately using your configured directory and filename template.
  - `O` / `Shift+O`: `O` toggles in-place interactive OCR text selection on the selected region using the configured offline OCR engine (`PP-OCRv5` or system `Tesseract`, or quick-copies directly if `ocr-quick-copy=true`); `Shift+O` always copies recognized text and exits immediately.
  - `L`: hands the selected rectangle directly into [Scrolling Capture](scrolling-capture.md).
  - `V`: hands the selected rectangle directly into [Screen Recording](recording.md).
  - `Space`: shows or hides the editing toolbar and subbar.

## Backend Acquisition Behavior

- **Standard capture**: calls `org.freedesktop.portal.Screenshot` noninteractively on every session and crops/renders in Clivuu's overlay.
- **Cursor-inclusive capture**: when `capture-cursor=true` is enabled in `Preferences -> Screen capture -> Behavior`, Wayland sessions capture a single compositor frame through `ScreenCast` so the cursor image and desktop pixels share the same frame (and stop the stream before editing), while real X11 sessions composite cursor shapes via XFixes. On GNOME Wayland (via `org.gnome.Mutter.ScreenCast`) and real X11, interactive capture also retains the clean cursor-free frame so pressing `` ` `` or `!` shows or hides the real-position pointer after selecting a region.
- **Window capture (`--window` / `--active-window`)**: opens the system `ScreenCast` window picker on every desktop, reads one window frame, and stops the stream before opening editing tools.
- **Pixel & byte guardrails**: maximum capture dimension/area is capped at `48 MP` (`48,000,000` pixels) per frame and `64 MiB` for encoded transfer safety.

## Related Manuals

- [Annotations & Editing](annotations.md)
- [Saving, Naming & Notifications](saving.md)
- [Troubleshooting](troubleshooting.md)

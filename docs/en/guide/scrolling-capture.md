# Scrolling Capture

[User Guide](../README.md) | **English** | [简体中文](../../zh-CN/guide/scrolling-capture.md)

Scrolling capture stitches long web pages, chat logs, documents, and horizontal tables into one continuous image with a live side preview strip.

## Entry Points

- **From screenshot capture**: drag to frame the scrollable area (avoiding fixed window chrome where possible), then click `Scrolling capture` on the toolbar or press `L`.
- **From CLI**: run `clivuu scroll` to pick a region interactively, or `clivuu scroll --area X Y W H` (`-o pin|clipboard|quick-save|file-dialog|FILE.png|FILE.jpg`) to start on exact coordinates.

## Toolbar Controls & Modes

The floating scrolling toolbar uses the same draggable grip as capture and recording:

| Control | Function |
| :--- | :--- |
| **Direction toggle (`Vertical` / `Horizontal`)** | Switches stitching axis; switching axes resets the current accumulated scroll document |
| **Move capture region** | Slides the capture rectangle along the active scroll axis (vertically in Vertical mode; horizontally in Horizontal mode) without changing its width or height, pausing frame sampling while dragging |
| **Auto-scroll** | Enables closed-loop proportional scroll injection through `org.freedesktop.portal.RemoteDesktop`, stopping automatically when reaching the end of the scrollable view |
| **Trim scrolling capture** | Opens manual endpoint trimming (`Trim above current view` / `Trim below current view` in vertical mode; `Trim left of current view` / `Trim right of current view` in horizontal mode) plus the `Auto-crop on reverse scroll` toggle (`scroll-auto-crop`) |
| **Reset scrolling capture (`Ctrl+R`)** | Opens in-session recovery options without exiting or re-selecting the region: `Continue from current view` (keeps already stitched frames and re-anchors matching at the current screen view) or `Restart in current region` (`Ctrl+R`, clears stitched frames and captures a fresh initial frame in place) |
| **Edit** | Opens the stitched result directly as a [Floating Pin](pins.md) with the annotation toolbar visible |
| **Pin image (`Enter`)** | Places the stitched image onto the desktop as an active pin |
| **Save image (`Ctrl+S` / `Ctrl+Shift+S`)** | Opens the GTK Save file dialog or quick-saves |
| **Copy image (`Ctrl+C`)** | Copies the stitched image to the clipboard |
| **Cancel (`Esc`)** | Aborts scrolling capture |

## Manual vs. Auto-Scroll

- **Manual scrolling (default)**: scroll with your wheel, trackpad, or scrollbar inside the capture rectangle while keeping overlap between consecutive views (`~20–40%` overlap improves alignment on repetitive layouts).
- **Auto-scroll**: requests system `RemoteDesktop` input authorization when clicked. Once authorized, Clivuu injects closed-loop proportional smooth scroll pulses every `scroll-interval` milliseconds (spin setting `300–3000 ms`, default `700 ms`, clamped `150–5000 ms` at runtime, using shorter intervals on scroll spans under `220 px`), falling back to discrete wheel steps if smooth scrolling yields no motion on regions at least `120 px` long. Moving your mouse outside the capture rectangle or hovering the toolbar, hint banner, preview strip, or an open popup pauses auto-scroll automatically so you remain in control. If `RemoteDesktop` is denied or unsupported, manual scrolling continues working normally.

## Live Preview & Stitch Alignment

When screen space outside the selection permits, Clivuu displays a live miniature preview beside the capture rectangle:

- **Green viewport box**: marks the position of the current visible frame within the full stitched image.
- **Toolbar & HUD isolation**: moving the Clivuu toolbar or capture region pauses frame sampling and excludes Clivuu's borders and controls from the captured document.
- **Partial matches, recovery & auto-crop**: if a scroll step moves too far (`>1` full screen without overlap) or the page changes mid-capture (such as expanding a collapsed block or switching views), Clivuu keeps already committed frames exportable. You can scroll back to regain overlap, use `Reset scrolling capture` -> `Continue from current view` to keep stitched frames and continue from the new view, or press `Ctrl+R` (`Restart in current region`) to start fresh in the same box. Enabling `Auto-crop on reverse scroll` (`scroll-auto-crop`, off by default) trims overshot tail or head content as you roll back.

## Limits & Preferences

- **Retained keyframes (`max-scroll-frames`)**: `2–200` (default `40`), configured under `Preferences -> Screen capture -> Behavior`.
- **Auto-scroll step interval (`scroll-interval`)**: `300–3000 ms` (default `700 ms`).
- **Auto-crop on reverse scroll (`scroll-auto-crop`)**: default `false`, synced between the inline `Trim scrolling capture` menu and `Preferences -> Screen capture -> Behavior`.
- **Document & export limits**: minimum scroll-axis span `48 px`, maximum stitched length `65,535 px` along the scroll axis, `128 MiB` internal document budget, and standard `48 MP` / `64 MiB` PNG export limits.

## Related Manuals

- [Screenshot Capture](capture.md)
- [Floating Pins](pins.md)
- [Preferences](preferences.md)

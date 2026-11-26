# Floating Pins

[User Guide](../README.md) | **English** | [简体中文](../../zh-CN/guide/pins.md)

Floating pins keep reference screenshots, text snippets, HTML blocks, color cards, and local image files always-on-top over your desktop. Pins share Clivuu's full [annotation editor](annotations.md) and support nondestructive transforms, opacity changes, click-through mode, grouping, and recovery after closing.

## Creating Pins

1. **From capture**: press `Ctrl+T` or click `Pin` on the capture toolbar.
2. **From clipboard or tray**: run `clivuu paste` or choose `Pin clipboard` in the system tray. Clipboard content is classified automatically in order:
   - Image files (`<= 32` files)
   - Clipboard raster bitmap
   - Non-image copied file paths (skipped by default unless `pin-text-skip-paths=false`)
   - HTML snippets (renders supported inline formatting without loading external URLs; standalone color values become color cards)
   - Native color MIME (`application/x-color`) and plain text (up to `40,000` characters; standalone color values such as `#bd93f9`, `#aarrggbb`, `rgb(255, 128, 0)`, or normalized float triples render as color cards)
3. **From CLI explicit sources**:
   - `clivuu paste --files DIR image1.png nested/image2.jpg` (directories in `--files` update the base path for subsequent relative paths)
   - `clivuu paste --plain "literal text"`
   - `clivuu paste --html "<b>Formatted</b> text"`
   - Optional `--pos X Y` sets the initial screen position.
4. **Drag-and-drop vs. `Ctrl+V` replacement**: dragging supported files or text onto an existing pin creates new pin(s) at that pin's position and hides the target pin unless you hold `Shift` during the drop (or enable `pin-preserve-drag=true`). Pressing `Ctrl+V` while a pin has keyboard focus replaces that pin in place when the clipboard holds a single item, keeping its screen position.

Repeating an identical clipboard or capture source brings the matching existing pin to the front (reopening it first if closed) and flashes it instead of creating duplicates. Calling `clivuu paste` on an empty clipboard makes no changes and returns `{"pinned":0,"ids":[]}` (use `clivuu reopen-pin`, `clivuu reopen-pins`, or the tray `Pin recovery & cleanup` menu to restore closed pins).

## Mouse Gestures & Keyboard Editing

Default controls on an ordinary pin (configurable in `Preferences -> Control -> Mouse`):

| Action | Default Gesture / Key |
| :--- | :--- |
| Move pin | Drag (`Left button`) |
| Resize proportionally | Drag edges or corners |
| Scale pin size | `Wheel` (plus `+` / `-` when enabled) |
| Adjust opacity (`20%–100%` interactively; `10%–100%` default setting) | `Ctrl+Wheel` (or context menu `−` / `+` buttons) |
| Toggle thumbnail / restore | `Shift+Double-click` (or menu `Thumbnail / restore`) |
| Copy OCR or original text | `Shift+Right-click` (copies selected OCR text when active, else source text) or `Ctrl+Shift+C` |
| Reset scale & opacity | Middle click |
| Close pin (recoverable) | Double-click, `Ctrl+W`, or `Esc` (when toolbar is closed) |
| Destroy pin permanently | `Ctrl+Double-click`, `Shift+Esc`, or menu `Destroy pin` |
| Open context menu | Right-click |
| Toggle annotation toolbar | `Space` (or menu `Show toolbar`) |
| Apply toolbar changes & close bar | `Enter`, `Esc` (when toolbar open), or `Space` |

Quick actions on a focused pin (when no annotation drawing tool is active):
- **`1` / `2`**: rotate `90°` right / left
- **`3` / `4`**: flip horizontally / vertically
- **`5`**: toggle grayscale preview/export filter
- **`6`**: toggle color inversion filter
- **`Arrow keys`**: nudge position by `1` logical pixel
- **`Alt`**: display inline pixel magnifier (`Shift` toggles RGB/HEX, `C` copies center pixel color) and allow dragging a pin over active OCR text
- **`O`**: toggle in-place OCR text selection on the pin
- **`Ctrl+Shift+C`**: copies original unrasterized source text from text/HTML/color-card pins
- **`Ctrl+Shift+P`**: opens Clivuu Preferences
- **`Ctrl+A`**: selects all visible pins in the active group so transforms, movement, `Copy image`, `Save image`, close, or destroy apply to the merged group batch.

## Context Menu & Thumbnail Mode

Right-clicking any pin opens its context menu:

- `Copy image` / `Save image` / `Recognize text` (with conditional `Copy selected text` / `Copy all text` when pin OCR is active)
- `Paste` (pins clipboard content as a new or revealed matching pin; press `Ctrl+V` to replace in place)
- `Show toolbar` (toggles full editing & crop controls; `Crop` uses a `1px` accent border with circular outer anchors and supports direct edge/corner resize, interior move of an active crop region, and sub-crop selection)
- `Thumbnail / restore` (collapses the pin to a compact thumbnail and back)
- `Reset size`
- `Opacity` (`−` / current `%` reset to `100%` / `+`)
- `Pointer passthrough` (lets mouse clicks pass straight through the pin to windows below; restore all click-through pins from the tray item `Restore pointer interaction` or `clivuu no-click-through`)
- `Move to group` (moves the clicked pin, or all `Ctrl+A` selected pins, to another group, `Move to new group...`, or opens `Manage groups...`)
- `Close pin` (moves to recoverable history) / `Destroy pin` (deletes immediately)

## Groups, Persistence & Closed-Pin Recovery

- **Pin limits**: up to `32` total active (unclosed) pins across all groups and `128` total stored pins combined (active plus closed), with closed-pin retention set by `pin-closed-limit` (default `1`, range `0–128`).
- **Storage modes**: `pin-storage` in `Preferences -> Pins -> Behavior` (`Store pins in memory`) chooses whether pins remain memory-only for the running session or persist across restarts under `$XDG_DATA_HOME/clivuu/pins/`. Persisted closed-pin bitmaps unload pixel memory until restored.
- **Pin groups**: organize workspaces via the tray `Pin groups` submenu, the pin right-click `Move to group` menu, the `Manage Pin Groups` window (`clivuu show-group-manager`), the quick switcher (`clivuu switch-groups`), or CLI commands (`create-group`, `switch-group`, `switch-group-prev`, `rename-group`, `delete-group`, `remove-empty-groups`, `empty-group`, `groups`). Groups support optional `#rrggbb` color tags, reordering, and migrating pins to another group on deletion; switching groups hides pins in inactive groups without closing them.

## Wayland & GNOME Focus Behavior

- **Native Wayland (`niri`, `Hyprland`, KDE)**: pins run as `gtk4-layer-shell` `ON_DEMAND` surfaces. Active pins highlight with the theme accent shadow.
- **GNOME Wayland**: pins display via XWayland because GNOME lacks `wlr-layer-shell`. `pin-gnome-focus` is enabled (`true`) by default so GNOME pins accept keyboard focus for shortcuts and modifier gestures. Setting `pin-gnome-focus=false` in `Preferences -> Pins -> Behavior` disables keyboard focus across the entire pin lifecycle (preventing `dash-to-panel` auto-hide changes), including while the pin toolbar is open; editing works via mouse and toolbar buttons, and pointer-leave deactivates normal pins automatically.

## Related Manuals

- [Annotations & Editing](annotations.md)
- [Shortcuts & Mouse Gestures](shortcuts.md)
- [Preferences](preferences.md)

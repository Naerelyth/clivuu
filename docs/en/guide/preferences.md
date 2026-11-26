# Preferences

[User Guide](../README.md) | **English** | [简体中文](../../zh-CN/guide/preferences.md)

Open settings from the tray menu (`Preferences`), pin shortcut (`Ctrl+Shift+P`), or `clivuu open-preferences`. Preferences run as an independent regular window (native Wayland on Wayland desktops) and always reopen on the `General` tab. Section reset buttons affect only the visible section and ask for confirmation first.

## General

- **Interface**:
  - `Interface language` (`language`): `System default`, `English`, or `简体中文` (endonyms).
  - `Appearance` (`theme`): `Follow system`, `Light`, or `Dark` Dracula theme styling.
  - `Preferences font` (`preferences-font`): Pango font override (`""` uses the GTK system font) for Preferences and dialogs opened from it.
  - `Accent color` (`accent-color`): picker + inline reset button (default `#bd93f9ff`). Applied to active buttons, capture selection borders, active pin glow, and controls.
- **Startup**:
  - `Run at login`: manages a standard XDG Autostart `.desktop` file directly (off by default).

## Screen Capture

Split into two sub-sections:

- **Display**:
  - Selection border width (`capture-border-width`, `1–8 px`) and backdrop mask color (`capture-mask-color`)
  - Resize anchors toggle (`capture-show-anchors`) and outline color (`capture-anchor-color`)
  - Magnifier internal toggles (`capture-magnifier-mask`, `capture-magnifier-border`, `capture-magnifier-anchors`)
  - Crosshair (`capture-crosshair`), alignment guides suppression (`capture-disable-guides`), and hint bar toggle (`capture-show-tips`)
- **Behavior**:
  - Smart window detection toggle (`capture-detect-windows`, Hyprland and niri), hide `1x1` helper hints (`capture-hide-1x1`), capture cursor (`capture-cursor`, toggleable with `` ` `` or `!` during interactive capture on GNOME Wayland and X11), hide existing pins during capture (`capture-hide-pins`), loop capture history (`capture-history-loop`), saved region history count (`capture-history-areas`, `0–100`), and in-memory capture history count (`capture-history-records`, `0–100`, `128 MiB` pixel ring buffer)
  - Scrolling options: `Retained keyframes` (`max-scroll-frames`, `2–200`, default `40`), `Auto-scroll step delay (ms)` (`scroll-interval`, `300–3000 ms`, default `700 ms`), and `Auto-crop on reverse scroll` (`scroll-auto-crop`, default `false`)

## Pins

Split into three sub-sections:

- **Display**: new-pin shadow toggle (`pin-shadow`) and inactive shadow tint (`pin-shadow-tint`; active/selected pins use `accent-color`), initial new-pin opacity (`pin-opacity`, `10–100`), maximum longest-edge dimension for new and existing pins (`pin-max-size`, `100–100000`), and thumbnail bounds (`pin-thumbnail-width`, `pin-thumbnail-height`, `24–1000`).
- **Behavior**: capture origin positioning (`pin-use-snip-position`), cursor-centered wheel zoom (`pin-zoom-at-cursor`), smooth vs crisp scaling for new pins (`pin-smooth-scaling`), thumbnail reset behavior (`pin-reset-thumbnail`), flash feedback (`pin-translucent-flash`, `pin-flash-group`, `pin-hide-on-flash`, `pin-flash-new`), default active state (`pin-activate-new`), preserve drag (`pin-preserve-drag`), `Allow pins to take keyboard focus` (`pin-gnome-focus`, active only on GNOME; enabled by default; when `false`, disables keyboard focus across the entire pin lifecycle including when the toolbar is open), close/destroy confirmations (`pin-confirm-close`, `pin-confirm-destroy`), closed-pin history cap (`pin-closed-limit`, `0–128`, default `1`), and storage mode (`pin-storage`: `persistent` vs `memory`).
- **Conversion**: text-to-pin conversion switches (`pin-text-enabled`, `pin-text-ignore-html`, `pin-text-skip-paths`), margin (`pin-text-margin`, `0–100`), max width (`pin-text-max-width`, `100–10000`), font (`pin-text-font`), and background/foreground colors (`pin-text-background`, `pin-text-foreground`).

## Save

Split into two sub-sections where each file-name field includes a **Naming rules** guide button inside its right-side tip popover:

- **Images**: manual template (`image-manual-name`), remember format (`image-remember-extension`, storing `image-last-extension`), JPEG quality (`image-quality`, `0–100`, default `100`; PNG stays lossless), quick save directory, template, and notification toggle (`image-quick-directory`, `image-quick-name`, `image-save-notify`, covering quick and automatic saves), plus auto-save switch, directory (empty falls back to quick save), and template (`image-auto-save`, `image-auto-directory`, `image-auto-name`).
- **Recording**: manual name template (`video-manual-name`), save directory, quick name template, and notification toggle (`video-directory`, `video-quick-name`, `video-save-notify`), storage mode (`recording-storage`: `temporary` buffering under `/tmp` vs `direct` writes to the video folder), and memory-first ceiling (`recording-temp-limit-mib`, `16–16384 MiB`).

## Control

- **Shortcuts**: read-only grouped reference tables covering `Annotation tools`, `Shared editing`, `Screenshot selection`, `Pins`, `Scrolling capture`, and `Magnifier`.
- **Mouse**: configurable pin gestures (`pin-mouse-zoom`, `pin-mouse-opacity`, `pin-mouse-close`, `pin-mouse-reset`, `pin-mouse-thumbnail`, `pin-mouse-copy-text`, `pin-mouse-destroy`), optional `+`/`-` wheel key toggles (`pin-mouse-zoom-keys`, `pin-mouse-opacity-keys`), and per-section reset for `Control -> Mouse`.

## OCR

- **Recognition and copying**:
  - `Recognition engine` (`ocr-engine`): `Automatic` (uses an installed PP-OCRv5 model first and falls back to system Tesseract), `PP-OCRv5 (Local)`, or `System Tesseract`. Recognition runs offline inside a short-lived worker process without keeping models in background memory.
  - `Keep paragraph breaks` (`ocr-smart-paragraphs`, default `true`): inserts blank lines between separate text blocks when copying.
  - `Copy immediately without selection` (`ocr-quick-copy`, default `false`): makes `O` copy recognized text and close immediately (`Shift+O` always quick-copies).
  - `Tesseract language packs` (`ocr-languages`, default `eng+chi_sim`): multi-select list that detects installed system `traineddata` packs automatically, highlights missing packs, provides a copyable package-manager install command, and supports live rescanning without free-text code entry.
- **Inference runtime**: manages the shared SHA-256-verified ONNX Runtime library used by local PP-OCRv5 models (reusing a compatible system `libonnxruntime` when installed or downloading into `$XDG_DATA_HOME/clivuu/ocr/runtime/onnxruntime-1.28.0-<arch>/installed/`), with `Open runtime folder` for manual offline placement. Resetting the OCR section keeps downloaded models and runtime files intact.
- **Local models (PP-OCRv5)** (`ocr-model`, default `chinese`): on-demand SHA-256-verified language packs (`chinese`, `latin`, plus expandable `korean`, `cyrillic`, `arabic`, `greek`, `thai`) stored under `$XDG_DATA_HOME/clivuu/ocr/models/`, with `Open models folder` for manual import and an optional `GitHub mirror prefix` (`ocr-download-mirror`).

## About

Displays `Clivuu`, build version, and repository link (`https://github.com/Naerelyth/clivuu`).

## Related Manuals

- [Configuration Schema](../development/configuration.md)
- [Saving, Naming & Notifications](saving.md)
- [Shortcuts & Mouse Gestures](shortcuts.md)

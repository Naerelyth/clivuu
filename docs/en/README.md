# Clivuu User Guide

**English** | [简体中文](../zh-CN/README.md) | [Project README](../../README.md)

Welcome to the Clivuu User Guide. Use the feature manuals below to learn how each tool works, or jump directly to the Developer Reference if you are scripting Clivuu, inspecting configuration keys, or packaging from source.

## Start Here

- **[Getting Started](guide/getting-started.md)**: verify runtime dependencies, start the background tray service, configure system hotkeys, and take your first screenshot.

## Feature Manuals

| Feature | What the manual covers | Link |
| :--- | :--- | :--- |
| **Screenshot Capture** | Region, monitor, full-desktop, and window selection, live pixel magnifier, color sampling, and capture history browsing | [Capture](guide/capture.md) |
| **Annotations & Editing** | `32 px` main toolbar and subbars, shapes, 11 arrow types with 5 line styles, freehand pen/highlight, sequence badges, mosaic/blur, inline text, eraser, and color popups | [Annotations](guide/annotations.md) |
| **Floating Pins** | Creating pins from captures or the clipboard (images, files, HTML, color values, plain text), transforms, mouse gestures, groups, closed-pin recovery, and GNOME focus rules | [Pins](guide/pins.md) |
| **Scrolling Capture** | Vertical and horizontal capture, overlap preview strip, manual vs `RemoteDesktop` auto-scroll, endpoint trimming, and export | [Scrolling Capture](guide/scrolling-capture.md) |
| **Screen Recording** | Live frame recording, FPS and delay presets, system/mic audio tracks, pause and HUD auto-hide rules, and playback trim/crop/speed export (`MP4`, `WebM`, `GIF`) | [Recording](guide/recording.md) |
| **OCR & Barcodes** | Dual-engine offline OCR (local PP-OCRv5 ONNX Runtime and system Tesseract), in-place text selection, multi-language configuration, `zbarimg` screen scan, and file barcode decoding | [OCR & Codes](guide/ocr-and-codes.md) |
| **Saving & Notifications** | Manual GTK file dialog, quick save vs auto save, naming template tokens, JPEG quality vs lossless PNG, and file manager reveal | [Saving](guide/saving.md) |
| **Shortcuts & Gestures** | Full keyboard shortcut tables across every mode plus configurable pin mouse gestures and wheel modifiers | [Shortcuts](guide/shortcuts.md) |
| **Preferences** | Every Preferences category (`General`, `Screen capture`, `Pins`, `Save`, `Control`, `OCR`, `About`) and section-level reset | [Preferences](guide/preferences.md) |
| **Troubleshooting** | Portal consent resets, GNOME focus behavior, cursor capture flags, missing OCR/barcode packages, and service diagnostics | [Troubleshooting](guide/troubleshooting.md) |

## Technical & Developer Reference

- **[Architecture](development/architecture.md)**: Workspace crate responsibilities, backend capability selection, and session lifecycle.
- **[CLI & D-Bus Control](development/cli.md)**: command options, exit output formats, worker commands, and desktop service IPC.
- **[Configuration Schema](development/configuration.md)**: sparse XDG INI storage rules and complete 89-key default reference.
- **[Building & Packaging](development/building.md)**: Cargo/CMake/CPack build steps, release checks, and DEB/RPM/TGZ/AppImage output.
- **[Localization](development/localization.md)**: shared UI and CLI catalogs (`core/locales/`), language override precedence, and `i18n.mjs` verification.
- **[Testing & Verification](development/testing.md)**: workspace test targets, private D-Bus/WirePlumber isolation, and compositor validation.

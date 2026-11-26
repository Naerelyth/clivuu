# Architecture

**English** | [简体中文](../../zh-CN/development/architecture.md) · [User Guide](../README.md)

## Workspace

| Component | Responsibilities |
| :--- | :--- |
| `core/` | Image operations/export, annotation model/geometry/history, stitching, configuration schema, protocol types, OCR core, and locale catalogs (no GTK dependency) |
| `cli/` | Argument parsing, local `config`/`portal`/`worker` commands, and D-Bus desktop client (no GUI dependency) |
| `desktop/src/application/` | Session orchestration across capture, pins, scrolling capture, recording/playback, and outputs |
| `desktop/src/editor/` | Shared annotation controller, coordinate mapping, scene rendering, and edit lifecycle |
| `desktop/src/ui/` | Main/sub toolbars, preferences window, file save dialogs, menus, and hints |
| `desktop/src/platform/` | Compositor/X11 surfaces, ScreenCast/Screenshot acquisition, clipboard, input regions, tray, and icon resolution |
| `desktop/src/infrastructure/` | INI file watcher, bounded worker queues, and storage/runtime helpers |
| `desktop/x11/` | Isolated X11 bindings and native window/XFixes operations |

## Editing & Export Pipeline

Capture overlays and pin toolbars share a single annotation session, operation dispatch, hit testing, tool parameters, render pipeline, and undo/redo history. Toolbars read undo/redo sensitivity directly from the active session. Consecutive wheel changes can coalesce into a single undoable transaction.

The editor keeps four coordinate domains distinct: source physical pixels, desktop logical coordinates, surface scale factors, and pointer hotspots (supporting negative origins and mixed/fractional scaling). Source bitmaps stay unscaled in memory, transforms/filters apply identically to screen preview and file output, and logical text or color cards select target surface scale prior to rasterization.

`core::image::export` owns extension resolution, encoder parameter mapping, quality scaling, and image writing. Output routing (`clipboard`, `pin`, `file-dialog`, `quick-save`, `image-auto-save`, `printer`, `ocr_clipboard`, `barcode`, or path output) and history rings live in desktop orchestration.

## Surfaces & Acquisition

Display surfaces and frame acquisition are resolved independently according to session capabilities:

- **Native Wayland (`niri`, `Hyprland`, KDE)**: overlays and pins use `gtk4-layer-shell` with `ON_DEMAND` pin focus.
- **GNOME Wayland**: overlays and pins use XWayland surfaces because GNOME omits `wlr-layer-shell`; Preferences, Save dialogs, Group Manager/Switcher, and confirmation dialogs run as independent native Wayland windows. GNOME pins accept keyboard focus by default and refuse keyboard focus across their entire lifecycle when `pin-gnome-focus=false`.
- **X11**: handled by the dedicated X11 adapter.

Acquisition paths:
- **Standard capture**: uses the noninteractive `Screenshot` portal across desktops (plus `XFixes` cursor compositing on real X11), unless `capture-cursor=true` on Wayland reads one compositor `ScreenCast` frame so cursor and image come from the same frame.
- **Window capture**: opens one system `ScreenCast` window frame on every desktop and closes the stream before annotation begins.
- **Continuous capture**: `ScreenCast` streams back scrolling capture and recording and display compositor sharing indicators while active.

All popups, grab scopes, and overlays release input regions immediately upon closing, cancellation, errors, or service shutdown.

## Bounded Asynchronous Work

GTK widgets run strictly on the main UI thread. Image decoding, PNG/JPEG encoding, long-image stitching, and filesystem persistence run in bounded background jobs. Dual-engine offline OCR (`PP-OCRv5` ONNX Runtime and system `Tesseract` 5) executes in a short-lived worker subprocess (`clivuu worker ocr-document` / `--ocr-worker`) reading shared image payloads over `memfd_create`. Pointer motion updates in-memory state and cached GPU/Pixbuf textures directly without spawning processes, re-encoding PNGs, or blocking on synchronous D-Bus waits.

Scrolling capture separates stitch body alignment, document coverage maps, and fixed margins, keeping committed segments exportable even when scrolling ends early or drops a match. Recording writes lossless intermediate streams (`VP9` Y444 in Matroska plus independent `FLAC` system/microphone tracks) through bounded queues and hands immutable source material to the playback editor.

## Configuration & Process Boundaries

One schema (`core/config-defaults.json`) supplies defaults and field validation across CLI and GUI. Configuration writes coalesce asynchronously into grouped sparse INI (`config.ini`), storing non-default entries only while watching external modifications.

Persistent pins and closed-pin pixel payloads reside in `$XDG_DATA_HOME/clivuu/`. Coordinated service restarts cancel open sessions, finalize recordings, flush config and pin data, release `io.github.naerelyth.Clivuu.Desktop` (`/io/github/naerelyth/Clivuu/Desktop`, `io.github.naerelyth.Clivuu.Desktop1`), and launch the durable binary entry point (`APPIMAGE` or resolved prefix executable).

See [CLI & D-Bus Control](cli.md), [Configuration Schema](configuration.md), and [Testing & Verification](testing.md) for interface details.

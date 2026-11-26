<div align="center">
  <img src="assets/icons/clivoo.svg" width="88" alt="Clivuu">
  <h1>Clivuu</h1>
  <p>Screenshot, annotation, image pinning, scrolling capture, and region recording tool for Linux Wayland and X11.</p>
  <p><strong>English</strong> | <a href="README.zh-CN.md">简体中文</a></p>
</div>

> **User Guide:** Step-by-step manuals for every desktop feature live in the [English User Guide](docs/en/README.md) ([简体中文使用手册](docs/zh-CN/README.md)). Read them for detailed workflows on selection, annotations, pins, scrolling capture, video recording, OCR, naming templates, shortcuts, and settings.

## Overview

Clivuu runs a background tray and D-Bus desktop service (`clivuu-desktop`) paired with a CLI client (`clivuu`), supporting Linux Wayland and X11 sessions.

| Capability | Summary | Manual |
| :--- | :--- | :--- |
| **Screenshot capture** | Static portal backdrop, frozen overlay selection, magnifier, color picker, window picker, and capture history | [Capture](docs/en/guide/capture.md) |
| **Canvas annotation** | Unified 8-tool editor (shapes, arrow, pen, highlight, sequence badges, mosaic, text) plus eraser across captures and pins | [Annotations](docs/en/guide/annotations.md) |
| **Floating pins** | Always-on-top windows from screenshots, clipboard bitmaps, files, HTML, or color codes, with transforms, groups, and recovery | [Pins](docs/en/guide/pins.md) |
| **Scrolling capture** | Vertical and horizontal stitched capture with live overlap preview, optional auto-scroll, and trim controls | [Scrolling Capture](docs/en/guide/scrolling-capture.md) |
| **Screen recording** | Lossless pipewire intermediate recording with system/mic audio and interactive trim/crop/speed playback export (`MP4`, `WebM`, `GIF`) | [Recording](docs/en/guide/recording.md) |
| **OCR & barcodes** | Offline dual-engine text recognition (local PP-OCRv5 ONNX Runtime and system `libtesseract` 5) with in-place selection or quick copy, plus QR/barcode decoding via `zbarimg` | [OCR & Codes](docs/en/guide/ocr-and-codes.md) |

## Desktop & Display Matrix

Clivuu selects display presentation and screen acquisition backends independently based on session capabilities:

| Session Environment | Overlay & Pins | Standard Capture | Window Capture / Cursor |
| :--- | :--- | :--- | :--- |
| **niri / Hyprland** | Native Wayland (`gtk4-layer-shell`, `ON_DEMAND` pins) | Noninteractive `Screenshot` portal | Compositor window detection (`niri msg` / `hyprctl`) + single-frame `ScreenCast` window picker & cursor composite |
| **KDE Plasma / wlr-layer-shell** | Native Wayland (`gtk4-layer-shell`, `ON_DEMAND` pins) | Noninteractive `Screenshot` portal | Single-frame `ScreenCast` window picker & cursor composite |
| **GNOME (Wayland)** | XWayland overlay and pins (`pin-gnome-focus` toggle); Preferences, Save, Group Manager, and confirmation dialogs run on native Wayland | Noninteractive `Screenshot` portal | Single-frame `ScreenCast` window picker & cursor composite |
| **X11** | Native X11 windows | Noninteractive `Screenshot` portal | Single-frame `ScreenCast` window picker + XFixes cursor capture |

Global hotkeys are configured in your desktop environment or window manager by binding shortcuts to `clivuu` CLI commands rather than grabbing global keys from inside the app.

## Requirements

Core runtime dependencies declared in [CMakeLists.txt](CMakeLists.txt):

- **Core desktop UI**: GTK 4 (`>= 4.14`), libadwaita (`>= 1.5`), `gtk4-layer-shell`
- **Portal & capture**: `xdg-desktop-portal` (plus your desktop's portal backend), `xwayland` (recommended on GNOME for overlays/pins)
- **Recording & audio**: GStreamer (`>= 1.20`), `gstreamer1.0-pipewire` (`pipewire-gstreamer`), `gstreamer1.0-pulseaudio`, `gstreamer1.0-plugins-base`, `gstreamer1.0-plugins-good`, `ffmpeg` (`ffmpeg-free` on Fedora)
- **Optional tools**: `libonnxruntime` / `curl` (for PP-OCRv5 models & ONNX Runtime downloads in Preferences), `libtesseract5` and trained language data (fallback OCR), `zbarimg` / `zbar-tools` (for barcode scanning), `lp` / CUPS (for printing)

<details>
<summary>Package installation examples (Debian/Ubuntu and Fedora)</summary>

**Debian / Ubuntu:**
```bash
sudo apt install libgtk-4-1 libadwaita-1-0 libgtk4-layer-shell0 \
  libgstreamer1.0-0 gstreamer1.0-pipewire gstreamer1.0-pulseaudio \
  gstreamer1.0-plugins-base gstreamer1.0-plugins-good xdg-desktop-portal \
  ffmpeg xwayland tesseract-ocr tesseract-ocr-eng zbar-tools
```

**Fedora:**
```bash
sudo dnf install gtk4 libadwaita gtk4-layer-shell gstreamer1 \
  pipewire-gstreamer gstreamer1-plugins-base gstreamer1-plugins-good \
  xdg-desktop-portal ffmpeg-free xorg-x11-server-Xwayland \
  tesseract tesseract-langpack-eng zbar
```

</details>

## Building & Packaging

Build artifacts and packages write under `/tmp` by default via [Makefile](Makefile) and [CMakeLists.txt](CMakeLists.txt):

```bash
# Build release CLI (clivuu) and desktop service (clivuu-desktop)
make build

# Run workspace unit checks, locale validation, config checks, doc checks, and smoke tests
make test

# Build distributable packages under /tmp/clivuu-dist
PACKAGER='Your Name <you@example.org>' make deb        # Debian/Ubuntu .deb
PACKAGER='Your Name <you@example.org>' make rpm        # Fedora/RHEL .rpm
make package                                           # Portable .tar.gz archive
make appimage APPIMAGE_RUNTIME=/path/to/runtime-x86_64 # AppImage bundle
```

For development headers, install prefixes, and AppImage runtime flags, see [Building & Packaging](docs/en/development/building.md).

## Quick Start & Basic CLI Usage

Start the desktop service (tray icon + D-Bus server):

```bash
clivuu
```

Run common desktop actions directly or bind them as desktop keyboard shortcuts:

```bash
clivuu snip                             # Start interactive region capture
clivuu snip --window                    # Pick a window via ScreenCast and capture one frame
clivuu snip --delay 3 -o quick-save     # Wait 3 seconds, capture, and quick-save
clivuu paste                            # Pin clipboard content (image, files, HTML, color, text)
clivuu scroll                           # Select a region and start scrolling capture
clivuu record                           # Select a region and open recording setup
clivuu pick-color                       # Sample a pixel color to clipboard
clivuu barcode-scan                     # Capture a region and decode QR/barcodes
clivuu open-preferences                 # Open Preferences
clivuu status                           # Print desktop service state as JSON
clivuu restart                          # Restart the desktop service
clivuu exit                             # Stop the service
```

Read or update configuration without starting GTK:

```bash
clivuu config path                      # Show active config.ini path
clivuu config get image-quality         # Read one setting
clivuu config set language zh-CN        # Write one non-default override
clivuu --help                           # Show command summary
```

Full command options and D-Bus specifications are documented in [CLI & D-Bus Control](docs/en/development/cli.md).

## Data & Configuration Locations

- **Configuration**: `$XDG_CONFIG_HOME/clivuu/config.ini` (sparse INI grouped by category; stores non-default overrides only)
- **Data (pins & OCR)**: `$XDG_DATA_HOME/clivuu/` (persistent pin data in `pins/`, downloaded OCR runtime/models in `ocr/`)
- **Cache**: `$XDG_CACHE_HOME/clivuu/`
- **Default quick saves**: `$XDG_PICTURES_DIR/Clivuu/` (images) and `$XDG_VIDEOS_DIR/Clivuu/` (videos)

## Repository Structure

```text
core/        Shared Rust core (images, annotation model, stitching, config schema, OCR, locales)
cli/         CLI client, local config commands, portal helper, and subcommand workers (clivuu)
desktop/     GTK4/libadwaita desktop service, adapters, capture/pin/scroll/recording sessions (clivuu-desktop) and dynamic X11 crate (desktop/x11/)
assets/      Shipped icons (assets/icons/) and icon design assets
packaging/   Desktop entry file, CPack package scripts, and AppImage templates (CMake install rules live in CMakeLists.txt)
docs/        Bilingual user manuals (guide/) and technical docs (development/)
tests/       Config, documentation, i18n, CLI readiness, packaging, and compositor test harnesses
```

## Documentation Index

- **User Manuals**: [English User Guide](docs/en/README.md) · [简体中文使用手册](docs/zh-CN/README.md)
- **Technical Reference**:
  - [Architecture](docs/en/development/architecture.md)
  - [CLI & D-Bus Control](docs/en/development/cli.md)
  - [Configuration Schema](docs/en/development/configuration.md)
  - [Building & Packaging](docs/en/development/building.md)
  - [Localization](docs/en/development/localization.md)
  - [Testing & Verification](docs/en/development/testing.md)

## License

Licensed under the [GNU General Public License v3.0 or later](LICENSE).

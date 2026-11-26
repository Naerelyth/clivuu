# Building & Packaging

**English** | [简体中文](../../zh-CN/development/building.md) · [User Guide](../README.md)

## Toolchain & System Dependencies

Clivuu compiles with Rust edition 2024 (`rustc 1.94` toolchain used in development). Cargo builds the `clivuu` and `clivuu-desktop` ELF binaries (`.cargo/config.toml` sets `target-dir = "/tmp/clivuu-target"`); CMake `>= 3.24` and CPack manage installation trees and package generation.

### Build-Time Development Headers

Required C/GTK/GStreamer development headers (`pkg-config` resolvable):

- GTK 4 (`>= 4.14`), libadwaita (`>= 1.5`)
- GStreamer (`>= 1.20`), `gstreamer-plugins-base`
- CMake `>= 3.24` and a standard C linker toolchain

`gtk4-layer-shell`, `libX11`, and `libXfixes` are loaded dynamically via `dlopen` at runtime and are not required as build-time `-dev` headers (`libxext`, `libxtst`, and `libxrandr` are not used).

Example on Debian/Ubuntu:

```sh
sudo apt install build-essential pkg-config cmake \
  libgtk-4-dev libadwaita-1-dev \
  libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev
```

### Runtime Dependencies

- PipeWire, `xdg-desktop-portal` plus compositor portal backend, `gtk4-layer-shell`, and XWayland (used for GNOME overlay/pin surfaces)
- GStreamer runtime plugins providing `pipewiresrc`, `appsink`/`appsrc`, `vp9enc`, `vp9dec`, `matroskamux`, `matroskademux`, `flacenc`, `flacparse`, `flacdec`, `pulsesrc` (`gstreamer1.0-pulseaudio`), `audiomixer`, and `scaletempo`
- `ffmpeg` and `ffprobe` (`ffmpeg-free` on Fedora) for WebM/MP4/GIF export transcoding
- Optional utilities: `libonnxruntime` C API v17+ + `curl` (local `PP-OCRv5` OCR downloads/inference), `libtesseract5` + language traineddata (System Tesseract OCR), and `lp` (CUPS printing); QR/barcode decoding is built in via `rxing`

## Compiling & Local Installation

All build targets write out-of-tree under `/tmp` by default (`/tmp/clivuu-target` and `/tmp/clivuu-package`):

```sh
make build
cmake -S . -B /tmp/clivuu-build \
  -DCLIVUU_PREBUILT_DIR=/tmp/clivuu-target/release
cmake --install /tmp/clivuu-build --prefix "$HOME/.local"
```

Omitting `CLIVUU_PREBUILT_DIR` and running `cmake --build /tmp/clivuu-build` builds both binaries via Cargo automatically.

Installed directory layout under `<prefix>`:
- `bin/clivuu` (CLI binary)
- `libexec/clivuu-desktop` (background GUI service discovered relative to `<prefix>`)
- `share/clivuu/icons/` & `share/icons/hicolor/scalable/apps/io.github.naerelyth.clivuu.svg`
- `share/applications/io.github.naerelyth.clivuu.desktop`
- `share/doc/clivuu/` (README files and bilingual Markdown documentation) & `share/licenses/clivuu/Octicons-LICENSE.txt`

User config and persisted pin data in `$XDG_CONFIG_HOME/clivuu/` and `$XDG_DATA_HOME/clivuu/` remain untouched on install or upgrade.

## DEB & RPM Packages

Native DEB and RPM packages target `x86_64` and `aarch64` hosts and rely on `dpkg-shlibdeps` (Debian/Ubuntu) or `rpmbuild` `AutoReq` (Fedora) for ELF shared-library resolution alongside declared non-linked runtime plugins (`gtk4-layer-shell`, GStreamer plugins, `ffmpeg`):

```sh
PACKAGER='Your Name <you@example.org>' make deb
PACKAGER='Your Name <you@example.org>' make rpm
```

Revision defaults to `1` (override via `-DCLIVUU_PACKAGE_RELEASE=...` when calling CMake directly; `Makefile` does not forward `CLIVUU_PACKAGE_RELEASE`). Build packages on the matching target distribution so shared-library dependency resolution stays accurate, and inspect payloads with `dpkg-deb --contents` / `rpm -qlp` before distributing.

## Portable Tarball & AppImage

```sh
make package                                        # Generates .tar.gz under /tmp/clivuu-dist
make appimage APPIMAGE_RUNTIME=/path/to/runtime-x86_64
```

`make appimage` stages `/usr` via CPack's External generator and invokes `appimagetool` (`APPIMAGETOOL` env override supported) paired with a locally supplied type-2 AppImage runtime (`APPIMAGE_RUNTIME`; the build never downloads runtimes automatically). Output directory defaults to `/tmp/clivuu-dist` (`PACKAGE_OUTPUT`).

Because AppImage runs as a mount, Clivuu's launcher keeps runtime execution decoupled from the mounted process so `clivuu` CLI calls can exit while `clivuu-desktop` stays running and restarts relaunch the external `.AppImage` path via `$APPIMAGE`. Verify GTK and GStreamer runtime availability on target systems since system libraries are not embedded inside the bundle.

See [Testing & Verification](testing.md) for package validation checks.

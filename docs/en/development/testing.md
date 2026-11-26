# Testing & Verification

**English** | [简体中文](../../zh-CN/development/testing.md) · [User Guide](../README.md)

Run all build and test commands from the repository root with build artifacts under `/tmp`. Test suites use isolated temporary directories and must never modify host user configuration, clipboard, autostart entries, or live services.

## Core & Static Checks

```sh
cargo build --locked
cargo test --workspace --locked
cargo fmt --all -- --check
cargo clippy --workspace --locked --all-targets -- -D warnings
node tests/i18n.mjs
GIO_USE_VFS=local dbus-run-session -- gjs -m tests/cli-readiness.js /tmp/clivuu-target/debug/clivuu
python3 -B tests/config.py
python3 -B tests/package_native.py
python3 -B tests/documentation.py
python3 -B tests/toolbar_parity_test.py
```

`cargo build --locked` compiles the debug binaries needed before running `tests/cli-readiness.js` and `tests/config.py`. `make test` runs the default automated suite and `make check` runs `rustfmt` and `clippy`. `tests/package_native.py` validates CMake installation layout plus TGZ, DEB, and RPM containers when native packaging tools are available (`--appimage-runtime /path/to/runtime-x86_64` enables AppImage container checks using local binaries). Container checks use ELF fixtures and temporary install trees; cross-distribution shared-library resolution still requires validation on the target Linux distribution.

## Desktop & Compositor Integration

`tests/gnome.py` launches a private nested GNOME Shell session to verify overlays, pins, editing, save dialogs, preferences, shortcuts, notifications, scrolling capture, and screen recording. List modes with:

```sh
python3 -B tests/gnome.py --help
```

Build the matching profile examples (matching `--binary /tmp/clivuu-target/release/clivuu` or debug `/tmp/clivuu-target/debug/clivuu`) before running probe-backed modes:

- **`--rust-capture-consent`**: portal screenshot authorization, tray/CLI capture triggers, shutter counts, and notifications (set `CLIVUU_TEST_AUTOSTART=1` for private autostart testing).
- **`--rust-color`**: repeated screen color picker requests, cancellation, pointer pixels, and dialog teardown via private `color_probe` (`cargo build --release -p clivuu-desktop --example color_probe`).
- **`--rust-continuous`**: `MP4`/`WebM`/`GIF` output, pause clock exclusion, countdown cancellation, `Normal`/`Quick` recording modes, playback edits, and native GTK Save dialog via `continuous_probe` and `recording_controls_probe`. Set `CLIVUU_TEST_NIRI_PREFIX` to test native `gtk4-layer-shell` surface behavior on `niri`.
- **`--rust-playback`**: normal stop-to-playback flow without virtual audio, timeline trim, speed changes, canvas crop, repeated export, and cancelled saves.
- **`--rust-recording-audio`**: private PipeWire/PulseAudio null sinks and generated `440/880 Hz` test tones covering track selection, gain/mute, Opus/AAC streams, pause handling, and teardown without touching real audio hardware.
- **`--rust-cursor-scale`**: dynamic `1×`/`2×` XSettings scale switching comparing cursor pixel sizes across active pins, open menus, and capture backdrops.
- **`--rust-monitors` (with optional `--single-monitor`)**: `2880×1800 + 1920×1080` dual-monitor (`200%/100%`) and single-monitor pixel/checkerboard alignment checks comparing raw capture, selection display, and export sharpness ([Screenshot Capture](../guide/capture.md)).

Standalone GStreamer/FFmpeg encoding and SSIM pixel-quality check (writes outputs under `/tmp`):

```sh
cargo test --locked -p clivuu-desktop --lib application::recording::encoding::tests -- --include-ignored --nocapture
```

## Media & Session Isolation

Running under a private session D-Bus alone does not isolate host audio or Bluetooth devices. `tests/gnome-session.sh` and `tests/wireplumber-test.conf` disable physical audio-card, Bluetooth, and camera probing inside test sessions. Keep hardware isolation enabled and run `python3 -B tests/media-isolation.py` whenever modifying session fixtures.

## Baseline & Regression Checks

- Frozen visual/geometry test fixtures in `tests/reference/legacy` are validated against their manifest hashes and remain outside release packages. Keep baselines unmodified when fixing bugs.
- Effective regression checks test discriminating paths directly: multi-annotation single-step undo, wheel transaction coalescing, rapid mosaic edits, three consecutive menu/dismiss cycles for zero lingering input regions, cancelled exports/saves, extension-to-byte format parity, and live dialog suffix updates.

## Reporting Verification Scope

Clearly separate static compilation checks, unit tests, protocol fixtures, nested compositor runs, and bare-metal sessions when summarizing test results. Verification on GNOME or nested `niri` does not certify KDE Plasma, Hyprland, fractional mixed-DPI monitors, or third-party portal backends on physical hardware.

# Clivuu: agent instructions

Keep durable technical decisions here, not session transcripts. Explicit user requirements override existing product contracts. Communicate in Chinese unless requested otherwise; keep agent instructions in English. Report verified behavior, evidence, and limitations.

## Working method

- Trace entry points, input handling, state, rendering, export, and teardown before changing behavior. Maintain a working baseline and simplify boundaries instead of accumulating special cases.
- Ground design and bug-fix decisions in codebase inspection, runtime measurements, and target desktop protocol specs. Keep release artifacts restricted to repository source and declared assets.
- Organize Rust modules by responsibility and maintenance clarity. Split crates only for concrete dependency, reuse, or build reasons. Delete dead implementations, unused callers, obsolete dependencies, and outdated docs; keep active test fixtures intact.

<!-- CODEGRAPH_START -->
## CodeGraph

When `.codegraph/` exists, use MCP `codegraph_explore` or `codegraph explore "symbols or question"` BEFORE grep/find or reading code to locate or understand it. Query file/symbol names for current source and call paths. Use `rg`/direct reads for omitted, unindexed or explicitly stale content. Without an index, use normal tools; do not create an index unasked.
<!-- CODEGRAPH_END -->

## Architectural boundaries

- `core`: images, annotation geometry/model/transactions, stitching, config, protocol, and OCR core; zero GTK/GDK dependencies. `cli`: GUI-free parsing/local actions plus D-Bus client.
- `desktop/application` orchestrates sessions; `editor` owns shared controllers, coordinate mapping, rendering, and export; `ui` sends operations and renders controls; `platform` adapts surfaces, capture, and desktop integration; `infrastructure` owns bounded jobs, config, and resources.
- Capture and pins share annotation models, parameters, hit testing, history, toolbar/subbar, and rendering. Platform adapters handle desktop input and display rather than parallel editing lifecycles. Undo/redo sensitivity comes from the active session. Dispatch each input once and coalesce wheel events from start to final state.
- Select display and acquisition capabilities independently. Running under XWayland does not imply GNOME recording permission, reliable XFixes coordinates, or compositor window queries.

## Platform and input invariants

- Prefer native Wayland on KDE, niri, and Hyprland: use layer-shell, report unsupported capabilities, and never fall back silently to X11. GNOME's overlay/pin display uses XWayland because layer-shell is absent, while Preferences and manual Save dialogs run as independent native Wayland windows. Support real X11 sessions separately.
- Pins use layer-shell `ON_DEMAND`, never editing-triggered `EXCLUSIVE`. GNOME pins accept keyboard focus by default regardless of toolbar visibility; `pin-gnome-focus=false` disables it across the entire pin lifecycle. Keep X11-specific workarounds inside the X11 adapter.
- GNOME non-focusing pin mode preserves dash-to-panel visibility; focused mode follows standard window activation. Menu hover, open, and close cycles must not flicker the panel. Toolbar toggling must not remap pins; image crop handles appear only when Crop is selected.
- Release temporary input regions, grabs, and surfaces on dismissal, cancellation, screen lock, errors, and exit. Regress three consecutive right-click-menu and external-dismiss cycles: no invisible click-blocking overlay may remain.
- Standard capture defaults to the noninteractive static Screenshot portal plus Clivuu's frozen overlay. Window capture fetches one frame via ScreenCast window selection on every desktop and stops the stream before editing; never use a system static-window dialog or fullscreen fallback.
- Wayland standard capture uses a one-frame stream when cursor capture is enabled so pointer and pixels share the same compositor frame. XFixes is real-X11-only. Scrolling and recording use continuous streams and respect system sharing indicators and native consent without duplicate shutter flashes.
- No production GNOME Shell helper extension or Shell1 D-Bus dependency. Keep test-only observer extensions and zero-Shell1-call regression checks outside release payloads.

## Performance and state

- Reuse textures and caches across drags, zooms, and previews. Avoid per-motion PNG round trips, worker spawning, or synchronous IPC waits. Keep GTK on the main thread; run OCR, stitching, encoding, and blocking I/O in bounded worker jobs.
- Distinguish source physical pixels, desktop logical coordinates, surface scale factors, and cursor hotspots, including negative origins and fractional/mixed DPR. Presentation must preserve source resolution; bitmap pixels and logical text/color-card sizing differ.
- One configuration schema owns all defaults. XDG INI files are grouped by function and persist non-default overrides only; reset deletes overrides. Coalesce background saves and watch external config updates cleanly.
- Cancellation or capture errors must not append failed runs to history or destroy existing pins or scroll documents. Keep verified content exportable. Closing a pin permits recovery; destroying removes it permanently. Unload persisted closed-pin pixels and reload lazily.

## Validation and delivery

- Write temporary build, test, media, and packaging outputs under `/tmp`. Use repository Cargo, Makefile, CMake, and CPack entry points; never hardcode machine-local SDK paths or old hashes.
- GUI tests must run with private HOME, XDG, runtime, and D-Bus directories, never the user's real config, clipboard, autostart, or services. Isolate media discovery with `tests/wireplumber-test.conf` and audit media isolation.
- Write discriminating tests: multiple annotations with single-step undo, rapid input, cancellation/cleanup, visible filenames, and verified encoding. Do not relax frozen baselines solely to pass.
- Rust verification: applicable unit/integration tests, `cargo fmt --all -- --check`, `cargo clippy --workspace --locked --all-targets -- -D warnings`. Locale verification: `node tests/i18n.mjs`. GUI/platform verification: relevant `tests/gnome.py` modes and native niri paths, verifying pixels and latency where needed.
- Distinguish compilation checks, protocol fixtures, nested compositors, and real hardware sessions. Passing on GNOME or niri does not certify KDE, Hyprland, or mixed-DPR displays; state untested boundaries explicitly.
- Release artifacts must bundle the verified CLI and desktop ELF pair, current icons, and docs without test extensions. Verify exact artifact paths and builds; never overwrite the user's installation or enable real autostart unless asked.

## Read on demand

- Annotation, text, cursor, toolbar, or settings rules: [editing/UI contracts](agent-docs/feature-contracts.md#editing-and-ui).
- Paste/pins, scrolling, recording, saving/notifications, or startup rules: matching sections in [feature contracts](agent-docs/feature-contracts.md).
- Architecture, capture pipeline, and lifecycle: [architecture](docs/en/development/architecture.md). Use current source code and contracts as source of truth.
- Shortcuts and selection controls: [shortcuts](docs/en/guide/shortcuts.md).
- Build, install, and packaging workflow: [building](docs/en/development/building.md), [testing](docs/en/development/testing.md), `CMakeLists.txt`, and `packaging/`. Cargo compiles binaries; CMake/CPack handles installation and native packages without Python packaging wrappers.

## Maintain this file

Keep this file at `<= 8 KiB` and under 200 lines, placing critical invariants first. Move feature-specific rules behind conditional links in `agent-docs/`. `CLAUDE.md` imports this file directly; see [loading notes](agent-docs/feature-contracts.md#agent-loading-and-size).

# CLI & D-Bus Control

**English** | [简体中文](../../zh-CN/development/cli.md) · [User Guide](../README.md)

Run `clivuu --help` for the categorized command overview, `clivuu <COMMAND> --help` (or `clivuu help <COMMAND>`, `clivuu config <SUBCOMMAND> --help`) for per-command options and mutual-exclusion rules, `clivuu docs` for manual locations, `clivuu --version` (`-V`) for version information, and `clivuu completions bash|zsh|fish` to generate shell completions. Running `clivuu` with no arguments starts or contacts `clivuu-desktop`. Commands return JSON payloads or exit with status `1` on error.

`status`, `config`, `completions`, `docs`, `help`, `portal`, `--help`, `--version`, and `worker` commands execute without initializing GTK. Options and positionals are validated per command from the declarative specification (`cli/src/spec.rs`), so invalid cross-command flags are rejected immediately.

## D-Bus Interface

`clivuu-desktop` owns the session bus name `io.github.naerelyth.Clivuu.Desktop` with object path `/io/github/naerelyth/Clivuu/Desktop` and interface `io.github.naerelyth.Clivuu.Desktop1`. The `clivuu` binary auto-launches `clivuu-desktop` when the bus name is absent and forwards command requests over D-Bus.

## Capture & Recording Commands

```sh
clivuu snip
clivuu snip --full -o /tmp/desktop.png
clivuu snip --area 100 100 640 480 -o clipboard
clivuu snip --delay 2
clivuu snip --window
clivuu scroll
clivuu record -o /tmp/demo.mp4
clivuu pause-recording
clivuu stop-recording
```

| Option | Behavior |
| :--- | :--- |
| `--area X Y W H` | Explicit desktop rectangle (negative origins allowed; width and height `>= 2`; supported on `snip`, `scroll`, and `record`) |
| `--size W H` | Immediate centered capture with fixed width and height `>= 2` (`snip` only; does not open interactive region selection) |
| `--full` | Full multi-monitor desktop bounding box (`snip` only) |
| `--active-screen` | Active monitor only (`snip` only) |
| `--last` | Last committed capture region (`snip` only) |
| `--window`, `--active-window` | Opens system single-window `ScreenCast` picker across desktops (`snip` only) |
| `--delay SECONDS` | Countdown delay between `0` and `3600` seconds before acquiring frame (`snip` and `record`) |
| `-o`, `--output TARGET` | Output destination (`snip`, `scroll`, and `record`) |

Region/size/full/screen/last/window selectors are mutually exclusive.

**Image output targets** (`snip` / `scroll`): `clipboard`, `file-dialog`, `quick-save`, `pin`, `ocr_clipboard`, `printer` (`lp`), `barcode`, `success`/`silent` (skip primary output while keeping automatic history/backup when configured), `no-auto-save`, or an explicit file path ending in `.png`, `.jpg`, or `.jpeg` (writes directly without opening a chooser). Combined semicolon or `exec(...)` output strings are rejected.

**Recording output targets** (`record -o`): `quick-save`, `file-dialog`, or a file path ending in `.mp4`, `.webm`, or `.gif`. Without `-o`, stopping a `Normal` recording enters the playback editor (or writes immediately in `Quick` mode).

## Paste & Pin Management

```sh
clivuu paste
clivuu paste --clipboard
clivuu paste --plain 'literal text'
clivuu paste --html '<b>Formatted text</b>'
clivuu paste --files /path/to/dir image1.png nested/image2.jpg
clivuu paste --pos 200 160
```

Source flags (`--clipboard`, `--plain`, `--html`, `--files`) are mutually exclusive, and `--pos X Y` sets the initial screen origin for pins opened via `paste`, `reopen-pin`, or `reopen-capture`:
- `--files` accepts `1` to `32` image files. Any directory path inside `--files` sets the working base directory for subsequent relative arguments.
- `--plain` and `--html` accept up to `40,000` characters (`--plain` preserves literal characters without auto-converting color strings into color cards).
- During an open interactive capture session, `clivuu paste` or `clivuu paste --clipboard` triggers the capture `Pin` action (`Ctrl+T`) and notifies the capture session directly. Outside capture, matching sources reveal and flash existing pins; when the clipboard is empty, `paste` does not reopen closed pins (use `reopen-pin`).

| Command | Behavior |
| :--- | :--- |
| `reopen-pin`, `reopen-pins` | Restore last or all closed recoverable pins |
| `close-pins` | Close visible pins in the active group (recoverable) |
| `destroy-closed-pins` | Permanently discard closed-pin history |
| `show-images`, `hide-images`, `toggle-images` | Toggle visibility of active group pins |
| `toggle-click-through`, `no-click-through` | Toggle or clear pointer pass-through across all pins |
| `create-group [NAME] [--color COLOR] [--no-switch] [--prompt]` | Create a pin group (optionally setting a color tag, keeping current group, or opening the dialog) |
| `switch-group [NAME]` | Switch to named group, or cycle forward when omitted |
| `switch-group-prev` | Switch back to previous group |
| `switch-groups` | Open the interactive pin group switcher window |
| `show-group-manager` | Open the graphical pin group manager window |
| `rename-group NAME NEW` | Rename an existing custom pin group |
| `delete-group NAME [--move-to GROUP]` | Delete a custom pin group and destroy or migrate its pins |
| `remove-empty-groups` | Remove inactive empty pin groups |
| `empty-group [NAME]` | Destroy all pins in the active or specified group |
| `groups` | Print pin groups, active/closed counts, and color metadata as JSON |

## History, Utilities & Service Control

| Command | Behavior |
| :--- | :--- |
| `capture-history` | List in-memory completed capture records, newest first |
| `reopen-capture [INDEX]` | Pin record `INDEX` (`0` = newest), or reveal/flash its existing pin independently of clipboard |
| `clear-snip-history` | Clear in-memory capture records, region history, and last region |
| `pick-color` | Open color sampling tool and copy HEX readout |
| `barcode-scan` | Capture a screen region via the desktop service and copy decoded QR/barcode text |
| `barcode-decode FILE` | Decode `FILE` through the desktop service, copy result to clipboard, and print JSON (use `clivuu worker barcode --input FILE` offline without the desktop service) |
| `open-preferences` | Open Preferences window |
| `start`, `status` | Launch desktop service / print JSON with desktop environment, session type, overlay/pin backend, config path, and optional helpers (`ffmpeg`, `ffprobe`, `lp`, OCR engines/runtime) |
| `restart`, `exit` | Flush state and relaunch / stop desktop service |
| `portal -o FILE.png\|FILE.jpg\|FILE.jpeg` | Invoke `org.freedesktop.portal.Screenshot` directly without starting desktop UI (non-overwriting) |
| `enable-hotkeys`, `disable-hotkeys`, `toggle-hotkeys` | Return exit code `1` with guidance explaining that global shortcuts bind in desktop/WM settings |

## Configuration Commands

```sh
clivuu config path
clivuu config dump
clivuu config defaults
clivuu config get theme
clivuu config set theme dark
clivuu config apply '{"theme":"dark","accent-color":"#bd93f9ff"}'
clivuu completions bash > ~/.local/share/bash-completion/completions/clivuu
```

`config` subcommands read and validate keys directly against `core/config-defaults.json`; see [Configuration Schema](configuration.md) for all keys, types, and bounds. `clivuu completions` generates completion scripts dynamically from `cli/src/spec.rs` and the configuration schema.

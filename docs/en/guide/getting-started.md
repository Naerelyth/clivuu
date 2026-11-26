# Getting Started

[User Guide](../README.md) | **English** | [简体中文](../../zh-CN/guide/getting-started.md)

Clivuu splits into two binaries: `clivuu-desktop` (the background service hosting the system tray, overlays, pins, dialogs, and D-Bus interface) and `clivuu` (the command-line entry point used interactively and in desktop hotkey bindings).

## First Launch

Start the background service with no arguments:

```bash
clivuu
```

If `clivuu-desktop` is not already running, the CLI starts it with application ID `io.github.naerelyth.clivuu`, registers the system tray icon, and exports the D-Bus service `io.github.naerelyth.Clivuu.Desktop`. Commands such as `clivuu status`, `clivuu config ...`, `clivuu --help`, and `clivuu --version` run locally without starting GTK (`clivuu status` only queries the service over D-Bus when it is already running).

## Desktop Integration & Hotkeys

Clivuu leaves global hotkeys to your desktop environment or window manager. Bind your system shortcuts directly to `clivuu` commands:

| Suggested Action | Command | Notes |
| :--- | :--- | :--- |
| Interactive capture | `clivuu snip` | Opens the frozen selection overlay |
| Window capture | `clivuu snip --window` | Opens the system `ScreenCast` window picker and captures one frame before editing |
| Pin clipboard | `clivuu paste` | Turns clipboard content into a floating pin (or pins the active capture selection; use `clivuu reopen-pin` to restore closed pins) |
| Scrolling capture | `clivuu scroll` | Lets you select a region and immediately begins stitched capture |
| Region recording | `clivuu record` | Selects a region and opens live recording controls |
| Pick screen color | `clivuu pick-color` | Samples a pixel color to clipboard |
| Toggle pins visibility | `clivuu toggle-images` | Shows or hides all active pins in the current group |
| Preferences | `clivuu open-preferences` | Opens the settings window |

On GNOME Wayland, Wayland compositors (`niri`, `Hyprland`, KDE), and X11, bind these commands under your compositor's custom shortcuts configuration (`Settings -> Keyboard -> Custom Shortcuts` on GNOME/KDE, or compositor config bindings on `niri` / `Hyprland`).

## System Tray Menu

Right-clicking or activating the tray icon opens the service menu:

- **Capture & recording**: `Capture`, `Window capture`, and `Region recording` (replaced in-place by `Pause / resume recording` and `Stop recording` while a recording is active)
- **Pin management**: `Pin clipboard`, `Show / hide pins`, conditional `Restore pointer interaction` (shown only when click-through pins exist in the active group), `Pin groups: <Active>` submenu, and `Pin recovery & cleanup` submenu (appends the closed count when closed pins exist: `Pin most recent capture`, `Reopen last closed pin`, `Reopen all closed pins`, `Close all pins`, `Destroy closed pins`)
- **Service**: `Preferences`, `Restart`, `Quit`

## Desktop Permissions

On Wayland desktops, screen pixels and optional input injection use standard `xdg-desktop-portal` interfaces:

- **Static capture (`Screenshot` portal)**: used for standard `clivuu snip`. Grant portal screenshot permission once if prompted by your desktop.
- **Window & cursor capture (`ScreenCast` portal)**: triggered by `clivuu snip --window`, `clivuu snip --active-window`, and Wayland `capture-cursor=true` screenshots; Clivuu reads one compositor frame and closes the stream before entering the annotation overlay.
- **Scrolling & recording**: maintain continuous `ScreenCast` streams and show your desktop's standard screen-sharing indicator while active.
- **Auto-scroll**: requests `RemoteDesktop` input permission only when auto-scroll is toggled on.

## Switching Language & Autostart

- **Interface language**: set `General -> Interface language` in [Preferences](preferences.md), run `clivuu config set language zh-CN` (or `en`), or set `CLIVUU_LANGUAGE=zh-CN` in the environment.
- **Run at login**: enable `Run at login` in `Preferences -> General` to create a standard XDG Autostart `.desktop` entry (disabled by default).

## Verify Your Setup

Run a quick health check:

```bash
clivuu status
```

This prints JSON reporting service state (`running`, `pid`, `ready`, `busy`, `recording`, `scrolling`), pin/group counts and metadata (`pins`, `closedPins`, `group`, `groups`), and detected display/session `capabilities` (`backend` such as `GdkWaylandDisplay` or `GdkX11Display`, `capture`, `non_focusing_pins`, `smart_windows`, `scrolling`, `recording`). Use `clivuu config path` to print the active config file path.

## Next Steps

- Take and customize screenshots in [Screenshot Capture](capture.md)
- Draw, highlight, add numbered steps, and type text in [Annotations & Editing](annotations.md)
- Keep references on top of other windows in [Floating Pins](pins.md)

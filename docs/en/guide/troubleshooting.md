# Troubleshooting

[User Guide](../README.md) | **English** | [简体中文](../../zh-CN/guide/troubleshooting.md)

Use `clivuu status` first when diagnosing unexpected behavior. It prints desktop environment, session type (`wayland` or `x11`), active overlay/pin backend, loaded config path, and detected optional helpers without opening UI windows.

## Portal & Capture Permission Errors

**Symptom**: Screenshot or window capture is denied or fails immediately.
**Checks**:
- Confirm `xdg-desktop-portal` and your compositor backend (`xdg-desktop-portal-gnome`, `xdg-desktop-portal-kde`, `xdg-desktop-portal-wlr`, or `xdg-desktop-portal-gtk`) are running in your D-Bus session.
- If a portal authorization prompt was previously denied, revoke/reset the permission in your desktop's Privacy/App Permissions settings and run `clivuu snip` or `clivuu snip --window` again to trigger a native authorization prompt.
- Verify the noninteractive portal directly outside `clivuu-desktop`:
  ```bash
  clivuu portal -o /tmp/portal-check.png
  ```

## Window Hover Detection on Wayland

**Symptom**: Moving the pointer before dragging selects full monitor boundaries instead of individual application windows.
**Cause**: Window rectangle queries require compositor IPC supported on **Hyprland** (`hyprctl` + `HYPRLAND_INSTANCE_SIGNATURE`) and **niri** (`niri msg` + `NIRI_SOCKET`). Other Wayland compositors and GNOME do not expose global window rectangles to unprivileged clients.
**Solution**: Use `clivuu snip --window` (or `Window capture` from the tray) to pick a single window through the system `ScreenCast` window picker.

## Recording or Scrolling Shows a Sharing Indicator

On Wayland desktops, scrolling capture, screen recording, `--window` capture, and `capture-cursor=true` screenshots use `xdg-desktop-portal` `ScreenCast` (on GNOME, `clivuu snip --window` and scrolling/recording interact with `org.gnome.Mutter.ScreenCast` directly without falling back to full-screen capture). Your compositor or shell controls the top-bar/tray sharing indicator and native source picker dialog. Standard `clivuu snip` with cursor capture off uses static `Screenshot` instead of a stream.

## GNOME Pin Keyboard Focus or Panel Behavior

**Symptom**: Shortcuts or `Shift`/`Ctrl` wheel modifiers do not reach a pin on GNOME, or selecting a pin changes `dash-to-panel` visibility.
**Details**:
- By default (`pin-gnome-focus=true`), GNOME pins run through XWayland and accept keyboard focus directly so arrow nudges, `1–6` transforms, and `Ctrl+wheel` work without opening the toolbar.
- If you prefer pins never to activate window focus on GNOME (preserving auto-hide panel visibility), turn off `Allow pins to take keyboard focus` in `Preferences -> Pins -> Behavior` (`clivuu config set pin-gnome-focus false`). When `false`, pins refuse keyboard focus across their entire lifecycle, including when the toolbar is open; `Right-click -> Show toolbar` provides pointer-driven buttons only, so re-enable `pin-gnome-focus` when keyboard shortcuts or text typing are needed.

## Invisible Input Block After Cancelling Overlay or Menu

All overlays, pin popups, and selection surfaces release grabs and input regions on `Esc`, right-click cancellation, errors, and quit. If you suspect a stuck UI state (for instance after a compositor crash or theme reload), run:

```bash
clivuu no-click-through    # Restores pointer interaction on any click-through pins
clivuu restart             # Persists closed pins/config and restarts clivuu-desktop
```

## Missing OCR or Barcode Results

- **OCR error**: open `Preferences -> OCR` to check runtime status, installed models, and error details. Built-in `PP-OCRv5` requires an ONNX Runtime (`libonnxruntime`) C API v17+ library and `curl` for downloads (set `ocr-download-mirror`, place files manually in the offline runtime/model directories, or override paths with environment variables). When using **System Tesseract**, install `libtesseract5` (`tesseract`), install matching `.traineddata` language packs (set `$TESSDATA_PREFIX` if custom), and rescan in `Preferences -> OCR`.
- **Barcode scanning**: run `clivuu worker barcode --input /path/to/code.png` to verify `rxing` barcode decoding offline without the desktop service.

## Recording Stops Early or Fails on Export

- **Memory ceiling reached**: when `recording-storage=temporary`, capture stops before exceeding `recording-temp-limit-mib` (default `512 MiB`, configurable `16–16384 MiB`). Switch `Preferences -> Save -> Recording -> Recording strategy` to `Video folder` for long recordings.
- **No audio tracks or encoder failure**: confirm GStreamer plugins (`gstreamer1.0-pipewire`, `gstreamer1.0-pulseaudio`, `gstreamer1.0-plugins-base`, `gstreamer1.0-plugins-good`, or Fedora `pipewire-gstreamer`, `gstreamer1-plugins-good`, `gstreamer1-plugins-bad-free`, `ffmpeg-free`) and `ffmpeg` (`ffprobe`) are installed. Test in isolated environments with private Pulse/PipeWire sinks rather than capturing physical audio hardware during tests.

## Related Manuals

- [Getting Started](getting-started.md)
- [Preferences](preferences.md)
- [Testing & Verification](../development/testing.md)

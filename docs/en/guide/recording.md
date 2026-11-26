# Screen Recording & Playback

[User Guide](../README.md) | **English** | [简体中文](../../zh-CN/guide/recording.md)

Clivuu records a screen region into a lossless video intermediate (`VP9` Y444 in Matroska) paired with separate lossless `FLAC` system-audio and microphone tracks (`48 kHz` stereo), then lets you trim, crop, change speed, balance tracks, and export to `MP4`, `WebM`, or `GIF`.

## Entry Points

- **Tray**: click `Region recording`, drag a region, and release directly into Ready controls.
- **Screenshot toolbar**: select an area during capture, then click `Region recording` or press `V`.
- **CLI**:
  - `clivuu record` (interactive selection into Ready controls)
  - `clivuu record --area X Y W H` (skips Ready setup and starts recording immediately in `Quick` mode on explicit coordinates)
  - `clivuu record --area X Y W H -o FILE.mp4|FILE.webm|FILE.gif` (also runs in `Quick` mode and writes the requested format upon stopping)
  - While recording: `clivuu pause-recording`, `clivuu stop-recording`, or use the tray items `Pause / resume recording` and `Stop recording`.

## Ready Controls & Capture Setup

Before recording starts, you can adjust the region by dragging its edges/corners or dragging `Move recording region`, press `Enter` to start, press `Esc` to discard immediately, or configure options on the floating toolbar:

- **FPS**: `5`, `16`, `24` (default), `30`, `60`.
- **Countdown (`Delay`)**: `0`, `1`, `2`, `3`, `5`, `10` seconds.
- **Mode**:
  - `Normal recording`: stops into the frameless playback editor before exporting.
  - `Quick recording`: skips the playback editor and writes an `MP4` file using Quick save settings upon stopping.
- **Tracks & Cursor**:
  - `Show mouse cursor` toggle (defaults on, independent of screenshot `capture-cursor`)
  - `System audio` toggle (defaults on when a monitor audio source is detected)
  - `Microphone` toggle (defaults on when an input source is detected)

All Ready setup options (FPS, delay, mode, region geometry, cursor, and audio tracks) lock when recording begins. Cancelling during Ready setup, countdown, portal authorization, or explicit discard exits without error notifications.

## Active Recording & Storage Behavior

- During recording, Clivuu hides controls that overlap the recording frame (including while dragging), keeping tray actions and `clivuu pause-recording` / `clivuu stop-recording` functional.
- **Pause / Resume**: paused wall-clock duration is omitted from both the playback timeline and exported timestamps.
- **Recording strategy** (`Preferences -> Save -> Recording -> Recording strategy`):
  - `Memory first` (`temporary`, `/tmp` RAM-backed storage capped by `recording-temp-limit-mib`, default `512 MiB`, configurable `16–16384 MiB`): stops capture before reaching the cap or when under `16 MiB` free space remains, keeping the recorded clip ready to save before discarding.
  - `Video folder` (`direct`): writes intermediate files inside your configured video save location (`video-directory`) to remove RAM pressure for longer clips.

Restarting or discarding while recording asks for confirmation before deleting captured intermediate data.

## Playback Editor (`Normal recording`)

Stopping a `Normal` recording opens the frameless playback editor over the recorded region:

- **Play / Pause**: click Play/Pause, click inside the video frame, or press `Space`.
- **Timeline trim**: drag start and end trim handles along the scrub bar.
- **Canvas crop**: drag the edges or corners of the playback frame directly to crop output pixels (`VP9` intermediate holds native pixels without pre-scaling).
- **Speed**: choose from `0.2x`, `0.25x`, `0.5x`, `0.75x`, `0.8x`, `1.0x`, `1.25x`, `1.5x`, `1.75x`, `2.0x`.
- **Recorded audio**: expand `Recorded audio` to adjust `System audio` and `Microphone` gain (`0%–200%`, click the `%` readout to reset to `100%`) or mute each track.
- **Reset edits**: clears trim, crop, speed, and volume changes back to the recorded clip.
- **Export & Finish**:
  - Formats: `MP4` (H.264/MPEG-4 + AAC), `WebM` (VP9 + Opus), or `GIF` (video only; retains recorded audio tracks for subsequent exports).
  - `Save recording` (`Ctrl+S`) opens the native GTK Save file dialog, and `Quick save` (`Ctrl+Shift+S`) writes directly to your configured quick video folder.
  - `Copy file` (`Ctrl+C`): exports and places the video file URI onto the clipboard.
  - `Restart recording`: asks for confirmation, discards the recorded clip and edits, and returns to Ready controls.
  - `Close playback` (`Esc`): always asks for confirmation before discarding the recorded clip and edits (even after an earlier export); export failures preserve your intermediate source so you can retry with a different path or format.

## Related Manuals

- [Saving, Naming & Notifications](saving.md)
- [Preferences](preferences.md)
- [Troubleshooting](troubleshooting.md)

# Saving, Naming & Notifications

[User Guide](../README.md) | **English** | [简体中文](../../zh-CN/guide/saving.md)

Clivuu provides manual saving (through a single Clivuu-managed GTK Save dialog), one-key quick saving, and optional automatic capture backups, sharing one filename template engine across images and recordings.

## Save Modes

| Mode | Trigger | Behavior |
| :--- | :--- | :--- |
| **Manual save** | `Ctrl+S`, toolbar `Save image`, pin menu `Save image`, or playback `Save` | Opens the native GTK file dialog pre-filled with the expanded manual template. Selecting PNG, JPEG, or a recording format updates the file extension live inside the same dialog |
| **Quick save** | `Ctrl+Shift+S`, `-o quick-save`, or `Quick` recording mode | Saves immediately into `image-quick-directory` (default `$XDG_PICTURES_DIR/Clivuu`) or `video-directory` (default `$XDG_VIDEOS_DIR/Clivuu`) without prompting |
| **Auto save (images)** | When `image-auto-save=true` | Automatically writes a second copy to `image-auto-directory` whenever a screenshot finishes to clipboard, pin, or manual save. Cancelled captures never create automatic copies |

Missing quick-save or auto-save subdirectories are created automatically. If a filename already exists in quick or automatic save mode, Clivuu appends an incrementing number instead of overwriting existing files.

## Supported Output Formats & Quality

- **Images**: `PNG` (`.png`) and `JPEG` (`.jpg` or `.jpeg`). Choosing `JPEG` defaults to `.jpg` while preserving explicitly typed `.jpeg`. Encoded bytes always match the final filename extension.
- **Image quality (`image-quality`)**: shared `0–100` slider (default `100`). `100` encodes lossless fast PNG (`0–99` applies adaptive compression for smaller files) and controls JPEG compression level (`1–100`; transparent regions are composited onto white).
- **Remember last manual format (`image-remember-extension`)**: enabled by default so subsequent Save dialogs keep your previously chosen format.
- **Videos**: `MP4`, `WebM`, and `GIF` during playback export (`Quick` mode writes `MP4`).

## Filename Template Tokens

Configure templates in `Preferences -> Save` (`image-manual-name`, `image-quick-name`, `image-auto-name`, `video-manual-name`, `video-quick-name`). Default templates use `Clivuu_$yyyy-MM-dd_HH-mm-ss$` without needing an explicit extension (previews and outputs default to `.png` for images and `.mp4` for recordings). Date/time format runs inside `$...$` may combine separators directly (such as `$yyyy-MM-dd_HH-mm-ss$`) or form relative subdirectories such as `$yyyy$/$MM$/Clivuu_$HH$$mm$$ss$`; path traversal outside the target directory (`..`, leading `/`) is rejected.

| Token | Meaning | Example |
| :--- | :--- | :--- |
| `$yyyy$` / `$yy$` | Year (4-digit / 2-digit) | `2026` / `26` |
| `$MMMM$` / `$MMM$` / `$MM$` / `$M$` | Month (full English name / short / zero-padded / bare) | `November` / `Nov` / `11` / `11` |
| `$dddd$` / `$ddd$` / `$dd$` / `$d$` | Day & weekday (full English weekday / short / zero-padded day / bare day) | `Saturday` / `Sat` / `14` / `14` |
| `$HH$` / `$H$` | Hour (`00–23` / `0–23`) | `09` / `9` |
| `$mm$` / `$m$` | Minute (`00–59` / `0–59`) | `05` / `5` |
| `$ss$` / `$s$` | Second (`00–59` / `0–59`) | `08` / `8` |
| `$zzz$` / `$z$` | Millisecond (`000–999` / `0–999`) | `042` / `42` |
| `$t$` | Locale/session suffix token | `AM` |
| `$uuid$` | Random UUID v4 | `550e8400-e29b-41d4-...` |
| `$unix$` / `$unix_ms$` | Unix epoch seconds / milliseconds | `1794614400` |
| `%username%` | Current OS user name | `alex` |
| `%computername%` | Machine host name | `workstation` |
| `%os%` | Operating system identifier | `linux` |

Hover over the info button at the right end of any `Preferences -> Save` file-name input to view usage hints, or click it to open **Naming rules** directly and copy tokens.

## Desktop Notifications & "Show in Files"

When `image-save-notify` or `video-save-notify` is enabled, quick saves emit a system notification displaying the filename and path:

- Saved image notifications attach a preview thumbnail of the exported image.
- Clicking **Show in Files** highlights the saved file via D-Bus (`org.freedesktop.FileManager1`) or opens the parent folder with the default directory handler (`Show in Files` matches the desktop session system locale).

## Related Manuals

- [Screen Recording & Playback](recording.md)
- [Preferences](preferences.md)
- [Configuration Schema](../development/configuration.md)

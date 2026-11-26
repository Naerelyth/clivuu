# Configuration Schema

**English** | [简体中文](../../zh-CN/development/configuration.md) · [User Guide](../README.md)

Configuration reads and writes `$XDG_CONFIG_HOME/clivuu/config.ini` (default `~/.config/clivuu/config.ini`). Entries use JSON syntax grouped into functional INI sections and persist non-default overrides only. Resetting a Preferences section deletes overrides in that section alone.

Use `clivuu config dump` for merged runtime settings, `defaults` for schema defaults, `path` for the resolved INI file path, `get KEY` / `set KEY VALUE` for single values, and `apply JSON` for validated bulk updates. The desktop service monitors external file edits and coalesces rapid GUI changes into background writes.

The reference tables below reflect `core/config-defaults.json`. Empty save directory keys resolve dynamically to `$XDG_PICTURES_DIR/Clivuu` and `$XDG_VIDEOS_DIR/Clivuu` (with empty `image-auto-directory` falling back to `image-quick-directory`). Inactive pins use `pin-shadow-tint`, while active or selected pins follow `accent-color`. Shortcut arrays store user preferences without binding system-wide keys inside Clivuu (bind global shortcuts to `clivuu` commands in your desktop environment or compositor). Autostart uses a standard XDG Autostart `.desktop` entry rather than an INI flag.

Color values store `#RRGGBBAA` hex strings (inputs in `#RRGGBB` format become opaque `#RRGGBBff`).


## `general`

| Key | Default | Validation |
| --- | --- | --- |
| `language` | `"system"` | `system` + registered languages |
| `preferences-font` | `""` | Pango font description, empty follows the GTK system font |
| `theme` | `"system"` | `choices: ["system","light","dark"]` |
| `accent-color` | `"#bd93f9ff"` | `color: true`; `alpha: true` |

## `shortcuts`

| Key | Default | Validation |
| --- | --- | --- |
| `snip` | `["<Super><Shift>a"]` | Schema type |
| `paste` | `["<Super><Shift>v"]` | Schema type |
| `toggle-images` | `["<Super><Shift>h"]` | Schema type |
| `stop-recording` | `["<Super><Shift>r"]` | Schema type |

## `ocr`

| Key | Default | Validation |
| --- | --- | --- |
| `ocr-engine` | `"auto"` | `choices: ["auto","paddle","tesseract"]` |
| `ocr-model` | `"chinese"` | `choices: ["chinese","latin","korean","cyrillic","arabic","greek","thai"]` |
| `ocr-languages` | `"eng+chi_sim"` | Schema type |
| `ocr-quick-copy` | `false` | Schema type |
| `ocr-smart-paragraphs` | `true` | Schema type |
| `ocr-download-mirror` | `""` | Schema type |

## `save.video`

| Key | Default | Validation |
| --- | --- | --- |
| `video-directory` | `""` | `absolute: true` |
| `video-manual-name` | `"Clivuu_$yyyy-MM-dd_HH-mm-ss$"` | `filename: "video"` |
| `video-quick-name` | `"Clivuu_$yyyy-MM-dd_HH-mm-ss$"` | `filename: "video"` |
| `video-save-notify` | `true` | Schema type |

## `pins`

| Key | Default | Validation |
| --- | --- | --- |
| `pin-storage` | `"persistent"` | `choices: ["persistent","memory"]` |

## `scrolling`

| Key | Default | Validation |
| --- | --- | --- |
| `max-scroll-frames` | `40` | `min: 2`; `max: 200` |
| `scroll-interval` | `700` | `min: 300`; `max: 3000` |
| `scroll-auto-crop` | `false` | Schema type |

## `recording`

| Key | Default | Validation |
| --- | --- | --- |
| `recording-storage` | `"temporary"` | `choices: ["temporary","direct"]` |
| `recording-temp-limit-mib` | `512` | `min: 16`; `max: 16384` |

## `capture.display`

| Key | Default | Validation |
| --- | --- | --- |
| `capture-mask-color` | `"#0000008c"` | `color: true`; `alpha: true` |
| `capture-anchor-color` | `"#ffffffff"` | `color: true`; `alpha: true` |
| `capture-show-anchors` | `true` | Schema type |
| `capture-magnifier-mask` | `true` | Schema type |
| `capture-magnifier-border` | `true` | Schema type |
| `capture-magnifier-anchors` | `false` | Schema type |
| `capture-crosshair` | `false` | Schema type |
| `capture-disable-guides` | `false` | Schema type |
| `capture-show-tips` | `true` | Schema type |
| `capture-border-width` | `2` | `min: 1`; `max: 8` |

## `capture.behavior`

| Key | Default | Validation |
| --- | --- | --- |
| `capture-detect-windows` | `true` | Schema type |
| `capture-hide-1x1` | `true` | Schema type |
| `capture-cursor` | `false` | Schema type |
| `capture-hide-pins` | `true` | Schema type |
| `capture-history-loop` | `false` | Schema type |
| `capture-history-areas` | `8` | `min: 0`; `max: 100` |
| `capture-history-records` | `20` | `min: 0`; `max: 100` |

## `annotation`

| Key | Default | Validation |
| --- | --- | --- |
| `custom-colors` | `[]` | `max_items: 16`; `color_items: true` |

## `save.image`

| Key | Default | Validation |
| --- | --- | --- |
| `image-manual-name` | `"Clivuu_$yyyy-MM-dd_HH-mm-ss$"` | `filename: "image"` |
| `image-quick-name` | `"Clivuu_$yyyy-MM-dd_HH-mm-ss$"` | `filename: "image"` |
| `image-auto-name` | `"Clivuu_$yyyy-MM-dd_HH-mm-ss$"` | `filename: "image"` |
| `image-quick-directory` | `""` | `absolute: true` |
| `image-auto-directory` | `""` | `absolute: true` |
| `image-auto-save` | `false` | Schema type |
| `image-save-notify` | `true` | Schema type |
| `image-remember-extension` | `true` | Schema type |
| `image-last-extension` | `"png"` | `choices: ["png","jpg","jpeg"]` |
| `image-quality` | `100` | `min: 0`; `max: 100` |

## `pins.display`

| Key | Default | Validation |
| --- | --- | --- |
| `pin-shadow` | `true` | Schema type |
| `pin-shadow-tint` | `"#909090ff"` | `color: true`; `alpha: true` |
| `pin-opacity` | `100` | `min: 10`; `max: 100` |
| `pin-max-size` | `12000` | `min: 100`; `max: 100000` |
| `pin-thumbnail-width` | `100` | `min: 24`; `max: 1000` |
| `pin-thumbnail-height` | `100` | `min: 24`; `max: 1000` |

## `pins.behavior`

| Key | Default | Validation |
| --- | --- | --- |
| `pin-use-snip-position` | `true` | Schema type |
| `pin-zoom-at-cursor` | `false` | Schema type |
| `pin-smooth-scaling` | `true` | Schema type |
| `pin-reset-thumbnail` | `true` | Schema type |
| `pin-translucent-flash` | `true` | Schema type |
| `pin-flash-group` | `true` | Schema type |
| `pin-hide-on-flash` | `false` | Schema type |
| `pin-preserve-drag` | `false` | Schema type |
| `pin-confirm-close` | `true` | Schema type |
| `pin-confirm-destroy` | `true` | Schema type |
| `pin-flash-new` | `false` | Schema type |
| `pin-activate-new` | `true` | Schema type |
| `pin-gnome-focus` | `true` | Schema type (effective on GNOME only) |
| `pin-closed-limit` | `1` | `min: 0`; `max: 128` |

## `pins.conversion`

| Key | Default | Validation |
| --- | --- | --- |
| `pin-text-enabled` | `true` | Schema type |
| `pin-text-ignore-html` | `false` | Schema type |
| `pin-text-skip-paths` | `true` | Schema type |
| `pin-text-margin` | `20` | `min: 0`; `max: 100` |
| `pin-text-max-width` | `1200` | `min: 100`; `max: 10000` |
| `pin-text-font` | `"Sans 9"` | Schema type |
| `pin-text-background` | `"#ffffffff"` | `color: true`; `alpha: true` |
| `pin-text-foreground` | `"#000000ff"` | `color: true`; `alpha: true` |


## `control.mouse`

Pin gestures accept up to two modifiers (`ctrl`, `shift`, `alt`, `super`) followed by a trigger. Scaling/opacity accept `wheel` or `none`; other actions accept `double-left`, `middle`, `right` or `none`. Bindings match modifiers exactly, with earlier rows taking priority. `none` disables the mouse gesture; the separate keys option can still enable +/− with the same modifiers.

| Key | Default | Validation |
| --- | --- | --- |
| `pin-mouse-zoom` | `"wheel"` | Pin binding |
| `pin-mouse-opacity` | `"ctrl+wheel"` | Pin binding |
| `pin-mouse-close` | `"double-left"` | Pin binding |
| `pin-mouse-reset` | `"middle"` | Pin binding |
| `pin-mouse-thumbnail` | `"shift+double-left"` | Pin binding |
| `pin-mouse-copy-text` | `"shift+right"` | Pin binding |
| `pin-mouse-destroy` | `"ctrl+double-left"` | Pin binding |
| `pin-mouse-zoom-keys` | `true` | Schema type |
| `pin-mouse-opacity-keys` | `true` | Schema type |

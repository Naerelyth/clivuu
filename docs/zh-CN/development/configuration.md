# 配置项参考

[English](../../en/development/configuration.md) | **简体中文** · [使用手册](../README.md)

配置文件位于 `$XDG_CONFIG_HOME/clivuu/config.ini`（默认 `~/.config/clivuu/config.ini`）。条目按功能归入 INI 分节，值采用 JSON 语法，仅持久化相对默认值发生变更的配置。执行分节重置时，仅移除该分节的自定义覆盖值。

通过 `clivuu config dump` 查看当前生效配置，`defaults` 查看默认架构，`path` 查看路径，`get KEY` / `set KEY VALUE` 读写单项，`apply JSON` 校验并批量写入对象。后台服务监听外部修改并在后台合并写入请求。

下表对应 `core/config-defaults.json`。保存目录为空时动态使用 `$XDG_PICTURES_DIR/Clivuu` 或 `$XDG_VIDEOS_DIR/Clivuu`（其中 `image-auto-directory` 为空时回退至 `image-quick-directory`）；未选中贴图使用 `pin-shadow-tint`，活动或选中贴图使用 `accent-color`；快捷键数组仅记录偏好设置，全局快捷键需在桌面或窗口管理器中绑定；登录自启动由标准 XDG Autostart `.desktop` 文件管理，不作为 INI 键存在。

颜色值统一保存为 `#RRGGBBAA` 十六进制格式（输入 `#RRGGBB` 会自动补充不透明通道成为 `#RRGGBBff`）。


## `general`

| 配置键 | 默认值 | 校验 |
| --- | --- | --- |
| `language` | `"system"` | `system` 与已注册语言 |
| `preferences-font` | `""` | Pango 字体描述，空值跟随 GTK 系统字体 |
| `theme` | `"system"` | `choices: ["system","light","dark"]` |
| `accent-color` | `"#bd93f9ff"` | `color: true`; `alpha: true` |

## `shortcuts`

| 配置键 | 默认值 | 校验 |
| --- | --- | --- |
| `snip` | `["<Super><Shift>a"]` | 遵循值类型 |
| `paste` | `["<Super><Shift>v"]` | 遵循值类型 |
| `toggle-images` | `["<Super><Shift>h"]` | 遵循值类型 |
| `stop-recording` | `["<Super><Shift>r"]` | 遵循值类型 |

## `ocr`

| 配置键 | 默认值 | 校验 |
| --- | --- | --- |
| `ocr-engine` | `"auto"` | `choices: ["auto","paddle","tesseract"]` |
| `ocr-model` | `"chinese"` | `choices: ["chinese","latin","korean","cyrillic","arabic","greek","thai"]` |
| `ocr-languages` | `"eng+chi_sim"` | 遵循值类型 |
| `ocr-quick-copy` | `false` | 遵循值类型 |
| `ocr-smart-paragraphs` | `true` | 遵循值类型 |
| `ocr-download-mirror` | `""` | 遵循值类型 |

## `save.video`

| 配置键 | 默认值 | 校验 |
| --- | --- | --- |
| `video-directory` | `""` | `absolute: true` |
| `video-manual-name` | `"Clivuu_$yyyy-MM-dd_HH-mm-ss$"` | `filename: "video"` |
| `video-quick-name` | `"Clivuu_$yyyy-MM-dd_HH-mm-ss$"` | `filename: "video"` |
| `video-save-notify` | `true` | 遵循值类型 |

## `pins`

| 配置键 | 默认值 | 校验 |
| --- | --- | --- |
| `pin-storage` | `"persistent"` | `choices: ["persistent","memory"]` |

## `scrolling`

| 配置键 | 默认值 | 校验 |
| --- | --- | --- |
| `max-scroll-frames` | `40` | `min: 2`; `max: 200` |
| `scroll-interval` | `700` | `min: 300`; `max: 3000` |
| `scroll-auto-crop` | `false` | 遵循值类型 |

## `recording`

| 配置键 | 默认值 | 校验 |
| --- | --- | --- |
| `recording-storage` | `"temporary"` | `choices: ["temporary","direct"]` |
| `recording-temp-limit-mib` | `512` | `min: 16`; `max: 16384` |

## `capture.display`

| 配置键 | 默认值 | 校验 |
| --- | --- | --- |
| `capture-mask-color` | `"#0000008c"` | `color: true`; `alpha: true` |
| `capture-anchor-color` | `"#ffffffff"` | `color: true`; `alpha: true` |
| `capture-show-anchors` | `true` | 遵循值类型 |
| `capture-magnifier-mask` | `true` | 遵循值类型 |
| `capture-magnifier-border` | `true` | 遵循值类型 |
| `capture-magnifier-anchors` | `false` | 遵循值类型 |
| `capture-crosshair` | `false` | 遵循值类型 |
| `capture-disable-guides` | `false` | 遵循值类型 |
| `capture-show-tips` | `true` | 遵循值类型 |
| `capture-border-width` | `2` | `min: 1`; `max: 8` |

## `capture.behavior`

| 配置键 | 默认值 | 校验 |
| --- | --- | --- |
| `capture-detect-windows` | `true` | 遵循值类型 |
| `capture-hide-1x1` | `true` | 遵循值类型 |
| `capture-cursor` | `false` | 遵循值类型 |
| `capture-hide-pins` | `true` | 遵循值类型 |
| `capture-history-loop` | `false` | 遵循值类型 |
| `capture-history-areas` | `8` | `min: 0`; `max: 100` |
| `capture-history-records` | `20` | `min: 0`; `max: 100` |

## `annotation`

| 配置键 | 默认值 | 校验 |
| --- | --- | --- |
| `custom-colors` | `[]` | `max_items: 16`; `color_items: true` |

## `save.image`

| 配置键 | 默认值 | 校验 |
| --- | --- | --- |
| `image-manual-name` | `"Clivuu_$yyyy-MM-dd_HH-mm-ss$"` | `filename: "image"` |
| `image-quick-name` | `"Clivuu_$yyyy-MM-dd_HH-mm-ss$"` | `filename: "image"` |
| `image-auto-name` | `"Clivuu_$yyyy-MM-dd_HH-mm-ss$"` | `filename: "image"` |
| `image-quick-directory` | `""` | `absolute: true` |
| `image-auto-directory` | `""` | `absolute: true` |
| `image-auto-save` | `false` | 遵循值类型 |
| `image-save-notify` | `true` | 遵循值类型 |
| `image-remember-extension` | `true` | 遵循值类型 |
| `image-last-extension` | `"png"` | `choices: ["png","jpg","jpeg"]` |
| `image-quality` | `100` | `min: 0`; `max: 100` |

## `pins.display`

| 配置键 | 默认值 | 校验 |
| --- | --- | --- |
| `pin-shadow` | `true` | 遵循值类型 |
| `pin-shadow-tint` | `"#909090ff"` | `color: true`; `alpha: true` |
| `pin-opacity` | `100` | `min: 10`; `max: 100` |
| `pin-max-size` | `12000` | `min: 100`; `max: 100000` |
| `pin-thumbnail-width` | `100` | `min: 24`; `max: 1000` |
| `pin-thumbnail-height` | `100` | `min: 24`; `max: 1000` |

## `pins.behavior`

| 配置键 | 默认值 | 校验 |
| --- | --- | --- |
| `pin-use-snip-position` | `true` | 遵循值类型 |
| `pin-zoom-at-cursor` | `false` | 遵循值类型 |
| `pin-smooth-scaling` | `true` | 遵循值类型 |
| `pin-reset-thumbnail` | `true` | 遵循值类型 |
| `pin-translucent-flash` | `true` | 遵循值类型 |
| `pin-flash-group` | `true` | 遵循值类型 |
| `pin-hide-on-flash` | `false` | 遵循值类型 |
| `pin-preserve-drag` | `false` | 遵循值类型 |
| `pin-confirm-close` | `true` | 遵循值类型 |
| `pin-confirm-destroy` | `true` | 遵循值类型 |
| `pin-flash-new` | `false` | 遵循值类型 |
| `pin-activate-new` | `true` | 遵循值类型 |
| `pin-gnome-focus` | `true` | 遵循值类型（仅在 GNOME 生效） |
| `pin-closed-limit` | `1` | `min: 0`; `max: 128` |

## `pins.conversion`

| 配置键 | 默认值 | 校验 |
| --- | --- | --- |
| `pin-text-enabled` | `true` | 遵循值类型 |
| `pin-text-ignore-html` | `false` | 遵循值类型 |
| `pin-text-skip-paths` | `true` | 遵循值类型 |
| `pin-text-margin` | `20` | `min: 0`; `max: 100` |
| `pin-text-max-width` | `1200` | `min: 100`; `max: 10000` |
| `pin-text-font` | `"Sans 9"` | 遵循值类型 |
| `pin-text-background` | `"#ffffffff"` | `color: true`; `alpha: true` |
| `pin-text-foreground` | `"#000000ff"` | `color: true`; `alpha: true` |


## `control.mouse`

贴图鼠标绑定可包含至多两个修饰键（`ctrl`、`shift`、`alt`、`super`），末尾为触发方式。缩放与透明度支持 `wheel` 或 `none`；其他操作支持 `double-left`、`middle`、`right` 或 `none`。修饰键须完全匹配，冲突时靠前条目优先。`none` 禁用鼠标操作，独立的备选按键选项仍可启用使用相同修饰键的 +/− 按键。

| 配置键 | 默认值 | 校验 |
| --- | --- | --- |
| `pin-mouse-zoom` | `"wheel"` | 贴图鼠标绑定 |
| `pin-mouse-opacity` | `"ctrl+wheel"` | 贴图鼠标绑定 |
| `pin-mouse-close` | `"double-left"` | 贴图鼠标绑定 |
| `pin-mouse-reset` | `"middle"` | 贴图鼠标绑定 |
| `pin-mouse-thumbnail` | `"shift+double-left"` | 贴图鼠标绑定 |
| `pin-mouse-copy-text` | `"shift+right"` | 贴图鼠标绑定 |
| `pin-mouse-destroy` | `"ctrl+double-left"` | 贴图鼠标绑定 |
| `pin-mouse-zoom-keys` | `true` | 遵循值类型 |
| `pin-mouse-opacity-keys` | `true` | 遵循值类型 |

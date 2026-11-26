# 首选项说明

[使用手册](../README.md) | [English](../../en/guide/preferences.md) | **简体中文**

可通过托盘菜单 `Preferences`、贴图快捷键 `Ctrl+Shift+P` 或命令行 `clivuu open-preferences` 打开设置窗口。首选项作为独立的普通桌面窗口运行（Wayland 会话下使用原生 Wayland），每次打开默认展示 `通用` 页面；分节重置按钮仅重置当前可见子页面内容并在确认后生效。

## 通用（`General`）

- **界面外观**：
  - `界面语言`（`language`）：`System default`（跟随系统）、`English` 或 `简体中文`（选项使用各语言母语名称展示）。
  - `外观`（`theme`）：`Follow system`（跟随系统）、`Light`（浅色）或 `Dark`（深色 Dracula 配色）。
  - `设置界面字体`（`preferences-font`）：自定义 Pango 字体（留空时沿用 GTK 系统字体），仅用于设置窗口及其对话框。
  - `强调色`（`accent-color`）：默认 `#bd93f9ff`，同行右侧带还原按钮；作用于激活工具按钮、截图选区边框、活跃贴图光晕及界面控件。
- **启动**：
  - `登录后自动启动`：直接读写标准 XDG Autostart `.desktop` 文件（默认关闭）。

## 屏幕截图（`Screen capture`）

分为 **显示（`Display`）** 与 **行为（`Behavior`）** 两个子页：

- **显示**：
  - 选区边框宽度（`capture-border-width`，`1–8 px`）与遮罩颜色（`capture-mask-color`）
  - 缩放锚点显隐与边框色（`capture-show-anchors`、`capture-anchor-color`）
  - 放大镜内部元素开关（`capture-magnifier-mask`、`capture-magnifier-border`、`capture-magnifier-anchors`）
  - 十字准线（`capture-crosshair`）、关闭对齐辅助线（`capture-disable-guides`）及操作提示栏（`capture-show-tips`）
- **行为**：
  - 智能窗口检测（`capture-detect-windows`，Hyprland 与 niri）、隐藏 `1x1` 提示（`capture-hide-1x1`）、捕获光标（`capture-cursor`，在 GNOME Wayland 与原生 X11 交互截屏中可按 `` ` `` 或 `!` 切换显隐）、截图时隐藏已有贴图（`capture-hide-pins`）、历史循环（`capture-history-loop`）、区域历史数（`capture-history-areas`，`0–100`）与内存截图历史数（`capture-history-records`，`0–100`，上限 `128 MiB`）
  - 滚动选项：`保留关键帧上限`（`max-scroll-frames`，`2–200`，默认 `40`）、`自动滚动步进等待（毫秒）`（`scroll-interval`，`300–3000 ms`，默认 `700 ms`）与 `回滚时自动裁剪`（`scroll-auto-crop`，默认 `false`）

## 贴图（`Pins`）

分为 **显示（`Display`）**、**行为（`Behavior`）** 与 **转换（`Conversion`）** 三个子页：

- **显示**：新建贴图阴影开关（`pin-shadow`）与非激活阴影色（`pin-shadow-tint`，活动或选中贴图改用强调色）、新建贴图默认不透明度（`pin-opacity`，`10–100`）、新旧贴图通用最大尺寸（`pin-max-size`，`100–100000`）及缩略图尺寸（`pin-thumbnail-width` / `pin-thumbnail-height`，`24–1000`）。
- **行为**：使用截图原坐标（`pin-use-snip-position`）、光标中心滚轮缩放（`pin-zoom-at-cursor`）、新建贴图平滑缩放（`pin-smooth-scaling`）、缩略图重置行为（`pin-reset-thumbnail`）、闪烁反馈开关（`pin-translucent-flash`、`pin-flash-group`、`pin-hide-on-flash`、`pin-flash-new`）、新建默认激活（`pin-activate-new`）、拖动保留原贴图（`pin-preserve-drag`）、`允许贴图获取键盘焦点`（`pin-gnome-focus`，仅 GNOME 会话可用，默认开启；关闭后贴图在整个生命周期内包括打开工具条时都不会获取键盘焦点）、关闭/销毁确认（`pin-confirm-close`、`pin-confirm-destroy`）、已关闭贴图保留数量（`pin-closed-limit`，`0–128`，默认 `1`）以及存储方式（`pin-storage`：`persistent` 持久化 vs `memory` 仅内存）。
- **转换**：文本转贴图开关（`pin-text-enabled`、`pin-text-ignore-html`、`pin-text-skip-paths`）、内边距（`pin-text-margin`，`0–100`）、最大行宽（`pin-text-max-width`，`100–10000`）、字体（`pin-text-font`）以及前景/背景色（`pin-text-background`、`pin-text-foreground`）。

## 保存（`Save`）

分为 **图片（`Images`）** 与 **录屏（`Recording`）** 两个子页，每个文件名输入框右侧的信息提示均提供 **命名规则** 速查弹窗入口：

- **图片**：手动命名模板（`image-manual-name`）、记忆手选扩展名（`image-remember-extension` / `image-last-extension`）、JPEG 质量（`image-quality`，`0–100`，默认 `100`；PNG 始终无损）、快速保存目录/模板/通知开关（`image-quick-directory`、`image-quick-name`、`image-save-notify`，同时覆盖快速保存与自动保存通知），以及自动保存开关/目录（留空回退至快速保存目录）/模板（`image-auto-save`、`image-auto-directory`、`image-auto-name`）。
- **录制**：手动命名模板（`video-manual-name`）、保存目录/快速命名/通知开关（`video-directory`、`video-quick-name`、`video-save-notify`）、录制存储策略（`recording-storage`：`temporary` 先写入 `/tmp` 并受容量限制 vs `direct` 直接写入视频目录）以及内存录制上限（`recording-temp-limit-mib`，`16–16384 MiB`）。

## 控制（`Control`）

- **快捷键**：分组展示 `标注工具`、`共用编辑`、`截图选区`、`贴图`、`滚动截图` 与 `放大镜` 只读键位速查表。
- **鼠标**：配置贴图鼠标与滚轮绑定（`pin-mouse-zoom`、`pin-mouse-opacity`、`pin-mouse-close`、`pin-mouse-reset`、`pin-mouse-thumbnail`、`pin-mouse-copy-text`、`pin-mouse-destroy`）及 `+`/`-` 同键开关（`pin-mouse-zoom-keys`、`pin-mouse-opacity-keys`），支持单独重置 `控制 -> 鼠标`。

## 文字识别（`OCR`）

- **识别与复制**：
  - `识别引擎`（`ocr-engine`）：`自动`（已安装本地模型时优先使用 PP-OCRv5，否则自动回退至系统 Tesseract）、`PP-OCRv5 本地模型` 或 `系统 Tesseract`。识别在独立子进程中离线完成，结束后立即释放内存。
  - `保留段落空行`（`ocr-smart-paragraphs`，默认 `true`）：复制多段文字时在不同文本块之间插入空行。
  - `识别后直接复制`（`ocr-quick-copy`，默认 `false`）：截图时按 `O` 直接复制全文并关闭（`Shift+O` 始终直接复制）。
  - `Tesseract 语言包`（`ocr-languages`，默认 `eng+chi_sim`）：自动探测本机已安装的 `traineddata` 系统语言包并支持勾选组合识别，无需手动输入代码；同时标出缺失语言包、提供发行版安装命令一键复制与实时重新检测。
- **本地推理运行时**：管理本地 PP-OCRv5 语种模型共用的 ONNX Runtime 离线推理库（优先复用系统已安装的兼容 `libonnxruntime`，或按需下载并经 SHA-256 校验保存至 `$XDG_DATA_HOME/clivuu/ocr/runtime/onnxruntime-1.28.0-<arch>/installed/`），支持点击 `打开运行时目录` 手动放入离线运行库文件。重置 OCR 分节不会删除已下载的模型或运行时文件。
- **本地模型（PP-OCRv5）**（`ocr-model`，默认 `chinese`）：按需下载并经 SHA-256 校验的语种模型包（置顶常用 `中日英混合`、`西欧与拉丁语系`，折叠收纳 `韩文`、`西里尔语系`、`阿拉伯语系`、`希腊文`、`泰文`），保存在 `$XDG_DATA_HOME/clivuu/ocr/models/` 下；支持点击 `打开模型目录` 手动导入离线文件，或配置可选的 `GitHub 下载镜像`（`ocr-download-mirror`）。

## 关于（`About`）

显示 `Clivuu` 版本号与仓库地址（`https://github.com/Naerelyth/clivuu`）。

## 相关手册

- [配置项参考](../development/configuration.md)
- [保存、命名模板与桌面通知](saving.md)
- [快捷键与鼠标手势](shortcuts.md)

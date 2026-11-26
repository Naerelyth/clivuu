# 故障排查

[使用手册](../README.md) | [English](../../en/guide/troubleshooting.md) | **简体中文**

遇到异常时可先执行 `clivuu status`。该命令不打开图形界面，直接输出桌面环境、会话协议（`wayland` 或 `x11`）、当前使用的渲染/采集后端、生效配置文件路径以及可选工具检测情况。

## Portal 权限或截图报错

**现象**：触发截图或窗口截图时被拒绝或立刻退出。
**排查步骤**：
- 确认当前 D-Bus 会话中已运行 `xdg-desktop-portal` 及桌面对应的后端（`xdg-desktop-portal-gnome`、`xdg-desktop-portal-kde`、`xdg-desktop-portal-wlr` 或 `xdg-desktop-portal-gtk`）。
- 若此前曾点击“拒绝”授权，请在桌面系统“隐私/应用权限”中重置 Clivuu 的截图/屏幕共享授权，再重新运行 `clivuu snip` 或 `clivuu snip --window` 即可再次呼出原生授权提示。
- 脱离后台服务单独测试静态 Screenshot portal 是否正常：
  ```bash
  clivuu portal -o /tmp/portal-check.png
  ```

## Wayland 下鼠标悬停无法识别单个窗口

**现象**：进入截图拖动前，滚轮或鼠标悬停仅识别整块显示器而没有高亮单个应用窗口。
**原因**：仅支持窗口查询 IPC 的合成器（**Hyprland** 的 `hyprctl` + `HYPRLAND_INSTANCE_SIGNATURE` 与 **niri** 的 `niri msg` + `NIRI_SOCKET`）可直接提供桌面窗口矩形；出于安全模型，GNOME 与部分其他 Wayland 合成器不向普通应用开放全局窗口位置。
**处理方法**：改用 `clivuu snip --window`（或托盘菜单中的 `Window capture`），通过系统原生 `ScreenCast` 窗口选择器单帧截取指定窗口。

## 录屏或长截图顶部出现共享图标

在 Wayland 会话下，滚动长截图、区域录屏、`--window` 窗口截图以及开启 `capture-cursor=true` 的截图均基于标准 `xdg-desktop-portal` `ScreenCast`（GNOME 下会直接配合 `org.gnome.Mutter.ScreenCast` 且不回退全屏）。系统顶部或托盘显示的共享提示与授权选择器由合成器/桌面控制；普通无光标截图则仅使用一次性的静态 `Screenshot` 接口。

## GNOME 下贴图焦点与面板自动隐藏行为

**现象**：在 GNOME 中对贴图按方向键或 `Ctrl+滚轮` 未响应，或者点击贴图触发 `dash-to-panel` 自动隐藏变化。
**说明**：
- 默认开启 `pin-gnome-focus=true` 时，XWayland 贴图接受键盘焦点，因此无需打开工具栏即可直接使用方向键微调、`1–6` 旋转滤镜及修饰键鼠标手势。
- 若希望贴图在 GNOME 下完全不抢占焦点（避免影响面板显隐状态），可在 `首选项 -> 贴图 -> 行为` 关闭 **允许贴图获取键盘焦点**（或执行 `clivuu config set pin-gnome-focus false`）。设为 `false` 后，贴图在整个生命周期内（包括打开工具条时）都会拒绝键盘焦点，此时右键 `Show toolbar`（`显示工具条`）仅提供指针点击按钮；如需键盘快捷键或输入文字，请重新开启 `pin-gnome-focus`。

## 遮罩或菜单关闭后疑似残留透明拦截

正常按 `Esc`、右键取消、出错或退出时，Clivuu 会释放所有抓屏输入区域与弹窗。如果合成器异常后怀疑界面状态卡住，可运行：

```bash
clivuu no-click-through    # 取消全部贴图的鼠标穿透状态
clivuu restart             # 保存当前贴图和配置并重启 clivuu-desktop
```

## OCR 或条码识别找不到工具或无结果

- **OCR 报错**：前往 `首选项 -> 文字识别` 查看运行时状态、模型安装情况与报错信息。内置 `PP-OCRv5` 需要 C API v17+ 版本的 ONNX Runtime（`libonnxruntime`）及用于下载的 `curl`（离线环境可设置 `ocr-download-mirror`、手动放入模型/运行时目录，或用环境变量覆盖路径）。若切换为 **System Tesseract** 引擎，请确认已安装 `libtesseract5`（Tesseract 库）与对应 `.traineddata` 语言包（自定义目录请设置 `$TESSDATA_PREFIX` 后在 `首选项 -> 文字识别` 重新扫描）。
- **离线条码解码测试**：可运行 `clivuu worker barcode --input /path/to/code.png` 独立测试 `rxing` 离线解码，无需启动桌面服务。

## 录屏提前停止或导出失败

- **触发内存限制提前停止**：在 `recording-storage=temporary` 模式下，接近 `recording-temp-limit-mib`（默认 `512 MiB`，可设 `16–16384 MiB`）时会自动安全停录以防止内存耗尽。长时长录屏请在 `首选项 -> 保存 -> 录制 -> 录制策略` 改用 `视频目录`。
- **无声音或导出失败**：检查是否安装全部 GStreamer 依赖（`gstreamer1.0-pipewire`、`gstreamer1.0-pulseaudio`、`gstreamer1.0-plugins-base`、`gstreamer1.0-plugins-good`；Fedora 上对应 `pipewire-gstreamer`、`gstreamer1-plugins-good`、`gstreamer1-plugins-bad-free`、`ffmpeg-free`）及 `ffmpeg`（`ffprobe`）。自动化测试场景请按 [测试与验证](../development/testing.md) 使用私有 WirePlumber 配置隔离真实麦克风与音箱。

## 相关手册

- [快速入门](getting-started.md)
- [首选项说明](preferences.md)
- [测试与验证](../development/testing.md)

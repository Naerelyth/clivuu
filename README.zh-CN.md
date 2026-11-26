<div align="center">
  <img src="assets/icons/clivoo.svg" width="88" alt="Clivuu">
  <h1>Clivuu</h1>
  <p>面向 Linux Wayland 与 X11 桌面的截图、标注、贴图、长截图与区域录屏工具。</p>
  <p><a href="README.md">English</a> | <strong>简体中文</strong></p>
</div>

> **使用手册：** 各功能的详细操作方法、工具参数、快捷键与设置项说明请查阅 [简体中文使用手册](docs/zh-CN/README.md)（[English User Guide](docs/en/README.md)），包括区域截图、画布标注、桌面贴图、滚动长截图、录屏与回放编辑、OCR 文字识别、文件命名规则与故障排查。

## 项目简介

Clivuu 采用后台桌面服务（`clivuu-desktop`，提供托盘菜单与 D-Bus 接口）搭配命令行客户端（`clivuu`）的架构，支持 Linux Wayland 与 X11 会话。

| 核心功能 | 功能说明 | 详细手册 |
| :--- | :--- | :--- |
| **屏幕截图** | 基于系统 Screenshot portal 冻结画面，支持拖动选区、放大镜像素对齐、取色、窗口选择与历史截图回溯 | [截图指南](docs/zh-CN/guide/capture.md) |
| **画布标注** | 截图与贴图共用同一套编辑器：8 种标注工具（形状、箭头、画笔、高亮、序号、马赛克、文字）及橡皮擦 | [标注编辑](docs/zh-CN/guide/annotations.md) |
| **桌面贴图** | 从截图、剪贴板图像、文件列表、HTML 片段或颜色值创建置顶浮窗，支持变换、分组与关闭恢复 | [桌面贴图](docs/zh-CN/guide/pins.md) |
| **滚动长截图** | 支持垂直与水平方向长图拼接，提供实时覆盖预览、手动/自动滚动（RemoteDesktop）与首尾裁剪 | [滚动长截图](docs/zh-CN/guide/scrolling-capture.md) |
| **区域录屏** | 经 PipeWire 采集无损中间流并分轨录制系统音与麦克风，停止后可在回放界面裁剪、裁切画面与调速导出（`MP4`、`WebM`、`GIF`） | [录屏与回放](docs/zh-CN/guide/recording.md) |
| **OCR 与条码** | 双引擎离线文字识别（本地 PP-OCRv5 ONNX Runtime 与系统 `libtesseract` 5 回退），支持原位选字与快速复制；调用 `zbarimg` 扫描或解码二维码与条形码 | [OCR 与条码](docs/zh-CN/guide/ocr-and-codes.md) |

## 桌面环境与显示支持矩阵

Clivuu 根据当前会话能力分别选择画面显示后端与像素采集通道：

| 会话环境 | 遮罩与贴图显示 | 普通截图采集 | 窗口截图 / 光标捕获 |
| :--- | :--- | :--- | :--- |
| **niri / Hyprland** | 原生 Wayland（`gtk4-layer-shell`，贴图使用 `ON_DEMAND`） | 非交互 `Screenshot` portal | 合成器窗口探测（`niri msg` / `hyprctl`）+ 单帧 `ScreenCast` 窗口选择器与光标合成 |
| **KDE Plasma / wlr-layer-shell** | 原生 Wayland（`gtk4-layer-shell`，贴图使用 `ON_DEMAND`） | 非交互 `Screenshot` portal | 单帧 `ScreenCast` 系统窗口选择器与光标合成 |
| **GNOME（Wayland）** | 遮罩与贴图走 XWayland（支持 `pin-gnome-focus` 控制焦点）；首选项、保存、分组管理与确认弹窗使用原生 Wayland | 非交互 `Screenshot` portal | 单帧 `ScreenCast` 系统窗口选择器与光标合成 |
| **X11** | 原生 X11 窗口 | 非交互 `Screenshot` portal | 单帧 `ScreenCast` 系统窗口选择器 + XFixes 光标捕获 |

全局热键不在应用进程内强行劫持，建议在桌面环境或窗口管理器设置中直接将快捷键绑定到 `clivuu` 子命令。

## 运行依赖

[CMakeLists.txt](CMakeLists.txt) 中声明的核心与可选运行依赖如下：

- **图形界面**：GTK 4（`>= 4.14`）、libadwaita（`>= 1.5`）、`gtk4-layer-shell`
- **系统门户与兼容层**：`xdg-desktop-portal`（及对应桌面后端）、`xwayland`（推荐用于 GNOME 遮罩与贴图）
- **录屏与音频**：GStreamer（`>= 1.20`）、`gstreamer1.0-pipewire`（`pipewire-gstreamer`）、`gstreamer1.0-pulseaudio`、`gstreamer1.0-plugins-base`、`gstreamer1.0-plugins-good`、`ffmpeg`（Fedora 为 `ffmpeg-free`）
- **可选扩展**：`libonnxruntime` / `curl`（用于设置中按需下载 PP-OCRv5 模型与运行时）、`libtesseract5` 与语言包（回退 OCR 识别）、`zbarimg` / `zbar-tools`（条码与二维码识别）、`lp` / CUPS（打印输出）

<details>
<summary>常用发行版运行依赖安装命令（Debian / Ubuntu 与 Fedora）</summary>

**Debian / Ubuntu：**
```bash
sudo apt install libgtk-4-1 libadwaita-1-0 libgtk4-layer-shell0 \
  libgstreamer1.0-0 gstreamer1.0-pipewire gstreamer1.0-pulseaudio \
  gstreamer1.0-plugins-base gstreamer1.0-plugins-good xdg-desktop-portal \
  ffmpeg xwayland tesseract-ocr tesseract-ocr-eng zbar-tools
```

**Fedora：**
```bash
sudo dnf install gtk4 libadwaita gtk4-layer-shell gstreamer1 \
  pipewire-gstreamer gstreamer1-plugins-base gstreamer1-plugins-good \
  xdg-desktop-portal ffmpeg-free xorg-x11-server-Xwayland \
  tesseract tesseract-langpack-eng zbar
```

</details>

## 构建与打包

项目通过 [Makefile](Makefile) 与 [CMakeLists.txt](CMakeLists.txt) 管理构建和安装包产出，所有构建与打包产物默认写入 `/tmp` 目录：

```bash
# 编译 Release 版本（clivuu 与 clivuu-desktop）
make build

# 运行单元测试、多语言校验、配置校验、文档校验与集成检查
make test

# 在 /tmp/clivuu-dist 生成各类安装包
PACKAGER='Your Name <you@example.org>' make deb        # Debian / Ubuntu .deb
PACKAGER='Your Name <you@example.org>' make rpm        # Fedora / RHEL .rpm
make package                                           # 通用 .tar.gz 压缩包
make appimage APPIMAGE_RUNTIME=/path/to/runtime-x86_64 # AppImage 便携包
```

开发头文件准备、自定义安装前缀及 AppImage 构建参数见 [构建与打包](docs/zh-CN/development/building.md)。

## 快速上手与常用命令

启动后台桌面服务（显示托盘图标并监听 D-Bus 请求）：

```bash
clivuu
```

直接在终端调用常用命令，或将其配置为桌面全局快捷键：

```bash
clivuu snip                             # 进入交互式区域截图
clivuu snip --window                    # 通过系统 ScreenCast 窗口选择器截取单个窗口
clivuu snip --delay 3 -o quick-save     # 延迟 3 秒截图并直接快速保存
clivuu paste                            # 将剪贴板内容贴到桌面（图片/文件/HTML/颜色/文本）
clivuu scroll                           # 框选区域后开始滚动长截图
clivuu record                           # 框选区域并打开录屏就绪栏
clivuu pick-color                       # 拾取屏幕像素颜色并复制
clivuu barcode-scan                     # 框选屏幕区域并识别条码/二维码
clivuu open-preferences                 # 打开首选项窗口
clivuu status                           # 以 JSON 输出桌面服务运行状态
clivuu restart                          # 重启桌面服务
clivuu exit                             # 退出服务
```

无需启动 GTK 即可读写配置与查看帮助：

```bash
clivuu config path                      # 输出当前 config.ini 文件路径
clivuu config get image-quality         # 读取指定配置项
clivuu config set language zh-CN        # 写入非默认配置覆盖
clivuu --help                           # 查看命令速览
```

完整命令参数与 D-Bus 控制说明请参阅 [命令行与 D-Bus 控制](docs/zh-CN/development/cli.md)。

## 配置与数据目录

- **配置文件**：`$XDG_CONFIG_HOME/clivuu/config.ini`（按功能分组，仅保存相对默认值发生变更的项）
- **应用数据（贴图与 OCR）**：`$XDG_DATA_HOME/clivuu/`（`pins/` 存放持久化贴图，`ocr/` 存放下载的 OCR 运行时与模型）
- **缓存目录**：`$XDG_CACHE_HOME/clivuu/`
- **默认快速保存目录**：`$XDG_PICTURES_DIR/Clivuu/`（图片）与 `$XDG_VIDEOS_DIR/Clivuu/`（视频）

## 源码目录结构

```text
core/        无 UI 核心库（图像编解码、标注几何/模型、长图拼接、配置 schema、OCR、本地化文本）
cli/         命令行客户端、配置子命令、portal 调用与独立计算 worker（clivuu）
desktop/     GTK4 / libadwaita 桌面服务、各会话适配层及截图/贴图/长截图/录屏会话（clivuu-desktop），含动态 X11 crate（desktop/x11/）
assets/      发布图标（assets/icons/）与图标设计源文件
packaging/   桌面入口文件、CPack 打包脚本与 AppImage 打包模板（CMake 安装规则位于根目录 CMakeLists.txt）
docs/        中英双语用户手册（guide/）与开发技术文档（development/）
tests/       配置校验、文档链接审计、i18n 检查、CLI 与合成器测试脚本
```

## 文档导航

- **用户使用手册**：[简体中文使用手册](docs/zh-CN/README.md) · [English User Guide](docs/en/README.md)
- **开发与技术参考**：
  - [架构说明](docs/zh-CN/development/architecture.md)
  - [命令行与 D-Bus 控制](docs/zh-CN/development/cli.md)
  - [配置项参考](docs/zh-CN/development/configuration.md)
  - [构建与打包](docs/zh-CN/development/building.md)
  - [本地化开发](docs/zh-CN/development/localization.md)
  - [测试与验证](docs/zh-CN/development/testing.md)

## 许可证

本项目基于 [GNU General Public License v3.0 or later](LICENSE) 开源协议发布。

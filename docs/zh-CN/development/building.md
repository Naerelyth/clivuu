# 构建与打包

[English](../../en/development/building.md) | **简体中文** · [使用手册](../README.md)

## 编译环境与系统依赖

项目采用 Rust edition 2024（开发基准版本 Rust 1.94）。Cargo 负责编译 `clivuu` 与 `clivuu-desktop` 两个可执行文件（`.cargo/config.toml` 将 `target-dir` 设在 `/tmp/clivuu-target`）；CMake（`>= 3.24`）与 CPack 负责目录安装布局与系统软件包打包。

### 编译期开发头文件

系统需安装可通过 `pkg-config` 检索的以下开发库：

- GTK 4（`>= 4.14`）、libadwaita（`>= 1.5`）
- GStreamer（`>= 1.20`）、`gstreamer-plugins-base`
- CMake `>= 3.24` 及标准 C 工具链

`gtk4-layer-shell`、`libX11` 与 `libXfixes` 均在运行时通过 `dlopen` 动态加载，编译期无需这些开发头文件（也不再使用 `libxext`、`libxtst` 与 `libxrandr`）。

Debian / Ubuntu 安装示例：

```sh
sudo apt install build-essential pkg-config cmake \
  libgtk-4-dev libadwaita-1-dev \
  libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev
```

### 运行时组件

- PipeWire、桌面对应的 `xdg-desktop-portal` 后端、`gtk4-layer-shell` 以及 XWayland（GNOME 下遮罩与贴图使用）
- GStreamer 插件元件：`pipewiresrc`、`appsink`/`appsrc`、`vp9enc`、`vp9dec`、`matroskamux`、`matroskademux`、`flacenc`、`flacparse`、`flacdec`、`pulsesrc`（`gstreamer1.0-pulseaudio`）、`audiomixer` 与 `scaletempo`
- `ffmpeg` 与 `ffprobe`（Fedora 为 `ffmpeg-free`），供 WebM / MP4 / GIF 导出与音频检测使用
- 可选扩展：`libonnxruntime` C API v17+ 与 `curl`（用于本地 `PP-OCRv5` 模型下载与推理）、`libtesseract5` 动态库及 traineddata 语言包（系统 Tesseract OCR）以及 `lp`（CUPS 打印）；条码/二维码解码由内置 `rxing` 实现

## 编译与本地安装

所有构建输出与 CMake 临时目录默认位于 `/tmp`（`/tmp/clivuu-target` 与 `/tmp/clivuu-package`）：

```sh
make build
cmake -S . -B /tmp/clivuu-build \
  -DCLIVUU_PREBUILT_DIR=/tmp/clivuu-target/release
cmake --install /tmp/clivuu-build --prefix "$HOME/.local"
```

若配置时不传入 `CLIVUU_PREBUILT_DIR`，执行 `cmake --build /tmp/clivuu-build` 会自动调用 Cargo 构建两个二进制程序。

安装至 `<prefix>` 后的目录结构如下：
- `bin/clivuu`（CLI 客户端）
- `libexec/clivuu-desktop`（后台服务，由 CLI 按同前缀路径自动查找）
- `share/clivuu/icons/` 与 `share/icons/hicolor/scalable/apps/io.github.naerelyth.clivuu.svg`
- `share/applications/io.github.naerelyth.clivuu.desktop`
- `share/doc/clivuu/`（README 与双语 Markdown 文档）及 `share/licenses/clivuu/Octicons-LICENSE.txt`

安装与包升级均不会改动用户 `$XDG_CONFIG_HOME/clivuu/` 或 `$XDG_DATA_HOME/clivuu/` 中的个人配置和持久化贴图数据。

## DEB 与 RPM 安装包

原生安装包支持 `x86_64` 与 `aarch64` 主机，DEB 启用 `dpkg-shlibdeps`、RPM 启用 `AutoReq` 解析共享库依赖，并在清单中显式声明非直接动态链接插件（`gtk4-layer-shell`、GStreamer 插件、`ffmpeg`）：

```sh
PACKAGER='Your Name <you@example.org>' make deb
PACKAGER='Your Name <you@example.org>' make rpm
```

请在目标发行版本机上打包以确保依赖库包名与版本扫描准确（Revision 默认 `1`，直接调用 CMake 时可通过 `-DCLIVUU_PACKAGE_RELEASE=...` 修改；`Makefile` 不会透传 `CLIVUU_PACKAGE_RELEASE`）。分发前建议运行 `dpkg-deb --contents` 或 `rpm -qlp` 核对产物内容。

## 便携归档与 AppImage

```sh
make package                                        # 在 /tmp/clivuu-dist 生成 .tar.gz
make appimage APPIMAGE_RUNTIME=/path/to/runtime-x86_64
```

`make appimage` 经 CPack External 生成 `/usr` 目录树并调用 `appimagetool`（支持 `APPIMAGETOOL` 指定路径），构建过程要求在本地通过 `APPIMAGE_RUNTIME` 提供对应架构的 type-2 AppImage runtime（不自动联网下载）。生成目录默认为 `/tmp/clivuu-dist`（可通过 `PACKAGE_OUTPUT` 覆盖）。

由于 AppImage 以只读挂载运行，启动流程会将长驻服务进程运行环境与 CLI 单次调用解耦，并且执行重启时会通过 `$APPIMAGE` 重新拉起外部 `.AppImage` 文件本身。由于未内嵌系统 GTK 与 GStreamer 共享库，请确保目标桌面满足最低版本要求。

打包校验流程见 [测试与验证](testing.md)。

# 架构说明

[English](../../en/development/architecture.md) | **简体中文** · [使用手册](../README.md)

## Workspace 模块划分

| 模块目录 | 主要职责 |
| :--- | :--- |
| `core/` | 图像编解码/导出、标注数据模型/几何/撤销栈、长图拼接、配置 schema、协议结构、本地 OCR 与语言资源（不依赖 GTK） |
| `cli/` | 命令行解析、本地 `config`/`portal`/`worker` 子命令以及 D-Bus 客户端（无图形界面依赖） |
| `desktop/src/application/` | 截图、贴图、滚动长截图、录屏回放与输出路由的会话编排 |
| `desktop/src/editor/` | 共用标注控制器、多坐标映射、场景绘制以及编辑生命周期 |
| `desktop/src/ui/` | 主工具栏与次级栏、首选项窗口、文件保存框、右键菜单与屏幕提示 |
| `desktop/src/platform/` | 各桌面表面适配、ScreenCast/Screenshot 采集、剪贴板、输入区域、托盘集成与图标资源解析 |
| `desktop/src/infrastructure/` | INI 配置文件监控、有界后台任务队列与存储/运行时辅助工具 |
| `desktop/x11/` | 隔离的 X11 绑定与原生窗口 / XFixes 交互 |

## 编辑与导出管道

截图遮罩与贴图工具栏共用同一套标注会话、操作分发、命中检测、参数状态、渲染管线与撤销/重做历史。工具栏直接从当前会话读取撤销与重做可用性，连续滚轮缩放或强度调节可合并为单步撤销事务。

编辑器明确区分四类坐标系：源图像物理像素、桌面逻辑坐标、表面缩放倍率与指针热点位置，完整处理负坐标原点及混合/分数 DPI 场景。源位图在内存中保持原始分辨率，显示变换和滤镜在画布预览与导出字节之间保持一致；纯文本与色卡在栅格化之前先确定目标表面密度。

`core::image::export` 统一负责格式识别、JPEG 压缩参数映射、质量控制与编码写入。桌面应用层统一路由剪贴板、贴图、手动/快速/自动文件落盘及截图历史记录。

## 表面后端与画面采集

显示载体与采集方式按当前桌面会话能力分别决策：

- **原生 Wayland（`niri`、`Hyprland`、KDE）**：遮罩与贴图使用 `gtk4-layer-shell`，贴图采用 `ON_DEMAND` 焦点策略。
- **GNOME Wayland**：因 GNOME 缺少 `wlr-layer-shell`，遮罩与贴图使用 XWayland 呈现；首选项、保存对话框、分组管理/切换器与确认弹窗以独立原生 Wayland 窗口运行。GNOME 贴图默认接受键盘焦点；设为 `pin-gnome-focus=false` 后，贴图在整个生命周期内均拒绝键盘焦点。
- **X11 会话**：由独立 X11 适配模块处理窗口与显示。

采集通道分为三类：
- **普通截图**：各桌面默认调用非交互 `Screenshot` portal（原生 X11 额外使用 `XFixes` 合成光标）；当 Wayland 下启用 `capture-cursor=true` 时，经 `ScreenCast` 读取单帧合成画面，让光标与屏幕像素来自同一合成帧。
- **窗口截图**：各桌面统一打开系统 `ScreenCast` 窗口选择器抓取一帧后，在进入标注前立刻关闭流。
- **连续采集**：滚动长截图与区域录屏使用持续 `ScreenCast` 采集流，运行期遵循系统屏幕共享指示规范。

任何弹窗、菜单、抓取状态或遮罩在关闭、取消、出错及服务退出时立即释放输入区域与抓取。

## 有界异步任务

所有 GTK 控件操作保持在主线程；图像解码、PNG/JPEG 编码、长图对齐拼接及磁盘写入均派发到有界后台任务。双引擎离线文字识别（`PP-OCRv5` ONNX Runtime 与系统 `Tesseract` 5）通过独立短生命周期子进程（`clivuu worker ocr-document` / `--ocr-worker`）配合 `memfd_create` 共享图像内存执行。指针移动直接更新进程内状态与缓存纹理，不发起阻塞 D-Bus 调用或频繁 PNG 编解码。

滚动长截图将主体匹配、已确认覆盖文档和固定边栏分离建模，即便中途取消或帧匹配中断，已确认部分依然可完整导出。区域录屏以有界队列写入无损中间格式（Matroska 容器内 `VP9` Y444 与独立 `FLAC` 系统/麦克风音轨），停止采集后释放采集流并将只读不可变源素材移交给回放编辑器。

## 配置管理与进程边界

`core/config-defaults.json` 为命令行与界面提供统一默认值与校验规则。写入操作经过异步合并后以稀疏 INI 形式写入 `config.ini`（仅保存相对默认值有修改的项），同时监听外部编辑变动。

持久化贴图与已关闭贴图数据保存在 `$XDG_DATA_HOME/clivuu/`。服务重启命令会先中止新请求、结束活动对话框与采集、完善收尾录制、落盘配置和贴图并释放 `io.github.naerelyth.Clivuu.Desktop`（对象路径 `/io/github/naerelyth/Clivuu/Desktop`，接口 `io.github.naerelyth.Clivuu.Desktop1`），再拉起持久入口（AppImage 外层路径或前缀二进制）。

相关接口详情见 [命令行与 D-Bus 控制](cli.md)、[配置项参考](configuration.md) 与 [测试与验证](testing.md)。

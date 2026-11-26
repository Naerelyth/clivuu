# Clivuu 使用手册

[English](../en/README.md) | **简体中文** | [项目主页 README](../../README.zh-CN.md)

本手册介绍 Clivuu 的各项桌面功能与操作细节。如果你需要通过命令行或 D-Bus 集成 Clivuu、查阅完整配置项或从源码构建安装包，请参阅下方的开发参考文档。

## 从这里开始

- **[快速入门](guide/getting-started.md)**：确认运行环境依赖、启动后台托盘服务、绑定系统全局热键并完成首次截图。

## 功能使用手册

| 功能模块 | 主要内容 | 文档链接 |
| :--- | :--- | :--- |
| **屏幕截图** | 自由框选、当前屏与全桌面、窗口选择器、像素放大镜、颜色拾取以及历史截图切换 | [截图指南](guide/capture.md) |
| **画布标注** | 32px 工具栏与次级栏、形状、11 种箭头与 5 种线型、画笔/高亮、序号徽章、马赛克/模糊、画布直输文字与橡皮擦 | [标注编辑](guide/annotations.md) |
| **桌面贴图** | 从截图或剪贴板（图片、文件列表、HTML、颜色代码、文本）生成贴图，缩放旋转滤镜、鼠标手势、分组与关闭恢复 | [桌面贴图](guide/pins.md) |
| **滚动长截图** | 垂直与水平方向采集、侧边预览框对齐、手动滚动与 `RemoteDesktop` 自动滚动、首尾裁剪与导出 | [滚动长截图](guide/scrolling-capture.md) |
| **区域录屏与回放** | 帧率与倒计时预设、系统音与麦克风分轨、暂停与录制期界面自动隐藏、回放裁切/裁剪/倍速及 `MP4`/`WebM`/`GIF` 导出 | [录屏与回放](guide/recording.md) |
| **OCR 与条码识别** | 本地双引擎文字识别（PP-OCRv5 ONNX Runtime 与系统 Tesseract）、原位选字、多语言配置、`zbarimg` 框选扫码与图片解码 | [OCR 与条码](guide/ocr-and-codes.md) |
| **保存、命名与通知** | 手动原生 GTK 保存框、快速保存与自动保存、文件名模板变量、JPEG 质量与 PNG 无损编码及在文件管理器中定位 | [保存与命名](guide/saving.md) |
| **快捷键与鼠标手势** | 截图、标注、贴图、长截图、录屏与放大镜完整快捷键对照，以及可自定义贴图鼠标手势 | [快捷键](guide/shortcuts.md) |
| **首选项说明** | 常规、截屏、贴图、保存、控制、文字识别、关于等设置页面与分节重置说明 | [首选项](guide/preferences.md) |
| **故障排查** | Portal 权限重置、GNOME 焦点行为、包含光标截图条件、OCR/条码缺失报错与状态诊断 | [故障排查](guide/troubleshooting.md) |

## 技术与开发参考

- **[架构说明](development/architecture.md)**：Workspace 模块划分、显示与采集能力选择逻辑以及会话生命周期。
- **[命令行与 D-Bus 控制](development/cli.md)**：CLI 子命令、参数校验规则、JSON 输出与 D-Bus 接口。
- **[配置项参考](development/configuration.md)**：按功能分组的稀疏 XDG INI 格式与全部 89 个配置默认值清单。
- **[构建与打包](development/building.md)**：Cargo、CMake 与 CPack 编译安装及 DEB/RPM/TGZ/AppImage 打包方法。
- **[本地化开发](development/localization.md)**：界面与命令行共享字典（`core/locales/`）、语言解析优先级与 `i18n.mjs` 校验方法。
- **[测试与验证](development/testing.md)**：测试脚本说明、私有 D-Bus / WirePlumber 媒体隔离配置与合成器回归方法。

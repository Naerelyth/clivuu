# OCR 文字识别与条码解码

[使用手册](../README.md) | [English](../../en/guide/ocr-and-codes.md) | **简体中文**

Clivuu 通过短生命周期的独立工作进程（`clivuu worker ocr-document` 或 `clivuu-desktop --ocr-worker`，`90 s` 超时，识别结束后立即退出以释放模型内存）提供双引擎离线 OCR 文字识别，并调用 `zbarimg` 识别二维码与条形码。

## 本地 OCR 文字识别

### 识别引擎

- **PP-OCRv5 Mobile（`paddle`）**：通过 `dlopen` 动态加载 ONNX Runtime（`libonnxruntime`，C API `v17+`）。Clivuu 会优先复用兼容的系统动态库，也可在设置中按需下载经过 SHA-256 校验的 ONNX Runtime（`1.28.0`，支持 `linux-x64` / `linux-aarch64`）至 `$XDG_DATA_HOME/clivuu/ocr/runtime/`，并按需下载 7 种离线语言模型包（`chinese`、`latin`、`korean`、`cyrillic`、`arabic`、`greek`、`thai`）至 `$XDG_DATA_HOME/clivuu/ocr/models/`。
- **系统 Tesseract（`tesseract`）**：通过 `dlopen` 加载 `libtesseract` 5，解析 TSV 词框排版并清理多余中文空格。
- **引擎切换（`ocr-engine`）**：可选 `自动`（`auto`，默认，优先使用已安装的 PP-OCRv5 与 ONNX Runtime，否则回退至系统 Tesseract 5）、`PP-OCRv5（本地）`（`paddle`）或 `系统 Tesseract`（`tesseract`）。

### 触发方式与原位选字

- **交互式截图（`O` 或 `识别文字`）**：默认（`ocr-quick-copy=false`）在截图选区内直接进入原位交互式选字：
  - 支持按字符拖拽选择、双击选整行、三击选整段，`Ctrl+A` 全选。
  - `Ctrl+C` 原位复制当前选中或全部文本，并在界面内提示 `已复制`（不发送系统通知）。
  - `Enter` 复制并关闭截图；`Esc` 有选字时先清空选字，无选字时退出 OCR 模式。
  - 悬浮 OCR 工具条提供引擎与模型切换、`智能分段`（`ocr-smart-paragraphs`，默认 `true`）、`全选`、`贴图文本`（将识别文本生成为贴图）、`复制选中`/`复制全部` 以及 `OCR 设置`（直达 `首选项 -> 文字识别`）。若当前尚无可用引擎或模型，会在栏内给出 `下载模型` 引导（自动下载需要系统已安装 `curl`）。
- **快速复制模式（`Shift+O` 或 `ocr-quick-copy=true`）**：截图时按 `Shift+O` 始终直接识别并复制全部文字后退出；在设置开启 `识别后立即复制，跳过原位选字`（`ocr-quick-copy=true`）后，截图下按 `O` 或点击 `识别文字` 也会直接复制并退出。
- **桌面贴图（`O` 或 `识别文字`）**：聚焦贴图且未激活绘图工具时按 `O`，或通过右键菜单与贴图工具栏点击 `识别文字`，均可在贴图画布上切换随缩放比例自适应的原位选字（`Ctrl+A`、`Ctrl+C`、`Esc` 清空/退出；按住 `Alt` 可直接在密集文字上方拖动贴图）。
- **命令行**：执行 `clivuu snip -o ocr_clipboard`（可搭配 `--area`、`--window`、`--last` 等参数），会直接复制识别文本并输出 JSON `{"text":"...","rect":...}`。

### 首选项配置与环境变量覆盖

`首选项 -> 文字识别` 包含三个配置分节：

- **识别与复制**：设置 `识别引擎`（`ocr-engine`）、`保留智能分段`（`ocr-smart-paragraphs`，默认 `true`）、`识别后立即复制，跳过原位选字`（`ocr-quick-copy`，默认 `false`），以及可视化多选清单 `Tesseract 语言包`（`ocr-languages`，默认 `eng+chi_sim`，自动扫描系统已安装的 `*.traineddata`、提示缺失语言安装命令并支持刷新重扫；命令行 `clivuu config set ocr-languages eng+jpn+chi_sim` 仍使用 `+` 拼接代号）。
- **推理运行时**：查看 ONNX Runtime 状态、一键下载安装，或通过 `打开运行时目录` 定位到 `$XDG_DATA_HOME/clivuu/ocr/runtime/onnxruntime-1.28.0-<arch>/installed/` 手动放置库文件。
- **本地模型（PP-OCRv5）**：切换默认模型包（`ocr-model`，默认 `chinese`）、按需通过 `curl` 下载模型、打开模型目录以及配置 `GitHub 下载镜像前缀`（`ocr-download-mirror`）。

高级或离线部署支持的环境变量：`CLIVUU_ORT_LIB`、`CLIVUU_IGNORE_SYSTEM_ORT`、`CLIVUU_OCR_MODELS_DIR`、`CLIVUU_OCR_MIRROR`、`CLIVUU_DISABLE_TESSERACT` 与 `TESSDATA_PREFIX`。

## 二维码与条形码识别

条码扫描依赖 `zbarimg`（Debian/Ubuntu 安装 `zbar-tools`，Fedora 安装 `zbar`）。

### 框选屏幕扫码（`clivuu barcode-scan`）

运行 `clivuu barcode-scan`（建议绑定为全局快捷键）后拖动框选屏幕中的单个或多个二维码/条形码，识别到的原始内容将复制到剪贴板；使用 `clivuu snip -o barcode` 也能触发同样流程。

### 解码本地图片（`clivuu barcode-decode FILE`）

通过后台桌面服务对图片文件解码：

```bash
clivuu barcode-decode /path/to/qrcode.png
```

服务调用 `zbarimg --quiet --raw` 解码后，将结果写入剪贴板并在标准输出打印 JSON（`{"text":"..."}`）。若希望在不启动桌面服务且不写入剪贴板的纯命令行环境逐行输出原始内容，可直接调用 worker 子命令：

```bash
clivuu worker barcode /path/to/qrcode.png
```

## 相关手册

- [屏幕截图](capture.md)
- [首选项](preferences.md)
- [故障排查](troubleshooting.md)

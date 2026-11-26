# 快速入门

[使用手册](../README.md) | [English](../../en/guide/getting-started.md) | **简体中文**

Clivuu 由两个可执行文件组成：后台桌面服务 `clivuu-desktop`（负责系统托盘、全屏遮罩、桌面贴图、保存对话框与 D-Bus 服务）以及命令行客户端 `clivuu`（用于终端调用和桌面快捷键绑定）。

## 首次启动

直接运行不带参数的命令即可启动后台服务：

```bash
clivuu
```

若 `clivuu-desktop` 尚未运行，`clivuu` 会自动拉起该服务（应用 ID 为 `io.github.naerelyth.clivuu`），注册系统托盘图标并导出 D-Bus 服务 `io.github.naerelyth.Clivuu.Desktop`。诸如 `clivuu status`、`clivuu config ...`、`clivuu --help` 与 `clivuu --version` 等命令不会在 CLI 进程中启动 GTK（`clivuu status` 仅在服务已运行时通过 D-Bus 查询状态）。

## 桌面集成与全局快捷键

Clivuu 将全局热键交由桌面环境或窗口管理器触发，请把系统快捷键直接绑定到对应的 `clivuu` 命令：

| 常用动作 | 绑定命令 | 说明 |
| :--- | :--- | :--- |
| 区域截图 | `clivuu snip` | 进入全桌面冻结框选界面 |
| 窗口截图 | `clivuu snip --window` | 打开系统 `ScreenCast` 窗口选择器截取单帧画面并进入编辑 |
| 贴图剪贴板 | `clivuu paste` | 将剪贴板内容生成贴图（截图框选中调用会直接将当前选区贴图；恢复已关闭贴图请使用 `clivuu reopen-pin`） |
| 滚动长截图 | `clivuu scroll` | 框选滚动区域后开始拼接捕获 |
| 区域录屏 | `clivuu record` | 框选录屏范围并显示录制控制栏 |
| 屏幕取色 | `clivuu pick-color` | 拾取屏幕像素颜色到剪贴板 |
| 显示/隐藏贴图 | `clivuu toggle-images` | 切换当前分组所有可见贴图的显示状态 |
| 首选项 | `clivuu open-preferences` | 打开设置窗口 |

无论在 GNOME Wayland、支持 layer-shell 的 Wayland 合成器（如 `niri`、`Hyprland`、KDE）还是 X11 中，只需在系统“自定义快捷键”或合成器配置文件中填写上述命令即可。

## 系统托盘菜单

右键或点击托盘图标可打开以下操作项：

- **截图与录屏**：`截图`、`窗口截图`、`选区录制`（录制期间原位切换为 `暂停录制` / `继续录制` 与 `停止录制`）
- **贴图与分组**：`贴图剪贴板`、`显示 / 隐藏贴图`、按需出现的 `恢复鼠标交互`（仅当前分组存在鼠标穿透贴图时显示）、`贴图组` 子菜单（标题实时显示当前分组名）以及 `贴图恢复与清理` 子菜单（存在已关闭贴图时附带数量，收纳 `贴图最近截图`、`重新贴出最近关闭的贴图`、`重新贴出全部已关闭贴图`、`关闭全部贴图`、`销毁已关闭贴图`）
- **服务控制**：`首选项`、`重启`、`退出`

## 桌面权限说明

在 Wayland 会话中，屏幕像素抓取与模拟滚轮输入走标准 `xdg-desktop-portal` 接口：

- **静态截图（`Screenshot` portal）**：普通 `clivuu snip` 使用非交互截图接口，按系统提示授权一次即可。
- **窗口与包含光标截图（`ScreenCast` portal）**：使用 `clivuu snip --window`、`--active-window`，或启用 Wayland 下的 `capture-cursor=true` 时，Clivuu 通过 `ScreenCast` 读取单帧画面后立即关闭视频流，再进入标注界面。
- **长截图与录屏**：持续使用 `ScreenCast` 采集流，期间桌面顶栏或托盘会显示系统自带的屏幕共享状态图标。
- **自动滚动**：仅在开启滚动长截图中的“自动滚动”按钮时请求 `RemoteDesktop` 输入授权。

## 语言切换与登录自启

- **界面语言**：可在 [首选项](preferences.md) 的 `常规 -> 界面语言` 中切换，也可运行 `clivuu config set language zh-CN`（或 `en`），或通过环境变量 `CLIVUU_LANGUAGE=zh-CN` 临时覆盖。
- **登录自启**：在 `首选项 -> 常规` 中开启“登录后自动启动”会写入标准 XDG Autostart `.desktop` 文件（默认关闭）。

## 检查运行状态

执行以下命令查询服务与会话能力状态：

```bash
clivuu status
```

命令以 JSON 输出服务运行状态（`running`、`pid`、`ready`、`busy`、`recording`、`scrolling`）、贴图与分组统计（`pins`、`closedPins`、`group`、`groups`）以及检测到的显示与会话能力（`capabilities`，含 `backend` 显示后端如 `GdkWaylandDisplay` / `GdkX11Display`、`capture`、`non_focusing_pins`、`smart_windows`、`scrolling`、`recording`）。查看当前配置文件路径请运行 `clivuu config path`。

## 下一步

- 了解选区操控与像素放大镜：[截图指南](capture.md)
- 掌握画笔、箭头、序号与文字直输：[标注编辑](annotations.md)
- 使用剪贴板贴图、分组与恢复机制：[桌面贴图](pins.md)

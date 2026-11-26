# 命令行与 D-Bus 控制

[English](../../en/development/cli.md) | **简体中文** · [使用手册](../README.md)

运行 `clivuu --help` 查看按类别分组的命令总览，运行 `clivuu <COMMAND> --help`（或 `clivuu help <COMMAND>`、`clivuu config <SUBCOMMAND> --help`）查看具体命令参数与互斥约束；运行 `clivuu docs` 查看使用手册与文档路径，运行 `clivuu --version`（`-V`）查看版本号，运行 `clivuu completions bash|zsh|fish` 生成终端自动补全脚本。直接执行 `clivuu` 不带参数将启动或连接 `clivuu-desktop`。所有桌面命令以 JSON 格式返回结果，出错时输出错误信息并以状态码 `1` 退出。

`status`、`config`、`completions`、`docs`、`help`、`portal`、`--help`、`--version` 与 `worker` 命令均不初始化 GTK 界面。所有选项与位置参数按声明式命令规范（`cli/src/spec.rs`）按子命令严格校验，跨命令传入无效参数会立即被拒绝。

## D-Bus 控制接口

`clivuu-desktop` 在会话总线上注册名称 `io.github.naerelyth.Clivuu.Desktop`、对象路径 `/io/github/naerelyth/Clivuu/Desktop` 与接口 `io.github.naerelyth.Clivuu.Desktop1`。执行 `clivuu` 命令时，若后台服务尚未启动会自动拉起并经由 D-Bus 转发请求。

## 截图、长截图与录屏命令

```sh
clivuu snip
clivuu snip --full -o /tmp/desktop.png
clivuu snip --area 100 100 640 480 -o clipboard
clivuu snip --delay 2
clivuu snip --window
clivuu scroll
clivuu record -o /tmp/demo.mp4
clivuu pause-recording
clivuu stop-recording
```

| 参数选项 | 说明 |
| :--- | :--- |
| `--area X Y W H` | 指定桌面矩形区域（支持负坐标原点，宽高须不小于 `2`；适用于 `snip`、`scroll` 与 `record`） |
| `--size W H` | 以固定宽高在屏幕居中直接截取（宽高 `>= 2`，仅适用于 `snip`，不打开交互选区） |
| `--full` | 全桌面合并区域（仅适用于 `snip`） |
| `--active-screen` | 当前指针或焦点所在显示器（仅适用于 `snip`） |
| `--last` | 上次成功截图使用的区域（仅适用于 `snip`） |
| `--window`、`--active-window` | 在各类桌面上打开系统 `ScreenCast` 单窗口选择器抓取单帧（仅适用于 `snip`） |
| `--delay SECONDS` | 抓取画面前等待 `0` 到 `3600` 秒（适用于 `snip` 与 `record`） |
| `-o`、`--output TARGET` | 指定输出目标（适用于 `snip`、`scroll` 与 `record`） |

区域/尺寸/全桌面/屏幕/上次/窗口等选区方式互斥，不可同时指定。

**图片输出目标**（`snip` / `scroll`）：支持 `clipboard`、`file-dialog`、`quick-save`、`pin`、`ocr_clipboard`、`printer`（调用系统 `lp`）、`barcode`、`success`/`silent`（跳过主输出，但按配置写入历史和自动保存）、`no-auto-save`，以及以 `.png`、`.jpg` 或 `.jpeg` 结尾的显式文件路径（直接落盘不弹窗）。包含分号或 `exec(...)` 的复合输出字符串会被拒绝。

**录屏输出目标**（`record -o`）：支持 `quick-save`、`file-dialog`，或以 `.mp4`、`.webm`、`.gif` 结尾的相对/绝对文件路径。未提供 `-o` 时，`Normal` 模式停止后打开回放编辑器，`Quick` 模式直接按快速保存设置输出。

## 贴图与分组管理

```sh
clivuu paste
clivuu paste --clipboard
clivuu paste --plain '原样保留的纯文本'
clivuu paste --html '<b>富文本格式</b>'
clivuu paste --files /path/to/dir image1.png nested/image2.jpg
clivuu paste --pos 200 160
```

来源选项（`--clipboard`、`--plain`、`--html`、`--files`）之间互斥，`--pos X Y` 用于为 `paste`、`reopen-pin` 或 `reopen-capture` 贴出的窗口指定初始逻辑坐标：
- `--files` 接收 `1` 到 `32` 个图片文件；参数列表中的目录项会作为后续相对路径的基准目录。
- `--plain` 与 `--html` 上限均为 `40,000` 字符；`--plain` 原样展示文本，不会把十六进制或 RGB 字符串自动转为色卡。
- 在交互式截图框选期间运行 `clivuu paste` 或 `clivuu paste --clipboard`，相当于触发截图界面上的 `Pin` 动作（`Ctrl+T`）并将完成事件返回给原截图请求。日常状态下匹配相同来源会前置并闪烁已有贴图；剪贴板为空时不会重新打开已关闭贴图（请使用 `reopen-pin`）。

| 子命令 | 功能描述 |
| :--- | :--- |
| `reopen-pin`、`reopen-pins` | 恢复最近关闭的一张或全部贴图 |
| `close-pins` | 关闭当前分组中的可见贴图（可恢复） |
| `destroy-closed-pins` | 永久清空已关闭贴图历史 |
| `show-images`、`hide-images`、`toggle-images` | 显示、隐藏或切换当前分组贴图 |
| `toggle-click-through`、`no-click-through` | 切换或解除全部贴图的鼠标穿透 |
| `create-group [NAME] [--color COLOR] [--no-switch] [--prompt]` | 创建贴图分组（支持颜色标记、不自动切换或弹出创建对话框） |
| `switch-group [NAME]` | 切换到指定分组，省略名称时顺次轮换 |
| `switch-group-prev` | 切回上一个分组 |
| `switch-groups` | 打开交互式贴图组快速切换窗口 |
| `show-group-manager` | 打开图形化贴图组管理窗口 |
| `rename-group NAME NEW` | 重命名已有的自定义贴图分组 |
| `delete-group NAME [--move-to GROUP]` | 删除自定义贴图分组并销毁或迁移其中的贴图 |
| `remove-empty-groups` | 清除当前未激活且无贴图的空分组 |
| `empty-group [NAME]` | 销毁当前分组或指定分组中的全部贴图 |
| `groups` | 以 JSON 返回分组列表、贴图计数与颜色标记元数据 |

## 历史记录、工具与服务控制

| 子命令 | 功能描述 |
| :--- | :--- |
| `capture-history` | 按时间倒序列出内存中的已完成截图记录 |
| `reopen-capture [INDEX]` | 贴出指定索引的最近截图（默认 `0` 为最新），或前置闪烁对应贴图 |
| `clear-snip-history` | 清空内存截图快照、历史选区列表与上次选区 |
| `pick-color` | 启动屏幕取色并复制 HEX 色值 |
| `barcode-scan` | 通过桌面服务框选屏幕区域并复制内置 `rxing` 识别的条码内容 |
| `barcode-decode FILE` | 通过桌面服务解码 `FILE` 并复制到剪贴板及打印 JSON（离线无界面解码可运行 `clivuu worker barcode --input FILE`） |
| `open-preferences` | 打开首选项窗口 |
| `start`、`status` | 启动后台服务 / 以 JSON 输出桌面环境、会话类型、显示后端、配置路径与可选工具（`ffmpeg`、`ffprobe`、`lp`、OCR 运行时与引擎） |
| `restart`、`exit` | 落盘状态并重启或停止桌面服务 |
| `portal -o FILE.png\|FILE.jpg\|FILE.jpeg` | 不通过后台服务直接调用静态截图 portal 保存图片（不覆盖已有文件） |
| `enable-hotkeys`、`disable-hotkeys`、`toggle-hotkeys` | 以退出码 `1` 返回说明，提示全局快捷键需在桌面或窗口管理器中配置 |

## 配置读写命令

```sh
clivuu config path
clivuu config dump
clivuu config defaults
clivuu config get theme
clivuu config set theme dark
clivuu config apply '{"theme":"dark","accent-color":"#bd93f9ff"}'
clivuu completions bash > ~/.local/share/bash-completion/completions/clivuu
```

所有 `config` 命令均按 `core/config-defaults.json` 校验数据类型与取值边界，完整字段清单见 [配置项参考](configuration.md)。`clivuu completions` 会基于 `cli/src/spec.rs` 与配置 Schema 动态生成全部子命令、选项及配置键补全脚本。

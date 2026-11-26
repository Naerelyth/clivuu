# 测试与验证

[English](../../en/development/testing.md) | **简体中文** · [使用手册](../README.md)

请在仓库根目录运行构建与测试命令，所有中间产物与测试临时文件均写入 `/tmp`。GUI 测试必须运行于隔离目录，严禁覆盖当前用户的真实配置、剪贴板、开机自启项或后台服务。

## 基础与静态检查

```sh
cargo build --locked
cargo test --workspace --locked
cargo fmt --all -- --check
cargo clippy --workspace --locked --all-targets -- -D warnings
node tests/i18n.mjs
GIO_USE_VFS=local dbus-run-session -- gjs -m tests/cli-readiness.js /tmp/clivuu-target/debug/clivuu
python3 -B tests/config.py
python3 -B tests/package_native.py
python3 -B tests/documentation.py
python3 -B tests/toolbar_parity_test.py
```

`tests/cli-readiness.js` 与 `tests/config.py` 依赖事先通过 `cargo build --locked` 编译的 debug 二进制。`make test` 运行默认自动测试集，`make check` 运行格式化与 Clippy 检查。`tests/package_native.py` 会在存在打包工具时校验 CMake 安装布局以及 TGZ、DEB、RPM 打包输出（传入 `--appimage-runtime /path/to/runtime-x86_64` 可同时测试本地 AppImage 构建）。容器与夹具检查使用 ELF 夹具与 `/tmp` 安装前缀，跨发行版动态库依赖解析仍需在目标系统上进行验证。

## 桌面与合成器集成测试

`tests/gnome.py` 在私有嵌套 GNOME Shell 会话中端到端验证遮罩、贴图、标注、保存框、首选项、快捷键、通知、长截图与录屏功能。查看全部测试模式：

```sh
python3 -B tests/gnome.py --help
```

运行使用独立测试探针的模式前，请构建与 `--binary` 同配置档位（release 或 debug）的示例程序：

- **`--rust-capture-consent`**：portal 截图授权流程、托盘与命令行触发、快门计数及系统通知（设 `CLIVUU_TEST_AUTOSTART=1` 测试私有 XDG 自启动）。
- **`--rust-color`**：基于私有 `color_probe` 测试反复屏幕取色、指针像素采样、取消与对话框销毁流程（`cargo build --release -p clivuu-desktop --example color_probe`）。
- **`--rust-continuous`**：利用 `continuous_probe` 与 `recording_controls_probe` 验证 `MP4`/`WebM`/`GIF` 实际输出、暂停时长扣减、倒计时取消、`Normal`/`Quick` 模式、回放编辑及原生 GTK 保存对话框。设置 `CLIVUU_TEST_NIRI_PREFIX` 可在私有 `niri` 安装下检查原生 `gtk4-layer-shell` 行为。
- **`--rust-playback`**：无虚拟音频条件下的停止录制转回放、时间轴裁剪、倍速、画布裁切、重复导出与取消保存检查。
- **`--rust-recording-audio`**：创建隔离的 PipeWire/PulseAudio 虚拟汇与 `440/880 Hz` 测试音频信号，检查设备自动启用、音轨分录、增益静音、Opus/AAC 封装与清理收尾，完全隔绝物理麦克风和外放硬件。
- **`--rust-cursor-scale`**：动态切换 `1×` / `2×` XSettings 缩放倍率，测量贴图、菜单与截图底图光标像素尺寸一致性。
- **`--rust-monitors`（可附加 `--single-monitor`）**：在 `2880×1800 + 1920×1080`（`200%/100%`）双屏及单屏虚拟显示器下对比像素棋盘格与原图/选区/导出清晰度（参见 [屏幕截图](../guide/capture.md)）。

独立录屏编码与 SSIM 像素质量自测（写入 `/tmp`）：

```sh
cargo test --locked -p clivuu-desktop --lib application::recording::encoding::tests -- --include-ignored --nocapture
```

## 媒体与硬件隔离要求

仅仅隔离会话 D-Bus 总线并不足以隔离物理音频或蓝牙外设。`tests/gnome-session.sh` 与 `tests/wireplumber-test.conf` 在测试会话中禁用了物理声卡、蓝牙与摄像头探测；修改会话脚本后需同步运行 `python3 -B tests/media-isolation.py` 验证媒体硬件隔离状态。

## 基准夹具与回归焦点

- `tests/reference/legacy` 中的冻结几何与图像测试输入通过校验清单核验，不随安装包发布，修复缺陷时不得为凑过测试随意修改基线。
- 回归测试应覆盖关键判别行为：多笔标注后单步撤销、滚轮事务合并、高速连续马赛克绘制、连续三次菜单打开/外部取消循环确保无残留隐形拦截层、取消导出恢复源素材、文件扩展名与实际字节编码一致以及对话框后缀同步刷新。

## 验证范围说明

汇报验证结果时应明确区分静态编译检查、单元测试、协议夹具、嵌套合成器测试与物理机实测。GNOME 或嵌套 `niri` 测试通过并不自动代表 KDE Plasma、Hyprland、物理混合缩放显示器或全部第三方 portal 后端表现一致。

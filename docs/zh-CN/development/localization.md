# 本地化开发

[English](../../en/development/localization.md) | **简体中文** · [使用手册](../README.md)

## 语言字典与解析顺序

`core/locales/languages.json` 注册了全部内置语言（包含 `id`、母语名称 `name`、别名 `aliases` 及 JSON 字典文件名 `catalog`）。`core/build.rs` 在编译期将字典文件嵌入 `clivuu_core::i18n`，并在首次访问时按需加载。首选项中的语言列表、界面文本以及命令行帮助与提示文案均共用同一套字典注册表。

语言按如下优先级确定：
1. 非空环境变量 `CLIVUU_LANGUAGE`
2. 配置文件 `language` 键（非 `"system"` 时）
3. 系统环境变量：首先检查 `LC_ALL`、`LC_MESSAGES` 与 `LANG` 是否强制指定了 `C` / `POSIX`，随后按顺序解析 `LANGUAGE`（冒号分隔列表），再回退到 `LC_ALL`、`LC_MESSAGES` 与 `LANG`；不支持或缺失词条默认回退到 `en`

界面语言列表以母语名显示（例如 `English`、`简体中文`），而字体名称、图片/视频格式名、键位标识与命名规则变量保留原始标识符。桌面通知中的“在文件管理器中显示 / Show in Files”固定根据系统语言选择翻译，不受界面语言偏好覆盖影响。

## 新增或更新翻译

1. 复制 `core/locales/en.json` 的所有键名至新语言文件 `core/locales/<lang>.json`，保持 `{0}`、`{1}` 等序号占位符完整（可根据语序调整前后顺序）。
2. 在 `core/locales/languages.json` 中注册语言元数据：
   ```json
   {"id": "fr", "name": "Français", "aliases": [], "catalog": "fr.json"}
   ```
3. 命令行子命令说明、参数提示与报错文本（`cli/src/`）同样通过 `t(...)` 与 `tf(...)` 读取 `<lang>.json` 字典，无需额外维护独立帮助文本文件。
4. 若某些译文需要与英文源文本保持一致，请在 `core/locales/languages.json` 对应语言条目的 `unchanged` 对象中注明原因（全局通用保留词维护在 `tests/i18n-invariants.json`）。
5. 运行字典一致性校验与本地化单元测试：
   ```sh
   node tests/i18n.mjs
   cargo test --locked -p clivuu-core i18n::
   ```

## 界面翻译写法

Rust 代码中使用 `t("Source text")` 获取静态翻译，使用 `tf("Message {0}", &[arg])` 进行带参插值。首选项控件与共用工具栏提示绑定时会保留原始英文源键，确保运行时切换语言能即时刷新全部可见界面文本，同时保留当前画布标注与撤销栈。

可在隔离桌面会话中对真实界面控件进行动态语言切换自检（需同时具备对应 release 二进制 `clivuu` 与 `i18n_probe`）：

```sh
make build
cargo build --release -p clivuu-desktop --example i18n_probe
python3 -B tests/gnome.py --rust-i18n --binary /tmp/clivuu-target/release/clivuu
```

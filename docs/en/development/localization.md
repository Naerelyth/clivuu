# Localization

**English** | [简体中文](../../zh-CN/development/localization.md) · [User Guide](../README.md)

## Catalogs & Language Resolution

`core/locales/languages.json` registers every supported language (`id`, native `name`, `aliases`, and JSON `catalog`). `core/build.rs` embeds these catalogs into `clivuu_core::i18n`, initializing catalogs on first lookup. Preferences, desktop dialogs, and localized CLI help/errors all share this registry.

Language selection order:
1. Non-empty `CLIVUU_LANGUAGE` environment variable
2. `language` setting in `config.ini` (when not `"system"`)
3. System locale: checks whether `LC_ALL`, `LC_MESSAGES`, or `LANG` forces `C`/`POSIX` first, then resolves `LANGUAGE` (colon-separated list) before falling back to `LC_ALL`, `LC_MESSAGES`, and `LANG`, defaulting to `en` for unsupported tags or missing keys

Language options render in their own script (`English`, `简体中文`), while font names, format names, shortcut legends, and naming template tokens keep their technical identifiers. The notification button `"Show in Files"` resolves from the desktop system locale independently of the application UI language override.

## Adding or Updating a Language

1. Copy all keys from `core/locales/en.json` into `core/locales/<lang>.json` and translate values while preserving numbered placeholders such as `{0}` and `{1}` (placeholders may be reordered freely).
2. Register the language in `core/locales/languages.json`:
   ```json
   {"id": "fr", "name": "Français", "aliases": [], "catalog": "fr.json"}
   ```
3. CLI command summaries, options, and error messages (`cli/src/`) also read directly from `<lang>.json` via `t(...)` and `tf(...)`; no separate CLI help text file is maintained.
4. If an entry intentionally matches English verbatim, document the key and reason inside that language's `unchanged` object in `core/locales/languages.json` (shared technical tokens are declared in `tests/i18n-invariants.json`).
5. Validate catalogs and unit tests:
   ```sh
   node tests/i18n.mjs
   cargo test --locked -p clivuu-core i18n::
   ```

## UI Translation APIs

Use `t("Source text")` for static strings and `tf("Message {0}", &[arg])` for formatted strings. Settings widgets and shared toolbar tooltips preserve source message keys so live language switching updates visible widgets immediately without losing annotations or undo history.

Verify UI translation coverage and live language switching in a private nested session (build `clivuu` alongside `i18n_probe`):

```sh
make build
cargo build --release -p clivuu-desktop --example i18n_probe
python3 -B tests/gnome.py --rust-i18n --binary /tmp/clivuu-target/release/clivuu
```

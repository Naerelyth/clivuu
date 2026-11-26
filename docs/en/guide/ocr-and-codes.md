# OCR & Barcode Recognition

[User Guide](../README.md) | **English** | [简体中文](../../zh-CN/guide/ocr-and-codes.md)

Clivuu runs offline text recognition through a dual-engine OCR subsystem in a short-lived worker process (`clivuu worker ocr-document` / `clivuu-desktop --ocr-worker`, `90 s` timeout, exiting once complete so model memory is freed immediately), and decodes QR codes and 1D/2D barcodes via `zbarimg`.

## Optical Character Recognition (OCR)

### Engines

- **PP-OCRv5 Mobile (`paddle`)**: runs via dynamically loaded (`dlopen`) ONNX Runtime (`libonnxruntime`, C API `v17+`). Clivuu reuses a compatible system library if present, or downloads a SHA-256-verified ONNX Runtime (`1.28.0` for `linux-x64` / `linux-aarch64`) into `$XDG_DATA_HOME/clivuu/ocr/runtime/` alongside on-demand SHA-256-verified language packs (`chinese`, `latin`, `korean`, `cyrillic`, `arabic`, `greek`, `thai`) into `$XDG_DATA_HOME/clivuu/ocr/models/`.
- **System Tesseract (`tesseract`)**: loads `libtesseract` 5 (`dlopen`) using TSV word-box layout and CJK spacing cleanup.
- **Engine selection (`ocr-engine`)**: `Automatic` (`auto`, default; uses installed PP-OCRv5 + ONNX Runtime first and falls back to system Tesseract 5), `PP-OCRv5 (Local)` (`paddle`), or `System Tesseract` (`tesseract`).

### Entry Points & In-Place Selection

- **Interactive capture (`O` or `Recognize text`)**: by default (`ocr-quick-copy=false`), opens in-place text selection directly over the capture region:
  - Drag across characters, double-click a line, or triple-click a paragraph block (`Ctrl+A` selects all text).
  - `Ctrl+C` copies selected or all recognized text with inline `Copied` feedback (no desktop notification is emitted).
  - `Enter` copies and closes capture; `Esc` clears any text selection first, then exits OCR mode.
  - The floating OCR bar provides an engine/model switcher, `Paragraphs` (`ocr-smart-paragraphs`, default `true`), `Select all`, `Pin text` (creates a plain-text pin), `Copy selected` / `Copy all`, and `OCR settings` (opens `Preferences -> OCR`). If no engine or model pack is installed yet, it shows an inline `Download model` setup prompt (`curl` required for automatic downloads).
- **Quick copy on capture (`Shift+O` or `ocr-quick-copy=true`)**: `Shift+O` during capture copies all recognized text immediately and exits; enabling `Copy immediately without selection` (`ocr-quick-copy=true`) makes `O` and `Recognize text` quick-copy directly too.
- **Floating pin (`O` or `Recognize text`)**: press `O` (when no drawing tool is active) or choose `Recognize text` from the pin context menu or toolbar to toggle live zoom-aware text selection (`Ctrl+A`, `Ctrl+C`, `Esc` to clear/exit; hold `Alt` to drag the pin over recognized text).
- **CLI**: `clivuu snip -o ocr_clipboard` (paired with `--area`, `--window`, `--last`, etc.) copies all recognized text directly to the clipboard and prints JSON `{"text":"...","rect":...}`.

### Preferences & Environment Overrides

`Preferences -> OCR` exposes three sections:

- **Recognition and copying**: `Recognition engine` (`ocr-engine`), `Keep paragraph breaks` (`ocr-smart-paragraphs`, default `true`), `Copy immediately without selection` (`ocr-quick-copy`, default `false`), and `Tesseract language packs` (`ocr-languages`, default `eng+chi_sim`, a multi-select checklist scanning installed `*.traineddata` files and offering distribution install commands; `clivuu config set ocr-languages eng+jpn+chi_sim` accepts `+`-joined codes).
- **Inference runtime**: ONNX Runtime status, automatic download, and `Open runtime folder` (`$XDG_DATA_HOME/clivuu/ocr/runtime/onnxruntime-1.28.0-<arch>/installed/`).
- **Local models (PP-OCRv5)**: pack selection (`ocr-model`, default `chinese`), on-demand SHA-256 downloads via `curl`, `Open models folder`, and `GitHub mirror prefix` (`ocr-download-mirror`).

Environment overrides for custom packaging or offline setups: `CLIVUU_ORT_LIB`, `CLIVUU_IGNORE_SYSTEM_ORT`, `CLIVUU_OCR_MODELS_DIR`, `CLIVUU_OCR_MIRROR`, `CLIVUU_DISABLE_TESSERACT`, and `TESSDATA_PREFIX`.

## QR & Barcode Scanning

Barcode decoding requires `zbarimg` (`zbar-tools` on Debian/Ubuntu, `zbar` on Fedora).

### Screen Scan (`clivuu barcode-scan`)

Run `clivuu barcode-scan` (or bind it to a desktop hotkey) to open the capture overlay, drag around one or more QR/barcodes on screen, and copy the decoded raw payload to the clipboard. You can also pass `-o barcode` to `clivuu snip`.

### Local Image Decode (`clivuu barcode-decode FILE`)

Decode an existing image file through the desktop service:

```bash
clivuu barcode-decode /path/to/qrcode.png
```

The desktop service runs `zbarimg --quiet --raw`, copies the decoded text to the clipboard, and prints JSON (`{"text":"..."}`). For standalone raw stdout output without starting the desktop service or touching the clipboard, run the CLI worker directly:

```bash
clivuu worker barcode /path/to/qrcode.png
```

## Related Manuals

- [Screenshot Capture](capture.md)
- [Preferences](preferences.md)
- [Troubleshooting](troubleshooting.md)

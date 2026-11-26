//! Dual-engine GUI-independent OCR subsystem:
//! - Primary (`Paddle`): PP-OCRv5 Mobile via dynamically loaded ONNX Runtime C API (`libonnxruntime`).
//! - Fallback (`Tesseract`): System `libtesseract.so.5` with TSV word geometry and CJK space cleanup.
//! - Shared layout, proportional character boundary solver, and interactive selection state machine.
pub mod assets;
pub mod layout;
pub mod model;
pub mod ort_api;
pub mod paddle;
pub mod selection;
pub mod tesseract;
pub mod worker;

pub use model::{
    OcrBlock, OcrDocument, OcrEngineChoice, OcrEngineUsed, OcrLine, OcrSettings, RectF,
};
pub use selection::{LineSelection, OcrSelection};

use crate::{Result, i18n::t, image::RgbaImage, image_ops::load};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineAvailability {
    pub runtime_ready: bool,
    pub model_ready: bool,
    pub any_model_ready: bool,
    pub tesseract_ready: bool,
}

impl EngineAvailability {
    pub fn can_recognize(&self, choice: OcrEngineChoice) -> bool {
        match choice {
            OcrEngineChoice::Paddle => self.runtime_ready && self.any_model_ready,
            OcrEngineChoice::Tesseract => self.tesseract_ready,
            OcrEngineChoice::Auto => {
                (self.runtime_ready && self.any_model_ready) || self.tesseract_ready
            }
        }
    }

    pub fn needs_paddle_download(&self, choice: OcrEngineChoice) -> bool {
        match choice {
            OcrEngineChoice::Tesseract => false,
            OcrEngineChoice::Paddle => !self.runtime_ready || !self.model_ready,
            OcrEngineChoice::Auto => {
                (!self.runtime_ready || !self.any_model_ready) && !self.tesseract_ready
            }
        }
    }
}

pub fn availability(settings: &OcrSettings) -> EngineAvailability {
    let runtime_ready = assets::find_onnxruntime_library().is_some();
    let model_ready = assets::is_model_installed(&settings.model_id);
    let any_model_ready = !assets::installed_models().is_empty();
    let tesseract_ready = tesseract::is_available();
    EngineAvailability {
        runtime_ready,
        model_ready,
        any_model_ready,
        tesseract_ready,
    }
}

/// Recognize an image into a structured `OcrDocument` with per-character boundaries.
pub fn recognize_document(image: &RgbaImage, settings: &OcrSettings) -> Result<OcrDocument> {
    match settings.engine {
        OcrEngineChoice::Paddle => recognize_paddle(image, &settings.model_id),
        OcrEngineChoice::Tesseract => {
            tesseract::recognize_document(image, &settings.tesseract_languages)
        }
        OcrEngineChoice::Auto => {
            if let Some(ort) = assets::find_onnxruntime_library()
                && let Some((spec, files)) = assets::resolve_model_files(&settings.model_id)
            {
                return paddle::recognize_document(image, ort.path(), spec, &files);
            }
            if tesseract::is_available() {
                return tesseract::recognize_document(image, &settings.tesseract_languages);
            }
            Err(t(
                "No OCR engine is ready; download a PP-OCRv5 language model in Settings -> OCR or install system Tesseract 5",
            )
            .into())
        }
    }
}

fn recognize_paddle(image: &RgbaImage, model_id: &str) -> Result<OcrDocument> {
    let ort = assets::find_onnxruntime_library().ok_or_else(|| {
        t("ONNX Runtime is not installed; install it in Settings -> OCR or via your package manager")
    })?;
    let (spec, files) = assets::resolve_model_files(model_id).ok_or_else(|| {
        t("PP-OCRv5 language model is not installed; download it in Settings -> OCR")
    })?;
    paddle::recognize_document(image, ort.path(), spec, &files)
}

/// Backwards-compatible file-based plain-text OCR entry point (`clivuu worker ocr`).
pub fn recognize(path: &str, languages: &str) -> Result<String> {
    recognize_image(&load(path)?, languages)
}

/// Backwards-compatible image-based plain-text OCR entry point.
pub fn recognize_image(image: &RgbaImage, languages: &str) -> Result<String> {
    // Validate explicit language parameter first to preserve existing contract checks.
    tesseract::validate_languages(languages)?;
    let mut settings = crate::config::read()
        .map(|vals| OcrSettings::from_values(&vals))
        .unwrap_or_default();
    settings.tesseract_languages = languages.to_owned();
    let doc = recognize_document(image, &settings)?;
    let mut text = doc.full_text(settings.smart_paragraphs);
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn char_boundaries_anchor_to_edges_and_stay_monotonic() {
        let bounds = RectF::from_ltrb(100.0, 10.0, 610.0, 50.0).unwrap();
        let xs = layout::char_boundaries(&bounds, "MONDAY", &[0.2, 0.35, 0.45, 0.6, 0.75, 0.9]);
        assert_eq!(xs.len(), 7);
        assert_eq!((xs[0], xs[6]), (100.0, 610.0));
        assert!(xs.windows(2).all(|w| w[0] <= w[1]));
        // Narrow 'I' should occupy narrower span than wide 'W'.
        let prop = layout::char_boundaries(&bounds, "IW", &[0.25, 0.75]);
        assert!(prop[1] - prop[0] < prop[2] - prop[1]);
    }

    #[test]
    fn small_crops_are_padded_to_multiples_of_32_with_border_median() {
        let mut img = ::image::RgbImage::from_pixel(100, 20, ::image::Rgb([200, 210, 220]));
        img.put_pixel(50, 10, ::image::Rgb([10, 20, 30]));
        let (out, (left, top)) = paddle::pad_for_detection(&img);
        assert_eq!((out.width() % 32, out.height() % 32), (0, 0));
        assert!(left >= 16 && top >= 16);
        assert_eq!(out.get_pixel(left + 50, top + 10).0, [10, 20, 30]);
        assert_eq!(out.get_pixel(0, 0).0, [200, 210, 220]);
    }

    #[test]
    fn white_matte_preserves_opaque_pixels_and_whitens_transparent_background() {
        let mut rgba = RgbaImage::new(2, 1);
        rgba.put_pixel(0, 0, crate::image::Rgba([0, 0, 0, 0]));
        rgba.put_pixel(1, 0, crate::image::Rgba([20, 40, 60, 255]));
        let rgb = paddle::rgba_to_white_matted_rgb(&rgba);
        assert_eq!(rgb.get_pixel(0, 0).0, [255, 255, 255]);
        assert_eq!(rgb.get_pixel(1, 0).0, [20, 40, 60]);
    }

    #[test]
    fn tsv_parser_strips_spurious_cjk_spaces_while_preserving_latin_spaces() {
        let tsv = [
            "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext",
            "5\t1\t1\t1\t1\t1\t10\t20\t24\t24\t95.0\t中",
            "5\t1\t1\t1\t1\t2\t36\t20\t24\t24\t96.0\t文",
            "5\t1\t1\t1\t1\t3\t68\t20\t48\t24\t94.0\tHello",
            "5\t1\t1\t1\t1\t4\t124\t20\t52\t24\t93.0\tWorld",
        ]
        .join("\n");
        let lines = tesseract::parse_tsv_lines(&tsv, 300.0, 100.0);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "中文 Hello World");
        assert_eq!(
            lines[0].char_x.len(),
            "中文 Hello World".chars().count() + 1
        );
        assert!(lines[0].char_x.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn selection_confines_horizontal_drag_to_anchor_block_and_supports_line_and_block_select() {
        let left_col = vec![
            OcrLine {
                text: "LeftOne".into(),
                bounds: RectF::from_xywh(10.0, 10.0, 80.0, 20.0).unwrap(),
                char_x: vec![10.0, 21.0, 32.0, 43.0, 54.0, 65.0, 76.0, 90.0],
            },
            OcrLine {
                text: "LeftTwo".into(),
                bounds: RectF::from_xywh(10.0, 34.0, 80.0, 20.0).unwrap(),
                char_x: vec![10.0, 21.0, 32.0, 43.0, 54.0, 65.0, 76.0, 90.0],
            },
        ];
        let right_col = OcrLine {
            text: "RightCol".into(),
            bounds: RectF::from_xywh(300.0, 10.0, 90.0, 20.0).unwrap(),
            char_x: vec![
                300.0, 311.0, 322.0, 333.0, 344.0, 355.0, 366.0, 377.0, 390.0,
            ],
        };
        let mut all_lines = left_col;
        all_lines.push(right_col);
        let doc = OcrDocument::from_lines(all_lines, 500, 200, OcrEngineUsed::Paddle);
        assert_eq!(doc.blocks.len(), 2);

        let mut sel = OcrSelection::default();
        sel.set_document(doc);
        // Dragging horizontally from LeftOne into empty space at y=20 stays inside left block!
        sel.begin_drag((10.0, 20.0));
        sel.extend_drag((250.0, 20.0));
        assert_eq!(sel.selected_text(true).as_deref(), Some("LeftOne"));

        // Double-click equivalent (`select_line`) vs triple-click (`select_block`).
        sel.select_line(0);
        assert_eq!(sel.selected_text(true).as_deref(), Some("LeftOne"));
        sel.select_block(0);
        assert_eq!(sel.selected_text(true).as_deref(), Some("LeftOne\nLeftTwo"));

        sel.select_all();
        assert_eq!(
            sel.selected_text(true).as_deref(),
            Some("LeftOne\nLeftTwo\n\nRightCol")
        );
        assert_eq!(
            sel.selected_text(false).as_deref(),
            Some("LeftOne\nLeftTwo\nRightCol")
        );
    }

    #[test]
    fn worker_framing_roundtrips_document_and_errors() {
        let mut rgba = RgbaImage::new(4, 3);
        rgba.put_pixel(2, 1, crate::image::Rgba([10, 20, 30, 255]));
        let settings = OcrSettings::default();
        let mut wire = Vec::new();
        worker::write_request(&mut wire, &rgba, &settings).unwrap();
        let (decoded_img, decoded_cfg) =
            worker::read_request(&mut std::io::Cursor::new(&wire)).unwrap();
        assert_eq!(decoded_img.dimensions(), (4, 3));
        assert_eq!(decoded_img.get_pixel(2, 1).0, [10, 20, 30, 255]);
        assert_eq!(decoded_cfg, settings);

        let doc = OcrDocument::from_lines(
            vec![OcrLine {
                text: "Clivuu".into(),
                bounds: RectF::from_xywh(1.0, 2.0, 30.0, 10.0).unwrap(),
                char_x: vec![1.0, 6.0, 11.0, 16.0, 21.0, 26.0, 31.0],
            }],
            40,
            20,
            OcrEngineUsed::Paddle,
        );
        let mut reply_buf = Vec::new();
        worker::write_reply(&mut reply_buf, &worker::WorkerReply::Document(doc.clone())).unwrap();
        let parsed = worker::read_reply(&mut std::io::Cursor::new(&reply_buf)).unwrap();
        assert_eq!(parsed, doc);
    }

    #[test]
    fn row_line_sort_orders_horizontally_aligned_cells_left_to_right_despite_vertical_jitter() {
        let mut lines = vec![
            OcrLine {
                text: "Item".into(),
                bounds: RectF::from_xywh(32.15, 24.76, 63.64, 34.46).unwrap(),
                char_x: vec![32.0, 48.0, 64.0, 80.0, 96.0],
            },
            OcrLine {
                text: "Total".into(),
                bounds: RectF::from_xywh(473.01, 25.01, 62.96, 31.96).unwrap(),
                char_x: vec![473.0, 485.0, 498.0, 510.0, 523.0, 536.0],
            },
            OcrLine {
                text: "Count".into(),
                bounds: RectF::from_xywh(253.98, 25.98, 77.02, 31.02).unwrap(),
                char_x: vec![254.0, 269.0, 285.0, 300.0, 315.0, 331.0],
            },
        ];
        layout::sort_lines_by_rows(&mut lines);
        let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, vec!["Item", "Count", "Total"]);
    }
}

//! Shared GUI-independent OCR geometry, document structure, and settings.
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RectF {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl RectF {
    pub fn from_ltrb(left: f32, top: f32, right: f32, bottom: f32) -> Option<Self> {
        if !left.is_finite()
            || !top.is_finite()
            || !right.is_finite()
            || !bottom.is_finite()
            || right <= left
            || bottom <= top
        {
            return None;
        }
        Some(Self {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        })
    }

    pub fn from_xywh(x: f32, y: f32, width: f32, height: f32) -> Option<Self> {
        if !x.is_finite()
            || !y.is_finite()
            || !width.is_finite()
            || !height.is_finite()
            || width <= 0.0
            || height <= 0.0
        {
            return None;
        }
        Some(Self {
            x,
            y,
            width,
            height,
        })
    }

    pub fn left(self) -> f32 {
        self.x
    }

    pub fn top(self) -> f32 {
        self.y
    }

    pub fn right(self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(self) -> f32 {
        self.y + self.height
    }

    pub fn center_y(self) -> f32 {
        self.y + self.height * 0.5
    }

    pub fn intersect(self, other: Self) -> Option<Self> {
        Self::from_ltrb(
            self.left().max(other.left()),
            self.top().max(other.top()),
            self.right().min(other.right()),
            self.bottom().min(other.bottom()),
        )
    }

    pub fn union(self, other: Self) -> Self {
        Self::from_ltrb(
            self.left().min(other.left()),
            self.top().min(other.top()),
            self.right().max(other.right()),
            self.bottom().max(other.bottom()),
        )
        .unwrap_or(self)
    }

    pub fn contains(self, x: f32, y: f32) -> bool {
        x >= self.left() && x <= self.right() && y >= self.top() && y <= self.bottom()
    }

    pub fn scaled(self, factor: f32) -> Self {
        if !factor.is_finite() || factor <= 0.0 {
            return self;
        }
        Self {
            x: self.x * factor,
            y: self.y * factor,
            width: self.width * factor,
            height: self.height * factor,
        }
    }
}

/// One visual text line with character-level X boundaries (`char_count + 1` values).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OcrLine {
    pub text: String,
    pub bounds: RectF,
    pub char_x: Vec<f32>,
}

impl OcrLine {
    pub fn char_count(&self) -> usize {
        self.text.chars().count()
    }
}

/// Reading-order paragraph/column block referencing indices in `OcrDocument::lines`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OcrBlock {
    pub lines: Vec<usize>,
    pub bounds: RectF,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OcrEngineChoice {
    #[default]
    Auto,
    Paddle,
    Tesseract,
}

impl OcrEngineChoice {
    pub fn parse(value: &str) -> Self {
        match value.trim() {
            "paddle" => Self::Paddle,
            "tesseract" => Self::Tesseract,
            _ => Self::Auto,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Paddle => "paddle",
            Self::Tesseract => "tesseract",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OcrEngineUsed {
    #[default]
    Paddle,
    Tesseract,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OcrDocument {
    pub lines: Vec<OcrLine>,
    pub blocks: Vec<OcrBlock>,
    pub width: u32,
    pub height: u32,
    pub engine: OcrEngineUsed,
}

impl OcrDocument {
    pub fn from_lines(lines: Vec<OcrLine>, width: u32, height: u32, engine: OcrEngineUsed) -> Self {
        let blocks = super::layout::group_blocks(&lines);
        Self {
            lines,
            blocks,
            width,
            height,
            engine,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Extract all recognized text in reading order.
    pub fn full_text(&self, smart_paragraphs: bool) -> String {
        let mut sel = super::selection::OcrSelection::default();
        sel.set_document(self.clone());
        sel.all_text(smart_paragraphs)
    }

    /// Scale pixel-space line and block geometry into logical coordinates (`1.0 / scale`).
    pub fn scale_by(&mut self, factor: f32) {
        if !factor.is_finite() || factor <= 0.0 || (factor - 1.0).abs() <= f32::EPSILON {
            return;
        }
        for line in &mut self.lines {
            line.bounds = line.bounds.scaled(factor);
            for x in &mut line.char_x {
                *x *= factor;
            }
        }
        for block in &mut self.blocks {
            block.bounds = block.bounds.scaled(factor);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcrSettings {
    pub engine: OcrEngineChoice,
    pub model_id: String,
    pub tesseract_languages: String,
    pub smart_paragraphs: bool,
    pub quick_copy: bool,
    pub download_mirror: String,
}

impl Default for OcrSettings {
    fn default() -> Self {
        Self {
            engine: OcrEngineChoice::Auto,
            model_id: super::assets::recommended_model_id().to_owned(),
            tesseract_languages: "eng+chi_sim".to_owned(),
            smart_paragraphs: true,
            quick_copy: false,
            download_mirror: String::new(),
        }
    }
}

impl OcrSettings {
    pub fn from_values(values: &Map<String, Value>) -> Self {
        let engine = values
            .get("ocr-engine")
            .and_then(Value::as_str)
            .map(OcrEngineChoice::parse)
            .unwrap_or(OcrEngineChoice::Auto);
        let model_id = values
            .get("ocr-model")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("chinese")
            .to_owned();
        let tesseract_languages = values
            .get("ocr-languages")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("eng+chi_sim")
            .to_owned();
        let smart_paragraphs = values
            .get("ocr-smart-paragraphs")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let quick_copy = values
            .get("ocr-quick-copy")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let download_mirror = values
            .get("ocr-download-mirror")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_owned();
        Self {
            engine,
            model_id,
            tesseract_languages,
            smart_paragraphs,
            quick_copy,
            download_mirror,
        }
    }
}

use super::arrow::{ArrowType, LineStyle};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub type Point = [f64; 2];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Rect,
    Ellipse,
    Arrow,
    Pen,
    Highlight,
    Sequence,
    Mosaic,
    Text,
    Eraser,
}
impl Kind {
    pub const ALL: [Self; 9] = [
        Self::Rect,
        Self::Ellipse,
        Self::Arrow,
        Self::Pen,
        Self::Highlight,
        Self::Sequence,
        Self::Mosaic,
        Self::Text,
        Self::Eraser,
    ];
    pub fn stroke_range(self, scale: f64) -> [f64; 3] {
        let broad = matches!(
            self,
            Self::Pen | Self::Highlight | Self::Mosaic | Self::Eraser | Self::Sequence
        );
        [
            if matches!(self, Self::Highlight | Self::Mosaic | Self::Eraser) {
                (4. * scale).floor().max(1.)
            } else {
                1.
            },
            ((if broad { 50. } else { 16. }) * scale).ceil().max(1.),
            if broad { scale.floor().max(1.) } else { 1. },
        ]
    }
    pub fn supports_fill(self, fill: Fill) -> bool {
        self != Self::Mosaic || matches!(fill, Fill::Stroke | Fill::Rect)
    }
    pub fn stroke_width(self, width: f64, scale: f64) -> f64 {
        let [min, max, _] = self.stroke_range(scale);
        width.clamp(min, max)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Fill {
    #[default]
    Stroke,
    Solid,
    Rect,
    Ellipse,
    Line,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Transform {
    pub x: f64,
    pub y: f64,
    pub sx: f64,
    pub sy: f64,
    pub angle: f64,
}
impl Default for Transform {
    fn default() -> Self {
        Self {
            x: 0.,
            y: 0.,
            sx: 1.,
            sy: 1.,
            angle: 0.,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Style {
    pub color: String,
    pub width: f64,
    pub fill: Fill,
    pub effect: String,
    pub effect_level: f64,
    pub effect_scale: f64,
    pub alpha: u8,
    pub arrow_type: ArrowType,
    pub line_style: LineStyle,
    pub font_family: String,
    pub font_size: f64,
    pub font_unit: String,
    pub font_scale: f64,
    pub bold: bool,
    pub italic: bool,
    pub outline_mode: String,
    pub outline_last_mode: String,
    pub outline_width: f64,
    pub outline_color: String,
    pub sequence: super::sequence::Sequence,
}
impl Default for Style {
    fn default() -> Self {
        Self {
            color: "#ff6058".into(),
            width: 3.,
            fill: Fill::Stroke,
            effect: "pixelate".into(),
            effect_level: f64::from(crate::image::effects::Effect::Pixelate.strength_range()[2]),
            effect_scale: 1.,
            alpha: 255,
            arrow_type: ArrowType::default(),
            line_style: LineStyle::default(),
            font_family: "Sans".into(),
            font_size: 18.,
            font_unit: "pt".into(),
            font_scale: 1.,
            bold: false,
            italic: false,
            outline_mode: "none".into(),
            outline_last_mode: "auto".into(),
            outline_width: 1.,
            outline_color: "#000000".into(),
            sequence: Default::default(),
        }
    }
}
impl Style {
    pub fn mosaic_effect(&self) -> crate::image::effects::Effect {
        if self.effect == "blur" {
            crate::image::effects::Effect::Blur
        } else {
            crate::image::effects::Effect::Pixelate
        }
    }
    pub fn mosaic_level(&self) -> u8 {
        self.mosaic_effect().strength(self.effect_level)
    }
    pub fn for_kind(kind: Kind, scale: f64) -> Self {
        Self {
            effect_scale: if kind == Kind::Mosaic { scale } else { 1. },
            width: if kind == Kind::Text {
                3.
            } else {
                kind.stroke_width(
                    match kind {
                        Kind::Highlight | Kind::Eraser => 14.,
                        Kind::Mosaic => 12.,
                        Kind::Sequence => 16.,
                        _ => 3.,
                    } * scale,
                    scale,
                )
            },
            alpha: if kind == Kind::Highlight { 82 } else { 255 },
            font_scale: if kind == Kind::Text { scale } else { 1. },
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", try_from = "ImportedMark")]
pub struct Mark {
    pub kind: Kind,
    pub points: Arc<Vec<Point>>,
    #[serde(flatten)]
    pub style: Style,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<Transform>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_size: Option<Point>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub preedit_frame: bool,
}
impl Mark {
    pub fn new(kind: Kind, point: Point, scale: f64) -> Self {
        Self {
            kind,
            points: Arc::new(vec![point]),
            style: Style::for_kind(kind, scale),
            transform: (kind == Kind::Text).then_some(Transform {
                sx: 0.625,
                sy: 0.625,
                ..Transform::default()
            }),
            text: (kind == Kind::Text).then(String::new),
            text_size: None,
            preedit_frame: false,
        }
    }
    pub fn transform(&self) -> Transform {
        self.transform.unwrap_or_default()
    }
    pub fn freehand(&self) -> bool {
        self.kind == Kind::Pen
            || (matches!(self.kind, Kind::Highlight | Kind::Mosaic | Kind::Eraser)
                && self.style.fill == Fill::Stroke)
    }
    pub fn reselectable(&self) -> bool {
        matches!(self.kind, Kind::Text | Kind::Sequence)
    }
    pub fn line(&self) -> bool {
        self.kind == Kind::Arrow || (self.kind == Kind::Highlight && self.style.fill == Fill::Line)
    }
    pub fn complete(&self) -> bool {
        if self.points.is_empty() {
            return false;
        }
        if self.kind == Kind::Eraser {
            return self.style.fill == Fill::Stroke
                || self.points.len() > 1
                    && self.points[0][0] != self.points.last().unwrap()[0]
                    && self.points[0][1] != self.points.last().unwrap()[1];
        }
        if self.kind == Kind::Sequence {
            return true;
        }
        self.points.len() > 1
    }
    pub fn text_pixels(&self, unit: f64) -> f64 {
        if self.style.font_unit == "pt" {
            self.style.font_size.clamp(1., 300.) * 96. / 72. * self.style.font_scale / unit
        } else {
            self.style.font_size / unit
        }
    }
    pub fn text_level(&self) -> f64 {
        (self.text_pixels(1.) * self.transform().sx / self.style.font_scale / 24. - 0.25) / 0.125
            + 1.
    }
    pub fn valid(&self) -> bool {
        !self.points.is_empty()
            && self.kind.supports_fill(self.style.fill)
            && self.points.iter().flatten().all(|v| v.is_finite())
            && [
                self.style.width,
                self.style.font_size,
                self.style.font_scale,
                self.style.effect_level,
                self.style.effect_scale,
                self.style.outline_width,
            ]
            .iter()
            .all(|v| v.is_finite())
            && self.style.effect_scale > 0.
            && self.style.font_scale > 0.
            && self.style.font_size > 0.
            && (self.kind != Kind::Text || self.text.is_some())
            && self.style.width > 0.
            && (1..=super::sequence::MAX_INDEX).contains(&self.style.sequence.index)
            && self
                .text_size
                .is_none_or(|s| s.iter().all(|v| v.is_finite() && *v >= 0.))
            && {
                let t = self.transform();
                [t.x, t.y, t.sx, t.sy, t.angle]
                    .iter()
                    .all(|v| v.is_finite())
                    && t.sx > 0.
                    && t.sy > 0.
            }
    }
}

// Keep compatibility at the document boundary, with no legacy endpoint state
// in the editable model or serialized output.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportedMark {
    kind: Kind,
    points: Arc<Vec<Point>>,
    #[serde(default)]
    transform: Option<Transform>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    text_size: Option<Point>,
    #[serde(default)]
    preedit_frame: bool,
    #[serde(flatten)]
    fields: serde_json::Map<String, serde_json::Value>,
}
impl TryFrom<ImportedMark> for Mark {
    type Error = serde_json::Error;
    fn try_from(mut raw: ImportedMark) -> Result<Self, Self::Error> {
        let fields = &mut raw.fields;
        if !fields.contains_key("arrowType")
            && ["startStyle", "endStyle", "arrowStart", "arrowEnd"]
                .iter()
                .any(|k| fields.contains_key(*k))
        {
            let head = |name: &str, flag: &str, default: bool| {
                fields
                    .get(name)
                    .and_then(|v| v.as_str())
                    .unwrap_or_else(|| {
                        if fields
                            .get(flag)
                            .and_then(|v| v.as_bool())
                            .unwrap_or(default)
                        {
                            "open"
                        } else {
                            "none"
                        }
                    })
            };
            let start = head("startStyle", "arrowStart", false);
            let end = head("endStyle", "arrowEnd", true);
            let start_only = start != "none" && end == "none";
            let both = start != "none" && end != "none";
            let open = if start_only {
                start == "open"
            } else {
                end == "open"
            };
            let kind = match (both, open) {
                _ if start == "none" && end == "none" => ArrowType::PlainLine,
                (true, true) => ArrowType::LineBilateral,
                (false, true) => ArrowType::Line,
                (true, false) => ArrowType::StraightBilateral,
                _ => ArrowType::Straight,
            };
            fields.insert("arrowType".into(), serde_json::to_value(kind)?);
            if start_only && raw.kind == Kind::Arrow {
                Arc::make_mut(&mut raw.points).reverse();
            }
        }
        let explicit_level = raw.fields.contains_key("effectLevel");
        let mut style = Style::deserialize(serde_json::Value::Object(raw.fields))?;
        if raw.kind == Kind::Mosaic && !explicit_level {
            style.effect_level = f64::from(style.mosaic_effect().strength_range()[2]);
        }
        if !raw.kind.supports_fill(style.fill) {
            return Err(serde::de::Error::custom(crate::i18n::t(
                "Unsupported fill for annotation kind",
            )));
        }
        Ok(Self {
            kind: raw.kind,
            points: raw.points,
            style,
            transform: raw.transform,
            text: raw.text,
            text_size: raw.text_size,
            preedit_frame: raw.preedit_frame,
        })
    }
}

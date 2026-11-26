//! Content and request policy for every paste entry point; no desktop objects.
use crate::{annotation::Bounds, i18n::t, pins::Board};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
pub mod html;
pub const MAX_TEXT: usize = 40_000;
pub const MAX_FILES: usize = 32;

#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    Clipboard,
    Text { text: String, html: bool },
    Files(Vec<PathBuf>),
}
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub source: Source,
    pub position: Option<[f64; 2]>,
}
impl Request {
    pub fn parse(value: &Value) -> Result<Self, String> {
        let explicit = value.get("source").and_then(Value::as_str);
        let sources = usize::from(value.get("text").is_some())
            + usize::from(value.get("files").is_some())
            + usize::from(explicit == Some("clipboard"));
        if sources > 1 {
            return Err(t("Choose one paste source: clipboard, text, HTML or files"));
        }
        let source = if let Some(files) = value.get("files") {
            let files = files.as_array().ok_or_else(|| t("Invalid paste request"))?;
            if files.is_empty() || files.len() > MAX_FILES {
                return Err(t("--files requires 1 to 32 image files"));
            }
            Source::Files(
                files
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .filter(|s| !s.is_empty())
                            .map(PathBuf::from)
                            .ok_or_else(|| t("Invalid paste request"))
                    })
                    .collect::<Result<_, _>>()?,
            )
        } else if let Some(text) = value.get("text") {
            let text = text.as_str().ok_or_else(|| t("Invalid paste request"))?;
            check_text(text)?;
            Source::Text {
                text: text.into(),
                html: value["html"].as_bool().unwrap_or(false),
            }
        } else {
            Source::Clipboard
        };
        if let Some(kind) = explicit {
            let valid = matches!(
                (&source, kind),
                (Source::Clipboard, "clipboard")
                    | (Source::Files(_), "files")
                    | (Source::Text { html: false, .. }, "plain")
                    | (Source::Text { html: true, .. }, "html")
            );
            if !valid {
                return Err(t("Invalid paste request"));
            }
        }
        let position = value
            .get("position")
            .map(|p| {
                let a = p
                    .as_array()
                    .filter(|v| v.len() == 2)
                    .ok_or_else(|| t("Invalid paste position"))?;
                let coordinate = |v: &Value| {
                    v.as_f64()
                        .filter(|n| n.is_finite() && n.abs() <= 1_000_000.)
                        .ok_or_else(|| t("Invalid paste position"))
                };
                Ok::<_, String>([coordinate(&a[0])?, coordinate(&a[1])?])
            })
            .transpose()?;
        Ok(Self { source, position })
    }
}
pub fn check_text(text: &str) -> Result<(), String> {
    if text.chars().count() > MAX_TEXT {
        Err(t("Pin text is too long"))
    } else {
        Ok(())
    }
}

/// Domain-separated stable identity, persisted with the pin, not its pixels.
pub fn identity(kind: &str, bytes: &[u8]) -> String {
    identity_parts(kind, &[bytes])
}

pub fn identity_parts(kind: &str, parts: &[&[u8]]) -> String {
    let mut digest = Sha256::new();
    digest.update(kind.as_bytes());
    digest.update([0]);
    for part in parts {
        digest.update(part);
    }
    format!("{:x}", digest.finalize())
}

/// Snipaste's clipboard color syntax is narrower than CSS color syntax.
pub fn color(text: &str) -> Option<[u8; 4]> {
    let s = text.trim();
    if let Some(hex) = s.strip_prefix('#') {
        if ![6, 8].contains(&hex.len()) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let v = u32::from_str_radix(hex, 16).ok()?;
        return Some(if hex.len() == 6 {
            [(v >> 16) as u8, (v >> 8) as u8, v as u8, 255]
        } else {
            [(v >> 16) as u8, (v >> 8) as u8, v as u8, (v >> 24) as u8]
        });
    }
    let s = s
        .strip_prefix("rgb(")
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or(s);
    let values: Vec<_> = s
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .collect();
    if values.len() != 3 {
        return None;
    }
    let fractional = values.iter().any(|s| s.contains('.'));
    let mut result = [0, 0, 0, 255];
    for (i, s) in values.iter().enumerate() {
        if fractional {
            let v = s.parse::<f64>().ok()?;
            if !v.is_finite() || !(0. ..=1.).contains(&v) {
                return None;
            }
            result[i] = (v * 255.).round() as u8;
        } else {
            result[i] = s.parse::<u8>().ok()?;
        }
    }
    Some(result)
}

/// Reuse only the requested source; unrelated closed pins are never a fallback.
pub fn existing(board: &Board, source: &str) -> Option<u64> {
    board
        .pins
        .iter()
        .rev()
        .find(|p| p.source.as_deref() == Some(source))
        .map(|p| p.id)
}

/// Bitmap imports map one image pixel to one output pixel. Generated text/cards
/// retain their UI size in desktop coordinates.
#[derive(Clone, Copy)]
pub enum Units {
    PhysicalPixels,
    LogicalPixels,
}
pub fn target(
    outputs: &[crate::capture::Output],
    position: Option<[f64; 2]>,
    focused: Option<Bounds>,
) -> Option<crate::capture::Output> {
    position
        .and_then(|p| outputs.iter().find(|o| crate::capture::contains(o.rect, p)))
        .or_else(|| {
            focused.and_then(|window| {
                outputs
                    .iter()
                    .filter_map(|o| {
                        let overlap = crate::capture::intersect(o.rect, window);
                        let area = overlap.width * overlap.height;
                        (area > 0.).then_some((o, area))
                    })
                    .max_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(output, _)| output)
            })
        })
        .or_else(|| outputs.first())
        .copied()
}
pub fn centered(size: [u32; 2], output: crate::capture::Output, units: Units) -> Bounds {
    let scale = match units {
        Units::PhysicalPixels if output.scale.is_finite() && output.scale > 0. => output.scale,
        _ => 1.,
    };
    let [width, height] = size.map(|n| n as f64 / scale);
    Bounds {
        x: (output.rect.x + (output.rect.width - width) / 2.).floor(),
        y: (output.rect.y + (output.rect.height - height) / 2.).floor(),
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn imported_pixels_and_generated_ui_have_distinct_scale_rules() {
        for scale in [1., 1.25, 1.5, 2.] {
            let output = crate::capture::Output {
                rect: Bounds {
                    x: -800.,
                    y: 0.,
                    width: 800.,
                    height: 600.,
                },
                scale,
            };
            let bitmap = centered([400, 200], output, Units::PhysicalPixels);
            assert_eq!([bitmap.width * scale, bitmap.height * scale], [400., 200.]);
            let card = centered([158, 140], output, Units::LogicalPixels);
            assert_eq!([card.width, card.height], [158., 140.]);
        }
        let outputs = [
            crate::capture::Output {
                rect: Bounds {
                    x: -800.,
                    y: 0.,
                    width: 800.,
                    height: 600.,
                },
                scale: 1.,
            },
            crate::capture::Output {
                rect: Bounds {
                    x: 0.,
                    y: 0.,
                    width: 800.,
                    height: 600.,
                },
                scale: 2.,
            },
        ];
        assert_eq!(
            target(&outputs, Some([10., 20.]), Some(outputs[0].rect))
                .unwrap()
                .scale,
            2.
        );
        assert_eq!(
            target(&outputs, None, Some(outputs[0].rect)).unwrap().scale,
            1.
        );
        assert_eq!(target(&outputs, None, None).unwrap().rect.x, -800.);
    }
    #[test]
    fn sources_are_explicit_and_legacy_requests_still_work() {
        assert_eq!(
            Request::parse(&json!({})).unwrap().source,
            Source::Clipboard
        );
        assert!(Request::parse(&json!({"source":"clipboard","text":"x"})).is_err());
        assert!(Request::parse(&json!({"files":["x"],"text":"y"})).is_err());
        assert!(Request::parse(&json!({"source":"html","text":"x","html":false})).is_err());
        assert!(Request::parse(&json!({"position":[0,1e30]})).is_err());
        assert!(Request::parse(&json!({"files":[]})).is_err());
        assert!(matches!(
            Request::parse(&json!({"text":"<b>x</b>","html":true}))
                .unwrap()
                .source,
            Source::Text { html: true, .. }
        ));
    }
    #[test]
    fn colors_use_reference_hex_and_rgb_domains() {
        assert_eq!(color("#fF0000"), Some([255, 0, 0, 255]));
        assert_eq!(color("#80010203"), Some([1, 2, 3, 128]));
        assert_eq!(color("255, 0, 0"), color("1.0, 0.0, 0.0"));
        for s in [
            "#f00",
            "red",
            "256, 0, 0",
            "NaN, 0.0, 0.0",
            "1.1, 0.0, 0.0",
            "1 2 3 4",
            "hello",
        ] {
            assert_eq!(color(s), None, "{s}");
        }
    }
    #[test]
    fn paste_identity_and_history_are_stable_without_resident_pixels() {
        let mut b = Board::default();
        let r = Bounds {
            x: 0.,
            y: 0.,
            width: 20.,
            height: 30.,
        };
        let a = b.add(r).unwrap();
        b.get_mut(a).unwrap().source = Some(identity("text", b"A"));
        let c = b.add(r).unwrap();
        b.get_mut(c).unwrap().source = Some(identity("text", b"B"));
        b.close(a, 2);
        b.close(c, 3);
        assert_eq!(existing(&b, &identity("text", b"A")), Some(a));
        assert_eq!(existing(&b, &identity("text", b"C")), None);
        b.pins.retain(|p| p.id != c);
        assert_eq!(existing(&b, &identity("text", b"B")), None);
        let v = serde_json::to_value(&b).unwrap();
        let restored: Board = serde_json::from_value(v).unwrap();
        assert_eq!(restored.pins[0].source, b.get(a).unwrap().source);
        assert_ne!(identity("text", b"x"), identity("html", b"x"));
    }
}

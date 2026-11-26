//! HTML-to-rich-text structure. Parsing never loads files, URLs or scripts.
use super::{MAX_TEXT, check_text};
use crate::i18n::t;
use html5gum::{Token, Tokenizer};
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub size: Option<f64>,
    pub family: Option<String>,
    pub foreground: Option<String>,
    pub background: Option<String>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub text: String,
    pub style: Style,
}
#[derive(Clone, Debug, Default)]
pub struct Document {
    pub runs: Vec<Run>,
}
impl Document {
    pub fn plain_text(text: &str) -> Self {
        Self {
            runs: vec![Run {
                text: text.into(),
                style: Style::default(),
            }],
        }
    }
    pub fn plain(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }
    fn push(&mut self, text: &str, style: &Style) {
        if text.is_empty() {
            return;
        }
        if let Some(last) = self.runs.last_mut().filter(|r| r.style == *style) {
            last.text.push_str(text);
        } else {
            self.runs.push(Run {
                text: text.into(),
                style: style.clone(),
            });
        }
    }
    fn newline(&mut self) {
        if self.runs.last().is_some_and(|r| !r.text.ends_with('\n')) {
            self.push("\n", &Style::default());
        }
    }
}
fn color(s: &str) -> Option<String> {
    let s = s.trim().to_ascii_lowercase();
    if let Some(h) = s.strip_prefix('#')
        && h.bytes().all(|b| b.is_ascii_hexdigit())
    {
        if h.len() == 6 {
            return Some(s);
        }
        if h.len() == 3 {
            return Some(format!(
                "#{}",
                h.chars().flat_map(|c| [c, c]).collect::<String>()
            ));
        }
    }
    let hex = match s.as_str() {
        "black" => "000000",
        "white" => "ffffff",
        "red" => "ff0000",
        "green" => "008000",
        "blue" => "0000ff",
        "yellow" => "ffff00",
        "gray" | "grey" => "808080",
        "silver" => "c0c0c0",
        "maroon" => "800000",
        "purple" => "800080",
        "fuchsia" => "ff00ff",
        "lime" => "00ff00",
        "olive" => "808000",
        "navy" => "000080",
        "teal" => "008080",
        "aqua" => "00ffff",
        "orange" => "ffa500",
        _ => return None,
    };
    Some(format!("#{hex}"))
}
fn size(s: &str) -> Option<f64> {
    let s = s.trim();
    let (n, factor) = if let Some(s) = s.strip_suffix("px") {
        (s, 0.75)
    } else if let Some(s) = s.strip_suffix("pt") {
        (s, 1.)
    } else if let Some(s) = s.strip_suffix("em") {
        (s, 9.)
    } else if let Some(s) = s.strip_suffix('%') {
        (s, 0.09)
    } else {
        (s, 1.)
    };
    n.parse::<f64>()
        .ok()
        .map(|n| n * factor)
        .filter(|n| n.is_finite() && (1. ..=200.).contains(n))
}
fn css(style: &mut Style, attributes: &str) -> bool {
    let mut hidden = false;
    for declaration in attributes.split(';') {
        let Some((key, value)) = declaration.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match key.trim().to_ascii_lowercase().as_str() {
            "color" => {
                if let Some(v) = color(value) {
                    style.foreground = Some(v);
                }
            }
            "background" | "background-color" => {
                if let Some(v) = color(value) {
                    style.background = Some(v);
                }
            }
            "font-size" => {
                if let Some(v) = size(value) {
                    style.size = Some(v);
                }
            }
            "font-family" => {
                style.family = Some(
                    value
                        .split(',')
                        .next()
                        .unwrap_or("Sans")
                        .trim_matches(['\'', '"'])
                        .to_owned(),
                );
            }
            "font-weight" => {
                style.bold = value == "bold" || value.parse::<u32>().is_ok_and(|n| n >= 600)
            }
            "font-style" => style.italic = matches!(value, "italic" | "oblique"),
            "text-decoration" => {
                style.underline = value.contains("underline");
                style.strike = value.contains("line-through");
            }
            "display" if value == "none" => hidden = true,
            _ => {}
        }
    }
    hidden
}
fn block(tag: &str) -> bool {
    matches!(
        tag,
        "p" | "div"
            | "section"
            | "article"
            | "blockquote"
            | "pre"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "ul"
            | "ol"
            | "li"
            | "table"
            | "tr"
    )
}

pub fn parse(input: &str) -> Result<Document, String> {
    check_text(input)?;
    let mut doc = Document::default();
    let mut stack: Vec<(String, Style, bool, bool)> = Vec::new();
    let mut skip = 0usize;
    let mut count = 0usize;
    for token in Tokenizer::new(input).flatten() {
        match token {
            Token::StartTag(tag) => {
                let name = String::from_utf8_lossy(&tag.name).to_string();
                let parent = stack.last();
                let mut style = parent.map(|p| p.1.clone()).unwrap_or_default();
                let preserve = parent.is_some_and(|p| p.3) || name == "pre";
                let mut hidden = skip > 0
                    || matches!(
                        name.as_str(),
                        "script" | "style" | "head" | "title" | "template"
                    );
                match name.as_str() {
                    "b" | "strong" => style.bold = true,
                    "i" | "em" => style.italic = true,
                    "u" => style.underline = true,
                    "s" | "strike" | "del" => style.strike = true,
                    "code" | "pre" | "tt" => style.family = Some("Monospace".into()),
                    "small" => style.size = Some(style.size.unwrap_or(9.) * 0.8),
                    "big" => style.size = Some(style.size.unwrap_or(9.) * 1.2),
                    "h1" => {
                        style.bold = true;
                        style.size = Some(18.);
                    }
                    "h2" => {
                        style.bold = true;
                        style.size = Some(14.);
                    }
                    "h3" => {
                        style.bold = true;
                        style.size = Some(12.);
                    }
                    _ => {}
                }
                for (key, value) in &tag.attributes {
                    let value = String::from_utf8_lossy(value);
                    match key.as_slice() {
                        b"style" => hidden |= css(&mut style, &value),
                        b"color" => {
                            if let Some(v) = color(&value) {
                                style.foreground = Some(v);
                            }
                        }
                        b"bgcolor" => {
                            if let Some(v) = color(&value) {
                                style.background = Some(v);
                            }
                        }
                        b"face" => style.family = Some(value.into()),
                        b"size" if name == "font" => {
                            if let Ok(n) = value.parse::<usize>() {
                                style.size =
                                    Some([7., 8., 9., 10., 12., 16., 24.][n.clamp(1, 7) - 1]);
                            }
                        }
                        _ => {}
                    }
                }
                if !hidden {
                    if block(&name) {
                        doc.newline();
                    }
                    if name == "br" {
                        doc.push("\n", &style);
                    }
                    if name == "li" {
                        doc.push("• ", &style);
                    }
                    if matches!(name.as_str(), "td" | "th")
                        && doc.runs.last().is_some_and(|r| !r.text.ends_with('\n'))
                    {
                        doc.push("\t", &style);
                    }
                    if name == "img"
                        && let Some(alt) = tag.attributes.get(b"alt".as_slice())
                    {
                        doc.push(&String::from_utf8_lossy(alt), &style);
                    }
                }
                if !tag.self_closing
                    && !matches!(
                        name.as_str(),
                        "br" | "img" | "hr" | "meta" | "link" | "input" | "wbr"
                    )
                {
                    if stack.len() >= 128 {
                        return Err(t("Pin text is too long"));
                    }
                    if hidden {
                        skip += 1;
                    }
                    stack.push((name, style, hidden, preserve));
                }
            }
            Token::EndTag(tag) => {
                let name = String::from_utf8_lossy(&tag.name);
                if let Some(index) = stack.iter().rposition(|p| p.0 == name) {
                    let hidden = stack[index].2;
                    for (_, _, hidden, _) in stack.drain(index..) {
                        if hidden {
                            skip = skip.saturating_sub(1);
                        }
                    }
                    if !hidden && block(&name) {
                        doc.newline();
                    }
                }
            }
            Token::String(bytes) if skip == 0 => {
                let text = String::from_utf8_lossy(&bytes);
                let style = stack.last().map(|p| p.1.clone()).unwrap_or_default();
                if stack.last().is_some_and(|p| p.3) {
                    count += text.chars().count();
                    doc.push(&text, &style);
                } else {
                    let mut normalized = String::new();
                    let mut space = doc
                        .runs
                        .last()
                        .and_then(|r| r.text.chars().last())
                        .is_none_or(char::is_whitespace);
                    for c in text.chars() {
                        if c.is_ascii_whitespace() {
                            if !space {
                                normalized.push(' ');
                                space = true;
                            }
                        } else {
                            normalized.push(c);
                            space = false;
                        }
                    }
                    count += normalized.chars().count();
                    doc.push(&normalized, &style);
                }
                if count > MAX_TEXT {
                    return Err(t("Pin text is too long"));
                }
            }
            _ => {}
        }
    }
    while doc
        .runs
        .last()
        .is_some_and(|r| r.text.trim_end().is_empty())
    {
        doc.runs.pop();
    }
    if let Some(last) = doc.runs.last_mut() {
        last.text = last.text.trim_end_matches('\n').into();
    }
    Ok(doc)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn html_is_decoded_and_inherits_styles_without_executing_content() {
        let d=parse("<p><b>A &amp; <i>B</i></b> <span style='color:red;font-size:24pt'>大字</span></p><script>bad()</script><ul><li>one</li><li>two</li></ul>").unwrap();
        assert!(d.plain().contains("A & B 大字"));
        assert!(!d.plain().contains("bad"));
        assert!(d.plain().contains("• one\n• two"));
        assert!(
            d.runs
                .iter()
                .any(|r| r.style.bold && r.style.italic && r.text == "B")
        );
        assert!(
            d.runs
                .iter()
                .any(|r| r.style.size == Some(24.)
                    && r.style.foreground.as_deref() == Some("#ff0000"))
        );
    }
    #[test]
    fn tables_and_preformatted_text_keep_their_structure() {
        let d=parse("<table><tr><td>A</td><td>B</td></tr><tr><td>C</td><td>D</td></tr></table><pre>x  y\n z</pre>").unwrap();
        assert!(d.plain().starts_with("A\tB\nC\tD"));
        assert!(d.plain().contains("x  y\n z"));
        assert!(
            !parse("<div style='display:none'>secret</div>shown")
                .unwrap()
                .plain()
                .contains("secret")
        );
    }
}

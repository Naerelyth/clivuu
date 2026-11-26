//! Observe allocated color controls under their real settings/popover hosts.
use gtk::prelude::*;
use serde_json::{Value, json};

fn descendants(widget: &gtk::Widget, out: &mut Vec<gtk::Widget>) {
    out.push(widget.clone());
    let mut child = widget.first_child();
    while let Some(w) = child {
        child = w.next_sibling();
        descendants(&w, out);
    }
}

pub fn inspect(host: &gtk::Widget) -> Option<Value> {
    let mut widgets = vec![];
    descendants(host, &mut widgets);
    let panel = widgets
        .iter()
        .find(|w| w.widget_name() == "clivuu-color-editor" && w.is_mapped())?
        .clone();
    widgets.clear();
    descendants(&panel, &mut widgets);
    let font = panel.pango_context().font_description().unwrap();
    let swatches: Vec<_> = widgets
        .iter()
        .filter(|w| w.is_mapped() && w.has_css_class("clivuu-swatch"))
        .map(|w| json!([w.width(), w.height()]))
        .collect();
    let field = |name: &str| widgets.iter().find(|w| w.widget_name() == name).cloned();
    let input = |name: &str| {
        let w = field(name)?;
        let capsule = w.parent()?;
        let grid = capsule.parent()?;
        let b = capsule.compute_bounds(&grid)?;
        Some(json!({
            "width": w.width(),
            "capsuleWidth": capsule.width(),
            "x": b.x(),
            "y": b.y(),
            "right": b.x() + b.width(),
            "gridWidth": grid.width(),
            "inheritsFont": w.pango_context().font_description().as_ref() == Some(&font),
        }))
    };
    let bounds_of = |name: &str| -> Option<(f32, f32, f32, f32, f32)> {
        let w = field(name)?;
        let capsule = w.parent()?;
        let grid = capsule.parent()?;
        let b = capsule.compute_bounds(&grid)?;
        Some((
            b.x(),
            b.y(),
            b.x() + b.width(),
            b.width(),
            grid.width() as f32,
        ))
    };
    let hue = bounds_of("clivuu-color-hue").unwrap();
    let sat = bounds_of("clivuu-color-saturation").unwrap();
    let val = bounds_of("clivuu-color-value").unwrap();
    let red = bounds_of("clivuu-color-red").unwrap();
    let green = bounds_of("clivuu-color-green").unwrap();
    let blue = bounds_of("clivuu-color-blue").unwrap();
    let html_b = bounds_of("clivuu-color-html").unwrap();
    let alpha_b = bounds_of("clivuu-color-alpha").unwrap();
    let close = |a: f32, b: f32| (a - b).abs() < 1.5;
    let channels: Vec<_> = [hue, sat, val, red, green, blue]
        .into_iter()
        .map(|b| json!(b.3))
        .collect();
    let channels_uniform = [hue, sat, val, red, green, blue]
        .into_iter()
        .all(|b| close(b.3, alpha_b.3));
    let grid_aligned = close(hue.0, 0.)
        && close(red.0, 0.)
        && close(html_b.0, 0.)
        && close(val.2, val.4)
        && close(blue.2, blue.4)
        && close(alpha_b.2, alpha_b.4)
        && close(hue.1, sat.1)
        && close(sat.1, val.1)
        && close(red.1, green.1)
        && close(green.1, blue.1)
        && close(html_b.1, alpha_b.1)
        && close(html_b.2, green.2)
        && close(alpha_b.0, blue.0);
    let steppers_hidden = [
        "hue",
        "saturation",
        "value",
        "red",
        "green",
        "blue",
        "alpha",
    ]
    .into_iter()
    .all(|name| {
        let Some(spin) = field(&format!("clivuu-color-{name}")) else {
            return false;
        };
        let mut kids = vec![];
        descendants(&spin, &mut kids);
        !kids.iter().any(|w| w.is::<gtk::Button>() && w.is_visible())
    });
    let has_alpha_slider = field("clivuu-color-alpha-slider")
        .is_some_and(|w| w.is_mapped() && w.width() >= 80 && w.height() >= 20);
    let has_preview = field("clivuu-color-preview")
        .is_some_and(|w| w.is_mapped() && w.width() >= 30 && w.height() >= 20);
    let has_copy_button = field("clivuu-color-copy").is_some_and(|w| w.is_mapped());
    Some(json!({
        "font": font.to_string(),
        "swatches": swatches,
        "alpha": input("clivuu-color-alpha"),
        "html": input("clivuu-color-html"),
        "channels": channels,
        "channelsUniform": channels_uniform,
        "gridAligned": grid_aligned,
        "steppersHidden": steppers_hidden,
        "hasAlphaSlider": has_alpha_slider,
        "hasPreview": has_preview,
        "hasCopyButton": has_copy_button,
        "width": panel.width(),
    }))
}

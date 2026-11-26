//! Shared bounded text/HTML/color rendering. All Cairo/Pango objects are worker-local.
use clivuu_core::{
    i18n::t,
    image::{Rgba, RgbaImage},
    paste::html::Document,
};
use gtk::{cairo, glib, pango};
fn markup(document: &Document) -> String {
    let mut out = String::new();
    for run in &document.runs {
        let s = &run.style;
        out.push_str("<span");
        for (name, value) in [
            ("foreground", s.foreground.as_deref()),
            ("background", s.background.as_deref()),
            ("font_family", s.family.as_deref()),
        ] {
            if let Some(v) = value {
                out.push_str(&format!(" {name}=\"{}\"", glib::markup_escape_text(v)));
            }
        }
        if let Some(size) = s.size {
            out.push_str(&format!(
                " size=\"{}\"",
                (size * pango::SCALE as f64).round() as i32
            ));
        }
        if s.bold {
            out.push_str(" weight=\"bold\"");
        }
        if s.italic {
            out.push_str(" style=\"italic\"");
        }
        if s.underline {
            out.push_str(" underline=\"single\"");
        }
        if s.strike {
            out.push_str(" strikethrough=\"true\"");
        }
        out.push('>');
        out.push_str(&glib::markup_escape_text(&run.text));
        out.push_str("</span>");
    }
    out
}
fn pixels(mut surface: cairo::ImageSurface) -> Result<RgbaImage, String> {
    let width = surface.width() as u32;
    let height = surface.height() as u32;
    let stride = surface.stride() as usize;
    let mut image = RgbaImage::new(width, height);
    let data = surface.data().map_err(|e| e.to_string())?;
    for (y, row) in data.chunks(stride).enumerate() {
        for (x, p) in row.chunks_exact(4).take(width as usize).enumerate() {
            let v = u32::from_ne_bytes(p.try_into().unwrap());
            let a = (v >> 24) as u8;
            let c = |shift: u32| -> u8 {
                if a == 0 {
                    0
                } else {
                    ((((v >> shift) & 255u32) * 255u32 + u32::from(a) / 2) / u32::from(a)) as u8
                }
            };
            image.put_pixel(x as u32, y as u32, Rgba([c(16), c(8), c(0), a]));
        }
    }
    Ok(image)
}
#[cfg(test)]
fn render(text: &str, html: bool) -> Result<RgbaImage, String> {
    clivuu_core::paste::check_text(text)?;
    let document = if html {
        clivuu_core::paste::html::parse(text)?
    } else {
        Document {
            runs: vec![clivuu_core::paste::html::Run {
                text: text.into(),
                style: clivuu_core::paste::html::Style::default(),
            }],
        }
    };
    render_document(&document)
}
#[cfg(test)]
fn render_document(document: &Document) -> Result<RgbaImage, String> {
    Ok(render_document_with_options(
        document,
        &clivuu_core::pins::preferences::Preferences::default().text,
        1.,
    )?
    .0)
}
pub fn render_document_with_options(
    document: &Document,
    options: &clivuu_core::pins::preferences::TextOptions,
    scale: f64,
) -> Result<(RgbaImage, [u32; 2]), String> {
    if document.plain().trim().is_empty() {
        return Err(t("The clipboard contains no image or text"));
    }
    if options.max_width == 0 || options.max_width > 10000 || options.margin > 100 {
        return Err(t("Invalid paste request"));
    }
    let initial =
        cairo::ImageSurface::create(cairo::Format::ARgb32, 1, 1).map_err(|e| e.to_string())?;
    if !scale.is_finite() || scale <= 0. {
        return Err(t("Invalid paste request"));
    }
    initial.set_device_scale(scale, scale);
    let context = cairo::Context::new(&initial).map_err(|e| e.to_string())?;
    let layout = pangocairo::functions::create_layout(&context);
    layout.set_font_description(Some(&pango::FontDescription::from_string(&options.font)));
    layout.set_markup(&markup(document));
    layout.set_wrap(pango::WrapMode::WordChar);
    let margin = options.margin.min((options.max_width - 1) / 2) as i32;
    let max_width = options.max_width as i32 - 2 * margin;
    if layout.pixel_size().0 > max_width {
        layout.set_width(max_width * pango::SCALE);
    }
    let (width, height) = layout.pixel_size();
    if height > 4096 {
        return Err(t("Rendered text exceeds 4096 pixels in height"));
    }
    drop(context);
    let logical = [
        (width.max(1) + 2 * margin) as u32,
        (height.max(1) + 2 * margin) as u32,
    ];
    let [w, h] = logical.map(|n| (n as f64 * scale).ceil());
    if w * h > clivuu_core::image_ops::MAX_PIXELS as f64
        || w > i32::MAX as f64
        || h > i32::MAX as f64
    {
        return Err(t(
            "The image exceeds 48 megapixels or has invalid dimensions",
        ));
    }
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, w as i32, h as i32)
        .map_err(|e| e.to_string())?;
    surface.set_device_scale(scale, scale);
    let context = cairo::Context::new(&surface).map_err(|e| e.to_string())?;
    let bg = gtk::gdk::RGBA::parse(&options.background).map_err(|e| e.to_string())?;
    context.set_source_rgba(
        bg.red() as f64,
        bg.green() as f64,
        bg.blue() as f64,
        bg.alpha() as f64,
    );
    context.paint().map_err(|e| e.to_string())?;
    let fg = gtk::gdk::RGBA::parse(&options.foreground).map_err(|e| e.to_string())?;
    context.set_source_rgba(
        fg.red() as f64,
        fg.green() as f64,
        fg.blue() as f64,
        fg.alpha() as f64,
    );
    context.move_to(margin as f64, margin as f64);
    pangocairo::functions::update_layout(&context, &layout);
    pangocairo::functions::show_layout(&context, &layout);
    drop(context);
    drop(layout);
    Ok((pixels(surface)?, logical))
}
pub const COLOR_CARD_SIZE: [u32; 2] = [158, 140];

pub fn color_card([r, g, b, a]: [u8; 4], scale: f64) -> Result<RgbaImage, String> {
    if !scale.is_finite() || scale <= 0. {
        return Err(t("Invalid paste request"));
    }
    let [width, height] = COLOR_CARD_SIZE.map(|n| (n as f64 * scale).ceil());
    if width > i32::MAX as f64
        || height > i32::MAX as f64
        || width * height > clivuu_core::image_ops::MAX_PIXELS as f64
    {
        return Err(t(
            "The image exceeds 48 megapixels or has invalid dimensions",
        ));
    }
    let [rf, gf, bf] = [r, g, b].map(|v| v as f64 / 255.);
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let delta = max - min;
    let hue = if delta == 0. {
        0.
    } else if max == rf {
        60. * ((gf - bf) / delta).rem_euclid(6.)
    } else if max == gf {
        60. * ((bf - rf) / delta + 2.)
    } else {
        60. * ((rf - gf) / delta + 4.)
    };
    let light = (max + min) / 2.;
    let sat_hsl = if delta == 0. {
        0.
    } else {
        delta / (1. - (2. * light - 1.).abs())
    };
    let intensity = (rf + gf + bf) / 3.;
    let sat_hsi = if intensity == 0. {
        0.
    } else {
        1. - min / intensity
    };
    let text = format!(
        "RGB: {r:3}, {g:3}, {b:3}\nRGB: {rf:.2},{gf:.2},{bf:.2}\nHSV: {:3}, {:3}, {:3}\nHSL: {:3}, {:3}, {:3}\nHSI: {:3}, {:3}, {:3}\nHEX:      #{r:02X}{g:02X}{b:02X}",
        hue.round(),
        if max == 0. { 0. } else { delta / max * 100. }.round(),
        (max * 100.).round(),
        hue.round(),
        (sat_hsl * 100.).round(),
        (light * 100.).round(),
        hue.round(),
        (sat_hsi * 100.).round(),
        (intensity * 100.).round()
    );
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, width as i32, height as i32)
        .map_err(|e| e.to_string())?;
    // Rasterize glyphs at the destination density; scaling a finished card loses
    // detail. Keep all layout coordinates in the card's logical units.
    surface.set_device_scale(scale, scale);
    let c = cairo::Context::new(&surface).map_err(|e| e.to_string())?;
    c.set_source_rgb(1., 1., 1.);
    c.paint().map_err(|e| e.to_string())?;
    if a < 255 {
        for y in 0..2 {
            for x in 0..16 {
                let v = if (x + y) % 2 == 0 { 0.7 } else { 0.9 };
                c.set_source_rgb(v, v, v);
                c.rectangle(x as f64 * 10., y as f64 * 10., 10., 10.);
                c.fill().map_err(|e| e.to_string())?;
            }
        }
    }
    c.set_source_rgba(rf, gf, bf, a as f64 / 255.);
    c.rectangle(0., 0., 158., 20.);
    c.fill().map_err(|e| e.to_string())?;
    let layout = pangocairo::functions::create_layout(&c);
    let mut font = pango::FontDescription::new();
    font.set_family("Consolas,Monospace");
    font.set_absolute_size(13. * pango::SCALE as f64);
    layout.set_font_description(Some(&font));
    c.set_source_rgb(0., 0., 0.);
    for (row, text) in text.lines().enumerate() {
        layout.set_text(text);
        let (_, height) = layout.pixel_size();
        c.move_to(6., 20. + row as f64 * 20. + (20. - height as f64) / 2.);
        pangocairo::functions::show_layout(&c, &layout);
    }
    drop(layout);
    drop(c);
    pixels(surface)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn text_preferences_control_font_margin_colors_and_device_density() {
        let document = Document::plain_text("AaOo12");
        let mut options = clivuu_core::pins::preferences::Preferences::default().text;
        let (one, size) = render_document_with_options(&document, &options, 1.).unwrap();
        let (two, size2) = render_document_with_options(&document, &options, 2.).unwrap();
        assert_eq!(size, size2);
        assert_eq!(two.dimensions(), (size[0] * 2, size[1] * 2));
        options.margin = 30;
        options.background = "#123456".into();
        options.foreground = "#abcdef".into();
        let (margin, size3) = render_document_with_options(&document, &options, 1.).unwrap();
        assert_eq!(size3, [size[0] + 20, size[1] + 20]);
        assert_eq!(margin.get_pixel(0, 0).0, [0x12, 0x34, 0x56, 255]);
        options.font = "Sans 24".into();
        let (_, large) = render_document_with_options(&document, &options, 1.).unwrap();
        assert!(large[1] > size3[1]);
        options.max_width = 100;
        let (_, wrapped) = render_document_with_options(
            &Document::plain_text("many words ".repeat(10).as_str()),
            &options,
            1.,
        )
        .unwrap();
        assert!(wrapped[0] <= 100);
        assert!(one.width() > 40);
    }
    #[test]
    fn natural_text_width_and_html_styles_survive_rendering() {
        let short = render("Hello", false).unwrap();
        assert!(short.width() < 150 && short.height() < 100);
        let rich = render(
            "<b>A</b><span style='color:red;font-size:24pt'>B &amp; C</span>",
            true,
        )
        .unwrap();
        assert!(rich.height() > short.height());
        assert!(
            rich.pixels()
                .any(|p| u16::from(p[0]) > u16::from(p[1]) + 30 && p[1] < 100)
        );
        assert!(render(" ", false).is_err());
    }
    #[test]
    fn color_cards_have_reference_size_and_exact_swatch() {
        let card = color_card([17, 99, 201, 255], 1.).unwrap();
        assert_eq!(card.dimensions(), (158, 140));
        assert_eq!(card.get_pixel(40, 5).0, [17, 99, 201, 255]);
    }

    #[test]
    fn color_card_rasterizes_at_device_density_instead_of_magnifying_glyphs() {
        let color = [237, 28, 36, 255];
        let normal = color_card(color, 1.).unwrap();
        for scale in [1.25, 1.5, 2.] {
            let card = color_card(color, scale).unwrap();
            let expected = COLOR_CARD_SIZE.map(|n| (n as f64 * scale).ceil() as u32);
            assert_eq!([card.width(), card.height()], expected);
            assert_eq!(card.get_pixel(40, 5).0, color);
            let magnified = clivuu_core::image::imageops::resize(
                &normal,
                card.width(),
                card.height(),
                clivuu_core::image::imageops::FilterType::Triangle,
            );
            assert_ne!(card, magnified);
            // All six rows remain present, including HEX at the bottom.
            for row in 0..6 {
                let top = ((20 + row * 20) as f64 * scale).ceil() as u32;
                let bottom = ((40 + row * 20) as f64 * scale).floor() as u32;
                assert!((top..bottom).any(|y| (0..card.width()).any(|x| card[(x, y)][0] < 80)));
            }
        }
        for scale in [0., -1., f64::NAN, f64::INFINITY, 100_000.] {
            assert!(color_card(color, scale).is_err());
        }
    }
}

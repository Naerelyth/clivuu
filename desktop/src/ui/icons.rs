//! Port of Clivuu's existing vector artwork. It inherits CSS ink, so both
//! appearances and both editor entry points use the same strokes.
use gtk::{cairo, prelude::*};
fn line(c: &cairo::Context, p: &[f64]) {
    c.move_to(p[0], p[1]);
    for p in p[2..].chunks_exact(2) {
        c.line_to(p[0], p[1]);
    }
}
fn rounded(c: &cairo::Context, x: f64, y: f64, width: f64, height: f64, radius: f64) {
    let pi = std::f64::consts::PI;
    c.new_sub_path();
    c.arc(x + width - radius, y + radius, radius, -pi / 2., 0.);
    c.arc(x + width - radius, y + height - radius, radius, 0., pi / 2.);
    c.arc(x + radius, y + height - radius, radius, pi / 2., pi);
    c.arc(x + radius, y + radius, radius, pi, pi * 1.5);
    c.close_path();
}
pub fn icon(kind: &'static str) -> gtk::DrawingArea {
    icon_sized(kind, 18)
}
fn widget_scale(area: &gtk::DrawingArea) -> f64 {
    area.native()
        .and_then(|native| native.surface())
        .map(|surface| surface.scale())
        .filter(|scale| scale.is_finite() && *scale >= 1.0)
        .unwrap_or_else(|| f64::from(area.scale_factor()).max(1.0))
}
pub fn icon_sized(kind: &'static str, size: i32) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::builder()
        .content_width(size)
        .content_height(size)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .build();
    area.connect_scale_factor_notify(|a| a.queue_draw());
    area.set_draw_func(move |a, c, w, h| {
        let ink = a.color();
        c.set_source_rgba(
            ink.red() as f64,
            ink.green() as f64,
            ink.blue() as f64,
            ink.alpha() as f64,
        );
        let _ = draw_scaled(c, w as f64, h as f64, widget_scale(a), kind);
    });
    area
}
pub fn draw(c: &cairo::Context, w: f64, h: f64, kind: &str) -> Result<(), cairo::Error> {
    let (sx, sy) = c.target().device_scale();
    draw_scaled(c, w, h, sx.max(sy).max(1.0), kind)
}
pub fn draw_scaled(
    c: &cairo::Context,
    w: f64,
    h: f64,
    scale: f64,
    kind: &str,
) -> Result<(), cairo::Error> {
    if matches!(kind, "fill-pattern" | "fill-ellipse") {
        // Replicate Snipaste's `pb_rectFill` / `pb_ellipseFill`: 4:3 shape sized to
        // `S = 16`, height `(round(S * 0.75) & ~1) = 12`, filled with Qt's physical
        // device-pixel `Dense4Pattern` checkerboard (`(px + py) % 2 == 0`).
        let dpr = if scale.is_finite() && scale >= 1.0 {
            scale
        } else {
            1.0
        };
        let side = (w.min(h) * (16. / 18.)).round().max(2.);
        let rect_h = (((side * 0.75).round() as i32) & !1).max(2) as f64;
        let rw = (side * dpr).round().max(2.);
        let rh = (rect_h * dpr).round().max(2.);
        let ox = ((w * dpr - rw) * 0.5).floor();
        let oy = ((h * dpr - rh) * 0.5).floor();

        let stride = cairo::Format::A8.stride_for_width(2)? as usize;
        let mut mask_bits = vec![0u8; stride * 2];
        mask_bits[0] = 0xFF;
        mask_bits[1] = 0x00;
        mask_bits[stride] = 0x00;
        mask_bits[stride + 1] = 0xFF;
        let tile = cairo::ImageSurface::create_for_data(
            mask_bits,
            cairo::Format::A8,
            2,
            2,
            stride as i32,
        )?;
        let pattern = cairo::SurfacePattern::create(&tile);
        pattern.set_extend(cairo::Extend::Repeat);
        pattern.set_filter(cairo::Filter::Nearest);

        c.save()?;
        c.scale(1. / dpr, 1. / dpr);
        if kind == "fill-ellipse" {
            c.set_antialias(cairo::Antialias::Best);
            c.save()?;
            c.translate(ox + rw * 0.5, oy + rh * 0.5);
            c.scale(rw * 0.5, rh * 0.5);
            c.arc(0., 0., 1., 0., std::f64::consts::TAU);
            c.restore()?;
            c.clip();
        } else {
            c.set_antialias(cairo::Antialias::None);
            c.rectangle(ox, oy, rw, rh);
            c.clip();
        }
        c.mask(&pattern)?;
        return c.restore();
    }
    c.save()?;
    c.scale(w / 24., h / 24.);
    c.set_line_width(1.6);
    c.set_line_cap(cairo::LineCap::Round);
    c.set_line_join(cairo::LineJoin::Round);
    match kind {
        "scroll-vertical" => {
            line(c, &[12., 3., 12., 21.]);
            line(c, &[8., 7., 12., 3., 16., 7.]);
            line(c, &[8., 17., 12., 21., 16., 17.]);
        }
        "scroll-horizontal" => {
            line(c, &[3., 12., 21., 12.]);
            line(c, &[7., 8., 3., 12., 7., 16.]);
            line(c, &[17., 8., 21., 12., 17., 16.]);
        }

        "brush" | "highlight-free" => {
            c.set_line_width(1.9);
            c.move_to(4.2, 18.4);
            c.curve_to(6.2, 13.0, 9.6, 5.2, 11.6, 5.6);
            c.curve_to(12.8, 5.9, 8.0, 17.5, 9.2, 18.2);
            c.curve_to(10.5, 18.8, 16.2, 9.0, 17.8, 9.6);
            c.curve_to(19.0, 10.1, 16.0, 18.0, 17.2, 18.6);
            c.curve_to(18.0, 19.0, 19.3, 18.6, 20.2, 18.2);
        }
        "replace" => line(c, &[4., 12., 10., 18., 21., 5.]),
        "scissors" => {
            c.arc(6., 6., 3., 0., std::f64::consts::TAU);
            c.stroke()?;
            c.arc(6., 18., 3., 0., std::f64::consts::TAU);
            c.stroke()?;
            line(c, &[8.2, 8.2, 21., 21.]);
            line(c, &[8.2, 15.8, 21., 3.]);
        }
        "trim-top" | "trim-bottom" | "trim-left" | "trim-right" => {
            // Scissors face the retained open bracket; rotate the complete
            // pictogram so the handles identify the side being discarded.
            c.translate(12., 12.);
            c.rotate(match kind {
                "trim-top" => std::f64::consts::FRAC_PI_2,
                "trim-bottom" => -std::f64::consts::FRAC_PI_2,
                "trim-right" => std::f64::consts::PI,
                _ => 0.,
            });
            c.translate(-12., -12.);
            line(c, &[14., 3., 21., 3., 21., 21., 14., 21.]);
            c.stroke()?;
            for y in [7., 17.] {
                c.arc(5., y, 2.2, 0., std::f64::consts::TAU);
                c.stroke()?;
            }
            line(c, &[6.6, 8.6, 16., 18.]);
            line(c, &[6.6, 15.4, 16., 6.]);
        }
        "crop" => {
            line(c, &[3., 6., 18., 6., 18., 21.]);
            line(c, &[6., 3., 6., 18., 21., 18.]);
        }
        "rotate-right" => {
            c.arc(12., 12., 7., -2.7, 2.7);
            line(c, &[7., 3., 7., 9., 2., 7.]);
        }
        "rotate-left" => {
            c.arc(12., 12., 7., -0.4, 4.9);
            line(c, &[17., 3., 17., 9., 22., 7.]);
        }
        "flip-horizontal" => {
            line(c, &[10., 3., 10., 21.]);
            line(c, &[7., 6., 3., 18., 7., 18., 7., 6.]);
            line(c, &[14., 6., 21., 18., 14., 18., 14., 6.]);
        }
        "flip-vertical" => {
            line(c, &[3., 11., 21., 11.]);
            line(c, &[6., 7., 18., 7., 12., 3., 6., 7.]);
            line(c, &[6., 15., 18., 15., 12., 21., 6., 15.]);
        }
        "select" => line(
            c,
            &[
                5., 3., 5., 19., 9., 15., 12., 21., 15., 19., 12., 13., 18., 13., 5., 3.,
            ],
        ),
        "rect" => c.rectangle(4., 6., 16., 12.),
        "shape" => {
            c.rectangle(3., 4., 12., 10.);
            c.stroke()?;
            c.save()?;
            c.translate(15.5, 15.5);
            c.scale(6., 4.5);
            c.arc(0., 0., 1., 0., std::f64::consts::TAU);
            c.restore()?;
        }
        "ellipse" => {
            c.save()?;
            c.translate(12., 12.);
            c.scale(8., 6.);
            c.arc(0., 0., 1., 0., std::f64::consts::TAU);
            c.restore()?;
        }
        "arrow" => {
            line(c, &[5., 19., 19., 5., 11., 5.]);
            line(c, &[19., 5., 19., 13.]);
        }
        "pen" => {
            line(c, &[4., 20., 6., 13., 16., 3., 21., 8., 11., 18., 4., 20.]);
            line(c, &[6., 13., 11., 18.]);
        }
        "highlight" => {
            line(c, &[4., 17., 8., 12., 12., 16., 7., 20.]);
            line(c, &[8., 12., 16., 4., 20., 8., 12., 16.]);
            line(c, &[3., 21., 13., 21.]);
        }
        "highlight-line" => {
            c.set_line_width(6.);
            c.set_line_cap(cairo::LineCap::Butt);
            line(c, &[4., 15., 20., 9.]);
            c.stroke()?;
            c.restore()?;
            return Ok(());
        }
        "mosaic" => {
            c.rectangle(4., 4., 16., 16.);
            c.stroke()?;
            c.rectangle(4., 4., 8., 8.);
            c.rectangle(12., 12., 8., 8.);
            c.fill()?;
            line(c, &[12., 4., 12., 20.]);
            line(c, &[4., 12., 20., 12.]);
        }
        "sequence" => {
            c.arc(12., 12., 9., 0., std::f64::consts::TAU);
            c.stroke()?;
            line(c, &[9., 9., 12., 7., 12., 17.]);
            line(c, &[9., 17., 15., 17.]);
        }
        "text" => {
            line(c, &[5., 5., 19., 5.]);
            line(c, &[12., 5., 12., 20.]);
            line(c, &[8., 20., 16., 20.]);
        }
        "text-bold" => {
            c.set_line_width(2.7);
            line(c, &[6., 4., 6., 20.]);
            c.move_to(6., 4.);
            c.line_to(12., 4.);
            c.curve_to(20., 4., 20., 11., 12., 11.);
            c.line_to(6., 11.);
            c.move_to(12., 11.);
            c.curve_to(22., 11., 22., 20., 12., 20.);
            c.line_to(6., 20.);
        }
        "text-italic" => {
            c.set_line_width(2.);
            line(c, &[10., 4., 20., 4.]);
            line(c, &[15., 4., 9., 20.]);
            line(c, &[4., 20., 14., 20.]);
        }
        "text-outline" => {
            c.set_line_width(1.2);
            line(
                c,
                &[
                    3., 20., 9.5, 4., 14.5, 4., 21., 20., 16., 20., 14.5, 16., 9.5, 16., 8., 20.,
                    3., 20.,
                ],
            );
            line(c, &[11., 12., 12., 8.5, 13., 12., 11., 12.]);
        }
        "eraser" => {
            line(
                c,
                &[3., 14., 13., 4., 21., 12., 12., 21., 10., 21., 3., 14.],
            );
            line(c, &[8., 9., 16., 17.]);
            line(c, &[10., 21., 21., 21.]);
        }
        "undo" => {
            line(c, &[9., 5., 4., 10., 9., 15.]);
            c.move_to(4., 10.);
            c.curve_to(22., 4., 24., 23., 10., 20.);
        }
        "redo" => {
            line(c, &[15., 5., 20., 10., 15., 15.]);
            c.move_to(20., 10.);
            c.curve_to(2., 4., 0., 23., 14., 20.);
        }
        "clipboard" => {
            c.rectangle(4., 7., 12., 14.);
            line(c, &[9., 7., 9., 3., 21., 3., 21., 17., 16., 17.]);
        }
        "pin" => {
            line(
                c,
                &[
                    7., 3., 17., 3., 15.5, 10., 19., 14., 5., 14., 8.5, 10., 7., 3.,
                ],
            );
            line(c, &[12., 14., 12., 21.]);
        }
        "quick-save" => {
            line(c, &[4., 3., 18., 3., 21., 6., 21., 21., 3., 21., 3., 3.]);
            c.rectangle(7., 3., 9., 6.);
            c.rectangle(7., 14., 10., 7.);
        }
        "download" => {
            line(c, &[12., 3., 12., 16.]);
            line(c, &[7., 11., 12., 16., 17., 11.]);
            line(c, &[4., 16., 4., 21., 20., 21., 20., 16.]);
        }
        "ocr_clipboard" => {
            line(c, &[3., 8., 3., 3., 8., 3.]);
            line(c, &[16., 3., 21., 3., 21., 8.]);
            line(c, &[3., 16., 3., 21., 8., 21.]);
            line(c, &[16., 21., 21., 21., 21., 16.]);
            line(c, &[7., 16., 12., 7., 17., 16.]);
            line(c, &[9., 13., 15., 13.]);
        }
        "long-capture" => {
            // Capture corners and repeated downward chevrons indicate a
            // scrolling capture, distinct from cropping and automatic motion.
            line(c, &[8., 3., 4., 3., 4., 8.]);
            line(c, &[16., 3., 20., 3., 20., 8.]);
            line(c, &[4., 16., 4., 21., 8., 21.]);
            line(c, &[20., 16., 20., 21., 16., 21.]);
            line(c, &[8.5, 8., 12., 11.5, 15.5, 8.]);
            line(c, &[8.5, 13., 12., 16.5, 15.5, 13.]);
        }
        "scroll" => {
            c.rectangle(6., 3., 12., 18.);
            line(c, &[9., 12., 12., 16., 15., 12.]);
            line(c, &[12., 7., 12., 16.]);
        }
        "record-start" => {
            c.set_line_width(1.5);
            c.arc(12., 12., 10., 0., std::f64::consts::TAU);
            c.stroke()?;
            c.arc(12., 12., 3.3, 0., std::f64::consts::TAU);
            c.fill()?;
        }
        "record-speaker" => {
            line(c, &[3., 9., 7., 9., 12., 5., 12., 19., 7., 15., 3., 15.]);
            c.close_path();
            c.stroke()?;
            c.arc(12., 12., 5., -0.8, 0.8);
            c.stroke()?;
            c.arc(12., 12., 9., -0.8, 0.8);
            c.stroke()?;
        }
        "record-speaker-muted" => {
            line(c, &[3., 9., 7., 9., 12., 5., 12., 19., 7., 15., 3., 15.]);
            c.close_path();
            c.stroke()?;
            line(c, &[16., 9., 21., 15.]);
            line(c, &[21., 9., 16., 15.]);
            c.stroke()?;
        }
        "record-microphone" => {
            rounded(c, 9., 2.5, 6., 12., 3.);
            c.stroke()?;
            c.arc(12., 12., 6., 0., std::f64::consts::PI);
            c.stroke()?;
            line(c, &[6., 12., 6., 10.]);
            line(c, &[18., 12., 18., 10.]);
            line(c, &[12., 18., 12., 21.]);
            line(c, &[8., 21., 16., 21.]);
            c.stroke()?;
        }
        "record-microphone-muted" => {
            rounded(c, 9., 2.5, 6., 12., 3.);
            c.stroke()?;
            c.arc(12., 12., 6., 0., std::f64::consts::PI);
            c.stroke()?;
            line(c, &[12., 18., 12., 21.]);
            line(c, &[8., 21., 16., 21.]);
            line(c, &[4., 4., 20., 20.]);
            c.stroke()?;
        }
        "record-cursor" => {
            line(
                c,
                &[
                    5., 3., 5., 20., 9.5, 15., 14., 22., 17., 20., 12.5, 13., 20., 12.,
                ],
            );
            c.close_path();
            c.stroke()?;
        }
        "record-stop" => {
            rounded(c, 6., 6., 12., 12., 1.3);
            c.fill()?;
        }
        "pause" => {
            c.set_line_width(2.);
            rounded(c, 2.5, 2.5, 19., 19., 1.7);
            c.stroke()?;
            line(c, &[9., 7.5, 9., 16.5]);
            line(c, &[15., 7.5, 15., 16.5]);
            c.stroke()?;
        }
        "play" => {
            c.set_line_width(1.5);
            c.arc(12., 12., 10., 0., std::f64::consts::TAU);
            c.stroke()?;
            line(c, &[10., 7.5, 16.5, 12., 10., 16.5]);
            c.close_path();
            c.fill()?;
        }
        "move-region" => {
            line(c, &[12., 2.5, 12., 21.5]);
            line(c, &[2.5, 12., 21.5, 12.]);
            line(c, &[8.5, 6., 12., 2.5, 15.5, 6.]);
            line(c, &[8.5, 18., 12., 21.5, 15.5, 18.]);
            line(c, &[6., 8.5, 2.5, 12., 6., 15.5]);
            line(c, &[18., 8.5, 21.5, 12., 18., 15.5]);
        }
        "restart-recording" => {
            c.set_line_width(2.);
            c.arc(12., 12., 9., -2.4, 2.4);
            c.stroke()?;
            line(c, &[2.5, 3., 2.5, 9., 8.5, 9.]);
            c.stroke()?;
            c.arc(12., 12., 3.2, 0., std::f64::consts::TAU);
            c.fill()?;
        }
        "record" => {
            c.rectangle(3., 6., 12., 12.);
            line(c, &[15., 10., 21., 6., 21., 18., 15., 14.]);
        }
        "grip" => {
            for x in [9.5, 14.5] {
                for y in [6., 12., 18.] {
                    c.arc(x, y, 1.35, 0., std::f64::consts::TAU);
                    c.fill()?;
                }
            }
        }
        "warning" => {
            c.set_line_width(1.7);
            c.move_to(12., 3.5);
            c.line_to(21., 19.5);
            c.line_to(3., 19.5);
            c.close_path();
            c.stroke()?;
            line(c, &[12., 9.5, 12., 14.]);
            c.stroke()?;
            c.arc(12., 16.8, 0.9, 0., std::f64::consts::TAU);
            c.fill()?;
        }
        "close" => {
            line(c, &[6., 6., 18., 18.]);
            line(c, &[6., 18., 18., 6.]);
        }
        "preferences" => {
            let teeth = 6;
            let step = std::f64::consts::TAU / teeth as f64;
            let r_in = 6.8;
            let r_out = 9.2;
            let t_base = 0.28;
            let t_tip = 0.19;
            c.new_sub_path();
            for i in 0..teeth {
                let a = -std::f64::consts::FRAC_PI_2 + i as f64 * step;
                c.arc(12., 12., r_out, a - t_tip, a + t_tip);
                c.arc(12., 12., r_in, a + t_base, a + step - t_base);
            }
            c.close_path();
            c.new_sub_path();
            c.arc(12., 12., 2.6, 0., std::f64::consts::TAU);
        }
        "chevron-down" => {
            c.set_line_width(2.2);
            line(c, &[5.5, 9., 12., 15., 18.5, 9.]);
        }
        unknown => {
            debug_assert!(false, "unknown_clivuu_icon:{unknown}");
            line(c, &[6., 6., 18., 18.]);
            line(c, &[6., 18., 18., 6.]);
        }
    }
    c.stroke()?;
    c.restore()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_icon_scaled(kind: &str, scale: u32) -> Vec<u8> {
        let px = 18 * scale as i32;
        let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, px, px).unwrap();
        surface.set_device_scale(f64::from(scale), f64::from(scale));
        {
            let ctx = cairo::Context::new(&surface).unwrap();
            ctx.set_source_rgb(1., 1., 1.);
            draw(&ctx, 18., 18., kind).unwrap();
        }
        surface.flush();
        surface.data().unwrap().to_vec()
    }

    fn render_icon(kind: &str) -> Vec<u8> {
        render_icon_scaled(kind, 1)
    }

    #[test]
    fn freehand_brush_and_highlight_share_identical_stroke_pixels() {
        let brush = render_icon("brush");
        let free = render_icon("highlight-free");
        assert_eq!(brush, free);
        assert!(brush.iter().any(|&b| b != 0));
    }

    #[test]
    fn preferences_icon_renders_gear_distinct_from_close_fallback() {
        let prefs = render_icon("preferences");
        let close = render_icon("close");
        assert!(prefs.iter().any(|&b| b != 0));
        assert_ne!(prefs, close);
    }

    #[test]
    fn stipple_fill_icons_match_crisp_landscape_lattice_and_ellipse_contour() {
        for scale in [1u32, 2u32] {
            let rect = render_icon_scaled("fill-pattern", scale);
            let ellipse = render_icon_scaled("fill-ellipse", scale);
            assert_ne!(rect, ellipse);
            // Every pixel of the 4:3 physical stipple lands on exact integer device pixels (alpha 0 or 255).
            for px in rect.chunks_exact(4) {
                assert!(px[3] == 0 || px[3] == 255);
            }
            let rw = (16 * scale) as usize;
            let rh = (12 * scale) as usize;
            let lit_rect = rect.chunks_exact(4).filter(|p| p[3] == 255).count();
            assert_eq!(lit_rect, rw * rh / 2);
            // Elliptical stipple clips corners while preserving dense interior physical lattice pixels.
            let lit_ellipse = ellipse.chunks_exact(4).filter(|p| p[3] > 0).count();
            assert!(lit_ellipse > lit_rect / 2 && lit_ellipse < lit_rect);
            // Top-left pixel of the centered 16x12 box (`ox = 1*scale`, `oy = 3*scale`) is lit in rect
            // (`(px + py) % 2 == 0`) and clipped outside the ellipse.
            let stride = (18 * scale) as usize;
            let corner_idx = ((3 * scale as usize) * stride + scale as usize) * 4;
            assert_eq!(rect[corner_idx + 3], 255);
            assert_eq!(ellipse[corner_idx + 3], 0);
        }
    }
}

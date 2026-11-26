//! Toolkit-independent draft for the shared color panel. Only acceptance emits
//! a StylePatch, so dragging channels never adds editor history entries.
#[derive(Clone, Debug, PartialEq)]
pub struct ColorDraft {
    pub rgb: [u8; 3],
    pub hsv: [f64; 3],
    pub alpha: u8,
}
impl ColorDraft {
    pub fn parse(text: &str, alpha: u8) -> Option<Self> {
        let raw = text.strip_prefix('#')?;
        if !matches!(raw.len(), 6 | 8) || !raw.bytes().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let read = |i| u8::from_str_radix(&raw[i..i + 2], 16).ok();
        let rgb = [read(0)?, read(2)?, read(4)?];
        Some(Self {
            rgb,
            hsv: rgb_to_hsv(rgb, 0.),
            alpha: if raw.len() == 8 { read(6)? } else { alpha },
        })
    }
    pub fn color(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.rgb[0], self.rgb[1], self.rgb[2])
    }
    pub fn set_rgb(&mut self, index: usize, value: f64) {
        self.rgb[index] = value.round().clamp(0., 255.) as u8;
        self.hsv = rgb_to_hsv(self.rgb, self.hsv[0]);
    }
    pub fn set_hsv(&mut self, index: usize, value: f64) {
        self.hsv[index] = value.clamp(0., if index == 0 { 359. } else { 255. });
        self.rgb = hsv_to_rgb(self.hsv);
    }
}
pub fn hsv_to_rgb([h, s, v]: [f64; 3]) -> [u8; 3] {
    let h = h.rem_euclid(360.) / 60.;
    let s = s.clamp(0., 255.) / 255.;
    let v = v.clamp(0., 255.);
    let c = v * s;
    let x = c * (1. - (h % 2. - 1.).abs());
    let m = v - c;
    let parts = [
        [c, x, 0.],
        [x, c, 0.],
        [0., c, x],
        [0., x, c],
        [x, 0., c],
        [c, 0., x],
    ][h.floor() as usize];
    parts.map(|n| (n + m).round().clamp(0., 255.) as u8)
}
pub fn rgb_to_hsv(rgb: [u8; 3], hue: f64) -> [f64; 3] {
    let [r, g, b] = rgb.map(f64::from);
    let v = r.max(g).max(b);
    let d = v - r.min(g).min(b);
    let h = if d == 0. {
        hue
    } else {
        60. * if v == r {
            (g - b) / d + if g < b { 6. } else { 0. }
        } else if v == g {
            (b - r) / d + 2.
        } else {
            (r - g) / d + 4.
        }
    };
    [h, if v == 0. { 0. } else { 255. * d / v }, v]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn byte_colors_roundtrip_through_hsv_and_achromatic_keeps_previous_hue() {
        for r in (0..=255).step_by(17) {
            for g in (0..=255).step_by(17) {
                for b in (0..=255).step_by(17) {
                    let rgb = [r, g, b];
                    assert_eq!(hsv_to_rgb(rgb_to_hsv(rgb, 0.)), rgb);
                }
            }
        }
        assert_eq!(rgb_to_hsv([90, 90, 90], 237.)[0], 237.);
    }
    #[test]
    fn accepted_html_preserves_alpha_and_rejects_invalid_utf8_slices() {
        let mut draft = ColorDraft::parse("#ed1c2480", 255).unwrap();
        assert_eq!(draft.alpha, 128);
        draft.set_rgb(0, 42.);
        assert_eq!(draft.color(), "#2a1c24");
        assert!(ColorDraft::parse("#一abc", 255).is_none());
        assert!(ColorDraft::parse("#fffg00", 255).is_none());
        assert_eq!(ColorDraft::parse("#123456", 128).unwrap().alpha, 128);
    }
}

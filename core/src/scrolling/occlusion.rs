//! Pixel exclusions describe UI coverage, independently of image alpha.
//! Only pixels observed without an overlay may enter the exported document.
use crate::image::RgbaImage;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExcludedRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
impl ExcludedRect {
    pub(crate) fn contains(self, x: u32, y: u32) -> bool {
        x >= self.x && y >= self.y && x - self.x < self.width && y - self.y < self.height
    }
    fn intersection(self, other: Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self
            .x
            .saturating_add(self.width)
            .min(other.x.saturating_add(other.width));
        let bottom = self
            .y
            .saturating_add(self.height)
            .min(other.y.saturating_add(other.height));
        (right > x && bottom > y).then(|| Self {
            x,
            y,
            width: right - x,
            height: bottom - y,
        })
    }
    pub(crate) fn clipped(self, width: u32, height: u32) -> Option<Self> {
        self.intersection(Self {
            x: 0,
            y: 0,
            width,
            height,
        })
    }
    pub(crate) fn rotated(self, height: u32) -> Self {
        Self {
            x: height - self.y - self.height,
            y: self.x,
            width: self.height,
            height: self.width,
        }
    }
    pub(crate) fn scaled(self, scale: f64) -> Self {
        let x = (self.x as f64 * scale).floor() as u32;
        let y = (self.y as f64 * scale).floor() as u32;
        Self {
            x,
            y,
            width: ((self.x + self.width) as f64 * scale).ceil() as u32 - x,
            height: ((self.y + self.height) as f64 * scale).ceil() as u32 - y,
        }
    }
}
pub(super) fn intersection(a: &[ExcludedRect], b: &[ExcludedRect]) -> Vec<ExcludedRect> {
    let mut result = Vec::new();
    for rect in a
        .iter()
        .flat_map(|a| b.iter().filter_map(|b| a.intersection(*b)))
    {
        if !result.contains(&rect) {
            result.push(rect);
        }
    }
    result
}
pub(super) fn clear(image: &mut RgbaImage, excluded: &[ExcludedRect]) {
    for r in excluded {
        for y in r.y..r.y + r.height {
            let start = (y as usize * image.width() as usize + r.x as usize) * 4;
            image.as_mut()[start..start + r.width as usize * 4].fill(0);
        }
    }
}
/// Valid spans avoid a per-pixel mask allocation for every compressed frame.
pub(super) fn spans(width: u32, y: u32, excluded: &[ExcludedRect]) -> Vec<(u32, u32)> {
    let mut cuts: Vec<_> = excluded
        .iter()
        .filter(|r| y >= r.y && y - r.y < r.height)
        .map(|r| (r.x.min(width), r.x.saturating_add(r.width).min(width)))
        .collect();
    cuts.sort_unstable();
    let mut x = 0;
    let mut spans = Vec::with_capacity(cuts.len() + 1);
    for (left, right) in cuts {
        if left > x {
            spans.push((x, left));
        }
        x = x.max(right);
    }
    if x < width {
        spans.push((x, width));
    }
    spans
}
pub(super) fn merge(old: &mut RgbaImage, new: &RgbaImage, excluded: &[ExcludedRect]) {
    let width = old.width();
    for y in 0..old.height() {
        for (a, b) in spans(width, y, excluded) {
            let range = ((y as usize * width as usize + a as usize) * 4)
                ..((y as usize * width as usize + b as usize) * 4);
            old.as_mut()[range.clone()].copy_from_slice(&new.as_raw()[range]);
        }
    }
}

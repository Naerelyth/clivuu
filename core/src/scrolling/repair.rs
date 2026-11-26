//! Repair historical overlay coverage without navigating or appending a frame.
use super::*;
use crate::annotation::Bounds;

#[derive(Clone, Copy, Debug)]
pub struct Repair {
    pub viewport: Bounds,
    offset: i64,
}

impl Document {
    pub fn repairs(&self) -> Vec<Repair> {
        let height = self.dimensions.map_or(0, |(_, h)| i64::from(h));
        let mut seen = Vec::new();
        self.frames
            .iter()
            .rev()
            .filter_map(|f| {
                let has_visible_body = f.body_range.is_none_or(|[low, high]| {
                    high > low && high > f.offset && low < f.offset + height
                });
                let has_visible_excluded = !f.excluded.is_empty()
                    && f.body_range.is_none_or(|[low, high]| {
                        f.excluded.iter().any(|r| {
                            let y0 = f.offset + i64::from(r.y);
                            let y1 = y0 + i64::from(r.height);
                            y1 > low && y0 < high
                        })
                    });
                (has_visible_body && has_visible_excluded)
                    .then_some(f.viewport)
                    .flatten()
                    .and_then(|viewport| {
                        let req = Repair {
                            viewport,
                            offset: f.offset,
                        };
                        if seen.iter().any(|prev: &Repair| {
                            prev.offset == req.offset && prev.viewport == req.viewport
                        }) {
                            None
                        } else {
                            seen.push(req);
                            Some(req)
                        }
                    })
            })
            .collect()
    }

    /// The unoccluded pixels must still describe the recorded viewport. Never
    /// fill old coverage with unrelated content after the page has scrolled.
    pub fn repair(&mut self, request: Repair, image: RgbaImage) -> Result<()> {
        let indices: Vec<_> = self
            .frames
            .iter()
            .enumerate()
            .filter_map(|(i, f)| {
                (f.offset == request.offset && f.viewport == Some(request.viewport)).then_some(i)
            })
            .collect();
        if indices.is_empty() {
            return Err(t("No reliable overlap found").into());
        }
        let image = match self.axis {
            Axis::Vertical => image,
            Axis::Horizontal => image::imageops::rotate90(&image),
        };
        if Some(image.dimensions()) != self.dimensions {
            return Err(t("Capture dimensions changed; select the region again").into());
        }
        let fresh = Profile::new(image);
        let mut staged = Vec::with_capacity(indices.len());
        let mut total_bytes = self.bytes;
        for index in indices {
            let frame = &self.frames[index];
            let mut restored = (*frame.pixels.image()?).clone();
            // Clocks, hover effects and panel feedback can change in physically
            // fixed chrome. Validate the moving body before repairing its pixels.
            let mut validation = frame.excluded.clone();
            let (header, footer) = self
                .fixed
                .edges(frame.viewport, self.axis, restored.height());
            if header > 0 {
                validation.push(ExcludedRect {
                    x: 0,
                    y: 0,
                    width: restored.width(),
                    height: header,
                });
            }
            if footer > 0 {
                validation.push(ExcludedRect {
                    x: 0,
                    y: restored.height() - footer,
                    width: restored.width(),
                    height: footer,
                });
            }
            let old = Profile::masked(restored.clone(), &validation);
            if !old.same_visible_view(&fresh) {
                return Err(t("No reliable overlap found").into());
            }
            if frame.excluded.is_empty() {
                continue;
            }
            for r in &frame.excluded {
                for y in r.y..r.y + r.height {
                    for x in r.x..r.x + r.width {
                        restored.put_pixel(x, y, *fresh.image.get_pixel(x, y));
                    }
                }
            }
            let preview = make_preview_source(&restored);
            let pixels = match &frame.pixels {
                FramePixels::Raw(_)
                    if total_bytes - frame.pixels.byte_len() + restored.as_raw().len()
                        <= RAW_BUDGET_BYTES =>
                {
                    FramePixels::Raw(Arc::new(restored))
                }
                _ => FramePixels::Png(FramePixels::encode_fast(&restored)?),
            };
            total_bytes = total_bytes - frame.pixels.byte_len() + pixels.byte_len();
            if total_bytes > MAX_DOCUMENT_BYTES {
                return Err(t("Frame limit reached; finish the capture").into());
            }
            staged.push((index, pixels, preview));
        }
        self.bytes = total_bytes;
        for (index, pixels, preview) in staged {
            self.frames[index].pixels = pixels;
            self.frames[index].preview = preview;
            *self.frames[index].scaled_preview.lock().unwrap() = None;
            self.frames[index].excluded.clear();
        }
        retention::prune_clipped(self);
        Ok(())
    }
}

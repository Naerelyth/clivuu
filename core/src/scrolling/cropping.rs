//! Commit a directional cut, then continue matching from the current viewport.
use super::*;
impl Document {
    pub fn auto_crop(&self) -> bool {
        self.auto_crop
    }

    pub fn set_auto_crop(&mut self, enabled: bool) {
        if self.auto_crop == enabled {
            return;
        }
        self.auto_crop = enabled;
        self.auto_crop_anchor = self.position;
        if !enabled || self.maximum - self.minimum <= 4 {
            self.auto_crop_trend = 0;
        } else if self.auto_crop_trend == 0 {
            self.auto_crop_trend = if self.maximum - self.position <= self.position - self.minimum {
                1
            } else {
                -1
            };
        }
    }

    pub(super) fn apply_auto_crop(&mut self) -> Result<bool> {
        if !self.auto_crop || self.dimensions.is_none() {
            self.auto_crop_anchor = self.position;
            return Ok(false);
        }
        if self.maximum - self.minimum <= 4 {
            self.auto_crop_trend = 0;
            self.auto_crop_anchor = self.position;
            return Ok(false);
        }
        let delta = self.position - self.auto_crop_anchor;
        if delta.abs() < 2 {
            return Ok(false);
        }
        let step_dir = if delta > 0 { 1 } else { -1 };
        self.auto_crop_anchor = self.position;
        if self.auto_crop_trend == 0 {
            self.auto_crop_trend = step_dir;
            return Ok(false);
        }
        if step_dir == self.auto_crop_trend {
            return Ok(false);
        }
        // Keep primary_trend anchored to the capture growth direction so rolling
        // back across multiple wheel notches trims monotonically, while nudging
        // forward again resumes extending instead of cutting the leading end.
        let cropped = if self.auto_crop_trend > 0 && self.position < self.maximum {
            self.crop_to_view(false)?
        } else if self.auto_crop_trend < 0 && self.position > self.minimum {
            self.crop_to_view(true)?
        } else {
            false
        };
        if self.maximum - self.minimum <= 4 {
            self.auto_crop_trend = 0;
        }
        Ok(cropped)
    }

    /// Replace the matching baseline at the current document cursor while
    /// keeping all previously stitched frames intact (remedy for non-overshoot
    /// mismatches such as expanded sections, tab switches, or mutated content).
    pub fn reanchor_masked_at(
        &mut self,
        image: RgbaImage,
        excluded: Vec<ExcludedRect>,
        viewport: Option<crate::annotation::Bounds>,
    ) -> Result<Progress> {
        if self.frames.is_empty() || self.dimensions.is_none() {
            return self.push_sample(Arc::new(image), excluded, viewport);
        }
        let (image, excluded) = self.normalize_sample(Arc::new(image), excluded)?;
        let previous_excluded = excluded.clone();
        let profile = Arc::new(Profile::masked_arc(image, &excluded));
        let position = self.position.clamp(self.minimum, self.maximum);
        let body_verified = self
            .frames
            .iter()
            .any(|f| f.viewport == viewport && f.body_verified);
        if let Some(index) = self
            .frames
            .iter()
            .position(|frame| covers_view(frame, position, profile.image.height()))
        {
            self.merge_frame_pixels(index, &profile.image, &excluded)?;
            let remaining = occlusion::intersection(&self.frames[index].excluded, &excluded);
            let prev_range = self.frames[index].body_range;
            let prev_verified = self.frames[index].body_verified;
            self.frames[index].offset = position;
            self.frames[index].body_range = prev_range;
            self.frames[index].body_verified = prev_verified || body_verified;
            self.frames[index].viewport = viewport;
            self.frames[index].excluded = remaining;
        } else {
            let pixels = self.frame_pixels(Arc::clone(&profile.image), 0)?;
            let mut frame = Frame {
                offset: position,
                body_range: None,
                body_verified,
                viewport,
                pixels,
                preview: Arc::new(RgbaImage::new(1, 1)),
                scaled_preview: Mutex::new(None),
                excluded,
            };
            let remove = retention::removals(self, &frame, &self.fixed)?;
            let old_count = self.frames.len();
            let retained = !remove.contains(&old_count);
            for &index in remove.iter().rev().filter(|&&i| i < old_count) {
                self.bytes -= self.frames[index].pixels.byte_len();
                self.frames.remove(index);
            }
            if retained {
                frame.preview = make_preview_source(&profile.image);
                self.bytes += frame.pixels.byte_len();
                self.frames.push(frame);
            }
        }
        self.position = position;
        self.auto_crop_anchor = position;
        retention::prune_clipped(self);
        self.previous_viewport = viewport;
        self.previous_excluded = previous_excluded;
        self.previous = Some(profile);
        Ok(self.progress(true))
    }

    pub fn can_crop_to_view(&self, start: bool) -> bool {
        if self.dimensions.is_none() {
            return false;
        }
        let position = self.position.clamp(self.minimum, self.maximum);
        let (low, high) = if start {
            (position, self.maximum)
        } else {
            (self.minimum, position)
        };
        low != self.minimum || high != self.maximum
    }

    pub fn current_view_occluded(&self) -> bool {
        let Some((_, height)) = self.dimensions else {
            return false;
        };
        let position = self.position.clamp(self.minimum, self.maximum);
        !self
            .frames
            .iter()
            .any(|f| covers_view(f, position, height) && f.excluded.is_empty())
    }

    /// Return `true` only when committing `crop_to_view(start)` would leave
    /// uncovered rows unless the current occluded viewport is recaptured cleanly.
    pub fn crop_needs_clean_anchor(&self, start: bool) -> bool {
        if !self.can_crop_to_view(start) || !self.current_view_occluded() {
            return false;
        }
        let Some((_, height)) = self.dimensions else {
            return false;
        };
        let position = self.position.clamp(self.minimum, self.maximum);
        let (low, high) = if start {
            (position, self.maximum)
        } else {
            (self.minimum, position)
        };
        if low == high {
            return true;
        }
        let anchor = if start { low } else { high };
        let empty_preview = Arc::new(RgbaImage::new(1, 1));
        let mut scratch = Self {
            axis: self.axis,
            frames: self
                .frames
                .iter()
                .map(|f| Frame {
                    offset: f.offset,
                    body_range: f.body_range,
                    body_verified: f.body_verified,
                    viewport: f.viewport,
                    pixels: FramePixels::Png(vec![]),
                    preview: Arc::clone(&empty_preview),
                    scaled_preview: Mutex::new(None),
                    excluded: f.excluded.clone(),
                })
                .collect(),
            previous: None,
            position: self.position,
            dimensions: self.dimensions,
            minimum: self.minimum,
            maximum: self.maximum,
            fixed: self.fixed.clone(),
            previous_viewport: self.previous_viewport,
            bytes: 0,
            limit: self.limit,
            previous_excluded: vec![],
            auto_crop: false,
            auto_crop_trend: 0,
            auto_crop_anchor: 0,
        };
        if !scratch
            .frames
            .iter()
            .any(|f| covers_view(f, anchor, height))
        {
            scratch.frames.push(Frame {
                offset: anchor,
                body_range: None,
                body_verified: scratch
                    .frames
                    .iter()
                    .any(|f| f.body_verified && f.viewport == scratch.previous_viewport),
                viewport: scratch.previous_viewport,
                pixels: FramePixels::Png(vec![]),
                preview: empty_preview,
                scaled_preview: Mutex::new(None),
                excluded: self.previous_excluded.clone(),
            });
        }
        let (Some(low_frame), Some(high_frame)) = (
            scratch.boundary_frame(None, low),
            scratch.boundary_frame(None, high),
        ) else {
            return true;
        };
        let (raw_head, _) = scratch
            .fixed
            .edges(low_frame.viewport, scratch.axis, height);
        let (_, raw_foot) = scratch
            .fixed
            .edges(high_frame.viewport, scratch.axis, height);
        let head = if low_frame.excluded.iter().any(|r| r.y < raw_head) {
            0
        } else {
            raw_head
        };
        let foot = if high_frame
            .excluded
            .iter()
            .any(|r| r.y + r.height > height - raw_foot)
        {
            0
        } else {
            raw_foot
        };
        scratch.minimum = low;
        scratch.maximum = high;
        let view_end = high + i64::from(height);
        for frame in &mut scratch.frames {
            let s = low
                + if frame.offset < low {
                    i64::from(head)
                } else {
                    0
                };
            let e = view_end
                - if frame.offset > high {
                    i64::from(foot)
                } else {
                    0
                };
            if let Some(previous) = frame.body_range {
                frame.body_range = Some([previous[0].max(s), previous[1].min(e)]);
            } else if frame.offset < s || frame.offset + i64::from(height) > e {
                frame.body_range = Some([s, e]);
            }
        }
        let Ok(layout) = composition::Layout::new(&scratch, None, &scratch.fixed, false) else {
            return true;
        };
        let anchor_viewport = scratch
            .boundary_frame(None, anchor)
            .map_or(scratch.previous_viewport, |f| f.viewport);
        let clean_anchor = Frame {
            offset: anchor,
            body_range: None,
            body_verified: true,
            viewport: anchor_viewport,
            pixels: FramePixels::Png(vec![]),
            preview: Arc::new(RgbaImage::new(1, 1)),
            scaled_preview: Mutex::new(None),
            excluded: vec![],
        };
        let mut uncovered = layout.coverage(&clean_anchor, &scratch.fixed, false);
        if uncovered.is_empty() {
            return false;
        }
        for frame in &scratch.frames {
            for rect in layout.coverage(frame, &scratch.fixed, false) {
                uncovered = retention::subtract(uncovered, rect);
                if uncovered.is_empty() {
                    return false;
                }
            }
        }
        true
    }

    /// Keep the current view and the uncut end. Trimming a side that has no
    /// content beyond the current view is a no-op.
    pub fn crop_to_view(&mut self, start: bool) -> Result<bool> {
        let Some((_, height)) = self.dimensions else {
            return Ok(false);
        };
        let position = self.position.clamp(self.minimum, self.maximum);
        let (low, high) = if start {
            (position, self.maximum)
        } else {
            (self.minimum, position)
        };
        if low == self.minimum && high == self.maximum {
            return Ok(false);
        }
        let anchor = if start { low } else { high };
        // Keyframe retention can omit the current, already-covered sample.
        // Restore precisely that sample before constraining either side.
        let mut incoming = if self.frames.iter().any(|f| covers_view(f, anchor, height)) {
            None
        } else {
            let profile = self
                .previous
                .as_ref()
                .ok_or_else(|| t("The system returned no image"))?;
            let pixels = FramePixels::Raw(Arc::clone(&profile.image));
            let preview = make_preview_source(&profile.image);
            Some(Frame {
                offset: anchor,
                body_range: None,
                body_verified: self
                    .frames
                    .iter()
                    .any(|f| f.body_verified && f.viewport == self.previous_viewport),
                viewport: self.previous_viewport,
                pixels,
                preview,
                scaled_preview: Mutex::new(None),
                excluded: self.previous_excluded.clone(),
            })
        };
        let old_bounds = (self.minimum, self.maximum);
        let old_ranges: Vec<_> = self.frames.iter().map(|f| f.body_range).collect();
        let boundary = |offset| {
            self.boundary_frame(incoming.as_ref(), offset)
                .expect("cropped boundary has a retained viewport")
        };
        let (head, _) = self.fixed.edges(boundary(low).viewport, self.axis, height);
        let (_, foot) = self.fixed.edges(boundary(high).viewport, self.axis, height);
        // A clean boundary sample owns its fixed strip even if the estimate
        // later shrinks. Otherwise preserve crossing body donors for those
        // masked rows; never reclaim them solely to meet the frame budget.
        let head = if boundary(low).excluded.iter().any(|r| r.y < head) {
            0
        } else {
            head
        };
        let foot = if boundary(high)
            .excluded
            .iter()
            .any(|r| r.y + r.height > height - foot)
        {
            0
        } else {
            foot
        };
        self.minimum = low;
        self.maximum = high;
        // Boundary-crossing frames may be the only donors for pixels masked by
        // the HUD. Keep their proven pixels, but clip their body contribution
        // permanently so scrolling back cannot resurrect discarded content.
        // Interior unclipped frames already lie within `[low, high + height]`
        // and keep `body_range == None` so overshoot donor recovery stays intact.
        let view_end = high + i64::from(height);
        for frame in self.frames.iter_mut().chain(incoming.iter_mut()) {
            let start = low
                + if frame.offset < low {
                    i64::from(head)
                } else {
                    0
                };
            let end = view_end
                - if frame.offset > high {
                    i64::from(foot)
                } else {
                    0
                };
            if let Some(previous) = frame.body_range {
                frame.body_range = Some([previous[0].max(start), previous[1].min(end)]);
            } else if frame.offset < start || frame.offset + i64::from(height) > end {
                frame.body_range = Some([start, end]);
            }
        }
        if let Some(mut frame) = incoming {
            let needed = frame.pixels.byte_len();
            self.compact_raw_frames(needed)?;
            if self.bytes + frame.pixels.byte_len() > MAX_DOCUMENT_BYTES
                || frame.pixels.byte_len() > RAW_BUDGET_BYTES
            {
                frame.pixels.compress()?;
            }
            // Judge the budget after the cut: discarded body pixels no longer
            // need donors. Stage only metadata until that coverage proof passes.
            let remove = match retention::removals(self, &frame, &self.fixed) {
                Ok(remove) => remove,
                Err(error) => {
                    (self.minimum, self.maximum) = old_bounds;
                    for (frame, range) in self.frames.iter_mut().zip(old_ranges) {
                        frame.body_range = range;
                    }
                    return Err(error);
                }
            };
            let old_count = self.frames.len();
            for &index in remove.iter().rev().filter(|&&i| i < old_count) {
                self.bytes -= self.frames[index].pixels.byte_len();
                self.frames.remove(index);
            }
            if !remove.contains(&old_count) {
                self.bytes += frame.pixels.byte_len();
                self.frames.push(frame);
            }
        }
        retention::prune_clipped(self);
        // The matching baseline is still the latest committed source frame;
        // no capture API or UI state is involved in a document cut.
        self.position = position;
        self.auto_crop_anchor = position;
        if self.maximum - self.minimum <= 4 {
            self.auto_crop_trend = 0;
        }
        debug_assert!(self.minimum <= self.position && self.position <= self.maximum);
        debug_assert!(self.extent() >= height);
        Ok(true)
    }
}

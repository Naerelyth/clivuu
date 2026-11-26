//! Shared in-place OCR text-selection overlay state, coordinate mapping, and GSK rendering
//! used identically by both capture overlays and pin windows.
use clivuu_core::{
    annotation::Bounds,
    ocr::{OcrDocument, OcrSelection, OcrSettings},
};
use gtk::{gdk, gio, glib, graphene, gsk, prelude::*};

#[derive(Clone, Debug)]
pub enum OcrPhase {
    Inactive,
    NeedsSetup,
    Scanning {
        started_us: i64,
        cancel: gio::Cancellable,
    },
    Ready,
    Failed(String),
}

pub struct OcrOverlayState {
    pub phase: OcrPhase,
    pub selection: OcrSelection,
    pub settings: OcrSettings,
    pub dragging: bool,
    pub copied_until_us: i64,
    pub generation: u64,
}

impl Default for OcrOverlayState {
    fn default() -> Self {
        Self {
            phase: OcrPhase::Inactive,
            selection: OcrSelection::default(),
            settings: OcrSettings::default(),
            dragging: false,
            copied_until_us: 0,
            generation: 0,
        }
    }
}

impl OcrOverlayState {
    pub fn is_active(&self) -> bool {
        !matches!(self.phase, OcrPhase::Inactive)
    }

    pub fn is_scanning(&self) -> bool {
        matches!(self.phase, OcrPhase::Scanning { .. })
    }

    pub fn is_ready(&self) -> bool {
        matches!(self.phase, OcrPhase::Ready)
    }

    pub fn deactivate(&mut self) {
        if let OcrPhase::Scanning { cancel, .. } = &self.phase {
            cancel.cancel();
        }
        self.generation = self.generation.wrapping_add(1);
        self.phase = OcrPhase::Inactive;
        self.selection.clear();
        self.dragging = false;
        self.copied_until_us = 0;
    }

    pub fn begin_scan(&mut self) -> (u64, gio::Cancellable) {
        if let OcrPhase::Scanning { cancel, .. } = &self.phase {
            cancel.cancel();
        }
        self.generation = self.generation.wrapping_add(1);
        self.selection.clear();
        self.dragging = false;
        let cancel = gio::Cancellable::new();
        self.phase = OcrPhase::Scanning {
            started_us: glib::monotonic_time(),
            cancel: cancel.clone(),
        };
        (self.generation, cancel)
    }

    pub fn enter_needs_setup(&mut self) {
        if let OcrPhase::Scanning { cancel, .. } = &self.phase {
            cancel.cancel();
        }
        self.generation = self.generation.wrapping_add(1);
        self.selection.clear();
        self.dragging = false;
        self.phase = OcrPhase::NeedsSetup;
    }

    pub fn finish_scan(&mut self, generation: u64, result: Result<OcrDocument, String>) -> bool {
        if self.generation != generation {
            return false;
        }
        self.dragging = false;
        match result {
            Ok(doc) => {
                self.selection.set_document(doc);
                self.phase = OcrPhase::Ready;
            }
            Err(err) => {
                self.selection.clear();
                self.phase = OcrPhase::Failed(err);
            }
        }
        true
    }

    pub fn mark_copied(&mut self) {
        self.copied_until_us = glib::monotonic_time() + 1_200_000;
    }

    pub fn recently_copied(&self) -> bool {
        self.copied_until_us > glib::monotonic_time()
    }

    fn doc_dimensions(&self) -> Option<(f64, f64)> {
        let doc = self.selection.document()?;
        if doc.width == 0 || doc.height == 0 {
            return None;
        }
        Some((doc.width as f64, doc.height as f64))
    }

    pub fn to_doc_point(&self, target_rect: Bounds, global: [f64; 2]) -> Option<(f64, f64)> {
        let (dw, dh) = self.doc_dimensions()?;
        if target_rect.width <= 0.0 || target_rect.height <= 0.0 {
            return None;
        }
        let lx = (global[0] - target_rect.x) * dw / target_rect.width;
        let ly = (global[1] - target_rect.y) * dh / target_rect.height;
        Some((lx, ly))
    }

    pub fn line_at(&self, target_rect: Bounds, global: [f64; 2]) -> Option<usize> {
        if !self.is_ready() || !target_rect.contains(global, 4.0) {
            return None;
        }
        let pt = self.to_doc_point(target_rect, global)?;
        self.selection.line_at(pt)
    }

    pub fn handle_press(&mut self, target_rect: Bounds, global: [f64; 2], clicks: i32) -> bool {
        if !self.is_ready() || !target_rect.contains(global, 4.0) {
            return false;
        }
        let Some(pt) = self.to_doc_point(target_rect, global) else {
            return false;
        };
        if clicks >= 3 {
            if let Some(line) = self.selection.line_at(pt) {
                self.dragging = false;
                let _ = self.selection.select_block(line);
                return true;
            }
        } else if clicks == 2
            && let Some(line) = self.selection.line_at(pt)
        {
            self.dragging = false;
            let _ = self.selection.select_line(line);
            return true;
        }
        if self.selection.line_at(pt).is_some() {
            self.dragging = true;
            let _ = self.selection.begin_drag(pt);
            return true;
        }
        if self.selection.has_selection() {
            self.dragging = false;
            self.selection.deselect();
            return true;
        }
        false
    }

    pub fn handle_motion(&mut self, target_rect: Bounds, global: [f64; 2]) -> bool {
        if !self.dragging || !self.is_ready() {
            return false;
        }
        let Some(pt) = self.to_doc_point(target_rect, global) else {
            return false;
        };
        self.selection.extend_drag(pt)
    }

    pub fn handle_release(&mut self) -> bool {
        let was = self.dragging;
        self.dragging = false;
        was
    }

    pub fn text_to_copy(&self) -> Option<String> {
        if !self.is_ready() || !self.selection.is_active() {
            return None;
        }
        let text = self
            .selection
            .selected_or_all_text(self.settings.smart_paragraphs);
        (!text.is_empty()).then_some(text)
    }
}

/// Render the in-place OCR overlay (scanning band or block plates + selection highlights)
/// clipped to `target_rect` in the coordinate space of a surface rooted at `surface_origin`.
pub fn paint_ocr_overlay(
    snapshot: &gtk::Snapshot,
    state: &OcrOverlayState,
    target_rect: Bounds,
    surface_origin: [f64; 2],
    accent: gdk::RGBA,
    dark: bool,
) {
    if !state.is_active() || target_rect.width <= 1.0 || target_rect.height <= 1.0 {
        return;
    }
    let rx = (target_rect.x - surface_origin[0]) as f32;
    let ry = (target_rect.y - surface_origin[1]) as f32;
    let rw = target_rect.width as f32;
    let rh = target_rect.height as f32;
    let clip = graphene::Rect::new(rx, ry, rw, rh);

    snapshot.push_clip(&clip);

    match &state.phase {
        OcrPhase::Inactive => {}
        OcrPhase::NeedsSetup | OcrPhase::Failed(_) => {
            let shade = if dark {
                gdk::RGBA::new(0.06, 0.07, 0.10, 0.22)
            } else {
                gdk::RGBA::new(0.96, 0.97, 0.99, 0.22)
            };
            snapshot.append_color(&shade, &clip);
        }
        OcrPhase::Scanning { started_us, .. } => {
            let shade = if dark {
                gdk::RGBA::new(0.06, 0.07, 0.10, 0.20)
            } else {
                gdk::RGBA::new(0.08, 0.10, 0.14, 0.12)
            };
            snapshot.append_color(&shade, &clip);

            // Ping-pong vertical sweep every 1.35s for crisp visual scanning feedback.
            let elapsed_s = ((glib::monotonic_time() - *started_us).max(0) as f64) / 1_000_000.0;
            let cycle = (elapsed_s / 1.35) % 2.0;
            let t = if cycle <= 1.0 { cycle } else { 2.0 - cycle } as f32;
            let ease = t * t * (3.0 - 2.0 * t);
            let beam_y = ry + ease * rh;
            let glow_h = (rh * 0.16).clamp(14.0, 44.0);
            let glow_rect = graphene::Rect::new(rx, (beam_y - glow_h * 0.5).max(ry), rw, glow_h);
            let glow = gdk::RGBA::new(accent.red(), accent.green(), accent.blue(), 0.15);
            snapshot.append_color(&glow, &glow_rect);

            let line_rect =
                graphene::Rect::new(rx, (beam_y - 1.0).clamp(ry, ry + rh - 2.0), rw, 2.0);
            let line_color = gdk::RGBA::new(accent.red(), accent.green(), accent.blue(), 0.88);
            snapshot.append_color(&line_color, &line_rect);
        }
        OcrPhase::Ready => {
            let Some(doc) = state.selection.document() else {
                snapshot.pop();
                return;
            };
            if doc.width == 0 || doc.height == 0 {
                snapshot.pop();
                return;
            }
            let sx = rw / doc.width as f32;
            let sy = rh / doc.height as f32;

            // Subtle frost dim over non-text background so selectable blocks stand out cleanly.
            let dim = if dark {
                gdk::RGBA::new(0.05, 0.06, 0.09, 0.16)
            } else {
                gdk::RGBA::new(0.08, 0.09, 0.12, 0.10)
            };
            snapshot.append_color(&dim, &clip);

            let plate_fill = if dark {
                gdk::RGBA::new(1.0, 1.0, 1.0, 0.11)
            } else {
                gdk::RGBA::new(1.0, 1.0, 1.0, 0.28)
            };
            let plate_border = if dark {
                gdk::RGBA::new(1.0, 1.0, 1.0, 0.18)
            } else {
                gdk::RGBA::new(0.15, 0.18, 0.24, 0.16)
            };
            for block in &doc.blocks {
                let bx = rx + block.bounds.x * sx - 3.0;
                let by = ry + block.bounds.y * sy - 1.5;
                let bw = block.bounds.width * sx + 6.0;
                let bh = block.bounds.height * sy + 3.0;
                let rect = graphene::Rect::new(bx, by, bw, bh);
                let rounded = gsk::RoundedRect::from_rect(rect, 4.5);
                snapshot.push_rounded_clip(&rounded);
                snapshot.append_color(&plate_fill, &rect);
                snapshot.pop();
                snapshot.append_border(&rounded, &[1.0; 4], &[plate_border; 4]);
            }

            let sel_fill = gdk::RGBA::new(accent.red(), accent.green(), accent.blue(), 0.36);
            let sel_border = gdk::RGBA::new(accent.red(), accent.green(), accent.blue(), 0.72);
            for span in state.selection.selected_rects() {
                let lx = rx + span.x * sx;
                let ly = ry + span.y * sy;
                let lw = (span.width * sx).max(2.0);
                let lh = (span.height * sy).max(2.0);
                let rect = graphene::Rect::new(lx, ly, lw, lh);
                let rounded = gsk::RoundedRect::from_rect(rect, 2.5);
                snapshot.push_rounded_clip(&rounded);
                snapshot.append_color(&sel_fill, &rect);
                snapshot.pop();
                snapshot.append_border(&rounded, &[1.0; 4], &[sel_border; 4]);
            }
        }
    }

    snapshot.pop();
}

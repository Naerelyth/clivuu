//! Fixed chrome is anchored to the capture's physical geometry, not its
//! accumulated scroll offset. Moving the region must not erase this evidence.
use super::Axis;
use crate::annotation::Bounds;

#[derive(Clone, Default, PartialEq)]
pub(super) struct Fixed {
    header: Option<f64>,
    footer: Option<f64>,
    matched: bool,
    viewport: Option<Bounds>,
}
impl Fixed {
    fn delta(&self, viewport: Option<Bounds>, axis: Axis, height: u32) -> f64 {
        Self::physical_delta(self.viewport, viewport, axis, height)
    }
    pub(super) fn physical_delta(
        reference: Option<Bounds>,
        viewport: Option<Bounds>,
        axis: Axis,
        height: u32,
    ) -> f64 {
        match (reference, viewport) {
            (Some(reference), Some(current)) => {
                let (a, b, span) = match axis {
                    Axis::Vertical => (reference.y, current.y, current.height),
                    Axis::Horizontal => (reference.x, current.x, current.width),
                };
                (b - a) * height as f64 / span
            }
            _ => 0.,
        }
    }
    pub fn observe(
        &mut self,
        header: u32,
        footer: u32,
        viewport: Option<Bounds>,
        axis: Axis,
        height: u32,
    ) {
        if !self.matched {
            self.header = (header > 0).then_some(header as f64);
            self.footer = (footer > 0).then_some(footer as f64);
            self.viewport = viewport;
            self.matched = true;
            return;
        }
        let delta = self.delta(viewport, axis, height);
        // Signed boundaries may lie outside the original viewport. A newly
        // exposed title/footer can establish an edge there, without turning a
        // later blank paragraph inside a known body region into fixed chrome.
        let head = (header > 0).then_some(header as f64 + delta);
        let foot = (footer > 0).then_some(footer as f64 - delta);
        match self.header {
            Some(old) if old > delta => self.header = head.map(|v| old.min(v)),
            None if head.is_some_and(|v| v <= 0.) => self.header = head,
            _ => {}
        }
        match self.footer {
            Some(old) if old > -delta => self.footer = foot.map(|v| old.min(v)),
            None if foot.is_some_and(|v| v <= 0.) => self.footer = foot,
            _ => {}
        }
    }

    pub fn edges(&self, viewport: Option<Bounds>, axis: Axis, height: u32) -> (u32, u32) {
        let delta = self.delta(viewport, axis, height);
        let header = self
            .header
            .map_or(0, |h| (h - delta).round().clamp(0., height as f64) as u32);
        let footer = self.footer.map_or(0, |f| {
            (f + delta).round().clamp(0., (height - header) as f64) as u32
        });
        (header, footer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn newly_visible_edges_can_lie_outside_the_original_viewport() {
        let base = Bounds {
            x: 200.,
            y: 200.,
            width: 320.,
            height: 320.,
        };
        for axis in [Axis::Vertical, Axis::Horizontal] {
            for edge in [0, 1] {
                let mut fixed = Fixed::default();
                fixed.observe(0, 0, Some(base), axis, 320);
                let delta = if edge == 0 { -100. } else { 100. };
                let moved = if axis == Axis::Vertical {
                    Bounds {
                        y: base.y + delta,
                        ..base
                    }
                } else {
                    Bounds {
                        x: base.x + delta,
                        ..base
                    }
                };
                fixed.observe(
                    if edge == 0 { 40 } else { 0 },
                    if edge == 1 { 40 } else { 0 },
                    Some(moved),
                    axis,
                    320,
                );
                assert_eq!(
                    fixed.edges(Some(moved), axis, 320),
                    if edge == 0 { (40, 0) } else { (0, 40) }
                );
                assert_eq!(fixed.edges(Some(base), axis, 320), (0, 0));
            }
        }
    }
}

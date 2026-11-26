//! Temporary magnifier over native pin hosts; cached widgets never receive input.
use super::*;
use crate::ui::viewport::Viewport;
#[derive(Default)]
pub(super) struct Lens {
    pub alt: u8,
    pub hex: bool,
    pub widgets: Vec<(glib::WeakRef<Viewport>, crate::editor::capture_view::Canvas)>,
}
impl Drop for Lens {
    fn drop(&mut self) {
        for (root, w) in self.widgets.drain(..) {
            if let Some(root) = root.upgrade()
                && w.parent().is_some()
            {
                root.remove(&w);
            }
        }
    }
}
impl Pins {
    pub(super) fn lens_key(self: &Rc<Self>, id: u64, key: gdk::Key, released: bool) -> bool {
        match key {
            gdk::Key::Alt_L | gdk::Key::Alt_R => {
                let bit = if key == gdk::Key::Alt_L { 1 } else { 2 };
                let mut lens = self.lens.borrow_mut();
                if released {
                    lens.alt &= !bit;
                } else {
                    lens.alt |= bit;
                }
                drop(lens);
                self.update_lens(id);
                true
            }
            gdk::Key::Shift_L | gdk::Key::Shift_R if !released && self.lens.borrow().alt != 0 => {
                let mut lens = self.lens.borrow_mut();
                lens.hex = !lens.hex;
                drop(lens);
                self.update_lens(id);
                true
            }
            gdk::Key::c | gdk::Key::C if !released && self.lens.borrow().alt != 0 => {
                let board = self.board.borrow();
                let data = self.data.borrow();
                if let (Some(pin), Some(d), Some(point)) =
                    (board.get(id), data.get(&id), self.pointer.get())
                {
                    let r = pin.rect();
                    if r.contains(point, 0.) {
                        let x = ((point[0] - r.x) * d.image.pixels.width() as f64 / r.width)
                            .floor()
                            .clamp(0., d.image.pixels.width() as f64 - 1.)
                            as u32;
                        let y = ((point[1] - r.y) * d.image.pixels.height() as f64 / r.height)
                            .floor()
                            .clamp(0., d.image.pixels.height() as f64 - 1.)
                            as u32;
                        let p = d.image.pixels.get_pixel(x, y);
                        self.display
                            .clipboard()
                            .set_text(&if self.lens.borrow().hex {
                                format!("#{:02X}{:02X}{:02X}", p[0], p[1], p[2])
                            } else {
                                format!("{}, {}, {}", p[0], p[1], p[2])
                            });
                    }
                }
                true
            }
            _ => false,
        }
    }
    pub(super) fn update_lens(self: &Rc<Self>, id: u64) {
        let point = self.pointer.get();
        let rect = self.board.borrow().get(id).map(Pin::rect);
        let visible =
            self.lens.borrow().alt != 0 && rect.zip(point).is_some_and(|(r, p)| r.contains(p, 0.));
        if !visible {
            for (_, w) in &self.lens.borrow().widgets {
                w.set_visible(false);
            }
            return;
        }
        if self.lens.borrow().widgets.is_empty()
            && let Some(hosts) = self.hosts.borrow().as_ref()
        {
            for surface in &hosts.surfaces {
                let weak = Rc::downgrade(self);
                let context = surface.root.pango_context();
                let canvas = crate::editor::capture_view::Canvas::new(move |s| {
                    let Some(this) = weak.upgrade() else {
                        return;
                    };
                    let Some(point) = this.pointer.get() else {
                        return;
                    };
                    let board = this.board.borrow();
                    let data = this.data.borrow();
                    let Some(id) = this.lens_id.get() else {
                        return;
                    };
                    let (Some(pin), Some(d)) = (board.get(id), data.get(&id)) else {
                        return;
                    };
                    let r = pin.rect();
                    if let Some(mut selection) = clivuu_core::capture::Selection::new(
                        vec![clivuu_core::capture::Output { rect: r, scale: 1. }],
                        vec![],
                        Some(r),
                    ) {
                        selection.pointer = Some(point);
                        let style = crate::editor::capture_draw::CaptureStyle {
                            magnifier_mask: false,
                            magnifier_border: false,
                            magnifier_anchors: false,
                            ..Default::default()
                        };
                        crate::editor::capture_draw::lens(
                            s,
                            &d.image,
                            &selection,
                            &style,
                            &context,
                            this.lens.borrow().hex,
                        );
                    }
                });
                surface.root.put(&canvas, 0., 0.);
                surface.root.set_child_size(&canvas, Some((184, 236)));
                self.lens
                    .borrow_mut()
                    .widgets
                    .push((surface.root.downgrade(), canvas));
            }
        }
        self.lens_id.set(Some(id));
        let p = point.unwrap();
        if let Some(hosts) = self.hosts.borrow().as_ref() {
            for ((_, canvas), s) in self.lens.borrow().widgets.iter().zip(&hosts.surfaces) {
                let visible = s.rect.contains(p, 0.);
                canvas.set_visible(visible);
                if visible {
                    let r = s.rect;
                    let x = (p[0] + 26.).min(r.x + r.width - 192.).max(r.x + 8.);
                    let y = (p[1] + 26.).min(r.y + r.height - 244.).max(r.y + 8.);
                    s.root.move_(canvas, x - r.x, y - r.y);
                    canvas.queue_draw();
                }
            }
        }
    }
}

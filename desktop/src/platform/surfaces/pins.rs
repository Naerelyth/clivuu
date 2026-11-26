//! Persistent pin-only input surfaces; outside-click menu surfaces are separate
//! owners and are destroyed on dismissal, never merged into a pin input shape.
use super::{
    MenuWatch, PinRole,
    backend::Backend,
    capture::{LayoutWatch, gdk_outputs},
};
use crate::ui::viewport::Viewport;
use clivuu_core::{annotation::Bounds, capture::Output, i18n::t};
use gtk::{cairo, gdk, gio, glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
pub struct Surface {
    pub window: gtk::Window,
    pub root: Viewport,
    pub rect: Bounds,
    pub scale: f64,
    canvas: Bounds,
    backend: Backend,
    role: PinRole,
    keyboard: Cell<bool>,
    fresh_frames: Cell<u8>,
    input: RefCell<cairo::Region>,
    clock: RefCell<Option<(gdk::FrameClock, glib::SignalHandlerId)>>,
}
impl Surface {
    pub fn uses_native_focus(&self) -> bool {
        self.backend.uses_native_focus()
    }

    /// A monitor's scale may be estimated from rounded output geometry. Use the
    /// surface's configured scale for pixel-exact bitmap placement instead.
    pub async fn pixel_scale(&self, cancel: &gio::Cancellable) -> Result<f64, String> {
        if self.backend.uses_native_focus() && !self.window.is_mapped() {
            struct Probe<'a> {
                window: &'a gtk::Window,
                visible: bool,
                signal: Option<(gdk::FrameClock, glib::SignalHandlerId)>,
            }
            impl Drop for Probe<'_> {
                fn drop(&mut self) {
                    if let Some((clock, id)) = self.signal.take() {
                        clock.disconnect(id);
                    }
                    if !self.visible {
                        self.window.set_visible(false);
                    }
                }
            }
            // The existing surface has an empty input region and transparent
            // content. ON_DEMAND mapping cannot take keyboard focus.
            let mut probe = Probe {
                window: &self.window,
                visible: self.window.is_visible(),
                signal: None,
            };
            self.window.set_visible(true);
            let clock = self
                .window
                .frame_clock()
                .ok_or_else(|| t("The desktop service is unavailable"))?;
            let (sender, receiver) = async_channel::bounded(1);
            let id = clock.connect_after_paint(move |_| {
                let _ = sender.try_send(());
            });
            probe.signal = Some((clock, id));
            glib::future_with_timeout(
                std::time::Duration::from_secs(2),
                gio::CancellableFuture::new(receiver.recv(), cancel.clone()),
            )
            .await
            .map_err(|_| t("The desktop service is unavailable"))?
            .map_err(|_| t("Operation cancelled"))?
            .map_err(|_| t("Operation cancelled"))?;
            let scale = self
                .window
                .surface()
                .map(|s| s.scale())
                .filter(|s| s.is_finite() && *s > 0.)
                .ok_or_else(|| t("The desktop service is unavailable"))?;
            return Ok(scale);
        }
        self.window
            .surface()
            .map(|s| s.scale())
            .filter(|s| s.is_finite() && *s > 0.)
            .ok_or_else(|| t("The desktop service is unavailable"))
    }

    fn apply_input(&self, force: bool) {
        if let Some(s) = self.window.surface() {
            let region = self.input.borrow();
            if force && !self.uses_native_focus() {
                let bust = cairo::Region::create_rectangle(&cairo::RectangleInt::new(-2, -2, 1, 1));
                s.set_input_region(&bust);
            }
            s.set_input_region(&region);
        }
    }
    pub fn set_input(&self, rects: impl IntoIterator<Item = Bounds>) {
        let region = cairo::Region::create();
        for r in rects {
            let left = (r.x - self.rect.x).floor().max(0.);
            let top = (r.y - self.rect.y).floor().max(0.);
            let right = (r.x + r.width - self.rect.x).ceil().min(self.rect.width);
            let bottom = (r.y + r.height - self.rect.y).ceil().min(self.rect.height);
            if right > left && bottom > top {
                let _ = region.union_rectangle(&cairo::RectangleInt::new(
                    left as i32,
                    top as i32,
                    (right - left) as i32,
                    (bottom - top) as i32,
                ));
            }
        }
        if *self.input.borrow() != region {
            self.input.replace(region);
            self.apply_input(self.fresh_frames.get() > 0);
        }
    }
    pub fn set_keyboard(&self, enabled: bool, focus: bool) -> Result<(), String> {
        let was = self.keyboard.replace(enabled);
        if !self.uses_native_focus() {
            self.window.set_focusable(enabled);
        }
        if was == enabled && !focus {
            return Ok(());
        }
        if was && !enabled {
            gtk::prelude::GtkWindowExt::set_focus(&self.window, None::<&gtk::Widget>);
        }
        self.backend.set_pin_keyboard(
            &self.window,
            self.canvas,
            self.role,
            enabled,
            focus,
            was && !enabled && self.window.is_mapped(),
        )?;
        if was && !enabled {
            self.window.queue_draw();
        }
        Ok(())
    }
}
impl Drop for Surface {
    fn drop(&mut self) {
        if let Some((clock, id)) = self.clock.take() {
            clock.disconnect(id);
        }
        self.window.destroy();
    }
}
pub struct Hosts {
    pub surfaces: Vec<Rc<Surface>>,
    _group: gtk::WindowGroup,
    _watch: LayoutWatch,
}
impl Hosts {
    pub fn watch_menu(&self, dismiss: impl Fn() + 'static) -> MenuWatch {
        self.surfaces[0].backend.watch_menu(dismiss)
    }

    pub fn new(
        display: &gdk::Display,
        role: PinRole,
        changed: impl Fn() + 'static,
    ) -> Result<Self, String> {
        let outputs = gdk_outputs(display);
        if outputs.is_empty() {
            return Err(t("Invalid monitor layout"));
        }
        let backend = Backend::detect(display)?;
        let group = gtk::WindowGroup::new();
        let mut surfaces = vec![];
        let physical = display.monitors();
        for (i, Output { rect, scale }) in outputs.iter().copied().enumerate() {
            let canvas = backend.pin_canvas(rect, &outputs);
            let window = gtk::Window::builder()
                .decorated(false)
                .title(if role == PinRole::Menu {
                    "Clivuu pin menu"
                } else {
                    "Clivuu pins"
                })
                .default_width(canvas.width as i32)
                .default_height(canvas.height as i32)
                .build();
            window.add_css_class("clivuu-layer");
            group.add_window(&window);
            let root = Viewport::new();
            root.set_hexpand(true);
            root.set_vexpand(true);
            root.set_overflow(gtk::Overflow::Hidden);
            root.set_clip(Some(gtk::graphene::Rect::new(
                0.,
                0.,
                rect.width as f32,
                rect.height as f32,
            )));
            window.set_child(Some(&root));
            let surface = Rc::new(Surface {
                window: window.clone(),
                root,
                rect,
                scale,
                canvas,
                backend: backend.clone(),
                role,
                keyboard: Cell::new(false),
                fresh_frames: Cell::new(2),
                input: RefCell::new(cairo::Region::create()),
                clock: RefCell::new(None),
            });
            let monitor = physical
                .item(i as u32)
                .and_downcast::<gdk::Monitor>()
                .ok_or_else(|| t("Invalid monitor layout"))?;
            backend.prepare_pin(&window, canvas, &monitor, role)?;
            let weak = Rc::downgrade(&surface);
            window.connect_map(move |_| {
                if let Some(s) = weak.upgrade() {
                    if let Err(e) = s.backend.set_pin_keyboard(
                        &s.window,
                        s.canvas,
                        s.role,
                        s.keyboard.get(),
                        false,
                        false,
                    ) {
                        eprintln!("{e}");
                        s.window.set_visible(false);
                        return;
                    }
                    s.fresh_frames.set(2);
                    s.apply_input(true);
                    s.backend.report_frame_timings(&s.window);
                    if let Some((old_clock, old_id)) = s.clock.borrow_mut().take() {
                        old_clock.disconnect(old_id);
                    }
                    if let Some(clock) = s.window.frame_clock() {
                        let w = Rc::downgrade(&s);
                        let id = clock.connect_after_paint(move |_| {
                            if let Some(s) = w.upgrade() {
                                let rem = s.fresh_frames.get();
                                if rem > 1 {
                                    s.fresh_frames.set(rem - 1);
                                    s.apply_input(true);
                                } else if rem == 1 {
                                    s.fresh_frames.set(0);
                                    s.apply_input(false);
                                    if s.uses_native_focus()
                                        && let Some((clock, id)) = s.clock.borrow_mut().take()
                                    {
                                        clock.disconnect(id);
                                        return;
                                    }
                                }
                                s.backend.report_frame_timings(&s.window);
                            }
                        });
                        s.clock.replace(Some((clock, id)));
                    }
                }
            });
            let weak = Rc::downgrade(&surface);
            window.connect_unmap(move |_| {
                if let Some(s) = weak.upgrade()
                    && let Some((clock, id)) = s.clock.borrow_mut().take()
                {
                    clock.disconnect(id);
                }
            });
            surfaces.push(surface);
        }
        Ok(Self {
            surfaces,
            _group: group,
            _watch: LayoutWatch::new(display, changed),
        })
    }
}

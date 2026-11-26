//! Native text input on the existing editor surface. GTK supplies selection,
//! IME and preedit; the shared session owns content, geometry and history.
use super::viewport::Viewport;
use crate::editor::{Action, Effect, render::text_metrics, scene::Scene};
use gtk::{gdk, glib, graphene, gsk, prelude::*, subclass::prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
mod imp {
    use super::*;
    #[derive(Default)]
    pub struct FeedbackText {
        pub on: Cell<bool>,
        pub scale: Cell<f64>,
    }
    #[glib::object_subclass]
    impl ObjectSubclass for FeedbackText {
        const NAME: &'static str = "ClivuuFeedbackText";
        type Type = super::FeedbackText;
        type ParentType = gtk::TextView;
    }
    impl ObjectImpl for FeedbackText {}
    impl TextViewImpl for FeedbackText {}
    impl WidgetImpl for FeedbackText {
        fn snapshot(&self, s: &gtk::Snapshot) {
            self.parent_snapshot(s);
            let v = self.obj();
            if !v.has_focus() || !self.on.get() || v.buffer().has_selection() {
                return;
            }
            let (r, _) = v.cursor_locations(None);
            if r.height() <= 0 {
                return;
            }
            let (x, y) = v.buffer_to_window_coords(gtk::TextWindowType::Widget, r.x(), r.y());
            let width = 1. / self.scale.get().abs().max(0.01) as f32;
            s.append_color(
                &gdk::RGBA::BLACK,
                &graphene::Rect::new(x as f32 - width, y as f32, width, r.height() as f32),
            );
            s.append_color(
                &gdk::RGBA::WHITE,
                &graphene::Rect::new(x as f32, y as f32, width, r.height() as f32),
            );
        }
    }
}
glib::wrapper! {pub struct FeedbackText(ObjectSubclass<imp::FeedbackText>) @extends gtk::TextView,gtk::Widget,@implements gtk::Accessible,gtk::Buildable,gtk::ConstraintTarget,gtk::Scrollable;}
struct Inner {
    scene: glib::WeakRef<Scene>,
    root: glib::WeakRef<Viewport>,
    clip: Viewport,
    view: FeedbackText,
    syncing: Cell<bool>,
    pending: Cell<bool>,
    scheduled: Cell<bool>,
    preedit: RefCell<String>,
    provider: gtk::CssProvider,
    class: String,
    css: RefCell<String>,
    active: Cell<bool>,
    enabled: Cell<bool>,
    before: RefCell<Option<crate::editor::scene::DispatchHook>>,
    events: RefCell<Option<gtk::EventControllerLegacy>>,
    origin: Cell<[f64; 2]>,
}
impl Drop for Inner {
    fn drop(&mut self) {
        gtk::style_context_remove_provider_for_display(&self.view.display(), &self.provider);
        if let Some(root) = self.root.upgrade() {
            if let Some(events) = self.events.take() {
                root.remove_controller(&events);
            }
            if self.clip.parent().is_some() {
                root.remove(&self.clip);
            }
        }
    }
}
#[derive(Clone)]
pub struct InlineText(Rc<Inner>);
impl InlineText {
    pub fn set_enabled(&self, enabled: bool) {
        if self.0.enabled.replace(enabled) != enabled {
            self.sync(false);
        }
    }
    pub fn new(scene: &Scene, root: &Viewport) -> Self {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let view: FeedbackText = glib::Object::new();
        view.set_widget_name("clivuu-inline-text");
        view.set_wrap_mode(gtk::WrapMode::None);
        view.set_accepts_tab(false);
        view.buffer().set_enable_undo(false);
        view.set_left_margin(0);
        view.set_right_margin(0);
        view.set_top_margin(0);
        view.set_bottom_margin(0);
        let class = format!("clivuu-inline-{}", SERIAL.fetch_add(1, Ordering::Relaxed));
        view.add_css_class(&class);
        let clip = Viewport::new();
        clip.set_overflow(gtk::Overflow::Hidden);
        clip.put(&view, 0., 0.);
        clip.set_visible(false);
        root.put(&clip, 0., 0.);
        let provider = gtk::CssProvider::new();
        gtk::style_context_add_provider_for_display(
            &view.display(),
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
        );
        let this = Self(Rc::new(Inner {
            scene: scene.downgrade(),
            root: root.downgrade(),
            clip,
            view,
            syncing: Cell::new(false),
            pending: Cell::new(false),
            scheduled: Cell::new(false),
            preedit: RefCell::new(String::new()),
            provider,
            class,
            css: RefCell::new(String::new()),
            active: Cell::new(false),
            enabled: Cell::new(true),
            before: RefCell::new(None),
            events: RefCell::new(None),
            origin: Cell::new([0., 0.]),
        }));
        let weak = Rc::downgrade(&this.0);
        scene.connect_text_focus(move || {
            if let Some(inner) = weak.upgrade()
                && inner.active.get()
            {
                inner.view.grab_focus();
            }
        });
        let weak = Rc::downgrade(&this.0);
        let hook = scene.controller().before_dispatch(move |_| {
            if let Some(inner) = weak.upgrade() {
                Self(inner).flush();
            }
        });
        this.0.before.replace(Some(hook));
        let weak = Rc::downgrade(&this.0);
        scene.connect_changed(move |effect| {
            if let Some(inner) = weak.upgrade() {
                Self(inner).sync(effect == Effect::BeginText);
            }
        });
        let weak = Rc::downgrade(&this.0);
        this.0.view.buffer().connect_changed(move |_| {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            if inner.syncing.get() {
                return;
            }
            inner.pending.set(true);
            if !inner.scheduled.replace(true) {
                let weak = Rc::downgrade(&inner);
                glib::idle_add_local_once(move || {
                    if let Some(inner) = weak.upgrade() {
                        inner.scheduled.set(false);
                        Self(inner).flush();
                    }
                });
            }
        });
        let weak = Rc::downgrade(&this.0);
        this.0.view.buffer().connect_end_user_action(move |_| {
            if let Some(inner) = weak.upgrade() {
                Self(inner).flush();
            }
        });
        let weak = Rc::downgrade(&this.0);
        this.0.view.connect_preedit_changed(move |_, text| {
            if let Some(inner) = weak.upgrade() {
                let this = Self(inner);
                *this.0.preedit.borrow_mut() = text.into();
                this.flush();
                this.preedit();
            }
        });
        let weak = Rc::downgrade(&this.0);
        this.0.view.buffer().connect_mark_set(move |_, _, _| {
            if let Some(inner) = weak.upgrade() {
                inner.view.imp().on.set(true);
                inner.view.queue_draw();
            }
        });
        let weak = Rc::downgrade(&this.0);
        glib::timeout_add_local(Duration::from_millis(500), move || {
            let Some(inner) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if inner.active.get() {
                inner.view.imp().on.set(!inner.view.imp().on.get());
                inner.view.queue_draw();
            }
            glib::ControlFlow::Continue
        });
        let scroll = gtk::EventControllerScroll::builder()
            .flags(gtk::EventControllerScrollFlags::VERTICAL)
            .propagation_phase(gtk::PropagationPhase::Capture)
            .build();
        let weak = this.0.scene.clone();
        scroll.connect_scroll(move |_, _, dy| {
            if let Some(scene) = weak.upgrade() {
                scene.dispatch(Action::Wheel(dy));
            }
            glib::Propagation::Stop
        });
        this.0.view.add_controller(scroll);
        let keys = gtk::EventControllerKey::builder()
            .propagation_phase(gtk::PropagationPhase::Capture)
            .build();
        let weak = Rc::downgrade(&this.0);
        keys.connect_key_pressed(move |_, key, _, state| {
            let Some(inner) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            let Some(scene) = inner.scene.upgrade() else {
                return glib::Propagation::Proceed;
            };
            let action = if state.contains(gdk::ModifierType::CONTROL_MASK) {
                match key {
                    gdk::Key::z | gdk::Key::Z => Some(Action::TextHistory {
                        redo: state.contains(gdk::ModifierType::SHIFT_MASK),
                    }),
                    gdk::Key::y | gdk::Key::Y => Some(Action::TextHistory { redo: true }),
                    _ => None,
                }
            } else if key == gdk::Key::Escape && inner.preedit.borrow().is_empty() {
                Some(Action::CommitText)
            } else {
                None
            };
            if let Some(action) = action {
                scene.dispatch(action);
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        this.0.view.add_controller(keys);
        let weak = Rc::downgrade(&this.0);
        this.0.clip.set_contains(move |x, y| {
            let Some(inner) = weak.upgrade() else {
                return false;
            };
            let Some(scene) = inner.scene.upgrade() else {
                return false;
            };
            let controller = scene.controller();
            let c = controller.borrow();
            let m = c.mapping();
            let p = m.point([x + m.display.x, y + m.display.y]);
            if c.session()
                .target(p, c.tool().kind(), m.screen_scale())
                .is_some_and(|t| t.text)
            {
                return false;
            }
            inner
                .view
                .compute_bounds(&inner.clip)
                .is_some_and(|b| b.contains_point(&graphene::Point::new(x as f32, y as f32)))
        });
        // Controls can sit outside the TextView's glyph rectangle. Route only
        // model-confirmed geometry hits before native text selection handles it.
        let events = gtk::EventControllerLegacy::builder()
            .propagation_phase(gtk::PropagationPhase::Capture)
            .build();
        let weak = Rc::downgrade(&this.0);
        events.connect_event(move |_, event| {
            let Some(inner) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            let Some(scene) = inner.scene.upgrade() else {
                return glib::Propagation::Proceed;
            };
            let Some((x, y)) = event.position() else {
                return glib::Propagation::Proceed;
            };
            let controller = scene.controller();
            let c = controller.borrow();
            if !c.session().is_text_editing() {
                return glib::Propagation::Proceed;
            }
            let m = c.mapping();
            let origin = inner.origin.get();
            let world = [x + origin[0], y + origin[1]];
            let p = m.point(world);
            let transforming = c.session().is_text_transforming();
            let target = c
                .session()
                .target(p, c.tool().kind(), m.screen_scale())
                .is_some_and(|t| t.text);
            drop(c);
            if event.event_type() == gdk::EventType::ButtonPress
                && event
                    .downcast_ref::<gdk::ButtonEvent>()
                    .is_some_and(|e| e.button() == 1)
                && target
            {
                if let Some(root) = inner.root.upgrade() {
                    let pick = root.pick(x, y, gtk::PickFlags::DEFAULT);
                    if pick.is_some_and(|w| {
                        !(w == root.clone().upcast::<gtk::Widget>()
                            || w == scene.clone().upcast::<gtk::Widget>()
                            || w == inner.clip.clone().upcast::<gtk::Widget>()
                            || w.is_ancestor(&inner.clip))
                    }) {
                        return glib::Propagation::Proceed;
                    }
                }
                scene.dispatch(Action::Begin {
                    point: world,
                    shift: event
                        .modifier_state()
                        .contains(gdk::ModifierType::SHIFT_MASK),
                });
                return glib::Propagation::Stop;
            }
            if transforming {
                match event.event_type() {
                    gdk::EventType::MotionNotify => {
                        scene.dispatch(Action::Move {
                            point: world,
                            shift: event
                                .modifier_state()
                                .contains(gdk::ModifierType::SHIFT_MASK),
                        });
                        return glib::Propagation::Stop;
                    }
                    gdk::EventType::ButtonRelease => {
                        scene.dispatch(Action::End);
                        inner.view.grab_focus();
                        return glib::Propagation::Stop;
                    }
                    _ => {}
                }
            }
            glib::Propagation::Proceed
        });
        root.add_controller(events.clone());
        this.0.events.replace(Some(events));
        this
    }
    pub fn set_surface_origin(&self, origin: [f64; 2]) {
        self.0.origin.set(origin);
        self.sync(false);
    }
    pub fn view(&self) -> &FeedbackText {
        &self.0.view
    }
    pub fn flush(&self) {
        if !self.0.pending.replace(false) {
            return;
        }
        let Some(scene) = self.0.scene.upgrade() else {
            return;
        };
        if !scene.controller().borrow().session().is_text_editing() {
            return;
        }
        let b = self.0.view.buffer();
        let value = b.text(&b.start_iter(), &b.end_iter(), true).to_string();
        scene.dispatch(Action::Text {
            value,
            time_ms: glib::monotonic_time() as u64 / 1000,
        });
    }
    fn preedit(&self) {
        let Some(scene) = self.0.scene.upgrade() else {
            return;
        };
        let controller = scene.controller();
        let c = controller.borrow();
        if !c.session().is_text_editing() {
            return;
        }
        let Some(mark) = c.session().current() else {
            return;
        };
        let mut mark = mark.clone();
        drop(c);
        let text = self.0.preedit.borrow();
        let size = if text.is_empty() {
            None
        } else {
            let b = self.0.view.buffer();
            let offset = b.iter_at_mark(&b.get_insert()).offset() as usize;
            let value = mark.text.take().unwrap_or_default();
            mark.text = Some(
                value
                    .chars()
                    .take(offset)
                    .chain(text.chars())
                    .chain(value.chars().skip(offset))
                    .collect(),
            );
            Some(text_metrics(&self.0.view.pango_context(), &mark, 1.))
        };
        scene.dispatch(Action::Preedit(size));
    }
    fn sync(&self, begin: bool) {
        let Some(scene) = self.0.scene.upgrade() else {
            return;
        };
        let controller = scene.controller();
        let c = controller.borrow();
        let editing = self.0.enabled.get() && c.session().is_text_editing();
        let state = editing.then(|| {
            (
                c.session().control_mark().unwrap().clone(),
                c.session().current().unwrap().clone(),
                c.mapping(),
            )
        });
        drop(c);
        let was = self.0.active.replace(editing);
        let had_focus = self.0.view.has_focus();
        self.0.clip.set_visible(editing);
        let Some((mark, current, m)) = state else {
            if was {
                self.0.preedit.borrow_mut().clear();
                if had_focus && self.0.enabled.get() {
                    scene.grab_focus();
                }
            }
            return;
        };
        let buffer = self.0.view.buffer();
        let value = current.text.as_deref().unwrap_or("");
        if !self.0.pending.get()
            && buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), true)
                .as_str()
                != value
        {
            self.0.syncing.set(true);
            buffer.set_text(value);
            buffer.place_cursor(&buffer.end_iter());
            self.0.syncing.set(false);
        }
        let family = serde_json::to_string(&mark.style.font_family).unwrap();
        let css = format!(
            ".{class}, .{class} text {{background:transparent;color:{color};caret-color:transparent;border:0;border-radius:0;box-shadow:none;padding:0;margin:0;font-family:{family};font-size:{pixels}px;font-weight:{weight};font-style:{slant};}}",
            class = self.0.class,
            color = if mark.preedit_frame {
                mark.style.color.as_str()
            } else {
                "transparent"
            },
            pixels = mark.text_pixels(1.),
            weight = if mark.style.bold { "bold" } else { "normal" },
            slant = if mark.style.italic {
                "italic"
            } else {
                "normal"
            }
        );
        if *self.0.css.borrow() != css {
            self.0.provider.load_from_string(&css);
            *self.0.css.borrow_mut() = css;
        }
        self.0.view.set_opacity(if mark.preedit_frame {
            mark.style.alpha as f64 / 255.
        } else {
            1.
        });
        let p = mark.map_point(mark.points[0], 1., false);
        let scale = m.screen_scale();
        let t = mark.transform();
        self.0.view.imp().scale.set(t.sx.abs() * scale);
        let size = mark.text_size.unwrap_or([1., mark.text_pixels(1.) * 1.3]);
        self.0.view.set_size_request(
            (size[0] + 4.).max(12.).ceil() as i32,
            size[1].max(mark.text_pixels(1.) * 1.3).ceil() as i32,
        );
        self.0.clip.set_size_request(
            m.display.width.ceil() as i32,
            m.display.height.ceil() as i32,
        );
        if let Some(root) = self.0.root.upgrade() {
            let origin = self.0.origin.get();
            root.move_(
                &self.0.clip,
                m.display.x - origin[0],
                m.display.y - origin[1],
            );
        }
        self.0.clip.set_child_transform(
            &self.0.view,
            Some(
                gsk::Transform::new()
                    .translate(&graphene::Point::new(
                        (p[0] * scale) as f32,
                        (p[1] * scale) as f32,
                    ))
                    .rotate((t.angle * 180. / std::f64::consts::PI) as f32)
                    .scale((t.sx * scale) as f32, (t.sy * scale) as f32),
            ),
        );
        if (begin || !was) && scene.has_focus() {
            self.0.view.imp().on.set(true);
            self.0.view.grab_focus();
            buffer.place_cursor(&buffer.end_iter());
        }
    }
}

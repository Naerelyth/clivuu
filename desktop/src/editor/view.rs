//! One editor presentation for both capture and pin sessions. Platform hosts
//! own native surfaces/focus/input shapes; this view only places shared widgets.
use super::{
    Effect,
    layout::{self, Panels, Size},
    scene::{Controller, Scene},
};
use crate::ui::{
    color::ColorServices,
    inline_text::InlineText,
    ocr_bar::OcrBar,
    popup::PopupLayer,
    subbar::Subbar,
    toolbar::{Output, Toolbar},
    viewport::Viewport,
};
use clivuu_core::{
    annotation::Bounds,
    i18n::{self, tf},
};
use gtk::{gdk, glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
struct Keyboard {
    enabled: Rc<Cell<bool>>,
    controller: gtk::EventControllerKey,
}
impl Keyboard {
    fn attach(&self, root: &Viewport) {
        let host = root
            .root()
            .and_downcast::<gtk::Window>()
            .map(|w| w.upcast::<gtk::Widget>())
            .unwrap_or_else(|| root.clone().upcast());
        if self.controller.widget().as_ref() == Some(&host) {
            return;
        }
        if let Some(previous) = self.controller.widget() {
            previous.remove_controller(&self.controller);
        }
        host.add_controller(self.controller.clone());
    }
}
impl Drop for Keyboard {
    fn drop(&mut self) {
        if let Some(host) = self.controller.widget() {
            host.remove_controller(&self.controller);
        }
    }
}
struct Motion {
    controller: gtk::EventControllerMotion,
}
impl Drop for Motion {
    fn drop(&mut self) {
        if let Some(host) = self.controller.widget() {
            host.remove_controller(&self.controller);
        }
    }
}
type HintCache = ([u32; 2], i18n::Language, i32, i32);

struct Inner {
    root: glib::WeakRef<Viewport>,
    _keyboard: Keyboard,
    _motion: Option<Motion>,
    scene: Scene,
    inline: InlineText,
    toolbar: Toolbar,
    options: Subbar,
    ocr_bar: OcrBar,
    popups: PopupLayer,
    hint: gtk::Label,
    viewport: Cell<Bounds>,
    origin: Cell<[f64; 2]>,
    selection: Cell<Bounds>,
    pointer: Cell<Option<[f64; 2]>>,
    capture_pixels: Cell<Option<[u32; 2]>>,
    last_hint: Cell<Option<HintCache>>,
    scene_size: Cell<Option<[f64; 2]>>,
    dirty_controls: Cell<bool>,
    toolbar_width: Cell<i32>,
    options_width: Cell<i32>,
    last_bar_state: Cell<Option<(bool, bool, i32, i18n::Language)>>,
    ready: Cell<bool>,
    enabled: Cell<bool>,
    hint_visible: Cell<bool>,
    toolbar_visible: Cell<bool>,
    toolbar_position: Rc<crate::ui::toolbar_drag::ToolbarPosition>,
    queued: Cell<bool>,
    defer_layout: Cell<bool>,
    layout: Cell<Option<Panels>>,
    options_bounds: Cell<Option<Bounds>>,
    ocr_bounds: Cell<Option<Bounds>>,
    changed: RefCell<Vec<Box<dyn Fn()>>>,
    pin: bool,
    mapped: RefCell<Option<glib::SignalHandlerId>>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.options.close_popups();
        self.popups.close();
        if let Some(root) = self.root.upgrade() {
            if let Some(signal) = self.mapped.take() {
                root.disconnect(signal);
            }
            for w in [
                self.scene.upcast_ref::<gtk::Widget>(),
                self.toolbar.widget().upcast_ref(),
                self.options.widget().upcast_ref(),
                self.ocr_bar.widget().upcast_ref(),
                self.hint.upcast_ref(),
            ] {
                if w.parent().is_some() {
                    root.remove(w);
                }
            }
        }
    }
}
#[derive(Clone)]
pub struct EditorView(Rc<Inner>);
impl EditorView {
    #[expect(
        clippy::too_many_arguments,
        reason = "Explicit host coordinates and shared document are required for both capture and pin views"
    )]
    pub fn new(
        root: &Viewport,
        document: Controller,
        texture: &gdk::Texture,
        viewport: Bounds,
        origin: [f64; 2],
        pin: bool,
        services: ColorServices,
        output: impl Fn(Output) + 'static,
    ) -> Self {
        let rect = document.borrow().mapping().display;
        let scene = Scene::new(document, texture);
        root.put(&scene, rect.x - origin[0], rect.y - origin[1]);
        let inline = InlineText::new(&scene, root);
        inline.set_surface_origin(origin);
        let toolbar = Toolbar::new(&scene, pin, output);
        let popups = PopupLayer::new(root);
        let options = Subbar::new(&scene, &popups, services);
        let ocr_bar = OcrBar::new(&popups, pin);
        let hint = gtk::Label::builder()
            .xalign(0.)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .can_target(false)
            .build();
        hint.add_css_class("clivuu-capture-hint");
        root.put(&hint, 0., 0.);
        root.put(toolbar.widget(), 0., 0.);
        root.put(options.widget(), 0., 0.);
        root.put(ocr_bar.widget(), 0., 0.);
        // Listen on the owning root as well as the canvas, matching the former
        // desktop host and its capture phase. A handled key stops propagation,
        // so Undo/output cannot run twice. Popups and editable controls retain
        // their native key semantics.
        let keys = gtk::EventControllerKey::builder()
            .propagation_phase(gtk::PropagationPhase::Capture)
            .build();
        let popup_guard = options.clone();
        let keyboard_enabled = Rc::new(Cell::new(true));
        let enabled = keyboard_enabled.clone();
        let weak = scene.downgrade();
        let weak_root = root.downgrade();
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            if !enabled.get() || popup_guard.has_popup() {
                return glib::Propagation::Proceed;
            }
            let Some(root) = weak_root.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if !root.is_mapped() || !root.is_sensitive() {
                return glib::Propagation::Proceed;
            }
            let focus = root
                .root()
                .and_downcast::<gtk::Window>()
                .and_then(|w| gtk::prelude::GtkWindowExt::focus(&w));
            let text_editing = weak
                .upgrade()
                .is_some_and(|scene| scene.controller().borrow().session().is_text_editing());
            let inline_command = focus.as_ref().is_some_and(|w| w.is::<gtk::TextView>())
                && super::shortcuts::while_typing(key, modifiers, pin)
                && text_editing;
            if !inline_command
                && focus.is_some_and(|w| {
                    (w.is::<gtk::TextView>() && text_editing)
                        || w.is::<gtk::Text>()
                        || w.is::<gtk::Entry>()
                        || w.is::<gtk::SpinButton>()
                })
            {
                return glib::Propagation::Proceed;
            }
            weak.upgrade().map_or(glib::Propagation::Proceed, |scene| {
                scene.key_pressed(key, modifiers)
            })
        });
        let enabled = keyboard_enabled.clone();
        let weak = scene.downgrade();
        keys.connect_key_released(move |_, key, _, _| {
            if enabled.get()
                && let Some(scene) = weak.upgrade()
            {
                scene.key_released(key);
            }
        });
        let keyboard = Keyboard {
            controller: keys,
            enabled: keyboard_enabled,
        };
        keyboard.attach(root);
        let this = Self(Rc::new(Inner {
            root: root.downgrade(),
            _keyboard: keyboard,
            _motion: (!pin).then(|| {
                let controller = gtk::EventControllerMotion::builder()
                    .propagation_phase(gtk::PropagationPhase::Capture)
                    .build();
                root.add_controller(controller.clone());
                Motion { controller }
            }),
            scene,
            inline,
            toolbar,
            options,
            ocr_bar,
            popups,
            hint,
            viewport: Cell::new(viewport),
            origin: Cell::new(origin),
            selection: Cell::new(rect),
            pointer: Cell::new(None),
            capture_pixels: Cell::new(None),
            last_hint: Cell::new(None),
            scene_size: Cell::new(None),
            dirty_controls: Cell::new(true),
            toolbar_width: Cell::new(0),
            options_width: Cell::new(0),
            last_bar_state: Cell::new(None),
            ready: Cell::new(true),
            enabled: Cell::new(true),
            hint_visible: Cell::new(true),
            toolbar_visible: Cell::new(true),
            toolbar_position: Rc::new(crate::ui::toolbar_drag::ToolbarPosition::default()),
            queued: Cell::new(false),
            defer_layout: Cell::new(false),
            layout: Cell::new(None),
            options_bounds: Cell::new(None),
            ocr_bounds: Cell::new(None),
            changed: RefCell::new(vec![]),
            pin,
            mapped: RefCell::new(None),
        }));
        if let Some(motion) = &this.0._motion {
            for connect_move in [false, true] {
                let weak = Rc::downgrade(&this.0);
                let update = move |x: f64, y: f64| {
                    if let Some(inner) = weak.upgrade() {
                        let origin = inner.origin.get();
                        let view = Self(inner);
                        view.0.pointer.set(Some([x + origin[0], y + origin[1]]));
                        view.update_hint_visibility();
                    }
                };
                if connect_move {
                    motion
                        .controller
                        .connect_motion(move |_, x, y| update(x, y));
                } else {
                    motion.controller.connect_enter(move |_, x, y| update(x, y));
                }
            }
            let weak = Rc::downgrade(&this.0);
            motion.controller.connect_leave(move |_| {
                if let Some(inner) = weak.upgrade() {
                    let view = Self(inner);
                    view.0.pointer.set(None);
                    view.update_hint_visibility();
                }
            });
        }
        let weak = Rc::downgrade(&this.0);
        this.0.scene.connect_changed(move |effect| {
            if effect != Effect::None
                && let Some(inner) = weak.upgrade()
            {
                inner.dirty_controls.set(true);
                Self(inner).queue_layout();
            }
        });
        let weak = Rc::downgrade(&this.0);
        let mapped = root.connect_map(move |root| {
            if let Some(inner) = weak.upgrade() {
                inner._keyboard.attach(root);
                inner.dirty_controls.set(true);
                Self(inner).queue_layout();
            }
        });
        this.0.mapped.replace(Some(mapped));
        let weak = Rc::downgrade(&this.0);
        this.0.popups.connect_changed(move || {
            if let Some(inner) = weak.upgrade() {
                for f in inner.changed.borrow().iter() {
                    f();
                }
            }
        });
        let weak = Rc::downgrade(&this.0);
        let changed = weak.clone();
        this.0.toolbar_position.install(
            this.0.toolbar.grip(),
            move || weak.upgrade()?.layout.get().map(|p| p.toolbar),
            move |_| {
                if let Some(inner) = changed.upgrade() {
                    Self(inner).queue_layout();
                }
            },
        );
        this.layout();
        this
    }
    pub fn input_rects(&self) -> Vec<Bounds> {
        let origin = self.0.origin.get();
        let mut rects = vec![];
        // Publish the layout used for the upcoming frame, not the previous
        // GTK allocation. Wayland commits its input region with that frame.
        if self.0.toolbar.widget().is_visible()
            && let Some(panels) = self.0.layout.get()
        {
            rects.push(panels.toolbar);
        }
        if self.0.options.widget().is_visible()
            && let Some(rect) = self.0.options_bounds.get()
        {
            rects.push(rect);
        }
        if self.0.ocr_bar.widget().is_visible()
            && let Some(rect) = self.0.ocr_bounds.get()
        {
            rects.push(rect);
        }
        if let Some(r) = self.0.popups.bounds() {
            rects.push(Bounds {
                x: r.x() as f64 + origin[0],
                y: r.y() as f64 + origin[1],
                width: r.width() as f64,
                height: r.height() as f64,
            });
        }
        let doc = self.0.scene.controller();
        let c = doc.borrow();
        let mapping = c.mapping();
        if self.0.scene.is_visible() && self.0.scene.can_target() {
            if c.is_polyline_drawing() {
                rects.push(self.0.viewport.get());
            } else if c.tool() == super::Tool::Crop {
                let m = super::controller::CROP_MARGIN;
                rects.push(Bounds {
                    x: mapping.display.x - m,
                    y: mapping.display.y - m,
                    width: mapping.display.width + 2. * m,
                    height: mapping.display.height + 2. * m,
                });
            }
        }
        if let Some(mark) = c
            .session()
            .control_mark()
            .filter(|_| self.0.scene.is_visible() && self.0.scene.can_target())
        {
            for h in mark.handles(1., mapping.screen_scale()) {
                let x = mapping.display.x + h.point[0] * mapping.screen_scale();
                let y =
                    mapping.display.y + h.point[1] * mapping.display.height / mapping.document[1];
                rects.push(Bounds {
                    x: x - 9.,
                    y: y - 9.,
                    width: 18.,
                    height: 18.,
                });
            }
        }
        rects
    }
    pub fn set_keyboard_enabled(&self, enabled: bool) {
        self.0._keyboard.enabled.set(enabled);
    }
    pub fn scene(&self) -> &Scene {
        &self.0.scene
    }
    pub fn inline(&self) -> &InlineText {
        &self.0.inline
    }
    pub fn toolbar(&self) -> &Toolbar {
        &self.0.toolbar
    }
    pub fn options(&self) -> &Subbar {
        &self.0.options
    }
    pub fn ocr_bar(&self) -> &OcrBar {
        &self.0.ocr_bar
    }
    pub fn refresh_layout(&self) {
        self.0.dirty_controls.set(true);
        self.queue_layout();
    }
    pub fn panels(&self) -> Option<Panels> {
        self.0.layout.get()
    }
    pub fn hint(&self) -> &gtk::Label {
        &self.0.hint
    }
    pub fn set_hint_visible(&self, visible: bool) {
        if self.0.hint_visible.replace(visible) != visible {
            self.queue_layout();
        }
    }
    pub fn set_theme(&self, theme: &crate::ui::toolbar::Theme) {
        for w in [
            self.0.toolbar.widget().upcast_ref::<gtk::Widget>(),
            self.0.options.widget().upcast_ref(),
            self.0.ocr_bar.widget().upcast_ref(),
        ] {
            if theme.is_dark() {
                w.add_css_class("clivuu-theme-dark");
            } else {
                w.remove_css_class("clivuu-theme-dark");
            }
        }
        self.0.scene.set_decoration_style(theme.decoration_style());
        self.0.toolbar.widget().queue_draw();
        self.0.options.widget().queue_draw();
        self.0.hint.queue_draw();
        self.0.dirty_controls.set(true);
        self.queue_layout();
    }
    pub fn set_selection(&self, rect: Bounds, ready: bool) {
        let changed = self.0.selection.replace(rect) != rect;
        let ready_changed = self.0.ready.replace(ready) != ready;
        if ready_changed {
            self.0.dirty_controls.set(true);
        }
        if ready_changed || changed {
            self.queue_layout();
        }
    }
    pub fn set_capture_pixels(&self, size: Option<[u32; 2]>) {
        if self.0.capture_pixels.replace(size) != size {
            self.queue_layout();
        }
    }
    pub fn set_output(&self, viewport: Bounds, origin: [f64; 2]) {
        let prev = self.0.viewport.replace(viewport);
        let changed = prev != viewport;
        if prev.width.floor() != viewport.width.floor() {
            self.0.dirty_controls.set(true);
        }
        if self.0.origin.replace(origin) != origin {
            self.0.inline.set_surface_origin(origin);
            self.queue_layout();
        } else if changed {
            self.queue_layout();
        }
    }
    pub fn set_toolbar_visible(&self, visible: bool) {
        if self.0.toolbar_visible.replace(visible) != visible {
            if visible {
                self.0.dirty_controls.set(true);
            }
            self.queue_layout();
        }
    }
    pub fn set_controls_visible(&self, visible: bool) {
        if self.0.enabled.replace(visible) == visible {
            return;
        }
        if !visible {
            self.0.options.close_popups();
            self.0.popups.close();
        } else {
            self.0.dirty_controls.set(true);
        }
        self.queue_layout();
    }
    pub fn connect_layout_changed(&self, callback: impl Fn() + 'static) {
        self.0.changed.borrow_mut().push(Box::new(callback));
    }
    pub fn set_defer_layout(&self, defer: bool) {
        self.0.defer_layout.set(defer);
    }
    fn queue_layout(&self) {
        if self.0.defer_layout.get() || self.0.queued.replace(true) {
            return;
        }
        let Some(root) = self.0.root.upgrade() else {
            self.0.queued.set(false);
            return;
        };
        let weak = Rc::downgrade(&self.0);
        root.add_tick_callback(move |_, _| {
            if let Some(inner) = weak.upgrade()
                && inner.queued.replace(false)
            {
                Self(inner).layout();
            }
            glib::ControlFlow::Break
        });
    }
    fn update_hint_visibility(&self) {
        let hovered = self.0.ready.get()
            && self.0.pointer.get().is_some_and(|p| {
                self.0
                    .layout
                    .get()
                    .is_some_and(|panels| panels.hint.contains(p, 0.))
            });
        let show = !self.0.pin && self.0.enabled.get() && self.0.hint_visible.get() && !hovered;
        if self.0.hint.is_visible() != show {
            self.0.hint.set_visible(show);
        }
    }
    pub fn layout(&self) {
        self.0.queued.set(false);
        let Some(root) = self.0.root.upgrade() else {
            return;
        };
        let viewport = self.0.viewport.get();
        let origin = self.0.origin.get();
        let document = self.0.scene.controller();
        let c = document.borrow();
        let m = c.mapping();
        let visible = self.0.toolbar_visible.get() && self.0.enabled.get() && self.0.ready.get();
        let ocr_visible = self.0.ocr_bar.widget().is_visible();
        let options = visible && !ocr_visible && c.appearance().is_some();
        let ocr_active_bar = !options && visible && ocr_visible;
        let rect = self.0.selection.get();
        drop(c);
        root.move_(
            &self.0.scene,
            m.display.x - origin[0],
            m.display.y - origin[1],
        );
        let size = [m.display.width, m.display.height];
        if self.0.scene_size.replace(Some(size)) != Some(size) {
            self.0.scene.queue_resize();
        }
        self.0.toolbar.widget().set_visible(visible);
        let want_hint = !self.0.pin && self.0.enabled.get() && self.0.hint_visible.get();
        let lang = i18n::current_language();
        let out_w = viewport.width.floor() as i32;
        let bar_state = (ocr_visible, options, out_w, lang);
        let refresh_bars = self.0.dirty_controls.replace(false)
            || self.0.last_bar_state.replace(Some(bar_state)) != Some(bar_state);
        // A hidden or purely translated bar needs no re-measurement or button
        // rebuild. Touching size requests on every pointer tick stalls motion.
        let w = if visible {
            if refresh_bars || self.0.toolbar_width.get() <= 0 {
                self.0.toolbar.refresh();
                self.0.toolbar.set_output_width(out_w);
                let measured = self
                    .0
                    .toolbar
                    .widget()
                    .measure(gtk::Orientation::Horizontal, -1)
                    .1;
                self.0.toolbar_width.set(measured);
                measured
            } else {
                self.0.toolbar_width.get()
            }
        } else {
            self.0.layout.get().map_or(0, |p| p.toolbar.width as i32)
        };
        let subbar = if options {
            self.0.options.widget().set_visible(true);
            let ow = if refresh_bars || self.0.options_width.get() <= 0 {
                self.0.options.refresh();
                let measured = self
                    .0
                    .options
                    .sync_output_width((viewport.width as i32 - 10).max(1));
                self.0.options_width.set(measured);
                measured
            } else {
                self.0.options_width.get()
            };
            Some(Size {
                width: ow as f64,
                height: 32.,
            })
        } else {
            self.0.options.widget().set_visible(false);
            if ocr_active_bar {
                self.0
                    .ocr_bar
                    .set_max_content_width((viewport.width as i32 - 10).max(1));
                let (_, ow, _, _) = self
                    .0
                    .ocr_bar
                    .widget()
                    .measure(gtk::Orientation::Horizontal, -1);
                Some(Size {
                    width: ow as f64,
                    height: 32.,
                })
            } else {
                None
            }
        };
        let pixels = self.0.capture_pixels.get().unwrap_or([
            (rect.width * m.document[0] / m.display.width).round() as u32,
            (rect.height * m.document[1] / m.display.height).round() as u32,
        ]);
        let (hw, hh) = match self.0.last_hint.get() {
            Some((cached_px, cached_lang, cw, ch))
                if cached_px == pixels && cached_lang == lang =>
            {
                (cw, ch)
            }
            _ => {
                let pixel_width = pixels[0].to_string();
                let pixel_height = pixels[1].to_string();
                self.0
                    .hint
                    .set_label(&tf("{0} × {1} px", &[&pixel_width, &pixel_height]));
                if want_hint && !self.0.hint.is_visible() {
                    self.0.hint.set_visible(true);
                }
                let (_, hw, _, _) = self.0.hint.measure(gtk::Orientation::Horizontal, -1);
                let (_, hh, _, _) = self.0.hint.measure(gtk::Orientation::Vertical, hw);
                self.0.last_hint.set(Some((pixels, lang, hw, hh)));
                (hw, hh)
            }
        };
        let mut panels = layout::selection_with_subbar(
            rect,
            viewport,
            Size {
                width: w as f64,
                height: 32.,
            },
            Size {
                width: hw as f64,
                height: hh as f64,
            },
            subbar,
        );
        panels.toolbar = self.0.toolbar_position.place(panels.toolbar, viewport);
        if visible {
            root.move_(
                self.0.toolbar.widget(),
                panels.toolbar.x - origin[0],
                panels.toolbar.y - origin[1],
            );
            root.set_child_size(
                self.0.toolbar.widget(),
                Some((panels.toolbar.width.ceil() as i32, 32)),
            );
        }
        if want_hint {
            root.move_(
                &self.0.hint,
                panels.hint.x - origin[0],
                panels.hint.y - origin[1],
            );
            root.set_child_size(
                &self.0.hint,
                Some((
                    panels.hint.width.ceil() as i32,
                    panels.hint.height.ceil() as i32,
                )),
            );
        }
        let subbar_placement = if self.0.toolbar_position.is_custom() {
            layout::Placement::Below
        } else {
            panels.placement
        };
        let subbar_bounds =
            subbar.map(|s| layout::annotation_for(subbar_placement, panels.toolbar, viewport, s));
        if let Some(p) = subbar_bounds.filter(|_| options) {
            root.move_(self.0.options.widget(), p.x - origin[0], p.y - origin[1]);
            root.set_child_size(self.0.options.widget(), Some((p.width.ceil() as i32, 32)));
            self.0.options_bounds.set(Some(p));
        } else {
            self.0.options_bounds.set(None);
        }
        if let Some(p) = subbar_bounds.filter(|_| ocr_active_bar) {
            root.move_(self.0.ocr_bar.widget(), p.x - origin[0], p.y - origin[1]);
            root.set_child_size(self.0.ocr_bar.widget(), Some((p.width.ceil() as i32, 32)));
            self.0.ocr_bounds.set(Some(p));
        } else {
            self.0.ocr_bounds.set(None);
        }
        self.0.layout.set(Some(panels));
        self.update_hint_visibility();
        self.0.popups.set_viewport(Bounds {
            x: viewport.x - origin[0],
            y: viewport.y - origin[1],
            width: viewport.width,
            height: viewport.height,
        });
        self.0.popups.reposition();
        self.0.toolbar.widget().set_visible(visible);
        for f in self.0.changed.borrow().iter() {
            f();
        }
    }
}

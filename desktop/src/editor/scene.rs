use super::decorations::{self, DecorationStyle};
use super::mosaics::Mosaics;
use super::render::{AnnotationNodes, mosaic_spec};
use super::{Action, EditorController, Effect};
use clivuu_core::annotation::Kind;
use gtk::{gdk, glib, graphene, prelude::*, subclass::prelude::*};
use std::{
    cell::{Cell, Ref, RefCell},
    rc::{Rc, Weak},
};
/// All output views observe the same local dispatcher. A pointer event updates
/// every view without IPC, and no view can bypass notifications via borrow_mut.
type BeforeDispatch = (Weak<()>, Rc<dyn Fn(&Action)>);
pub struct EditorDocument {
    model: RefCell<EditorController>,
    views: RefCell<Vec<glib::WeakRef<Scene>>>,
    before: RefCell<Vec<BeforeDispatch>>,
    mosaics: Mosaics,
}
#[must_use = "Retain this subscription for the lifetime of its input adapter"]
pub struct DispatchHook {
    _alive: Rc<()>,
}
pub type Controller = Rc<EditorDocument>;
impl EditorDocument {
    pub fn new(controller: EditorController) -> Controller {
        Rc::new(Self {
            model: RefCell::new(controller),
            views: RefCell::new(vec![]),
            before: RefCell::new(vec![]),
            mosaics: Mosaics::default(),
        })
    }
    pub fn borrow(&self) -> Ref<'_, EditorController> {
        self.model.borrow()
    }
    pub fn checkpoint(&self) -> EditorController {
        self.model.borrow().clone()
    }
    pub fn restore_checkpoint(&self, mut saved: EditorController) {
        saved.set_mapping(self.model.borrow().mapping());
        self.model.replace(saved);
        self.refresh_mosaics();
        let views: Vec<_> = self
            .views
            .borrow()
            .iter()
            .filter_map(glib::WeakRef::upgrade)
            .collect();
        for scene in views {
            scene.notify_changed(Effect::Committed);
        }
    }
    pub fn dispatch(&self, action: Action) -> Effect {
        if !matches!(&action, Action::Text { .. } | Action::Preedit(_)) {
            let callbacks: Vec<_> = {
                let mut before = self.before.borrow_mut();
                before.retain(|(alive, _)| alive.strong_count() > 0);
                before
                    .iter()
                    .map(|(_, callback)| callback.clone())
                    .collect()
            };
            for f in callbacks {
                f(&action);
            }
        }
        let effect = self.model.borrow_mut().dispatch(action);
        self.refresh_mosaics();
        let views: Vec<_> = self
            .views
            .borrow()
            .iter()
            .filter_map(glib::WeakRef::upgrade)
            .collect();
        self.views.borrow_mut().retain(|v| v.upgrade().is_some());
        for scene in views {
            scene.notify_changed(effect);
        }
        effect
    }
    pub fn set_mapping(&self, mapping: super::ImageMapping) {
        let prev = self.model.borrow().mapping();
        if prev == mapping {
            return;
        }
        let resized = prev.display.width != mapping.display.width
            || prev.display.height != mapping.display.height
            || prev.document != mapping.document;
        self.model.borrow_mut().set_mapping(mapping);
        for view in self.views.borrow().iter().filter_map(|v| v.upgrade()) {
            if resized {
                view.queue_resize();
                view.notify_changed(Effect::Preview);
            } else if !view.imp().hide_decorations.get() && view.imp().pointer.get().is_some() {
                view.queue_draw();
            }
        }
    }
    pub fn before_dispatch(&self, callback: impl Fn(&Action) + 'static) -> DispatchHook {
        let alive = Rc::new(());
        let mut before = self.before.borrow_mut();
        before.retain(|(alive, _)| alive.strong_count() > 0);
        before.push((Rc::downgrade(&alive), Rc::new(callback)));
        DispatchHook { _alive: alive }
    }
    pub fn set_source(
        self: &Rc<Self>,
        image: clivuu_core::image::RgbaImage,
        pool: Rc<crate::infrastructure::tasks::TaskPool>,
    ) {
        self.set_shared_source(std::sync::Arc::new(image), pool);
    }
    pub fn set_shared_source(
        self: &Rc<Self>,
        image: std::sync::Arc<clivuu_core::image::RgbaImage>,
        pool: Rc<crate::infrastructure::tasks::TaskPool>,
    ) {
        let weak = Rc::downgrade(self);
        self.mosaics.set_shared_source(image, pool, move || {
            if let Some(document) = weak.upgrade() {
                for scene in document
                    .views
                    .borrow()
                    .iter()
                    .filter_map(glib::WeakRef::upgrade)
                {
                    scene.imp().nodes.borrow_mut().invalidate_mosaics();
                    scene.imp().baked_annotations.take();
                    scene.queue_draw();
                }
            }
        });
        self.refresh_mosaics();
    }
    pub fn mosaics(&self) -> &Mosaics {
        &self.mosaics
    }
    fn refresh_mosaics(&self) {
        let model = self.model.borrow();
        self.mosaics
            .warm((model.tool() == super::Tool::Mosaic).then(|| {
                model
                    .drawing()
                    .or_else(|| {
                        model
                            .session()
                            .current()
                            .filter(|mark| mark.kind == clivuu_core::annotation::Kind::Mosaic)
                    })
                    .unwrap_or_else(|| model.session().style(clivuu_core::annotation::Kind::Mosaic))
                    .style
                    .mosaic_effect()
            }));
        self.mosaics.request(
            model
                .session()
                .displayed()
                .chain(model.drawing())
                .filter_map(|mark| mosaic_spec(mark, self.mosaics.dimensions())),
        );
    }
}
type Changed = Box<dyn Fn(Effect)>;
/// Selection geometry is capture-specific; annotation input below remains
/// exactly the same for capture and pin views. Returning true consumes input.
pub enum Input {
    Secondary {
        point: [f64; 2],
    },
    Begin {
        point: [f64; 2],
        shift: bool,
        clicks: i32,
    },
    Move {
        point: [f64; 2],
        shift: bool,
    },
    Hover {
        point: [f64; 2],
        pressed: bool,
    },
    End,
    Wheel(f64),
    Key(gdk::Key, gdk::ModifierType),
    KeyReleased(gdk::Key),
}
type InputRoute = Rc<dyn Fn(&Input) -> bool>;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discarded_input_views_do_not_accumulate_dispatch_hooks() {
        let document = EditorDocument::new(EditorController::new(super::super::ImageMapping {
            display: clivuu_core::annotation::Bounds {
                x: 0.,
                y: 0.,
                width: 100.,
                height: 100.,
            },
            document: [100., 100.],
        }));
        let calls = Rc::new(Cell::new(0));
        for _ in 0..2000 {
            let counter = calls.clone();
            let hook = document.before_dispatch(move |_| counter.set(counter.get() + 1));
            assert_eq!(document.before.borrow().len(), 1);
            drop(hook);
        }
        document.dispatch(Action::Undo);
        assert_eq!(calls.get(), 0);
        assert!(document.before.borrow().is_empty());
        let counter = calls.clone();
        let _hook = document.before_dispatch(move |_| counter.set(counter.get() + 1));
        document.dispatch(Action::Undo);
        assert_eq!(calls.get(), 1);
    }
}

mod imp {
    use super::*;
    pub type CaptureOverlay = Box<dyn Fn(&gtk::Snapshot)>;
    #[derive(Default)]
    pub struct Scene {
        pub filter: Cell<super::super::filter::Filter>,
        pub controller: RefCell<Option<Controller>>,
        pub texture: RefCell<Option<gdk::Texture>>,
        pub nodes: RefCell<AnnotationNodes>,
        pub baked_annotations: RefCell<Option<(u64, [u32; 2], gdk::Texture)>>,
        pub pointer: Cell<Option<[f64; 2]>>,
        pub pointer_owned: Cell<bool>,
        pub last_cursor: Cell<Option<&'static str>>,
        pub tool_cursor: RefCell<Option<super::super::cursor::ToolCursor>>,
        pub sequence_cursor:
            RefCell<Option<(clivuu_core::annotation::Style, gtk::gsk::RenderNode)>>,
        pub changed: RefCell<Vec<Changed>>,
        pub text_focus: RefCell<Vec<Box<dyn Fn()>>>,
        pub pressed: Cell<bool>,
        pub held_shifts: Cell<u8>,
        pub decoration: RefCell<DecorationStyle>,
        pub route: RefCell<Option<InputRoute>>,
        pub hide_source: Cell<bool>,
        pub nearest_source: Cell<bool>,
        pub hide_decorations: Cell<bool>,
        pub capture_overlay: RefCell<Option<CaptureOverlay>>,
    }
    #[glib::object_subclass]
    impl ObjectSubclass for Scene {
        const NAME: &'static str = "ClivuuEditorScene";
        type Type = super::Scene;
        type ParentType = gtk::Widget;
    }
    impl ObjectImpl for Scene {}
    impl WidgetImpl for Scene {
        fn unrealize(&self) {
            self.baked_annotations.take();
            self.parent_unrealize();
        }
        fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let controller = self.controller.borrow();
            let Some(controller) = controller.as_ref() else {
                return (0, 0, -1, -1);
            };
            let m = controller.borrow().mapping();
            let size = if orientation == gtk::Orientation::Horizontal {
                m.display.width
            } else {
                m.display.height
            };
            (0, size.ceil() as i32, -1, -1)
        }
        fn contains(&self, x: f64, y: f64) -> bool {
            if self.parent_contains(x, y) {
                return true;
            }
            if self.hide_decorations.get() {
                return false;
            }
            let controller = self.controller.borrow();
            let Some(controller) = controller.as_ref() else {
                return false;
            };
            let c = controller.borrow();
            let m = c.mapping();
            if c.is_polyline_drawing()
                || c.crop_contains_display([x + m.display.x, y + m.display.y])
            {
                return true;
            }
            let p = [
                x * m.document[0] / m.display.width,
                y * m.document[1] / m.display.height,
            ];
            c.session().control_mark().is_some_and(|mark| {
                mark.nearest_handle(p, 1., m.screen_scale(), 9. / m.screen_scale())
                    .is_some()
            })
        }
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let controller = self.controller.borrow();
            let Some(controller) = controller.as_ref() else {
                return;
            };
            let c = controller.borrow();
            let m = c.mapping();
            snapshot.save();
            snapshot.scale(
                (m.display.width / m.document[0]) as f32,
                (m.display.height / m.document[1]) as f32,
            );
            snapshot.push_clip(&graphene::Rect::new(
                0.,
                0.,
                m.document[0] as f32,
                m.document[1] as f32,
            ));
            let filtered = self.filter.get().push(snapshot);
            if let Some(texture) = self
                .texture
                .borrow()
                .as_ref()
                .filter(|_| !self.hide_source.get())
            {
                crate::editor::render::source_texture(
                    snapshot,
                    texture,
                    &graphene::Rect::new(0., 0., m.document[0] as f32, m.document[1] as f32),
                    !self.nearest_source.get(),
                );
            }
            let marks = c.session().displayed();
            let node = self.nodes.borrow_mut().compose_preview(
                marks,
                c.drawing(),
                &self.obj().pango_context(),
                &controller.mosaics.textures(),
            );
            if let Some(node) = node {
                let (has_eraser, revision) = {
                    let nodes = self.nodes.borrow();
                    (nodes.has_eraser(), nodes.revision())
                };
                let doc_w = m.document[0].round() as u32;
                let doc_h = m.document[1].round() as u32;
                let can_bake = has_eraser
                    && c.drawing().is_none()
                    && !c.session().is_transforming()
                    && doc_w > 0
                    && doc_h > 0
                    && u64::from(doc_w) * u64::from(doc_h) <= clivuu_core::image_ops::MAX_PIXELS
                    && m.display.width <= m.document[0] * 1.01
                    && m.display.height <= m.document[1] * 1.01;
                if !has_eraser {
                    self.baked_annotations.take();
                }
                let baked = if can_bake {
                    let existing = self
                        .baked_annotations
                        .borrow()
                        .as_ref()
                        .filter(|(rev, size, _)| *rev == revision && *size == [doc_w, doc_h])
                        .map(|(_, _, tex)| tex.clone());
                    if let Some(tex) = existing {
                        Some(tex)
                    } else if let Some(renderer) =
                        self.obj().native().and_then(|native| native.renderer())
                    {
                        let tex = renderer.render_texture(
                            &node,
                            Some(&graphene::Rect::new(
                                0.,
                                0.,
                                m.document[0] as f32,
                                m.document[1] as f32,
                            )),
                        );
                        self.baked_annotations.replace(Some((
                            revision,
                            [doc_w, doc_h],
                            tex.clone(),
                        )));
                        Some(tex)
                    } else {
                        None
                    }
                } else {
                    None
                };
                if let Some(tex) = baked {
                    crate::editor::render::source_texture(
                        snapshot,
                        &tex,
                        &graphene::Rect::new(0., 0., m.document[0] as f32, m.document[1] as f32),
                        !self.nearest_source.get(),
                    );
                } else {
                    snapshot.append_node(node);
                }
            } else {
                self.baked_annotations.take();
            }
            if filtered {
                snapshot.pop();
            }
            snapshot.pop();
            snapshot.restore();
            if self.hide_decorations.get() {
                return;
            }
            if let Some(overlay) = self.capture_overlay.borrow().as_ref() {
                overlay(snapshot);
            }
            snapshot.save();
            snapshot.scale(
                (m.display.width / m.document[0]) as f32,
                (m.display.height / m.document[1]) as f32,
            );
            let scale = m.screen_scale();
            let pointer = self.pointer.get().map(|p| m.point(p));
            if let Some(mark) = c.session().control_mark() {
                decorations::selected(snapshot, mark, pointer, scale, &self.decoration.borrow());
            }
            if let Some(mark) = pointer.and_then(|p| c.session().hover(p, c.tool().kind(), scale)) {
                decorations::hover(snapshot, mark, scale);
            }
            if let Some(mark) = c.drawing() {
                decorations::draft(snapshot, mark, scale);
            }
            if let Some(pointer) = pointer
                && let Some(cursor) = self.tool_cursor.borrow().as_ref()
            {
                cursor.snapshot(snapshot, pointer, scale);
            }
            if self.pointer_owned.get()
                && self
                    .pointer
                    .get()
                    .is_some_and(|p| c.sequence_preview(p).is_some())
                && let Some(pointer) = pointer
                && let Some((_, node)) = self.sequence_cursor.borrow().as_ref()
            {
                snapshot.save();
                snapshot.translate(&graphene::Point::new(pointer[0] as f32, pointer[1] as f32));
                snapshot.append_node(node);
                snapshot.restore();
            }
            snapshot.restore();
            if c.tool() == super::super::Tool::Crop {
                let dc = c.display_crop();
                decorations::crop(
                    snapshot,
                    clivuu_core::annotation::Bounds {
                        x: dc.x - m.display.x,
                        y: dc.y - m.display.y,
                        width: dc.width,
                        height: dc.height,
                    },
                    [m.display.width, m.display.height],
                    &self.decoration.borrow(),
                );
            }
        }
    }
}
glib::wrapper! {pub struct Scene(ObjectSubclass<imp::Scene>) @extends gtk::Widget,@implements gtk::Accessible,gtk::Buildable,gtk::ConstraintTarget;}
impl Scene {
    pub fn new(controller: Controller, texture: &gdk::Texture) -> Self {
        let scene: Self = glib::Object::new();
        scene.set_focusable(true);
        scene.set_overflow(gtk::Overflow::Visible);
        scene.imp().controller.replace(Some(controller));
        scene
            .controller()
            .views
            .borrow_mut()
            .push(scene.downgrade());
        scene.imp().texture.replace(Some(texture.clone()));
        scene.input();
        scene
    }
    pub fn controller(&self) -> Controller {
        self.imp().controller.borrow().as_ref().unwrap().clone()
    }
    pub fn replace_texture(&self, texture: &gdk::Texture) {
        self.imp().texture.replace(Some(texture.clone()));
        self.imp().baked_annotations.take();
        self.queue_draw();
    }
    pub fn source_texture(&self) -> Option<gdk::Texture> {
        self.imp().texture.borrow().clone()
    }
    pub fn set_input_route(&self, route: impl Fn(&Input) -> bool + 'static) {
        self.imp().route.replace(Some(Rc::new(route)));
    }
    fn route(&self, input: Input) -> bool {
        let route = self.imp().route.borrow().clone();
        route.is_some_and(|route| route(&input))
    }
    pub fn set_named_cursor(&self, name: Option<&'static str>) {
        if self.imp().last_cursor.replace(name) != name {
            self.set_cursor_from_name(name);
        }
    }
    pub fn set_decorations_visible(&self, visible: bool) {
        if self.imp().hide_decorations.replace(!visible) != visible {
            return;
        }
        if !visible {
            self.imp().tool_cursor.borrow_mut().take();
            self.set_named_cursor(None);
        }
        self.queue_draw();
    }
    pub fn set_smooth(&self, smooth: bool) {
        if self.imp().nearest_source.replace(!smooth) == smooth {
            self.queue_draw();
        }
    }
    pub fn filter(&self) -> super::filter::Filter {
        self.imp().filter.get()
    }
    pub fn set_filter(&self, filter: super::filter::Filter) {
        if self.imp().filter.replace(filter) != filter {
            self.queue_draw();
        }
    }
    pub fn set_source_visible(&self, visible: bool) {
        self.imp().hide_source.set(!visible);
        self.queue_draw();
    }
    pub fn set_capture_overlay(&self, paint: impl Fn(&gtk::Snapshot) + 'static) {
        self.imp().capture_overlay.replace(Some(Box::new(paint)));
        self.queue_draw();
    }
    pub fn set_decoration_style(&self, style: DecorationStyle) {
        self.imp().decoration.replace(style);
        self.queue_draw();
    }
    pub fn dispatch(&self, action: Action) -> Effect {
        self.controller().dispatch(action)
    }
    fn notify_changed(&self, effect: Effect) {
        if self.imp().pointer_owned.get() {
            self.refresh_cursor();
        }
        self.queue_draw();
        for changed in self.imp().changed.borrow().iter() {
            changed(effect);
        }
    }
    pub fn connect_changed(&self, changed: impl Fn(Effect) + 'static) {
        self.imp().changed.borrow_mut().push(Box::new(changed));
    }
    pub fn connect_text_focus(&self, callback: impl Fn() + 'static) {
        self.imp().text_focus.borrow_mut().push(Box::new(callback));
    }
    pub fn focus_text_input(&self) {
        for f in self.imp().text_focus.borrow().iter() {
            f();
        }
    }
    fn point(&self, x: f64, y: f64) -> [f64; 2] {
        let m = self.controller().borrow().mapping();
        [x + m.display.x, y + m.display.y]
    }
    fn end(&self) {
        if self.imp().pressed.replace(false) {
            if self.route(Input::End) {
                return;
            }
            self.dispatch(Action::End);
        }
    }
    fn input(&self) {
        let secondary = gtk::GestureClick::builder().button(3).build();
        let weak = self.downgrade();
        secondary.connect_pressed(move |gesture, _, x, y| {
            if let Some(scene) = weak.upgrade() {
                scene.dispatch(Action::CommitText);
                let polyline = scene.controller().borrow().is_polyline_drawing();
                let selected = scene.controller().borrow().session().current().is_some();
                let consumed = if polyline {
                    scene.dispatch(Action::FinishPolyline);
                    true
                } else if selected {
                    scene.dispatch(Action::Deselect);
                    true
                } else {
                    scene.route(Input::Secondary {
                        point: scene.point(x, y),
                    })
                };
                if consumed {
                    scene.imp().pressed.set(false);
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                }
            }
        });
        self.add_controller(secondary);
        let click = gtk::GestureClick::builder().button(1).build();
        let weak = self.downgrade();
        click.connect_pressed(move |gesture, count, x, y| {
            if let Some(scene) = weak.upgrade() {
                scene.imp().pressed.set(true);
                scene.grab_focus();
                let point = scene.point(x, y);
                let shift = gesture
                    .current_event_state()
                    .contains(gdk::ModifierType::SHIFT_MASK);
                if !shift {
                    scene.imp().held_shifts.set(0);
                }
                if count >= 2 && scene.controller().borrow().is_polyline_drawing() {
                    scene.imp().pressed.set(false);
                    scene.dispatch(Action::FinishPolyline);
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                    return;
                }
                if scene.route(Input::Begin {
                    point,
                    shift,
                    clicks: count,
                }) {
                    return;
                }
                if count == 2
                    && scene.dispatch(Action::EditText {
                        point: scene.point(x, y),
                    }) == Effect::BeginText
                {
                    return;
                }
                scene.dispatch(Action::Begin {
                    point: scene.point(x, y),
                    shift,
                });
            }
        });
        let weak = self.downgrade();
        click.connect_released(move |_, _, _, _| {
            if let Some(scene) = weak.upgrade() {
                scene.end();
            }
        });
        let drag = gtk::GestureDrag::builder().button(1).build();
        let weak = self.downgrade();
        drag.connect_drag_update(move |gesture, dx, dy| {
            if let Some(scene) = weak.upgrade()
                && scene.imp().pressed.get()
                && let Some((x, y)) = gesture.start_point()
            {
                let point = scene.point(x + dx, y + dy);
                scene.imp().pointer.set(Some(point));
                let shift = gesture
                    .current_event_state()
                    .contains(gdk::ModifierType::SHIFT_MASK);
                if scene.route(Input::Move { point, shift }) {
                    return;
                }
                scene.dispatch(Action::Move { point, shift });
            }
        });
        let weak = self.downgrade();
        drag.connect_drag_end(move |_, _, _| {
            if let Some(scene) = weak.upgrade() {
                scene.end();
            }
        });
        click.group_with(&drag);
        self.add_controller(click);
        self.add_controller(drag);
        let motion = gtk::EventControllerMotion::new();
        let weak = self.downgrade();
        motion.connect_motion(move |ctrl, x, y| {
            if let Some(scene) = weak.upgrade() {
                scene.track_pointer(
                    x,
                    y,
                    ctrl.current_event_state()
                        .contains(gdk::ModifierType::SHIFT_MASK),
                );
            }
        });
        let weak = self.downgrade();
        motion.connect_enter(move |ctrl, x, y| {
            if let Some(scene) = weak.upgrade() {
                scene.track_pointer(
                    x,
                    y,
                    ctrl.current_event_state()
                        .contains(gdk::ModifierType::SHIFT_MASK),
                );
            }
        });
        let weak = self.downgrade();
        motion.connect_leave(move |_| {
            if let Some(scene) = weak.upgrade() {
                scene.imp().pointer.set(None);
                let was_owned = scene.imp().pointer_owned.replace(false);
                let had_cursor = scene.imp().tool_cursor.borrow_mut().take().is_some();
                scene.set_named_cursor(None);
                if was_owned || had_cursor {
                    scene.queue_draw();
                }
            }
        });
        self.add_controller(motion);
        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        let weak = self.downgrade();
        scroll.connect_scroll(move |_, _, dy| {
            if let Some(scene) = weak.upgrade() {
                if scene.route(Input::Wheel(dy)) {
                    return glib::Propagation::Stop;
                }
                scene.dispatch(Action::Wheel(dy));
            }
            glib::Propagation::Stop
        });
        self.add_controller(scroll);
        let keys = gtk::EventControllerKey::new();
        let weak = self.downgrade();
        keys.connect_key_pressed(move |_, key, _, state| {
            weak.upgrade().map_or(glib::Propagation::Proceed, |scene| {
                scene.key_pressed(key, state)
            })
        });
        let weak = self.downgrade();
        keys.connect_key_released(move |_, key, _, _| {
            if let Some(scene) = weak.upgrade() {
                scene.key_released(key);
            }
        });
        self.add_controller(keys);
    }
    /// Shared keyboard semantics for the canvas and its owning editor root.
    pub fn key_pressed(&self, key: gdk::Key, state: gdk::ModifierType) -> glib::Propagation {
        let scene = self;
        let bit = shift_bit(key);
        if bit != 0 {
            self.imp()
                .held_shifts
                .set(self.imp().held_shifts.get() | bit);
            if self.constraint_modifier(true) {
                return glib::Propagation::Stop;
            }
        }
        if self.controller().borrow().is_polyline_drawing() {
            let m = super::shortcuts::modifiers(state);
            if m.is_empty() && key == gdk::Key::Escape {
                self.imp().pressed.set(false);
                self.dispatch(Action::CancelPolyline);
                return glib::Propagation::Stop;
            }
            if m.is_empty() && matches!(key, gdk::Key::Return | gdk::Key::KP_Enter) {
                self.imp().pressed.set(false);
                self.dispatch(Action::FinishPolyline);
                return glib::Propagation::Stop;
            }
        }
        if scene.route(Input::Key(key, state)) {
            return glib::Propagation::Stop;
        }
        let tool = self.controller().borrow().tool();
        let action = super::shortcuts::edit(key, state, tool);
        if let Some(action) = action {
            scene.dispatch(action);
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    }
    pub fn key_released(&self, key: gdk::Key) {
        self.route(Input::KeyReleased(key));
        let bit = shift_bit(key);
        if bit != 0 {
            let held = self.imp().held_shifts.get() & !bit;
            self.imp().held_shifts.set(held);
            self.constraint_modifier(held != 0);
        }
    }
    fn constraint_modifier(&self, shift: bool) -> bool {
        let controller = self.controller();
        let c = controller.borrow();
        let polyline = c.is_polyline_drawing();
        if !self.imp().pressed.get() && !polyline {
            return false;
        }
        let constrained = polyline
            || c.drawing().is_some_and(|m| {
                matches!(
                    m.kind,
                    Kind::Arrow | Kind::Sequence | Kind::Rect | Kind::Ellipse
                ) || (m.kind == Kind::Highlight && !m.freehand())
            })
            || c.session().is_transforming()
                && c.session().current().is_some_and(|m| !m.freehand());
        drop(c);
        let Some(point) = self.imp().pointer.get().filter(|_| constrained) else {
            return false;
        };
        if !polyline && self.route(Input::Move { point, shift }) {
            return true;
        }
        self.dispatch(Action::Move { point, shift });
        true
    }
    fn track_pointer(&self, x: f64, y: f64, shift: bool) {
        let p = self.point(x, y);
        let pressed = self.imp().pressed.get();
        self.imp().pointer.set(Some(p));
        if !pressed && self.controller().borrow().is_polyline_drawing() {
            let shift = shift || self.imp().held_shifts.get() != 0;
            self.dispatch(Action::Move { point: p, shift });
        }
        let owned = !self.route(Input::Hover { point: p, pressed });
        let was_owned = self.imp().pointer_owned.replace(owned);
        if owned {
            if !pressed {
                self.refresh_cursor();
                self.queue_draw();
            }
        } else if self.imp().tool_cursor.borrow_mut().take().is_some() || was_owned {
            self.queue_draw();
        }
    }
    fn refresh_cursor(&self) {
        if self.imp().hide_decorations.get() {
            return;
        }
        let Some(point) = self.imp().pointer.get() else {
            return;
        };
        let controller = self.controller();
        let editor = controller.borrow();
        let cursor = super::cursor::ToolCursor::for_editor(&editor, point);
        let sequence = editor.sequence_preview(point);
        if let Some(mark) = sequence {
            let mut cache = self.imp().sequence_cursor.borrow_mut();
            if cache.as_ref().is_none_or(|(style, _)| *style != mark.style) {
                let mut preview = clivuu_core::annotation::Mark::new(Kind::Sequence, [0., 0.], 1.);
                preview.style = mark.style.clone();
                let snapshot = gtk::Snapshot::new();
                super::render::sequence_badge(&snapshot, &preview, &self.pango_context());
                *cache = snapshot.to_node().map(|node| (mark.style.clone(), node));
            }
        }
        let name = if cursor.is_some() || sequence.is_some() {
            "none"
        } else {
            editor.cursor(point).unwrap_or("crosshair")
        };
        self.set_named_cursor(Some(name));
        self.imp().tool_cursor.replace(cursor);
    }
}

fn shift_bit(key: gdk::Key) -> u8 {
    if key == gdk::Key::Shift_L {
        1
    } else if key == gdk::Key::Shift_R {
        2
    } else {
        0
    }
}

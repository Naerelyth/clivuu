//! A stationary, zero-minimum canvas. Unlike GtkFixedLayout, child extents
//! cannot enlarge a desktop host. Moving a child changes allocation only.
use gtk::{glib, graphene, gsk, prelude::*, subclass::prelude::*};
use std::cell::RefCell;
#[derive(Clone)]
struct Placement {
    widget: gtk::Widget,
    x: f64,
    y: f64,
    transform: Option<gsk::Transform>,
    size: Option<(i32, i32)>,
}
fn is_gtk8339_translate(dx: f32, dy: f32) -> bool {
    dx.is_finite() && dy.is_finite() && dx == dx.floor() && dy != dy.floor() && (dx - 0.5) != dx
}
fn gtk8339_translate_offset(t: &gsk::Transform) -> Option<(f32, f32)> {
    (t.category() == gsk::TransformCategory::_2dTranslate)
        .then(|| t.to_translate())
        .filter(|&(dx, dy)| is_gtk8339_translate(dx, dy))
}
fn needs_gtk8339_split(child: &Placement) -> bool {
    if let Some(extra) = &child.transform {
        let t = gsk::Transform::new()
            .translate(&graphene::Point::new(child.x as f32, child.y as f32))
            .transform(Some(extra));
        gtk8339_translate_offset(&t).is_some()
    } else {
        is_gtk8339_translate(child.x as f32, child.y as f32)
    }
}
/// Work around GTK #8339 (`gsk_transform_node_diff` checks `dy != floorf(dy)` on
/// `2D_TRANSLATE` nodes and enters the integer `cairo_region_translate` path,
/// dropping 1px horizontal strips during partial redraws). Splitting `(dx, dy)`
/// into two `2D_TRANSLATE` nodes forces both onto GSK's exact `2D_AFFINE` diff
/// path while keeping the GPU renderer's CPU offset stack intact (no FBO).
fn split_gtk8339_node(node: gsk::RenderNode) -> gsk::RenderNode {
    if let Ok(tn) = node.clone().downcast::<gsk::TransformNode>()
        && let Some((dx, dy)) = gtk8339_translate_offset(&tn.transform())
    {
        let inner_t = gsk::Transform::new().translate(&graphene::Point::new(dx - 0.5, dy));
        let outer_t = gsk::Transform::new().translate(&graphene::Point::new(0.5, 0.0));
        return gsk::TransformNode::new(gsk::TransformNode::new(tn.child(), &inner_t), &outer_t)
            .upcast();
    }
    node
}
type HitTest = Box<dyn Fn(f64, f64) -> bool>;
mod imp {
    use super::*;
    #[derive(Default)]
    pub struct Viewport {
        pub(super) children: RefCell<Vec<Placement>>,
        pub clip: RefCell<Option<graphene::Rect>>,
        pub contains: RefCell<Option<HitTest>>,
    }
    #[glib::object_subclass]
    impl ObjectSubclass for Viewport {
        const NAME: &'static str = "ClivuuViewport";
        type Type = super::Viewport;
        type ParentType = gtk::Widget;
    }
    impl ObjectImpl for Viewport {
        fn dispose(&self) {
            self.children.borrow_mut().clear();
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }
    impl WidgetImpl for Viewport {
        fn realize(&self) {
            self.parent_realize();
            // GTK stops cursor inheritance at an implicit button grab. Keep
            // a surface fallback as well, so an unset button cursor cannot
            // inherit a differently scaled X11 root cursor while pressed.
            // Device/widget cursors (brushes, text, resize) still override it.
            super::super::native_cursor::apply_surface(&*self.obj());
        }
        fn measure(&self, _: gtk::Orientation, _: i32) -> (i32, i32, i32, i32) {
            (0, 0, -1, -1)
        }
        fn size_allocate(&self, _: i32, _: i32, _: i32) {
            for child in self.children.borrow().iter() {
                if !child.widget.should_layout() {
                    continue;
                }
                let (w, h) = if let Some((w, h)) = child.size {
                    let (min_w, _, _, _) = child.widget.measure(gtk::Orientation::Horizontal, -1);
                    let w = w.max(min_w);
                    let (min_h, _, _, _) = child.widget.measure(gtk::Orientation::Vertical, w);
                    (w, h.max(min_h))
                } else {
                    let (min, w, _, _) = child.widget.measure(gtk::Orientation::Horizontal, -1);
                    let w = min.max(w);
                    let (min, h, _, _) = child.widget.measure(gtk::Orientation::Vertical, w);
                    (w, min.max(h))
                };
                let mut t = gsk::Transform::new()
                    .translate(&graphene::Point::new(child.x as f32, child.y as f32));
                if let Some(extra) = &child.transform {
                    t = t.transform(Some(extra));
                }
                child.widget.allocate(w, h, -1, Some(t));
            }
            // GtkPopover is a native child: its parent must present it during
            // allocation, rather than treating it as an ordinary canvas item.
            let mut child = self.obj().first_child();
            while let Some(widget) = child {
                child = widget.next_sibling();
                if let Some(popover) = widget.downcast_ref::<gtk::Popover>()
                    && popover.is_visible()
                {
                    popover.present();
                }
            }
        }
        fn snapshot(&self, s: &gtk::Snapshot) {
            let clip = *self.clip.borrow();
            if let Some(r) = clip {
                s.push_clip(&r);
            }
            for child in self.children.borrow().iter() {
                if child.widget.is_visible() && super::needs_gtk8339_split(child) {
                    let sub = gtk::Snapshot::new();
                    self.obj().snapshot_child(&child.widget, &sub);
                    if let Some(node) = sub.to_node() {
                        s.append_node(super::split_gtk8339_node(node));
                    }
                } else {
                    self.obj().snapshot_child(&child.widget, s);
                }
            }
            if clip.is_some() {
                s.pop();
            }
        }
        fn contains(&self, x: f64, y: f64) -> bool {
            self.contains
                .borrow()
                .as_ref()
                .map_or_else(|| self.parent_contains(x, y), |f| f(x, y))
        }
    }
}
glib::wrapper! {pub struct Viewport(ObjectSubclass<imp::Viewport>) @extends gtk::Widget,@implements gtk::Accessible,gtk::Buildable,gtk::ConstraintTarget;}
impl Default for Viewport {
    fn default() -> Self {
        Self::new()
    }
}
impl Viewport {
    pub fn new() -> Self {
        let this: Self = glib::Object::new();
        this.set_overflow(gtk::Overflow::Visible);
        // Unset cursors inherit the X11 root cursor, whose size can differ
        // from GDK's themed cursors on another monitor. Child tools override it.
        this.set_cursor_from_name(Some("default"));
        this
    }
    pub fn put(&self, child: &impl IsA<gtk::Widget>, x: f64, y: f64) {
        child.as_ref().set_parent(self);
        self.imp().children.borrow_mut().push(Placement {
            widget: child.as_ref().clone(),
            x,
            y,
            transform: None,
            size: None,
        });
        self.queue_allocate();
    }
    /// Keep both GTK picking and snapshot order consistent, leaving controls on top.
    pub fn order_background(&self, widgets: &[gtk::Widget]) {
        let changed = {
            let mut children = self.imp().children.borrow_mut();
            if children.len() >= widgets.len()
                && children
                    .iter()
                    .zip(widgets.iter())
                    .all(|(p, w)| p.widget == *w)
            {
                return;
            }
            let mut ordered = Vec::with_capacity(children.len());
            for w in widgets {
                if let Some(p) = children.iter().find(|p| p.widget == *w) {
                    ordered.push(p.clone());
                }
            }
            ordered.extend(
                children
                    .iter()
                    .filter(|p| !widgets.contains(&p.widget))
                    .cloned(),
            );
            let changed = children
                .iter()
                .map(|p| &p.widget)
                .ne(ordered.iter().map(|p| &p.widget));
            if changed {
                *children = ordered;
            }
            changed
        };
        if changed {
            let children: Vec<_> = self
                .imp()
                .children
                .borrow()
                .iter()
                .map(|p| p.widget.clone())
                .collect();
            let mut previous: Option<gtk::Widget> = None;
            for child in children {
                child.insert_after(self, previous.as_ref());
                previous = Some(child);
            }
            self.queue_draw();
        }
    }
    pub fn remove(&self, child: &impl IsA<gtk::Widget>) {
        self.imp()
            .children
            .borrow_mut()
            .retain(|p| p.widget != *child.as_ref());
        child.as_ref().unparent();
        self.queue_allocate();
    }
    pub fn move_(&self, child: &impl IsA<gtk::Widget>, x: f64, y: f64) {
        if let Some(p) = self
            .imp()
            .children
            .borrow_mut()
            .iter_mut()
            .find(|p| p.widget == *child.as_ref())
        {
            if p.x == x && p.y == y {
                return;
            }
            p.x = x;
            p.y = y;
        }
        self.queue_allocate();
    }
    pub fn set_child_transform(
        &self,
        child: &impl IsA<gtk::Widget>,
        transform: Option<gsk::Transform>,
    ) {
        if let Some(p) = self
            .imp()
            .children
            .borrow_mut()
            .iter_mut()
            .find(|p| p.widget == *child.as_ref())
        {
            p.transform = transform;
        }
        self.queue_allocate();
    }
    pub fn set_clip(&self, clip: Option<graphene::Rect>) {
        self.imp().clip.replace(clip);
        self.queue_draw();
    }
    /// Unlike set_size_request, this sets the actual allocation, not its minimum.
    pub fn set_child_size(&self, child: &impl IsA<gtk::Widget>, size: Option<(i32, i32)>) {
        let size = size.map(|(w, h)| (w.max(1), h.max(1)));
        if let Some(p) = self
            .imp()
            .children
            .borrow_mut()
            .iter_mut()
            .find(|p| p.widget == *child.as_ref())
        {
            if p.size == size {
                return;
            }
            p.size = size;
        }
        self.queue_allocate();
    }
    pub fn set_contains(&self, contains: impl Fn(f64, f64) -> bool + 'static) {
        self.imp().contains.replace(Some(Box::new(contains)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gtk8339_guard_matches_only_integer_x_fractional_y_2d_translate() {
        assert!(is_gtk8339_translate(60.0, 60.5));
        assert!(is_gtk8339_translate(0.0, 60.75));
        assert!(is_gtk8339_translate(-40.0, -0.25));
        assert!(!is_gtk8339_translate(60.0, 60.0));
        assert!(!is_gtk8339_translate(60.5, 60.5));
        assert!(!is_gtk8339_translate(60.5, 60.0));
        assert!(!is_gtk8339_translate(f32::NAN, 60.5));
        assert!(!is_gtk8339_translate(1e10, 60.5));

        let affected = gsk::Transform::new().translate(&graphene::Point::new(60.0, 60.5));
        assert_eq!(gtk8339_translate_offset(&affected), Some((60.0, 60.5)));

        let both_frac = gsk::Transform::new().translate(&graphene::Point::new(60.5, 60.5));
        assert_eq!(gtk8339_translate_offset(&both_frac), None);

        let both_int = gsk::Transform::new().translate(&graphene::Point::new(60.0, 60.0));
        assert_eq!(gtk8339_translate_offset(&both_int), None);

        let rotated = affected.clone().rotate(15.0);
        assert_eq!(gtk8339_translate_offset(&rotated), None);

        let scaled = affected.scale(2.0, 2.0);
        assert_eq!(gtk8339_translate_offset(&scaled), None);
    }
}

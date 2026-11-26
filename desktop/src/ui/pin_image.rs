//! Retained image + measured eight-tile glow; allocations depend on radius,
//! never on desktop or image area. Moving the widget reuses all GPU nodes.
use gtk::{gdk, glib, graphene, gsk, prelude::*, subclass::prelude::*};
use std::cell::{Cell, RefCell};
pub const RADIUS: f64 = 12.;
const PROFILE: [f64; 12] = [96., 70., 49., 32., 20., 13., 7., 3., 0., 0., 0., 0.];
fn coverage(p: i32, length: i32) -> f64 {
    if p < 0 {
        PROFILE.get((-p - 1) as usize).copied().unwrap_or(0.) / 255.
    } else if p >= length {
        PROFILE.get((p - length) as usize).copied().unwrap_or(0.) / 255.
    } else {
        1. - PROFILE
            .get(p.min(length - 1 - p) as usize)
            .copied()
            .unwrap_or(0.)
            / 255.
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct RenderKey {
    width_bits: u64,
    height_bits: u64,
    rgba: [u32; 4],
    image_visible: bool,
    shadow: bool,
    smooth: bool,
    grayscale: bool,
    invert: bool,
}
type ShadowKey = (i32, i32, [u8; 4]);
type ShadowSet = [Option<gdk::Texture>; 8];
thread_local! {
    static SHADOW_CACHE: RefCell<Vec<(ShadowKey, ShadowSet)>> = const { RefCell::new(Vec::new()) };
}
fn shadow_tiles(width: i32, height: i32, color: &gdk::RGBA) -> ShadowSet {
    let rgba = [
        (color.red() * 255.).round() as u8,
        (color.green() * 255.).round() as u8,
        (color.blue() * 255.).round() as u8,
        (color.alpha() * 255.).round() as u8,
    ];
    let key_w = width.min(25);
    let key_h = height.min(25);
    let key = (key_w, key_h, rgba);
    if let Some(found) = SHADOW_CACHE.with(|cache| {
        cache
            .borrow()
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, tiles)| tiles.clone())
    }) {
        return found;
    }
    let ix = 12.min(key_w / 2);
    let iy = 12.min(key_h / 2);
    let cw = 12 + ix;
    let ch = 12 + iy;
    let specs = [
        (-12, -12, cw, ch, key_w > 0 && key_h > 0),
        (key_w - ix, -12, cw, ch, key_w > 0 && key_h > 0),
        (-12, key_h - iy, cw, ch, key_w > 0 && key_h > 0),
        (key_w - ix, key_h - iy, cw, ch, key_w > 0 && key_h > 0),
        (ix, -12, 1, 12, key_w - ix * 2 > 0),
        (ix, key_h, 1, 12, key_w - ix * 2 > 0),
        (-12, iy, 12, 1, key_h - iy * 2 > 0),
        (key_w, iy, 12, 1, key_h - iy * 2 > 0),
    ];
    let tiles: ShadowSet = std::array::from_fn(|idx| {
        let (ox, oy, w, h, active) = specs[idx];
        if !active || w <= 0 || h <= 0 {
            return None;
        }
        let mut pixels = vec![0u8; w as usize * h as usize * 4];
        for y in 0..h {
            for x in 0..w {
                let px = ox + x;
                let py = oy + y;
                if px >= 0 && px < key_w && py >= 0 && py < key_h {
                    continue;
                }
                let i = ((y * w + x) * 4) as usize;
                pixels[i..i + 3].copy_from_slice(&rgba[..3]);
                pixels[i + 3] =
                    (f64::from(rgba[3]) * coverage(px, key_w) * coverage(py, key_h)).round() as u8;
            }
        }
        Some(
            gdk::MemoryTexture::new(
                w,
                h,
                gdk::MemoryFormat::R8g8b8a8,
                &glib::Bytes::from_owned(pixels),
                w as usize * 4,
            )
            .upcast(),
        )
    });
    SHADOW_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() >= 12 {
            cache.remove(0);
        }
        cache.push((key, tiles.clone()));
    });
    tiles
}
mod imp {
    use super::*;
    #[derive(Default)]
    pub struct PinImage {
        pub node: RefCell<Option<gsk::RenderNode>>,
        pub(super) key: Cell<Option<RenderKey>>,
        pub source: RefCell<Option<gdk::Texture>>,
        pub shadow: Cell<bool>,
        pub filter: Cell<crate::editor::filter::Filter>,
        pub smooth: Cell<bool>,
        pub flash: RefCell<Option<gdk::RGBA>>,
        pub size: Cell<[f32; 2]>,
    }
    #[glib::object_subclass]
    impl ObjectSubclass for PinImage {
        const NAME: &'static str = "ClivuuPinImage";
        type Type = super::PinImage;
        type ParentType = gtk::Widget;
    }
    impl ObjectImpl for PinImage {}
    impl WidgetImpl for PinImage {
        fn measure(&self, _: gtk::Orientation, _: i32) -> (i32, i32, i32, i32) {
            (0, 0, -1, -1)
        }
        fn snapshot(&self, s: &gtk::Snapshot) {
            if let Some(node) = self.node.borrow().as_ref() {
                s.append_node(node);
            }
            if let Some(color) = self.flash.borrow().as_ref() {
                let [w, h] = self.size.get();
                let bounds = graphene::Rect::new(11., 11., w + 2., h + 2.);
                s.append_border(
                    &gsk::RoundedRect::from_rect(bounds, 0.),
                    &[2.; 4],
                    &[*color; 4],
                );
            }
        }
    }
}
glib::wrapper! {pub struct PinImage(ObjectSubclass<imp::PinImage>) @extends gtk::Widget,@implements gtk::Accessible,gtk::Buildable,gtk::ConstraintTarget;}
impl PinImage {
    pub fn new(texture: &gdk::Texture) -> Self {
        let this: Self = glib::Object::new();
        this.set_can_target(false);
        this.set_overflow(gtk::Overflow::Visible);
        this.imp().source.replace(Some(texture.clone()));
        this.configure(true, true);
        this
    }
    pub fn set_filter(&self, filter: crate::editor::filter::Filter) {
        if self.imp().filter.replace(filter) != filter {
            self.imp().key.set(None);
        }
    }
    pub fn source(&self, texture: &gdk::Texture) {
        self.imp().source.replace(Some(texture.clone()));
        self.imp().key.set(None);
    }
    pub fn configure(&self, shadow: bool, smooth: bool) {
        if self.imp().shadow.replace(shadow) != shadow || self.imp().smooth.get() != smooth {
            self.imp().key.set(None);
        }
        self.imp().smooth.set(smooth);
    }
    pub fn flash(&self, color: Option<gdk::RGBA>) {
        if *self.imp().flash.borrow() != color {
            self.imp().flash.replace(color);
            self.queue_draw();
        }
    }
    pub fn update(
        &self,
        image_width: f64,
        image_height: f64,
        color: &gdk::RGBA,
        image_visible: bool,
    ) {
        let filter = self.imp().filter.get();
        let shadow = self.imp().shadow.get();
        let smooth = self.imp().smooth.get();
        let key = RenderKey {
            width_bits: image_width.to_bits(),
            height_bits: image_height.to_bits(),
            rgba: [
                color.red().to_bits(),
                color.green().to_bits(),
                color.blue().to_bits(),
                color.alpha().to_bits(),
            ],
            image_visible,
            shadow,
            smooth,
            grayscale: filter.grayscale,
            invert: filter.invert,
        };
        if self.imp().key.get() == Some(key) {
            return;
        }
        self.imp().key.set(Some(key));
        self.imp()
            .size
            .set([image_width as f32, image_height as f32]);
        let snapshot = gtk::Snapshot::new();
        // Shadow tile allocation is integer-sized; image bounds retain fractions
        // so odd pixel dimensions stay exact on HiDPI outputs.
        let width = image_width.ceil() as i32;
        let height = image_height.ceil() as i32;
        if shadow && width > 0 && height > 0 {
            let ix = 12.min(width / 2);
            let iy = 12.min(height / 2);
            let cw = 12 + ix;
            let ch = 12 + iy;
            let layouts = [
                (-12, -12, cw, ch, 1, 1),
                (width - ix, -12, cw, ch, 1, 1),
                (-12, height - iy, cw, ch, 1, 1),
                (width - ix, height - iy, cw, ch, 1, 1),
                (ix, -12, 1, 12, width - ix * 2, 1),
                (ix, height, 1, 12, width - ix * 2, 1),
                (-12, iy, 12, 1, 1, height - iy * 2),
                (width, iy, 12, 1, 1, height - iy * 2),
            ];
            let tiles = shadow_tiles(width, height, color);
            let dx = image_width - f64::from(width);
            let dy = image_height - f64::from(height);
            for ((ox, oy, w, h, sx, sy), tile) in layouts.into_iter().zip(tiles) {
                if sx <= 0 || sy <= 0 {
                    continue;
                }
                if let Some(texture) = tile {
                    snapshot.append_texture(
                        &texture,
                        &graphene::Rect::new(
                            ((ox + 12) as f64 + if ox >= width - ix { dx } else { 0. }) as f32,
                            ((oy + 12) as f64 + if oy >= height - iy { dy } else { 0. }) as f32,
                            ((w * sx) as f64 + if w == 1 && ox == ix { dx } else { 0. }) as f32,
                            ((h * sy) as f64 + if h == 1 && oy == iy { dy } else { 0. }) as f32,
                        ),
                    );
                }
            }
        }
        if image_visible && let Some(source) = self.imp().source.borrow().as_ref() {
            let filtered = filter.push(&snapshot);
            crate::editor::render::source_texture(
                &snapshot,
                source,
                &graphene::Rect::new(12., 12., image_width as f32, image_height as f32),
                smooth,
            );
            if filtered {
                snapshot.pop();
            }
        }
        self.imp().node.replace(snapshot.to_node());
        self.queue_draw();
    }
}

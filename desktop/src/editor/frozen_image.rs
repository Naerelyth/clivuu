//! Frozen pixels are shared by texture upload, pixel sampling and effects.
//! There is no PNG encode/decode cycle when opening a capture or a pin editor.
use clivuu_core::image::RgbaImage;
use gtk::{gdk, glib, prelude::*};
use std::sync::Arc;
#[derive(Clone)]
pub struct FrozenImage {
    pub pixels: Arc<RgbaImage>,
    pub texture: gdk::Texture,
}
struct SharedBytes(Arc<RgbaImage>);
impl AsRef<[u8]> for SharedBytes {
    fn as_ref(&self) -> &[u8] {
        self.0.as_raw()
    }
}
impl FrozenImage {
    pub fn new(image: RgbaImage) -> Option<Self> {
        Self::from_shared(Arc::new(image))
    }
    pub fn from_shared(pixels: Arc<RgbaImage>) -> Option<Self> {
        let image = &pixels;
        let width = image.width();
        let height = image.height();
        if width == 0
            || height == 0
            || width > i32::MAX as u32
            || height > i32::MAX as u32
            || u64::from(width) * u64::from(height) > clivuu_core::image_ops::MAX_PIXELS
        {
            return None;
        }
        let texture = gdk::MemoryTexture::new(
            width as i32,
            height as i32,
            gdk::MemoryFormat::R8g8b8a8,
            &glib::Bytes::from_owned(SharedBytes(pixels.clone())),
            width as usize * 4,
        )
        .upcast();
        Some(Self { pixels, texture })
    }
    pub fn dimensions(&self) -> [u32; 2] {
        [self.pixels.width(), self.pixels.height()]
    }
}

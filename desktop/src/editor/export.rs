//! Export reuses the exact annotation nodes shown by Scene. GPU snapshotting
//! stays on GTK's thread; the immutable, Send+Sync GdkTexture may be downloaded
//! and encoded on the bounded pool. No widget or Pango context leaves GTK.
use super::{
    Action,
    render::{AnnotationNodes, mosaic_spec},
    scene::Scene,
};
use crate::infrastructure::tasks::{TaskError, TaskPool};
use clivuu_core::annotation::Bounds;
use gtk::{gio, glib, graphene, gsk, prelude::*};
use std::{rc::Rc, time::Duration};
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Cancelled,
    InvalidBounds,
    MissingSource,
    EffectFailed(String),
    Worker(TaskError),
}
struct CancelGuard {
    cancellable: gio::Cancellable,
    handler: Option<gio::CancelledHandlerId>,
}
impl Drop for CancelGuard {
    fn drop(&mut self) {
        if let Some(handler) = self.handler.take() {
            self.cancellable.disconnect_cancelled(handler);
        }
    }
}

pub async fn frame(
    scene: &Scene,
    renderer: &gsk::Renderer,
    crop: Option<Bounds>,
    pool: &Rc<TaskPool>,
    cancel: &gio::Cancellable,
) -> Result<super::frozen_image::FrozenImage, Error> {
    frame_filtered(scene, renderer, crop, None, pool, cancel, true, || {}).await
}
/// Render the source crop and annotations directly at the selected outputs'
/// native density, avoiding a full-resolution download followed by CPU scaling.
pub async fn frame_sized(
    scene: &Scene,
    renderer: &gsk::Renderer,
    crop: Option<Bounds>,
    size: [u32; 2],
    pool: &Rc<TaskPool>,
    cancel: &gio::Cancellable,
) -> Result<super::frozen_image::FrozenImage, Error> {
    frame_filtered(scene, renderer, crop, Some(size), pool, cancel, true, || {}).await
}
pub async fn frame_sized_then(
    scene: &Scene,
    renderer: &gsk::Renderer,
    crop: Option<Bounds>,
    size: [u32; 2],
    pool: &Rc<TaskPool>,
    cancel: &gio::Cancellable,
    on_rendered: impl FnOnce(),
) -> Result<super::frozen_image::FrozenImage, Error> {
    frame_filtered(
        scene,
        renderer,
        crop,
        Some(size),
        pool,
        cancel,
        true,
        on_rendered,
    )
    .await
}
pub async fn frame_unfiltered(
    scene: &Scene,
    renderer: &gsk::Renderer,
    crop: Option<Bounds>,
    pool: &Rc<TaskPool>,
    cancel: &gio::Cancellable,
) -> Result<super::frozen_image::FrozenImage, Error> {
    frame_filtered(scene, renderer, crop, None, pool, cancel, false, || {}).await
}
#[expect(
    clippy::too_many_arguments,
    reason = "Internal export helper shares crop, target size, filter policy and post-render hook"
)]
async fn frame_filtered(
    scene: &Scene,
    renderer: &gsk::Renderer,
    crop: Option<Bounds>,
    size: Option<[u32; 2]>,
    pool: &Rc<TaskPool>,
    cancel: &gio::Cancellable,
    filtered: bool,
    on_rendered: impl FnOnce(),
) -> Result<super::frozen_image::FrozenImage, Error> {
    let filter = if filtered {
        scene.filter()
    } else {
        super::filter::Filter::default()
    };
    if cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }
    scene.dispatch(Action::CommitText);
    scene.dispatch(Action::FinishPolyline);
    let document = scene.controller();
    let source = scene.source_texture().ok_or(Error::MissingSource)?;
    let (dimensions, marks) = {
        let model = document.borrow();
        let marks: Vec<_> = model
            .session()
            .displayed()
            .chain(model.drawing())
            .cloned()
            .collect();
        (model.mapping().document, marks)
    };
    let effect_dimensions = if document.mosaics().dimensions().contains(&0) {
        dimensions.map(|v| v as u32)
    } else {
        document.mosaics().dimensions()
    };
    let wanted: Vec<_> = marks
        .iter()
        .filter_map(|mark| mosaic_spec(mark, effect_dimensions))
        .collect();
    if !wanted.is_empty() && document.mosaics().dimensions().contains(&0) {
        return Err(Error::MissingSource);
    }
    let effects = if !wanted.is_empty() {
        Some(
            document
                .mosaics()
                .fork(wanted)
                .ok_or(Error::MissingSource)?,
        )
    } else {
        None
    };
    while effects.as_ref().is_some_and(|cache| !cache.is_idle()) {
        if let Some(error) = effects.as_ref().and_then(|cache| cache.error()) {
            return Err(Error::EffectFailed(error));
        }
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        glib::timeout_future(Duration::from_millis(10)).await;
    }
    if cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }
    if let Some(error) = effects.as_ref().and_then(|cache| cache.error()) {
        return Err(Error::EffectFailed(error));
    }
    let crop = crop.unwrap_or(Bounds {
        x: 0.,
        y: 0.,
        width: dimensions[0],
        height: dimensions[1],
    });
    if ![crop.x, crop.y, crop.width, crop.height]
        .iter()
        .all(|v| v.is_finite())
        || crop.width <= 0.
        || crop.height <= 0.
    {
        return Err(Error::InvalidBounds);
    }
    let x = crop.x.floor().max(0.);
    let y = crop.y.floor().max(0.);
    let right = (crop.x + crop.width).ceil().min(dimensions[0]);
    let bottom = (crop.y + crop.height).ceil().min(dimensions[1]);
    let width = (right - x).max(0.) as u32;
    let height = (bottom - y).max(0.) as u32;
    if width == 0
        || height == 0
        || u64::from(width) * u64::from(height) > clivuu_core::image_ops::MAX_PIXELS
    {
        return Err(Error::InvalidBounds);
    }
    let [output_width, output_height] = size.unwrap_or([width, height]);
    if output_width == 0
        || output_height == 0
        || u64::from(output_width) * u64::from(output_height) > clivuu_core::image_ops::MAX_PIXELS
    {
        return Err(Error::InvalidBounds);
    }
    let sx = output_width as f32 / width as f32;
    let sy = output_height as f32 / height as f32;
    let snapshot = gtk::Snapshot::new();
    snapshot.push_clip(&graphene::Rect::new(
        0.,
        0.,
        output_width as f32,
        output_height as f32,
    ));
    let filtered = filter.push(&snapshot);
    let source_rect = graphene::Rect::new(
        -x as f32 * sx,
        -y as f32 * sy,
        dimensions[0] as f32 * sx,
        dimensions[1] as f32 * sy,
    );
    if [output_width, output_height] != [width, height] {
        // Put the texture directly in output coordinates: Cairo determines
        // nearest sampling from the node bounds, ignoring an outer transform.
        // This recovers low-DPI tiles expanded in a uniform desktop backing.
        snapshot.append_scaled_texture(&source, gsk::ScalingFilter::Nearest, &source_rect);
    } else {
        snapshot.append_texture(&source, &source_rect);
    }
    let mut nodes = AnnotationNodes::default();
    let empty = super::render::MosaicTextures::new();
    let textures = effects.as_ref().map(|cache| cache.textures());
    if let Some(node) = nodes.compose(
        &marks,
        &scene.pango_context(),
        textures.as_deref().unwrap_or(&empty),
    ) {
        // Annotations retain vector antialiasing at the final pixel density.
        snapshot.save();
        snapshot.scale(sx, sy);
        snapshot.translate(&graphene::Point::new(-x as f32, -y as f32));
        snapshot.append_node(node);
        snapshot.restore();
    }
    if filtered {
        snapshot.pop();
    }
    snapshot.pop();
    let node = snapshot.to_node().ok_or(Error::MissingSource)?;
    let (width, height) = (output_width, output_height);
    let frame = renderer.render_texture(
        &node,
        Some(&graphene::Rect::new(0., 0., width as f32, height as f32)),
    );
    on_rendered();
    let task = pool
        .submit(move |token| {
            token.check()?;
            let mut pixels = vec![0; width as usize * height as usize * 4];
            // GdkTexture explicitly implements Send+Sync in gdk-rs. Download has
            // no GTK widget state; the renderer and node remain on the main loop.
            frame.download(&mut pixels, width as usize * 4);
            for (i, p) in pixels.chunks_exact_mut(4).enumerate() {
                if i % 4096 == 0 {
                    token.check()?;
                }
                let argb = u32::from_ne_bytes(p.try_into().unwrap());
                let a = argb >> 24;
                if a == 255 {
                    p.copy_from_slice(&[(argb >> 16) as u8, (argb >> 8) as u8, argb as u8, 255]);
                } else if a == 0 {
                    p.copy_from_slice(&[0, 0, 0, 0]);
                } else {
                    let channel = |shift| ((((argb >> shift) & 255u32) * 255 + a / 2) / a) as u8;
                    p.copy_from_slice(&[channel(16u32), channel(8u32), channel(0u32), a as u8]);
                }
            }
            token.check()?;
            let image = clivuu_core::image::RgbaImage::from_raw(width, height, pixels).unwrap();
            Ok(image)
        })
        .map_err(Error::Worker)?;
    let token = task.cancellation();
    let handler = cancel.connect_cancelled(move |_| token.cancel());
    let _guard = CancelGuard {
        cancellable: cancel.clone(),
        handler,
    };
    let result = task.result().await;
    if cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }
    let pixels = result.map_err(Error::Worker)?;
    super::frozen_image::FrozenImage::new(pixels).ok_or(Error::InvalidBounds)
}

pub async fn encode(
    pixels: std::sync::Arc<clivuu_core::image::RgbaImage>,
    pool: &Rc<TaskPool>,
    cancel: &gio::Cancellable,
) -> Result<Vec<u8>, Error> {
    if cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }
    let task = pool
        .submit(move |token| {
            token.check()?;
            let mut png = Vec::new();
            clivuu_core::image::export::write(
                clivuu_core::image::export::Source::Pixels(&pixels),
                std::path::Path::new("capture.png"),
                clivuu_core::config::default_image_quality(),
                &mut png,
            )
            .map_err(|e| TaskError::Failed(e.to_string()))?;
            token.check()?;
            Ok(png)
        })
        .map_err(Error::Worker)?;
    let token = task.cancellation();
    let handler = cancel.connect_cancelled(move |_| token.cancel());
    let _guard = CancelGuard {
        cancellable: cancel.clone(),
        handler,
    };
    let bytes = task.result().await.map_err(Error::Worker)?;
    if cancel.is_cancelled() {
        Err(Error::Cancelled)
    } else {
        Ok(bytes)
    }
}

pub async fn png(
    scene: &Scene,
    renderer: &gsk::Renderer,
    crop: Option<Bounds>,
    pool: &Rc<TaskPool>,
    cancel: &gio::Cancellable,
) -> Result<Vec<u8>, Error> {
    let image = frame(scene, renderer, crop, pool, cancel).await?;
    encode(image.pixels, pool, cancel).await
}

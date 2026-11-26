//! A bounded one-frame ScreenCast capture when a Wayland screenshot needs its
//! real cursor. The compositor embeds cursor pixels in the same frame; no X11
//! position, cursor texture or post-capture pointer overlay is involved.
use super::{frames::Frames, session::Session};
use crate::infrastructure::tasks::{TaskError, TaskPool};
use clivuu_core::{
    annotation::Bounds,
    capture::{Output, Selection},
    i18n::t,
    image::{RgbaImage, imageops},
    image_ops,
};
use gtk::{gio, prelude::CancellableExt};
use std::{rc::Rc, sync::Arc};

pub struct Captured {
    pub image: Arc<RgbaImage>,
    pub without_cursor: Option<Arc<RgbaImage>>,
    pub outputs: Vec<Output>,
}

pub async fn capture(
    connection: &gio::DBusConnection,
    pool: &Rc<TaskPool>,
    outputs: &[Output],
    include_clean: bool,
    cancel: &gio::Cancellable,
) -> Result<Captured, String> {
    let limit = Selection::new(outputs.to_vec(), vec![], None)
        .ok_or_else(|| t("Invalid monitor layout"))?
        .limit;
    let gnome = std::env::var("XDG_CURRENT_DESKTOP")
        .is_ok_and(|s| s.split(':').any(|s| s.eq_ignore_ascii_case("GNOME")));
    let session = if gnome {
        // The existing native GNOME capture API supports a virtual-desktop
        // area, including mixed-scale monitors, and embedded cursor pixels.
        Session::mutter_pair(connection, cancel, limit, include_clean).await?
    } else {
        // Other compositors retain their own source picker and authorization.
        // This requests screen capture only, never RemoteDesktop input access.
        Session::portal(connection, cancel, false, 1, true, true).await?
    };
    let mut readers = Vec::new();
    let result = async {
        if gnome && session.streams.len() == 2 {
            let (r0, r1) = join_pair(
                Frames::open(&session, &session.streams[0], pool),
                Frames::open(&session, &session.streams[1], pool),
            )
            .await;
            readers.push(r0?);
            if let Ok(r1) = r1 {
                readers.push(r1);
            }
        } else {
            for (idx, stream) in session.streams.iter().enumerate() {
                match Frames::open(&session, stream, pool).await {
                    Ok(reader) => readers.push(reader),
                    Err(_) if gnome && idx > 0 => {}
                    Err(e) => return Err(e),
                }
            }
        }
        if gnome && readers.len() == 2 {
            let (primary, clean) = join_pair(
                readers[0].frame(pool, &session.scope.token),
                readers[1].frame(pool, &session.scope.token),
            )
            .await;
            let primary = primary?;
            let clean = clean
                .ok()
                .filter(|c| c.dimensions() == primary.dimensions());
            let task = pool
                .submit(move |token| {
                    token.check()?;
                    let mut out = compose(vec![(limit, primary)])?;
                    out.without_cursor = clean;
                    Ok(out)
                })
                .map_err(|e| e.to_string())?;
            return gio::CancellableFuture::new(task.result(), cancel.clone())
                .await
                .map_err(|_| t("Operation cancelled"))?
                .map_err(|e| e.to_string());
        }
        let mut tiles = Vec::new();
        for (stream, reader) in session.streams.iter().zip(&readers) {
            let image = reader.frame(pool, &session.scope.token).await?;
            // Position/size are compositor logical coordinates, not GTK or
            // XWayland root coordinates. Never guess an unidentified monitor.
            let rect = stream
                .rect
                .or_else(|| (outputs.len() == 1).then_some(outputs[0].rect))
                .ok_or_else(|| t("The captured image does not match the monitor layout"))?;
            tiles.push((rect, image));
        }
        let task = pool
            .submit(move |token| {
                token.check()?;
                compose(tiles)
            })
            .map_err(|e| e.to_string())?;
        gio::CancellableFuture::new(task.result(), cancel.clone())
            .await
            .map_err(|_| t("Operation cancelled"))?
            .map_err(|e| e.to_string())
    }
    .await;
    // Close both the PipeWire readers and their owned session on success,
    // cancellation and read/assembly errors, before opening our selector.
    for reader in readers {
        reader.stop(pool).await;
    }
    session.close().await;
    if cancel.is_cancelled() {
        return Err(t("Operation cancelled"));
    }
    result.map(|mut captured| {
        // One GNOME RecordArea stream covers the virtual desktop; retain its
        // individual monitors for selection/Tab cycling and mixed-DPI hints.
        if gnome {
            captured.outputs = outputs.to_vec();
        }
        captured
    })
}

async fn join_pair<A, B>(
    a: impl std::future::Future<Output = A>,
    b: impl std::future::Future<Output = B>,
) -> (A, B) {
    let mut a = std::pin::pin!(a);
    let mut b = std::pin::pin!(b);
    let (mut oa, mut ob) = (None, None);
    std::future::poll_fn(|cx| {
        if oa.is_none()
            && let std::task::Poll::Ready(v) = a.as_mut().poll(cx)
        {
            oa = Some(v);
        }
        if ob.is_none()
            && let std::task::Poll::Ready(v) = b.as_mut().poll(cx)
        {
            ob = Some(v);
        }
        if oa.is_some() && ob.is_some() {
            std::task::Poll::Ready((oa.take().unwrap(), ob.take().unwrap()))
        } else {
            std::task::Poll::Pending
        }
    })
    .await
}

fn compose(tiles: Vec<(Bounds, Arc<RgbaImage>)>) -> Result<Captured, TaskError> {
    let invalid = || TaskError::Failed(t("The captured image does not match the monitor layout"));
    let mut outputs = Vec::new();
    for (rect, image) in &tiles {
        let scale = image.width() as f64 / rect.width;
        if !scale.is_finite()
            || scale <= 0.
            || (image.height() as f64 - rect.height * scale).abs() > 2.
        {
            return Err(invalid());
        }
        outputs.push(Output { rect: *rect, scale });
    }
    let layout = Selection::new(outputs.clone(), vec![], None).ok_or_else(invalid)?;
    if tiles.len() == 1 {
        return Ok(Captured {
            image: tiles.into_iter().next().unwrap().1,
            without_cursor: None,
            outputs,
        });
    }
    let scale = outputs.iter().map(|o| o.scale).fold(0., f64::max);
    let width = (layout.limit.width * scale).round();
    let height = (layout.limit.height * scale).round();
    if width < 1. || height < 1. || width * height > image_ops::MAX_PIXELS as f64 {
        return Err(invalid());
    }
    let mut image = RgbaImage::new(width as u32, height as u32);
    for (rect, pixels) in tiles {
        let w = (rect.width * scale).round() as u32;
        let h = (rect.height * scale).round() as u32;
        let resized;
        let pixels = if pixels.dimensions() == (w, h) {
            &*pixels
        } else {
            resized = imageops::resize(&*pixels, w, h, imageops::FilterType::Nearest);
            &resized
        };
        imageops::replace(
            &mut image,
            pixels,
            ((rect.x - layout.limit.x) * scale).round() as i64,
            ((rect.y - layout.limit.y) * scale).round() as i64,
        );
    }
    Ok(Captured {
        image: Arc::new(image),
        without_cursor: None,
        outputs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn assembles_negative_origins_and_mixed_scales_without_repositioning_cursor_pixels() {
        let mut left = RgbaImage::from_pixel(100, 80, clivuu_core::image::Rgba([10, 20, 30, 255]));
        left.put_pixel(25, 30, clivuu_core::image::Rgba([255, 0, 0, 255]));
        let right = RgbaImage::from_pixel(200, 160, clivuu_core::image::Rgba([0, 50, 100, 255]));
        let result = compose(vec![
            (
                Bounds {
                    x: -100.,
                    y: 0.,
                    width: 100.,
                    height: 80.,
                },
                Arc::new(left),
            ),
            (
                Bounds {
                    x: 0.,
                    y: 0.,
                    width: 100.,
                    height: 80.,
                },
                Arc::new(right),
            ),
        ])
        .unwrap();
        assert_eq!(result.image.dimensions(), (400, 160));
        assert_eq!(result.image.get_pixel(50, 60).0, [255, 0, 0, 255]);
        assert_eq!(result.image.get_pixel(250, 60).0, [0, 50, 100, 255]);
        assert_eq!(result.outputs[0].rect.x, -100.);
    }
}

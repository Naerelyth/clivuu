//! XFixes snapshots are valid only in a real X11 session, never for a
//! Wayland desktop whose pin surfaces happen to use XWayland.
use crate::infrastructure::tasks::{TaskError, TaskPool};
use clivuu_core::{annotation::Bounds, i18n::t, image::RgbaImage, image_ops, protocol::Rect};
use gtk::{gdk, gio, prelude::*};

pub fn real_x11(display: &gdk::Display) -> bool {
    display.type_().name() == "GdkX11Display"
        && std::env::var_os("WAYLAND_DISPLAY").is_none()
        && std::env::var("XDG_SESSION_TYPE").as_deref() != Ok("wayland")
}
use std::sync::Arc;

pub async fn snapshot(
    pool: &TaskPool,
    cancel: &gio::Cancellable,
) -> Result<clivuu_x11::CursorImage, String> {
    let task = pool
        .submit(|token| {
            token.check()?;
            let cursor = clivuu_x11::cursor_image().map_err(TaskError::Failed)?;
            token.check()?;
            Ok(cursor)
        })
        .map_err(|e| e.to_string())?;
    gio::CancellableFuture::new(task.result(), cancel.clone())
        .await
        .map_err(|_| t("Operation cancelled"))?
        .map_err(|e| e.to_string())
}

/// The caller has already established a real X11 session, never XWayland.
pub async fn position(pool: &TaskPool, cancel: &gio::Cancellable) -> Result<[f64; 2], String> {
    let task = pool
        .submit(|token| {
            token.check()?;
            let (x, y) = clivuu_x11::pointer_position().map_err(TaskError::Failed)?;
            Ok([x as f64, y as f64])
        })
        .map_err(|e| e.to_string())?;
    gio::CancellableFuture::new(task.result(), cancel.clone())
        .await
        .map_err(|_| t("Operation cancelled"))?
        .map_err(|e| e.to_string())
}

pub fn desktop_point(point: [f64; 2], root: Bounds, scale: f64, desktop: Bounds) -> [f64; 2] {
    [
        desktop.x + (point[0] / scale - root.x) * desktop.width / root.width,
        desktop.y + (point[1] / scale - root.y) * desktop.height / root.height,
    ]
}
fn image_rect(
    cursor: &clivuu_x11::CursorImage,
    root: Bounds,
    scale: f64,
    width: u32,
    height: u32,
) -> Rect {
    let sx = width as f64 / root.width / scale;
    let sy = height as f64 / root.height / scale;
    let round = |value: f64| (value + 0.5).floor(); // JS Math.round, including negative hotspots.
    Rect {
        x: round((cursor.x as f64 - cursor.xhot as f64 - root.x * scale) * sx) as i64,
        y: round((cursor.y as f64 - cursor.yhot as f64 - root.y * scale) * sy) as i64,
        width: round(cursor.width as f64 * sx).max(1.) as u32,
        height: round(cursor.height as f64 * sy).max(1.) as u32,
    }
}

pub async fn composite(
    image: Arc<RgbaImage>,
    cursor: clivuu_x11::CursorImage,
    root: Bounds,
    scale: f64,
    pool: &TaskPool,
    cancel: &gio::Cancellable,
) -> Result<Arc<RgbaImage>, String> {
    let task = pool
        .submit(move |token| {
            token.check()?;
            let rect = image_rect(&cursor, root, scale, image.width(), image.height());
            let pointer =
                RgbaImage::from_raw(cursor.width as u32, cursor.height as u32, cursor.pixels)
                    .ok_or_else(|| TaskError::Failed(t("Cannot read the XWayland cursor image")))?;
            let mut image = Arc::try_unwrap(image).unwrap_or_else(|image| (*image).clone());
            image_ops::composite_cursor_image(&mut image, pointer, rect)
                .map_err(|e| TaskError::Failed(e.to_string()))?;
            token.check()?;
            Ok(Arc::new(image))
        })
        .map_err(|e| e.to_string())?;
    gio::CancellableFuture::new(task.result(), cancel.clone())
        .await
        .map_err(|_| t("Operation cancelled"))?
        .map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pointer_uses_whole_root_origin_scale_and_hotspot() {
        let cursor = clivuu_x11::CursorImage {
            x: -1199,
            y: 400,
            xhot: 2,
            yhot: 4,
            width: 24,
            height: 32,
            pixels: vec![],
        };
        let root = Bounds {
            x: -600.,
            y: 0.,
            width: 1800.,
            height: 800.,
        };
        assert_eq!(
            image_rect(&cursor, root, 2., 1800, 800),
            Rect {
                x: 0,
                y: 198,
                width: 12,
                height: 16
            }
        );
        assert_eq!(
            image_rect(&cursor, root, 1., 3600, 1600),
            Rect {
                x: -1202,
                y: 792,
                width: 48,
                height: 64
            }
        );
    }
}

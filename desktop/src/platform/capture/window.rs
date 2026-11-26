//! One-frame window capture through the shared public ScreenCast lifecycle.
use super::{frames::Frames, session::Session};
use crate::infrastructure::tasks::TaskPool;
use clivuu_core::{annotation::Bounds, capture::Output, i18n::t, image::RgbaImage};
use gtk::{gio, prelude::CancellableExt};
use std::{rc::Rc, sync::Arc};

pub async fn capture(
    connection: &gio::DBusConnection,
    pool: &Rc<TaskPool>,
    cursor: bool,
    cancel: &gio::Cancellable,
) -> Result<Option<Arc<RgbaImage>>, String> {
    let Some(session) = Session::window(connection, cancel, cursor).await? else {
        return Ok(None);
    };
    let result = async {
        let [stream] = session.streams.as_slice() else {
            return Err(t("The system returned no capture stream"));
        };
        if stream
            .properties
            .get("source_type")
            .and_then(|v| v.get::<u32>())
            .is_some_and(|v| v != 2)
        {
            return Err(t("The system returned no capture stream"));
        }
        let reader = Frames::open(&session, stream, pool).await?;
        let pixels = reader.frame(pool, &session.scope.token).await;
        reader.stop(pool).await;
        pixels
    }
    .await;
    // The permission/session and PipeWire pipeline end before any editor is mapped.
    session.close().await;
    if cancel.is_cancelled() {
        return Err(t("Operation cancelled"));
    }
    result.map(Some)
}

/// Presentation only: an isolated portal image has no trustworthy screen origin.
/// Reuse its pixels at their original resolution; fit only the display mapping.
pub fn presentation(size: [u32; 2], output: Output) -> Bounds {
    let [w, h] = size.map(f64::from);
    let scale = output
        .scale
        .max(w / (output.rect.width - 48.).max(1.))
        .max(h / (output.rect.height - 160.).max(1.));
    let width = w / scale;
    let height = h / scale;
    Bounds {
        x: output.rect.x + (output.rect.width - width) / 2.,
        y: output.rect.y + (output.rect.height - height) / 2.,
        width,
        height,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn isolated_pixels_fit_negative_fractional_outputs_without_resampling() {
        let output = Output {
            rect: Bounds {
                x: -1280.,
                y: -100.,
                width: 1280.,
                height: 800.,
            },
            scale: 1.25,
        };
        for size in [[73, 41], [1600, 900], [4000, 6000]] {
            let r = presentation(size, output);
            assert!(r.x >= output.rect.x && r.y >= output.rect.y);
            assert!(
                r.x + r.width <= output.rect.x + output.rect.width
                    && r.y + r.height <= output.rect.y + output.rect.height
            );
            assert!((r.width / r.height - size[0] as f64 / size[1] as f64).abs() < 1e-6);
        }
    }
}

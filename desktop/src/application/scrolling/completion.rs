//! Finish from committed pixels; repair only when HUD coverage left a hole.
use super::*;

pub(super) async fn image(
    connection: &gio::DBusConnection,
    pool: &Rc<TaskPool>,
    document: &Arc<Mutex<Document>>,
    controls: &Controls,
    outputs: &[clivuu_core::capture::Output],
    source: &mut Option<RegionSource>,
    token: &gio::Cancellable,
) -> Result<Arc<RgbaImage>, String> {
    let error = match acquisition::image(document, pool).await {
        Ok(image) => return Ok(image),
        Err(error) => error,
    };
    let repairs = acquisition::repairs(document, pool).await?;
    if repairs.is_empty() {
        return Err(error);
    }
    controls.prepare_finish().await?;
    let mut result = Err(error);
    for request in repairs {
        // Moving back may leave a masked tail outside the current selection.
        // Core verifies the recorded visible pixels at this original geometry
        // before filling a hole; this never changes document navigation.
        let pixels = if let Some(reader) = source
            .as_ref()
            .filter(|reader| reader.covers(request.viewport))
        {
            reader.clean_image(request.viewport, pool, token).await?
        } else {
            let reader = open(connection, pool, request.viewport, outputs, token, false).await?;
            let pixels = reader.image(request.viewport, pool, token).await;
            reader.close(pool).await;
            pixels?
        };
        match acquisition::repair(document, request, pixels, pool).await {
            Ok(()) => {
                result = acquisition::image(document, pool).await;
                if result.is_ok() {
                    break;
                }
            }
            Err(error) => result = Err(error),
        }
    }
    result
}

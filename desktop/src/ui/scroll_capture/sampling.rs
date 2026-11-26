//! Presentation barriers for starting capture and the final clean still.
//! Never conceal the HUD in the live sampling loop.
use super::*;
struct PaintWait {
    clock: gdk::FrameClock,
    signal: Option<glib::SignalHandlerId>,
}
impl Drop for PaintWait {
    fn drop(&mut self) {
        if let Some(signal) = self.signal.take() {
            self.clock.disconnect(signal);
        }
    }
}
impl Controls {
    pub async fn prepare_start(&self) -> Result<(), String> {
        self.present_changes().await?;
        self.inner.surface().set_keyboard(true, true)?;
        self.present_changes().await
    }
    pub async fn prepare_finish(&self) -> Result<(), String> {
        self.sampling(true);
        self.present_changes().await
    }
    /// Hide only painted controls (toolbar/hint/popup) that overlap `rect`.
    /// Keeps the exterior selection frame and preview strip visible without flicker.
    pub async fn conceal_overlapping_hud(&self, rect: Bounds) -> Result<bool, String> {
        let i = &self.inner;
        let popup_overlapped = i
            .current_popup_exclusion()
            .is_some_and(|b| clivuu_core::pins::overlaps(b, rect));
        if i.popup.is_open() {
            i.suppress_popup_close_move.set(true);
            i.popup.close();
            i.suppress_popup_close_move.set(false);
        }
        i.open_popup_exclusion.set(None);
        i.closing_popup_exclusion.set(None);
        let hide_panel = clivuu_core::pins::overlaps(placement::paint_bounds(i.input.get()), rect);
        let hide_hint = clivuu_core::pins::overlaps(i.hint_rect.get(), rect);
        if !hide_panel && !hide_hint && !popup_overlapped {
            return Ok(false);
        }
        i.tooltips(false);
        if hide_panel {
            i.panel.set_opacity(0.);
        }
        if hide_hint {
            i.hint.set_opacity(0.);
        }
        self.present_changes().await?;
        Ok(true)
    }
    pub fn restore_hud(&self) {
        let i = &self.inner;
        i.panel.set_opacity(1.);
        i.hint.set_opacity(1.);
        i.tooltips(!i.sampling.get());
    }
    /// Wait for presentation without changing any visible widget.
    pub async fn present_changes(&self) -> Result<(), String> {
        let surface = self.inner.surface();
        let clock = surface
            .window
            .frame_clock()
            .ok_or_else(|| t("The desktop service is unavailable"))?;
        let (tx, rx) = async_channel::bounded(1);
        let signal = clock.connect_after_paint(move |clock| {
            let _ = tx.try_send(clock.frame_counter());
        });
        let wait = PaintWait {
            clock,
            signal: Some(signal),
        };
        surface.window.queue_draw();
        let submitted = glib::future_with_timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .map_err(|_| t("The capture stream timed out"))?
            .map_err(|_| t("Operation cancelled"))?;
        // Flush X11 changes before starting a reader for the new layout.
        gtk::prelude::WidgetExt::display(&surface.window).sync();
        // Cross the next frame boundary as well: submitting to XWayland and
        // presenting that buffer in the compositor are separate operations.
        surface.window.queue_draw();
        let counter = glib::future_with_timeout(std::time::Duration::from_secs(2), async {
            loop {
                let counter = rx.recv().await.map_err(|_| t("Operation cancelled"))?;
                if counter > submitted {
                    return Ok::<_, String>(counter);
                }
            }
        })
        .await
        .map_err(|_| t("The capture stream timed out"))??;
        glib::future_with_timeout(std::time::Duration::from_secs(2), async {
            while !wait.clock.timings(counter).is_some_and(|t| t.is_complete()) {
                glib::timeout_future(std::time::Duration::from_millis(2)).await;
            }
        })
        .await
        .map_err(|_| t("The capture stream timed out"))?;
        Ok(())
    }
}

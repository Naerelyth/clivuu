//! Invisible foreground owner for a first-use system permission dialog.
//! The portal draws all authorization UI; no application confirmation is shown.
use clivuu_core::i18n::t;
use gtk::{gdk, gio, glib, prelude::*};
use std::time::Duration;

/// GNOME's AccessDialog alone checks that a named caller is foreground.
/// Other desktops must keep their portal's native activation/consent behavior.
pub fn needs_owner() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|desktop| {
        desktop
            .split(':')
            .any(|name| name.eq_ignore_ascii_case("GNOME"))
    })
}

pub struct Owner {
    window: gtk::Window,
    scope: crate::infrastructure::cancellation::Scope,
}
impl Owner {
    pub fn new(cancel: &gio::Cancellable) -> Self {
        let scope = crate::infrastructure::cancellation::Scope::new(cancel);
        let window = gtk::Window::builder()
            .title("Clivuu")
            .decorated(false)
            .default_width(1)
            .default_height(1)
            .resizable(false)
            .opacity(0.)
            .build();
        if let Some(app) = gio::Application::default().and_downcast::<gtk::Application>() {
            window.set_application(Some(&app));
        }
        crate::platform::integration::identity::configure(&window);
        let close = scope.token.clone();
        window.connect_close_request(move |_| {
            close.cancel();
            glib::Propagation::Stop
        });
        let keys = gtk::EventControllerKey::new();
        let close = scope.token.clone();
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gdk::Key::Escape {
                close.cancel();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        window.add_controller(keys);
        Self { window, scope }
    }
    pub fn cancellable(&self) -> &gio::Cancellable {
        &self.scope.token
    }
    pub async fn activate(&self) -> Result<(), String> {
        let (tx, rx) = async_channel::bounded(1);
        let signal = self.window.connect_is_active_notify(move |window| {
            if window.is_active() {
                let _ = tx.try_send(());
            }
        });
        self.window.present();
        let result = if self.window.is_active() {
            Ok(())
        } else {
            match gio::CancellableFuture::new(
                glib::future_with_timeout(Duration::from_secs(5), rx.recv()),
                self.scope.token.clone(),
            )
            .await
            {
                Ok(Ok(Ok(()))) => Ok(()),
                Err(_) => Err(t("Operation cancelled")),
                _ => Err(t("The desktop could not activate screenshot authorization")),
            }
        };
        self.window.disconnect(signal);
        result
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        self.window.destroy();
    }
}

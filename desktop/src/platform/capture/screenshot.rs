//! Static Screenshot portal acquisition. This module never opens a ScreenCast
//! session or PipeWire remote and never mutates/deletes the portal's image.
use crate::infrastructure::tasks::{TaskError, TaskPool};
use crate::platform::portal::{self, Results};
use clivuu_core::{i18n::t, image::RgbaImage, image_ops};
use gtk::{gio, glib, prelude::*};
use std::{cell::Cell, io::Read, sync::Arc};

pub struct Screenshot {
    connection: gio::DBusConnection,
    destination: String,
    identity: Option<String>,
    authorized: Cell<bool>,
}
impl Screenshot {
    pub fn is_connected(&self) -> bool {
        !self.connection.is_closed()
    }
    /// A private peer must register before its first portal call. Inheriting a
    /// terminal/launcher systemd scope does not identify the desktop service.
    /// A new peer also re-registers correctly after a portal daemon restart.
    pub async fn connect(cancel: &gio::Cancellable) -> Result<Self, String> {
        let address = gio::dbus_address_get_for_bus_sync(gio::BusType::Session, Some(cancel))
            .map_err(|e| e.to_string())?;
        let connection = gio::DBusConnection::for_address_future(
            &address,
            gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
                | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
            None,
        );
        let connection = gio::CancellableFuture::new(connection, cancel.clone())
            .await
            .map_err(|_| t("Operation cancelled"))?
            .map_err(|e| e.to_string())?;
        let mut this = Self::with_destination(&connection, portal::DESTINATION);
        if let Some(id) = crate::platform::integration::identity::desktop_id() {
            let registered = gio::CancellableFuture::new(
                connection.call_future(
                    Some(portal::DESTINATION),
                    portal::PATH,
                    "org.freedesktop.host.portal.Registry",
                    "Register",
                    Some(&(id.as_str(), Results::new()).to_variant()),
                    None,
                    gio::DBusCallFlags::NONE,
                    10000,
                ),
                cancel.clone(),
            )
            .await
            .map_err(|_| t("Operation cancelled"))?;
            match registered {
                Ok(_) => this.identity = Some(id),
                Err(e)
                    if e.matches(gio::DBusError::UnknownMethod)
                        || e.matches(gio::DBusError::UnknownInterface) => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        Ok(this)
    }
    /// Alternative endpoint for a backend or isolated protocol fixture.
    pub fn with_destination(connection: &gio::DBusConnection, destination: &str) -> Self {
        Self {
            connection: connection.clone(),
            destination: destination.into(),
            identity: None,
            authorized: Cell::new(false),
        }
    }
    pub async fn capture(
        &self,
        pool: &TaskPool,
        cancel: &gio::Cancellable,
    ) -> Result<Arc<RgbaImage>, String> {
        if !self.authorized.get()
            && let Some(id) = &self.identity
            && crate::platform::surfaces::authorization::needs_owner()
        {
            // Only read permissions. Grants/denials are owned by the system UI.
            let permission = gio::CancellableFuture::new(
                self.connection.call_future(
                    Some("org.freedesktop.impl.portal.PermissionStore"),
                    "/org/freedesktop/impl/portal/PermissionStore",
                    "org.freedesktop.impl.portal.PermissionStore",
                    "Lookup",
                    Some(&("screenshot", "screenshot").to_variant()),
                    None,
                    gio::DBusCallFlags::NONE,
                    5000,
                ),
                cancel.clone(),
            )
            .await
            .map_err(|_| t("Operation cancelled"))?;
            let missing = match permission {
                Ok(reply) => reply
                    .child_value(0)
                    .get::<std::collections::HashMap<String, Vec<String>>>()
                    .is_some_and(|values| {
                        if values
                            .get(id)
                            .and_then(|v| v.first())
                            .is_some_and(|s| s == "yes")
                        {
                            self.authorized.set(true);
                        }
                        !values.contains_key(id)
                    }),
                Err(e) => gio::DBusError::remote_error(&e)
                    .is_some_and(|name| name == "org.freedesktop.portal.Error.NotFound"),
            };
            if missing {
                let parent = crate::platform::surfaces::authorization::Owner::new(cancel);
                parent.activate().await?;
                // Authorization and capture are one portal request. The owner
                // is transparent; reuse the returned still instead of taking
                // a second screenshot (and triggering a second shutter flash).
                let result = self.frame(pool, parent.cancellable()).await;
                drop(parent);
                if result.is_ok() {
                    self.authorized.set(true);
                }
                return result;
            }
        }
        let result = self.frame(pool, cancel).await;
        if result.is_err() {
            self.authorized.set(false);
        }
        if self.identity.is_none()
            && result
                .as_ref()
                .is_err_and(|e| e == &t("The system denied the capture or recording request"))
        {
            return Err(t(
                "Screenshot authorization failed. Quit Clivuu and start it from its installed application launcher. Reliable authorization from any launcher requires a portal with host Registry support.",
            ));
        }
        result
    }
    async fn frame(
        &self,
        pool: &TaskPool,
        cancel: &gio::Cancellable,
    ) -> Result<Arc<RgbaImage>, String> {
        let options = Results::from_iter([
            ("interactive".into(), false.to_variant()),
            ("modal".into(), false.to_variant()),
        ]);
        let result = portal::request(
            &self.connection,
            &self.destination,
            "Screenshot",
            "Screenshot",
            vec!["".to_variant()],
            options,
            cancel,
        )
        .await?;
        Self::image(result, pool, cancel).await
    }
    async fn image(
        result: Results,
        pool: &TaskPool,
        cancel: &gio::Cancellable,
    ) -> Result<Arc<RgbaImage>, String> {
        let uri = result
            .get("uri")
            .and_then(|v| v.str())
            .ok_or_else(|| t("The system returned no image"))?;
        let file = gio::File::for_uri(uri);
        if !file.is_native() {
            return Err(t("The system returned no image"));
        }
        if let Some(path) = file.path() {
            let task = pool
                .submit(move |token| {
                    token.check()?;
                    let f =
                        std::fs::File::open(&path).map_err(|e| TaskError::Failed(e.to_string()))?;
                    let hint = f
                        .metadata()
                        .map(|m| m.len().min(image_ops::MAX_BYTES) as usize)
                        .unwrap_or(0);
                    let mut bytes = Vec::with_capacity(hint);
                    f.take(image_ops::MAX_BYTES + 1)
                        .read_to_end(&mut bytes)
                        .map_err(|e| TaskError::Failed(e.to_string()))?;
                    if bytes.len() as u64 > image_ops::MAX_BYTES {
                        return Err(TaskError::Failed(t("The image file exceeds 64 MiB")));
                    }
                    token.check()?;
                    let image =
                        image_ops::decode(&bytes).map_err(|e| TaskError::Failed(e.to_string()))?;
                    token.check()?;
                    Ok(Arc::new(image))
                })
                .map_err(|error| error.to_string())?;
            return gio::CancellableFuture::new(task.result(), cancel.clone())
                .await
                .map_err(|_| t("Operation cancelled"))?
                .map_err(|error| error.to_string());
        }
        gio::CancellableFuture::new(
            async {
                // Read in chunks with an actual byte limit: a size query alone can
                // race a growing file, and load_contents allocates without a bound.
                let stream = file
                    .read_future(glib::Priority::DEFAULT)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut bytes = Vec::new();
                loop {
                    let block = stream
                        .read_bytes_future(1024 * 1024, glib::Priority::DEFAULT)
                        .await
                        .map_err(|e| e.to_string())?;
                    if block.is_empty() {
                        break;
                    }
                    if bytes.len() as u64 + block.len() as u64 > image_ops::MAX_BYTES {
                        return Err(t("The image file exceeds 64 MiB"));
                    }
                    bytes.extend_from_slice(&block);
                }
                stream
                    .close_future(glib::Priority::DEFAULT)
                    .await
                    .map_err(|e| e.to_string())?;
                let task = pool
                    .submit(move |token| {
                        token.check()?;
                        let image = image_ops::decode(&bytes)
                            .map_err(|e| TaskError::Failed(e.to_string()))?;
                        token.check()?;
                        Ok(Arc::new(image))
                    })
                    .map_err(|error| error.to_string())?;
                task.result().await.map_err(|error| error.to_string())
            },
            cancel.clone(),
        )
        .await
        .map_err(|_| t("Operation cancelled"))?
    }
}

#[cfg(test)]
mod tests;

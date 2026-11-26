//! Asynchronous desktop portal requests, independent of the display backend.
//! Install the response subscription before the method call: cached permission
//! responses may arrive before the method reply. Cleanup also closes cancelled
//! requests, without disconnecting a Cancellable from its own callback.
use clivuu_core::i18n::t;
use gtk::{gio, glib, prelude::*};
use std::{collections::HashMap, time::Duration};
pub const DESTINATION: &str = "org.freedesktop.portal.Desktop";
pub const PATH: &str = "/org/freedesktop/portal/desktop";
pub type Results = HashMap<String, glib::Variant>;
struct Request {
    connection: gio::DBusConnection,
    destination: String,
    path: String,
    _subscription: gio::SignalSubscription,
    cancellable: gio::Cancellable,
    cancel: Option<gio::CancelledHandlerId>,
    complete: bool,
}
impl Drop for Request {
    fn drop(&mut self) {
        if let Some(id) = self.cancel.take() {
            self.cancellable.disconnect_cancelled(id);
        }
        if !self.complete {
            self.connection.call(
                Some(&self.destination),
                &self.path,
                "org.freedesktop.portal.Request",
                "Close",
                None,
                None,
                gio::DBusCallFlags::NO_AUTO_START,
                5000,
                None::<&gio::Cancellable>,
                |_| {},
            );
        }
    }
}
pub async fn request(
    connection: &gio::DBusConnection,
    destination: &str,
    interface: &str,
    method: &str,
    arguments: Vec<glib::Variant>,
    options: Results,
    cancellable: &gio::Cancellable,
) -> Result<Results, String> {
    request_optional(
        connection,
        destination,
        interface,
        method,
        arguments,
        options,
        cancellable,
    )
    .await?
    .ok_or_else(|| t("System authorization or selection was cancelled"))
}
/// User cancellation is a normal result for system selection UI. Other clients
/// retain the existing strict request() contract.
pub async fn request_optional(
    connection: &gio::DBusConnection,
    destination: &str,
    interface: &str,
    method: &str,
    arguments: Vec<glib::Variant>,
    options: Results,
    cancellable: &gio::Cancellable,
) -> Result<Option<Results>, String> {
    glib::future_with_timeout(
        Duration::from_secs(120),
        request_inner(
            connection,
            destination,
            interface,
            method,
            arguments,
            options,
            cancellable,
        ),
    )
    .await
    .map_err(|_| t("The system authorization request timed out"))?
}
async fn request_inner(
    connection: &gio::DBusConnection,
    destination: &str,
    interface: &str,
    method: &str,
    mut arguments: Vec<glib::Variant>,
    mut options: Results,
    cancellable: &gio::Cancellable,
) -> Result<Option<Results>, String> {
    if cancellable.is_cancelled() {
        return Err(t("Operation cancelled"));
    }
    let token = format!("clivuu_{}", glib::uuid_string_random().replace('-', "_"));
    let sender = connection
        .unique_name()
        .ok_or_else(|| t("The desktop service is unavailable"))?
        .trim_start_matches(':')
        .replace('.', "_");
    let path = format!("{PATH}/request/{sender}/{token}");
    options.insert("handle_token".into(), token.to_variant());
    arguments.push(options.to_variant());
    let parameters = glib::Variant::tuple_from_iter(arguments);
    let (tx, rx) = async_channel::bounded(1);
    let response = tx.clone();
    let subscription = connection.subscribe_to_signal(
        Some(destination),
        Some("org.freedesktop.portal.Request"),
        Some("Response"),
        Some(&path),
        None,
        gio::DBusSignalFlags::NONE,
        move |signal| {
            let result = signal
                .parameters
                .get::<(u32, Results)>()
                .ok_or_else(|| t("Invalid desktop response"));
            let _ = response.try_send(result);
        },
    );
    let cancelled = tx.clone();
    let cancel = cancellable.connect_cancelled(move |_| {
        let _ = cancelled.try_send(Err(t("Operation cancelled")));
    });
    let mut request = Request {
        connection: connection.clone(),
        destination: destination.into(),
        path: path.clone(),
        _subscription: subscription,
        cancellable: cancellable.clone(),
        cancel,
        complete: false,
    };
    let expected = path;
    connection.call(
        Some(destination),
        PATH,
        &format!("org.freedesktop.portal.{interface}"),
        method,
        Some(&parameters),
        Some(glib::VariantTy::new("(o)").unwrap()),
        gio::DBusCallFlags::NONE,
        130000,
        Some(cancellable),
        move |reply| {
            let error = match reply {
                Err(error) => Some(error.to_string()),
                Ok(reply) => {
                    let path = reply.child_value(0);
                    (path.str() != Some(&expected)).then(|| t("Invalid desktop response"))
                }
            };
            if let Some(error) = error {
                let _ = tx.try_send(Err(error));
            }
        },
    );
    let result = rx
        .recv()
        .await
        .map_err(|_| t("The desktop service is unavailable"))?;
    let (code, values) = result?;
    request.complete = true;
    match code {
        0 => Ok(Some(values)),
        1 => Ok(None),
        _ => Err(t("The system denied the capture or recording request")),
    }
}
pub async fn pick_color(cancellable: gio::Cancellable) -> Result<[u8; 3], String> {
    let connection = gio::bus_get_future(gio::BusType::Session)
        .await
        .map_err(|e| e.to_string())?;
    let results = request(
        &connection,
        DESTINATION,
        "Screenshot",
        "PickColor",
        vec!["".to_variant()],
        Results::new(),
        &cancellable,
    )
    .await?;
    let (r, g, b) = results
        .get("color")
        .and_then(|v| v.get::<(f64, f64, f64)>())
        .ok_or_else(|| t("Invalid desktop response"))?;
    if [r, g, b]
        .iter()
        .any(|v| !v.is_finite() || !(0.0..=1.).contains(v))
    {
        return Err(t("Invalid desktop response"));
    }
    Ok([r, g, b].map(|v| (v * 255.).round() as u8))
}

//! Owned streaming capture sessions, including one-frame cursor-inclusive stills.
//! Pointer permission is requested only for explicitly automatic scrolling.
use crate::infrastructure::cancellation::Scope;
use crate::platform::portal::{self, Results};
use clivuu_core::{annotation::Bounds, i18n::t};
use gtk::{gio, glib, prelude::*};
use std::{cell::Cell, os::fd::OwnedFd, rc::Rc};
const MUTTER: &str = "org.gnome.Mutter.ScreenCast";
const ROOT: &str = "/org/gnome/Mutter/ScreenCast";
#[cfg(test)]
mod tests;
#[derive(Clone, Debug)]
pub struct Stream {
    pub node: u32,
    pub rect: Option<Bounds>,
    pub properties: Results,
}
#[derive(Clone, Copy)]
enum Backend {
    Portal { pointer: bool },
    Mutter,
}
pub struct Session {
    connection: gio::DBusConnection,
    backend: Backend,
    path: String,
    pub streams: Vec<Stream>,
    pub scope: Scope,
    signals: Vec<gio::SignalSubscription>,
    closing: Cell<bool>,
    parent_scope: Option<Scope>,
}
impl Drop for Session {
    fn drop(&mut self) {
        if !self.closing.replace(true) && !self.path.is_empty() {
            let (path, interface, method) = self.stop_arguments();
            self.connection.call(
                Some(self.destination()),
                &path,
                &interface,
                method,
                None,
                None,
                gio::DBusCallFlags::NO_AUTO_START,
                5000,
                gio::Cancellable::NONE,
                |_| {},
            );
        }
    }
}
impl Session {
    fn new(connection: &gio::DBusConnection, backend: Backend, cancel: &gio::Cancellable) -> Self {
        Self {
            connection: connection.clone(),
            backend,
            path: String::new(),
            streams: vec![],
            scope: Scope::new(cancel),
            signals: vec![],
            closing: Cell::new(false),
            parent_scope: None,
        }
    }
    pub fn can_scroll(&self) -> bool {
        matches!(self.backend, Backend::Portal { pointer: true })
    }
    pub fn retain_scope(&mut self, scope: Scope) {
        self.parent_scope = Some(scope);
    }
    fn destination(&self) -> &str {
        match self.backend {
            Backend::Mutter => MUTTER,
            _ => portal::DESTINATION,
        }
    }
    fn stop_arguments(&self) -> (String, String, &'static str) {
        match self.backend {
            Backend::Mutter => (self.path.clone(), format!("{MUTTER}.Session"), "Stop"),
            _ => (
                self.path.clone(),
                "org.freedesktop.portal.Session".into(),
                "Close",
            ),
        }
    }
    fn observe(&mut self) {
        let cancel = self.scope.token.clone();
        self.signals.push(self.connection.subscribe_to_signal(
            Some(self.destination()),
            Some(match self.backend {
                Backend::Mutter => "org.gnome.Mutter.ScreenCast.Session",
                _ => "org.freedesktop.portal.Session",
            }),
            Some("Closed"),
            Some(&self.path),
            None,
            gio::DBusSignalFlags::NONE,
            move |_| cancel.cancel(),
        ));
        let cancel = self.scope.token.clone();
        let destination = self.destination().to_owned();
        self.signals.push(self.connection.subscribe_to_signal(
            Some("org.freedesktop.DBus"),
            Some("org.freedesktop.DBus"),
            Some("NameOwnerChanged"),
            Some("/org/freedesktop/DBus"),
            Some(&destination),
            gio::DBusSignalFlags::NONE,
            move |signal| {
                if signal
                    .parameters
                    .get::<(String, String, String)>()
                    .is_some_and(|(_, old, new)| !old.is_empty() && old != new)
                {
                    cancel.cancel();
                }
            },
        ));
    }
    pub async fn portal(
        connection: &gio::DBusConnection,
        cancel: &gio::Cancellable,
        pointer: bool,
        types: u32,
        multiple: bool,
        cursor: bool,
    ) -> Result<Self, String> {
        Self::portal_mode(
            connection,
            cancel,
            pointer,
            types,
            multiple,
            if cursor { 2 } else { 1 },
        )
        .await?
        .ok_or_else(|| t("System authorization or selection was cancelled"))
    }
    /// Recording monitor selection, with user dismissal as a normal outcome.
    pub async fn monitor(
        connection: &gio::DBusConnection,
        cancel: &gio::Cancellable,
        cursor: bool,
    ) -> Result<Option<Self>, String> {
        Self::portal_mode(
            connection,
            cancel,
            false,
            1,
            false,
            if cursor { 2 } else { 1 },
        )
        .await
    }
    /// Single-window stills use the public portal on every compositor. The
    /// system picker owns window selection, never a Screenshot save workflow.
    pub async fn window(
        connection: &gio::DBusConnection,
        cancel: &gio::Cancellable,
        cursor: bool,
    ) -> Result<Option<Self>, String> {
        let reply = gio::CancellableFuture::new(
            connection.call_future(
                Some(portal::DESTINATION),
                portal::PATH,
                "org.freedesktop.DBus.Properties",
                "GetAll",
                Some(&("org.freedesktop.portal.ScreenCast",).to_variant()),
                None,
                gio::DBusCallFlags::NONE,
                5000,
            ),
            cancel.clone(),
        )
        .await
        .map_err(|_| t("Operation cancelled"))?
        .map_err(|e| e.to_string())?;
        let (properties,) = reply
            .get::<(Results,)>()
            .ok_or_else(|| t("Invalid desktop response"))?;
        if properties
            .get("AvailableSourceTypes")
            .and_then(|v| v.get::<u32>())
            .unwrap_or(0)
            & 2
            == 0
        {
            return Err(t("Window capture is unavailable on this desktop"));
        }
        let mode = if cursor { 2u32 } else { 1u32 };
        if properties
            .get("AvailableCursorModes")
            .and_then(|v| v.get::<u32>())
            .unwrap_or(1)
            & mode
            == 0
        {
            return Err(t("Cursor capture is unavailable on this desktop"));
        }
        Self::portal_mode(connection, cancel, false, 2, false, mode).await
    }
    /// Cursor metadata observes native Wayland motion without moving the pointer
    /// or baking it into the collected pixels. XWayland root queries go stale
    /// whenever the real pointer is over a native Wayland client.
    pub async fn automatic(
        connection: &gio::DBusConnection,
        cancel: &gio::Cancellable,
    ) -> Result<Self, String> {
        let reply = gio::CancellableFuture::new(
            connection.call_future(
                Some(portal::DESTINATION),
                portal::PATH,
                "org.freedesktop.DBus.Properties",
                "Get",
                Some(&("org.freedesktop.portal.ScreenCast", "AvailableCursorModes").to_variant()),
                None,
                gio::DBusCallFlags::NONE,
                5000,
            ),
            cancel.clone(),
        )
        .await
        .map_err(|_| t("Operation cancelled"))?
        .map_err(|e| e.to_string())?;
        let modes = reply
            .child_value(0)
            .as_variant()
            .and_then(|v| v.get::<u32>())
            .unwrap_or(0);
        if modes & 4 == 0 {
            return Err(t(
                "Automatic scrolling is unavailable on this desktop. Scroll manually instead.",
            ));
        }
        Self::portal_mode(connection, cancel, true, 1, false, 4)
            .await?
            .ok_or_else(|| t("System authorization or selection was cancelled"))
    }
    async fn portal_mode(
        connection: &gio::DBusConnection,
        cancel: &gio::Cancellable,
        pointer: bool,
        types: u32,
        multiple: bool,
        cursor_mode: u32,
    ) -> Result<Option<Self>, String> {
        let mut session = Self::new(connection, Backend::Portal { pointer }, cancel);
        let token = format!("clivuu_{}", glib::uuid_string_random().replace('-', "_"));
        let sender = connection
            .unique_name()
            .ok_or_else(|| t("The desktop service is unavailable"))?
            .trim_start_matches(':')
            .replace('.', "_");
        session.path = format!("{}/session/{sender}/{token}", portal::PATH);
        session.observe();
        let Some(created) = portal::request_optional(
            connection,
            portal::DESTINATION,
            if pointer {
                "RemoteDesktop"
            } else {
                "ScreenCast"
            },
            "CreateSession",
            vec![],
            Results::from_iter([("session_handle_token".into(), token.to_variant())]),
            &session.scope.token,
        )
        .await?
        else {
            session.close().await;
            return Ok(None);
        };
        if created.get("session_handle").and_then(|v| v.str()) != Some(&session.path) {
            if let Some(path) = created
                .get("session_handle")
                .and_then(|v| v.str())
                .filter(|p| glib::Variant::is_object_path(p))
            {
                connection.call(
                    Some(portal::DESTINATION),
                    path,
                    "org.freedesktop.portal.Session",
                    "Close",
                    None,
                    None,
                    gio::DBusCallFlags::NONE,
                    5000,
                    gio::Cancellable::NONE,
                    |_| {},
                );
            }
            return Err(t("The system returned an unexpected capture session"));
        }
        if pointer
            && session
                .request(
                    "RemoteDesktop",
                    "SelectDevices",
                    vec![object_path(&session.path)],
                    Results::from_iter([("types".into(), 2u32.to_variant())]),
                )
                .await?
                .is_none()
        {
            session.close().await;
            return Ok(None);
        }
        if session
            .request(
                "ScreenCast",
                "SelectSources",
                vec![object_path(&session.path)],
                Results::from_iter([
                    ("types".into(), types.to_variant()),
                    ("multiple".into(), multiple.to_variant()),
                    ("cursor_mode".into(), cursor_mode.to_variant()),
                ]),
            )
            .await?
            .is_none()
        {
            session.close().await;
            return Ok(None);
        }
        let Some(result) = session
            .request(
                if pointer {
                    "RemoteDesktop"
                } else {
                    "ScreenCast"
                },
                "Start",
                vec![object_path(&session.path), "".to_variant()],
                Results::new(),
            )
            .await?
        else {
            session.close().await;
            return Ok(None);
        };
        if pointer
            && result
                .get("devices")
                .and_then(|v| v.get::<u32>())
                .is_none_or(|v| v & 2 == 0)
        {
            return Err(t("Pointer access is required for automatic scrolling"));
        }
        let streams = result
            .get("streams")
            .and_then(|v| v.get::<Vec<(u32, Results)>>())
            .filter(|v| !v.is_empty() && v.len() <= 16)
            .ok_or_else(|| t("The system returned no capture stream"))?;
        session.streams = streams
            .into_iter()
            .map(|(node, properties)| {
                let rect = properties
                    .get("position")
                    .and_then(|v| v.get::<(i32, i32)>())
                    .zip(properties.get("size").and_then(|v| v.get::<(i32, i32)>()))
                    .filter(|(_, s)| s.0 > 0 && s.1 > 0)
                    .map(|((x, y), (w, h))| Bounds {
                        x: x as f64,
                        y: y as f64,
                        width: w as f64,
                        height: h as f64,
                    });
                Stream {
                    node,
                    rect,
                    properties,
                }
            })
            .collect();
        if session.scope.token.is_cancelled() {
            return Err(t("Operation cancelled"));
        }
        Ok(Some(session))
    }
    /// Portal backends such as xdg-desktop-portal-wlr may omit logical stream
    /// coordinates on a single-output monitor cast. Never guess across multiple
    /// monitors or for window streams.
    pub fn fallback_single_monitor(&mut self, outputs: &[clivuu_core::capture::Output]) {
        if matches!(self.backend, Backend::Mutter)
            || outputs.len() != 1
            || self.streams.len() != 1
            || self.streams[0].rect.is_some()
            || self.streams[0]
                .properties
                .get("source_type")
                .and_then(|v| v.get::<u32>())
                == Some(2)
        {
            return;
        }
        let output = outputs[0].rect;
        if let Some((w, h)) = self.streams[0]
            .properties
            .get("size")
            .and_then(|v| v.get::<(i32, i32)>())
            .filter(|(w, h)| *w > 0 && *h > 0)
            && ((f64::from(w) - output.width).abs() > 1.
                || (f64::from(h) - output.height).abs() > 1.)
        {
            return;
        }
        self.streams[0].rect = Some(output);
    }
    pub async fn mutter(
        connection: &gio::DBusConnection,
        cancel: &gio::Cancellable,
        rect: Bounds,
        cursor: bool,
    ) -> Result<Self, String> {
        Self::mutter_modes(connection, cancel, rect, &[if cursor { 1 } else { 0 }]).await
    }
    /// Starts embedded-cursor and clean cursor-hidden streams in one Mutter
    /// session so interactive capture can toggle the real cursor without a
    /// second sharing handle, portal prompt or shutter flash.
    pub async fn mutter_pair(
        connection: &gio::DBusConnection,
        cancel: &gio::Cancellable,
        rect: Bounds,
        include_clean: bool,
    ) -> Result<Self, String> {
        if include_clean {
            Self::mutter_modes(connection, cancel, rect, &[1, 0]).await
        } else {
            Self::mutter_modes(connection, cancel, rect, &[1]).await
        }
    }
    async fn mutter_modes(
        connection: &gio::DBusConnection,
        cancel: &gio::Cancellable,
        rect: Bounds,
        cursor_modes: &[u32],
    ) -> Result<Self, String> {
        if cursor_modes.is_empty() {
            return Err(t("Invalid monitor layout"));
        }
        let outputs = crate::platform::integration::layout::mutter_outputs(connection, cancel)
            .await
            .unwrap_or_default();
        let Some((area_x, area_y, area_width, area_height, area)) = mutter_area(rect, &outputs)
        else {
            return Err(t("Invalid monitor layout"));
        };
        let mut session = Self::new(connection, Backend::Mutter, cancel);
        // Receive the owned path even if cancellation races creation. The compositor
        // also ties this session to our unique bus owner, never to another client.
        let reply = connection
            .call_future(
                Some(MUTTER),
                ROOT,
                MUTTER,
                "CreateSession",
                Some(&(Results::new(),).to_variant()),
                None,
                gio::DBusCallFlags::NO_AUTO_START,
                10000,
            )
            .await
            .map_err(|e| e.to_string())?;
        session.path = reply
            .get::<(glib::variant::ObjectPath,)>()
            .ok_or_else(|| t("Invalid desktop response"))?
            .0
            .to_string();
        session.observe();
        if session.scope.token.is_cancelled() {
            return Err(t("Operation cancelled"));
        }
        let mut receivers = Vec::with_capacity(cursor_modes.len());
        let mut subscriptions = Vec::with_capacity(cursor_modes.len());
        for (idx, &mode) in cursor_modes.iter().enumerate() {
            let properties = Results::from_iter([("cursor-mode".into(), mode.to_variant())]);
            let reply = session
                .call(
                    &session.path,
                    &format!("{MUTTER}.Session"),
                    "RecordArea",
                    Some(&(area_x, area_y, area_width, area_height, properties).to_variant()),
                )
                .await;
            let reply = match reply {
                Ok(r) => r,
                Err(_) if idx > 0 => continue,
                Err(e) => return Err(e),
            };
            let path = reply
                .get::<(glib::variant::ObjectPath,)>()
                .ok_or_else(|| t("Invalid desktop response"))?
                .0
                .to_string();
            let (tx, rx) = async_channel::bounded(1);
            let signal = session.connection.subscribe_to_signal(
                Some(MUTTER),
                Some(&format!("{MUTTER}.Stream")),
                Some("PipeWireStreamAdded"),
                Some(&path),
                None,
                gio::DBusSignalFlags::NONE,
                move |signal| {
                    let result = signal
                        .parameters
                        .get::<(u32,)>()
                        .map(|v| v.0)
                        .ok_or_else(|| t("The system returned no capture stream"));
                    let _ = tx.try_send(result);
                },
            );
            receivers.push((idx == 0, rx));
            subscriptions.push(signal);
        }
        let timeout_fired = Rc::new(Cell::new(false));
        let fired = timeout_fired.clone();
        let token = session.scope.token.clone();
        let timer = glib::timeout_add_local_once(std::time::Duration::from_secs(15), move || {
            fired.set(true);
            token.cancel();
        });
        let result = gio::CancellableFuture::new(
            async {
                session
                    .call(&session.path, &format!("{MUTTER}.Session"), "Start", None)
                    .await?;
                let mut nodes = Vec::with_capacity(receivers.len());
                for (required, rx) in receivers {
                    match rx.recv().await {
                        Ok(Ok(node)) => nodes.push(node),
                        Ok(Err(e)) if required => return Err(e),
                        Err(_) if required => return Err(t("The capture stream timed out")),
                        _ => {}
                    }
                }
                Ok(nodes)
            },
            session.scope.token.clone(),
        )
        .await;
        if !timeout_fired.get() {
            timer.remove();
        }
        drop(subscriptions);
        let nodes = result.map_err(|_| {
            if timeout_fired.get() {
                t("The capture stream timed out")
            } else {
                t("Operation cancelled")
            }
        })??;
        session.streams = nodes
            .into_iter()
            .map(|node| Stream {
                node,
                rect: Some(area),
                properties: Results::new(),
            })
            .collect();
        Ok(session)
    }
    async fn request(
        &self,
        interface: &str,
        method: &str,
        arguments: Vec<glib::Variant>,
        options: Results,
    ) -> Result<Option<Results>, String> {
        portal::request_optional(
            &self.connection,
            portal::DESTINATION,
            interface,
            method,
            arguments,
            options,
            &self.scope.token,
        )
        .await
    }
    async fn call(
        &self,
        path: &str,
        interface: &str,
        method: &str,
        args: Option<&glib::Variant>,
    ) -> Result<glib::Variant, String> {
        gio::CancellableFuture::new(
            self.connection.call_future(
                Some(self.destination()),
                path,
                interface,
                method,
                args,
                None,
                gio::DBusCallFlags::NO_AUTO_START,
                10000,
            ),
            self.scope.token.clone(),
        )
        .await
        .map_err(|_| t("Operation cancelled"))?
        .map_err(|e| e.to_string())
    }
    pub async fn remote(&self) -> Result<Option<OwnedFd>, String> {
        if matches!(self.backend, Backend::Mutter) {
            return Ok(None);
        }
        let (reply, list) = gio::CancellableFuture::new(
            self.connection.call_with_unix_fd_list_future(
                Some(portal::DESTINATION),
                portal::PATH,
                "org.freedesktop.portal.ScreenCast",
                "OpenPipeWireRemote",
                Some(
                    &(
                        glib::variant::ObjectPath::try_from(self.path.as_str()).unwrap(),
                        Results::new(),
                    )
                        .to_variant(),
                ),
                None,
                gio::DBusCallFlags::NONE,
                10000,
                None::<&gio::UnixFDList>,
            ),
            self.scope.token.clone(),
        )
        .await
        .map_err(|_| t("Operation cancelled"))?
        .map_err(|e| e.to_string())?;
        let index = reply
            .child_value(0)
            .get::<glib::variant::Handle>()
            .ok_or_else(|| t("Invalid desktop response"))?;
        let fd = list
            .ok_or_else(|| t("Invalid desktop response"))?
            .get(index.into())
            .map_err(|e| e.to_string())?;
        Ok(Some(fd))
    }
    /// Scroll at the real pointer. Never reposition it as a side effect.
    pub async fn scroll(&self, axis: u32, steps: i32) -> Result<(), String> {
        if !matches!(self.backend, Backend::Portal { pointer: true }) {
            return Err(t("Pointer access is required for automatic scrolling"));
        }
        self.call(
            portal::PATH,
            "org.freedesktop.portal.RemoteDesktop",
            "NotifyPointerAxisDiscrete",
            Some(
                &(
                    glib::variant::ObjectPath::try_from(self.path.as_str()).unwrap(),
                    Results::new(),
                    axis,
                    steps,
                )
                    .to_variant(),
            ),
        )
        .await?;
        Ok(())
    }
    /// Continuous axis delta without `finish = true`: compositors map portal
    /// smooth deltas to `wl_pointer.axis_source.finger`, where an immediate
    /// `axis_stop` synthesizes a high-velocity flick/kinetic fling or resets
    /// sub-detent scroll accumulators on small selections.
    pub async fn scroll_smooth(&self, dx: f64, dy: f64) -> Result<(), String> {
        if !matches!(self.backend, Backend::Portal { pointer: true }) {
            return Err(t("Pointer access is required for automatic scrolling"));
        }
        let session = glib::variant::ObjectPath::try_from(self.path.as_str()).unwrap();
        self.call(
            portal::PATH,
            "org.freedesktop.portal.RemoteDesktop",
            "NotifyPointerAxis",
            Some(&(session, Results::new(), dx, dy).to_variant()),
        )
        .await?;
        Ok(())
    }
    pub async fn close(&self) {
        if self.closing.replace(true) {
            return;
        }
        self.scope.token.cancel();
        let (path, interface, method) = self.stop_arguments();
        let _ = self
            .connection
            .call_future(
                Some(self.destination()),
                &path,
                &interface,
                method,
                None,
                None,
                gio::DBusCallFlags::NO_AUTO_START,
                5000,
            )
            .await;
    }
}
fn object_path(path: &str) -> glib::Variant {
    glib::variant::ObjectPath::try_from(path)
        .unwrap()
        .to_variant()
}

pub(crate) fn mutter_area(
    rect: Bounds,
    outputs: &[clivuu_core::capture::Output],
) -> Option<(i32, i32, i32, i32, Bounds)> {
    const EPS: f64 = 1e-9;
    if ![
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        rect.x + rect.width,
        rect.y + rect.height,
    ]
    .iter()
    .all(|v| v.is_finite())
        || rect.width <= 0.
        || rect.height <= 0.
    {
        return None;
    }
    let owner = outputs.iter().copied().find(|o| {
        rect.x >= o.rect.x - EPS
            && rect.y >= o.rect.y - EPS
            && rect.x + rect.width <= o.rect.x + o.rect.width + EPS
            && rect.y + rect.height <= o.rect.y + o.rect.height + EPS
    });
    let (left, top, right, bottom) = if let Some(m) = owner {
        let q = (1..=16)
            .find(|&q| {
                let qf = f64::from(q);
                let scaled = m.scale * qf;
                (scaled - scaled.round()).abs() < 1e-6
                    && ((m.rect.width / qf) - (m.rect.width / qf).round()).abs() < 1e-6
                    && ((m.rect.height / qf) - (m.rect.height / qf).round()).abs() < 1e-6
            })
            .unwrap_or(1) as f64;
        let mx = m.rect.x.round();
        let my = m.rect.y.round();
        let mr = (m.rect.x + m.rect.width).round();
        let mb = (m.rect.y + m.rect.height).round();
        let l = (mx + ((rect.x - mx + EPS) / q).floor() * q).clamp(mx, mr - 1.);
        let t = (my + ((rect.y - my + EPS) / q).floor() * q).clamp(my, mb - 1.);
        let r = (mx + ((rect.x + rect.width - mx - EPS) / q).ceil() * q)
            .max(l + q)
            .clamp(l + 1., mr);
        let b = (my + ((rect.y + rect.height - my - EPS) / q).ceil() * q)
            .max(t + q)
            .clamp(t + 1., mb);
        (l, t, r, b)
    } else {
        let l = (rect.x + EPS).floor();
        let t = (rect.y + EPS).floor();
        let r = (rect.x + rect.width - EPS).ceil().max(l + 1.);
        let b = (rect.y + rect.height - EPS).ceil().max(t + 1.);
        (l, t, r, b)
    };
    let width = right - left;
    let height = bottom - top;
    if !(f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(&left)
        || !(f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(&top)
        || !(1. ..=f64::from(i32::MAX)).contains(&width)
        || !(1. ..=f64::from(i32::MAX)).contains(&height)
    {
        return None;
    }
    Some((
        left as i32,
        top as i32,
        width as i32,
        height as i32,
        Bounds {
            x: left,
            y: top,
            width,
            height,
        },
    ))
}

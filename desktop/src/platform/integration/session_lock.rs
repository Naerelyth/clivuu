//! Session lock lifecycle through screen-saver APIs, independent of extensions.
use gtk::{gio, glib, prelude::*};
use std::{cell::Cell, rc::Rc};
const SERVICES: [(&str, &str, &str); 2] = [
    (
        "org.freedesktop.ScreenSaver",
        "/ScreenSaver",
        "org.freedesktop.ScreenSaver",
    ),
    (
        "org.gnome.ScreenSaver",
        "/org/gnome/ScreenSaver",
        "org.gnome.ScreenSaver",
    ),
];
struct State {
    active: [Cell<bool>; 2],
    epoch: [Cell<u64>; 2],
    changed: Box<dyn Fn(bool)>,
}
impl State {
    fn update(&self, index: usize, active: bool) {
        let before = self.active.iter().any(Cell::get);
        self.active[index].set(active);
        let after = self.active.iter().any(Cell::get);
        if before != after {
            (self.changed)(after);
        }
    }
    fn refresh(
        self: &Rc<Self>,
        connection: &gio::DBusConnection,
        cancel: &gio::Cancellable,
        index: usize,
        destination: &str,
    ) {
        let peer = connection.clone();
        let token = cancel.clone();
        let weak = Rc::downgrade(self);
        let epoch = self.epoch[index].get();
        let destination = destination.to_owned();
        let (_, path, interface) = SERVICES[index];
        // NO_AUTO_START: do not start a screen saver merely to query it.
        glib::MainContext::default().spawn_local(async move {
            if let Ok(Ok(reply)) = gio::CancellableFuture::new(
                peer.call_future(
                    Some(&destination),
                    path,
                    interface,
                    "GetActive",
                    None,
                    None,
                    gio::DBusCallFlags::NO_AUTO_START,
                    1500,
                ),
                token,
            )
            .await
                && let Some((active,)) = reply.get::<(bool,)>()
                && let Some(state) = weak.upgrade()
                && state.epoch[index].get() == epoch
            {
                state.update(index, active);
            }
        });
    }
}
pub struct SessionLock {
    cancel: gio::Cancellable,
    _state: Rc<State>,
    _signals: Vec<gio::SignalSubscription>,
}
impl SessionLock {
    pub fn new(connection: &gio::DBusConnection, changed: impl Fn(bool) + 'static) -> Self {
        let cancel = gio::Cancellable::new();
        let state = Rc::new(State {
            active: Default::default(),
            epoch: Default::default(),
            changed: Box::new(changed),
        });
        let mut signals = vec![];
        for (index, (name, path, interface)) in SERVICES.into_iter().enumerate() {
            let weak = Rc::downgrade(&state);
            signals.push(connection.subscribe_to_signal(
                Some(name),
                Some(interface),
                Some("ActiveChanged"),
                Some(path),
                None,
                gio::DBusSignalFlags::NONE,
                move |signal| {
                    if let Some(state) = weak.upgrade()
                        && let Some((active,)) = signal.parameters.get::<(bool,)>()
                    {
                        state.epoch[index].set(state.epoch[index].get().wrapping_add(1));
                        state.update(index, active);
                    }
                },
            ));
            let peer = connection.clone();
            let token = cancel.clone();
            let weak = Rc::downgrade(&state);
            signals.push(connection.subscribe_to_signal(
                Some("org.freedesktop.DBus"),
                Some("org.freedesktop.DBus"),
                Some("NameOwnerChanged"),
                Some("/org/freedesktop/DBus"),
                Some(name),
                gio::DBusSignalFlags::NONE,
                move |signal| {
                    if let Some(state) = weak.upgrade()
                        && let Some((_, _, owner)) =
                            signal.parameters.get::<(String, String, String)>()
                    {
                        state.epoch[index].set(state.epoch[index].get().wrapping_add(1));
                        // Keep locked pins hidden during a provider restart.
                        // Only the replacement's own reply may unlock them.
                        if !owner.is_empty() {
                            state.refresh(&peer, &token, index, &owner);
                        }
                    }
                },
            ));
            state.refresh(connection, &cancel, index, name);
        }
        Self {
            cancel,
            _state: state,
            _signals: signals,
        }
    }
}
impl Drop for SessionLock {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

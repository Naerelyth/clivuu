//! An independently cancellable session token linked to the application token.
use gtk::{gio, prelude::*};
pub struct Scope {
    pub token: gio::Cancellable,
    parent: gio::Cancellable,
    handler: Option<gio::CancelledHandlerId>,
}
impl Scope {
    pub fn new(parent: &gio::Cancellable) -> Self {
        let token = gio::Cancellable::new();
        let child = token.clone();
        let handler = parent.connect_cancelled(move |_| child.cancel());
        Self {
            token,
            parent: parent.clone(),
            handler,
        }
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        self.token.cancel();
        if let Some(id) = self.handler.take() {
            self.parent.disconnect_cancelled(id);
        }
    }
}

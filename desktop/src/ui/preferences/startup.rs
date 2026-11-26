//! Login startup reflects the XDG entry itself; no competing config.ini flag.
use super::*;
use crate::{
    infrastructure::tasks::TaskError,
    platform::integration::{autostart::Autostart, launch::Launch},
};
impl Preferences {
    pub(super) fn build_startup(self: &Rc<Self>, general: &adw::PreferencesPage) {
        let group = self.group(general, "Startup", "");
        let row = adw::SwitchRow::new();
        row.set_widget_name("run-at-login");
        self.text(&row, "title", "Run at login");
        self.text(
            &row,
            "subtitle",
            "Start Clivuu in the background when you log in to your desktop session",
        );
        row.set_sensitive(false);
        group.add(&row);
        let busy = Rc::new(Cell::new(true));
        let pool = self.config.pool();
        let weak = Rc::downgrade(self);
        let pending = busy.clone();
        let switch = row.downgrade();
        glib::MainContext::default().spawn_local(async move {
            let result =
                match pool.submit(|_| Autostart::default().enabled().map_err(TaskError::Failed)) {
                    Ok(task) => task.result().await.map_err(|e| e.to_string()),
                    Err(e) => Err(e.to_string()),
                };
            if let (Some(this), Some(row)) = (weak.upgrade(), switch.upgrade()) {
                match result {
                    Ok(enabled) => row.set_active(enabled),
                    Err(error) => this.toasts.add_toast(adw::Toast::new(&error)),
                }
                pending.set(false);
                row.set_sensitive(true);
            }
        });
        let weak = Rc::downgrade(self);
        row.connect_active_notify(move |row| {
            if busy.replace(true) {
                return;
            }
            let Some(this) = weak.upgrade() else {
                return;
            };
            let desired = row.is_active();
            row.set_sensitive(false);
            // Keep the native preferences process alive if its last window is
            // closed immediately after this explicit OS-state write.
            let hold = this.window.application().map(|app| app.hold());
            let pool = this.config.pool();
            let weak = weak.clone();
            let switch = row.downgrade();
            let busy = busy.clone();
            glib::MainContext::default().spawn_local(async move {
                let result = match pool.submit(move |_| {
                    let launch = desired
                        .then(Launch::current)
                        .transpose()
                        .map_err(TaskError::Failed)?;
                    Autostart::default()
                        .set(desired, launch.as_ref())
                        .map_err(TaskError::Failed)
                }) {
                    Ok(task) => task.result().await.map_err(|e| e.to_string()),
                    Err(e) => Err(e.to_string()),
                };
                if let (Some(this), Some(row)) = (weak.upgrade(), switch.upgrade()) {
                    match result {
                        Ok(enabled) => row.set_active(enabled),
                        Err(error) => {
                            row.set_active(!desired);
                            this.toasts.add_toast(adw::Toast::new(&error));
                        }
                    }
                    busy.set(false);
                    row.set_sensitive(true);
                }
                drop(hold);
            });
        });
    }
}

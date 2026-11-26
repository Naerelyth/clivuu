//! Separate source artwork, optional saved-image previews and file actions.
//! Serialized async delivery preserves replacement IDs without blocking GTK.
use crate::infrastructure::tasks::TaskPool;
use clivuu_core::saving::Media;
use gtk::{gio, glib, prelude::*};
use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
};

struct Attachment {
    path: PathBuf,
    media: Media,
}
struct Message {
    key: String,
    title: String,
    body: String,
    file: Option<Attachment>,
}
#[derive(Default)]
struct Delivery {
    owner: Option<String>,
    ids: HashMap<String, u32>,
    files: HashMap<u32, gio::File>,
}
impl Delivery {
    fn owner(&mut self, owner: Option<String>) {
        if self.owner != owner || owner.is_none() {
            self.ids.clear();
            self.files.clear();
        }
        self.owner = owner;
    }
}
pub struct Notifications {
    sender: async_channel::Sender<Message>,
    _signals: Vec<gio::SignalSubscription>,
}
impl Notifications {
    pub fn new(connection: &gio::DBusConnection, pool: Rc<TaskPool>) -> Self {
        let delivery = Rc::new(RefCell::new(Delivery::default()));
        let mut signals = Vec::new();
        let state = delivery.clone();
        signals.push(connection.subscribe_to_signal(
            Some("org.freedesktop.DBus"),
            Some("org.freedesktop.DBus"),
            Some("NameOwnerChanged"),
            Some("/org/freedesktop/DBus"),
            Some("org.freedesktop.Notifications"),
            gio::DBusSignalFlags::NONE,
            move |signal| {
                if let Some((_, _, owner)) = signal.parameters.get::<(String, String, String)>() {
                    state
                        .borrow_mut()
                        .owner((!owner.is_empty()).then_some(owner));
                }
            },
        ));
        let state = delivery.clone();
        let peer = connection.clone();
        signals.push(connection.subscribe_to_signal(
            Some("org.freedesktop.Notifications"),
            Some("org.freedesktop.Notifications"),
            Some("ActionInvoked"),
            Some("/org/freedesktop/Notifications"),
            None,
            gio::DBusSignalFlags::NONE,
            move |signal| {
                let Some((id, action)) = signal.parameters.get::<(u32, String)>() else {
                    return;
                };
                if action != "show-in-files" {
                    return;
                }
                let file = {
                    let mut state = state.borrow_mut();
                    if state.owner.as_deref() != Some(signal.sender_name) {
                        return;
                    }
                    state.files.remove(&id)
                };
                if let Some(file) = file {
                    let peer = peer.clone();
                    glib::MainContext::default().spawn_local(async move {
                        if let Err(error) = super::files::show(&peer, &file).await {
                            eprintln!("Clivuu notification action: {error}");
                        }
                    });
                }
            },
        ));
        let state = delivery.clone();
        signals.push(connection.subscribe_to_signal(
            Some("org.freedesktop.Notifications"),
            Some("org.freedesktop.Notifications"),
            Some("NotificationClosed"),
            Some("/org/freedesktop/Notifications"),
            None,
            gio::DBusSignalFlags::NONE,
            move |signal| {
                if let Some((id, _)) = signal.parameters.get::<(u32, u32)>() {
                    let mut state = state.borrow_mut();
                    if state.owner.as_deref() == Some(signal.sender_name) {
                        state.files.remove(&id);
                    }
                }
            },
        ));
        let (sender, receiver) = async_channel::bounded::<Message>(16);
        let connection = connection.clone();
        glib::MainContext::default().spawn_local(async move {
            while let Ok(message) = receiver.recv().await {
                let mut hints = HashMap::new();
                if let Some(file) = message.file.as_ref().filter(|f| f.media == Media::Image)
                    && let Some(preview) = thumbnail(&pool, &file.path).await
                {
                    hints.insert("image-data".to_owned(), preview);
                }
                let owner = daemon_owner(&connection).await;
                delivery.borrow_mut().owner(owner.clone());
                let icon = super::icons::notification();
                let actions = if message.file.is_some() {
                    vec!["show-in-files".to_owned(), super::files::show_label()]
                } else {
                    vec![]
                };
                // desktop-entry keeps the source associated with the launcher;
                // app_icon supplies the same themed SVG when none is installed.
                if let Some(id) = super::identity::desktop_id() {
                    hints.insert("desktop-entry".to_owned(), id.to_variant());
                }
                let parameters = (
                    "Clivuu",
                    delivery
                        .borrow()
                        .ids
                        .get(&message.key)
                        .copied()
                        .unwrap_or(0),
                    icon.as_str(),
                    message.title,
                    glib::markup_escape_text(&message.body).to_string(),
                    actions,
                    hints,
                    -1i32,
                )
                    .to_variant();
                match connection
                    .call_future(
                        Some(owner.as_deref().unwrap_or("org.freedesktop.Notifications")),
                        "/org/freedesktop/Notifications",
                        "org.freedesktop.Notifications",
                        "Notify",
                        Some(&parameters),
                        Some(glib::VariantTy::new("(u)").unwrap()),
                        gio::DBusCallFlags::NONE,
                        5000,
                    )
                    .await
                {
                    Ok(reply) => {
                        // Resolve an auto-activated daemon before accepting its actions.
                        let accepted_owner = if owner.is_none() {
                            daemon_owner(&connection).await
                        } else {
                            owner
                        };
                        let mut state = delivery.borrow_mut();
                        if state.owner != accepted_owner && state.owner.is_some() {
                            continue;
                        }
                        state.owner = accepted_owner;
                        let id = reply.get::<(u32,)>().unwrap().0;
                        if let Some(old) = state.ids.insert(message.key, id) {
                            state.files.remove(&old);
                        }
                        if let Some(file) = message.file {
                            state.files.insert(id, gio::File::for_path(file.path));
                        }
                    }
                    Err(error) => {
                        delivery.borrow_mut().owner(None);
                        eprintln!("Clivuu notification: {error}");
                    }
                }
            }
        });
        Self {
            sender,
            _signals: signals,
        }
    }
    pub fn send(&self, key: &str, title: &str, body: &str) {
        self.enqueue(key, title, body, None);
    }
    pub fn send_file(&self, key: &str, title: &str, body: &str, path: &Path, media: Media) {
        self.enqueue(
            key,
            title,
            body,
            Some(Attachment {
                path: path.into(),
                media,
            }),
        );
    }
    fn enqueue(&self, key: &str, title: &str, body: &str, file: Option<Attachment>) {
        if let Err(error) = self.sender.try_send(Message {
            key: key.into(),
            title: title.into(),
            body: body.into(),
            file,
        }) {
            eprintln!("Clivuu notification: {error}");
        }
    }
}
async fn daemon_owner(connection: &gio::DBusConnection) -> Option<String> {
    connection
        .call_future(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "GetNameOwner",
            Some(&("org.freedesktop.Notifications",).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            5000,
        )
        .await
        .ok()
        .and_then(|reply| reply.get::<(String,)>().map(|v| v.0))
}

async fn thumbnail(pool: &TaskPool, path: &Path) -> Option<glib::Variant> {
    use crate::infrastructure::tasks::TaskError;
    loop {
        let path = path.to_owned();
        match pool.submit(move |token| {
            token.check()?;
            let image = clivuu_core::image_ops::load(path.to_str().unwrap_or(""))
                .map_err(|e| TaskError::Failed(e.to_string()))?;
            token.check()?;
            let edge = image.width().max(image.height()).max(160) as u64;
            let width = (u64::from(image.width()) * 160 / edge).max(1) as u32;
            let height = (u64::from(image.height()) * 160 / edge).max(1) as u32;
            let image = clivuu_core::image::imageops::thumbnail(&image, width, height);
            Ok((
                image.width() as i32,
                image.height() as i32,
                image.into_raw(),
            ))
        }) {
            Ok(task) => {
                return task
                    .result()
                    .await
                    .ok()
                    .map(|(w, h, pixels)| (w, h, w * 4, true, 8i32, 4i32, pixels).to_variant());
            }
            Err(TaskError::Busy) => {
                glib::timeout_future(std::time::Duration::from_millis(25)).await
            }
            Err(_) => return None,
        }
    }
}

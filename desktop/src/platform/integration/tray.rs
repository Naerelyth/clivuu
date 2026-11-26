//! StatusNotifierItem/DBusMenu are fully exported before host registration.
//! A watcher owner generation owns every retry, so a late/restarted panel gets
//! fresh scalable icons and the current complete menu, never a login cache.
use clivuu_core::{
    i18n::{t, tf},
    pins::{DEFAULT_GROUP, GroupSummary, group_display_name},
};
use gtk::{gio, glib, prelude::*};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};
const ITEM: &str = "org.kde.StatusNotifierItem";
const MENU: &str = "com.canonical.dbusmenu";
const PIN_GROUPS_CMD: &str = "@pin-groups";
const PIN_RECOVERY_CMD: &str = "@pin-recovery";
const SUB_SEP_1: i32 = 1001;
const SUB_CREATE_GROUP: i32 = 1002;
const SUB_SWITCH_PREV: i32 = 1003;
const SUB_EMPTY_GROUP: i32 = 1004;
const SUB_REMOVE_EMPTY: i32 = 1005;
const SUB_SEP_2: i32 = 1006;
const SUB_MANAGE_GROUPS: i32 = 1007;
const REC_RECENT_CAPTURE: i32 = 1101;
const REC_REOPEN_LAST: i32 = 1102;
const REC_REOPEN_ALL: i32 = 1103;
const REC_SEP: i32 = 1104;
const REC_CLOSE_ALL: i32 = 1105;
const REC_DESTROY_CLOSED: i32 = 1106;
const SUB_GROUP_BASE: i32 = 2000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TrayState {
    pub closed_count: usize,
    pub active_pins: usize,
    pub has_click_through: bool,
    pub has_recent_capture: bool,
    pub recording: bool,
    pub pausable: bool,
}

#[derive(Clone, Copy)]
struct Entry {
    label: &'static str,
    command: &'static str,
    mode: Option<&'static str>,
}
const ENTRIES: &[Entry] = &[
    Entry {
        label: "Capture",
        command: "snip",
        mode: None,
    },
    Entry {
        label: "Window capture",
        command: "snip",
        mode: Some("window"),
    },
    Entry {
        label: "Region recording",
        command: "record",
        mode: None,
    },
    Entry {
        label: "Pause / resume recording",
        command: "pause-recording",
        mode: None,
    },
    Entry {
        label: "Stop recording",
        command: "stop-recording",
        mode: None,
    },
    SEP,
    Entry {
        label: "Pin clipboard",
        command: "paste",
        mode: None,
    },
    Entry {
        label: "Show / hide pins",
        command: "toggle-images",
        mode: None,
    },
    Entry {
        label: "Restore pointer interaction",
        command: "no-click-through",
        mode: None,
    },
    Entry {
        label: "Pin groups",
        command: PIN_GROUPS_CMD,
        mode: None,
    },
    Entry {
        label: "Pin recovery & cleanup",
        command: PIN_RECOVERY_CMD,
        mode: None,
    },
    SEP,
    Entry {
        label: "Preferences",
        command: "open-preferences",
        mode: None,
    },
    Entry {
        label: "Restart",
        command: "restart",
        mode: None,
    },
    Entry {
        label: "Quit",
        command: "exit",
        mode: None,
    },
];
const SEP: Entry = Entry {
    label: "",
    command: "",
    mode: None,
};
pub struct Tray {
    connection: gio::DBusConnection,
    registrations: RefCell<Vec<gio::RegistrationId>>,
    unwatch: RefCell<Option<Box<dyn FnOnce()>>>,
    retry: RefCell<Option<glib::SourceId>>,
    generation: Cell<u64>,
    revision: Cell<u32>,
    state: Cell<TrayState>,
    groups: RefCell<Vec<GroupSummary>>,
    has_empty_groups: Cell<bool>,
    icon: String,
    icons: String,
    invoke: Rc<dyn Fn(Value)>,
    cancel: gio::Cancellable,
    menu_open: Cell<bool>,
    pending_capture: RefCell<Option<Value>>,
    capture_timer: RefCell<Option<glib::SourceId>>,
}
impl Drop for Tray {
    fn drop(&mut self) {
        self.cancel.cancel();
        if let Some(id) = self.capture_timer.take() {
            id.remove();
        }
        if let Some(id) = self.retry.take() {
            id.remove();
        }
        if let Some(f) = self.unwatch.take() {
            f();
        }
        for id in self.registrations.take() {
            let _ = self.connection.unregister_object(id);
        }
    }
}
impl Tray {
    pub fn new(
        connection: &gio::DBusConnection,
        invoke: impl Fn(Value) + 'static,
    ) -> Result<Rc<Self>, String> {
        let (icons, icon) = icon_location();
        let this = Rc::new(Self {
            connection: connection.clone(),
            registrations: RefCell::new(vec![]),
            unwatch: RefCell::new(None),
            retry: RefCell::new(None),
            generation: Cell::new(0),
            revision: Cell::new(1),
            state: Cell::new(TrayState::default()),
            groups: RefCell::new(vec![GroupSummary {
                name: DEFAULT_GROUP.into(),
                display_name: group_display_name(DEFAULT_GROUP),
                color: None,
                active_pins: 0,
                closed_pins: 0,
                current: true,
                default: true,
            }]),
            has_empty_groups: Cell::new(false),
            icon,
            icons,
            invoke: Rc::new(invoke),
            cancel: gio::Cancellable::new(),
            menu_open: Cell::new(false),
            pending_capture: RefCell::new(None),
            capture_timer: RefCell::new(None),
        });
        for (path, interface, xml) in [
            (
                "/StatusNotifierItem",
                ITEM,
                include_str!("../../../resources/status-notifier.xml"),
            ),
            (
                "/MenuBar",
                MENU,
                include_str!("../../../resources/dbus-menu.xml"),
            ),
        ] {
            let info = gio::DBusNodeInfo::for_xml(xml)
                .map_err(|e| e.to_string())?
                .lookup_interface(interface)
                .unwrap();
            let weak = Rc::downgrade(&this);
            let property = Rc::downgrade(&this);
            let id = connection
                .register_object(path, &info)
                .method_call(move |_, _, _, _, method, args, invocation| {
                    let Some(this) = weak.upgrade() else { return };
                    let result = this.method(interface, method, &args);
                    match result {
                        Some(v) => invocation.return_value(Some(&v)),
                        None => invocation
                            .return_dbus_error("org.freedesktop.DBus.Error.UnknownMethod", method),
                    }
                })
                .property(move |_, _, _, _, name| {
                    property
                        .upgrade()
                        .map_or_else(|| "".to_variant(), |this| this.property(interface, name))
                })
                .build()
                .map_err(|e| e.to_string())?;
            this.registrations.borrow_mut().push(id);
        }
        let up = Rc::downgrade(&this);
        let down = Rc::downgrade(&this);
        let id = gio::bus_watch_name_on_connection(
            connection,
            "org.kde.StatusNotifierWatcher",
            gio::BusNameWatcherFlags::NONE,
            move |_, _, owner| {
                if let Some(this) = up.upgrade() {
                    let epoch = this.new_owner();
                    this.register(owner.to_string(), epoch, 0);
                }
            },
            move |_, _| {
                if let Some(this) = down.upgrade() {
                    this.new_owner();
                }
            },
        );
        this.unwatch
            .replace(Some(Box::new(move || gio::bus_unwatch_name(id))));
        Ok(this)
    }
    fn new_owner(&self) -> u64 {
        self.menu_open.set(false);
        self.pending_capture.borrow_mut().take();
        if let Some(id) = self.capture_timer.borrow_mut().take() {
            id.remove();
        }
        if let Some(id) = self.retry.borrow_mut().take() {
            id.remove();
        }
        let n = self.generation.get().wrapping_add(1);
        self.generation.set(n);
        n
    }
    fn register(self: &Rc<Self>, owner: String, epoch: u64, attempt: u32) {
        let weak = Rc::downgrade(self);
        let connection = self.connection.clone();
        let cancel = self.cancel.clone();
        let unique = connection.unique_name().unwrap().to_string();
        glib::MainContext::default().spawn_local(async move {
            let result = gio::CancellableFuture::new(
                connection.call_future(
                    Some(&owner),
                    "/StatusNotifierWatcher",
                    "org.kde.StatusNotifierWatcher",
                    "RegisterStatusNotifierItem",
                    Some(&(unique,).to_variant()),
                    None,
                    gio::DBusCallFlags::NONE,
                    5000,
                ),
                cancel,
            )
            .await;
            let Some(this) = weak.upgrade() else { return };
            if this.generation.get() != epoch {
                return;
            }
            if matches!(result, Ok(Ok(_))) {
                this.new_icon();
                this.refresh();
            } else {
                let weak = Rc::downgrade(&this);
                let delay = (150u64.saturating_mul(1 << attempt.min(6))).min(4000);
                this.retry.replace(Some(glib::timeout_add_local_once(
                    std::time::Duration::from_millis(delay),
                    move || {
                        if let Some(this) = weak.upgrade() {
                            this.retry.borrow_mut().take();
                            if this.generation.get() == epoch {
                                this.register(owner, epoch, attempt.saturating_add(1));
                            }
                        }
                    },
                )));
            }
        });
    }
    pub fn state(&self, next: TrayState) {
        let prev = self.state.replace(next);
        if prev.recording != next.recording {
            self.new_icon();
        }
        if prev != next {
            self.refresh();
        }
    }
    pub fn set_groups(&self, groups: Vec<GroupSummary>, has_empty: bool) {
        let empty_changed = self.has_empty_groups.replace(has_empty) != has_empty;
        let list_changed = *self.groups.borrow() != groups;
        if list_changed {
            self.groups.replace(groups);
        }
        if empty_changed || list_changed {
            self.refresh();
        }
    }
    pub fn refresh(&self) {
        self.revision.set(self.revision.get().wrapping_add(1));
        let _ = self.connection.emit_signal(
            None,
            "/MenuBar",
            MENU,
            "LayoutUpdated",
            Some(&(self.revision.get(), 0i32).to_variant()),
        );
    }
    fn new_icon(&self) {
        let _ = self
            .connection
            .emit_signal(None, "/StatusNotifierItem", ITEM, "NewIcon", None);
        let changed = HashMap::from([
            ("IconName".to_string(), self.property(ITEM, "IconName")),
            ("Title".into(), t("Clivuu").to_variant()),
        ]);
        let _ = self.connection.emit_signal(
            None,
            "/StatusNotifierItem",
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
            Some(&(ITEM, changed, Vec::<String>::new()).to_variant()),
        );
    }
    fn property(&self, interface: &str, name: &str) -> glib::Variant {
        if interface == ITEM {
            match name {
                "Category" => "ApplicationStatus".to_variant(),
                "Id" => "clivuu".to_variant(),
                "Title" => t("Clivuu").to_variant(),
                "Status" => "Active".to_variant(),
                "IconName" => if self.state.get().recording {
                    "media-record-symbolic"
                } else {
                    &self.icon
                }
                .to_variant(),
                "IconThemePath" => self.icons.to_variant(),
                "ItemIsMenu" => false.to_variant(),
                "Menu" => glib::variant::ObjectPath::try_from("/MenuBar")
                    .unwrap()
                    .to_variant(),
                _ => "".to_variant(),
            }
        } else {
            match name {
                "Version" => 4u32.to_variant(),
                "TextDirection" => "ltr".to_variant(),
                "Status" => "normal".to_variant(),
                "IconThemePath" => vec![&self.icons].to_variant(),
                _ => "".to_variant(),
            }
        }
    }
    fn submenu_parent_id(cmd: &str) -> i32 {
        ENTRIES
            .iter()
            .position(|e| e.command == cmd)
            .map(|i| i as i32 + 1)
            .expect("tray submenu entry")
    }
    fn submenu_child_ids(&self) -> Vec<i32> {
        let count = self.groups.borrow().len() as i32;
        let mut ids: Vec<i32> = (0..count).map(|i| SUB_GROUP_BASE + i).collect();
        ids.extend([
            SUB_SEP_1,
            SUB_CREATE_GROUP,
            SUB_SWITCH_PREV,
            SUB_EMPTY_GROUP,
            SUB_REMOVE_EMPTY,
            SUB_SEP_2,
            SUB_MANAGE_GROUPS,
        ]);
        ids
    }
    fn recovery_child_ids() -> [i32; 6] {
        [
            REC_RECENT_CAPTURE,
            REC_REOPEN_LAST,
            REC_REOPEN_ALL,
            REC_SEP,
            REC_CLOSE_ALL,
            REC_DESTROY_CLOSED,
        ]
    }
    fn children_of(&self, parent: i32) -> Vec<i32> {
        if parent == 0 {
            (1..=ENTRIES.len() as i32).collect()
        } else if parent == Self::submenu_parent_id(PIN_GROUPS_CMD) {
            self.submenu_child_ids()
        } else if parent == Self::submenu_parent_id(PIN_RECOVERY_CMD) {
            Self::recovery_child_ids().to_vec()
        } else {
            vec![]
        }
    }
    fn layout_node(&self, id: i32, depth: i32) -> glib::Variant {
        let children = if depth == 0 {
            vec![]
        } else {
            let next_depth = if depth < 0 { -1 } else { depth - 1 };
            self.children_of(id)
                .into_iter()
                .map(|cid| self.layout_node(cid, next_depth))
                .collect()
        };
        (id, self.properties(id), children).to_variant()
    }
    fn properties(&self, id: i32) -> HashMap<String, glib::Variant> {
        if id == 0 {
            return HashMap::from([("children-display".into(), "submenu".to_variant())]);
        }
        let st = self.state.get();
        if id >= SUB_GROUP_BASE {
            let index = (id - SUB_GROUP_BASE) as usize;
            let groups = self.groups.borrow();
            let Some(g) = groups.get(index) else {
                return HashMap::new();
            };
            let label = format!("{} ({})", group_display_name(&g.name), g.active_pins);
            return HashMap::from([
                ("label".into(), label.to_variant()),
                ("visible".into(), true.to_variant()),
                ("enabled".into(), true.to_variant()),
                ("toggle-type".into(), "radio".to_variant()),
                (
                    "toggle-state".into(),
                    if g.current { 1i32 } else { 0i32 }.to_variant(),
                ),
            ]);
        }
        match id {
            SUB_SEP_1 | SUB_SEP_2 | REC_SEP => {
                return HashMap::from([
                    ("type".into(), "separator".to_variant()),
                    ("visible".into(), true.to_variant()),
                ]);
            }
            SUB_CREATE_GROUP => {
                return HashMap::from([
                    ("label".into(), t("Create new group...").to_variant()),
                    ("visible".into(), true.to_variant()),
                    ("enabled".into(), true.to_variant()),
                ]);
            }
            SUB_SWITCH_PREV => {
                let enabled = self.groups.borrow().len() > 1;
                return HashMap::from([
                    ("label".into(), t("Switch to previous group").to_variant()),
                    ("visible".into(), true.to_variant()),
                    ("enabled".into(), enabled.to_variant()),
                ]);
            }
            SUB_EMPTY_GROUP => {
                let enabled = self
                    .groups
                    .borrow()
                    .iter()
                    .find(|g| g.current)
                    .is_some_and(|g| g.active_pins + g.closed_pins > 0);
                return HashMap::from([
                    ("label".into(), t("Empty current group").to_variant()),
                    ("visible".into(), true.to_variant()),
                    ("enabled".into(), enabled.to_variant()),
                ]);
            }
            SUB_REMOVE_EMPTY => {
                return HashMap::from([
                    ("label".into(), t("Remove empty groups").to_variant()),
                    ("visible".into(), true.to_variant()),
                    ("enabled".into(), self.has_empty_groups.get().to_variant()),
                ]);
            }
            SUB_MANAGE_GROUPS => {
                return HashMap::from([
                    ("label".into(), t("Manage groups...").to_variant()),
                    ("visible".into(), true.to_variant()),
                    ("enabled".into(), true.to_variant()),
                ]);
            }
            REC_RECENT_CAPTURE => {
                return HashMap::from([
                    ("label".into(), t("Pin most recent capture").to_variant()),
                    ("visible".into(), true.to_variant()),
                    ("enabled".into(), st.has_recent_capture.to_variant()),
                ]);
            }
            REC_REOPEN_LAST => {
                return HashMap::from([
                    ("label".into(), t("Reopen last closed pin").to_variant()),
                    ("visible".into(), true.to_variant()),
                    ("enabled".into(), (st.closed_count > 0).to_variant()),
                ]);
            }
            REC_REOPEN_ALL => {
                return HashMap::from([
                    ("label".into(), t("Reopen all closed pins").to_variant()),
                    ("visible".into(), true.to_variant()),
                    ("enabled".into(), (st.closed_count > 0).to_variant()),
                ]);
            }
            REC_CLOSE_ALL => {
                return HashMap::from([
                    ("label".into(), t("Close all pins").to_variant()),
                    ("visible".into(), true.to_variant()),
                    ("enabled".into(), (st.active_pins > 0).to_variant()),
                ]);
            }
            REC_DESTROY_CLOSED => {
                return HashMap::from([
                    ("label".into(), t("Destroy closed pins").to_variant()),
                    ("visible".into(), true.to_variant()),
                    ("enabled".into(), (st.closed_count > 0).to_variant()),
                ]);
            }
            _ => {}
        }
        let Some(e) = id.checked_sub(1).and_then(|i| ENTRIES.get(i as usize)) else {
            return HashMap::new();
        };
        if e.command.is_empty() {
            return HashMap::from([
                ("type".into(), "separator".to_variant()),
                ("visible".into(), true.to_variant()),
            ]);
        }
        if e.command == PIN_GROUPS_CMD {
            let active_name = self.groups.borrow().iter().find(|g| g.current).map_or_else(
                || group_display_name(DEFAULT_GROUP),
                |g| group_display_name(&g.name),
            );
            return HashMap::from([
                (
                    "label".into(),
                    tf("Pin groups: {0}", &[&active_name]).to_variant(),
                ),
                ("visible".into(), true.to_variant()),
                ("enabled".into(), true.to_variant()),
                ("children-display".into(), "submenu".to_variant()),
            ]);
        }
        if e.command == PIN_RECOVERY_CMD {
            let label = if st.closed_count > 0 {
                tf(
                    "Pin recovery & cleanup ({0})",
                    &[&st.closed_count.to_string()],
                )
            } else {
                t("Pin recovery & cleanup")
            };
            return HashMap::from([
                ("label".into(), label.to_variant()),
                ("visible".into(), true.to_variant()),
                ("enabled".into(), true.to_variant()),
                ("children-display".into(), "submenu".to_variant()),
            ]);
        }
        let (visible, enabled) = match e.command {
            "record" => (!st.recording, !st.recording),
            "pause-recording" => (st.recording, st.recording && st.pausable),
            "stop-recording" => (st.recording, st.recording),
            "no-click-through" => (st.has_click_through, st.has_click_through),
            _ => (true, true),
        };
        HashMap::from([
            ("label".into(), t(e.label).to_variant()),
            ("visible".into(), visible.to_variant()),
            ("enabled".into(), enabled.to_variant()),
        ])
    }
    fn click(self: &Rc<Self>, id: i32) {
        let enabled = self
            .properties(id)
            .get("enabled")
            .and_then(|v| v.get::<bool>())
            == Some(true);
        if !enabled {
            return;
        }
        let version = clivuu_core::protocol::VERSION;
        if id >= SUB_GROUP_BASE {
            let index = (id - SUB_GROUP_BASE) as usize;
            let target = self.groups.borrow().get(index).map(|g| g.name.clone());
            if let Some(name) = target {
                (self.invoke)(json!({
                    "protocol": version,
                    "command": "switch-group",
                    "argument": name
                }));
            }
            return;
        }
        match id {
            SUB_CREATE_GROUP => {
                (self.invoke)(json!({
                    "protocol": version,
                    "command": "show-group-manager",
                    "mode": "create"
                }));
                return;
            }
            SUB_SWITCH_PREV => {
                (self.invoke)(json!({"protocol": version, "command": "switch-group-prev"}));
                return;
            }
            SUB_EMPTY_GROUP => {
                (self.invoke)(json!({"protocol": version, "command": "empty-group"}));
                return;
            }
            SUB_REMOVE_EMPTY => {
                (self.invoke)(json!({"protocol": version, "command": "remove-empty-groups"}));
                return;
            }
            SUB_MANAGE_GROUPS => {
                (self.invoke)(json!({"protocol": version, "command": "show-group-manager"}));
                return;
            }
            REC_RECENT_CAPTURE => {
                (self.invoke)(json!({"protocol": version, "command": "reopen-capture"}));
                return;
            }
            REC_REOPEN_LAST => {
                (self.invoke)(json!({"protocol": version, "command": "reopen-pin"}));
                return;
            }
            REC_REOPEN_ALL => {
                (self.invoke)(json!({"protocol": version, "command": "reopen-pins"}));
                return;
            }
            REC_CLOSE_ALL => {
                (self.invoke)(json!({"protocol": version, "command": "close-pins"}));
                return;
            }
            REC_DESTROY_CLOSED => {
                (self.invoke)(json!({"protocol": version, "command": "destroy-closed-pins"}));
                return;
            }
            _ => {}
        }
        if let Some(e) = id.checked_sub(1).and_then(|i| ENTRIES.get(i as usize))
            && !e.command.is_empty()
            && !matches!(e.command, PIN_GROUPS_CMD | PIN_RECOVERY_CMD)
        {
            let mut r = json!({"protocol": version, "command": e.command});
            if let Some(mode) = e.mode {
                r["mode"] = json!(mode);
            }
            if matches!(e.command, "snip" | "record") {
                self.pending_capture.replace(Some(r));
                // The host owns the menu. Return the D-Bus reply first, then
                // let its popup disappear; root closed may arrive after clicked.
                self.schedule_capture(if self.menu_open.get() { 500 } else { 200 });
            } else {
                (self.invoke)(r);
            }
        }
    }
    fn schedule_capture(self: &Rc<Self>, delay_ms: u64) {
        if let Some(id) = self.capture_timer.borrow_mut().take() {
            id.remove();
        }
        let weak = Rc::downgrade(self);
        self.capture_timer
            .replace(Some(glib::timeout_add_local_once(
                std::time::Duration::from_millis(delay_ms),
                move || {
                    if let Some(this) = weak.upgrade() {
                        this.capture_timer.borrow_mut().take();
                        let request = this.pending_capture.borrow_mut().take();
                        if let Some(request) = request {
                            (this.invoke)(request);
                        }
                    }
                },
            )));
    }
    fn event(self: &Rc<Self>, id: i32, event: &str) {
        match (id, event) {
            (0, "opened") => {
                self.menu_open.set(true);
                // Reopening the popup cancels an activation still settling.
                self.pending_capture.borrow_mut().take();
                if let Some(id) = self.capture_timer.borrow_mut().take() {
                    id.remove();
                }
            }
            (0, "closed") => {
                self.menu_open.set(false);
                if self.pending_capture.borrow().is_some() {
                    // closed marks the start of some hosts' exit animation.
                    // Use the same settling interval across desktop backends.
                    self.schedule_capture(200);
                }
            }
            (_, "clicked") => self.click(id),
            _ => {}
        }
    }
    fn method(
        self: &Rc<Self>,
        interface: &str,
        method: &str,
        args: &glib::Variant,
    ) -> Option<glib::Variant> {
        if interface == ITEM {
            match method {
                "Activate" => (self.invoke)(json!({"protocol":1,"command":"snip"})),
                "SecondaryActivate" => (self.invoke)(json!({"protocol":1,"command":"paste"})),
                "ContextMenu" | "Scroll" => {}
                _ => return None,
            };
            return Some(().to_variant());
        }
        Some(match method {
            "GetLayout" => {
                let (parent, depth, _) = args.get::<(i32, i32, Vec<String>)>()?;
                let layout = self.layout_node(parent, depth);
                glib::Variant::tuple_from_iter([self.revision.get().to_variant(), layout])
            }
            "GetGroupProperties" => {
                let (mut ids, _) = args.get::<(Vec<i32>, Vec<String>)>()?;
                if ids.is_empty() {
                    ids = (0..=ENTRIES.len() as i32).collect();
                    ids.extend(self.submenu_child_ids());
                    ids.extend(Self::recovery_child_ids());
                }
                (ids.into_iter()
                    .map(|id| (id, self.properties(id)))
                    .collect::<Vec<_>>(),)
                    .to_variant()
            }
            "GetProperty" => {
                let (id, name) = args.get::<(i32, String)>()?;
                (self
                    .properties(id)
                    .remove(&name)
                    .unwrap_or_else(|| "".to_variant()),)
                    .to_variant()
            }
            "Event" => {
                let (id, event, _, _) = args.get::<(i32, String, glib::Variant, u32)>()?;
                self.event(id, &event);
                ().to_variant()
            }
            "EventGroup" => {
                let (events,) = args.get::<(Vec<(i32, String, glib::Variant, u32)>,)>()?;
                for (id, event, _, _) in events {
                    self.event(id, &event);
                }
                (Vec::<i32>::new(),).to_variant()
            }
            "AboutToShow" => (false,).to_variant(),
            "AboutToShowGroup" => (Vec::<i32>::new(), Vec::<i32>::new()).to_variant(),
            _ => return None,
        })
    }
}
fn icon_location() -> (String, String) {
    let icon = super::icons::transparent();
    (
        icon.parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_string_lossy()
            .into(),
        "clivoo-transparent".into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // Exercises the exported protocol on a private test bus, including a host
    // that waits for Event's reply before starting its close animation.
    #[test]
    fn capture_waits_for_menu_dismissal_without_delaying_other_tray_actions() {
        use std::{
            io::BufRead,
            process::{Child, Command, Stdio},
        };
        struct PrivateBus(Child);
        impl Drop for PrivateBus {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        // An explicit private address avoids changing the process environment
        // or registering a test icon with the user's real watcher under cargo test.
        let mut bus = PrivateBus(
            Command::new("dbus-daemon")
                .args(["--session", "--nofork", "--print-address=1"])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        );
        let mut address = String::new();
        assert!(
            std::io::BufReader::new(bus.0.stdout.take().unwrap())
                .read_line(&mut address)
                .unwrap()
                > 0,
            "dbus-daemon could not create the private test bus; local IPC is required"
        );
        let context = glib::MainContext::default();
        context.block_on(async {
            let connection = gio::DBusConnection::for_address_future(
                address.trim(),
                gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
                    | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
                None,
            )
            .await
            .unwrap();
            let calls = Rc::new(RefCell::new(Vec::new()));
            let observed = calls.clone();
            let tray = Tray::new(&connection, move |r| observed.borrow_mut().push(r)).unwrap();
            glib::timeout_future(Duration::from_millis(20)).await;
            let event = |id, name: &str| {
                connection.call_future(
                    connection.unique_name().as_deref(),
                    "/MenuBar",
                    MENU,
                    "Event",
                    Some(&(id, name, 0i32.to_variant(), 0u32).to_variant()),
                    None,
                    gio::DBusCallFlags::NONE,
                    2000,
                )
            };
            event(0, "opened").await.unwrap();
            event(1, "clicked").await.unwrap();
            assert!(
                calls.borrow().is_empty(),
                "Event must reply before acquisition"
            );
            glib::timeout_future(Duration::from_millis(240)).await;
            assert!(
                calls.borrow().is_empty(),
                "known-open host has not closed yet"
            );
            event(0, "closed").await.unwrap();
            glib::timeout_future(Duration::from_millis(120)).await;
            assert!(calls.borrow().is_empty(), "exit animation still settling");
            glib::timeout_future(Duration::from_millis(140)).await;
            assert_eq!(calls.borrow().len(), 1);
            assert_eq!(calls.borrow_mut().pop().unwrap()["command"], "snip");

            // Qt/GTK hosts can report closed before activation or in a group.
            let grouped = vec![
                (0i32, "closed", 0i32.to_variant(), 0u32),
                (2i32, "clicked", 0i32.to_variant(), 0u32),
            ];
            connection
                .call_future(
                    connection.unique_name().as_deref(),
                    "/MenuBar",
                    MENU,
                    "EventGroup",
                    Some(&(grouped,).to_variant()),
                    None,
                    gio::DBusCallFlags::NONE,
                    2000,
                )
                .await
                .unwrap();
            assert!(calls.borrow().is_empty());
            glib::timeout_future(Duration::from_millis(250)).await;
            let call = calls.borrow_mut().pop().unwrap();
            assert_eq!(call["command"], "snip");
            assert_eq!(call["mode"], "window");

            // Missing lifecycle messages must not hang a minimal SNI host.
            event(1, "clicked").await.unwrap();
            event(2, "clicked").await.unwrap();
            glib::timeout_future(Duration::from_millis(250)).await;
            assert_eq!(calls.borrow().len(), 1, "rapid activations coalesce");
            assert_eq!(calls.borrow_mut().pop().unwrap()["mode"], "window");
            let paste_id = ENTRIES.iter().position(|e| e.command == "paste").unwrap() as i32 + 1;
            event(paste_id, "clicked").await.unwrap(); // paste is not capture
            assert_eq!(calls.borrow_mut().pop().unwrap()["command"], "paste");

            let record_id = ENTRIES.iter().position(|e| e.command == "record").unwrap() as i32 + 1;
            let stop_id = ENTRIES
                .iter()
                .position(|e| e.command == "stop-recording")
                .unwrap() as i32
                + 1;
            let through_id = ENTRIES
                .iter()
                .position(|e| e.command == "no-click-through")
                .unwrap() as i32
                + 1;
            let recovery_id = Tray::submenu_parent_id(PIN_RECOVERY_CMD);
            assert_eq!(
                tray.properties(record_id)["visible"].get::<bool>(),
                Some(true)
            );
            assert_eq!(
                tray.properties(stop_id)["visible"].get::<bool>(),
                Some(false)
            );
            assert_eq!(
                tray.properties(through_id)["visible"].get::<bool>(),
                Some(false)
            );
            tray.state(TrayState {
                closed_count: 2,
                active_pins: 1,
                has_click_through: true,
                has_recent_capture: true,
                recording: true,
                pausable: true,
            });
            assert_eq!(
                tray.properties(record_id)["visible"].get::<bool>(),
                Some(false)
            );
            assert_eq!(
                tray.properties(stop_id)["visible"].get::<bool>(),
                Some(true)
            );
            assert_eq!(
                tray.properties(through_id)["visible"].get::<bool>(),
                Some(true)
            );
            assert_eq!(
                tray.properties(recovery_id)["label"]
                    .get::<String>()
                    .as_deref(),
                Some("Pin recovery & cleanup (2)")
            );
            event(REC_REOPEN_LAST, "clicked").await.unwrap();
            assert_eq!(calls.borrow_mut().pop().unwrap()["command"], "reopen-pin");

            tray.set_groups(
                vec![
                    GroupSummary {
                        name: DEFAULT_GROUP.into(),
                        display_name: "Default".into(),
                        color: None,
                        active_pins: 2,
                        closed_pins: 0,
                        current: true,
                        default: true,
                    },
                    GroupSummary {
                        name: "Review".into(),
                        display_name: "Review".into(),
                        color: Some("#bd93f9".into()),
                        active_pins: 0,
                        closed_pins: 0,
                        current: false,
                        default: false,
                    },
                ],
                true,
            );
            event(SUB_GROUP_BASE + 1, "clicked").await.unwrap();
            let switched = calls.borrow_mut().pop().unwrap();
            assert_eq!(switched["command"], "switch-group");
            assert_eq!(switched["argument"], "Review");
            event(SUB_REMOVE_EMPTY, "clicked").await.unwrap();
            assert_eq!(
                calls.borrow_mut().pop().unwrap()["command"],
                "remove-empty-groups"
            );
            event(SUB_CREATE_GROUP, "clicked").await.unwrap();
            let create = calls.borrow_mut().pop().unwrap();
            assert_eq!(create["command"], "show-group-manager");
            assert_eq!(create["mode"], "create");

            event(1, "clicked").await.unwrap();
            event(0, "opened").await.unwrap();
            glib::timeout_future(Duration::from_millis(250)).await;
            assert!(
                calls.borrow().is_empty(),
                "reopened popup cancels queued capture"
            );
            event(1, "clicked").await.unwrap();
            tray.new_owner();
            glib::timeout_future(Duration::from_millis(550)).await;
            assert!(
                calls.borrow().is_empty(),
                "host loss cancels queued capture"
            );
            event(1, "clicked").await.unwrap();
            drop(tray);
            glib::timeout_future(Duration::from_millis(250)).await;
            assert!(calls.borrow().is_empty(), "shutdown removes pending timer");
            connection.close_future().await.unwrap();
        });
    }
}

//! Native Libadwaita windows for Pin Group Management, Group Creation, and Quick Group Switching.
use crate::{
    application::{group_manager::desktop_execute, pins::group_color_dot},
    infrastructure::configuration::{Configuration, Subscription, Values},
};
use adw::prelude::*;
use clivuu_core::{
    i18n::{t, tf},
    pins::{DEFAULT_GROUP, GROUP_COLOR_PRESETS, GroupSummary, group_display_name},
};
use gtk::{gdk, gio, glib};
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Clone, Debug)]
pub enum Mode {
    Manage,
    Create { move_ids: Vec<u64> },
    Switcher,
}

#[derive(Clone, Debug, Default)]
struct Snapshot {
    current: String,
    has_empty: bool,
    groups: Vec<GroupSummary>,
}

pub struct GroupWindows {
    app: glib::WeakRef<gtk::Application>,
    connection: gio::DBusConnection,
    config: Rc<Configuration>,
    theme: gtk::CssProvider,
    theme_installed: Cell<bool>,
    theme_subscription: RefCell<Option<(adw::StyleManager, glib::SignalHandlerId)>>,
    config_subscription: RefCell<Option<Subscription>>,
    snapshot: RefCell<Snapshot>,
    manage: RefCell<Option<Rc<ManageView>>>,
    create: RefCell<Option<Rc<CreateWindow>>>,
    switcher: RefCell<Option<Rc<SwitcherView>>>,
}

impl Drop for GroupWindows {
    fn drop(&mut self) {
        if let Some((manager, id)) = self.theme_subscription.get_mut().take() {
            manager.disconnect(id);
        }
        if self.theme_installed.get()
            && let Some(display) = gdk::Display::default()
        {
            gtk::style_context_remove_provider_for_display(&display, &self.theme);
        }
    }
}

impl GroupWindows {
    pub fn new(
        app: &gtk::Application,
        connection: gio::DBusConnection,
        config: Rc<Configuration>,
    ) -> Rc<Self> {
        let theme = gtk::CssProvider::new();
        let installed = if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &theme,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
            true
        } else {
            false
        };
        let this = Rc::new(Self {
            app: app.downgrade(),
            connection,
            config,
            theme,
            theme_installed: Cell::new(installed),
            theme_subscription: RefCell::new(None),
            config_subscription: RefCell::new(None),
            snapshot: RefCell::new(Snapshot::default()),
            manage: RefCell::new(None),
            create: RefCell::new(None),
            switcher: RefCell::new(None),
        });
        this.apply_style(&this.config.values());
        let manager = adw::StyleManager::default();
        let weak = Rc::downgrade(&this);
        let theme_id = manager.connect_dark_notify(move |_| {
            if let Some(this) = weak.upgrade() {
                this.apply_style(&this.config.values());
            }
        });
        this.theme_subscription.replace(Some((manager, theme_id)));
        let weak = Rc::downgrade(&this);
        let sub = this.config.subscribe(move |values| {
            if let Some(this) = weak.upgrade() {
                clivuu_core::i18n::initialize(values["language"].as_str());
                this.apply_style(values);
                this.schedule_reload();
            }
        });
        this.config_subscription.replace(Some(sub));
        this
    }

    fn apply_style(&self, values: &Values) {
        let dark = match values["theme"].as_str() {
            Some("dark") => true,
            Some("light") => false,
            _ => adw::StyleManager::default().is_dark(),
        };
        adw::StyleManager::default().set_color_scheme(match values["theme"].as_str() {
            Some("dark") => adw::ColorScheme::ForceDark,
            Some("light") => adw::ColorScheme::ForceLight,
            _ => adw::ColorScheme::Default,
        });
        let font = values["preferences-font"].as_str().unwrap_or("");
        let mut css = super::preferences::style(dark, font);
        let accent = values["accent-color"].as_str().unwrap_or("#bd93f9");
        let (chip_border, badge_bg, muted_fg) = if dark {
            ("#565a70", "#21222c", "#b6b7c5")
        } else {
            ("#c8c5d8", "#eeecf5", "#626477")
        };
        css.push_str(&format!(
            "\n.clivuu-group-current {{ background: alpha({accent}, 0.18); color: {accent}; border-radius: 99px; padding: 2px 8px; font-size: 11px; font-weight: 600; }}\
             \n.clivuu-group-count {{ background: {badge_bg}; color: {muted_fg}; border: 1px solid {chip_border}; border-radius: 99px; padding: 1px 7px; font-size: 11px; font-weight: 500; }}\
             \n.clivuu-color-chip {{ background: transparent; min-width: 26px; min-height: 26px; padding: 3px; border-radius: 99px; border: 2px solid transparent; }}\
             \n.clivuu-color-chip-active {{ background: alpha({accent}, 0.14); border-color: {accent}; }}\
             \n.clivuu-switcher-key {{ background: {badge_bg}; color: {muted_fg}; border: 1px solid {chip_border}; border-radius: 5px; min-width: 20px; padding: 1px 5px; font-size: 11px; font-weight: 600; }}\
             \n.clivuu-group-error {{ background: transparent; color: #ff6e6e; font-size: 12px; }}"
        ));
        self.theme.load_from_string(&css);
    }

    pub fn present(self: &Rc<Self>, mode: Mode) {
        match mode {
            Mode::Manage => {
                let view = self.ensure_manage();
                if let Some(view) = view {
                    view.window.present();
                }
                self.schedule_reload();
            }
            Mode::Create { move_ids } => {
                if let Some(manage) = self.manage.borrow().as_ref().cloned()
                    && manage.window.is_visible()
                {
                    manage.window.present();
                    manage.open_create_sheet(self, move_ids);
                } else if let Some(win) = self.open_create_window(move_ids) {
                    win.window.present();
                }
                self.schedule_reload();
            }
            Mode::Switcher => {
                let already_open = self
                    .switcher
                    .borrow()
                    .as_ref()
                    .is_some_and(|s| s.window.is_visible());
                if already_open {
                    if let Some(sw) = self.switcher.borrow().as_ref() {
                        sw.step(1);
                        sw.window.present();
                    }
                } else if let Some(sw) = self.ensure_switcher() {
                    sw.reset_initial_selection(&self.snapshot.borrow());
                    sw.window.present();
                    self.schedule_reload();
                }
            }
        }
    }

    pub fn prepare_capture(&self) {
        let manage = self.manage.borrow().clone();
        if let Some(manage) = manage {
            dismiss_popovers(manage.window.upcast_ref());
            if let Some(dialog) = manage.window.visible_dialog() {
                dialog.close();
            }
        }
        let create = self.create.borrow_mut().take();
        if let Some(create) = create {
            create.window.close();
        }
        let switcher = self.switcher.borrow_mut().take();
        if let Some(switcher) = switcher {
            switcher.window.close();
        }
    }

    pub fn close_all(&self) {
        let manage = self.manage.borrow_mut().take();
        if let Some(manage) = manage {
            manage.window.close();
        }
        let create = self.create.borrow_mut().take();
        if let Some(create) = create {
            create.window.close();
        }
        let switcher = self.switcher.borrow_mut().take();
        if let Some(switcher) = switcher {
            switcher.window.close();
        }
    }

    pub fn schedule_reload(self: &Rc<Self>) {
        let this = self.clone();
        glib::MainContext::default().spawn_local(async move {
            if let Ok(reply) =
                desktop_execute(&this.connection, json!({ "command": "groups" })).await
            {
                let current = reply["current"]
                    .as_str()
                    .unwrap_or(DEFAULT_GROUP)
                    .to_owned();
                let has_empty = reply["hasEmptyGroups"].as_bool().unwrap_or(false);
                let groups: Vec<GroupSummary> = serde_json::from_value(reply["details"].clone())
                    .unwrap_or_else(|_| {
                        vec![GroupSummary {
                            name: DEFAULT_GROUP.into(),
                            display_name: group_display_name(DEFAULT_GROUP),
                            color: None,
                            active_pins: 0,
                            closed_pins: 0,
                            current: true,
                            default: true,
                        }]
                    });
                let snap = Snapshot {
                    current,
                    has_empty,
                    groups,
                };
                this.snapshot.replace(snap.clone());
                if let Some(manage) = this.manage.borrow().as_ref() {
                    manage.sync(&snap);
                }
                if let Some(create) = this.create.borrow().as_ref() {
                    create.form.sync_snapshot(&snap);
                }
                if let Some(switcher) = this.switcher.borrow().as_ref() {
                    switcher.sync(&snap);
                }
            }
        });
    }

    fn ensure_manage(self: &Rc<Self>) -> Option<Rc<ManageView>> {
        if let Some(existing) = self.manage.borrow().as_ref() {
            return Some(existing.clone());
        }
        let app = self.app.upgrade()?;
        let view = ManageView::new(&app, self);
        let weak = Rc::downgrade(self);
        view.window.connect_close_request(move |_| {
            if let Some(this) = weak.upgrade() {
                this.manage.borrow_mut().take();
            }
            glib::Propagation::Proceed
        });
        view.sync(&self.snapshot.borrow());
        self.manage.replace(Some(view.clone()));
        Some(view)
    }

    fn open_create_window(self: &Rc<Self>, move_ids: Vec<u64>) -> Option<Rc<CreateWindow>> {
        let old = self.create.borrow_mut().take();
        if let Some(old) = old {
            old.window.close();
        }
        let app = self.app.upgrade()?;
        let win = CreateWindow::new(&app, self, move_ids);
        let weak = Rc::downgrade(self);
        win.window.connect_close_request(move |_| {
            if let Some(this) = weak.upgrade() {
                this.create.borrow_mut().take();
            }
            glib::Propagation::Proceed
        });
        win.form.sync_snapshot(&self.snapshot.borrow());
        self.create.replace(Some(win.clone()));
        Some(win)
    }

    fn ensure_switcher(self: &Rc<Self>) -> Option<Rc<SwitcherView>> {
        if let Some(existing) = self.switcher.borrow().as_ref() {
            return Some(existing.clone());
        }
        let app = self.app.upgrade()?;
        let sw = SwitcherView::new(&app, self);
        let weak = Rc::downgrade(self);
        sw.window.connect_close_request(move |_| {
            if let Some(this) = weak.upgrade() {
                this.switcher.borrow_mut().take();
            }
            glib::Propagation::Proceed
        });
        sw.sync(&self.snapshot.borrow());
        self.switcher.replace(Some(sw.clone()));
        Some(sw)
    }
}

fn dismiss_popovers(widget: &gtk::Widget) {
    if let Some(popover) = widget.downcast_ref::<gtk::Popover>() {
        popover.popdown();
        popover.set_visible(false);
    }
    let mut child = widget.first_child();
    while let Some(w) = child {
        child = w.next_sibling();
        dismiss_popovers(&w);
    }
}

struct ColorSelector {
    row: gtk::Box,
    selected: RefCell<Option<String>>,
    chips: Vec<(Option<String>, gtk::Button)>,
}

impl ColorSelector {
    fn new(on_change: impl Fn() + 'static) -> Rc<Self> {
        let on_change: Rc<dyn Fn()> = Rc::new(on_change);
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        row.set_valign(gtk::Align::Center);
        let mut options: Vec<(Option<String>, String)> = vec![(None, t("None"))];
        for (name, hex) in GROUP_COLOR_PRESETS {
            options.push((Some((*hex).to_owned()), t(name)));
        }
        let mut chips = Vec::with_capacity(options.len());
        for (value, tooltip) in &options {
            let btn = gtk::Button::builder()
                .tooltip_text(tooltip)
                .css_classes(["clivuu-color-chip"])
                .valign(gtk::Align::Center)
                .build();
            let suffix = value
                .as_deref()
                .map_or("none", |hex| hex.trim_start_matches('#'));
            btn.set_widget_name(&format!("chip-{suffix}"));
            let dot = group_color_dot(value.as_deref(), 14);
            btn.set_child(Some(&dot));
            row.append(&btn);
            chips.push((value.clone(), btn));
        }
        let selector = Rc::new(Self {
            row,
            selected: RefCell::new(None),
            chips,
        });
        for (value, btn) in &selector.chips {
            let weak = Rc::downgrade(&selector);
            let chosen = value.clone();
            let notify = on_change.clone();
            btn.connect_clicked(move |_| {
                if let Some(sel) = weak.upgrade() {
                    sel.set_value(chosen.as_deref());
                    notify();
                }
            });
        }
        selector.set_value(None);
        selector
    }

    fn set_value(&self, color: Option<&str>) {
        let normalized = color.map(|c| c.to_ascii_lowercase());
        self.selected.replace(normalized.clone());
        for (val, btn) in &self.chips {
            if *val == normalized {
                btn.add_css_class("clivuu-color-chip-active");
            } else {
                btn.remove_css_class("clivuu-color-chip-active");
            }
        }
    }

    fn value(&self) -> Option<String> {
        self.selected.borrow().clone()
    }
}

struct CreateForm {
    root: gtk::Box,
    entry: adw::EntryRow,
    colors: Rc<ColorSelector>,
    activate_switch: adw::SwitchRow,
    error_label: gtk::Label,
    create_button: gtk::Button,
    existing_groups: RefCell<Vec<String>>,
    move_ids: Vec<u64>,
}

impl CreateForm {
    fn new(owner: &Rc<GroupWindows>, move_ids: Vec<u64>, on_done: impl Fn() + 'static) -> Rc<Self> {
        let on_done: Rc<dyn Fn()> = Rc::new(on_done);
        let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
        for edge in ["margin-top", "margin-bottom", "margin-start", "margin-end"] {
            root.set_property(edge, 16);
        }
        let group = adw::PreferencesGroup::new();
        if !move_ids.is_empty() {
            group.set_description(Some(&tf(
                "{0} selected pin(s) will be moved to the new group",
                &[&move_ids.len().to_string()],
            )));
        }
        let entry = adw::EntryRow::builder().title(t("Group name")).build();
        entry.set_widget_name("create-group-name");
        group.add(&entry);

        let colors = ColorSelector::new(|| {});
        let color_row = adw::ActionRow::builder().title(t("Group color")).build();
        color_row.add_suffix(&colors.row);
        group.add(&color_row);

        let activate_switch = adw::SwitchRow::builder()
            .title(t("Switch to new group"))
            .subtitle(t("Activate the newly created group immediately"))
            .active(true)
            .build();
        activate_switch.set_widget_name("create-group-activate");
        group.add(&activate_switch);
        root.append(&group);

        let error_label = gtk::Label::builder()
            .xalign(0.)
            .wrap(true)
            .visible(false)
            .css_classes(["clivuu-group-error"])
            .build();
        root.append(&error_label);

        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        actions.set_halign(gtk::Align::End);
        let cancel_btn = gtk::Button::with_label(&t("Cancel"));
        cancel_btn.set_widget_name("create-group-cancel");
        let close = on_done.clone();
        cancel_btn.connect_clicked(move |_| close());
        let create_button = gtk::Button::with_label(&t("Create"));
        create_button.set_widget_name("create-group-submit");
        create_button.add_css_class("suggested-action");
        create_button.add_css_class("pill");
        actions.append(&cancel_btn);
        actions.append(&create_button);
        root.append(&actions);

        let form = Rc::new(Self {
            root,
            entry,
            colors,
            activate_switch,
            error_label,
            create_button,
            existing_groups: RefCell::new(vec![]),
            move_ids,
        });

        let weak = Rc::downgrade(&form);
        form.entry.connect_changed(move |_| {
            if let Some(f) = weak.upgrade() {
                f.validate_live();
            }
        });
        let submit_form = Rc::downgrade(&form);
        let submit_owner = Rc::downgrade(owner);
        let submit_done = on_done.clone();
        form.create_button.connect_clicked(move |_| {
            if let (Some(f), Some(owner)) = (submit_form.upgrade(), submit_owner.upgrade()) {
                f.submit(owner, submit_done.clone());
            }
        });
        let enter_form = Rc::downgrade(&form);
        let enter_owner = Rc::downgrade(owner);
        form.entry.connect_entry_activated(move |_| {
            if let (Some(f), Some(owner)) = (enter_form.upgrade(), enter_owner.upgrade()) {
                f.submit(owner, on_done.clone());
            }
        });
        form
    }

    fn sync_snapshot(&self, snap: &Snapshot) {
        self.existing_groups
            .replace(snap.groups.iter().map(|g| g.name.clone()).collect());
        if self.entry.text().is_empty() {
            let mut n = snap.groups.len() + 1;
            let mut candidate = tf("Pin group {0}", &[&n.to_string()]);
            while snap.groups.iter().any(|g| g.name == candidate) {
                n += 1;
                candidate = tf("Pin group {0}", &[&n.to_string()]);
            }
            self.entry.set_text(&candidate);
            self.entry.select_region(0, -1);
        }
        self.validate_live();
    }

    fn validate_live(&self) -> bool {
        let trimmed = self.entry.text().trim().to_owned();
        if trimmed.is_empty() {
            self.error_label.set_label(&t("Group name cannot be empty"));
            self.error_label.set_visible(true);
            self.create_button.set_sensitive(false);
            return false;
        }
        let duplicate = trimmed.eq_ignore_ascii_case(DEFAULT_GROUP)
            || trimmed == group_display_name(DEFAULT_GROUP)
            || self.existing_groups.borrow().contains(&trimmed);
        if duplicate {
            self.error_label
                .set_label(&t("This pin group already exists"));
            self.error_label.set_visible(true);
            self.create_button.set_sensitive(false);
            return false;
        }
        self.error_label.set_visible(false);
        self.create_button.set_sensitive(true);
        true
    }

    fn submit(self: &Rc<Self>, owner: Rc<GroupWindows>, on_done: Rc<dyn Fn()>) {
        if !self.validate_live() {
            return;
        }
        let name = self.entry.text().trim().to_owned();
        let color = self.colors.value();
        let activate = self.activate_switch.is_active();
        let move_ids = self.move_ids.clone();
        let form = self.clone();
        form.create_button.set_sensitive(false);
        glib::MainContext::default().spawn_local(async move {
            let mut req = json!({
                "command": "create-group",
                "argument": name,
                "activate": activate,
                "strict": true
            });
            if let Some(hex) = color {
                req["color"] = json!(hex);
            }
            if !move_ids.is_empty() {
                req["moveIds"] = json!(move_ids);
            }
            match desktop_execute(&owner.connection, req).await {
                Ok(_) => {
                    owner.schedule_reload();
                    on_done();
                }
                Err(e) => {
                    form.error_label.set_label(&e);
                    form.error_label.set_visible(true);
                    form.create_button.set_sensitive(true);
                }
            }
        });
    }
}

struct CreateWindow {
    window: adw::ApplicationWindow,
    form: Rc<CreateForm>,
}

impl CreateWindow {
    fn new(app: &gtk::Application, owner: &Rc<GroupWindows>, move_ids: Vec<u64>) -> Rc<Self> {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title(t("Create Pin Group"))
            .default_width(440)
            .default_height(330)
            .resizable(false)
            .build();
        window.add_css_class("clivuu-preferences");
        crate::platform::integration::identity::configure(&window);
        let close_win = window.downgrade();
        let form = CreateForm::new(owner, move_ids, move || {
            if let Some(w) = close_win.upgrade() {
                w.close();
            }
        });
        let toolbar = adw::ToolbarView::new();
        let header = adw::HeaderBar::builder()
            .title_widget(
                &gtk::Label::builder()
                    .label(t("Create Pin Group"))
                    .css_classes(["heading"])
                    .build(),
            )
            .build();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&form.root));
        window.set_content(Some(&toolbar));

        let keys = gtk::EventControllerKey::new();
        let esc_win = window.downgrade();
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gdk::Key::Escape
                && let Some(w) = esc_win.upgrade()
            {
                w.close();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        window.add_controller(keys);
        gtk::prelude::GtkWindowExt::set_focus(&window, Some(&form.entry));
        Rc::new(Self { window, form })
    }
}

struct ManageView {
    window: adw::ApplicationWindow,
    toasts: adw::ToastOverlay,
    list: gtk::ListBox,
    heading: gtk::Label,
    move_up_btn: gtk::Button,
    move_down_btn: gtk::Button,
    remove_empty_btn: gtk::Button,
    name_entry: adw::EntryRow,
    name_group: adw::PreferencesGroup,
    colors: Rc<ColorSelector>,
    apply_btn: gtk::Button,
    error_label: gtk::Label,
    active_row: adw::ActionRow,
    switch_btn: gtk::Button,
    empty_row: adw::ActionRow,
    empty_btn: gtk::Button,
    delete_row: adw::ActionRow,
    delete_btn: gtk::Button,
    selected_group: RefCell<String>,
    updating: Cell<bool>,
    snapshot: RefCell<Snapshot>,
}

impl ManageView {
    fn new(app: &gtk::Application, owner: &Rc<GroupWindows>) -> Rc<Self> {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title(t("Manage Pin Groups"))
            .default_width(780)
            .default_height(520)
            .width_request(600)
            .height_request(420)
            .build();
        window.add_css_class("clivuu-preferences");
        crate::platform::integration::identity::configure(&window);

        let split = adw::NavigationSplitView::builder()
            .min_sidebar_width(240.)
            .max_sidebar_width(290.)
            .show_content(true)
            .build();

        let side_bar = adw::ToolbarView::new();
        side_bar.add_css_class("preferences-sidebar");
        let side_header = adw::HeaderBar::builder()
            .show_start_title_buttons(false)
            .show_end_title_buttons(false)
            .title_widget(
                &gtk::Label::builder()
                    .label(t("Pin Groups"))
                    .css_classes(["heading"])
                    .build(),
            )
            .build();
        let new_btn = gtk::Button::builder()
            .icon_name("list-add-symbolic")
            .tooltip_text(t("Create new group"))
            .css_classes(["flat"])
            .build();
        new_btn.set_widget_name("new-group-btn");
        side_header.pack_end(&new_btn);
        side_bar.add_top_bar(&side_header);

        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .css_classes(["navigation-sidebar"])
            .build();
        list.set_widget_name("group-list");
        let list_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&list)
            .build();
        side_bar.set_content(Some(&list_scroll));

        let bottom_bar = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        for edge in ["margin-top", "margin-bottom", "margin-start", "margin-end"] {
            bottom_bar.set_property(edge, 6);
        }
        let move_up_btn = gtk::Button::builder()
            .icon_name("go-up-symbolic")
            .tooltip_text(t("Move group up"))
            .css_classes(["flat"])
            .build();
        move_up_btn.set_widget_name("move-up-btn");
        let move_down_btn = gtk::Button::builder()
            .icon_name("go-down-symbolic")
            .tooltip_text(t("Move group down"))
            .css_classes(["flat"])
            .build();
        move_down_btn.set_widget_name("move-down-btn");
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        let remove_empty_btn = gtk::Button::builder()
            .label(t("Remove empty"))
            .tooltip_text(t("Remove inactive empty pin groups"))
            .css_classes(["flat"])
            .build();
        remove_empty_btn.set_widget_name("remove-empty-btn");
        bottom_bar.append(&move_up_btn);
        bottom_bar.append(&move_down_btn);
        bottom_bar.append(&spacer);
        bottom_bar.append(&remove_empty_btn);
        side_bar.add_bottom_bar(&bottom_bar);

        let side_page = adw::NavigationPage::builder()
            .title(t("Pin Groups"))
            .child(&side_bar)
            .build();

        let heading = gtk::Label::builder().css_classes(["heading"]).build();
        let content_bar = adw::ToolbarView::new();
        let content_header = adw::HeaderBar::builder()
            .show_start_title_buttons(false)
            .title_widget(&heading)
            .build();
        content_bar.add_top_bar(&content_header);

        let body = gtk::Box::new(gtk::Orientation::Vertical, 18);
        for edge in ["margin-top", "margin-bottom", "margin-start", "margin-end"] {
            body.set_property(edge, 20);
        }
        let name_group = adw::PreferencesGroup::builder()
            .title(t("Group settings"))
            .build();
        let name_entry = adw::EntryRow::builder().title(t("Group name")).build();
        name_entry.set_widget_name("group-name-entry");
        name_group.add(&name_entry);

        let colors_holder: Rc<RefCell<Option<glib::WeakRef<gtk::Button>>>> =
            Rc::new(RefCell::new(None));
        let colors = ColorSelector::new({
            let holder = colors_holder.clone();
            move || {
                if let Some(btn) = holder.borrow().as_ref().and_then(|w| w.upgrade()) {
                    btn.set_sensitive(true);
                }
            }
        });
        let color_row = adw::ActionRow::builder()
            .title(t("Group color"))
            .subtitle(t("Displayed in menus and the group switcher"))
            .build();
        color_row.add_suffix(&colors.row);
        name_group.add(&color_row);

        let apply_row = adw::ActionRow::builder()
            .title(t("Save group changes"))
            .build();
        let apply_btn = gtk::Button::builder()
            .label(t("Apply"))
            .valign(gtk::Align::Center)
            .css_classes(["suggested-action", "pill"])
            .build();
        apply_btn.set_widget_name("apply-group-btn");
        colors_holder.replace(Some(apply_btn.downgrade()));
        apply_row.add_suffix(&apply_btn);
        name_group.add(&apply_row);
        body.append(&name_group);

        let error_label = gtk::Label::builder()
            .xalign(0.)
            .wrap(true)
            .visible(false)
            .css_classes(["clivuu-group-error"])
            .build();
        body.append(&error_label);

        let actions_group = adw::PreferencesGroup::builder()
            .title(t("Group actions"))
            .build();
        let active_row = adw::ActionRow::builder()
            .title(t("Active pin group"))
            .build();
        let switch_btn = gtk::Button::builder()
            .label(t("Switch to group"))
            .valign(gtk::Align::Center)
            .css_classes(["pill"])
            .build();
        switch_btn.set_widget_name("switch-group-btn");
        active_row.add_suffix(&switch_btn);
        actions_group.add(&active_row);

        let empty_row = adw::ActionRow::builder()
            .title(t("Empty group"))
            .subtitle(t("Permanently destroy all pins in this group"))
            .build();
        let empty_btn = gtk::Button::builder()
            .label(t("Empty"))
            .valign(gtk::Align::Center)
            .css_classes(["pill"])
            .build();
        empty_btn.set_widget_name("empty-group-btn");
        empty_row.add_suffix(&empty_btn);
        actions_group.add(&empty_row);

        let delete_row = adw::ActionRow::builder().title(t("Delete group")).build();
        let delete_btn = gtk::Button::builder()
            .label(t("Delete"))
            .valign(gtk::Align::Center)
            .css_classes(["destructive-action", "pill"])
            .build();
        delete_btn.set_widget_name("delete-group-btn");
        delete_row.add_suffix(&delete_btn);
        actions_group.add(&delete_row);
        body.append(&actions_group);

        let clamp = adw::Clamp::builder().maximum_size(540).child(&body).build();
        let detail_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&clamp)
            .build();
        content_bar.set_content(Some(&detail_scroll));

        let content_page = adw::NavigationPage::builder()
            .title(t("Manage Pin Groups"))
            .child(&content_bar)
            .build();

        split.set_sidebar(Some(&side_page));
        split.set_content(Some(&content_page));

        let toasts = adw::ToastOverlay::new();
        toasts.set_child(Some(&split));
        window.set_content(Some(&toasts));

        let keys = gtk::EventControllerKey::new();
        let esc_win = window.downgrade();
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gdk::Key::Escape
                && let Some(w) = esc_win.upgrade()
                && w.visible_dialog().is_none()
            {
                w.close();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        window.add_controller(keys);

        let view = Rc::new(Self {
            window,
            toasts,
            list,
            heading,
            move_up_btn,
            move_down_btn,
            remove_empty_btn,
            name_entry,
            name_group,
            colors,
            apply_btn,
            error_label,
            active_row,
            switch_btn,
            empty_row,
            empty_btn,
            delete_row,
            delete_btn,
            selected_group: RefCell::new(DEFAULT_GROUP.into()),
            updating: Cell::new(false),
            snapshot: RefCell::new(Snapshot::default()),
        });
        view.wire(owner);
        view
    }

    fn wire(self: &Rc<Self>, owner: &Rc<GroupWindows>) {
        {
            let v = Rc::downgrade(self);
            self.list.connect_row_selected(move |_, row| {
                if let (Some(v), Some(row)) = (v.upgrade(), row)
                    && !v.updating.get()
                {
                    let name = row.widget_name().to_string();
                    if let Some(group) = name.strip_prefix("row-") {
                        v.selected_group.replace(group.to_owned());
                        v.populate_details();
                    }
                }
            });
        }
        {
            let v = Rc::downgrade(self);
            let o = Rc::downgrade(owner);
            let dbl = gtk::GestureClick::builder().button(1).build();
            dbl.connect_pressed(move |_, count, _, y| {
                if count == 2
                    && let (Some(v), Some(owner)) = (v.upgrade(), o.upgrade())
                    && let Some(row) = v.list.row_at_y(y as i32)
                    && let Some(target) = row.widget_name().as_str().strip_prefix("row-")
                {
                    let target = target.to_owned();
                    glib::MainContext::default().spawn_local(async move {
                        let _ = desktop_execute(
                            &owner.connection,
                            json!({ "command": "switch-group", "argument": target }),
                        )
                        .await;
                        owner.schedule_reload();
                    });
                }
            });
            self.list.add_controller(dbl);
        }
        {
            let v = Rc::downgrade(self);
            let o = Rc::downgrade(owner);
            if let Some(btn) = find_by_name(self.window.upcast_ref(), "new-group-btn")
                .and_downcast::<gtk::Button>()
            {
                btn.connect_clicked(move |_| {
                    if let (Some(v), Some(owner)) = (v.upgrade(), o.upgrade()) {
                        v.open_create_sheet(&owner, vec![]);
                    }
                });
            }
        }
        {
            let v = Rc::downgrade(self);
            let o = Rc::downgrade(owner);
            self.move_up_btn.connect_clicked(move |_| {
                if let (Some(v), Some(owner)) = (v.upgrade(), o.upgrade()) {
                    v.reorder(&owner, -1);
                }
            });
        }
        {
            let v = Rc::downgrade(self);
            let o = Rc::downgrade(owner);
            self.move_down_btn.connect_clicked(move |_| {
                if let (Some(v), Some(owner)) = (v.upgrade(), o.upgrade()) {
                    v.reorder(&owner, 1);
                }
            });
        }
        {
            let v = Rc::downgrade(self);
            let o = Rc::downgrade(owner);
            self.remove_empty_btn.connect_clicked(move |_| {
                if let (Some(v), Some(owner)) = (v.upgrade(), o.upgrade()) {
                    glib::MainContext::default().spawn_local(async move {
                        match desktop_execute(
                            &owner.connection,
                            json!({ "command": "remove-empty-groups" }),
                        )
                        .await
                        {
                            Ok(_) => owner.schedule_reload(),
                            Err(e) => v.toasts.add_toast(adw::Toast::new(&e)),
                        }
                    });
                }
            });
        }
        {
            let v = Rc::downgrade(self);
            self.name_entry.connect_changed(move |_| {
                if let Some(v) = v.upgrade()
                    && !v.updating.get()
                {
                    v.validate_edit();
                }
            });
        }
        {
            let v = Rc::downgrade(self);
            let o = Rc::downgrade(owner);
            self.apply_btn.connect_clicked(move |_| {
                if let (Some(v), Some(owner)) = (v.upgrade(), o.upgrade()) {
                    v.apply_edits(&owner);
                }
            });
        }
        {
            let v = Rc::downgrade(self);
            let o = Rc::downgrade(owner);
            self.name_entry.connect_entry_activated(move |_| {
                if let (Some(v), Some(owner)) = (v.upgrade(), o.upgrade()) {
                    v.apply_edits(&owner);
                }
            });
        }
        {
            let v = Rc::downgrade(self);
            let o = Rc::downgrade(owner);
            self.switch_btn.connect_clicked(move |_| {
                if let (Some(v), Some(owner)) = (v.upgrade(), o.upgrade()) {
                    let target = v.selected_group.borrow().clone();
                    glib::MainContext::default().spawn_local(async move {
                        match desktop_execute(
                            &owner.connection,
                            json!({ "command": "switch-group", "argument": target }),
                        )
                        .await
                        {
                            Ok(_) => owner.schedule_reload(),
                            Err(e) => v.toasts.add_toast(adw::Toast::new(&e)),
                        }
                    });
                }
            });
        }
        {
            let v = Rc::downgrade(self);
            let o = Rc::downgrade(owner);
            self.empty_btn.connect_clicked(move |_| {
                if let (Some(v), Some(owner)) = (v.upgrade(), o.upgrade()) {
                    v.confirm_empty(&owner);
                }
            });
        }
        {
            let v = Rc::downgrade(self);
            let o = Rc::downgrade(owner);
            self.delete_btn.connect_clicked(move |_| {
                if let (Some(v), Some(owner)) = (v.upgrade(), o.upgrade()) {
                    v.confirm_delete(&owner);
                }
            });
        }
    }

    fn open_create_sheet(&self, owner: &Rc<GroupWindows>, move_ids: Vec<u64>) {
        let dialog = adw::Dialog::builder()
            .title(t("Create Pin Group"))
            .content_width(420)
            .build();
        dialog.set_widget_name("create-group-dialog");
        dialog.add_css_class("clivuu-preferences-dialog");
        let close_dlg = dialog.downgrade();
        let form = CreateForm::new(owner, move_ids, move || {
            if let Some(d) = close_dlg.upgrade() {
                d.close();
            }
        });
        form.sync_snapshot(&self.snapshot.borrow());
        let view = adw::ToolbarView::new();
        view.add_css_class("clivuu-preferences");
        let header = adw::HeaderBar::builder()
            .show_start_title_buttons(false)
            .show_end_title_buttons(false)
            .title_widget(
                &gtk::Label::builder()
                    .label(t("Create Pin Group"))
                    .css_classes(["heading"])
                    .build(),
            )
            .build();
        view.add_top_bar(&header);
        view.set_content(Some(&form.root));
        dialog.set_child(Some(&view));
        dialog.set_focus(Some(&form.entry));
        dialog.present(Some(&self.window));
    }

    fn sync(&self, snap: &Snapshot) {
        self.updating.set(true);
        self.snapshot.replace(snap.clone());
        let current_sel = self.selected_group.borrow().clone();
        let valid_sel = if snap.groups.iter().any(|g| g.name == current_sel) {
            current_sel
        } else {
            snap.current.clone()
        };
        self.selected_group.replace(valid_sel.clone());

        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        let mut to_select = None;
        for g in &snap.groups {
            let row = gtk::ListBoxRow::new();
            row.set_widget_name(&format!("row-{}", g.name));
            let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            for (edge, px) in [
                ("margin-top", 8),
                ("margin-bottom", 8),
                ("margin-start", 10),
                ("margin-end", 10),
            ] {
                hbox.set_property(edge, px);
            }
            hbox.append(&group_color_dot(g.color.as_deref(), 10));
            let label = gtk::Label::builder()
                .label(&g.display_name)
                .xalign(0.)
                .hexpand(true)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();
            hbox.append(&label);
            if g.current {
                let badge = gtk::Label::builder()
                    .label(t("Active"))
                    .valign(gtk::Align::Center)
                    .css_classes(["clivuu-group-current"])
                    .build();
                hbox.append(&badge);
            }
            let count = gtk::Label::builder()
                .label(g.active_pins.to_string())
                .valign(gtk::Align::Center)
                .css_classes(["clivuu-group-count", "numeric"])
                .build();
            hbox.append(&count);
            row.set_child(Some(&hbox));
            self.list.append(&row);
            if g.name == valid_sel {
                to_select = Some(row);
            }
        }
        if let Some(row) = to_select {
            self.list.select_row(Some(&row));
        }
        self.remove_empty_btn.set_sensitive(snap.has_empty);
        self.updating.set(false);
        self.populate_details();
    }

    fn populate_details(&self) {
        self.updating.set(true);
        let snap = self.snapshot.borrow();
        let selected_name = self.selected_group.borrow().clone();
        let idx = snap
            .groups
            .iter()
            .position(|g| g.name == selected_name)
            .unwrap_or(0);
        let Some(g) = snap.groups.get(idx) else {
            self.updating.set(false);
            return;
        };
        self.heading.set_label(&g.display_name);
        self.move_up_btn.set_sensitive(idx > 0);
        self.move_down_btn
            .set_sensitive(idx + 1 < snap.groups.len());

        self.name_entry.set_text(&g.display_name);
        self.name_entry.set_sensitive(!g.default);
        let default_desc = g.default.then(|| {
            t("The default pin group name is fixed, while its color tag can still be customized")
        });
        self.name_group.set_description(default_desc.as_deref());
        self.colors.set_value(g.color.as_deref());
        self.error_label.set_visible(false);
        self.apply_btn.set_sensitive(false);

        self.active_row.set_subtitle(&if g.current {
            t("Currently displayed on screen")
        } else {
            t("Switch to display pins in this group")
        });
        self.switch_btn.set_sensitive(!g.current);

        let total_pins = g.active_pins + g.closed_pins;
        self.empty_row.set_subtitle(&tf(
            "Permanently destroy all pins in this group ({0})",
            &[&total_pins.to_string()],
        ));
        self.empty_btn.set_sensitive(total_pins > 0);

        self.delete_row.set_subtitle(&if g.default {
            t("The default pin group cannot be deleted")
        } else {
            t("Remove this group and migrate or destroy its pins")
        });
        self.delete_btn.set_sensitive(!g.default);
        self.updating.set(false);
    }

    fn validate_edit(&self) -> bool {
        let snap = self.snapshot.borrow();
        let selected_name = self.selected_group.borrow().clone();
        if selected_name == DEFAULT_GROUP {
            self.error_label.set_visible(false);
            self.apply_btn.set_sensitive(true);
            return true;
        }
        let trimmed = self.name_entry.text().trim().to_owned();
        if trimmed.is_empty() {
            self.error_label.set_label(&t("Group name cannot be empty"));
            self.error_label.set_visible(true);
            self.apply_btn.set_sensitive(false);
            return false;
        }
        let duplicate = trimmed != selected_name
            && (trimmed.eq_ignore_ascii_case(DEFAULT_GROUP)
                || trimmed == group_display_name(DEFAULT_GROUP)
                || snap.groups.iter().any(|g| g.name == trimmed));
        if duplicate {
            self.error_label
                .set_label(&t("This pin group already exists"));
            self.error_label.set_visible(true);
            self.apply_btn.set_sensitive(false);
            return false;
        }
        self.error_label.set_visible(false);
        self.apply_btn.set_sensitive(true);
        true
    }

    fn apply_edits(self: &Rc<Self>, owner: &Rc<GroupWindows>) {
        if !self.validate_edit() {
            return;
        }
        let old_name = self.selected_group.borrow().clone();
        let new_name = self.name_entry.text().trim().to_owned();
        let color = self.colors.value().unwrap_or_else(|| "none".into());
        let v = self.clone();
        let owner = owner.clone();
        glib::MainContext::default().spawn_local(async move {
            let mut target_name = old_name.clone();
            if old_name != DEFAULT_GROUP && new_name != old_name {
                match desktop_execute(
                    &owner.connection,
                    json!({
                        "command": "rename-group",
                        "argument": old_name,
                        "to": new_name
                    }),
                )
                .await
                {
                    Ok(reply) => {
                        if let Some(updated) = reply["group"].as_str() {
                            target_name = updated.to_owned();
                            v.selected_group.replace(target_name.clone());
                        }
                    }
                    Err(e) => {
                        v.error_label.set_label(&e);
                        v.error_label.set_visible(true);
                        return;
                    }
                }
            }
            match desktop_execute(
                &owner.connection,
                json!({
                    "command": "set-group-color",
                    "argument": target_name,
                    "color": color
                }),
            )
            .await
            {
                Ok(_) => {
                    v.apply_btn.set_sensitive(false);
                    owner.schedule_reload();
                }
                Err(e) => {
                    v.error_label.set_label(&e);
                    v.error_label.set_visible(true);
                }
            }
        });
    }

    fn reorder(self: &Rc<Self>, owner: &Rc<GroupWindows>, delta: isize) {
        let mut order: Vec<String> = self
            .snapshot
            .borrow()
            .groups
            .iter()
            .map(|g| g.name.clone())
            .collect();
        let selected = self.selected_group.borrow().clone();
        let Some(idx) = order.iter().position(|g| g == &selected) else {
            return;
        };
        let next = idx as isize + delta;
        if next < 0 || next >= order.len() as isize {
            return;
        }
        order.swap(idx, next as usize);
        let v = self.clone();
        let owner = owner.clone();
        glib::MainContext::default().spawn_local(async move {
            match desktop_execute(
                &owner.connection,
                json!({ "command": "reorder-groups", "groups": order }),
            )
            .await
            {
                Ok(_) => owner.schedule_reload(),
                Err(e) => v.toasts.add_toast(adw::Toast::new(&e)),
            }
        });
    }

    fn confirm_empty(self: &Rc<Self>, owner: &Rc<GroupWindows>) {
        let snap = self.snapshot.borrow();
        let target = self.selected_group.borrow().clone();
        let Some(g) = snap.groups.iter().find(|g| g.name == target) else {
            return;
        };
        let count = g.active_pins + g.closed_pins;
        if count == 0 {
            return;
        }
        let dialog = adw::AlertDialog::builder()
            .heading(tf("Empty pin group \"{0}\"?", &[&g.display_name]))
            .body(tf(
                "Permanently destroy {0} pin(s) in this group. This cannot be undone.",
                &[&count.to_string()],
            ))
            .build();
        dialog.add_css_class("clivuu-preferences-dialog");
        dialog.add_responses(&[("resp-cancel", &t("Cancel")), ("resp-confirm", &t("Empty"))]);
        dialog.set_response_appearance("resp-confirm", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("resp-cancel"));
        dialog.set_close_response("resp-cancel");
        let v = self.clone();
        let owner = owner.clone();
        dialog.connect_response(Some("resp-confirm"), move |_, _| {
            let v = v.clone();
            let owner = owner.clone();
            let target = target.clone();
            glib::MainContext::default().spawn_local(async move {
                match desktop_execute(
                    &owner.connection,
                    json!({
                        "command": "empty-group",
                        "argument": target,
                        "confirmed": true
                    }),
                )
                .await
                {
                    Ok(_) => owner.schedule_reload(),
                    Err(e) => v.toasts.add_toast(adw::Toast::new(&e)),
                }
            });
        });
        dialog.present(Some(&self.window));
    }

    fn confirm_delete(self: &Rc<Self>, owner: &Rc<GroupWindows>) {
        let snap = self.snapshot.borrow();
        let target = self.selected_group.borrow().clone();
        if target == DEFAULT_GROUP {
            return;
        }
        let Some(g) = snap.groups.iter().find(|g| g.name == target) else {
            return;
        };
        let count = g.active_pins + g.closed_pins;
        let candidates: Vec<(String, String)> = snap
            .groups
            .iter()
            .filter(|other| other.name != target)
            .map(|other| (other.name.clone(), other.display_name.clone()))
            .collect();

        let dialog = adw::AlertDialog::builder()
            .heading(tf("Delete pin group \"{0}\"?", &[&g.display_name]))
            .body(if count == 0 {
                t("This empty pin group will be removed.")
            } else {
                tf(
                    "This group contains {0} pin(s). Move them to another group or destroy them permanently.",
                    &[&count.to_string()],
                )
            })
            .build();
        dialog.add_css_class("clivuu-preferences-dialog");

        let combo_row = if count > 0 && !candidates.is_empty() {
            let labels: Vec<&str> = candidates.iter().map(|(_, d)| d.as_str()).collect();
            let model = gtk::StringList::new(&labels);
            let combo = adw::ComboRow::builder()
                .title(t("Target group"))
                .model(&model)
                .build();
            let extra_group = adw::PreferencesGroup::new();
            extra_group.add(&combo);
            dialog.set_extra_child(Some(&extra_group));
            dialog.add_responses(&[
                ("resp-cancel", &t("Cancel")),
                ("resp-migrate", &t("Move pins & delete")),
                ("resp-destroy", &t("Destroy pins & delete")),
            ]);
            dialog.set_response_appearance("resp-migrate", adw::ResponseAppearance::Suggested);
            dialog.set_response_appearance("resp-destroy", adw::ResponseAppearance::Destructive);
            dialog.set_default_response(Some("resp-migrate"));
            Some(combo)
        } else {
            dialog.add_responses(&[
                ("resp-cancel", &t("Cancel")),
                ("resp-destroy", &t("Delete")),
            ]);
            dialog.set_response_appearance("resp-destroy", adw::ResponseAppearance::Destructive);
            dialog.set_default_response(Some("resp-cancel"));
            None
        };
        dialog.set_close_response("resp-cancel");

        let v = self.clone();
        let owner = owner.clone();
        dialog.connect_response(None, move |_, resp| {
            if resp == "resp-cancel" {
                return;
            }
            let migrate_to = if resp == "resp-migrate" {
                combo_row
                    .as_ref()
                    .and_then(|c| candidates.get(c.selected() as usize))
                    .map(|(name, _)| name.clone())
            } else if resp == "resp-destroy" {
                None
            } else {
                return;
            };
            let v = v.clone();
            let owner = owner.clone();
            let target = target.clone();
            glib::MainContext::default().spawn_local(async move {
                let mut req = json!({
                    "command": "delete-group",
                    "argument": target,
                    "confirmed": true
                });
                if let Some(dest) = migrate_to {
                    req["move_to"] = json!(dest);
                }
                match desktop_execute(&owner.connection, req).await {
                    Ok(_) => owner.schedule_reload(),
                    Err(e) => v.toasts.add_toast(adw::Toast::new(&e)),
                }
            });
        });
        dialog.present(Some(&self.window));
    }
}

struct SwitcherView {
    window: adw::ApplicationWindow,
    list: gtk::ListBox,
    groups: RefCell<Vec<GroupSummary>>,
    selected_idx: Cell<usize>,
}

impl SwitcherView {
    fn new(app: &gtk::Application, owner: &Rc<GroupWindows>) -> Rc<Self> {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title(t("Switch Pin Group"))
            .default_width(390)
            .default_height(350)
            .resizable(false)
            .build();
        window.add_css_class("clivuu-preferences");
        crate::platform::integration::identity::configure(&window);

        let toolbar = adw::ToolbarView::new();
        let header = adw::HeaderBar::builder()
            .title_widget(
                &gtk::Label::builder()
                    .label(t("Switch Pin Group"))
                    .css_classes(["heading"])
                    .build(),
            )
            .build();
        toolbar.add_top_bar(&header);

        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .css_classes(["boxed-list"])
            .build();
        list.set_widget_name("switcher-group-list");
        let container = gtk::Box::new(gtk::Orientation::Vertical, 8);
        for edge in ["margin-top", "margin-bottom", "margin-start", "margin-end"] {
            container.set_property(edge, 14);
        }
        container.append(&list);
        let hint = gtk::Label::builder()
            .label(t("Press ↑/↓ or 1–9, Enter to switch, Esc to cancel"))
            .css_classes(["dim-label", "caption"])
            .halign(gtk::Align::Center)
            .build();
        container.append(&hint);
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&container)
            .build();
        toolbar.set_content(Some(&scroll));
        window.set_content(Some(&toolbar));

        let sw = Rc::new(Self {
            window,
            list,
            groups: RefCell::new(vec![]),
            selected_idx: Cell::new(0),
        });

        {
            let s = Rc::downgrade(&sw);
            let o = Rc::downgrade(owner);
            let click = gtk::GestureClick::builder().button(1).build();
            click.connect_released(move |_, _, _, y| {
                if let (Some(s), Some(owner)) = (s.upgrade(), o.upgrade())
                    && let Some(row) = s.list.row_at_y(y as i32)
                {
                    s.selected_idx.set(row.index().max(0) as usize);
                    s.commit(&owner);
                }
            });
            sw.list.add_controller(click);
        }
        {
            let s = Rc::downgrade(&sw);
            let o = Rc::downgrade(owner);
            let keys = gtk::EventControllerKey::builder()
                .propagation_phase(gtk::PropagationPhase::Capture)
                .build();
            keys.connect_key_pressed(move |_, key, _, state| {
                let (Some(s), Some(owner)) = (s.upgrade(), o.upgrade()) else {
                    return glib::Propagation::Proceed;
                };
                match key {
                    gdk::Key::Escape => {
                        s.window.close();
                        glib::Propagation::Stop
                    }
                    gdk::Key::Up | gdk::Key::KP_Up | gdk::Key::ISO_Left_Tab => {
                        s.step(-1);
                        glib::Propagation::Stop
                    }
                    gdk::Key::Down | gdk::Key::KP_Down => {
                        s.step(1);
                        glib::Propagation::Stop
                    }
                    gdk::Key::Tab => {
                        s.step(if state.contains(gdk::ModifierType::SHIFT_MASK) {
                            -1
                        } else {
                            1
                        });
                        glib::Propagation::Stop
                    }
                    gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::space => {
                        s.commit(&owner);
                        glib::Propagation::Stop
                    }
                    _ => {
                        let digit = key.to_unicode().and_then(|c| c.to_digit(10));
                        if let Some(d @ 1..=9) = digit {
                            let idx = (d - 1) as usize;
                            if idx < s.groups.borrow().len() {
                                s.selected_idx.set(idx);
                                s.commit(&owner);
                                return glib::Propagation::Stop;
                            }
                        }
                        glib::Propagation::Proceed
                    }
                }
            });
            sw.window.add_controller(keys);
        }
        sw
    }

    fn reset_initial_selection(&self, snap: &Snapshot) {
        if snap.groups.len() <= 1 {
            self.selected_idx.set(0);
            return;
        }
        let cur = snap.groups.iter().position(|g| g.current).unwrap_or(0);
        self.selected_idx.set((cur + 1) % snap.groups.len());
    }

    fn sync(&self, snap: &Snapshot) {
        let was_empty = self.groups.borrow().is_empty();
        self.groups.replace(snap.groups.clone());
        if was_empty {
            self.reset_initial_selection(snap);
        }
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        let clamp_idx = self
            .selected_idx
            .get()
            .min(snap.groups.len().saturating_sub(1));
        self.selected_idx.set(clamp_idx);
        for (i, g) in snap.groups.iter().enumerate() {
            let row = gtk::ListBoxRow::new();
            let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            for (edge, px) in [
                ("margin-top", 9),
                ("margin-bottom", 9),
                ("margin-start", 12),
                ("margin-end", 12),
            ] {
                hbox.set_property(edge, px);
            }
            if i < 9 {
                let key_badge = gtk::Label::builder()
                    .label((i + 1).to_string())
                    .valign(gtk::Align::Center)
                    .css_classes(["clivuu-switcher-key", "numeric"])
                    .build();
                hbox.append(&key_badge);
            }
            hbox.append(&group_color_dot(g.color.as_deref(), 10));
            let label = gtk::Label::builder()
                .label(&g.display_name)
                .xalign(0.)
                .hexpand(true)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();
            hbox.append(&label);
            if g.current {
                let check = gtk::Image::from_icon_name("object-select-symbolic");
                check.set_pixel_size(14);
                hbox.append(&check);
            }
            let count = gtk::Label::builder()
                .label(g.active_pins.to_string())
                .valign(gtk::Align::Center)
                .css_classes(["clivuu-group-count", "numeric"])
                .build();
            hbox.append(&count);
            row.set_child(Some(&hbox));
            self.list.append(&row);
            if i == clamp_idx {
                self.list.select_row(Some(&row));
            }
        }
    }

    fn step(&self, delta: isize) {
        let len = self.groups.borrow().len();
        if len == 0 {
            return;
        }
        let next = (self.selected_idx.get() as isize + delta).rem_euclid(len as isize) as usize;
        self.selected_idx.set(next);
        if let Some(row) = self.list.row_at_index(next as i32) {
            self.list.select_row(Some(&row));
            row.grab_focus();
        }
    }

    fn commit(&self, owner: &Rc<GroupWindows>) {
        let target = self
            .groups
            .borrow()
            .get(self.selected_idx.get())
            .map(|g| g.name.clone());
        let Some(target) = target else {
            self.window.close();
            return;
        };
        let conn = owner.connection.clone();
        let win = self.window.clone();
        glib::MainContext::default().spawn_local(async move {
            let _ = desktop_execute(
                &conn,
                json!({ "command": "switch-group", "argument": target }),
            )
            .await;
            win.close();
        });
    }
}

fn find_by_name(root: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if root.widget_name() == name {
        return Some(root.clone());
    }
    let mut child = root.first_child();
    while let Some(w) = child {
        if let Some(found) = find_by_name(&w, name) {
            return Some(found);
        }
        child = w.next_sibling();
    }
    None
}

//! GTK preferences bind the same live configuration used by the CLI/service.
//! Labels and choice models are refreshed in place when the language changes.
use crate::infrastructure::configuration::{Configuration, Subscription, Values};
use adw::prelude::*;
use clivuu_core::{config, i18n::t};
use gtk::{gdk, glib};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
mod about;
mod appearance;
mod control;
mod naming_rules;
mod ocr;
mod pins;
mod saving;
mod startup;

type Binding = Box<dyn Fn(&Values)>;
struct ScopedBinding {
    keys: Vec<String>,
    on_locale: bool,
    apply: Binding,
}
pub struct Preferences {
    pub window: adw::ApplicationWindow,
    split: adw::NavigationSplitView,
    content_page: adw::NavigationPage,
    pages: gtk::Stack,
    sidebar: gtk::ListBox,
    heading: gtk::Label,
    toasts: adw::ToastOverlay,
    theme: gtk::CssProvider,
    css: RefCell<String>,
    theme_subscription: RefCell<Option<(adw::StyleManager, glib::SignalHandlerId)>>,
    font_subscription: RefCell<Option<(gtk::Settings, glib::SignalHandlerId)>>,
    color_services: super::color::ColorServices,
    config: Rc<Configuration>,
    updating: Cell<bool>,
    bindings: RefCell<Vec<ScopedBinding>>,
    last_values: RefCell<Option<Values>>,
    last_locale: RefCell<String>,
    subscription: RefCell<Option<Subscription>>,
}
impl Preferences {
    pub fn new(
        app: &gtk::Application,
        config: Rc<Configuration>,
        color_services: super::color::ColorServices,
    ) -> Rc<Self> {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .default_width(1000)
            .default_height(760)
            .width_request(620)
            .height_request(480)
            .build();
        window.add_css_class("clivuu-preferences");
        crate::platform::integration::identity::configure(&window);
        let split = adw::NavigationSplitView::builder()
            .min_sidebar_width(210.)
            .max_sidebar_width(260.)
            .show_content(true)
            .build();
        let content_page = adw::NavigationPage::builder().title("Clivuu").build();
        let pages = gtk::Stack::builder().vexpand(true).hexpand(true).build();
        pages.set_widget_name("preferences-pages");
        let sidebar = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .build();
        sidebar.add_css_class("navigation-sidebar");
        let heading = gtk::Label::new(None);
        heading.add_css_class("heading");
        let toasts = adw::ToastOverlay::new();
        let theme = gtk::CssProvider::new();
        gtk::style_context_add_provider_for_display(
            &gtk::prelude::WidgetExt::display(&window),
            &theme,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        let this = Rc::new(Self {
            window,
            split,
            content_page,
            pages,
            sidebar,
            heading,
            toasts,
            theme,
            css: RefCell::new(String::new()),
            theme_subscription: RefCell::new(None),
            font_subscription: RefCell::new(None),
            color_services,
            config,
            updating: Cell::new(false),
            bindings: RefCell::new(vec![]),
            last_values: RefCell::new(None),
            last_locale: RefCell::new(String::new()),
            subscription: RefCell::new(None),
        });
        this.text(&this.window, "title", "Clivuu Preferences");
        this.layout();
        this.build();
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
                this.refresh(values);
            }
        });
        this.subscription.replace(Some(sub));
        this.refresh(&this.config.values());
        this
    }
    pub fn present(&self) {
        self.window.present();
    }
    pub fn select_page(&self, page: &str) {
        if self.pages.child_by_name(page).is_some() {
            self.pages.set_visible_child_name(page);
            self.split.set_show_content(true);
        }
    }
    pub fn dismiss_popovers(&self) {
        fn close(widget: &gtk::Widget) {
            if let Some(popover) = widget.downcast_ref::<gtk::Popover>() {
                popover.popdown();
                popover.set_visible(false);
            }
            let mut child = widget.first_child();
            while let Some(w) = child {
                child = w.next_sibling();
                close(&w);
            }
        }
        close(self.window.upcast_ref());
    }
    fn layout(self: &Rc<Self>) {
        let side = adw::ToolbarView::new();
        side.add_css_class("preferences-sidebar");
        let header = adw::HeaderBar::builder()
            .show_start_title_buttons(false)
            .show_end_title_buttons(false)
            .title_widget(
                &gtk::Label::builder()
                    .label("Clivuu")
                    .css_classes(["heading"])
                    .build(),
            )
            .build();
        side.add_top_bar(&header);
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&self.sidebar)
            .build();
        side.set_content(Some(&scroll));
        let side_page = adw::NavigationPage::builder()
            .title("Clivuu")
            .child(&side)
            .build();

        let content = adw::ToolbarView::new();
        let header = adw::HeaderBar::builder()
            .show_start_title_buttons(false)
            .title_widget(&self.heading)
            .build();
        content.add_top_bar(&header);
        content.set_content(Some(&self.pages));
        self.content_page.set_child(Some(&content));
        self.split.set_sidebar(Some(&side_page));
        self.split.set_content(Some(&self.content_page));

        let breakpoint = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            720.,
            adw::LengthUnit::Sp,
        ));
        breakpoint.add_setter(&self.split, "collapsed", Some(&true.to_value()));
        self.window.add_breakpoint(breakpoint);

        self.toasts.set_child(Some(&self.split));
        self.window.set_content(Some(&self.toasts));
        let keys = gtk::EventControllerKey::new();
        let window = self.window.downgrade();
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gdk::Key::Escape
                && let Some(window) = window.upgrade()
                && window.visible_dialog().is_none()
            {
                window.close();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        self.window.add_controller(keys);
        let weak = Rc::downgrade(self);
        self.sidebar.connect_row_selected(move |_, row| {
            if let (Some(this), Some(row)) = (weak.upgrade(), row) {
                this.pages.set_visible_child_name(&row.widget_name());
            }
        });
        let weak = Rc::downgrade(self);
        self.sidebar.connect_row_activated(move |_, _| {
            if let Some(this) = weak.upgrade() {
                this.split.set_show_content(true);
            }
        });
        let weak = Rc::downgrade(self);
        self.pages.connect_visible_child_notify(move |_| {
            if let Some(this) = weak.upgrade() {
                this.update_heading();
            }
        });
    }
    fn update_heading(&self) {
        let visible_name = self.pages.visible_child_name();
        if let Some(child) = self.pages.visible_child() {
            let title = self.pages.page(&child).title().unwrap_or_default();
            self.heading.set_label(&title);
            self.content_page.set_title(&title);
        }
        if let Some(name) = visible_name
            && self
                .sidebar
                .selected_row()
                .map(|r| r.widget_name())
                .as_ref()
                != Some(&name)
        {
            let mut row = self.sidebar.first_child();
            while let Some(w) = row {
                if let Some(item) = w.downcast_ref::<gtk::ListBoxRow>()
                    && item.widget_name() == name
                {
                    self.sidebar.select_row(Some(item));
                    break;
                }
                row = w.next_sibling();
            }
        }
    }
    fn bind(&self, keys: &[&str], on_locale: bool, apply: impl Fn(&Values) + 'static) {
        self.bindings.borrow_mut().push(ScopedBinding {
            keys: keys.iter().map(|s| (*s).to_owned()).collect(),
            on_locale,
            apply: Box::new(apply),
        });
    }
    fn text(&self, object: &impl IsA<glib::Object>, property: &'static str, message: &str) {
        let weak = object.as_ref().downgrade();
        let message = message.to_owned();
        self.bind(&[], true, move |_| {
            if let Some(object) = weak.upgrade() {
                object.set_property(property, t(&message));
            }
        });
    }
    fn apply_style(&self, values: &Values) {
        let dark = adw::StyleManager::default().is_dark();
        let css = style(dark, values["preferences-font"].as_str().unwrap_or(""));
        if *self.css.borrow() != css {
            self.theme.load_from_string(&css);
            self.css.replace(css);
        }
    }
    fn refresh(&self, values: &Values) {
        self.updating.set(true);
        self.apply_style(values);
        let locale = clivuu_core::i18n::current_language().id().to_owned();
        let prev_locale = self.last_locale.replace(locale.clone());
        let prev_values = self.last_values.replace(Some(values.clone()));
        let locale_changed = prev_values.is_none() || prev_locale != locale;
        for binding in self.bindings.borrow().iter() {
            let key_changed = match &prev_values {
                None => true,
                Some(old) => binding.keys.iter().any(|k| old.get(k) != values.get(k)),
            };
            if key_changed || (binding.on_locale && locale_changed) {
                (binding.apply)(values);
            }
        }
        self.update_heading();
        self.updating.set(false);
    }
    fn page(&self, name: &str, title: &str, icon: &str) -> adw::PreferencesPage {
        let page = adw::PreferencesPage::builder()
            .name(name)
            .icon_name(icon)
            .build();
        self.text(&page, "title", title);
        self.add_page(&page, name, title, icon);
        page
    }
    // Each section owns its scroller; the fixed viewport never animates between
    // the natural heights of short and long forms.
    fn sectioned_page<const N: usize>(
        &self,
        name: &str,
        title: &str,
        icon: &str,
        sections: [(&str, &str); N],
    ) -> [adw::PreferencesPage; N] {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        self.add_page(&root, name, title, icon);
        let stack = gtk::Stack::builder()
            .vexpand(true)
            .hexpand(true)
            .transition_type(gtk::StackTransitionType::None)
            .vhomogeneous(false)
            .build();
        stack.set_widget_name(&format!("{name}-sections"));
        let switcher = gtk::StackSwitcher::builder()
            .stack(&stack)
            .halign(gtk::Align::Center)
            .margin_top(12)
            .margin_bottom(12)
            .build();
        root.append(&switcher);
        root.append(&stack);
        sections.map(|(name, title)| {
            let page = adw::PreferencesPage::builder().name(name).build();
            let entry = stack.add_titled(&page, Some(name), title);
            self.text(&entry, "title", title);
            page
        })
    }
    fn add_page(&self, widget: &impl IsA<gtk::Widget>, name: &str, title: &str, icon: &str) {
        let entry = self.pages.add_titled(widget, Some(name), title);
        self.text(&entry, "title", title);
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        content.set_margin_top(10);
        content.set_margin_bottom(10);
        content.set_margin_start(12);
        content.set_margin_end(12);
        content.append(&gtk::Image::from_icon_name(icon));
        let label = gtk::Label::builder().xalign(0.).build();
        self.text(&label, "label", title);
        content.append(&label);
        let row = gtk::ListBoxRow::builder().child(&content).build();
        row.set_widget_name(name);
        self.sidebar.append(&row);
    }
    fn group(
        &self,
        parent: &impl IsA<gtk::Widget>,
        title: &str,
        description: &str,
    ) -> adw::PreferencesGroup {
        let group = adw::PreferencesGroup::new();
        self.text(&group, "title", title);
        self.text(&group, "description", description);
        if let Some(page) = parent.as_ref().downcast_ref::<adw::PreferencesPage>() {
            page.add(&group);
        } else if let Some(box_) = parent.as_ref().downcast_ref::<gtk::Box>() {
            box_.append(&group);
        }
        group
    }
    fn row(&self, parent: &adw::PreferencesGroup, title: &str, subtitle: &str) -> adw::ActionRow {
        let row = adw::ActionRow::new();
        row.set_use_markup(false);
        self.text(&row, "title", title);
        self.text(&row, "subtitle", subtitle);
        parent.add(&row);
        row
    }
    fn toggle(
        self: &Rc<Self>,
        parent: &adw::PreferencesGroup,
        key: &str,
        title: &str,
        subtitle: &str,
    ) -> adw::SwitchRow {
        let row = adw::SwitchRow::new();
        self.text(&row, "title", title);
        self.text(&row, "subtitle", subtitle);
        row.set_widget_name(key);
        parent.add(&row);
        let weak = Rc::downgrade(self);
        let k = key.to_owned();
        row.connect_active_notify(move |r| {
            if let Some(this) = weak.upgrade()
                && !this.updating.get()
            {
                this.change(&k, json!(r.is_active()));
            }
        });
        let r = row.clone();
        let k = key.to_owned();
        self.bind(&[key], false, move |v| {
            r.set_active(v[&k].as_bool().unwrap_or(false))
        });
        row
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Binds a configuration key to a native range control with translated title and subtitle"
    )]
    fn spin(
        self: &Rc<Self>,
        parent: &adw::PreferencesGroup,
        key: &str,
        title: &str,
        min: f64,
        max: f64,
        step: f64,
        subtitle: &str,
    ) {
        let row = adw::SpinRow::builder()
            .adjustment(&gtk::Adjustment::new(min, min, max, step, step * 5., 0.))
            .build();
        self.text(&row, "title", title);
        self.text(&row, "subtitle", subtitle);
        row.set_widget_name(key);
        parent.add(&row);
        let weak = Rc::downgrade(self);
        let k = key.to_owned();
        row.connect_value_notify(move |r| {
            if let Some(this) = weak.upgrade()
                && !this.updating.get()
            {
                this.change(&k, json!(r.value() as i64));
            }
        });
        let r = row.clone();
        let k = key.to_owned();
        self.bind(&[key], false, move |v| {
            r.set_value(v[&k].as_f64().unwrap_or(min))
        });
    }
    fn combo(
        self: &Rc<Self>,
        parent: &adw::PreferencesGroup,
        key: &str,
        title: &str,
        ids: &[&str],
        values: &[&str],
        subtitle: &str,
    ) {
        let model = gtk::StringList::new(&[]);
        let row = adw::ComboRow::builder().model(&model).build();
        self.text(&row, "title", title);
        self.text(&row, "subtitle", subtitle);
        row.set_widget_name(key);
        parent.add(&row);
        let choices: Vec<_> = values.iter().map(|s| s.to_string()).collect();
        let weak = Rc::downgrade(self);
        let k = key.to_owned();
        let list = choices.clone();
        row.connect_selected_notify(move |r| {
            if let Some(this) = weak.upgrade()
                && !this.updating.get()
                && let Some(v) = list.get(r.selected() as usize)
            {
                this.change(&k, json!(v));
            }
        });
        let r = row.clone();
        let k = key.to_owned();
        let ids: Vec<_> = ids.iter().map(|s| s.to_string()).collect();
        self.bind(&[key], true, move |v| {
            let labels: Vec<_> = ids
                .iter()
                .enumerate()
                .map(|(index, id)| {
                    if k == "language" && choices[index] != "system" {
                        id.clone()
                    } else {
                        t(id)
                    }
                })
                .collect();
            let current: Vec<_> = (0..model.n_items())
                .filter_map(|i| model.string(i).map(|s| s.to_string()))
                .collect();
            if current != labels {
                model.splice(
                    0,
                    model.n_items(),
                    &labels.iter().map(String::as_str).collect::<Vec<_>>(),
                );
            }
            r.set_selected(
                choices
                    .iter()
                    .position(|s| v[&k].as_str() == Some(s))
                    .unwrap_or(0) as u32,
            );
        });
    }
    fn entry(
        self: &Rc<Self>,
        parent: &adw::PreferencesGroup,
        key: &str,
        title: &str,
        subtitle: &str,
    ) -> adw::EntryRow {
        self.entry_row(parent, key, title, subtitle, false)
    }
    pub(super) fn naming_entry(
        self: &Rc<Self>,
        parent: &adw::PreferencesGroup,
        key: &str,
        title: &str,
        subtitle: &str,
    ) -> adw::EntryRow {
        self.entry_row(parent, key, title, subtitle, true)
    }
    fn entry_row(
        self: &Rc<Self>,
        parent: &adw::PreferencesGroup,
        key: &str,
        title: &str,
        subtitle: &str,
        direct_naming_rules: bool,
    ) -> adw::EntryRow {
        let row = adw::EntryRow::builder().show_apply_button(true).build();
        self.localize_entry_apply_button(row.upcast_ref());
        self.text(&row, "title", title);
        self.text(&row, "tooltip-text", subtitle);
        row.set_widget_name(key);
        if !subtitle.is_empty() {
            if direct_naming_rules {
                let info = gtk::Button::builder()
                    .icon_name("dialog-information-symbolic")
                    .css_classes(["flat"])
                    .valign(gtk::Align::Center)
                    .build();
                info.set_widget_name(&format!("tip-{key}"));
                self.text(&info, "tooltip-text", subtitle);
                let weak = Rc::downgrade(self);
                info.connect_clicked(move |_| {
                    if let Some(this) = weak.upgrade() {
                        this.naming_rules();
                    }
                });
                row.add_suffix(&info);
            } else {
                let hint = gtk::Label::builder()
                    .xalign(0.)
                    .wrap(true)
                    .max_width_chars(36)
                    .margin_start(8)
                    .margin_end(8)
                    .margin_top(6)
                    .margin_bottom(6)
                    .build();
                self.text(&hint, "label", subtitle);
                let popover = gtk::Popover::builder().child(&hint).build();
                let info = gtk::MenuButton::builder()
                    .icon_name("dialog-information-symbolic")
                    .css_classes(["flat"])
                    .valign(gtk::Align::Center)
                    .popover(&popover)
                    .build();
                info.set_widget_name(&format!("tip-{key}"));
                self.text(&info, "tooltip-text", subtitle);
                row.add_suffix(&info);
            }
        }
        parent.add(&row);
        let weak = Rc::downgrade(self);
        let k = key.to_owned();
        row.connect_apply(move |r| {
            if let Some(this) = weak.upgrade() {
                this.change(&k, json!(r.text().as_str()));
            }
        });
        let k = key.to_owned();
        let r = row.clone();
        self.bind(&[key], false, move |v| {
            let value = v[&k].as_str().unwrap_or("");
            if r.text() != value {
                r.set_text(value);
            }
        });
        row
    }
    fn localize_entry_apply_button(&self, widget: &gtk::Widget) {
        if let Some(btn) = widget.downcast_ref::<gtk::Button>()
            && (btn.has_css_class("apply")
                || btn.has_css_class("apply-button")
                || btn.tooltip_text().is_some())
        {
            self.text(btn, "tooltip-text", "Apply");
        }
        let mut child = widget.first_child();
        while let Some(c) = child {
            child = c.next_sibling();
            self.localize_entry_apply_button(&c);
        }
    }
    fn change(&self, key: &str, value: Value) {
        if let Err(e) = self.config.set(key, value) {
            self.toasts.add_toast(adw::Toast::new(&e));
        }
    }
    fn reset_section(
        self: &Rc<Self>,
        parent: &impl IsA<gtk::Widget>,
        name: &str,
        sections: &[&str],
        description: &str,
    ) {
        let group = self.group(parent, "", "");
        let row = self.row(&group, "Reset settings", description);
        row.add_css_class("reset-preferences");
        let button = gtk::Button::builder().valign(gtk::Align::Center).build();
        button.set_widget_name(&format!("reset-{name}"));
        button.add_css_class("destructive-action");
        button.add_css_class("pill");
        self.text(&button, "label", "Reset");
        row.add_suffix(&button);
        let keys = sections
            .iter()
            .flat_map(|s| crate::infrastructure::saving::section_keys(s))
            .collect::<Vec<_>>();
        let schema = config::schema();
        let defaults: Vec<(String, Value)> = keys
            .iter()
            .filter_map(|k| {
                schema
                    .get(k)
                    .map(|entry| (k.clone(), entry["default"].clone()))
            })
            .collect();
        let key_refs: Vec<&str> = keys.iter().map(String::as_str).collect();
        let b = button.clone();
        self.bind(&key_refs, false, move |values| {
            b.set_sensitive(defaults.iter().any(|(k, d)| &values[k] != d));
        });
        let weak = Rc::downgrade(self);
        button.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                let dialog = adw::AlertDialog::builder()
                    .heading(t("Restore default settings?"))
                    .build();
                dialog.add_css_class("clivuu-preferences-dialog");
                dialog.add_responses(&[("cancel", &t("Cancel")), ("reset", &t("Reset"))]);
                dialog.set_response_appearance("reset", adw::ResponseAppearance::Destructive);
                dialog.set_default_response(Some("cancel"));
                dialog.set_close_response("cancel");
                let config = this.config.clone();
                let keys = keys.clone();
                dialog.connect_response(Some("reset"), move |_, _| config.reset(&keys));
                dialog.present(Some(&this.window));
            }
        });
    }
    fn build(self: &Rc<Self>) {
        let general = self.page("general", "General", "preferences-system-symbolic");
        let appearance = self.group(&general, "Interface", "");
        self.combo(
            &appearance,
            "language",
            "Interface language",
            &std::iter::once("System default")
                .chain(clivuu_core::i18n::languages().map(|l| l.native_name()))
                .collect::<Vec<_>>(),
            &std::iter::once("system")
                .chain(clivuu_core::i18n::languages().map(|l| l.id()))
                .collect::<Vec<_>>(),
            "Applies immediately; System default follows your desktop locale",
        );
        self.combo(
            &appearance,
            "theme",
            "Appearance",
            &["Follow system", "Light", "Dark"],
            &["system", "light", "dark"],
            "Controls preferences, toolbars, pins and capture overlays",
        );
        self.preferences_font(&appearance);
        self.color(
            &appearance,
            "accent-color",
            "Accent color",
            "Highlights capture outlines, active pin glow, toolbars and controls",
        );
        let [display, behavior] = self.sectioned_page(
            "capture",
            "Screen capture",
            "camera-photo-symbolic",
            [("display", "Display"), ("behavior", "Behavior")],
        );
        let frame = self.group(
            &display,
            "Selection border",
            "Outline and dimmed overlay shown while choosing a capture area",
        );
        self.spin(
            &frame,
            "capture-border-width",
            "Border width (px)",
            1.,
            8.,
            1.,
            "Thickness of the accent-colored selection border in logical pixels",
        );
        self.color(
            &frame,
            "capture-mask-color",
            "Mask color",
            "Tint and opacity dimming the area outside the selection",
        );
        let anchors = self.group(&display, "Resize anchors", "");
        self.toggle(
            &anchors,
            "capture-show-anchors",
            "Show anchors",
            "Draw circular handles at selection corners and edge midpoints",
        );
        self.color(
            &anchors,
            "capture-anchor-color",
            "Anchor outline color",
            "Stroke color around the resize handles",
        );
        let lens = self.group(
            &display,
            "Magnifier",
            "Shown before selecting, while dragging corners or holding Alt; press C to copy color and Shift to toggle HEX",
        );
        for (key, title, subtitle) in [
            (
                "capture-magnifier-mask",
                "Show mask",
                "Include the outside mask tint inside the magnified preview",
            ),
            (
                "capture-magnifier-border",
                "Show border",
                "Include the selection border inside the magnified preview",
            ),
            (
                "capture-magnifier-anchors",
                "Show anchors",
                "Include resize handles inside the magnified preview",
            ),
        ] {
            self.toggle(&lens, key, title, subtitle);
        }
        let guides = self.group(&display, "Guides and hints", "");
        for (key, title, subtitle) in [
            (
                "capture-crosshair",
                "Show fullscreen crosshair",
                "Extend pointer crosshairs across the screen before selecting, press Alt to toggle",
            ),
            (
                "capture-disable-guides",
                "Disable alignment guides",
                "Hide dashed edge and center extension lines after selecting, press Alt to toggle",
            ),
            (
                "capture-show-tips",
                "Show key binding tips",
                "Display the shortcut cheat sheet while capturing",
            ),
        ] {
            self.toggle(&guides, key, title, subtitle);
        }
        let selection = self.group(&behavior, "Selection", "");
        self.toggle(
            &selection,
            "capture-detect-windows",
            "Smart window detection",
            "Highlight windows under the pointer automatically on Hyprland and niri",
        );
        self.toggle(
            &selection,
            "capture-hide-1x1",
            "Hide the 1 × 1 indicator",
            "Hide the 1 × 1 size label before dragging out a selection",
        );
        let capturing = self.group(&behavior, "While capturing", "");
        self.toggle(
            &capturing,
            "capture-hide-pins",
            "Hide existing pins during capture",
            "Temporarily hide pins during capture and restore them when finished or cancelled",
        );
        self.toggle(
            &capturing,
            "capture-cursor",
            "Capture the cursor",
            "Include the pointer, press ` or ! during capture to show or hide it; on Wayland this briefly uses a one-frame compositor stream",
        );
        let history = self.group(
            &behavior,
            "Capture history",
            "Kept in memory for the current session and cleared when Clivuu exits",
        );
        self.spin(
            &history,
            "capture-history-areas",
            "Saved regions",
            0.,
            100.,
            1.,
            "Maximum selection regions recalled with Page Up and Page Down (0–100), zero disables area history",
        );
        self.toggle(
            &history,
            "capture-history-loop",
            "Loop through history areas",
            "Wrap between the newest and oldest saved regions when pressing Page Up or Page Down",
        );
        self.spin(
            &history,
            "capture-history-records",
            "Captured images",
            0.,
            100.,
            1.,
            "Maximum full captures recalled with , and . (0–100), zero disables it; capped at 128 MiB",
        );
        let scroll = self.group(
            &behavior,
            "Scrolling capture",
            "Used for manual and automatic long-screenshot stitching",
        );
        self.spin(
            &scroll,
            "max-scroll-frames",
            "Retained keyframes",
            2.,
            200.,
            1.,
            "Maximum non-redundant keyframes kept during a scroll session (2–200)",
        );
        self.spin(
            &scroll,
            "scroll-interval",
            "Auto-scroll step delay (ms)",
            300.,
            3000.,
            100.,
            "Wait after each automatic scroll step before capturing the next frame (300–3000 ms)",
        );
        self.toggle(
            &scroll,
            "scroll-auto-crop",
            "Auto-crop on reverse scroll",
            "Automatically trim excess tail or head content when scrolling back",
        );
        self.reset_section(
            &display,
            "capture-display",
            &["capture.display"],
            "Only settings in Screen capture → Display will be reset",
        );
        self.reset_section(
            &behavior,
            "capture-behavior",
            &["capture.behavior", "scrolling"],
            "Only settings in Screen capture → Behavior will be reset",
        );
        self.build_pins();
        self.build_saving();
        self.build_control();
        self.build_ocr();
        self.build_startup(&general);
        self.build_about();
        self.pages.set_visible_child_name("general");
        self.sidebar
            .select_row(self.sidebar.row_at_index(0).as_ref());
    }
}

impl Drop for Preferences {
    fn drop(&mut self) {
        if let Some((manager, id)) = self.theme_subscription.get_mut().take() {
            manager.disconnect(id);
        }
        if let Some((settings, id)) = self.font_subscription.get_mut().take() {
            settings.disconnect(id);
        }
        gtk::style_context_remove_provider_for_display(
            &gtk::prelude::WidgetExt::display(&self.window),
            &self.theme,
        );
    }
}

pub(crate) fn style(dark: bool, font: &str) -> String {
    let colors = if dark {
        [
            "#282a36",
            "#21222c",
            "#343746",
            "#f8f8f2",
            "#b6b7c5",
            "#44475a",
            "#bd93f9",
            "#ff808b",
            "#00000075",
        ]
    } else {
        [
            "#f8f7fc",
            "#eeecf5",
            "#ffffff",
            "#282a36",
            "#626477",
            "#dedbe9",
            "#7952b3",
            "#b92e48",
            "#00000047",
        ]
    };
    let mut css = include_str!("../../resources/preferences.css").to_owned();
    for (token, value) in [
        "background",
        "sidebar",
        "surface",
        "ink",
        "muted",
        "border",
        "accent",
        "danger",
        "scrim",
    ]
    .into_iter()
    .zip(colors)
    {
        css = css.replace(&format!("@prefs_{token}"), value);
    }
    css.push_str(&super::color::style(dark));
    if !font.is_empty() {
        use glib::translate::IntoGlib;
        let desc = gtk::pango::FontDescription::from_string(font);
        if let Some(family) = desc.family() {
            let family = family.replace('\\', "\\\\").replace('"', "\\\"");
            css.push_str(&format!(
                "\n.clivuu-preferences {{ font-family: \"{family}\";"
            ));
            if desc.size() > 0 {
                css.push_str(&format!(
                    " font-size: {}{};",
                    desc.size() as f64 / gtk::pango::SCALE as f64,
                    if desc.is_size_absolute() { "px" } else { "pt" }
                ));
            }
            css.push_str(&format!(
                " font-weight: {}; font-style: {}; }}",
                desc.weight().into_glib().clamp(100, 1000),
                match desc.style() {
                    gtk::pango::Style::Italic => "italic",
                    gtk::pango::Style::Oblique => "oblique",
                    _ => "normal",
                }
            ));
        }
    }
    css
}

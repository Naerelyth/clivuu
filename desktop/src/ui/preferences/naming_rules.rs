//! Readable, selectable reference shared by image and video save settings.
use super::*;
impl Preferences {
    pub(super) fn naming_rules(&self) {
        let dialog = adw::Dialog::builder()
            .title(t("Naming rules"))
            .content_width(600)
            .content_height(600)
            .build();
        dialog.set_widget_name("naming-rules-dialog");
        dialog.add_css_class("clivuu-preferences-dialog");
        let view = adw::ToolbarView::new();
        view.add_css_class("clivuu-preferences");
        let title = gtk::Label::builder()
            .label(t("Naming rules"))
            .css_classes(["heading"])
            .build();
        let header = adw::HeaderBar::builder()
            .title_widget(&title)
            .show_start_title_buttons(false)
            .build();
        view.add_top_bar(&header);
        let page = adw::PreferencesPage::new();
        page.set_focusable(true);
        let intro = gtk::Label::builder()
            .label(t(
                "Wrap date tokens in $…$, images and recordings share these naming rules",
            ))
            .xalign(0.)
            .wrap(true)
            .css_classes(["dim-label"])
            .build();
        intro.set_widget_name("naming-rules-intro");
        let group = adw::PreferencesGroup::new();
        group.add(&intro);
        page.add(&group);
        for (heading, tokens) in [
            (
                "Date and time",
                &[
                    ("d / dd", "Day / zero-padded day"),
                    ("ddd / dddd", "Weekday in English, short / full"),
                    ("M / MM", "Month / zero-padded month"),
                    ("MMM / MMMM", "Month in English, short / full"),
                    ("yy / yyyy", "Year, two / four digits"),
                    ("H / HH", "Hour, 0–23 / 00–23"),
                    ("m / mm", "Minute / zero-padded minute"),
                    ("s / ss", "Second / zero-padded second"),
                    ("z / zzz", "Millisecond / zero-padded millisecond"),
                    ("t", "Time zone"),
                ][..],
            ),
            (
                "Variables",
                &[
                    ("$uuid$", "Random UUID"),
                    ("$unix$ / $unix_ms$", "Unix time, seconds / milliseconds"),
                    ("%os%", "Operating system"),
                    ("%computername%", "Computer name"),
                    ("%username%", "User name"),
                ][..],
            ),
        ] {
            let group = adw::PreferencesGroup::builder().title(t(heading)).build();
            let list = gtk::ListBox::builder()
                .selection_mode(gtk::SelectionMode::None)
                .css_classes(["boxed-list", "naming-rules"])
                .build();
            for (token, description) in tokens {
                let content = gtk::Box::new(gtk::Orientation::Horizontal, 16);
                content.set_margin_top(9);
                content.set_margin_bottom(9);
                content.set_margin_start(14);
                content.set_margin_end(14);
                let code = gtk::Label::builder()
                    .label(*token)
                    .xalign(0.)
                    .width_chars(23)
                    .selectable(true)
                    .css_classes(["monospace", "rule-token"])
                    .build();
                let explanation = gtk::Label::builder()
                    .label(t(description))
                    .xalign(0.)
                    .wrap(true)
                    .hexpand(true)
                    .selectable(true)
                    .build();
                content.append(&code);
                content.append(&explanation);
                let row = gtk::ListBoxRow::builder()
                    .activatable(false)
                    .selectable(false)
                    .child(&content)
                    .build();
                list.append(&row);
            }
            group.add(&list);
            page.add(&group);
        }
        let group = adw::PreferencesGroup::builder()
            .title(t("Organize by year and month"))
            .description(t("Subfolders with / work in Quick save and Automatic save, use $$ for a literal $ and %% for a literal %"))
            .build();
        let example = gtk::Label::builder()
            .label("$yyyy$/$MM$/Clivuu_$yyyy-MM-dd_HH-mm-ss$")
            .xalign(0.)
            .wrap(true)
            .selectable(true)
            .margin_top(14)
            .margin_bottom(14)
            .margin_start(14)
            .margin_end(14)
            .css_classes(["monospace"])
            .build();
        let box_ = gtk::Box::new(gtk::Orientation::Vertical, 0);
        box_.add_css_class("card");
        box_.add_css_class("naming-example");
        box_.append(&example);
        group.add(&box_);
        page.add(&group);
        view.set_content(Some(&page));
        dialog.set_child(Some(&view));
        let keys = gtk::EventControllerKey::new();
        let weak = dialog.downgrade();
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gdk::Key::Escape {
                if let Some(dialog) = weak.upgrade() {
                    let _ = dialog.close();
                }
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        dialog.add_controller(keys);
        // Start on the reference page, not a selectable label: an empty label
        // selection still draws a caret. Tokens remain selectable on demand.
        dialog.set_focus(Some(&page));
        dialog.present(Some(&self.window));
    }
}

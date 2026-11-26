//! Read-only key reference and live, portable pin gesture bindings.
use super::*;
use crate::editor::shortcuts::TOOLS;
use clivuu_core::pins::mouse::{self, Action, Binding as MouseBinding, Trigger};

impl Preferences {
    pub(super) fn build_control(self: &Rc<Self>) {
        let [keys, mouse] = self.sectioned_page(
            "control",
            "Control",
            "input-keyboard-symbolic",
            [("keys", "Internal shortcuts"), ("mouse", "Mouse")],
        );
        let intro = self.group(&keys, "Keyboard shortcuts", "Internal shortcuts are fixed, with text fields, menus and native controls taking priority; configure global shortcuts in your desktop or window manager");
        let search = gtk::SearchEntry::new();
        search.set_widget_name("shortcut-search");
        self.text(&search, "placeholder-text", "Filter shortcuts");
        intro.add(&search);
        type ShortcutGroup = (
            adw::PreferencesGroup,
            Vec<(adw::ActionRow, &'static str, &'static str)>,
        );
        let mut groups: Vec<ShortcutGroup> = Vec::new();
        let tools = self.group(&keys, "Annotation tools", "");
        let tool_rows = TOOLS
            .iter()
            .map(|&(_, _, title, chord)| (self.key_row(&tools, title, chord), title, chord))
            .collect();
        groups.push((tools, tool_rows));
        for (title, description, rows) in [
            (
                "Shared editing",
                "",
                &[
                    ("Undo", "Ctrl+Z"),
                    ("Redo", "Ctrl+Y"),
                    ("Remove all annotations permanently", "Ctrl+Shift+Z"),
                    ("Delete selected annotation", "Delete / Backspace"),
                    ("Deselect annotation", "Right-click"),
                    (
                        "Adjust stroke size or rectangular mosaic strength",
                        "Wheel / 1 / 2 / [ / ]",
                    ),
                    ("Constrain shapes and arrows", "Shift"),
                    ("Increase next sequence number", "Z"),
                    ("Decrease next sequence number", "X"),
                    ("Copy image", "Ctrl+C"),
                    ("Save image", "Ctrl+S"),
                    ("Quick save", "Ctrl+Shift+S"),
                    ("Show / hide toolbar", "Space"),
                ][..],
            ),
            (
                "Screenshot selection",
                "Available during screenshot selection",
                &[
                    ("Select current screen / full desktop", "Ctrl+A"),
                    ("Use last successful capture region", "R"),
                    ("Previous / next captured image", ", / ."),
                    ("Previous / next region", "Page Up / Page Down"),
                    ("Show / hide captured cursor", "` / !"),
                    ("Refresh screenshot", "F5"),
                    ("Move selection by one pixel", "↑ / ↓ / ← / →"),
                    ("Expand selection boundary", "Ctrl+↑ / ↓ / ← / →"),
                    ("Shrink selection boundary", "Shift+↑ / ↓ / ← / →"),
                    ("Pin selected image", "Ctrl+T"),
                    ("Confirm and copy", "Enter"),
                    ("Cancel capture", "Esc"),
                    ("Reset region / cancel capture", "Right-click"),
                    ("Recognize text", "O"),
                    ("Quick copy text", "Shift+O"),
                    ("Scrolling capture", "L"),
                    ("Region recording", "V"),
                ][..],
            ),
            (
                "Pins",
                "Number keys transform pins when no annotation tool is active",
                &[
                    ("Rotate right / left", "1 / 2"),
                    ("Flip horizontally / vertically", "3 / 4"),
                    ("Toggle grayscale / inversion", "5 / 6"),
                    ("Move pin by one logical pixel", "↑ / ↓ / ← / →"),
                    ("Select visible pins", "Ctrl+A"),
                    ("Copy original text", "Ctrl+Shift+C"),
                    ("Recognize text", "O"),
                    ("Replace pin from clipboard", "Ctrl+V"),
                    ("Preferences", "Ctrl+Shift+P"),
                    ("Open toolbar / apply and hide", "Space"),
                    ("Apply and hide toolbar / close pin", "Esc"),
                    ("Apply changes", "Enter"),
                    ("Close pin", "Ctrl+W"),
                    ("Destroy pin", "Shift+Esc"),
                ][..],
            ),
            (
                "Scrolling capture",
                "",
                &[
                    ("Pin image", "Enter"),
                    ("Copy image", "Ctrl+C"),
                    ("Save image", "Ctrl+S"),
                    ("Quick save", "Ctrl+Shift+S"),
                    ("Restart in current region", "Ctrl+R"),
                    ("Cancel capture", "Esc"),
                ][..],
            ),
            (
                "Magnifier",
                "Color shortcuts apply while the magnifier is visible",
                &[
                    ("Show magnifier / toggle capture guides", "Alt"),
                    ("Switch RGB / HEX", "Shift"),
                    ("Copy color value", "C"),
                ][..],
            ),
        ] {
            let group = self.group(&keys, title, description);
            let action_rows = rows
                .iter()
                .map(|&(action, chord)| (self.key_row(&group, action, chord), action, chord))
                .collect();
            groups.push((group, action_rows));
        }
        let empty_group = adw::PreferencesGroup::new();
        empty_group.set_visible(false);
        let empty_row = adw::ActionRow::new();
        empty_row.set_widget_name("shortcut-search-empty");
        empty_row.set_activatable(false);
        empty_row.add_prefix(&gtk::Image::from_icon_name("edit-find-symbolic"));
        empty_row.add_css_class("dim-label");
        self.text(&empty_row, "title", "No matching shortcuts");
        empty_group.add(&empty_row);
        keys.add(&empty_group);
        let search_filter = search.clone();
        let apply_filter = Rc::new(move || {
            let query = search_filter.text().trim().to_lowercase();
            let mut any_group = false;
            for (group, rows) in &groups {
                let mut any_row = false;
                for &(ref row, title, chord) in rows {
                    let matched = query.is_empty()
                        || t(title).to_lowercase().contains(&query)
                        || t(chord).to_lowercase().contains(&query)
                        || title.to_lowercase().contains(&query)
                        || chord.to_lowercase().contains(&query);
                    row.set_visible(matched);
                    any_row |= matched;
                }
                group.set_visible(query.is_empty() || any_row);
                any_group |= any_row;
            }
            empty_group.set_visible(!query.is_empty() && !any_group);
        });
        let on_search = apply_filter.clone();
        search.connect_search_changed(move |_| on_search());
        self.bind(&[], true, move |_| apply_filter());
        let gestures = self.group(&mouse, "Pin gestures", "Apply to pins outside annotation editing, higher rows take priority on conflicts; +/- keys share the gesture modifiers; modifier gestures need focus (open the toolbar and choose Cursor if needed)");
        for action in Action::ALL {
            self.mouse_row(&gestures, action);
        }
        self.reset_section(
            &mouse,
            "control-mouse",
            &["control.mouse"],
            "Only settings in Control → Mouse will be reset",
        );
    }
    fn key_row(&self, group: &adw::PreferencesGroup, title: &str, chord: &str) -> adw::ActionRow {
        let row = self.row(group, title, "");
        let label = gtk::Label::builder().valign(gtk::Align::Center).build();
        self.text(&label, "label", chord);
        label.add_css_class("dim-label");
        row.add_suffix(&label);
        row
    }
    fn mouse_choices(&self, name: &str, labels: &[&str], hint: &str) -> gtk::DropDown {
        let model = gtk::StringList::new(labels);
        let choice = gtk::DropDown::builder()
            .model(&model)
            .valign(gtk::Align::Center)
            .build();
        choice.set_widget_name(name);
        self.text(&choice, "tooltip-text", hint);
        let labels = labels.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        self.bind(&[], true, move |_| {
            let translated = labels.iter().map(|s| t(s)).collect::<Vec<_>>();
            if translated
                .iter()
                .enumerate()
                .any(|(i, text)| model.string(i as u32).as_deref() != Some(text.as_str()))
            {
                model.splice(
                    0,
                    model.n_items(),
                    &translated.iter().map(String::as_str).collect::<Vec<_>>(),
                );
            }
        });
        choice
    }
    fn mouse_row(self: &Rc<Self>, group: &adw::PreferencesGroup, action: Action) {
        let row = adw::ExpanderRow::new();
        row.set_use_markup(false);
        row.set_widget_name(action.key());
        self.text(&row, "title", action.title());
        group.add(&row);
        let first = self.mouse_choices(
            &format!("{}-modifier-1", action.key()),
            &["None", "Ctrl", "Shift", "Alt", "Super"],
            "First modifier",
        );
        let second = self.mouse_choices(
            &format!("{}-modifier-2", action.key()),
            &["None", "Ctrl", "Shift", "Alt", "Super"],
            "Second modifier",
        );
        let triggers: Vec<_> = if action.keys_key().is_some() {
            vec![Trigger::None, Trigger::Wheel]
        } else {
            vec![
                Trigger::None,
                Trigger::DoubleLeft,
                Trigger::Middle,
                Trigger::Right,
            ]
        };
        let labels = if action.keys_key().is_some() {
            vec!["None", "Mouse wheel"]
        } else {
            vec!["None", "Left double-click", "Middle-click", "Right-click"]
        };
        let trigger = self.mouse_choices(
            &format!("{}-trigger", action.key()),
            &labels,
            "Mouse action",
        );
        let add_subrow = |title: &str, control: &gtk::DropDown| {
            let sub = adw::ActionRow::new();
            sub.set_use_markup(false);
            self.text(&sub, "title", title);
            sub.add_suffix(control);
            sub.set_activatable_widget(Some(control));
            row.add_row(&sub);
        };
        add_subrow("First modifier", &first);
        add_subrow("Second modifier", &second);
        add_subrow("Mouse action", &trigger);
        let keys = action.keys_key().map(|key| {
            let choice = self.mouse_choices(key, &["None", "+ / −"], "Alternative keys");
            add_subrow("Alternative keys", &choice);
            let weak = Rc::downgrade(self);
            choice.connect_selected_notify(move |choice| {
                if let Some(this) = weak.upgrade()
                    && !this.updating.get()
                {
                    this.change(key, json!(choice.selected() == 1));
                }
            });
            choice
        });
        let weak = Rc::downgrade(self);
        let controls = [first.downgrade(), second.downgrade(), trigger.downgrade()];
        let available = triggers.clone();
        let commit = Rc::new(move || {
            if let Some(this) = weak.upgrade()
                && !this.updating.get()
                && let [Some(first), Some(second), Some(trigger)] =
                    controls.each_ref().map(glib::WeakRef::upgrade)
            {
                let modifier = |index: u32| {
                    mouse::MODIFIERS
                        .get(index.wrapping_sub(1) as usize)
                        .map_or(0, |m| m.1)
                };
                let binding = MouseBinding {
                    modifiers: modifier(first.selected()) | modifier(second.selected()),
                    trigger: available[trigger.selected() as usize],
                };
                this.change(action.key(), json!(binding.encode()));
            }
        });
        let modifier_weak = [first.downgrade(), second.downgrade()];
        for (slot, control) in [&first, &second].into_iter().enumerate() {
            let other = modifier_weak[1 - slot].clone();
            let commit = commit.clone();
            let weak = Rc::downgrade(self);
            control.connect_selected_notify(move |changed| {
                if let Some(this) = weak.upgrade()
                    && !this.updating.get()
                {
                    let selected = changed.selected();
                    if selected != 0
                        && let Some(other) = other.upgrade()
                        && other.selected() == selected
                    {
                        this.updating.set(true);
                        other.set_selected(0);
                        this.updating.set(false);
                    }
                    commit();
                }
            });
        }
        trigger.connect_selected_notify(move |_| commit());
        let mut deps = vec![action.key()];
        if let Some(k) = action.keys_key() {
            deps.push(k);
        }
        self.bind(&deps, true, move |values| {
            row.set_subtitle(&mouse_summary(action, values));
            let binding = mouse::Bindings::from_values(values).binding(action);
            let selected = mouse::MODIFIERS
                .iter()
                .enumerate()
                .filter(|(_, m)| binding.modifiers & m.1 != 0)
                .map(|(i, _)| i as u32 + 1)
                .collect::<Vec<_>>();
            first.set_selected(selected.first().copied().unwrap_or(0));
            second.set_selected(selected.get(1).copied().unwrap_or(0));
            trigger.set_selected(
                triggers
                    .iter()
                    .position(|t| *t == binding.trigger)
                    .unwrap_or(0) as u32,
            );
            if let Some(keys) = &keys {
                keys.set_selected(u32::from(
                    values[action.keys_key().unwrap()]
                        .as_bool()
                        .unwrap_or(false),
                ));
            }
        });
    }
}

fn mouse_summary(action: Action, values: &Values) -> String {
    const MODIFIER_NAMES: [(&str, u8); 4] = [
        ("Ctrl", mouse::CONTROL),
        ("Shift", mouse::SHIFT),
        ("Alt", mouse::ALT),
        ("Super", mouse::SUPER),
    ];
    let binding = mouse::Bindings::from_values(values).binding(action);
    let mods: Vec<String> = MODIFIER_NAMES
        .iter()
        .filter(|(_, flag)| binding.modifiers & flag != 0)
        .map(|(name, _)| t(name))
        .collect();
    let with_mods = |tail: String| {
        mods.iter()
            .cloned()
            .chain(std::iter::once(tail))
            .collect::<Vec<_>>()
            .join(" + ")
    };
    let mut parts = Vec::new();
    let trigger = match binding.trigger {
        Trigger::None => None,
        Trigger::Wheel => Some(t("Mouse wheel")),
        Trigger::DoubleLeft => Some(t("Left double-click")),
        Trigger::Middle => Some(t("Middle-click")),
        Trigger::Right => Some(t("Right-click")),
    };
    if let Some(label) = trigger {
        parts.push(with_mods(label));
    }
    if action.keys_key().is_some_and(|k| values[k] == true) {
        parts.push(with_mods(t("+ / −")));
    }
    if parts.is_empty() {
        t("Disabled")
    } else {
        parts.join("  ·  ")
    }
}

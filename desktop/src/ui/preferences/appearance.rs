//! Settings-only font controls and a dialog host for the shared color editor.
use super::*;
use crate::ui::color::ColorPanel;
use clivuu_core::annotation::color::ColorDraft;

impl Preferences {
    pub(super) fn preferences_font(self: &Rc<Self>, parent: &adw::PreferencesGroup) {
        let row = self.row(
            parent,
            "Preferences font",
            "Used only in Preferences and dialogs opened from it",
        );
        let dialog = gtk::FontDialog::new();
        self.text(&dialog, "title", "Preferences font");
        let font = gtk::FontDialogButton::builder()
            .dialog(&dialog)
            .use_font(false)
            .use_size(false)
            .valign(gtk::Align::Center)
            .build();
        font.set_widget_name("preferences-font");
        row.add_suffix(&font);
        row.set_activatable_widget(Some(&font));
        let weak = Rc::downgrade(self);
        font.connect_font_desc_notify(move |button| {
            if let Some(this) = weak.upgrade()
                && !this.updating.get()
                && let Some(desc) = button.font_desc()
            {
                this.change("preferences-font", json!(desc.to_string()));
            }
        });
        let reset = gtk::Button::builder()
            .icon_name("edit-undo-symbolic")
            .css_classes(["flat"])
            .valign(gtk::Align::Center)
            .build();
        reset.set_widget_name("reset-preferences-font");
        self.text(&reset, "tooltip-text", "Use system font");
        row.add_suffix(&reset);
        let config = self.config.clone();
        reset.connect_clicked(move |_| config.reset(&["preferences-font".into()]));
        let settings = self.window.settings();
        self.bind(&["preferences-font"], false, move |values| {
            let selected = values["preferences-font"].as_str().unwrap_or("");
            let system = settings.gtk_font_name().unwrap_or_default();
            font.set_font_desc(&gtk::pango::FontDescription::from_string(
                if selected.is_empty() {
                    &system
                } else {
                    selected
                },
            ));
            reset.set_sensitive(!selected.is_empty());
        });
        let weak = Rc::downgrade(self);
        let settings = self.window.settings();
        let id = settings.connect_gtk_font_name_notify(move |_| {
            if let Some(this) = weak.upgrade() {
                this.refresh(&this.config.values());
            }
        });
        self.font_subscription.replace(Some((settings, id)));
    }

    pub(super) fn color(
        self: &Rc<Self>,
        parent: &adw::PreferencesGroup,
        key: &str,
        title: &str,
        subtitle: &str,
    ) -> adw::ActionRow {
        let row = self.row(parent, title, subtitle);
        let button = gtk::Button::builder()
            .valign(gtk::Align::Center)
            .css_classes(["preferences-color-button"])
            .build();
        button.set_widget_name(key);
        self.text(&button, "tooltip-text", title);
        let swatch = gtk::DrawingArea::builder()
            .content_width(28)
            .content_height(20)
            .build();
        let current = Rc::new(RefCell::new(gdk::RGBA::BLACK));
        let color = current.clone();
        swatch.set_draw_func(move |_, c, width, height| {
            for y in (0..height).step_by(5) {
                for x in (0..width).step_by(5) {
                    let grey = if (x / 5 + y / 5) % 2 == 0 { 0.82 } else { 0.62 };
                    c.set_source_rgb(grey, grey, grey);
                    c.rectangle(x as f64, y as f64, 5., 5.);
                    let _ = c.fill();
                }
            }
            let rgba = color.borrow();
            c.set_source_rgba(
                rgba.red().into(),
                rgba.green().into(),
                rgba.blue().into(),
                rgba.alpha().into(),
            );
            let _ = c.paint();
        });
        button.set_child(Some(&swatch));
        row.add_suffix(&button);
        row.set_activatable_widget(Some(&button));
        let reset = gtk::Button::builder()
            .icon_name("edit-undo-symbolic")
            .css_classes(["flat"])
            .valign(gtk::Align::Center)
            .build();
        reset.set_widget_name(&if key == "accent-color" {
            "reset-accent".to_owned()
        } else {
            format!("reset-{key}")
        });
        self.text(&reset, "tooltip-text", "Restore default color");
        let config = self.config.clone();
        let reset_key = key.to_owned();
        reset.connect_clicked(move |_| config.reset(std::slice::from_ref(&reset_key)));
        row.add_suffix(&reset);
        let weak = Rc::downgrade(self);
        let (k, title) = (key.to_owned(), title.to_owned());
        button.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.color_dialog(&k, &title);
            }
        });
        let default = config::schema()[key]["default"].clone();
        let key_name = key.to_owned();
        self.bind(&[key], false, move |values| {
            let text = values[&key_name].as_str().expect("validated color");
            if let Ok(color) = gdk::RGBA::parse(text) {
                current.replace(color);
                swatch.queue_draw();
            }
            reset.set_sensitive(values[&key_name] != default);
        });
        row
    }

    fn color_dialog(self: &Rc<Self>, key: &str, title: &str) {
        // Keep modal input inside the settings surface. A separate modal
        // toplevel blocks its parent from restoring the cursor after picking.
        let window = adw::Dialog::builder()
            .title(t(title))
            .content_width(560)
            .build();
        window.set_widget_name("preferences-color-dialog");
        window.add_css_class("clivuu-preferences-dialog");
        let values = self.config.values();
        let initial = values[key].as_str().expect("validated color");
        let alpha_enabled = config::schema()[key]["alpha"] == true;
        let draft = ColorDraft::parse(initial, 255).unwrap();
        let mut services = self.color_services.clone();
        let config = self.config.clone();
        services.load = Some(Rc::new(move || {
            config
                .value("custom-colors")
                .as_array()
                .map(|colors| {
                    colors
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default()
        }));
        let weak = Rc::downgrade(self);
        services.save = Some(Rc::new(move |colors| {
            if let Some(this) = weak.upgrade() {
                this.change("custom-colors", json!(colors));
            }
        }));
        let weak = Rc::downgrade(self);
        let key = key.to_owned();
        let close = window.downgrade();
        let panel = ColorPanel::for_dialog(
            draft,
            services,
            alpha_enabled,
            move |patch| {
                if let Some(this) = weak.upgrade()
                    && let Some(color) = patch.color
                {
                    let value = if alpha_enabled {
                        format!("{color}{:02x}", patch.alpha.unwrap_or(255))
                    } else {
                        color
                    };
                    this.change(&key, json!(value));
                }
            },
            move || {
                if let Some(window) = close.upgrade() {
                    window.close();
                }
            },
        );
        let view = adw::ToolbarView::new();
        view.add_css_class("clivuu-preferences");
        let header = adw::HeaderBar::builder()
            .title_widget(
                &gtk::Label::builder()
                    .label(t(title))
                    .css_classes(["heading"])
                    .build(),
            )
            .show_start_title_buttons(false)
            .show_end_title_buttons(false)
            .build();
        let cancel = gtk::Button::with_label(&t("Cancel"));
        cancel.set_widget_name("cancel-color");
        let close = window.downgrade();
        cancel.connect_clicked(move |_| {
            if let Some(window) = close.upgrade() {
                window.close();
            }
        });
        let accept = gtk::Button::with_label(&t("Select"));
        accept.set_widget_name("accept-color");
        accept.add_css_class("suggested-action");
        let selected = panel.clone();
        accept.connect_clicked(move |_| selected.commit());
        header.pack_start(&cancel);
        header.pack_end(&accept);
        view.add_top_bar(&header);
        let content = panel.widget();
        for edge in ["margin-top", "margin-bottom", "margin-start", "margin-end"] {
            content.set_property(edge, 16);
        }
        let scroll = gtk::ScrolledWindow::builder()
            .child(content)
            .propagate_natural_width(true)
            .propagate_natural_height(true)
            .max_content_width(680)
            .max_content_height(560)
            .build();
        view.set_content(Some(&scroll));
        window.set_child(Some(&view));
        window.set_default_widget(Some(&accept));
        let closing = panel.clone();
        window.connect_unrealize(move |_| closing.dismiss());
        window.set_focus(Some(&accept));
        window.present(Some(&self.window));
    }
}

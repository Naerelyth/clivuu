//! Pin sections bind to core policy; text preview uses the real paste renderer.
use super::*;
use crate::{application::text_image, infrastructure::tasks::TaskPool};
use clivuu_core::pins::preferences::{Preferences as PinPreferences, TextOptions};

impl Preferences {
    pub(super) fn build_pins(self: &Rc<Self>) {
        let [display, behavior, conversion] = self.sectioned_page(
            "pins",
            "Pins",
            "view-paged-symbolic",
            [
                ("display", "Display"),
                ("behavior", "Behavior"),
                ("conversion", "Conversion"),
            ],
        );
        let appearance = self.group(
            &display,
            "Pin appearance",
            "Default shadow and opacity apply to new pins; shadow color and size limit apply immediately",
        );
        self.toggle(
            &appearance,
            "pin-shadow",
            "Window shadow",
            "Draw a drop shadow around new pins",
        );
        self.color(
            &appearance,
            "pin-shadow-tint",
            "Shadow color",
            "Shadow color for inactive pins; active or selected pins use the accent color",
        );
        self.spin(
            &appearance,
            "pin-opacity",
            "Default opacity (%)",
            10.,
            100.,
            1.,
            "Initial opacity applied to new pins (10–100%)",
        );
        self.spin(
            &appearance,
            "pin-max-size",
            "Maximum window size",
            100.,
            100000.,
            100.,
            "Caps the longest side of new and existing pins, in logical pixels",
        );
        let thumbnail = self.group(
            &display,
            "Quick thumbnail size",
            "Target bounds when collapsing a pin into a thumbnail",
        );
        self.spin(
            &thumbnail,
            "pin-thumbnail-width",
            "Width",
            24.,
            1000.,
            1.,
            "Maximum thumbnail width in logical pixels (24–1000)",
        );
        self.spin(
            &thumbnail,
            "pin-thumbnail-height",
            "Height",
            24.,
            1000.,
            1.,
            "Maximum thumbnail height in logical pixels (24–1000)",
        );
        let geometry = self.group(&behavior, "Position and scaling", "");
        for (key, title, subtitle) in [
            (
                "pin-use-snip-position",
                "Use snipping position",
                "Otherwise, new capture pins are centered on the primary display",
            ),
            (
                "pin-zoom-at-cursor",
                "Zoom at mouse position",
                "When disabled, wheel zoom keeps the top-left corner fixed",
            ),
            (
                "pin-smooth-scaling",
                "Smooth scaling",
                "Use smooth image interpolation on new pins, disable for crisp pixel edges",
            ),
            (
                "pin-reset-thumbnail",
                "Allow resetting thumbnails",
                "Reset also restores thumbnails to their original 100% size",
            ),
        ] {
            self.toggle(&geometry, key, title, subtitle);
        }
        let feedback = self.group(&behavior, "Pin feedback", "");
        for (key, title, subtitle) in [
            (
                "pin-translucent-flash",
                "Translucent when flashing",
                "Reduce pin opacity to 55% while a pin is flashing",
            ),
            (
                "pin-flash-group",
                "Flash pins after switching groups",
                "Briefly highlight pins in the newly selected group",
            ),
            (
                "pin-hide-on-flash",
                "Paste while flashing hides all pins",
                "Press the paste shortcut again while a pin is flashing to hide all pins",
            ),
            (
                "pin-flash-new",
                "Flash new pins",
                "Briefly flash newly created pins to highlight where they appear",
            ),
            (
                "pin-activate-new",
                "Activate new pins",
                "Select and focus newly created pins automatically",
            ),
        ] {
            self.toggle(&feedback, key, title, subtitle);
        }
        let pin_groups = self.group(&behavior, "Pin groups", "");
        let manage_row = self.row(
            &pin_groups,
            "Manage pin groups",
            "Create, rename, color-code, reorder, or delete pin groups",
        );
        let manage_btn = gtk::Button::builder()
            .valign(gtk::Align::Center)
            .css_classes(["pill"])
            .build();
        manage_btn.set_widget_name("open-group-manager");
        self.text(&manage_btn, "label", "Manage groups");
        manage_row.add_suffix(&manage_btn);
        manage_row.set_activatable_widget(Some(&manage_btn));
        manage_btn.connect_clicked(|_| {
            if let Some(conn) = gtk::gio::Application::default().and_then(|a| a.dbus_connection()) {
                glib::MainContext::default().spawn_local(async move {
                    let _ = crate::application::group_manager::desktop_execute(
                        &conn,
                        json!({ "command": "show-group-manager" }),
                    )
                    .await;
                });
            }
        });
        let focus = self.group(&behavior, "GNOME", "");
        let focus_row = self.toggle(
            &focus,
            "pin-gnome-focus",
            "Allow pins to take keyboard focus",
            "When disabled, clicking pins preserves Dash-to-Panel and Dash-to-Dock smart hiding, but pins cannot receive keyboard input and activation is limited",
        );
        if !crate::platform::display::is_gnome() {
            focus_row.set_sensitive(false);
            self.text(
                &focus_row,
                "tooltip-text",
                "Only available on GNOME sessions",
            );
        }
        let lifecycle = self.group(&behavior, "Pin lifecycle", "");
        self.toggle(
            &lifecycle,
            "pin-preserve-drag",
            "Preserve the original pin during drag and drop",
            "Keep the original visible when dropping a new image onto it, hold Shift to preserve it temporarily",
        );
        self.toggle(
            &lifecycle,
            "pin-confirm-close",
            "Confirm closing multiple pins",
            "Ask for confirmation when closing 2 or more pins at once",
        );
        self.toggle(
            &lifecycle,
            "pin-confirm-destroy",
            "Confirm destroying multiple pins",
            "Ask for confirmation when permanently destroying 2 or more pins at once",
        );
        self.spin(
            &lifecycle,
            "pin-closed-limit",
            "Recoverable closed pins",
            0.,
            128.,
            1.,
            "Maximum closed pins kept for reopening (0–128), oldest are destroyed when full",
        );
        let memory = adw::SwitchRow::new();
        self.text(&memory, "title", "Store pins in memory");
        self.text(
            &memory,
            "subtitle",
            "When enabled, pins are discarded when Clivuu restarts",
        );
        memory.set_widget_name("pin-storage");
        lifecycle.add(&memory);
        let weak = Rc::downgrade(self);
        memory.connect_active_notify(move |row| {
            if let Some(this) = weak.upgrade()
                && !this.updating.get()
            {
                this.change(
                    "pin-storage",
                    json!(if row.is_active() {
                        "memory"
                    } else {
                        "persistent"
                    }),
                );
            }
        });
        self.bind(&["pin-storage"], false, move |values| {
            memory.set_active(values["pin-storage"] == "memory");
        });
        let text = self.group(&conversion, "Text to image", "");
        self.toggle(
            &text,
            "pin-text-enabled",
            "Convert text to images",
            "When disabled, #rrggbb color cards and image files can still be pasted as pins",
        );
        let options = self.group(&conversion, "Text conversion", "");
        self.toggle(
            &options,
            "pin-text-ignore-html",
            "Always ignore HTML formatting",
            "Render copied rich text as plain text using the font and colors below",
        );
        self.toggle(
            &options,
            "pin-text-skip-paths",
            "Never paste file paths",
            "Ignore copied file path text while still pasting image files as images",
        );
        self.spin(
            &options,
            "pin-text-margin",
            "Page margin",
            0.,
            100.,
            1.,
            "Blank space around converted text in logical pixels (0–100)",
        );
        self.spin(
            &options,
            "pin-text-max-width",
            "Maximum image width",
            100.,
            10000.,
            10.,
            "Wrap text within this width including margins (100–10000 px)",
        );
        let row = self.row(
            &options,
            "Font",
            "Font family and size used when rendering plain text as a pin",
        );
        let dialog = gtk::FontDialog::new();
        self.text(&dialog, "title", "Font");
        let font = gtk::FontDialogButton::builder()
            .dialog(&dialog)
            .valign(gtk::Align::Center)
            .build();
        font.set_widget_name("pin-text-font");
        row.add_suffix(&font);
        row.set_activatable_widget(Some(&font));
        let weak = Rc::downgrade(self);
        font.connect_font_desc_notify(move |b| {
            if let Some(this) = weak.upgrade()
                && !this.updating.get()
                && let Some(desc) = b.font_desc()
            {
                this.change("pin-text-font", json!(desc.to_string()));
            }
        });
        let f = font.clone();
        self.bind(&["pin-text-font"], false, move |v| {
            f.set_font_desc(&gtk::pango::FontDescription::from_string(
                v["pin-text-font"].as_str().unwrap_or("Sans 9"),
            ))
        });
        self.color(
            &options,
            "pin-text-background",
            "Background color",
            "Canvas fill color for converted plain-text pins",
        );
        self.color(
            &options,
            "pin-text-foreground",
            "Text color",
            "Ink color for converted plain-text pins",
        );
        let preview = gtk::Box::builder()
            .halign(gtk::Align::Center)
            .margin_top(16)
            .margin_bottom(16)
            .build();
        preview.set_widget_name("pin-text-preview");
        let preview_group = self.group(
            &conversion,
            "Preview",
            "Sample rendered with the text conversion options above",
        );
        preview_group.add(&preview);
        let pool = self.config.pool();
        let state = Rc::new(Preview {
            widget: preview.downgrade(),
            pool,
            requested: RefCell::new(None),
            rendering: Cell::new(false),
        });
        self.bind(
            &[
                "pin-text-enabled",
                "pin-text-ignore-html",
                "pin-text-skip-paths",
                "pin-text-margin",
                "pin-text-max-width",
                "pin-text-font",
                "pin-text-background",
                "pin-text-foreground",
            ],
            false,
            move |v| {
                options.set_sensitive(v["pin-text-enabled"] == true);
                let next = PinPreferences::from_values(v).text;
                if state.requested.borrow().as_ref() != Some(&next) {
                    state.requested.replace(Some(next));
                    state.render();
                }
            },
        );
        for (page, name, sections, description) in [
            (
                &display,
                "pins-display",
                vec!["pins.display"],
                "Only settings in Pins → Display will be reset",
            ),
            (
                &behavior,
                "pins-behavior",
                vec!["pins.behavior", "pins"],
                "Only settings in Pins → Behavior will be reset",
            ),
            (
                &conversion,
                "pins-conversion",
                vec!["pins.conversion"],
                "Only settings in Pins → Conversion will be reset",
            ),
        ] {
            self.reset_section(page, name, &sections, description);
        }
    }
}
struct Preview {
    widget: glib::WeakRef<gtk::Box>,
    pool: Rc<TaskPool>,
    requested: RefCell<Option<TextOptions>>,
    rendering: Cell<bool>,
}
impl Preview {
    fn render(self: &Rc<Self>) {
        if self.rendering.replace(true) {
            return;
        }
        let Some(widget) = self.widget.upgrade() else {
            return;
        };
        let options = self.requested.borrow().clone().unwrap();
        let scale = widget.scale_factor() as f64;
        let input = options.clone();
        let job = self.pool.submit(move |_| {
            text_image::render_document_with_options(
                &clivuu_core::paste::html::Document::plain_text("AaOo12"),
                &input,
                scale,
            )
            .map_err(crate::infrastructure::tasks::TaskError::Failed)
        });
        let this = self.clone();
        glib::MainContext::default().spawn_local(async move {
            let result = match job {
                Ok(job) => job.result().await,
                Err(crate::infrastructure::tasks::TaskError::Busy) => {
                    glib::timeout_future(std::time::Duration::from_millis(75)).await;
                    this.rendering.set(false);
                    if this.widget.upgrade().is_some() {
                        this.render();
                    }
                    return;
                }
                Err(e) => Err(e),
            };
            this.rendering.set(false);
            if this.requested.borrow().as_ref() != Some(&options) {
                this.render();
                return;
            }
            if let Ok((pixels, size)) = result
                && let Some(widget) = this.widget.upgrade()
            {
                let width = pixels.width();
                let height = pixels.height();
                let texture = gdk::MemoryTexture::new(
                    width as i32,
                    height as i32,
                    gdk::MemoryFormat::R8g8b8a8,
                    &glib::Bytes::from_owned(pixels.into_raw()),
                    width as usize * 4,
                );
                let fit = (500. / size[0] as f64).min(240. / size[1] as f64).min(1.);
                let width = size[0] as f64 * fit;
                let height = size[1] as f64 * fit;
                let image = crate::ui::pin_image::PinImage::new(texture.upcast_ref());
                image.configure(false, true);
                image.update(width, height, &gdk::RGBA::TRANSPARENT, true);
                image.set_size_request(width.ceil() as i32 + 24, height.ceil() as i32 + 24);
                while let Some(child) = widget.first_child() {
                    widget.remove(&child);
                }
                widget.append(&image);
            }
        });
    }
}

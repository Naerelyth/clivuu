//! Save-page controls publish to the same schema as CLI and desktop workflows.
use super::*;
use crate::infrastructure::saving::{self, Mode};
use clivuu_core::saving::{Media, NameContext};
use gtk::gio;

impl Preferences {
    pub(super) fn build_saving(self: &Rc<Self>) {
        let [images, recording] = self.sectioned_page(
            "save",
            "Save",
            "folder-download-symbolic",
            [("images", "Images"), ("recording", "Recording")],
        );
        let example = saving::now().unwrap_or_else(|_| NameContext::example());
        let manual = self.group(
            &images,
            "Manual save",
            "Choose location and format in the system file chooser, supports PNG, JPG and JPEG",
        );
        self.pattern(
            &manual,
            "image-manual-name",
            "File name",
            Media::Image,
            Mode::Manual,
            &example,
            None,
        );
        self.toggle(
            &manual,
            "image-remember-extension",
            "Remember the last image extension",
            "Pre-fill manual save names with the file extension used in your last save",
        );
        self.spin(
            &manual,
            "image-quality",
            "JPEG quality",
            0.,
            100.,
            1.,
            "Applies to JPG and JPEG (0–100, 100 is highest quality), PNG stays lossless",
        );
        let quick = self.group(
            &images,
            "Quick save",
            "Ctrl+Shift+S saves without a file chooser, creates missing folders, numbers duplicates and defaults empty paths to Pictures/Clivuu",
        );
        self.folder(
            &quick,
            "image-quick-directory",
            "Save folder",
            Media::Image,
            Mode::Quick,
            None,
        );
        self.pattern(
            &quick,
            "image-quick-name",
            "Relative path and file name",
            Media::Image,
            Mode::Quick,
            &example,
            None,
        );
        self.toggle(
            &quick,
            "image-save-notify",
            "Notify after quick or automatic save",
            "Show a desktop notification with a preview thumbnail when quick save or automatic save writes an image",
        );
        let auto = self.group(
            &images,
            "Automatic save",
            "Keep an extra copy when completing a capture (excluding cancelled captures), empty folder falls back to Quick save",
        );
        self.toggle(
            &auto,
            "image-auto-save",
            "Automatically save captures",
            "Write a background copy whenever a capture is confirmed or exported",
        );
        self.folder(
            &auto,
            "image-auto-directory",
            "Save folder",
            Media::Image,
            Mode::Automatic,
            Some("image-auto-save"),
        );
        self.pattern(
            &auto,
            "image-auto-name",
            "Relative path and file name",
            Media::Image,
            Mode::Automatic,
            &example,
            Some("image-auto-save"),
        );
        let manual = self.group(
            &recording,
            "Manual save",
            "Choose the format (WebM, MP4 or GIF) and location when exporting from playback",
        );
        self.pattern(
            &manual,
            "video-manual-name",
            "File name",
            Media::Video,
            Mode::Manual,
            &example,
            None,
        );
        let quick = self.group(
            &recording,
            "Quick save",
            "Save directly with the active export format, creates missing folders, numbers duplicates and defaults empty paths to Videos/Clivuu",
        );
        self.folder(
            &quick,
            "video-directory",
            "Save folder",
            Media::Video,
            Mode::Quick,
            None,
        );
        self.pattern(
            &quick,
            "video-quick-name",
            "Relative path and file name",
            Media::Video,
            Mode::Quick,
            &example,
            None,
        );
        self.toggle(
            &quick,
            "video-save-notify",
            "Notify after saving a recording",
            "Show a desktop notification after saving a recording from playback or quick save",
        );
        let storage = self.group(
            &recording,
            "Recording storage",
            "Memory first buffers recordings in /tmp up to the limit below, direct mode writes to the video folder without that limit",
        );
        self.combo(
            &storage,
            "recording-storage",
            "Recording strategy",
            &["Memory first", "Video folder"],
            &["temporary", "direct"],
            "Changes take effect on the next recording",
        );
        self.spin(
            &storage,
            "recording-temp-limit-mib",
            "Memory recording limit (MiB)",
            16.,
            16384.,
            16.,
            "Applies to memory-first mode only, stops near this limit or low disk space while keeping material ready for playback and export",
        );
        self.reset_section(
            &images,
            "save-images",
            &["save.image"],
            "Only settings in Save → Images will be reset",
        );
        self.reset_section(
            &recording,
            "save-recording",
            &["save.video", "recording"],
            "Only settings in Save → Recording will be reset",
        );
    }

    fn enabled(&self, widget: &impl IsA<gtk::Widget>, key: Option<&str>) {
        if let Some(key) = key {
            let owned = key.to_owned();
            let weak = widget.as_ref().downgrade();
            self.bind(&[key], false, move |v| {
                if let Some(w) = weak.upgrade() {
                    w.set_sensitive(v[&owned] == true);
                }
            });
        }
    }
    fn folder(
        self: &Rc<Self>,
        group: &adw::PreferencesGroup,
        key: &str,
        title: &str,
        media: Media,
        mode: Mode,
        enabled: Option<&str>,
    ) {
        let row = self.row(group, title, "");
        row.set_widget_name(key);
        row.set_subtitle_lines(2);
        self.enabled(&row, enabled);
        let button = gtk::Button::builder()
            .icon_name("folder-open-symbolic")
            .valign(gtk::Align::Center)
            .build();
        self.text(&button, "tooltip-text", "Choose folder");
        row.add_suffix(&button);
        row.set_activatable_widget(Some(&button));
        let r = row.clone();
        let deps: &[&str] = match (media, mode) {
            (Media::Image, Mode::Automatic) => &["image-auto-directory", "image-quick-directory"],
            (Media::Image, _) => &["image-quick-directory"],
            (Media::Video, _) => &["video-directory"],
        };
        self.bind(deps, false, move |values| {
            r.set_subtitle(&saving::directory(values, media, mode).to_string_lossy())
        });
        let key = key.to_owned();
        let weak = Rc::downgrade(self);
        button.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                let key = key.clone();
                let dialog = gtk::FileDialog::builder().title(t("Choose folder")).build();
                let folder = saving::directory(&this.config.values(), media, mode);
                dialog.set_initial_folder(Some(&gio::File::for_path(folder)));
                glib::MainContext::default().spawn_local(async move {
                    match dialog.select_folder_future(Some(&this.window)).await {
                        Ok(file) => {
                            if let Some(path) = file.path() {
                                this.change(&key, json!(path));
                            }
                        }
                        Err(e)
                            if e.matches(gtk::DialogError::Dismissed)
                                || e.matches(gtk::DialogError::Cancelled) => {}
                        Err(e) => this.toasts.add_toast(adw::Toast::new(&e.to_string())),
                    }
                });
            }
        });
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Each preview states its media, save mode and enable condition explicitly"
    )]
    fn pattern(
        self: &Rc<Self>,
        group: &adw::PreferencesGroup,
        key: &str,
        title: &str,
        media: Media,
        mode: Mode,
        context: &NameContext,
        enabled: Option<&str>,
    ) {
        let hint = match mode {
            Mode::Manual => {
                "Default file name pre-filled in the Save dialog, date tokens go between $…$, subfolders (/) are not allowed"
            }
            Mode::Quick | Mode::Automatic => {
                "Relative path under the save folder, date tokens go between $…$ and / creates subfolders"
            }
        };
        let entry = self.naming_entry(group, key, title, hint);
        self.enabled(&entry, enabled);
        let preview = self.row(group, "Preview", "");
        preview.set_widget_name(&format!("preview-{key}"));
        preview.set_subtitle_lines(2);
        self.enabled(&preview, enabled);
        let p = preview.clone();
        let time = context.clone();
        let owned_key = key.to_owned();
        let update = Rc::new(move |text: &str, values: &Values| {
            let mut values = values.clone();
            values.insert(owned_key.clone(), json!(text));
            let name = match mode {
                Mode::Manual => saving::name(&values, media, mode, &time),
                _ => saving::target(&values, media, mode, &time)
                    .map(|p| p.to_string_lossy().into_owned()),
            };
            match name {
                Ok(name) => {
                    p.set_subtitle(&name);
                    p.remove_css_class("error");
                }
                Err(error) => {
                    p.set_subtitle(&error);
                    p.add_css_class("error");
                }
            }
        });
        let r = entry.clone();
        let callback = update.clone();
        let deps: Vec<&str> = match (media, mode) {
            (Media::Image, Mode::Manual) => {
                vec![key, "image-remember-extension", "image-last-extension"]
            }
            (Media::Image, Mode::Quick) => vec![key, "image-quick-directory"],
            (Media::Image, Mode::Automatic) => {
                vec![key, "image-auto-directory", "image-quick-directory"]
            }
            (Media::Video, Mode::Manual) => vec![key],
            (Media::Video, _) => vec![key, "video-directory"],
        };
        self.bind(&deps, true, move |v| callback(r.text().as_str(), v));
        let weak = Rc::downgrade(self);
        entry.connect_changed(move |r| {
            if let Some(this) = weak.upgrade() {
                update(r.text().as_str(), &this.config.values());
            }
        });
    }
}

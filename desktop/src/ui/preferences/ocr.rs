//! OCR preferences page: recognition behavior, system-aware Tesseract language pack selector,
//! local ONNX Runtime status, and explicit PP-OCRv5 language pack management.
use super::Preferences;
use crate::infrastructure::ocr::{DownloadState, OcrDownloadManager};
use adw::prelude::*;
use clivuu_core::{
    i18n::{t, tf},
    ocr::{
        assets::{
            DETECTOR, MODELS, RuntimeSource, current_runtime_distribution,
            find_onnxruntime_library, is_model_installed, models_dir, runtime_dir,
        },
        tesseract,
    },
};
use gtk::{gdk, gio, glib};
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

const CURATED_TESSERACT_CODES: &[&str] = &["eng", "chi_sim", "chi_tra", "jpn"];

fn glyph_tile(glyph: &str) -> gtk::Box {
    let tile = gtk::Box::builder()
        .halign(gtk::Align::Start)
        .valign(gtk::Align::Center)
        .css_classes(["clivuu-ocr-script-tile"])
        .build();
    tile.set_size_request(32, 32);
    let label = gtk::Label::builder()
        .label(glyph)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .build();
    label.set_size_request(32, 32);
    tile.append(&label);
    tile
}

fn icon_tile(icon_name: &str) -> gtk::Box {
    let tile = gtk::Box::builder()
        .halign(gtk::Align::Start)
        .valign(gtk::Align::Center)
        .css_classes(["clivuu-ocr-script-tile"])
        .build();
    tile.set_size_request(32, 32);
    let icon = gtk::Image::builder()
        .icon_name(icon_name)
        .pixel_size(16)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .build();
    icon.set_size_request(32, 32);
    tile.append(&icon);
    tile
}

impl Preferences {
    pub(super) fn build_ocr(self: &Rc<Self>) {
        let ocr = self.page("ocr", "OCR", "scanner-symbolic");

        let general_group = self.group(
            &ocr,
            "Recognition and copying",
            "Runs offline on your device without keeping models in background memory",
        );
        self.combo(
            &general_group,
            "ocr-engine",
            "Recognition engine",
            &["Automatic", "PP-OCRv5 (Local)", "System Tesseract"],
            &["auto", "paddle", "tesseract"],
            "Uses PP-OCRv5 when installed, otherwise Tesseract",
        );
        self.toggle(
            &general_group,
            "ocr-smart-paragraphs",
            "Keep paragraph breaks",
            "Insert blank lines between separate text blocks when copying",
        );
        self.toggle(
            &general_group,
            "ocr-quick-copy",
            "Copy immediately without selection",
            "Pressing O during capture copies all text and closes immediately, Shift+O always quick-copies",
        );
        self.build_tesseract_selector(&general_group);

        let make_folder_btn =
            |label_key: &'static str, resolve: fn() -> clivuu_core::Result<std::path::PathBuf>| {
                let btn = gtk::Button::builder()
                    .valign(gtk::Align::Center)
                    .css_classes(["flat", "clivuu-ocr-folder-btn"])
                    .build();
                let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
                let icon = gtk::Image::from_icon_name("folder-open-symbolic");
                icon.set_pixel_size(14);
                content.append(&icon);
                let lbl = gtk::Label::new(None);
                self.text(&lbl, "label", label_key);
                content.append(&lbl);
                btn.set_child(Some(&content));
                let weak = Rc::downgrade(self);
                btn.connect_clicked(move |_| {
                    if let Ok(dir) = resolve() {
                        let _ = std::fs::create_dir_all(&dir);
                        let file = gio::File::for_path(&dir);
                        let launcher = gtk::FileLauncher::new(Some(&file));
                        let parent = weak.upgrade().map(|p| p.window.clone());
                        launcher.launch(parent.as_ref(), gio::Cancellable::NONE, |_| {});
                    }
                });
                btn
            };

        let runtime_group = self.group(
            &ocr,
            "Inference runtime",
            "Shared engine required by local PP-OCRv5 models",
        );
        runtime_group.set_header_suffix(Some(&make_folder_btn("Open runtime folder", runtime_dir)));

        // ONNX Runtime status row
        let rt_row = adw::ActionRow::new();
        rt_row.set_use_markup(false);
        rt_row.set_widget_name("ocr-onnx-runtime");
        let rt_tile = icon_tile("system-run-symbolic");
        rt_row.add_prefix(&rt_tile);
        self.text(&rt_row, "title", "ONNX Runtime");

        let rt_suffix = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        rt_suffix.set_valign(gtk::Align::Center);
        let rt_progress = gtk::ProgressBar::builder()
            .valign(gtk::Align::Center)
            .css_classes(["clivuu-ocr-progress"])
            .visible(false)
            .build();
        let rt_size_lbl = gtk::Label::builder()
            .valign(gtk::Align::Center)
            .css_classes(["clivuu-ocr-size"])
            .visible(false)
            .build();
        let rt_status_pill = gtk::Label::builder()
            .valign(gtk::Align::Center)
            .css_classes(["clivuu-ocr-status-pill"])
            .visible(false)
            .build();
        let rt_btn = gtk::Button::builder()
            .valign(gtk::Align::Center)
            .css_classes(["clivuu-ocr-action-btn"])
            .visible(false)
            .build();
        rt_suffix.append(&rt_progress);
        rt_suffix.append(&rt_size_lbl);
        rt_suffix.append(&rt_status_pill);
        rt_suffix.append(&rt_btn);
        rt_row.add_suffix(&rt_suffix);
        runtime_group.add(&rt_row);

        let models_group = self.group(
            &ocr,
            "Local models (PP-OCRv5)",
            "Download language packs on demand, or import files manually",
        );
        models_group.set_header_suffix(Some(&make_folder_btn("Open models folder", models_dir)));

        {
            let weak = Rc::downgrade(self);
            rt_btn.connect_clicked(move |_| {
                let manager = OcrDownloadManager::shared();
                if matches!(manager.state("runtime"), DownloadState::Downloading(_)) {
                    manager.cancel("runtime");
                } else {
                    let mirror = weak
                        .upgrade()
                        .and_then(|p| {
                            p.config.values()["ocr-download-mirror"]
                                .as_str()
                                .map(str::to_owned)
                        })
                        .unwrap_or_default();
                    manager.start_runtime_download(&mirror);
                }
            });
        }

        let more_expander = adw::ExpanderRow::new();
        more_expander.set_use_markup(false);
        more_expander.set_widget_name("ocr-more-languages");
        let more_tile = icon_tile("accessories-dictionary-symbolic");
        more_expander.add_prefix(&more_tile);
        self.text(&more_expander, "title", "More language models");
        self.text(
            &more_expander,
            "subtitle",
            "Korean, Cyrillic, Arabic, Greek, and Thai",
        );

        type ModelRefresher = Rc<dyn Fn(&str)>;
        let mut model_refreshers: Vec<ModelRefresher> = Vec::new();

        {
            let rt_row_ref = rt_row.clone();
            let rt_tile_ref = rt_tile.clone();
            let rt_prog_ref = rt_progress.clone();
            let rt_size_ref = rt_size_lbl.clone();
            let rt_pill_ref = rt_status_pill.clone();
            let rt_btn_ref = rt_btn.clone();
            model_refreshers.push(Rc::new(move |_| {
                let manager = OcrDownloadManager::shared();
                rt_row_ref.remove_css_class("error");
                match (find_onnxruntime_library(), manager.state("runtime")) {
                    (_, DownloadState::Downloading(pct)) => {
                        rt_tile_ref.remove_css_class("is-installed");
                        rt_row_ref
                            .set_subtitle(&tf("Downloading runtime ({0}%)…", &[&pct.to_string()]));
                        rt_prog_ref.set_fraction(f64::from(pct) / 100.0);
                        rt_prog_ref.set_visible(true);
                        rt_size_ref.set_text(&format!("{pct}%"));
                        rt_size_ref.set_visible(true);
                        rt_pill_ref.set_visible(false);
                        rt_btn_ref.set_visible(true);
                        rt_btn_ref.set_sensitive(true);
                        rt_btn_ref.remove_css_class("suggested-action");
                        rt_btn_ref.set_label(&t("Cancel"));
                    }
                    (Some(RuntimeSource::System(path)), _)
                    | (Some(RuntimeSource::EnvOverride(path)), _) => {
                        rt_tile_ref.add_css_class("is-installed");
                        rt_row_ref.set_subtitle(&tf(
                            "Using system library ({0})",
                            &[&path.display().to_string()],
                        ));
                        rt_prog_ref.set_visible(false);
                        rt_size_ref.set_visible(false);
                        rt_pill_ref.set_text(&t("Ready"));
                        rt_pill_ref.add_css_class("is-ready");
                        rt_pill_ref.set_visible(true);
                        rt_btn_ref.set_visible(false);
                    }
                    (Some(RuntimeSource::Downloaded(_)), _) | (None, DownloadState::Installed) => {
                        rt_tile_ref.add_css_class("is-installed");
                        rt_row_ref.set_subtitle(&t("Local inference library v1.28.0"));
                        rt_prog_ref.set_visible(false);
                        rt_size_ref.set_visible(false);
                        rt_pill_ref.set_text(&t("Ready"));
                        rt_pill_ref.add_css_class("is-ready");
                        rt_pill_ref.set_visible(true);
                        rt_btn_ref.set_visible(false);
                    }
                    (None, DownloadState::Failed(err)) => {
                        rt_tile_ref.remove_css_class("is-installed");
                        rt_row_ref.add_css_class("error");
                        rt_row_ref.set_subtitle(&err);
                        rt_prog_ref.set_visible(false);
                        rt_size_ref.set_visible(false);
                        rt_pill_ref.set_visible(false);
                        rt_btn_ref.set_visible(true);
                        rt_btn_ref.set_sensitive(true);
                        rt_btn_ref.add_css_class("suggested-action");
                        rt_btn_ref.set_label(&t("Retry"));
                    }
                    (None, DownloadState::Idle) => {
                        rt_tile_ref.remove_css_class("is-installed");
                        rt_prog_ref.set_visible(false);
                        rt_pill_ref.set_visible(false);
                        if let Some(dist) = current_runtime_distribution() {
                            let mib = dist.archive.size as f64 / (1024.0 * 1024.0);
                            let mib_text = format!("{mib:.1}");
                            rt_row_ref.set_subtitle(&t(
                                "Installed automatically with your first language model",
                            ));
                            rt_size_ref.set_text(&format!("{0} MB", mib_text));
                            rt_size_ref.set_visible(true);
                            rt_btn_ref.set_visible(true);
                            rt_btn_ref.set_sensitive(true);
                            rt_btn_ref.remove_css_class("suggested-action");
                            rt_btn_ref.set_label(&t("Install"));
                        } else {
                            rt_row_ref.set_subtitle(&t(
                                "ONNX Runtime is not supported on this CPU architecture",
                            ));
                            rt_size_ref.set_visible(false);
                            rt_btn_ref.set_visible(false);
                        }
                    }
                }
            }));
        }

        for spec in MODELS {
            let is_primary = matches!(spec.id, "chinese" | "latin");
            let row = adw::ActionRow::new();
            row.set_use_markup(false);
            row.set_widget_name(&format!("ocr-pack-{}", spec.id));

            let tile = glyph_tile(spec.glyph);
            row.add_prefix(&tile);

            let suffix = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            suffix.set_valign(gtk::Align::Center);

            let progress_bar = gtk::ProgressBar::builder()
                .valign(gtk::Align::Center)
                .css_classes(["clivuu-ocr-progress"])
                .visible(false)
                .build();
            let size_lbl = gtk::Label::builder()
                .valign(gtk::Align::Center)
                .css_classes(["clivuu-ocr-size"])
                .build();
            let active_pill = gtk::Label::builder()
                .valign(gtk::Align::Center)
                .css_classes(["clivuu-ocr-active-pill"])
                .visible(false)
                .build();
            let primary_btn = gtk::Button::builder()
                .valign(gtk::Align::Center)
                .css_classes(["clivuu-ocr-action-btn"])
                .build();
            let delete_btn = gtk::Button::builder()
                .icon_name("user-trash-symbolic")
                .valign(gtk::Align::Center)
                .css_classes(["flat", "clivuu-ocr-remove-btn"])
                .visible(false)
                .build();
            self.text(&delete_btn, "tooltip-text", "Remove model");

            suffix.append(&progress_bar);
            suffix.append(&size_lbl);
            suffix.append(&active_pill);
            suffix.append(&primary_btn);
            suffix.append(&delete_btn);
            row.add_suffix(&suffix);

            if is_primary {
                models_group.add(&row);
            } else {
                more_expander.add_row(&row);
            }

            {
                let weak = Rc::downgrade(self);
                let id = spec.id;
                primary_btn.connect_clicked(move |_| {
                    let manager = OcrDownloadManager::shared();
                    match manager.state(id) {
                        DownloadState::Downloading(_) => {
                            manager.cancel(id);
                        }
                        DownloadState::Installed => {
                            if let Some(prefs) = weak.upgrade() {
                                prefs.change("ocr-model", json!(id));
                            }
                        }
                        DownloadState::Idle | DownloadState::Failed(_) => {
                            let mirror = weak
                                .upgrade()
                                .map(|prefs| {
                                    prefs.change("ocr-model", json!(id));
                                    prefs.config.values()["ocr-download-mirror"]
                                        .as_str()
                                        .unwrap_or("")
                                        .to_owned()
                                })
                                .unwrap_or_default();
                            manager.start_model_download(id, true, &mirror);
                        }
                    }
                });
            }
            {
                let weak = Rc::downgrade(self);
                let id = spec.id;
                delete_btn.connect_clicked(move |_| {
                    if let Err(err) = OcrDownloadManager::shared().remove_model(id)
                        && let Some(prefs) = weak.upgrade()
                    {
                        prefs.toasts.add_toast(adw::Toast::new(&err));
                    }
                });
            }

            let row_ref = row.clone();
            let tile_ref = tile.clone();
            let prog_ref = progress_bar.clone();
            let size_ref = size_lbl.clone();
            let pill_ref = active_pill.clone();
            let prim_ref = primary_btn.clone();
            let del_ref = delete_btn.clone();
            model_refreshers.push(Rc::new(move |active_model: &str| {
                let manager = OcrDownloadManager::shared();
                let any_installed = MODELS.iter().any(|m| is_model_installed(m.id));
                row_ref.set_title(&spec.localized_name());
                let pack_mib = (spec.recognizer.size + spec.dict.size + DETECTOR.size) as f64
                    / (1024.0 * 1024.0);
                let size_text = format!("{0:.1} MB", pack_mib);

                row_ref.remove_css_class("error");
                row_ref.remove_css_class("clivuu-ocr-pack-active");
                tile_ref.remove_css_class("is-installed");
                tile_ref.remove_css_class("is-active");

                match manager.state(spec.id) {
                    DownloadState::Downloading(pct) => {
                        row_ref.set_subtitle(&spec.localized_note());
                        prog_ref.set_fraction(f64::from(pct) / 100.0);
                        prog_ref.set_visible(true);
                        size_ref.set_text(&format!("{pct}%"));
                        size_ref.set_visible(true);
                        pill_ref.set_visible(false);
                        prim_ref.set_label(&t("Cancel"));
                        prim_ref.set_visible(true);
                        prim_ref.set_sensitive(true);
                        prim_ref.remove_css_class("suggested-action");
                        del_ref.set_visible(false);
                    }
                    DownloadState::Installed => {
                        row_ref.set_subtitle(&spec.localized_note());
                        prog_ref.set_visible(false);
                        size_ref.set_text(&size_text);
                        size_ref.set_visible(true);
                        let is_selected = active_model == spec.id;
                        if is_selected {
                            row_ref.add_css_class("clivuu-ocr-pack-active");
                            tile_ref.add_css_class("is-active");
                            pill_ref.set_text(&t("In use"));
                            pill_ref.set_visible(true);
                            prim_ref.set_visible(false);
                        } else {
                            tile_ref.add_css_class("is-installed");
                            pill_ref.set_visible(false);
                            prim_ref.set_label(&t("Use"));
                            prim_ref.set_visible(true);
                            prim_ref.set_sensitive(true);
                            prim_ref.remove_css_class("suggested-action");
                        }
                        del_ref.set_visible(true);
                    }
                    DownloadState::Failed(err) => {
                        row_ref.add_css_class("error");
                        row_ref.set_subtitle(&err);
                        prog_ref.set_visible(false);
                        size_ref.set_text(&size_text);
                        size_ref.set_visible(true);
                        pill_ref.set_visible(false);
                        prim_ref.set_label(&t("Retry"));
                        prim_ref.set_visible(true);
                        prim_ref.set_sensitive(true);
                        prim_ref.add_css_class("suggested-action");
                        del_ref.set_visible(false);
                    }
                    DownloadState::Idle => {
                        row_ref.set_subtitle(&spec.localized_note());
                        prog_ref.set_visible(false);
                        size_ref.set_text(&size_text);
                        size_ref.set_visible(true);
                        pill_ref.set_visible(false);
                        prim_ref.set_label(&t("Download"));
                        prim_ref.set_visible(true);
                        prim_ref.set_sensitive(true);
                        if !any_installed && active_model == spec.id {
                            prim_ref.add_css_class("suggested-action");
                        } else {
                            prim_ref.remove_css_class("suggested-action");
                        }
                        del_ref.set_visible(false);
                    }
                }
            }));
        }

        models_group.add(&more_expander);

        {
            let exp_ref = more_expander.clone();
            let tile_ref = more_tile.clone();
            model_refreshers.push(Rc::new(move |active_model: &str| {
                let manager = OcrDownloadManager::shared();
                let mut any_regional_installed = false;
                let mut any_regional_active = false;
                let mut should_open = false;
                for spec in MODELS
                    .iter()
                    .filter(|s| !matches!(s.id, "chinese" | "latin"))
                {
                    let state = manager.state(spec.id);
                    if matches!(state, DownloadState::Installed) {
                        any_regional_installed = true;
                        if active_model == spec.id {
                            any_regional_active = true;
                        }
                    }
                    if active_model == spec.id || !matches!(state, DownloadState::Idle) {
                        should_open = true;
                    }
                }
                tile_ref.remove_css_class("is-installed");
                tile_ref.remove_css_class("is-active");
                if any_regional_active {
                    tile_ref.add_css_class("is-active");
                } else if any_regional_installed {
                    tile_ref.add_css_class("is-installed");
                }
                if should_open && !exp_ref.is_expanded() {
                    exp_ref.set_expanded(true);
                }
            }));
        }

        let mirror_row = self.entry(
            &models_group,
            "ocr-download-mirror",
            "GitHub mirror prefix",
            "Optional mirror prefix for restricted networks, such as https://ghfast.top, leave empty for direct GitHub downloads",
        );
        mirror_row.add_prefix(&icon_tile("insert-link-symbolic"));

        let refresh_all = {
            let refreshers = Rc::new(model_refreshers);
            let run = refreshers.clone();
            self.bind(&["ocr-model", "ocr-engine"], true, move |values| {
                OcrDownloadManager::shared().refresh_states();
                let active = values["ocr-model"].as_str().unwrap_or("chinese");
                for f in run.iter() {
                    f(active);
                }
            });
            refreshers
        };
        {
            let weak = Rc::downgrade(self);
            OcrDownloadManager::shared().connect_changed(move || {
                let Some(prefs) = weak.upgrade() else {
                    return;
                };
                let vals = prefs.config.values();
                let active = vals["ocr-model"].as_str().unwrap_or("chinese");
                for f in refresh_all.iter() {
                    f(active);
                }
            });
        }

        self.reset_section(&ocr, "ocr", &["ocr"], "Only settings in OCR will be reset");

        // Allow `clivuu-desktop --preferences` initial page override via env if set.
        if let Ok(page) = std::env::var("CLIVUU_PREFERENCES_PAGE")
            && !page.is_empty()
        {
            let weak = Rc::downgrade(self);
            glib::idle_add_local_once(move || {
                if let Some(prefs) = weak.upgrade() {
                    prefs.select_page(&page);
                }
            });
        }
    }

    fn build_tesseract_selector(self: &Rc<Self>, group: &adw::PreferencesGroup) {
        let expander = adw::ExpanderRow::new();
        expander.set_use_markup(false);
        expander.set_widget_name("ocr-languages");
        self.text(&expander, "title", "Tesseract language packs");

        let summary_pill = gtk::Label::builder()
            .valign(gtk::Align::Center)
            .css_classes(["clivuu-ocr-status-pill"])
            .build();
        expander.add_suffix(&summary_pill);
        group.add(&expander);

        let installed_cache: Rc<RefCell<Vec<String>>> =
            Rc::new(RefCell::new(tesseract::installed_languages()));
        let syncing = Rc::new(Cell::new(false));

        // Build deterministic list of selectable rows: all currently installed codes + curated codes
        // + any custom codes present in initial `ocr-languages`.
        let mut codes: Vec<String> = Vec::new();
        for c in installed_cache.borrow().iter() {
            if !codes.contains(c) {
                codes.push(c.clone());
            }
        }
        for &c in CURATED_TESSERACT_CODES {
            if !codes.iter().any(|existing| existing == c) {
                codes.push(c.to_owned());
            }
        }
        let initial_spec = self.config.values()["ocr-languages"]
            .as_str()
            .unwrap_or("eng+chi_sim")
            .to_owned();
        for c in tesseract::parse_language_codes(&initial_spec) {
            if !codes.contains(&c) {
                codes.push(c);
            }
        }

        struct LangRowWidgets {
            code: String,
            row: adw::ActionRow,
            check: gtk::CheckButton,
            state_lbl: gtk::Label,
        }
        let mut row_widgets: Vec<LangRowWidgets> = Vec::with_capacity(codes.len());

        for code in codes {
            let row = adw::ActionRow::new();
            row.set_use_markup(false);
            let check = gtk::CheckButton::builder()
                .valign(gtk::Align::Center)
                .build();
            check.set_widget_name(&format!("ocr-tess-{code}"));
            row.add_prefix(&check);
            row.set_activatable_widget(Some(&check));

            let suffix = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            suffix.set_valign(gtk::Align::Center);
            let code_pill = gtk::Label::builder()
                .label(&code)
                .valign(gtk::Align::Center)
                .css_classes(["clivuu-ocr-code-pill"])
                .build();
            let state_lbl = gtk::Label::builder().valign(gtk::Align::Center).build();
            suffix.append(&code_pill);
            suffix.append(&state_lbl);
            row.add_suffix(&suffix);
            expander.add_row(&row);

            row_widgets.push(LangRowWidgets {
                code,
                row,
                check,
                state_lbl,
            });
        }

        // Package-manager install guidance & live rescan footer row
        let hint_row = adw::ActionRow::new();
        hint_row.set_use_markup(false);
        self.text(&hint_row, "title", "Install more system language packs");
        let hint_suffix = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        hint_suffix.set_valign(gtk::Align::Center);
        let copy_cmd_btn = gtk::Button::builder()
            .valign(gtk::Align::Center)
            .css_classes(["clivuu-ocr-action-btn"])
            .build();
        self.text(&copy_cmd_btn, "label", "Copy command");
        let refresh_btn = gtk::Button::builder()
            .icon_name("view-refresh-symbolic")
            .valign(gtk::Align::Center)
            .css_classes(["flat", "clivuu-ocr-remove-btn"])
            .build();
        self.text(&refresh_btn, "tooltip-text", "Rescan installed packs");
        hint_suffix.append(&copy_cmd_btn);
        hint_suffix.append(&refresh_btn);
        hint_row.add_suffix(&hint_suffix);
        expander.add_row(&hint_row);

        let current_hint_cmd = Rc::new(RefCell::new(tesseract::distro_install_hint("chi_sim")));
        {
            let weak = Rc::downgrade(self);
            let cmd_ref = current_hint_cmd.clone();
            copy_cmd_btn.connect_clicked(move |_| {
                if let Some(display) = gdk::Display::default() {
                    display.clipboard().set_text(&cmd_ref.borrow());
                    if let Some(prefs) = weak.upgrade() {
                        prefs.toasts.add_toast(adw::Toast::new(&t("Copied")));
                    }
                }
            });
        }

        let row_widgets = Rc::new(row_widgets);
        let sync_ui: Rc<dyn Fn(&str)> = {
            let exp_ref = expander.clone();
            let pill_ref = summary_pill.clone();
            let rows_ref = row_widgets.clone();
            let inst_ref = installed_cache.clone();
            let hint_row_ref = hint_row.clone();
            let hint_cmd_ref = current_hint_cmd.clone();
            let syncing_ref = syncing.clone();
            Rc::new(move |spec_str: &str| {
                syncing_ref.set(true);
                let installed = inst_ref.borrow();
                let selected = tesseract::parse_language_codes(spec_str);
                let tess_ready = tesseract::is_available();

                let mut active_installed_names = Vec::new();
                let mut missing_selected_codes = Vec::new();
                let mut missing_selected_names = Vec::new();

                for w in rows_ref.iter() {
                    let is_inst = installed.iter().any(|c| c == &w.code);
                    let is_sel = selected.iter().any(|c| c == &w.code);
                    w.row.set_title(&tesseract::language_display_name(&w.code));
                    w.check.set_active(is_sel);

                    w.state_lbl.remove_css_class("clivuu-ocr-size");
                    w.state_lbl.remove_css_class("clivuu-ocr-missing-tag");
                    if is_inst {
                        w.state_lbl.add_css_class("clivuu-ocr-size");
                        w.state_lbl.set_text(&t("Installed"));
                        w.check.set_sensitive(true);
                        w.row.set_sensitive(true);
                        if is_sel {
                            active_installed_names.push(tesseract::language_display_name(&w.code));
                        }
                    } else {
                        w.state_lbl.add_css_class("clivuu-ocr-missing-tag");
                        w.state_lbl.set_text(&t("Not installed"));
                        // Allow unchecking a missing code that was in config, but prevent checking uninstalled packs.
                        w.check.set_sensitive(is_sel);
                        w.row.set_sensitive(is_sel);
                        if is_sel {
                            missing_selected_codes.push(w.code.clone());
                            missing_selected_names.push(tesseract::language_display_name(&w.code));
                        }
                    }
                }

                let sep = if clivuu_core::i18n::current_language().id().starts_with("zh") {
                    "、"
                } else {
                    ", "
                };
                pill_ref.remove_css_class("is-ready");
                pill_ref.remove_css_class("is-warning");
                if !tess_ready {
                    exp_ref.set_subtitle(&t(
                        "System Tesseract 5 not detected, install it or use a local model below",
                    ));
                    pill_ref.set_text(&t("Not installed"));
                } else if !missing_selected_names.is_empty() {
                    pill_ref.add_css_class("is-warning");
                    pill_ref.set_text(&selected.join(" + "));
                    let ok_part = if active_installed_names.is_empty() {
                        t("None")
                    } else {
                        active_installed_names.join(sep)
                    };
                    exp_ref.set_subtitle(&tf(
                        "Selected: {0}, missing system pack: {1}",
                        &[&ok_part, &missing_selected_names.join(sep)],
                    ));
                } else {
                    pill_ref.add_css_class("is-ready");
                    pill_ref.set_text(&selected.join(" + "));
                    exp_ref.set_subtitle(&tf("Active: {0}", &[&active_installed_names.join(sep)]));
                }

                let target_missing = missing_selected_codes
                    .first()
                    .map(String::as_str)
                    .or_else(|| {
                        CURATED_TESSERACT_CODES
                            .iter()
                            .copied()
                            .find(|c| !installed.iter().any(|inst| inst == c))
                    })
                    .unwrap_or("chi_sim");
                let cmd = tesseract::distro_install_hint(target_missing);
                hint_row_ref.set_subtitle(&cmd);
                hint_cmd_ref.replace(cmd);

                syncing_ref.set(false);
            })
        };

        for w in row_widgets.iter() {
            let weak = Rc::downgrade(self);
            let rows_ref = row_widgets.clone();
            let syncing_ref = syncing.clone();
            let sync_after_reject = sync_ui.clone();
            w.check.connect_toggled(move |btn| {
                if syncing_ref.get() {
                    return;
                }
                let Some(prefs) = weak.upgrade() else {
                    return;
                };
                let next_codes: Vec<&str> = rows_ref
                    .iter()
                    .filter(|item| item.check.is_active())
                    .map(|item| item.code.as_str())
                    .collect();
                if next_codes.is_empty() {
                    syncing_ref.set(true);
                    btn.set_active(true);
                    syncing_ref.set(false);
                    prefs
                        .toasts
                        .add_toast(adw::Toast::new(&t("Keep at least one language selected")));
                    let cur = prefs.config.values()["ocr-languages"]
                        .as_str()
                        .unwrap_or("eng")
                        .to_owned();
                    sync_after_reject(&cur);
                    return;
                }
                prefs.change("ocr-languages", json!(next_codes.join("+")));
            });
        }

        {
            let weak = Rc::downgrade(self);
            let inst_ref = installed_cache.clone();
            let sync_fn = sync_ui.clone();
            refresh_btn.connect_clicked(move |_| {
                inst_ref.replace(tesseract::installed_languages());
                if let Some(prefs) = weak.upgrade() {
                    let cur = prefs.config.values()["ocr-languages"]
                        .as_str()
                        .unwrap_or("eng+chi_sim")
                        .to_owned();
                    sync_fn(&cur);
                }
            });
        }

        {
            let sync_fn = sync_ui.clone();
            self.bind(&["ocr-languages"], true, move |values| {
                let spec = values["ocr-languages"].as_str().unwrap_or("eng+chi_sim");
                sync_fn(spec);
            });
        }
    }
}

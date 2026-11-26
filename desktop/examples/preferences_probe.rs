//! Real GTK layout, live choices, and page-scoped reset in a private XDG home.
#[path = "support/color_panel.rs"]
mod color_panel;
use adw::prelude::*;
use clivuu_desktop::{
    infrastructure::{configuration::Configuration, tasks::TaskPool},
    ui::preferences::Preferences,
};
use gtk::{gio, glib};
use serde_json::json;
use std::{cell::RefCell, path::PathBuf, rc::Rc};

fn find(widget: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if widget.widget_name() == name {
        return Some(widget.clone());
    }
    let mut child = widget.first_child();
    while let Some(w) = child {
        child = w.next_sibling();
        if let Some(found) = find(&w, name) {
            return Some(found);
        }
    }
    None
}
fn named(widget: &impl IsA<gtk::Widget>, name: &str) -> gtk::Widget {
    find(widget.as_ref(), name).unwrap_or_else(|| panic!("missing widget {name}"))
}
async fn settle() {
    glib::timeout_future(std::time::Duration::from_millis(300)).await;
}
fn snapshot(window: &impl IsA<gtk::Widget>, path: PathBuf) {
    let window = window.as_ref();
    let paintable = gtk::WidgetPaintable::new(Some(window));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(
        &snapshot,
        f64::from(window.width()),
        f64::from(window.height()),
    );
    let node = snapshot.to_node().expect("preferences render node");
    window
        .native()
        .unwrap()
        .renderer()
        .unwrap()
        .render_texture(&node, None)
        .save_to_png(path)
        .unwrap();
}
fn main() {
    adw::init().unwrap();
    glib::MainContext::default().block_on(async {
        let directory = PathBuf::from(std::env::var_os("CLIVUU_TEST_DIR").unwrap());
        let backend = std::env::var("GDK_BACKEND").unwrap();
        let prefix = format!("preferences-{backend}");
        let pool = Rc::new(TaskPool::new(1, 2).unwrap());
        let errors = Rc::new(RefCell::new(Vec::new()));
        let e = errors.clone();
        let config = Configuration::new(
            clivuu_core::config::read().unwrap(),
            pool.clone(),
            move |s| e.borrow_mut().push(s),
        )
        .unwrap();
        config.set("language", json!("en")).unwrap();
        config.set("theme", json!("dark")).unwrap();
        let app = gtk::Application::builder()
            .application_id("io.github.naerelyth.Clivuu.PreferencesProbe")
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(gio::Cancellable::NONE).unwrap();
        let prefs = Preferences::new(&app, config.clone(), Default::default());
        prefs.present();
        settle().await;
        let pages = named(&prefs.window, "preferences-pages")
            .downcast::<gtk::Stack>()
            .unwrap();
        assert_eq!(pages.visible_child_name().as_deref(), Some("general"));
        assert!(find(prefs.window.upcast_ref(), "naming-rules").is_none());
        let startup = named(&prefs.window, "run-at-login").downcast::<adw::SwitchRow>().unwrap();
        let startup_file = glib::user_config_dir().join("autostart/io.github.naerelyth.clivuu.desktop");
        assert!(startup.is_sensitive() && !startup.is_active());
        assert!(!startup_file.exists());
        startup.set_active(true); settle().await;
        assert!(startup.is_sensitive() && startup.is_active());
        let entry = glib::KeyFile::new();
        entry.load_from_file(&startup_file, glib::KeyFileFlags::NONE).unwrap();
        assert!(entry.string("Desktop Entry", "Exec").unwrap().contains("clivuu"));
        assert!(!clivuu_core::config::read().unwrap().contains_key("autostart"));
        let reopened_startup = Preferences::new(&app, config.clone(), Default::default()); settle().await;
        assert!(named(&reopened_startup.window, "run-at-login").downcast::<adw::SwitchRow>().unwrap().is_active());
        reopened_startup.window.destroy(); drop(reopened_startup);
        startup.set_active(false); settle().await;
        assert!(startup.is_sensitive() && !startup.is_active() && !startup_file.exists());
        let language = named(&prefs.window, "language")
            .downcast::<adw::ComboRow>()
            .unwrap();
        let labels = || {
            let model = language
                .model()
                .unwrap()
                .downcast::<gtk::StringList>()
                .unwrap();
            (
                model.string(1).unwrap().to_string(),
                model.string(2).unwrap().to_string(),
            )
        };
        assert_eq!(labels(), ("简体中文".into(), "English".into()));
        language.set_selected(1);
        settle().await;
        assert_eq!(config.value("language"), json!("zh-CN"));
        assert_eq!(labels(), ("简体中文".into(), "English".into()));
        snapshot(
            &prefs.window,
            directory.join(format!("{prefix}-dark-general.png")),
        );
        let accent = named(&prefs.window, "accent-color")
            .downcast::<gtk::Button>()
            .unwrap();
        let reset = named(&prefs.window, "reset-accent")
            .downcast::<gtk::Button>()
            .unwrap();
        let color_window = || prefs.window.visible_dialog().unwrap();
        let outside = gtk::Label::new(Some("Settings 设置"));
        let outside_font = outside.pango_context().font_description().unwrap();
        let font = named(&prefs.window, "preferences-font").downcast::<gtk::FontDialogButton>().unwrap();
        assert_eq!(config.value("preferences-font"), json!(""));
        font.set_font_desc(&gtk::pango::FontDescription::from_string("Serif 14"));
        settle().await;
        assert_eq!(config.value("preferences-font"), json!("Serif 14"));
        assert_eq!(prefs.window.pango_context().font_description().unwrap().family().as_deref(), Some("Serif"));
        assert_eq!(outside.pango_context().font_description().unwrap(), outside_font);
        assert_eq!(prefs.window.create_pango_layout(Some("Settings 设置")).unknown_glyphs_count(), 0);
        let font_reset = named(&prefs.window, "reset-preferences-font").downcast::<gtk::Button>().unwrap();
        font_reset.emit_clicked(); settle().await;
        assert_eq!(config.value("preferences-font"), json!(""));
        assert!(!font_reset.is_sensitive());
        accent.emit_clicked();
        glib::timeout_future(std::time::Duration::from_millis(600)).await;
        let dialog = color_window();
        snapshot(&prefs.window, directory.join(format!("{prefix}-color-dialog-window.png")));
        snapshot(&dialog.child().unwrap(), directory.join(format!("{prefix}-color-dialog.png")));
        let color_layout = color_panel::inspect(dialog.upcast_ref()).unwrap();
        assert_eq!(color_layout["swatches"].as_array().unwrap().len(), 64);
        assert!(color_layout["swatches"].as_array().unwrap().iter().all(|r| r[0]==r[1]));
        assert_eq!(color_layout["alpha"]["inheritsFont"], true);
        assert_eq!(color_layout["html"]["inheritsFont"], true);
        assert_eq!(color_layout["channelsUniform"], true);
        assert_eq!(color_layout["gridAligned"], true);
        assert_eq!(color_layout["steppersHidden"], true);
        assert_eq!(color_layout["hasAlphaSlider"], true);
        assert_eq!(color_layout["hasPreview"], true);
        assert_eq!(color_layout["hasCopyButton"], true);
        std::fs::write(directory.join(format!("{prefix}-color-layout.json")),color_layout.to_string()).unwrap();
        let html = named(&dialog, "clivuu-color-html").downcast::<gtk::Entry>().unwrap();
        assert!(named(&dialog, "clivuu-color-alpha").is_visible());
        html.set_text("#ff555580");
        named(&dialog, "accept-color").downcast::<gtk::Button>().unwrap().emit_clicked();
        settle().await;
        assert_eq!(config.value("accent-color"), json!("#ff555580"));
        accent.emit_clicked(); settle().await;
        let dialog = color_window();
        named(&dialog, "clivuu-color-html").downcast::<gtk::Entry>().unwrap().set_text("#12345640");
        named(&dialog, "cancel-color").downcast::<gtk::Button>().unwrap().emit_clicked();
        settle().await;
        assert_eq!(config.value("accent-color"), json!("#ff555580"));
        assert!(reset.is_sensitive());
        let a = accent.compute_bounds(&prefs.window).unwrap();
        let r = reset.compute_bounds(&prefs.window).unwrap();
        assert!((a.center().y() - r.center().y()).abs() < 1.);
        reset.emit_clicked();
        assert_eq!(
            config.value("accent-color"),
            clivuu_core::config::schema()["accent-color"]["default"]
        );
        assert!(!reset.is_sensitive());
        config.set("theme", json!("light")).unwrap();
        settle().await;
        snapshot(
            &prefs.window,
            directory.join(format!("{prefix}-light-general.png")),
        );
        config.set("capture-mask-color", json!("#000000ba")).unwrap();
        config.set("capture-hide-pins", json!(false)).unwrap();
        config.set("scroll-interval", json!(1700)).unwrap();
        config
            .set("video-directory", json!("/tmp/preferences-video-output"))
            .unwrap();
        let capture = named(&prefs.window, "capture")
            .downcast::<gtk::ListBoxRow>()
            .unwrap();
        capture
            .parent()
            .unwrap()
            .downcast::<gtk::ListBox>()
            .unwrap()
            .select_row(Some(&capture));
        settle().await;
        let page = pages.visible_child().unwrap();
        // PreferencesPage owns its scroller; expose the reset row at the bottom.
        fn scroller(widget: &gtk::Widget) -> Option<gtk::ScrolledWindow> {
            if let Some(s) = widget.downcast_ref::<gtk::ScrolledWindow>() {
                return Some(s.clone());
            }
            let mut child = widget.first_child();
            while let Some(w) = child {
                child = w.next_sibling();
                if let Some(s) = scroller(&w) {
                    return Some(s);
                }
            }
            None
        }
        let scroll = scroller(&page).unwrap().vadjustment();
        scroll.set_value(scroll.upper());
        settle().await;
        snapshot(
            &prefs.window,
            directory.join(format!("{prefix}-light-reset.png")),
        );
        config.set("theme", json!("dark")).unwrap();
        settle().await;
        snapshot(
            &prefs.window,
            directory.join(format!("{prefix}-dark-reset.png")),
        );
        let rules = || {
            prefs
                .window
                .visible_dialog()
                .filter(|d| d.widget_name() == "naming-rules-dialog")
                .expect("naming rules dialog")
        };
        let sections = named(&prefs.window, "capture-sections").downcast::<gtk::Stack>().unwrap();
        let dialog = || {
            prefs
                .window
                .visible_dialog()
                .and_then(|d| d.downcast::<adw::AlertDialog>().ok())
                .expect("confirmation dialog")
        };
        fn respond(dialog: &adw::AlertDialog, id: &str) {
            dialog.emit_by_name_with_details::<()>("response", glib::Quark::from_str(id), &[&id]);
            dialog.force_close();
        }
        named(&prefs.window, "reset-capture-display").downcast::<gtk::Button>().unwrap().emit_clicked();
        glib::timeout_future(std::time::Duration::from_millis(600)).await;
        assert_eq!(config.value("capture-mask-color"), json!("#000000ba"));
        snapshot(&prefs.window, directory.join(format!("{prefix}-reset-confirmation.png")));
        assert_eq!(dialog().heading().as_deref(),Some("恢复默认设置？"));
        assert!(dialog().body().is_empty());
        respond(&dialog(), "cancel");
        settle().await;
        assert_eq!(config.value("capture-mask-color"), json!("#000000ba"));
        named(&prefs.window, "reset-capture-display").downcast::<gtk::Button>().unwrap().emit_clicked();
        settle().await;
        respond(&dialog(), "reset");
        settle().await;
        assert_eq!(config.value("capture-mask-color"), clivuu_core::config::schema()["capture-mask-color"]["default"]);
        assert_eq!(config.value("capture-hide-pins"), json!(false));
        assert_eq!(config.value("scroll-interval"), json!(1700));
        sections.set_visible_child_name("behavior");
        settle().await;
        named(&prefs.window, "reset-capture-behavior").downcast::<gtk::Button>().unwrap().emit_clicked();
        settle().await;
        respond(&dialog(), "reset");
        settle().await;
        for key in ["capture-hide-pins", "scroll-interval"] {
            assert_eq!(config.value(key), clivuu_core::config::schema()[key]["default"]);
        }
        assert_eq!(config.value("language"), json!("zh-CN"));
        assert_eq!(config.value("theme"), json!("dark"));
        assert_eq!(
            config.value("video-directory"),
            json!("/tmp/preferences-video-output")
        );
        config.set("image-quick-name", json!("$yyyy$/$MM$/image_$uuid$.jpg")).unwrap();
        config.set("video-quick-name", json!("$yyyy$/$MM$/video_$unix_ms$.webm")).unwrap();
        let save = named(&prefs.window, "save").downcast::<gtk::ListBoxRow>().unwrap();
        save.parent().unwrap().downcast::<gtk::ListBox>().unwrap().select_row(Some(&save));
        settle().await;
        let saving = named(&prefs.window, "save-sections").downcast::<gtk::Stack>().unwrap();
        assert_eq!(saving.visible_child_name().as_deref(), Some("images"));
        assert!(find(prefs.window.upcast_ref(), "naming-rules").is_none());
        let entry_text = |key: &str| {
            named(&prefs.window, key)
                .downcast::<adw::EntryRow>()
                .unwrap()
                .text()
                .to_string()
        };
        let preview_text = |key: &str| {
            named(&prefs.window, &format!("preview-{key}"))
                .downcast::<adw::ActionRow>()
                .unwrap()
                .subtitle()
                .unwrap_or_default()
                .to_string()
        };
        assert_eq!(entry_text("image-manual-name"), "Clivuu_$yyyy-MM-dd_HH-mm-ss$");
        assert!(preview_text("image-manual-name").ends_with(".png"));
        assert_eq!(entry_text("image-auto-name"), "Clivuu_$yyyy-MM-dd_HH-mm-ss$");
        assert!(preview_text("image-auto-name").ends_with(".png"));
        assert_eq!(entry_text("video-manual-name"), "Clivuu_$yyyy-MM-dd_HH-mm-ss$");
        assert!(preview_text("video-manual-name").ends_with(".mp4"));
        let trigger_naming_tip = |key: &str| {
            assert!(find(prefs.window.upcast_ref(), &format!("naming-rules-{key}")).is_none());
            let tip = named(&prefs.window, &format!("tip-{key}"))
                .downcast::<gtk::Button>()
                .unwrap();
            assert!(!tip.is::<gtk::MenuButton>());
            assert!(tip.tooltip_text().is_some_and(|s| !s.is_empty()));
            tip
        };
        for key in ["image-quick-name", "image-auto-name"] {
            let tip = trigger_naming_tip(key);
            assert!(tip.is_visible());
        }
        trigger_naming_tip("image-manual-name").emit_clicked();
        glib::timeout_future(std::time::Duration::from_millis(600)).await;
        assert_eq!(rules().title(), "命名规则");
        snapshot(&prefs.window, directory.join(format!("{prefix}-naming-rules-window.png")));
        snapshot(&rules().child().unwrap(), directory.join(format!("{prefix}-naming-rules.png")));
        fn selection_count(widget: &gtk::Widget) -> usize {
            let own=usize::from(widget.downcast_ref::<gtk::Label>().is_some_and(|l|l.selection_bounds().is_some()));
            let mut total=own;let mut child=widget.first_child();
            while let Some(w)=child {child=w.next_sibling();total+=selection_count(&w);}
            total
        }
        let initial_naming_selection = selection_count(rules().upcast_ref());
        assert_eq!(initial_naming_selection,0,"naming help must open without selected text");
        let intro = named(&rules(), "naming-rules-intro").downcast::<gtk::Label>().unwrap();
        assert!(!intro.is_selectable() && !intro.has_focus());
        assert!(!rules().focus().is_some_and(|w| w.is::<gtk::Label>()), "naming help must not open with a label caret");
        std::fs::write(directory.join(format!("{prefix}-initial-selection.json")),json!({"selected_labels":initial_naming_selection}).to_string()).unwrap();
        fn selectable(widget: &gtk::Widget) -> usize {
            let own=usize::from(widget.downcast_ref::<gtk::Label>().is_some_and(|l|l.is_selectable()));
            let mut total=own;let mut child=widget.first_child();
            while let Some(w)=child {child=w.next_sibling();total+=selectable(&w);}
            total
        }
        assert!(selectable(rules().upcast_ref())>=31);
        let _ = rules().close();
        settle().await;
        snapshot(&prefs.window, directory.join(format!("{prefix}-dark-save.png")));
        config.set("theme", json!("light")).unwrap();
        settle().await;
        snapshot(&prefs.window, directory.join(format!("{prefix}-light-save.png")));
        trigger_naming_tip("image-manual-name").emit_clicked();
        settle().await;
        snapshot(&prefs.window, directory.join(format!("{prefix}-light-naming-rules-window.png")));
        snapshot(&rules().child().unwrap(),directory.join(format!("{prefix}-light-naming-rules.png")));
        let _ = rules().close();
        settle().await;
        config.set("theme", json!("dark")).unwrap();
        config.set("recording-storage", json!("direct")).unwrap();
        named(&prefs.window, "reset-save-images").downcast::<gtk::Button>().unwrap().emit_clicked();
        settle().await;
        assert_eq!(config.value("image-quick-name"), json!("$yyyy$/$MM$/image_$uuid$.jpg"));
        respond(&dialog(), "cancel");
        settle().await;
        assert_eq!(config.value("image-quick-name"), json!("$yyyy$/$MM$/image_$uuid$.jpg"));
        named(&prefs.window, "reset-save-images").downcast::<gtk::Button>().unwrap().emit_clicked();
        settle().await;
        respond(&dialog(), "reset");
        settle().await;
        assert_eq!(config.value("image-quick-name"), clivuu_core::config::schema()["image-quick-name"]["default"]);
        assert_eq!(entry_text("image-quick-name"), "Clivuu_$yyyy-MM-dd_HH-mm-ss$");
        assert!(preview_text("image-quick-name").ends_with(".png"));
        let quick_entry = named(&prefs.window, "image-quick-name")
            .downcast::<adw::EntryRow>()
            .unwrap();
        quick_entry.set_text("custom_$yyyy$");
        let quick_preview = preview_text("image-quick-name");
        assert!(quick_preview.ends_with(".png") && quick_preview.contains("custom_"));
        quick_entry.set_text("Clivuu_$yyyy-MM-dd_HH-mm-ss$");
        assert_eq!(config.value("video-directory"), json!("/tmp/preferences-video-output"));
        assert_eq!(config.value("video-quick-name"), json!("$yyyy$/$MM$/video_$unix_ms$.webm"));
        assert_eq!(config.value("recording-storage"), json!("direct"));
        config.set("image-quality", json!(85)).unwrap();
        saving.set_visible_child_name("recording");
        settle().await;
        assert!(trigger_naming_tip("video-manual-name").is_visible());
        config.set("language", json!("en")).unwrap();
        settle().await;
        assert_eq!(saving.page(&saving.visible_child().unwrap()).title().as_deref(), Some("Recording"));
        config.set("language", json!("zh-CN")).unwrap();
        settle().await;
        snapshot(&prefs.window, directory.join(format!("{prefix}-dark-recording-save.png")));
        config.set("theme", json!("light")).unwrap();
        settle().await;
        snapshot(&prefs.window, directory.join(format!("{prefix}-light-recording-save.png")));
        config.set("theme", json!("dark")).unwrap();
        trigger_naming_tip("video-quick-name").emit_clicked();
        settle().await;
        assert_eq!(rules().title(), "命名规则");
        let _ = rules().close();
        settle().await;
        named(&prefs.window, "reset-save-recording").downcast::<gtk::Button>().unwrap().emit_clicked();
        settle().await;
        assert_eq!(config.value("video-directory"), json!("/tmp/preferences-video-output"));
        respond(&dialog(), "cancel");
        settle().await;
        assert_eq!(config.value("recording-storage"), json!("direct"));
        named(&prefs.window, "reset-save-recording").downcast::<gtk::Button>().unwrap().emit_clicked();
        settle().await;
        respond(&dialog(), "reset");
        settle().await;
        assert_eq!(entry_text("video-quick-name"), "Clivuu_$yyyy-MM-dd_HH-mm-ss$");
        assert!(preview_text("video-quick-name").ends_with(".mp4"));
        let video_entry = named(&prefs.window, "video-quick-name")
            .downcast::<adw::EntryRow>()
            .unwrap();
        video_entry.set_text("clip_$yyyy$");
        let video_preview = preview_text("video-quick-name");
        assert!(video_preview.ends_with(".mp4") && video_preview.contains("clip_"));
        video_entry.set_text("Clivuu_$yyyy-MM-dd_HH-mm-ss$");
        for key in ["video-directory", "video-quick-name", "recording-storage"] {
            assert_eq!(config.value(key), clivuu_core::config::schema()[key]["default"]);
        }
        assert_eq!(config.value("image-quality"), json!(85));
        config.reset(&["image-quality".into()]);
        assert_eq!(config.value("language"), json!("zh-CN"));
        pages.set_visible_child_name("general");
        config.flush().await.unwrap();
        let file = std::fs::read_to_string(clivuu_core::config::path().unwrap()).unwrap();
        assert!(!file.contains("capture-mask-color ="));
        assert!(!file.contains("scroll-interval ="));
        let ocr_page = pages.child_by_name("ocr").unwrap();
        assert!(named(&prefs.window, "ocr-languages").is_ancestor(&ocr_page));
        let rt_row = named(&prefs.window, "ocr-onnx-runtime");
        let cn_row = named(&prefs.window, "ocr-pack-chinese");
        assert!(rt_row.is_ancestor(&ocr_page));
        assert!(cn_row.is_ancestor(&ocr_page));
        let group_of = |mut w: gtk::Widget| -> adw::PreferencesGroup {
            loop {
                w = w.parent().expect("row must belong to a PreferencesGroup");
                if let Ok(g) = w.clone().downcast::<adw::PreferencesGroup>() {
                    return g;
                }
            }
        };
        let rt_group = group_of(rt_row);
        let cn_group = group_of(cn_row);
        assert_ne!(rt_group, cn_group, "ONNX Runtime must sit in its own PreferencesGroup separate from language models");
        assert!(rt_group.header_suffix().is_some(), "ONNX Runtime group must expose Open runtime folder button");
        assert!(cn_group.header_suffix().is_some(), "Local models group must expose Open models folder button");
        let ocr_nav = named(&prefs.window, "ocr").downcast::<gtk::ListBoxRow>().unwrap();
        ocr_nav.parent().unwrap().downcast::<gtk::ListBox>().unwrap().select_row(Some(&ocr_nav));
        settle().await;
        assert_eq!(pages.page(&ocr_page).title().as_deref(), Some("文字识别"));
        let ocr = named(&prefs.window, "ocr-languages").downcast::<adw::ExpanderRow>().unwrap();
        ocr.set_expanded(true);
        settle().await;
        let chi_sim = named(&prefs.window, "ocr-tess-chi_sim").downcast::<gtk::CheckButton>().unwrap();
        chi_sim.set_active(false);
        settle().await;
        assert_eq!(config.value("ocr-languages"), "eng");
        snapshot(&prefs.window, directory.join(format!("{prefix}-ocr-expanded.png")));
        ocr.set_expanded(false);
        settle().await;
        snapshot(&prefs.window, directory.join(format!("{prefix}-ocr.png")));
        let reset_ocr = named(&prefs.window, "reset-ocr").downcast::<gtk::Button>().unwrap();
        reset_ocr.emit_clicked(); settle().await;
        respond(&dialog(), "cancel"); settle().await;
        assert_eq!(config.value("ocr-languages"), "eng");
        reset_ocr.emit_clicked(); settle().await;
        respond(&dialog(), "reset"); settle().await;
        assert_eq!(config.value("ocr-languages"), clivuu_core::config::schema()["ocr-languages"]["default"]);
        assert_eq!(config.value("language"), "zh-CN");
        let about_nav = named(&prefs.window, "about").downcast::<gtk::ListBoxRow>().unwrap();
        about_nav.parent().unwrap().downcast::<gtk::ListBox>().unwrap().select_row(Some(&about_nav));
        settle().await;
        assert_eq!(pages.page(&pages.visible_child().unwrap()).title().as_deref(), Some("关于"));
        assert_eq!(named(&prefs.window, "about-version").downcast::<gtk::Label>().unwrap().text(), env!("CARGO_PKG_VERSION"));
        assert_eq!(named(&prefs.window, "about-project").downcast::<gtk::LinkButton>().unwrap().uri(), "https://github.com/Naerelyth/clivuu");
        let artwork = named(&prefs.window, "about-icon").downcast::<gtk::Image>().unwrap();
        let artwork_icon = artwork.gicon().unwrap().downcast::<gio::FileIcon>().unwrap();
        assert_eq!(artwork_icon.file().path().unwrap(), clivuu_desktop::platform::integration::icons::transparent());
        fn source_texture(node: &gtk::gsk::RenderNode) -> gtk::gdk::Texture {
            if let Some(n) = node.downcast_ref::<gtk::gsk::TextureNode>() { return n.texture(); }
            if let Some(n) = node.downcast_ref::<gtk::gsk::TextureScaleNode>() { return n.texture(); }
            if let Some(n) = node.downcast_ref::<gtk::gsk::TransformNode>() { return source_texture(&n.child()); }
            panic!("unexpected artwork node {:?}", node.node_type());
        }
        let icon_theme = gtk::IconTheme::for_display(&gtk::prelude::WidgetExt::display(&artwork));
        let mut artwork_sizes = vec![];
        for scale in [1, 2] {
            let paintable = icon_theme.lookup_by_gicon(&artwork_icon, artwork.pixel_size(), scale, gtk::TextDirection::None, gtk::IconLookupFlags::empty());
            assert!(!paintable.is_symbolic());
            let snapshot = gtk::Snapshot::new();
            snapshot.scale(scale as f32, scale as f32);
            paintable.snapshot(&snapshot, 112., 112.);
            let node = snapshot.to_node().unwrap();
            let texture = source_texture(&node);
            assert_eq!((texture.width(),texture.height()),(112*scale,112*scale),"SVG must be rasterized at physical display size, not upscaled from 64px");
            artwork_sizes.push(json!({"scale":scale,"width":texture.width(),"height":texture.height()}));
            prefs.window.renderer().unwrap().render_texture(&node,None).save_to_png(directory.join(format!("{prefix}-about-icon-{scale}x.png"))).unwrap();
        }
        std::fs::write(directory.join(format!("{prefix}-about-icon.json")),json!(artwork_sizes).to_string()).unwrap();
        let github_icon = named(&prefs.window, "about-github-icon").downcast::<gtk::Image>().unwrap();
        assert!(github_icon.gicon().is_some());
        for theme in ["dark", "light"] {
            config.set("theme", json!(theme)).unwrap(); settle().await;
            snapshot(&prefs.window, directory.join(format!("{prefix}-{theme}-about.png")));
        }
        prefs.window.close();
        settle().await;
        assert!(app.windows().is_empty());
        let weak = Rc::downgrade(&prefs);
        drop(prefs);
        assert!(weak.upgrade().is_none());
        config.set("language", json!("en")).unwrap();
        let fresh = Preferences::new(&app, config.clone(), Default::default());
        let pages = named(&fresh.window, "preferences-pages")
            .downcast::<gtk::Stack>()
            .unwrap();
        assert_eq!(pages.visible_child_name().as_deref(), Some("general"));
        fresh.present();
        settle().await;
        pages.set_visible_child_name("ocr");
        settle().await;
        assert_eq!(pages.page(&pages.visible_child().unwrap()).title().as_deref(), Some("OCR"));
        snapshot(&fresh.window, directory.join(format!("{prefix}-ocr-en.png")));
        let ocr_scroller = named(&pages.visible_child().unwrap(), "GtkScrolledWindow")
            .downcast::<gtk::ScrolledWindow>()
            .unwrap();
        let vadj = ocr_scroller.vadjustment();
        vadj.set_value((vadj.upper() - vadj.page_size()).max(0.0));
        settle().await;
        let ocr_tab = named(&fresh.window, "ocr").compute_bounds(&fresh.window).unwrap();
        let ocr = named(&named(&fresh.window, "ocr-download-mirror"), "GtkText")
            .compute_bounds(&fresh.window).unwrap();
        std::fs::write(directory.join(format!("{prefix}-ocr-layout.json")), json!({
            "x":ocr.x(), "y":ocr.y(), "width":ocr.width(), "height":ocr.height(),
            "tab":{"x":ocr_tab.x(),"y":ocr_tab.y(),"width":ocr_tab.width(),"height":ocr_tab.height()}
        }).to_string()).unwrap();
        // The Shell observer verifies actual taskbar association while open.
        std::fs::write(directory.join(format!("{prefix}-ready")), "ready").unwrap();
        for _ in 0..600 {
            if directory.join(format!("{prefix}-observed")).exists() {
                break;
            }
            let surface=fresh.window.surface().unwrap();
            let pointer=gtk::prelude::WidgetExt::display(&fresh.window).default_seat().unwrap().pointer().unwrap();
            let cursor=surface.device_cursor(&pointer).or_else(||surface.cursor()).and_then(|c|c.name()).map(|n|n.to_string());
            let point=surface.device_position(&pointer);
            let (dx,dy)=fresh.window.surface_transform();
            let pick=point.and_then(|(x,y,_)|fresh.window.pick(x+dx,y+dy,gtk::PickFlags::DEFAULT));
            let control=|name:&str|{let w=named(&fresh.window,name);w.compute_bounds(&fresh.window).map(|r|json!({"x":r.x(),"y":r.y(),"width":r.width(),"height":r.height()}))};
            let grabbed=gtk::prelude::WidgetExt::display(&fresh.window).device_is_grabbed(&pointer);
            let state=json!({"cursor":cursor,"grabbed":grabbed,"pointer":point.map(|(x,y,_)|[x,y]),"pick":pick.map(|w|w.type_().name().to_string()),"width":fresh.window.width(),"height":fresh.window.height(),"save":control("save"),"manual_name":control("image-manual-name"),"manual_text":named(&named(&fresh.window,"image-manual-name"),"GtkText").compute_bounds(&fresh.window).map(|r|json!({"x":r.x(),"y":r.y(),"width":r.width(),"height":r.height()}))});
            let temp=directory.join(format!("{prefix}-input.tmp"));std::fs::write(&temp,state.to_string()).unwrap();std::fs::rename(temp,directory.join(format!("{prefix}-input.json"))).unwrap();
            glib::timeout_future(std::time::Duration::from_millis(50)).await;
        }
        fresh.window.close();
        drop(fresh);
        config.reset(&["language".into(), "theme".into(), "video-directory".into()]);
        config.flush().await.unwrap();
        assert!(errors.borrow().is_empty(), "{:?}", errors.borrow());
        std::fs::write(directory.join(format!("{prefix}.json")), json!({
            "ocr_page_and_scoped_reset":true,"localized_about_and_package_metadata":true,"startup_os_state":true,"settings_font_scope_and_fallback":true,"shared_rgba_color_dialog":true,"native_language_names":true, "live_language":true, "inline_accent_reset":true,
            "section_scoped_reset":true, "reset_confirmation":true, "save_sections_reset":true, "filename_tip_naming_rules":true, "save_sections_live_language":true, "selectable_naming_rules":true, "concise_reset_confirmation":true, "reset_removes_override":true, "fresh_general_window":true,"naming_help_initially_unselected":initial_naming_selection==0,
            "errors":errors.borrow().clone()
        }).to_string()).unwrap();
    });
}

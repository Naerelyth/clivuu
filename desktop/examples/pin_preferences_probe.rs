//! Real native GTK bindings, preview and per-section confirmation/reset.
use adw::prelude::*;
use clivuu_desktop::{
    infrastructure::{configuration::Configuration, tasks::TaskPool},
    ui::preferences::Preferences,
};
use gtk::{gio, glib};
use serde_json::json;
use std::{path::PathBuf, rc::Rc, time::Duration};
fn named(root: &impl IsA<gtk::Widget>, name: &str) -> gtk::Widget {
    fn find(w: &gtk::Widget, n: &str) -> Option<gtk::Widget> {
        if w.widget_name() == n {
            return Some(w.clone());
        }
        let mut c = w.first_child();
        while let Some(w) = c {
            c = w.next_sibling();
            if let Some(w) = find(&w, n) {
                return Some(w);
            }
        }
        None
    }
    find(root.as_ref(), name).unwrap_or_else(|| panic!("missing {name}"))
}
fn reset_rows(root: &impl IsA<gtk::Widget>) -> Vec<adw::ActionRow> {
    fn visit(w: &gtk::Widget, rows: &mut Vec<adw::ActionRow>) {
        if w.has_css_class("reset-preferences") {
            rows.push(w.clone().downcast().unwrap());
        }
        let mut child = w.first_child();
        while let Some(w) = child {
            child = w.next_sibling();
            visit(&w, rows);
        }
    }
    let mut rows = vec![];
    visit(root.as_ref(), &mut rows);
    rows
}
async fn settle() {
    glib::timeout_future(Duration::from_millis(350)).await;
}
fn shot(w: &impl IsA<gtk::Window>, path: PathBuf) {
    let w = w.as_ref();
    let snapshot = gtk::Snapshot::new();
    gtk::WidgetPaintable::new(Some(w)).snapshot(&snapshot, w.width() as f64, w.height() as f64);
    w.renderer()
        .unwrap()
        .render_texture(snapshot.to_node().unwrap(), None)
        .save_to_png(path)
        .unwrap();
}
fn dialog(window: &adw::ApplicationWindow) -> adw::AlertDialog {
    window
        .visible_dialog()
        .and_then(|d| d.downcast::<adw::AlertDialog>().ok())
        .unwrap()
}
fn respond(dialog: &adw::AlertDialog, id: &str) {
    dialog.emit_by_name_with_details::<()>("response", glib::Quark::from_str(id), &[&id]);
    dialog.force_close();
}
async fn probe() {
    let directory = PathBuf::from(std::env::var_os("CLIVUU_TEST_DIR").unwrap());
    let pool = Rc::new(TaskPool::new(1, 4).unwrap());
    let config = Configuration::new(clivuu_core::config::read().unwrap(), pool, |e| {
        panic!("{e}")
    })
    .unwrap();
    config.set("language", json!("en")).unwrap();
    config.set("theme", json!("dark")).unwrap();
    let app = gtk::Application::builder()
        .application_id("io.github.naerelyth.Clivuu.PinPreferencesProbe")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(gio::Cancellable::NONE).unwrap();
    let prefs = Preferences::new(&app, config.clone(), Default::default());
    prefs.present();
    settle().await;
    assert_eq!(
        gtk::prelude::WidgetExt::display(&prefs.window)
            .type_()
            .name(),
        "GdkWaylandDisplay"
    );
    let pages = named(&prefs.window, "preferences-pages")
        .downcast::<gtk::Stack>()
        .unwrap();
    pages.set_visible_child_name("pins");
    let sections = named(&prefs.window, "pins-sections")
        .downcast::<gtk::Stack>()
        .unwrap();
    let mut results = vec![];
    let mut check = |name: &str, passed: bool| {
        assert!(passed, "{name}");
        results.push(json!({"name":name,"passed":passed}));
    };
    check(
        "three native pin preference sections",
        sections.pages().n_items() == 3,
    );
    let resets = reset_rows(&prefs.window);
    check(
        "all reset rows share one title and retain their scope descriptions",
        resets.len() == 9
            && resets.iter().all(|r| {
                r.title() == "Reset settings"
                    && r.subtitle()
                        .is_some_and(|s| s.starts_with("Only settings in "))
            }),
    );

    // Verify the rendered glow, including alpha, rather than just the picker value.
    let source = gtk::gdk::MemoryTexture::new(
        32,
        32,
        gtk::gdk::MemoryFormat::R8g8b8a8,
        &glib::Bytes::from_owned(vec![255; 32 * 32 * 4]),
        32 * 4,
    );
    let image = clivuu_desktop::ui::pin_image::PinImage::new(source.upcast_ref());
    image.set_size_request(56, 56);
    let sample = gtk::Window::builder()
        .application(&app)
        .decorated(false)
        .child(&image)
        .build();
    sample.present();
    settle().await;
    let mut edge_alpha = vec![];
    for alpha in [1., 0.5, 0.] {
        image.update(32., 32., &gtk::gdk::RGBA::new(0.2, 0.4, 0.8, alpha), true);
        settle().await;
        let snapshot = gtk::Snapshot::new();
        gtk::WidgetPaintable::new(Some(&image)).snapshot(&snapshot, 56., 56.);
        let texture = sample.renderer().unwrap().render_texture(
            snapshot.to_node().unwrap(),
            Some(&gtk::graphene::Rect::new(0., 0., 56., 56.)),
        );
        let mut pixels = vec![0; 56 * 56 * 4];
        texture.download(&mut pixels, 56 * 4);
        edge_alpha.push(pixels[(28 * 56 + 11) * 4 + 3]);
    }
    let mut colored_edges = vec![];
    for color in [gtk::gdk::RGBA::RED, gtk::gdk::RGBA::new(0., 1., 0., 0.)] {
        image.update(32., 32., &color, true);
        settle().await;
        let snapshot = gtk::Snapshot::new();
        gtk::WidgetPaintable::new(Some(&image)).snapshot(&snapshot, 56., 56.);
        let texture = sample.renderer().unwrap().render_texture(
            snapshot.to_node().unwrap(),
            Some(&gtk::graphene::Rect::new(0., 0., 56., 56.)),
        );
        let mut bytes = vec![0; 56 * 56 * 4];
        texture.download(&mut bytes, 56 * 4);
        let edges = (1..=12)
            .map(|d| bytes[(28 * 56 + 12 - d) * 4..(28 * 56 + 12 - d) * 4 + 4].to_vec())
            .collect::<Vec<_>>();
        if color == gtk::gdk::RGBA::RED {
            check(
                "shadow uses the measured reference falloff without an activation opacity multiplier",
                edges.iter().map(|p| p[3]).collect::<Vec<_>>()
                    == [96, 70, 49, 32, 20, 13, 7, 3, 0, 0, 0, 0],
            );
        }
        colored_edges.push(edges);
    }
    check(
        "shadow renderer honors supplied RGBA without substituting a fixed color",
        colored_edges[0] != colored_edges[1]
            && colored_edges[0][0][2] == 96
            && colored_edges[0][0][3] == 96
            && colored_edges[1].iter().all(|p| p[3] == 0),
    );
    image.configure(false, true);
    image.update(32., 32., &gtk::gdk::RGBA::RED, true);
    settle().await;
    let snapshot = gtk::Snapshot::new();
    gtk::WidgetPaintable::new(Some(&image)).snapshot(&snapshot, 56., 56.);
    let texture = sample.renderer().unwrap().render_texture(
        snapshot.to_node().unwrap(),
        Some(&gtk::graphene::Rect::new(0., 0., 56., 56.)),
    );
    let mut bytes = vec![0; 56 * 56 * 4];
    texture.download(&mut bytes, 56 * 4);
    check(
        "disabling shadow removes active and inactive glows without changing image pixels",
        bytes[(28 * 56 + 11) * 4 + 3] == 0 && bytes[(28 * 56 + 20) * 4 + 3] == 255,
    );
    sample.destroy();
    check(
        "inactive pin shadow rendering preserves configured RGBA alpha",
        edge_alpha[0] > 0
            && edge_alpha[2] == 0
            && (i32::from(edge_alpha[1]) * 2 - i32::from(edge_alpha[0])).abs() <= 2,
    );

    for theme in ["dark", "light"] {
        config.set("theme", json!(theme)).unwrap();
        for section in ["display", "behavior", "conversion"] {
            sections.set_visible_child_name(section);
            settle().await;
            shot(
                &prefs.window,
                directory.join(format!("pins-{theme}-{section}.png")),
            );
        }
    }
    sections.set_visible_child_name("display");
    let opacity = named(&prefs.window, "pin-opacity")
        .downcast::<adw::SpinRow>()
        .unwrap();
    opacity.set_value(61.);
    let shadow = named(&prefs.window, "pin-shadow")
        .downcast::<adw::SwitchRow>()
        .unwrap();
    shadow.set_active(false);
    check(
        "display controls publish validated settings",
        config.value("pin-opacity") == 61 && config.value("pin-shadow") == false,
    );
    sections.set_visible_child_name("behavior");
    named(&prefs.window, "pin-zoom-at-cursor")
        .downcast::<adw::SwitchRow>()
        .unwrap()
        .set_active(true);
    check(
        "behavior switch publishes policy",
        config.value("pin-zoom-at-cursor") == true,
    );
    let focus = named(&prefs.window, "pin-gnome-focus")
        .downcast::<adw::SwitchRow>()
        .unwrap();
    let gnome = clivuu_desktop::platform::display::is_gnome();
    check(
        "GNOME focus switch stays visible and is enabled only on GNOME",
        focus.is_visible() && focus.is_sensitive() == gnome,
    );
    check(
        "pin focus hint describes panel and dock visibility and input limitations",
        focus.subtitle().as_deref()
            == Some(
                "When disabled, clicking pins preserves Dash-to-Panel and Dash-to-Dock smart hiding, but pins cannot receive keyboard input and activation is limited",
            ),
    );
    check("GNOME pin focus defaults on", focus.is_active());
    if gnome {
        focus.set_active(false);
        check(
            "GNOME focus switch publishes its policy",
            config.value("pin-gnome-focus") == false,
        );
    }
    let memory = named(&prefs.window, "pin-storage")
        .downcast::<adw::SwitchRow>()
        .unwrap();
    check(
        "memory storage switch defaults off and uses the persistent setting",
        !memory.is_active() && config.value("pin-storage") == "persistent",
    );
    memory.set_active(true);
    check(
        "memory storage switch maps on to the existing memory policy",
        config.value("pin-storage") == "memory",
    );
    memory.set_active(false);
    check(
        "memory storage switch maps off to persistent storage",
        config.value("pin-storage") == "persistent",
    );
    config.set("pin-storage", json!("memory")).unwrap();
    check(
        "external storage changes update the switch",
        memory.is_active(),
    );
    sections.set_visible_child_name("conversion");
    let preview = named(&prefs.window, "pin-text-preview")
        .downcast::<gtk::Box>()
        .unwrap();
    settle().await;
    check(
        "conversion has a rendered preview",
        preview.first_child().is_some(),
    );
    named(&prefs.window, "pin-text-margin")
        .downcast::<adw::SpinRow>()
        .unwrap()
        .set_value(33.);
    named(&prefs.window, "pin-text-font")
        .downcast::<gtk::FontDialogButton>()
        .unwrap()
        .set_font_desc(&gtk::pango::FontDescription::from_string("Sans 18"));
    settle().await;
    check(
        "font and margin controls update preview settings",
        config.value("pin-text-margin") == 33 && config.value("pin-text-font") == "Sans 18",
    );
    let enabled = named(&prefs.window, "pin-text-enabled")
        .downcast::<adw::SwitchRow>()
        .unwrap();
    enabled.set_active(false);
    check(
        "conversion options follow the main switch",
        !named(&prefs.window, "pin-text-margin").is_sensitive(),
    );
    enabled.set_active(true);
    sections.set_visible_child_name("display");
    let reset = named(&prefs.window, "reset-pins-display")
        .downcast::<gtk::Button>()
        .unwrap();
    reset.emit_clicked();
    settle().await;
    check(
        "reset requires confirmation",
        dialog(&prefs.window).default_response().as_deref() == Some("cancel"),
    );
    respond(&dialog(&prefs.window), "cancel");
    settle().await;
    check("cancel keeps values", config.value("pin-opacity") == 61);
    reset.emit_clicked();
    settle().await;
    respond(&dialog(&prefs.window), "reset");
    settle().await;
    check(
        "display reset restores defaults without touching other sections",
        config.value("pin-opacity") == 100
            && config.value("pin-shadow") == true
            && config.value("pin-zoom-at-cursor") == true
            && config.value("pin-text-margin") == 33
            && config.value("pin-storage") == "memory",
    );
    config.flush().await.unwrap();
    let ini = std::fs::read_to_string(clivuu_core::config::path().unwrap()).unwrap();
    check(
        "reset defaults are removed from the INI",
        !ini.contains("pin-opacity") && !ini.contains("pin-shadow="),
    );
    for (section, button) in [
        ("conversion", "reset-pins-conversion"),
        ("behavior", "reset-pins-behavior"),
    ] {
        sections.set_visible_child_name(section);
        named(&prefs.window, button)
            .downcast::<gtk::Button>()
            .unwrap()
            .emit_clicked();
        settle().await;
        respond(&dialog(&prefs.window), "reset");
        settle().await;
    }
    check(
        "each remaining section resets independently",
        config.value("pin-zoom-at-cursor") == false
            && config.value("pin-flash-new") == false
            && config.value("pin-text-margin") == 20
            && config.value("pin-text-font") == "Sans 9"
            && config.value("pin-gnome-focus") == true
            && config.value("pin-storage") == "persistent"
            && !memory.is_active(),
    );
    pages.set_visible_child_name("control");
    let controls = named(&prefs.window, "control-sections")
        .downcast::<gtk::Stack>()
        .unwrap();
    check(
        "control has fixed keys and configurable mouse sections",
        controls.pages().n_items() == 2,
    );
    let dropdown = |name| {
        named(&prefs.window, name)
            .downcast::<gtk::DropDown>()
            .unwrap()
    };
    for theme in ["dark", "light"] {
        config.set("theme", json!(theme)).unwrap();
        for section in ["keys", "mouse"] {
            controls.set_visible_child_name(section);
            settle().await;
            shot(
                &prefs.window,
                directory.join(format!("control-{theme}-{section}.png")),
            );
        }
    }
    dropdown("pin-mouse-destroy-modifier-1").set_selected(3);
    dropdown("pin-mouse-destroy-modifier-2").set_selected(2);
    dropdown("pin-mouse-destroy-trigger").set_selected(3);
    dropdown("pin-mouse-opacity-keys").set_selected(0);
    check(
        "mouse choices publish two modifiers, trigger and alternative keys",
        config.value("pin-mouse-destroy") == "shift+alt+right"
            && config.value("pin-mouse-opacity-keys") == false,
    );
    config
        .set("pin-mouse-destroy", json!("super+middle"))
        .unwrap();
    check(
        "external mouse configuration refreshes visible choices",
        dropdown("pin-mouse-destroy-modifier-1").selected() == 4
            && dropdown("pin-mouse-destroy-modifier-2").selected() == 0
            && dropdown("pin-mouse-destroy-trigger").selected() == 2
            && named(&prefs.window, "pin-mouse-destroy")
                .downcast::<adw::ExpanderRow>()
                .unwrap()
                .subtitle()
                == "Super + Middle-click",
    );
    dropdown("pin-mouse-destroy-modifier-1").set_selected(1);
    dropdown("pin-mouse-destroy-modifier-2").set_selected(1);
    check(
        "duplicate mouse modifier clears previous slot",
        dropdown("pin-mouse-destroy-modifier-1").selected() == 0
            && dropdown("pin-mouse-destroy-modifier-2").selected() == 1
            && config.value("pin-mouse-destroy") == "ctrl+middle",
    );
    config
        .set("pin-mouse-destroy", json!("super+middle"))
        .unwrap();
    config.set("language", json!("zh-CN")).unwrap();
    settle().await;
    check(
        "reset titles and pin hints use the requested Chinese text",
        reset_rows(&prefs.window)
            .iter()
            .all(|r| r.title() == "重置设置")
            && memory.title() == "仅在内存中保存贴图"
            && memory.subtitle().as_deref() == Some("开启后不再写入磁盘，Clivuu 重启后会丢失贴图")
            && focus.subtitle().as_deref()
                == Some(
                    "关闭后点击贴图不会改变 Dash-to-Panel 与 Dash-to-Dock 的 Panel/Dock 智能显隐状态，但贴图无法接收键盘输入且激活行为会受限",
                ),
    );

    check(
        "mouse dropdowns and section titles follow application language",
        controls
            .page(&controls.visible_child().unwrap())
            .title()
            .as_deref()
            == Some("鼠标")
            && dropdown("pin-mouse-destroy-trigger")
                .model()
                .unwrap()
                .downcast::<gtk::StringList>()
                .unwrap()
                .string(2)
                .as_deref()
                == Some("中键单击"),
    );
    shot(&prefs.window, directory.join("control-light-zh-mouse.png"));
    let reset = named(&prefs.window, "reset-control-mouse")
        .downcast::<gtk::Button>()
        .unwrap();
    config.set("pin-opacity", json!(72)).unwrap();
    reset.emit_clicked();
    settle().await;
    check(
        "mouse reset waits for confirmation",
        config.value("pin-mouse-destroy") == "super+middle",
    );
    respond(&dialog(&prefs.window), "cancel");
    settle().await;
    check(
        "cancel preserves mouse bindings",
        config.value("pin-mouse-opacity-keys") == false,
    );
    reset.emit_clicked();
    settle().await;
    respond(&dialog(&prefs.window), "reset");
    settle().await;
    check(
        "mouse reset restores its own defaults and leaves pin display unchanged",
        config.value("pin-mouse-destroy") == "ctrl+double-left"
            && config.value("pin-mouse-opacity-keys") == true
            && config.value("pin-opacity") == 72,
    );
    config.reset(&["pin-opacity".into(), "language".into(), "theme".into()]);
    config.flush().await.unwrap();
    prefs.window.close();
    std::fs::write(
        directory.join("pin-ui-results.json"),
        serde_json::to_vec(&results).unwrap(),
    )
    .unwrap();
}
fn main() {
    adw::init().unwrap();
    glib::MainContext::default().block_on(probe());
}

//! Inventory real settings strings across locales and live language changes.
use adw::prelude::*;
use clivuu_core::{
    annotation::Bounds,
    i18n::{self, Language},
};
use clivuu_desktop::editor::{
    Action, EditorController, ImageMapping, Tool, scene::EditorDocument, view::EditorView,
};
use clivuu_desktop::ui::viewport::Viewport;
use clivuu_desktop::{
    infrastructure::{configuration::Configuration, tasks::TaskPool},
    ui::preferences::Preferences,
};
use gtk::{gdk, gio, glib};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::{path::PathBuf, rc::Rc, time::Duration};
fn collect(w: &gtk::Widget, path: &str, out: &mut Vec<Value>) {
    let mut put = |property: &str, value: Option<glib::GString>| {
        if let Some(value) = value.filter(|s| !s.is_empty()) {
            out.push(json!({"path":path,"type":w.type_().name(),"name":w.widget_name().to_string(),"property":property,"text":value.to_string()}));
        }
    };
    put("tooltip", w.tooltip_text());
    if let Some(r) = w.downcast_ref::<adw::PreferencesRow>() {
        put("title", Some(r.title()));
    }
    if let Some(r) = w.downcast_ref::<adw::ActionRow>() {
        put("subtitle", r.subtitle());
    }
    if let Some(r) = w.downcast_ref::<adw::ExpanderRow>() {
        put("subtitle", Some(r.subtitle()));
    }
    if let Some(d) = w.downcast_ref::<adw::Dialog>() {
        put("title", Some(d.title()));
    }
    if let Some(d) = w.downcast_ref::<adw::AlertDialog>() {
        put("heading", d.heading());
        put("body", Some(d.body()));
    }
    if let Some(e) = w.downcast_ref::<gtk::SearchEntry>() {
        put("placeholder", e.placeholder_text());
    }
    if let Some(g) = w.downcast_ref::<adw::PreferencesGroup>() {
        put("title", Some(g.title()));
        put("description", g.description());
    }
    if let Some(l) = w.downcast_ref::<gtk::Label>() {
        put("label", Some(l.text()));
    }
    if let Some(b) = w.downcast_ref::<gtk::Button>() {
        put("label", b.label());
    }
    let model = if let Some(r) = w.downcast_ref::<adw::ComboRow>() {
        r.model()
    } else if let Some(d) = w.downcast_ref::<gtk::DropDown>() {
        d.model()
    } else {
        None
    };
    if let Some(model) = model.and_then(|m| m.downcast::<gtk::StringList>().ok()) {
        for i in 0..model.n_items() {
            put(&format!("choice-{i}"), model.string(i));
        }
    }
    if let Some(s) = w.downcast_ref::<gtk::Stack>() {
        let pages = s.pages();
        for i in 0..pages.n_items() {
            let p = pages.item(i).unwrap().downcast::<gtk::StackPage>().unwrap();
            put(&format!("page-{i}"), p.title());
        }
    }
    let mut child = w.first_child();
    let mut index = 0;
    while let Some(c) = child {
        child = c.next_sibling();
        collect(&c, &format!("{path}/{index}"), out);
        index += 1;
    }
}
fn named(root: &gtk::Widget, name: &str) -> gtk::Widget {
    if root.widget_name() == name {
        return root.clone();
    }
    let mut child = root.first_child();
    while let Some(c) = child {
        child = c.next_sibling();
        if let Some(found) = find(&c, name) {
            return found;
        }
    }
    panic!("missing {name}")
}
fn find(root: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if root.widget_name() == name {
        return Some(root.clone());
    }
    let mut child = root.first_child();
    while let Some(c) = child {
        child = c.next_sibling();
        if let Some(found) = find(&c, name) {
            return Some(found);
        }
    }
    None
}
async fn dialogs(prefs: &Rc<Preferences>, config: &Rc<Configuration>, out: &mut Vec<Value>) {
    named(prefs.window.upcast_ref(), "accent-color")
        .downcast::<gtk::Button>()
        .unwrap()
        .emit_clicked();
    glib::timeout_future(Duration::from_millis(180)).await;
    let dialog = prefs.window.visible_dialog().unwrap();
    collect(&dialog.child().unwrap(), "color", out);
    dialog.close();
    glib::timeout_future(Duration::from_millis(220)).await;
    let prev_border = config.value("capture-border-width");
    config.set("capture-border-width", json!(5)).unwrap();
    glib::timeout_future(Duration::from_millis(60)).await;
    named(prefs.window.upcast_ref(), "reset-capture-display")
        .downcast::<gtk::Button>()
        .unwrap()
        .emit_clicked();
    glib::timeout_future(Duration::from_millis(180)).await;
    let reset = prefs
        .window
        .visible_dialog()
        .and_then(|d| d.downcast::<adw::AlertDialog>().ok())
        .unwrap();
    collect(reset.upcast_ref(), "reset", out);
    reset.emit_by_name_with_details::<()>(
        "response",
        glib::Quark::from_str("cancel"),
        &[&"cancel"],
    );
    reset.force_close();
    glib::timeout_future(Duration::from_millis(220)).await;
    config.set("capture-border-width", prev_border).unwrap();
    named(prefs.window.upcast_ref(), "tip-image-manual-name")
        .downcast::<gtk::Button>()
        .unwrap()
        .emit_clicked();
    glib::timeout_future(Duration::from_millis(180)).await;
    let naming = prefs
        .window
        .visible_dialog()
        .filter(|d| d.widget_name() == "naming-rules-dialog")
        .unwrap();
    collect(naming.upcast_ref(), "naming", out);
    let _ = naming.close();
    glib::timeout_future(Duration::from_millis(180)).await;
}
fn translated_text(language: Language, text: &str, shortcut: bool) -> String {
    if shortcut
        && let Some((title, keys)) = text.rsplit_once(" (").filter(|(_, k)| k.ends_with(')'))
    {
        return format!("{} ({keys}", i18n::translate_for(language, title));
    }
    i18n::translate_for(language, text)
}

fn compare(base: &[Value], current: &[Value], language: Language) -> usize {
    let values: HashMap<_, _> = current
        .iter()
        .map(|v| {
            (
                (v["path"].as_str().unwrap(), v["property"].as_str().unwrap()),
                v["text"].as_str().unwrap(),
            )
        })
        .collect();
    let mut count = 0;
    for value in base {
        let source = value["text"].as_str().unwrap();
        if i18n::languages().any(|l| l.native_name() == source) {
            continue;
        }
        let shortcut = value["property"] == "tooltip"
            && value["path"].as_str().unwrap().starts_with("toolbar");
        let expected = translated_text(language, source, shortcut);
        if i18n::languages().any(|other| translated_text(other, source, shortcut) != source) {
            let key = (
                value["path"].as_str().unwrap(),
                value["property"].as_str().unwrap(),
            );
            assert_eq!(
                values.get(&key).copied(),
                Some(expected.as_str()),
                "{source} at {key:?}"
            );
            count += 1;
        }
    }
    count
}
fn main() {
    adw::init().unwrap();
    glib::MainContext::default().block_on(run());
}
async fn run() {
    let root = PathBuf::from(std::env::var_os("CLIVUU_TEST_DIR").unwrap());
    let pool = Rc::new(TaskPool::new(1, 2).unwrap());
    let config = Configuration::new(clivuu_core::config::read().unwrap(), pool, |e| {
        panic!("{e}")
    })
    .unwrap();
    config.set("language", json!("en")).unwrap();
    let app = gtk::Application::builder()
        .application_id("io.github.naerelyth.clivuu")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(gio::Cancellable::NONE).unwrap();
    let prefs = Preferences::new(&app, config.clone(), Default::default());
    prefs.present();
    let window = gtk::Window::builder()
        .title("Clivuu locale editor fixture")
        .default_width(800)
        .default_height(450)
        .build();
    let viewport = Viewport::new();
    window.set_child(Some(&viewport));
    let mut controller = EditorController::new(ImageMapping {
        display: Bounds {
            x: 0.,
            y: 0.,
            width: 600.,
            height: 300.,
        },
        document: [600., 300.],
    });
    for (tool, a, b) in [
        (Tool::Rect, [20., 20.], [80., 80.]),
        (Tool::Ellipse, [120., 20.], [190., 80.]),
    ] {
        controller.dispatch(Action::Tool(tool));
        controller.dispatch(Action::Begin {
            point: a,
            shift: false,
        });
        controller.dispatch(Action::Move {
            point: b,
            shift: false,
        });
        controller.dispatch(Action::End);
    }
    let document = EditorDocument::new(controller);
    let texture = gdk::MemoryTexture::new(
        600,
        300,
        gdk::MemoryFormat::R8g8b8a8,
        &glib::Bytes::from_owned(vec![255u8; 600 * 300 * 4]),
        600 * 4,
    )
    .upcast::<gdk::Texture>();
    let view = EditorView::new(
        &viewport,
        document.clone(),
        &texture,
        Bounds {
            x: 0.,
            y: 0.,
            width: 800.,
            height: 450.,
        },
        [0., 0.],
        true,
        Default::default(),
        |_| {},
    );
    let _theme = clivuu_desktop::ui::toolbar::Toolbar::install_theme(
        &gtk::prelude::WidgetExt::display(&window),
        true,
        "#bd93f9",
    );
    view.set_theme(&_theme);
    window.present();
    view.toolbar().grip().set_has_tooltip(false);
    let original = serde_json::to_value(document.borrow().session().state()).unwrap();
    let mut baseline = vec![];
    let mut editor_baseline = vec![];
    let mut reports = vec![];
    let english = i18n::language_for("en");
    let stages = std::iter::once(("en".to_owned(), english))
        .chain(
            i18n::languages()
                .filter(|l| *l != english)
                .map(|l| (l.id().to_owned(), l)),
        )
        .chain(std::iter::once(("en-again".to_owned(), english)));
    for (stage, language) in stages {
        config.set("language", json!(language.id())).unwrap();
        view.toolbar().refresh();
        view.options().refresh();
        glib::timeout_future(Duration::from_millis(400)).await;
        let mut text = vec![];
        collect(prefs.window.upcast_ref(), "settings", &mut text);
        dialogs(&prefs, &config, &mut text).await;
        let mut editor = vec![];
        collect(view.toolbar().widget().upcast_ref(), "toolbar", &mut editor);
        collect(view.options().widget().upcast_ref(), "subbar", &mut editor);
        if stage == "en" {
            baseline = text.clone();
            editor_baseline = editor.clone();
        }
        let checked = compare(&baseline, &text, language);
        let editor_checked = compare(&editor_baseline, &editor, language);
        assert!(
            !view.toolbar().grip().has_tooltip(),
            "translation must preserve disabled tooltips"
        );
        assert_eq!(
            serde_json::to_value(document.borrow().session().state()).unwrap(),
            original,
            "locale must preserve annotations/history"
        );
        std::fs::write(
            root.join(format!("i18n-{stage}.json")),
            serde_json::to_vec_pretty(&text).unwrap(),
        )
        .unwrap();
        std::fs::write(
            root.join(format!("i18n-editor-{stage}.json")),
            serde_json::to_vec_pretty(&editor).unwrap(),
        )
        .unwrap();
        reports.push(json!({"language":language.id(),"stage":stage,"properties":text.len(),"translated_properties":checked,"editor_translated_properties":editor_checked}));
    }
    view.scene().dispatch(Action::Undo);
    assert_eq!(document.borrow().session().marks().len(), 1);
    view.scene().dispatch(Action::Redo);
    assert_eq!(document.borrow().session().marks().len(), 2);
    std::fs::write(
        root.join("i18n-report.json"),
        serde_json::to_vec_pretty(&reports).unwrap(),
    )
    .unwrap();
    prefs.window.destroy();
    window.destroy();
    config.flush().await.unwrap();
    std::fs::write(root.join("i18n-probe-done"), "done").unwrap();
}

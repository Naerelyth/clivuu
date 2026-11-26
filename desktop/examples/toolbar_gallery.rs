//! Test-only gallery for pixel comparison against the pre-migration GJS UI.
use clivuu_core::annotation::{Bounds, StylePatch};
use clivuu_desktop::{
    editor::{
        Action, EditorController, ImageMapping, Tool,
        scene::{EditorDocument, Scene},
    },
    ui::{popup::PopupLayer, subbar::Subbar, toolbar::Toolbar, viewport::Viewport},
};
use gtk::{gdk, glib, graphene, prelude::*};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    time::Duration,
};
#[derive(Clone, serde::Deserialize)]
struct Variant {
    id: String,
    tool: Tool,
    #[serde(default)]
    style: StylePatch,
    #[serde(default)]
    accent: Option<String>,
    #[serde(default)]
    popup: Option<String>,
    #[serde(default)]
    hover: Option<String>,
    #[serde(default)]
    pressed: Option<String>,
    #[serde(default)]
    history: Option<String>,
}
struct Case {
    dark: bool,
    pin: bool,
    tool: Tool,
    name: String,
    style: Option<StylePatch>,
    accent: String,
    popup: Option<String>,
    hover: Option<String>,
    pressed: Option<String>,
    history: Option<String>,
}
struct Fixture {
    scene: Scene,
    toolbar: Toolbar,
    options: Subbar,
    popups: PopupLayer,
    panel: RefCell<Option<gtk::Widget>>,
}
fn control(parent: &gtk::Widget, label: &str) -> Option<gtk::Widget> {
    if parent.tooltip_text().as_deref() == Some(clivuu_core::i18n::t(label).as_str()) {
        return Some(parent.clone());
    }
    let mut child = parent.first_child();
    while let Some(w) = child {
        if let Some(found) = control(&w, label) {
            return Some(found);
        }
        child = w.next_sibling();
    }
    None
}
fn controls(parent: &gtk::Widget, data: &mut Vec<serde_json::Value>) {
    if let Some(spin) = parent.downcast_ref::<gtk::SpinButton>() {
        data.push(serde_json::json!({"value":spin.value(),"text":spin.text().to_string(),"width":spin.width(),"digits":spin.digits(),"tip":spin.tooltip_text().map(|s|s.to_string())}));
    }
    let mut child = parent.first_child();
    while let Some(w) = child {
        controls(&w, data);
        child = w.next_sibling();
    }
}
fn snapshot(
    window: &gtk::Window,
    widget: &impl IsA<gtk::Widget>,
    path: PathBuf,
) -> serde_json::Value {
    let widget = widget.as_ref();
    let paintable = gtk::WidgetPaintable::new(Some(widget));
    let (w, h) = (paintable.intrinsic_width(), paintable.intrinsic_height());
    let scale = widget.scale_factor();
    let snapshot = gtk::Snapshot::new();
    snapshot.scale(scale as f32, scale as f32);
    paintable.snapshot(&snapshot, w as f64, h as f64);
    let node = snapshot.to_node().unwrap();
    let texture = window.renderer().unwrap().render_texture(
        &node,
        Some(&graphene::Rect::new(
            0.,
            0.,
            (w * scale) as f32,
            (h * scale) as f32,
        )),
    );
    texture.save_to_png(path).unwrap();
    serde_json::json!({"width":w*scale,"height":h*scale})
}
fn main() {
    adw::init().unwrap();
    gtk::Settings::default()
        .unwrap()
        .set_gtk_cursor_blink(false);
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(false);
    let directory = PathBuf::from(std::env::var("CLIVUU_TEST_DIR").unwrap());
    let loop_ = glib::MainLoop::new(None, false);
    let window = gtk::Window::builder()
        .title("Clivuu native toolbar gallery")
        .decorated(false)
        .default_width(1280)
        .default_height(800)
        .build();
    let root = Viewport::new();
    window.add_css_class("clivuu-gallery-surface");
    let background = gtk::CssProvider::new();
    background.load_from_string(".clivuu-gallery-surface {background:#181818;}");
    gtk::style_context_add_provider_for_display(
        &gtk::prelude::WidgetExt::display(&window),
        &background,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 20,
    );
    window.set_child(Some(&root));
    window.present();
    window.maximize();
    let mut variants: Vec<Variant> = if std::env::var_os("CLIVUU_GALLERY_VARIANTS").is_some() {
        serde_json::from_str(include_str!("../../tests/toolbar-cases.json")).unwrap()
    } else {
        vec![]
    };
    if std::env::var_os("CLIVUU_GALLERY_PANELS").is_some() {
        variants.extend(
            serde_json::from_str::<Vec<Variant>>(include_str!(
                "../../tests/toolbar-panel-cases.json"
            ))
            .unwrap(),
        );
    }
    let mut cases = vec![];
    for dark in [false, true] {
        for pin in [false, true] {
            let prefix = format!(
                "{}-{}",
                if dark { "dark" } else { "light" },
                if pin { "pin" } else { "capture" }
            );
            for tool in [
                Tool::Select,
                Tool::Rect,
                Tool::Ellipse,
                Tool::Arrow,
                Tool::Pen,
                Tool::Highlight,
                Tool::Mosaic,
                Tool::Text,
                Tool::Eraser,
            ] {
                cases.push(Case {
                    dark,
                    pin,
                    tool,
                    name: format!("{prefix}-{tool:?}").to_lowercase(),
                    style: None,
                    accent: "#bd93f9".into(),
                    popup: None,
                    hover: None,
                    pressed: None,
                    history: None,
                });
            }
            if dark == pin {
                for variant in &variants {
                    cases.push(Case {
                        dark,
                        pin,
                        tool: variant.tool,
                        name: format!("{prefix}-{:?}-{}", variant.tool, variant.id).to_lowercase(),
                        style: Some(variant.style.clone()),
                        accent: variant.accent.clone().unwrap_or("#bd93f9".into()),
                        popup: variant.popup.clone(),
                        hover: variant.hover.clone(),
                        pressed: variant.pressed.clone(),
                        history: variant.history.clone(),
                    });
                }
            }
        }
    }
    if let Ok(filter) = std::env::var("CLIVUU_GALLERY_FILTER") {
        cases.retain(|case| filter.split(',').any(|name| name == case.name));
    }
    let index = Rc::new(Cell::new(0));
    let fixture = Rc::new(RefCell::new(None::<Fixture>));
    let provider = Rc::new(RefCell::new(None::<clivuu_desktop::ui::toolbar::Theme>));
    let records = Rc::new(RefCell::new(vec![]));
    let phase = Rc::new(Cell::new(0));
    let stop = loop_.clone();
    let gallery_window = window.clone();
    glib::timeout_add_local(Duration::from_millis(400), move || {
        let window = &gallery_window;
        let i = index.get();
        if i == cases.len() {
            std::fs::write(
                directory.join("native-toolbar-gallery.json"),
                serde_json::to_vec(&*records.borrow()).unwrap(),
            )
            .unwrap();
            stop.quit();
            return glib::ControlFlow::Break;
        }
        let case = &cases[i];
        let (dark, pin, tool) = (case.dark, case.pin, case.tool);
        let name = &case.name;
        if phase.get() == 1
            && (case.popup.is_some() || case.hover.is_some() || case.pressed.is_some())
        {
            let fixture = fixture.borrow();
            let f = fixture.as_ref().unwrap();
            if let Some(label) = &case.popup {
                let button = control(f.options.row().upcast_ref(), label).unwrap();
                button
                    .downcast::<gtk::ToggleButton>()
                    .unwrap()
                    .emit_clicked();
                f.panel.replace(root.last_child());
            }
            for (target, flag) in [
                (&case.hover, gtk::StateFlags::PRELIGHT),
                (&case.pressed, gtk::StateFlags::ACTIVE),
            ] {
                if let Some(target) = target {
                    let (part, label) = target.split_once(':').unwrap();
                    let row = if part == "toolbar" {
                        f.toolbar.row()
                    } else {
                        f.options.row()
                    };
                    control(row.upcast_ref(), label)
                        .unwrap()
                        .set_state_flags(flag, false);
                }
            }
            phase.set(2);
        } else if phase.get() > 0 {
            let fixture = fixture.borrow();
            let fixture = fixture.as_ref().unwrap();
            let panel = fixture.panel.borrow();
            let content = panel
                .as_ref()
                .and_then(|w| w.downcast_ref::<gtk::Frame>())
                .and_then(|f| f.child())
                .map(|c| {
                    snapshot(
                        window,
                        &c,
                        directory.join(format!("native-{name}-panel-content.png")),
                    )
                });
            if let Some(panel) = panel.as_ref() {
                let mut details = vec![];
                controls(panel, &mut details);
                std::fs::write(
                    directory.join(format!("native-{name}-controls.json")),
                    serde_json::to_vec(&details).unwrap(),
                )
                .unwrap();
            }
            let native_panel = panel
                .as_ref()
                .and_then(|w| w.downcast_ref::<gtk::Frame>())
                .and_then(|f| f.compute_bounds(root.upcast_ref::<gtk::Widget>()).map(|b| (f, b)))
                .map(|(f, b)| {
                    let scale = f.scale_factor();
                    let (width, height) = (b.width().ceil() as i32 * scale, b.height().ceil() as i32 * scale);
                    (
                        serde_json::json!({"width":width,"height":height}),
                        serde_json::json!({"id":format!("native-{name}-panel"),"title":"Clivuu native toolbar gallery","x":b.x().round() as i32 * scale,"y":b.y().round() as i32 * scale,"width":width,"height":height}),
                    )
                });
            if let Some((_, request)) = &native_panel {
                if phase.get() != 3 {
                    std::fs::write(
                        directory.join("gallery-capture-request.json"),
                        request.to_string(),
                    )
                    .unwrap();
                    phase.set(3);
                    return glib::ControlFlow::Continue;
                }
                if !directory.join(format!("native-{name}-panel-done")).exists() {
                    return glib::ControlFlow::Continue;
                }
            }
            if let Some(history) = &case.history {
                assert_eq!(
                    fixture.toolbar.undo_button().is_sensitive(),
                    history != "redo",
                    "Undo fixture {name}"
                );
                assert_eq!(
                    fixture.toolbar.redo_button().is_sensitive(),
                    history == "redo" || history == "both",
                    "Redo fixture {name}"
                );
                if history == "selected" {
                    assert_eq!(
                        fixture
                            .scene
                            .controller()
                            .borrow()
                            .session()
                            .current()
                            .unwrap()
                            .kind,
                        clivuu_core::annotation::Kind::Rect
                    );
                }
            }
            let toolbar = snapshot(
                window,
                fixture.toolbar.widget(),
                directory.join(format!("native-{name}-toolbar.png")),
            );
            let options = if fixture.options.widget().is_visible() {
                Some(snapshot(
                    window,
                    fixture.options.widget(),
                    directory.join(format!("native-{name}-options.png")),
                ))
            } else {
                None
            };
            let panel = native_panel.map(|p| p.0).or_else(|| {
                panel.as_ref().map(|p| {
                    snapshot(
                        window,
                        p,
                        directory.join(format!("native-{name}-panel.png")),
                    )
                })
            });
            records.borrow_mut().push(
                serde_json::json!({"case":name,"toolbar":toolbar,"options":options,"panel":panel,"panel-content":content}),
            );
            phase.set(0);
            index.set(i + 1);
        } else {
            if let Some(old) = fixture.borrow_mut().take() {
                old.options.close_popups();
                old.popups.close();
                root.remove(old.toolbar.widget());
                root.remove(old.options.widget());
                root.remove(&old.scene);
            }
            provider.borrow_mut().take();
            adw::StyleManager::default().set_color_scheme(if dark {
                adw::ColorScheme::ForceDark
            } else {
                adw::ColorScheme::ForceLight
            });
            provider.replace(Some(Toolbar::install_theme(
                &gtk::prelude::WidgetExt::display(window),
                dark,
                &case.accent,
            )));
            let mut controller = EditorController::new(ImageMapping {
                display: Bounds {
                    x: 0.,
                    y: 0.,
                    width: 640.,
                    height: 400.,
                },
                document: [640., 400.],
            });
            if let Some(history) = &case.history {
                controller.dispatch(Action::Tool(Tool::Rect));
                for x in if history == "both" {
                    vec![300., 400.]
                } else {
                    vec![300.]
                } {
                    controller.dispatch(Action::Begin {
                        point: [x, 300.],
                        shift: false,
                    });
                    controller.dispatch(Action::Move {
                        point: [x + 40., 340.],
                        shift: false,
                    });
                    controller.dispatch(Action::End);
                }
                if history == "redo" || history == "both" {
                    controller.dispatch(Action::Undo);
                }
            }
            controller.dispatch(Action::Tool(tool));
            if case.history.as_deref() == Some("selected") {
                controller.dispatch(Action::Undo);
                controller.dispatch(Action::Redo);
            }
            if let Some(patch) = &case.style {
                controller.dispatch(Action::Style(patch.clone()));
            }
            let pixels = glib::Bytes::from_owned(vec![255; 4]);
            let texture =
                gdk::MemoryTexture::new(1, 1, gdk::MemoryFormat::R8g8b8a8, &pixels, 4).upcast();
            let scene = Scene::new(EditorDocument::new(controller), &texture);
            scene.set_visible(false);
            root.put(&scene, 0., 0.);
            let toolbar = Toolbar::new(&scene, pin, |_| {});
            let popups = PopupLayer::new(&root);
            let options = Subbar::new(
                &scene,
                &popups,
                clivuu_desktop::application::editor_services::color_services(),
            );
            if dark {
                toolbar.widget().add_css_class("clivuu-theme-dark");
                options.widget().add_css_class("clivuu-theme-dark");
            }
            root.put(toolbar.widget(), 100., 100.);
            root.put(options.widget(), 100., 150.);
            fixture.replace(Some(Fixture {
                scene,
                toolbar,
                options,
                popups,
                panel: RefCell::new(None),
            }));
            phase.set(1);
        }
        glib::ControlFlow::Continue
    });
    loop_.run();
    window.destroy();
}

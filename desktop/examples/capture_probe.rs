//! Ordinary private fixture window, not the product's platform surface backend.
use clivuu_core::{
    annotation::Bounds,
    capture::{Output, Selection},
    image::{Rgba, RgbaImage},
};
use clivuu_desktop::{
    editor::{
        capture_view::{CaptureDocument, CaptureView},
        frozen_image::FrozenImage,
    },
    infrastructure::tasks::TaskPool,
    ui::{toolbar::Toolbar, viewport::Viewport},
};
use gtk::{glib, prelude::*};
use serde_json::json;
use std::{cell::Cell, path::PathBuf, rc::Rc, time::Duration};
fn controls(widget: &gtk::Widget, root: &gtk::Widget) -> Vec<serde_json::Value> {
    let mut out = vec![];
    if let (Some(label), Some(r)) = (widget.tooltip_text(), widget.compute_bounds(root)) {
        out.push(json!({"label":label.as_str(),"x":r.x(),"y":r.y(),"width":r.width(),"height":r.height(),"sensitive":widget.is_sensitive()}));
    }
    let mut child = widget.first_child();
    while let Some(w) = child {
        child = w.next_sibling();
        out.extend(controls(&w, root));
    }
    out
}
fn labels(widget: &gtk::Widget) -> Vec<String> {
    let mut texts = vec![];
    if let Some(label) = widget.downcast_ref::<gtk::Label>() {
        texts.push(label.text().to_string());
    }
    let mut child = widget.first_child();
    while let Some(w) = child {
        child = w.next_sibling();
        texts.extend(labels(&w));
    }
    texts
}
fn main() {
    adw::init().unwrap();
    let directory = PathBuf::from(std::env::var("CLIVUU_TEST_DIR").unwrap());
    let loop_ = glib::MainLoop::new(None, false);
    let window = gtk::Window::builder()
        .title("Clivuu Rust capture probe")
        .decorated(false)
        .default_width(1280)
        .default_height(800)
        .build();
    let root = Viewport::new();
    window.set_child(Some(&root));
    let image = FrozenImage::new(RgbaImage::from_fn(1280, 800, |x, y| {
        Rgba([(x / 5) as u8, (y / 4) as u8, 72, 255])
    }))
    .unwrap();
    let viewport = Bounds {
        x: 0.,
        y: 0.,
        width: 1280.,
        height: 800.,
    };
    let selection = Selection::new(
        vec![Output {
            rect: viewport,
            scale: 1.,
        }],
        vec![Bounds {
            x: 100.,
            y: 100.,
            width: 600.,
            height: 400.,
        }],
        None,
    )
    .unwrap();
    let stop = loop_.clone();
    let copies = Rc::new(Cell::new(0));
    let output_copies = copies.clone();
    let document = CaptureDocument::new(
        image,
        selection,
        &root.pango_context(),
        Rc::new(TaskPool::new(1, 2).unwrap()),
        move |command| {
            if matches!(command, clivuu_desktop::ui::toolbar::Output::Copy) {
                output_copies.set(output_copies.get() + 1);
            }
            if matches!(command, clivuu_desktop::ui::toolbar::Output::Close) {
                stop.quit();
            }
        },
    );
    let view = CaptureView::new(document.clone(), &root, viewport, Default::default());
    let _theme =
        Toolbar::install_theme(&gtk::prelude::WidgetExt::display(&window), true, "#bd93f9");
    document.set_theme(&_theme);
    window.present();
    window.maximize();
    let stop = loop_.clone();
    let mut ready = false;
    glib::timeout_add_local(Duration::from_millis(100), move || {
        let selection = document.selection();
        let model = document.editor.borrow();
        let button = |b: &gtk::Button| {
            let r = b.compute_bounds(&root).unwrap();
            json!({"x":r.x(),"y":r.y(),"width":r.width(),"height":r.height(),"sensitive":b.is_sensitive()})
        };
        let feedback = document.feedback_style();
        let tips = std::iter::successors(root.first_child(), |w| w.next_sibling())
            .find(|w| w.widget_name() == "capture-key-tips")
            .unwrap();
        let bounds = tips
            .compute_bounds(&root)
            .unwrap_or(gtk::graphene::Rect::zero());
        let tips_state = json!({"visible":tips.is_visible(),"labels":labels(&tips),"rect":{"x":bounds.x(),"y":bounds.y(),"width":bounds.width(),"height":bounds.height()}});
        let opt_widget = view.editor().options().widget();
        let opt_bounds = opt_widget
            .compute_bounds(&root)
            .unwrap_or(gtk::graphene::Rect::zero());
        let mut all_fit = opt_bounds.width() > 0.;
        let mut row_child = view.editor().options().row().first_child();
        while let Some(w) = row_child {
            if w.is_visible() {
                if let Some(b) = w.compute_bounds(&root) {
                    all_fit &= b.x() >= opt_bounds.x() - 0.5
                        && b.x() + b.width() <= opt_bounds.x() + opt_bounds.width() + 0.5;
                } else {
                    all_fit = false;
                }
            }
            row_child = w.next_sibling();
        }
        let hadj = opt_widget.hadjustment();
        let opt_controls = controls(opt_widget.upcast_ref(), root.upcast_ref());
        let options_state = json!({
            "visible": opt_widget.is_visible(),
            "bounds": [opt_bounds.x(), opt_bounds.y(), opt_bounds.width(), opt_bounds.height()],
            "allFit": all_fit && !opt_controls.is_empty(),
            "hscrollPolicy": format!("{:?}", opt_widget.hscrollbar_policy()),
            "overflow": hadj.upper() > hadj.page_size() + 0.5,
            "controls": opt_controls,
        });
        let state = json!({"keyTips":tips_state,"guides":feedback.guides,"crosshair":feedback.crosshair,"tools":controls(view.editor().toolbar().row().upcast_ref(),root.upcast_ref()),"options":options_state,"cursor":view.editor().scene().cursor().and_then(|c|c.name()).map(|s|s.to_string()),"rect":selection.rect,"selected":selection.selected,"drag":selection.drag,"lens":view.lens_visible(),"pointer":selection.pointer,"pixel":selection.lens_pixel(document.image.borrow().dimensions()),"tool":model.tool(),"marks":model.session().marks(),"current":model.session().current(),"copies":copies.get(),"undo":button(view.editor().toolbar().undo_button()),"redo":button(view.editor().toolbar().redo_button()),"toolbar":view.editor().toolbar().widget().is_visible(),"hint":view.editor().hint().text().to_string(),"hintVisible":view.editor().hint().is_visible(),"panels":view.editor().panels()});
        let temporary = directory.join("rust-capture-state.tmp");
        std::fs::write(&temporary, state.to_string()).unwrap();
        std::fs::rename(temporary, directory.join("rust-capture-state.json")).unwrap();
        if !ready {
            std::fs::write(directory.join("rust-capture-ready"), "ready").unwrap();
            ready = true;
        }
        if directory.join("rust-capture-stop").exists() {
            stop.quit();
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
    loop_.run();
    window.destroy();
}

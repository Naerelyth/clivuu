//! Private migration fixture. This ordinary window is test-only; production
//! editor widgets are embedded by platform surface adapters.
#[path = "support/arrows.rs"]
mod arrows;
#[path = "support/capture.rs"]
mod capture;
#[path = "support/color_panel.rs"]
mod color_panel;
#[path = "support/mosaics.rs"]
mod mosaics;
#[path = "support/sequence.rs"]
mod sequence;
use clivuu_core::annotation::{Bounds, EditorSession, Fill, Kind, Mark, StylePatch};
use clivuu_desktop::editor::{
    Action, EditorController, ImageMapping, Tool,
    render::{AnnotationNodes, MosaicTextures, text_metrics},
    view::EditorView,
};
use clivuu_desktop::ui::toolbar::Toolbar;
use gtk::{gdk, glib, graphene, prelude::*};
use serde_json::json;
use std::{cell::RefCell, path::PathBuf, rc::Rc, sync::Arc, time::Duration};
fn texture(width: i32, height: i32) -> gdk::Texture {
    gdk::MemoryTexture::new(
        width,
        height,
        gdk::MemoryFormat::R8g8b8a8,
        &glib::Bytes::from_owned(vec![255u8; width as usize * height as usize * 4]),
        width as usize * 4,
    )
    .upcast()
}
fn shape(kind: Kind, a: [f64; 2], b: [f64; 2]) -> Mark {
    let mut m = Mark::new(kind, a, 1.);
    m.points = Arc::new(vec![a, b]);
    m.style.fill = Fill::Rect;
    m
}
fn render_checks(window: &gtk::Window) -> Vec<serde_json::Value> {
    let mut results = arrows::check(window);
    results.extend(sequence::check(window));
    let mut nodes = AnnotationNodes::default();
    let context = window.pango_context();
    let mut rect = shape(Kind::Rect, [3., 3.], [29., 29.]);
    rect.style.fill = Fill::Solid;
    rect.style.color = "#ff0000".into();
    let erase = shape(Kind::Eraser, [10., 10.], [22., 22.]);
    let base = nodes
        .compose(&[rect.clone()], &context, &MosaicTextures::new())
        .unwrap();
    let reused = nodes
        .compose(&[rect.clone()], &context, &MosaicTextures::new())
        .unwrap();
    results.push(json!({"name":"unchanged annotations reuse the exact native render node","passed":base.as_ptr()==reused.as_ptr()}));
    let rendered_pixel = |node: &gtk::gsk::RenderNode, x: usize, y: usize| {
        let s = gtk::Snapshot::new();
        let r = graphene::Rect::new(0., 0., 32., 32.);
        s.append_texture(&texture(32, 32), &r);
        s.append_node(node);
        let frame = window
            .renderer()
            .unwrap()
            .render_texture(s.to_node().unwrap(), Some(&r));
        let mut pixels = vec![0; 32 * 32 * 4];
        frame.download(&mut pixels, 32 * 4);
        u32::from_ne_bytes(
            pixels[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4]
                .try_into()
                .unwrap(),
        )
    };
    let mut mosaic = shape(Kind::Mosaic, [8., 8.], [24., 24.]);
    let draft = gtk::Snapshot::new();
    clivuu_desktop::editor::decorations::draft(&draft, &mosaic, 1.);
    let draft = draft.to_node().unwrap();
    results.push(json!({"name":"rectangular mosaic drag has neutral dashed feedback while leaving its interior untouched","passed":(8..24).any(|x|rendered_pixel(&draft,x,8)!=0xffffffff)&&rendered_pixel(&draft,16,16)==0xffffffff}));
    mosaic.style.fill = Fill::Stroke;
    let draft = gtk::Snapshot::new();
    clivuu_desktop::editor::decorations::draft(&draft, &mosaic, 1.);
    results.push(json!({"name":"mosaic brush preview never acquires a rectangular selection border","passed":draft.to_node().is_none()}));
    for crosshair in [false, true] {
        let cursor = clivuu_desktop::editor::cursor::ToolCursor {
            radius: 3.,
            style: clivuu_desktop::editor::cursor::CursorStyle::Ink,
            color: gdk::RGBA::RED,
            crosshair,
        };
        let snapshot = gtk::Snapshot::new();
        cursor.snapshot(&snapshot, [16., 16.], 1.);
        let node = snapshot.to_node().unwrap();
        results.push(json!({"name":format!("native {} cursor is centered, colored and has the expected auxiliary rays",if crosshair {"arrow"} else {"brush"}),"passed":rendered_pixel(&node,16,16)==0xffff0000&&(rendered_pixel(&node,22,16)!=0xffffffff)==crosshair}));
        let snapshot = gtk::Snapshot::new();
        snapshot.append_texture(&texture(32, 32), &graphene::Rect::new(0., 0., 32., 32.));
        snapshot.append_node(&node);
        let frame = window.renderer().unwrap().render_texture(
            snapshot.to_node().unwrap(),
            Some(&graphene::Rect::new(0., 0., 32., 32.)),
        );
        let mut bytes = vec![0; 32 * 32 * 4];
        frame.download(&mut bytes, 32 * 4);
        results.push(json!({"name":format!("{} cursor dot and rays have no black/white outline", if crosshair {"arrow"} else {"brush"}),
            "passed":bytes.chunks_exact(4).all(|p| { let p = u32::from_ne_bytes(p.try_into().unwrap());
                (p>>16)&255==255 && (p>>8)&255==p&255 }) && rendered_pixel(&node,20,16)==0xffffffff}));
    }
    for (style, name) in [
        (
            clivuu_desktop::editor::cursor::CursorStyle::Mosaic,
            "mosaic",
        ),
        (
            clivuu_desktop::editor::cursor::CursorStyle::Eraser,
            "eraser",
        ),
    ] {
        let cursor = clivuu_desktop::editor::cursor::ToolCursor {
            radius: 7.,
            color: gdk::RGBA::WHITE,
            crosshair: false,
            style,
        };
        let snapshot = gtk::Snapshot::new();
        cursor.snapshot(&snapshot, [16., 16.], 1.);
        let node = snapshot.to_node().unwrap();
        let frame = window
            .renderer()
            .unwrap()
            .render_texture(&node, Some(&graphene::Rect::new(0., 0., 32., 32.)));
        let mut bytes = vec![0; 32 * 32 * 4];
        frame.download(&mut bytes, 32 * 4);
        let pixel = |x: usize, y: usize| {
            let i = (y * 32 + x) * 4;
            [bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]
        };
        let center = pixel(16, 16);
        let correct = if name == "mosaic" {
            center == [160, 160, 160, 160]
                && (8..12).any(|x| {
                    let p = pixel(x, 16);
                    // Subpixel coverage at this scanline antialiases the
                    // 1.4px rim; its interior remains the exact alpha-160 fill.
                    p[3] > 160 && p[0] < 100
                })
        } else {
            center == [0, 0, 0, 0]
                && (8..12).any(|x| {
                    let p = pixel(x, 16);
                    p[3] > 100 && p[0] < 60
                })
                && (9..13).any(|x| {
                    let p = pixel(x, 16);
                    p[0] > 100 && p[0] == p[1] && p[1] == p[2]
                })
        };
        results.push(json!({"name":format!("{name} brush uses the reverse-verified Snipaste fill and rim instead of an opaque white disk"),"passed":correct,"center":center}));
    }
    for scale in [0.5, 1., 2.] {
        let mut editor = EditorController::new(ImageMapping {
            display: Bounds {
                x: 0.,
                y: 0.,
                width: 100. * scale,
                height: 100. * scale,
            },
            document: [100., 100.],
        });
        for tool in [
            Tool::Pen,
            Tool::Highlight,
            Tool::Mosaic,
            Tool::Eraser,
            Tool::Arrow,
        ] {
            editor.dispatch(Action::Tool(Tool::Rect));
            editor.dispatch(Action::Begin {
                point: [10. * scale, 10. * scale],
                shift: false,
            });
            editor.dispatch(Action::Move {
                point: [20. * scale, 20. * scale],
                shift: false,
            });
            editor.dispatch(Action::End);
            editor.dispatch(Action::Tool(tool));
            editor.dispatch(Action::Style(StylePatch {
                width: Some(12. / scale),
                color: Some("#0000ff".into()),
                ..Default::default()
            }));
            editor.dispatch(Action::Begin {
                point: [30. * scale, 30. * scale],
                shift: false,
            });
            editor.dispatch(Action::Move {
                point: [40. * scale, 40. * scale],
                shift: false,
            });
            editor.dispatch(Action::End);
            if tool != Tool::Arrow {
                let ink = editor.session().marks().last().unwrap().clone();
                editor.dispatch(Action::Style(StylePatch {
                    width: Some(14. / scale),
                    ..Default::default()
                }));
                let cursor = clivuu_desktop::editor::cursor::ToolCursor::for_editor(
                    &editor,
                    [80. * scale, 80. * scale],
                )
                .unwrap();
                results.push(json!({"name":format!("{tool:?} committed width stays fixed while controls and dot reflect the next stroke at scale {scale}"),"passed":editor.session().marks().last()==Some(&ink)&&editor.appearance().unwrap().snapshot().style.width==14./scale&&cursor.radius==7./scale}));
                editor.dispatch(Action::Style(StylePatch {
                    width: Some(12. / scale),
                    ..Default::default()
                }));
            }
            editor.dispatch(Action::Undo);
            let cursor = clivuu_desktop::editor::cursor::ToolCursor::for_editor(
                &editor,
                [80. * scale, 80. * scale],
            )
            .unwrap();
            results.push(json!({"name":format!("{tool:?} dot uses its own width and color after undo selects another kind at scale {scale}"),"passed":cursor.radius==6./scale&&cursor.crosshair==(tool==Tool::Arrow)&&cursor.color==if matches!(tool,Tool::Mosaic|Tool::Eraser){gdk::RGBA::WHITE}else{gdk::RGBA::BLUE},"radius":cursor.radius,"expectedRadius":6./scale,"color":cursor.color.to_string(),"crosshair":cursor.crosshair}));
        }
    }
    let mut ellipse = rect.clone();
    ellipse.kind = Kind::Ellipse;
    let round = nodes
        .compose(&[ellipse], &context, &MosaicTextures::new())
        .unwrap();
    let square = nodes
        .compose(&[rect.clone()], &context, &MosaicTextures::new())
        .unwrap();
    results.push(json!({"name":"changing cached shape kind updates actual pixels in both directions","passed":rendered_pixel(&round,5,5)==0xffffffff&&rendered_pixel(&square,5,5)==0xffff0000}));
    let mut second = rect.clone();
    second.points = Arc::new(vec![[20., 20.], [28., 28.]]);
    let pair = [rect.clone(), second.clone()];
    let combined = nodes
        .compose(&pair, &context, &MosaicTextures::new())
        .unwrap();
    let reused = nodes
        .compose(&pair, &context, &MosaicTextures::new())
        .unwrap();
    nodes.invalidate_mosaics();
    let after_effect = nodes
        .compose(&pair, &context, &MosaicTextures::new())
        .unwrap();
    results.push(json!({"name":"whole multi-element composition is retained across unchanged frames and unrelated effect updates","passed":combined.as_ptr()==reused.as_ptr()&&combined.as_ptr()==after_effect.as_ptr()}));
    second.style.color = "#0000ff".into();
    let edited = nodes
        .compose(&[rect.clone(), second], &context, &MosaicTextures::new())
        .unwrap();
    results.push(json!({"name":"editing the last cached element preserves earlier pixels and updates only its appearance","passed":rendered_pixel(&edited,5,5)==0xffff0000&&rendered_pixel(&edited,24,24)==0xff0000ff}));
    let mut dot = Mark::new(Kind::Eraser, [16., 16.], 1.);
    dot.style.width = 10.;
    let erased = nodes
        .compose(&[rect.clone(), dot], &context, &MosaicTextures::new())
        .unwrap();
    results.push(json!({"name":"single-click eraser removes a circular area without erasing original image","passed":rendered_pixel(&erased,16,16)==0xffffffff&&rendered_pixel(&erased,5,5)==0xffff0000}));
    let foreground = nodes
        .compose(&[rect, erase], &context, &MosaicTextures::new())
        .unwrap();
    let snapshot = gtk::Snapshot::new();
    let bounds = graphene::Rect::new(0., 0., 32., 32.);
    snapshot.append_texture(&texture(32, 32), &bounds);
    snapshot.append_node(foreground);
    let frame = window
        .renderer()
        .unwrap()
        .render_texture(snapshot.to_node().unwrap(), Some(&bounds));
    let mut pixels = vec![0u8; 32 * 32 * 4];
    frame.download(&mut pixels, 32 * 4);
    let sample = |x: usize, y: usize| {
        u32::from_ne_bytes(
            pixels[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4]
                .try_into()
                .unwrap(),
        )
    };
    results.push(json!({"name":"native eraser removes annotations while preserving original pixels","passed":sample(16,16)==0xffffffff&&sample(5,5)==0xffff0000}));
    let context = context.clone();
    let mut e =
        EditorSession::new(1., 1.).with_text_measurer(move |m, u| text_metrics(&context, m, u));
    let mut text = Mark::new(Kind::Text, [0., 0.], 1.);
    text.text = Some("Clivuu 中文".into());
    e.add(text);
    let before = e.current().unwrap().clone();
    e.change(
        Kind::Text,
        &StylePatch {
            font_size: Some(36.),
            ..Default::default()
        },
        None,
    );
    let larger = e.current().unwrap().bounds(1.).width > before.bounds(1.).width * 1.8;
    e.undo();
    results.push(json!({"name":"real Pango metrics update placed text inside one reversible edit","passed":larger&&e.current()==Some(&before)}));
    results
}
fn popup_controls(widget: &gtk::Widget, root: &gtk::Widget, out: &mut Vec<serde_json::Value>) {
    if widget.is_mapped()
        && widget.is::<gtk::Button>()
        && let Some(label) = widget.tooltip_text()
        && let Some(r) = widget.compute_bounds(root)
    {
        out.push(json!({"label":label.to_string(),"x":r.x(),"y":r.y(),"width":r.width(),"height":r.height(),"sensitive":widget.is_sensitive()}));
    }
    let mut child = widget.first_child();
    while let Some(w) = child {
        popup_controls(&w, root, out);
        child = w.next_sibling();
    }
}
fn main() {
    adw::init().unwrap();
    let directory = PathBuf::from(std::env::var("CLIVUU_TEST_DIR").unwrap());
    let loop_ = glib::MainLoop::new(None, false);
    let window = gtk::Window::builder()
        .title("Clivuu Rust editor probe")
        .decorated(false)
        .default_width(1280)
        .default_height(800)
        .build();
    let fixed = clivuu_desktop::ui::viewport::Viewport::new();
    window.set_child(Some(&fixed));
    let context = fixed.pango_context();
    // Model a surface on a negative-origin output without moving this private
    // test window; events remain native-local and the view supplies the origin.
    let origin = [-1280., 400.];
    let mut controller = EditorController::new(ImageMapping {
        display: Bounds {
            x: 100. + origin[0],
            y: 120. + origin[1],
            width: 640.,
            height: 400.,
        },
        document: [640., 400.],
    })
    .with_text_measurer(move |m, u| text_metrics(&context, m, u));
    controller.dispatch(Action::Tool(Tool::Rect));
    let controller = clivuu_desktop::editor::scene::EditorDocument::new(controller);
    let editor_view = EditorView::new(
        &fixed,
        controller.clone(),
        &texture(640, 400),
        Bounds {
            x: origin[0],
            y: origin[1],
            width: 1280.,
            height: 800.,
        },
        origin,
        true,
        clivuu_desktop::application::editor_services::color_services(),
        |_| {},
    );
    let scene = editor_view.scene().clone();
    let inline = editor_view.inline().clone();
    let _theme =
        Toolbar::install_theme(&gtk::prelude::WidgetExt::display(&window), true, "#bd93f9");
    editor_view.set_theme(&_theme);
    let toolbar = editor_view.toolbar().clone();
    let options = editor_view.options().clone();
    // Exercise real allocation, rather than checking the size-request property:
    // a panel larger than a small output must scroll inside its native surface.
    let small = clivuu_desktop::ui::viewport::Viewport::new();
    small.set_size_request(300, 180);
    fixed.put(&small, 950., 100.);
    let anchor = gtk::Button::with_label("Boundary probe");
    small.put(&anchor, 220., 140.);
    let content = gtk::DrawingArea::builder()
        .content_width(600)
        .content_height(400)
        .build();
    let bounded = clivuu_desktop::ui::popup::PopupLayer::new(&small);
    let allocation_result = Rc::new(RefCell::new(vec![]));
    let result = allocation_result.clone();
    let test_root = fixed.clone();
    glib::timeout_add_local_once(Duration::from_millis(300), move || {
        bounded.set_viewport(Bounds {
            x: 30.,
            y: 20.,
            width: 260.,
            height: 120.,
        });
        bounded.show(&anchor, &content, || {});
        glib::timeout_add_local_once(Duration::from_millis(200), move || {
            let b = bounded.bounds().unwrap();
            let scroll = content
                .parent()
                .and_then(|w| w.parent())
                .and_then(|w| w.downcast::<gtk::ScrolledWindow>().ok())
                .unwrap();
            let overflow = scroll.hadjustment().upper() > scroll.hadjustment().page_size()
                && scroll.vadjustment().upper() > scroll.vadjustment().page_size();
            result.borrow_mut().push(json!({"name":"oversized in-surface panel stays within its monitor subviewport and has reachable scrolling content","passed":b.x()>=38.&&b.y()>=28.&&b.x()+b.width()<=282.&&b.y()+b.height()<=132.&&overflow,"bounds":[b.x(),b.y(),b.width(),b.height()]}));
            bounded.close();
            // A closed panel releases reusable contents immediately.
            assert!(content.parent().is_none());
            let controllers = small.observe_controllers().n_items();
            let area = Bounds {
                x: -100.,
                y: 30.,
                width: 300.,
                height: 180.,
            };
            let model = clivuu_desktop::editor::scene::EditorDocument::new(EditorController::new(
                ImageMapping {
                    display: area,
                    document: [300., 180.],
                },
            ));
            let view = EditorView::new(
                &small,
                model,
                &texture(1, 1),
                area,
                [-100., 30.],
                false,
                Default::default(),
                |_| {},
            );
            let before = view.panels().unwrap().hint;
            view.set_selection(
                Bounds {
                    x: -90.,
                    y: 45.,
                    width: 123.,
                    height: 45.,
                },
                false,
            );
            view.layout();
            result.borrow_mut().push(json!({"name":"capture hint updates size and position during selection while toolbar is hidden","passed":view.hint().text()=="123 × 45 px"&&view.hint().is_visible()&&!view.toolbar().widget().is_visible()&&view.panels().unwrap().hint!=before}));
            drop(view);
            result.borrow_mut().push(json!({"name":"destroyed editor view removes every native root input controller","passed":small.observe_controllers().n_items()==controllers}));
            test_root.remove(&small);
        });
    });
    window.present();
    window.maximize();
    let test_window = window.clone();
    let test_directory = directory.clone();
    let test_scene = scene.clone();
    let test_fixed = fixed.clone();
    glib::timeout_add_local_once(Duration::from_millis(800), move || {
        glib::MainContext::default().spawn_local(async move {
        let mut results = render_checks(&test_window);
            results.extend(capture::checks(&test_window));
        results.extend(capture::annotation_crop_checks(&test_window).await);
        results.extend(mosaics::checks(&test_window).await);
        assert_eq!(allocation_result.borrow().len(), 3);
        results.extend(allocation_result.borrow_mut().drain(..));
        let bounds = test_scene
            .compute_bounds(&test_fixed)
            .map(|r| [r.x(), r.y(), r.width(), r.height()]);
        std::fs::write(test_directory.join("rust-editor-geometry.json"),json!({"bounds":bounds,"mapped":test_scene.is_mapped(),"target":test_scene.can_target(),"width":test_scene.width(),"height":test_scene.height(),"sensitive":test_scene.is_sensitive()}).to_string()).unwrap();
        std::fs::write(
            test_directory.join("rust-render-results.json"),
            serde_json::to_vec(&results).unwrap(),
        )
        .unwrap();
        std::fs::write(test_directory.join("rust-editor-ready"), "ready").unwrap();
      });
    });
    let stop_loop = loop_.clone();
    let focus_window = window.downgrade();
    glib::timeout_add_local(Duration::from_millis(100), move || {
        let mut state = serde_json::to_value(controller.borrow().session().state()).unwrap();
        state["drawing"] = serde_json::to_value(controller.borrow().drawing()).unwrap();
        let button = |b: &gtk::Button| {
            let r = b.compute_bounds(&fixed).unwrap();
            json!({"x":r.x(),"y":r.y(),"width":r.width(),"height":r.height(),"sensitive":b.is_sensitive()})
        };
        state["toolbar"] = json!({"undo":button(toolbar.undo_button()),"redo":button(toolbar.redo_button()),"height":toolbar.widget().compute_bounds(&fixed).unwrap().height()});
        let bar = toolbar.widget().compute_bounds(&fixed).unwrap();
        let grip = toolbar.grip().compute_bounds(&fixed).unwrap();
        state["layout"] = json!({"panels":editor_view.panels(),"bar":[bar.x(),bar.y(),bar.width(),bar.height()],"grip":{"x":grip.x(),"y":grip.y(),"width":grip.width(),"height":grip.height()},"controlsVisible":toolbar.widget().is_visible()});
        let mut tools = vec![];
        let mut w = toolbar.row().first_child();
        while let Some(child) = w {
            if let Some(b) = child.compute_bounds(&fixed) {
                tools.push(json!({"label":child.tooltip_text().map(|s|s.to_string()),"x":b.x(),"y":b.y(),"width":b.width(),"height":b.height()}));
            }
            w = child.next_sibling();
        }
        state["tools"] = json!(tools);
        state["focusWidget"] = json!(
            focus_window
                .upgrade()
                .and_then(|w| gtk::prelude::GtkWindowExt::focus(&w))
                .map(|w| w.type_().name().to_string())
        );
        state["inline"] =
            json!({"mapped":inline.view().is_mapped(),"focused":inline.view().has_focus()});
        state["toolCursor"] = json!(
            clivuu_desktop::editor::cursor::ToolCursor::for_editor(
                &controller.borrow(),
                [700., 400.]
            )
            .map(
                |c| json!({"radius":c.radius,"color":c.color.to_string(),"crosshair":c.crosshair})
            )
        );
        state["cursor"] = json!(scene.cursor().and_then(|c| c.name()).map(|s| s.to_string()));
        let model = controller.borrow();
        state["tool"] = json!(model.tool());
        state["sequenceNext"] = json!(model.session().style(Kind::Sequence).style.sequence.index);
        state["handles"] = json!(
            model
                .session()
                .current()
                .map(|m| m.handles(1., model.mapping().screen_scale()))
        );
        drop(model);
        let mut controls = vec![];
        let mut swatches_fit = true;
        let mut all_fit = true;
        let mut swatch_count = 0usize;
        let opt_bounds = options
            .widget()
            .compute_bounds(&fixed)
            .unwrap_or(graphene::Rect::zero());
        let mut child = options.row().first_child();
        while let Some(w) = child {
            if w.is_visible()
                && let Some(b) = w.compute_bounds(&fixed)
            {
                all_fit &= b.x() >= opt_bounds.x() - 0.5
                    && b.x() + b.width() <= opt_bounds.x() + opt_bounds.width() + 0.5;
                controls.push(json!({"label":w.tooltip_text().map(|s|s.to_string()),"x":b.x(),"y":b.y(),"width":b.width(),"height":b.height(),"sensitive":w.is_sensitive(),"value":w.downcast_ref::<gtk::Range>().map(|r|r.value()),"range":w.downcast_ref::<gtk::Range>().map(|r|[r.adjustment().lower(),r.adjustment().upper()])}));
                if let Some(grid) = w.downcast_ref::<gtk::Grid>() {
                    let mut cell = grid.first_child();
                    while let Some(s) = cell {
                        cell = s.next_sibling();
                        if s.has_css_class("clivuu-swatch") {
                            swatch_count += 1;
                            if let Some(sb) = s.compute_bounds(&fixed) {
                                swatches_fit &= sb.x() >= opt_bounds.x() - 0.5
                                    && sb.x() + sb.width()
                                        <= opt_bounds.x() + opt_bounds.width() + 0.5
                                    && sb.y() >= opt_bounds.y() + 4.
                                    && sb.y() + sb.height()
                                        <= opt_bounds.y() + opt_bounds.height() - 4.;
                            } else {
                                swatches_fit = false;
                            }
                        }
                    }
                }
            }
            child = w.next_sibling();
        }
        let mut menu = vec![];
        popup_controls(fixed.upcast_ref(), fixed.upcast_ref(), &mut menu);
        state["optionMenu"] = json!(menu);
        state["colorPanel"] = color_panel::inspect(fixed.upcast_ref()).unwrap_or_default();
        state["toolbarFont"] = json!(
            toolbar
                .row()
                .pango_context()
                .font_description()
                .unwrap()
                .to_string()
        );
        let hadj = options.widget().hadjustment();
        state["options"] = json!({"visible":options.widget().is_visible(),"bounds":[opt_bounds.x(),opt_bounds.y(),opt_bounds.width(),opt_bounds.height()],"controls":controls,"height":opt_bounds.height(),"popup":options.has_popup(),"allFit":all_fit,"swatchesFit":swatch_count==20&&swatches_fit,"swatchCount":swatch_count,"hscrollPolicy":format!("{:?}",options.widget().hscrollbar_policy()),"overflow":hadj.upper()>hadj.page_size()+0.5});
        let state_file = directory.join("rust-editor-state.json");
        let temp = directory.join("rust-editor-state.tmp");
        std::fs::write(&temp, state.to_string()).unwrap();
        std::fs::rename(temp, state_file).unwrap();
        if directory.join("rust-editor-stop").exists() {
            stop_loop.quit();
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
    loop_.run();
    window.destroy();
}

use clivuu_core::{
    annotation::Bounds,
    capture::{Output, Selection},
    image::{Rgba, RgbaImage},
};
use clivuu_desktop::editor::{
    capture_draw::{self, CaptureStyle},
    frozen_image::FrozenImage,
};
use gtk::{graphene, prelude::*};
use serde_json::{Value, json};
pub fn checks(window: &gtk::Window) -> Vec<Value> {
    let mut results = vec![];
    let image = FrozenImage::new(RgbaImage::from_fn(1280, 800, |x, y| {
        Rgba([
            (x * 37 + y * 17) as u8,
            (x * 13 + y * 41) as u8,
            (x * 53 + y * 3) as u8,
            255,
        ])
    }))
    .unwrap();
    let output = Output {
        rect: Bounds {
            x: -100.,
            y: 20.,
            width: 640.,
            height: 400.,
        },
        scale: 2.,
    };
    let mut selection = Selection::new(
        vec![output],
        vec![],
        Some(Bounds {
            x: -30.,
            y: 70.,
            width: 200.,
            height: 100.,
        }),
    )
    .unwrap();
    let style = CaptureStyle::default();
    let pixel = |node: &gtk::gsk::RenderNode, width: usize, height: usize, x: usize, y: usize| {
        let frame = window.renderer().unwrap().render_texture(
            node,
            Some(&graphene::Rect::new(0., 0., width as f32, height as f32)),
        );
        let mut bytes = vec![0; width * height * 4];
        frame.download(&mut bytes, width * 4);
        u32::from_ne_bytes(
            bytes[(y * width + x) * 4..(y * width + x) * 4 + 4]
                .try_into()
                .unwrap(),
        )
    };
    selection.begin([166., 120.], 8., false);
    let mut stable = true;
    let mut samples = vec![];
    for point in [[340., 120.], [539.5, 120.], [127., 120.], [-100., 120.]] {
        selection.motion(point);
        let pos = selection.lens_pixel(image.dimensions()).unwrap();
        let c = image.pixels.get_pixel(pos[0], pos[1]);
        let expected =
            0xff000000 | (u32::from(c[0]) << 16) | (u32::from(c[1]) << 8) | u32::from(c[2]);
        let snapshot = gtk::Snapshot::new();
        capture_draw::lens(
            &snapshot,
            &image,
            &selection,
            &style,
            &window.pango_context(),
            false,
        );
        let node = snapshot.to_node().unwrap();
        let centre = pixel(&node, 184, 236, 92, 72);
        let swatch = pixel(&node, 184, 236, 19, 177);
        samples.push(
            json!({"point":point,"pixel":pos,"expected":expected,"centre":centre,"swatch":swatch}),
        );
        stable &= centre == expected && swatch == expected;
        if centre != expected || swatch != expected {
            let image = window
                .renderer()
                .unwrap()
                .render_texture(&node, Some(&graphene::Rect::new(0., 0., 184., 236.)));
            image
                .save_to_png(
                    std::path::PathBuf::from(std::env::var("CLIVUU_TEST_DIR").unwrap())
                        .join(format!("capture-lens-{}.png", samples.len())),
                )
                .unwrap();
        }
    }
    results.push(json!({"name":"fractional-DPR resize magnifier centre and swatch always show the included boundary pixel","passed":stable,"samples":samples}));
    let white =
        FrozenImage::new(RgbaImage::from_pixel(64, 64, Rgba([255, 255, 255, 255]))).unwrap();
    let output = Output {
        rect: Bounds {
            x: 0.,
            y: 0.,
            width: 64.,
            height: 64.,
        },
        scale: 1.,
    };
    let selection = Selection::new(vec![output], vec![], None).unwrap();
    let snapshot = gtk::Snapshot::new();
    capture_draw::backdrop(&snapshot, &white, &selection, &style, output.rect);
    let node = snapshot.to_node().unwrap();
    results.push(json!({"name":"full-screen capture frame has no inset coloured border in the selected image","passed":pixel(&node,64,64,0,0)==0xffffffff&&pixel(&node,64,64,63,63)==0xffffffff&&pixel(&node,64,64,32,0)==0xffffffff}));
    let mut selection = selection;
    selection.pointer = Some([32., 32.]);
    let mut style = style;
    style.crosshair = true;
    let snapshot = gtk::Snapshot::new();
    capture_draw::backdrop(&snapshot, &white, &selection, &style, output.rect);
    let node = snapshot.to_node().unwrap();
    results.push(json!({"name":"capture crosshair inverts the source once along its arms and twice at its crossing", "passed":pixel(&node,64,64,32,32)==0xffffffff&&pixel(&node,64,64,10,32)==0xff000000&&pixel(&node,64,64,32,10)==0xff000000}));
    selection.selected = true;
    selection.rect = Bounds {
        x: 16.,
        y: 16.,
        width: 32.,
        height: 32.,
    };
    style.crosshair = false;
    style.mask.set_alpha(0.);
    style.border_width = 0.;
    style.anchors = false;
    for guides in [true, false] {
        style.guides = guides;
        let snapshot = gtk::Snapshot::new();
        capture_draw::backdrop(&snapshot, &white, &selection, &style, output.rect);
        let node = snapshot.to_node().unwrap();
        let vertical = [16, 32, 48]
            .iter()
            .all(|x| (pixel(&node, 64, 64, *x, 4) != 0xffffffff) == guides);
        let horizontal = [16, 32, 48]
            .iter()
            .all(|y| (pixel(&node, 64, 64, 4, *y) != 0xffffffff) == guides);
        results.push(json!({"name":format!("selection edge and center guides are independently visible: {guides}"), "passed":vertical&&horizontal&&pixel(&node,64,64,24,24)==0xffffffff}));
    }
    let anchor_style = CaptureStyle {
        border_width: 8.,
        anchors: true,
        accent: gtk::gdk::RGBA::new(1., 0., 0., 1.),
        anchor: gtk::gdk::RGBA::WHITE,
        ..Default::default()
    };
    let snapshot = gtk::Snapshot::new();
    capture_draw::frame(&snapshot, selection.rect, &anchor_style);
    let node = snapshot.to_node().unwrap();
    // Border width 8 spans 8..16 around rect(16,16,32,32), so the outside-border
    // centerline sits at 12 and corner/midpoint anchors reach radius 11.2 there.
    let anchor_centered = pixel(&node, 64, 64, 12, 12) == 0xffff0000
        && pixel(&node, 64, 64, 32, 12) == 0xffff0000
        && pixel(&node, 64, 64, 32, 2) == 0xffff0000
        && pixel(&node, 64, 64, 32, 24) == 0;
    results.push(json!({"name":"selection anchors center on the outside-border centerline like Snipaste", "passed":anchor_centered}));
    results.extend(feedback_controls(window));
    results.extend(selection_controls(window));
    results.extend(monitor_controls(window));
    results
}

// Validate the same input route used by actual capture surfaces without a
// platform capture session: button activation and keys must have one meaning.
fn selection_controls(window: &gtk::Window) -> Vec<Value> {
    use clivuu_core::capture::Purpose;
    use clivuu_desktop::{
        editor::capture_view::{CaptureDocument, CaptureView},
        infrastructure::tasks::TaskPool,
        ui::viewport::Viewport,
    };
    use gtk::gdk;
    use std::{cell::RefCell, rc::Rc};
    let mut results = vec![];
    let viewport = Bounds {
        x: 0.,
        y: 0.,
        width: 800.,
        height: 600.,
    };
    let area = Bounds {
        x: 100.,
        y: 110.,
        width: 500.,
        height: 300.,
    };
    let pool = Rc::new(TaskPool::new(1, 4).unwrap());
    for (purpose, label, expected) in [
        (Purpose::Scrolling, "Start scrolling capture", "Scroll"),
        (Purpose::Recording, "", "Record"),
        (Purpose::Image, "Copy image", "Copy"),
    ] {
        let root = Viewport::new();
        let events = Rc::new(RefCell::new(Vec::new()));
        let emitted = events.clone();
        let document = CaptureDocument::new(
            FrozenImage::new(RgbaImage::from_pixel(800, 600, Rgba([36, 104, 172, 255]))).unwrap(),
            Selection::new(
                vec![Output {
                    rect: viewport,
                    scale: 1.,
                }],
                vec![area],
                Some(area),
            )
            .unwrap(),
            &window.pango_context(),
            pool.clone(),
            move |command| emitted.borrow_mut().push(format!("{command:?}")),
        );
        document.set_purpose(purpose);
        let view = CaptureView::new(document.clone(), &root, viewport, Default::default());
        if purpose == Purpose::Recording {
            let controllers = view.editor().scene().observe_controllers();
            for n in 0..controllers.n_items() {
                if let Some(click) = controllers
                    .item(n)
                    .and_then(|c| c.downcast::<gtk::GestureClick>().ok())
                    .filter(|c| c.button() == 1)
                {
                    click.emit_by_name::<()>("pressed", &[&1i32, &150f64, &150f64]);
                    click.emit_by_name::<()>("released", &[&1i32, &150f64, &150f64]);
                }
            }
            results.push(json!({"name":"recording selection release confirms directly without a capture toolbar", "passed":events.borrow().as_slice()==["Record"]&&!view.editor().toolbar().widget().is_visible()}));
            continue;
        }
        let mut child = view.editor().toolbar().row().first_child();
        let mut visible = vec![];
        while let Some(widget) = child {
            child = widget.next_sibling();
            if widget.is_visible()
                && let Some(button) = widget.downcast_ref::<gtk::Button>()
            {
                if let Some(title) = button.tooltip_text() {
                    visible.push(title.to_string());
                }
                if button.tooltip_text().is_some_and(|tip| {
                    tip.as_str() == label || tip.starts_with(&format!("{label} ("))
                }) {
                    button.emit_clicked();
                }
            }
        }
        view.editor()
            .scene()
            .key_pressed(gdk::Key::Return, gdk::ModifierType::empty());
        results.push(json!({"name":format!("{purpose:?} confirmation button and Enter use one purpose-specific route"),
            "passed":events.borrow().as_slice()==[expected,expected] && if purpose.editable(){visible.len()>8}else{visible==["Move toolbar",&format!("{label} (Enter)"),"Cancel (Esc)"]}}));
        if purpose.editable() {
            use clivuu_desktop::editor::Action;
            view.editor()
                .scene()
                .key_pressed(gdk::Key::_2, gdk::ModifierType::CONTROL_MASK);
            document.editor.dispatch(Action::Begin {
                point: [200., 200.],
                shift: false,
            });
            document.editor.dispatch(Action::Move {
                point: [280., 240.],
                shift: false,
            });
            document.editor.dispatch(Action::End);
            let drawn = document.editor.borrow().session().marks().len() == 1;
            view.editor()
                .scene()
                .key_pressed(gdk::Key::z, gdk::ModifierType::CONTROL_MASK);
            results.push(json!({"name":format!("{purpose:?} shares annotation tool and undo routing"),"passed":drawn&&document.editor.borrow().session().marks().is_empty()}));
            if purpose == Purpose::Image {
                events.borrow_mut().clear();
                for key in [gdk::Key::l, gdk::Key::v] {
                    view.editor()
                        .scene()
                        .key_pressed(key, gdk::ModifierType::empty());
                }
                results.push(json!({"name":"isolated window image hides and rejects continuous capture actions", "passed":events.borrow().is_empty()&&!visible.iter().any(|v|v=="Scroll L"||v=="Record V")}));
            }
        } else {
            events.borrow_mut().clear();
            view.editor()
                .scene()
                .key_pressed(gdk::Key::c, gdk::ModifierType::CONTROL_MASK);
            view.editor()
                .scene()
                .key_pressed(gdk::Key::p, gdk::ModifierType::empty());
            view.editor()
                .scene()
                .key_pressed(gdk::Key::_1, gdk::ModifierType::CONTROL_MASK);
            results.push(json!({"name":format!("{purpose:?} does not reinterpret Copy, Pin or annotation keys as start"),
                "passed":events.borrow().is_empty() && document.editor.borrow().tool()==clivuu_desktop::editor::Tool::Select}));
        }
    }
    results
}

/// Actual export and native composition, with ink initially outside the crop.
pub async fn annotation_crop_checks(window: &gtk::Window) -> Vec<Value> {
    use clivuu_core::annotation::{Fill, StylePatch};
    use clivuu_desktop::{
        editor::{
            Action, Tool,
            capture_view::{CaptureDocument, CaptureView},
            export,
        },
        infrastructure::tasks::TaskPool,
        ui::viewport::Viewport,
    };
    use gtk::{gdk, gio};
    use std::rc::Rc;
    let root = Viewport::new();
    let limit = Bounds {
        x: -100.,
        y: 20.,
        width: 64.,
        height: 64.,
    };
    let area = Bounds {
        x: -68.,
        y: 20.,
        width: 32.,
        height: 64.,
    };
    let image = FrozenImage::new(RgbaImage::from_pixel(128, 128, Rgba([0, 0, 255, 255]))).unwrap();
    let pool = Rc::new(TaskPool::new(1, 2).unwrap());
    let document = CaptureDocument::new(
        image,
        Selection::new(
            vec![Output {
                rect: limit,
                scale: 2.,
            }],
            vec![],
            Some(area),
        )
        .unwrap(),
        &window.pango_context(),
        pool.clone(),
        |_| {},
    );
    let view = CaptureView::new(document.clone(), &root, limit, Default::default());
    document.editor.dispatch(Action::Tool(Tool::Rect));
    document.editor.dispatch(Action::Style(StylePatch {
        color: Some("#ff0000".into()),
        alpha: Some(128),
        fill: Some(Fill::Solid),
        ..Default::default()
    }));
    document.editor.dispatch(Action::Begin {
        point: [-92., 28.],
        shift: false,
    });
    document.editor.dispatch(Action::Move {
        point: [-76., 44.],
        shift: false,
    });
    document.editor.dispatch(Action::End);
    let renderer = window.renderer().unwrap();
    let cancel = gio::Cancellable::new();
    let crop = Bounds {
        x: 64.,
        y: 0.,
        width: 64.,
        height: 128.,
    };
    let before = export::frame(view.editor().scene(), &renderer, Some(crop), &pool, &cancel)
        .await
        .unwrap();
    let full = export::frame(view.editor().scene(), &renderer, None, &pool, &cancel)
        .await
        .unwrap();
    let sized = export::frame_sized(
        view.editor().scene(),
        &renderer,
        Some(crop),
        [32, 64],
        &pool,
        &cancel,
    )
    .await
    .unwrap();
    let reduced_full = export::frame_sized(
        view.editor().scene(),
        &renderer,
        None,
        [64, 64],
        &pool,
        &cancel,
    )
    .await
    .unwrap();
    let mark = document.editor.borrow().session().marks()[0].clone();
    let mut results = vec![
        json!({"name":"native-density export preserves crop origin and annotation coordinates when backing pixels are reduced", "passed":sized.pixels.dimensions()==(32,64)&&sized.pixels.pixels().all(|p|*p==Rgba([0,0,255,255]))&&reduced_full.pixels.dimensions()==(64,64)&&reduced_full.pixels.get_pixel(12,12)[0]>120&&reduced_full.pixels.get_pixel(12,12)[2]<135}),
        json!({"name":"capture export crops external ink only at output, retaining negative-origin 2x document coordinates", "passed":before.pixels.dimensions()==(64,128)&&before.pixels.pixels().all(|p|*p==Rgba([0,0,255,255]))&&mark.points[0]==[16.,16.]&&mark.points[1]==[48.,48.]&&full.pixels.get_pixel(24,24)[0]>120&&full.pixels.get_pixel(24,24)[2]<135}),
    ];
    // Reference: compose annotations once over the base backdrop, then dim only
    // the crop's exterior in CaptureDraw::overlay across integer and fractional scales.
    let viewport = Bounds {
        x: 0.,
        y: 0.,
        width: 64.,
        height: 64.,
    };
    let focus = Bounds {
        x: 32.,
        y: 0.,
        width: 32.,
        height: 64.,
    };
    let download = |s: gtk::Snapshot| {
        let frame = renderer.render_texture(
            s.to_node().unwrap(),
            Some(&graphene::Rect::new(0., 0., 64., 64.)),
        );
        let mut bytes = vec![0; 64 * 64 * 4];
        frame.download(&mut bytes, 64 * 4);
        bytes
    };
    let base_img = FrozenImage::new(RgbaImage::from_pixel(64, 64, Rgba([0, 0, 255, 255]))).unwrap();
    let node = gtk::gsk::ColorNode::new(
        &gdk::RGBA::new(1., 0., 0., 0.5),
        &graphene::Rect::new(8., 8., 48., 48.),
    );
    for mask in [
        gdk::RGBA::new(0., 0., 0., 0.44),
        gdk::RGBA::new(0.1, 0.2, 0.3, 0.6),
    ] {
        let expected = gtk::Snapshot::new();
        expected.append_color(&gdk::RGBA::BLUE, &graphene::Rect::new(0., 0., 64., 64.));
        expected.append_node(&node);
        for band in clivuu_core::capture::outside(focus, viewport) {
            if band.width > 0. && band.height > 0. {
                expected.append_color(
                    &mask,
                    &graphene::Rect::new(
                        band.x as f32,
                        band.y as f32,
                        band.width as f32,
                        band.height as f32,
                    ),
                );
            }
        }
        let mut selection = Selection::new(
            vec![Output {
                rect: viewport,
                scale: 1.,
            }],
            vec![],
            None,
        )
        .unwrap();
        selection.selected = true;
        selection.rect = focus;
        let overlay_style = CaptureStyle {
            border_width: 0.,
            mask,
            anchors: false,
            guides: false,
            crosshair: false,
            ..Default::default()
        };
        let actual = gtk::Snapshot::new();
        capture_draw::backdrop_base(&actual, &base_img, &selection, &overlay_style, viewport);
        actual.append_node(&node);
        capture_draw::overlay(&actual, &selection, &overlay_style, [0., 0.], viewport);
        let expected = download(expected);
        let actual = download(actual);
        let error = expected
            .iter()
            .zip(&actual)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        results.push(json!({"name":format!("outside annotation preview preserves alpha and colored masks without crop seams ({mask})"),"passed":error<=1,"maxError":error}));
    }
    // Verify localized eraser MaskNode scoping + single-pass overlay composition
    // across dragged selection rects (both 1x and fractional 1.5x doc scaling).
    {
        use clivuu_core::annotation::{Fill, Kind, Mark};
        use clivuu_desktop::editor::render::{AnnotationNodes, MosaicTextures};
        use std::sync::Arc;
        let make_rect = |a: [f64; 2], b: [f64; 2], color: &str, fill: Fill| {
            let mut m = Mark::new(Kind::Rect, a, 1.);
            m.points = Arc::new(vec![a, b]);
            m.style.color = color.into();
            m.style.width = 4.;
            m.style.fill = fill;
            m
        };
        let make_freehand_eraser = |a: [f64; 2], b: [f64; 2]| {
            let mut m = Mark::new(Kind::Eraser, a, 1.);
            m.points = Arc::new(vec![a, b]);
            m.style.width = 10.;
            m.style.fill = Fill::Stroke;
            m
        };
        let make_area_eraser = |a: [f64; 2], b: [f64; 2]| {
            let mut m = Mark::new(Kind::Eraser, a, 1.);
            m.points = Arc::new(vec![a, b]);
            m.style.fill = Fill::Rect;
            m
        };
        let m1_stroke = make_rect([6., 6.], [26., 26.], "#ff0000", Fill::Stroke);
        let m2_stroke = make_rect([38., 6.], [58., 26.], "#00ff00", Fill::Stroke);
        let m3_solid = make_rect([38., 38.], [58., 58.], "#ffff00", Fill::Solid);
        let m4_overlap = make_rect([12., 12.], [30., 30.], "#00ffff", Fill::Solid);
        let scenarios: Vec<(&str, Vec<Mark>)> = vec![
            (
                "freehand_eraser_on_stroke_and_solid",
                vec![
                    m1_stroke.clone(),
                    m2_stroke.clone(),
                    m3_solid.clone(),
                    make_freehand_eraser([10., 16.], [22., 16.]),
                ],
            ),
            (
                "area_eraser_on_stroke_and_solid",
                vec![
                    m1_stroke.clone(),
                    m2_stroke.clone(),
                    m3_solid.clone(),
                    make_area_eraser([10., 10.], [20., 20.]),
                ],
            ),
            (
                "multi_layer_overlap_with_disjoint_middle",
                vec![
                    m1_stroke.clone(),
                    m2_stroke.clone(),
                    m4_overlap.clone(),
                    make_freehand_eraser([12., 16.], [24., 16.]),
                ],
            ),
            (
                "two_erasers_nested",
                vec![
                    m1_stroke.clone(),
                    make_freehand_eraser([10., 16.], [22., 16.]),
                    m2_stroke.clone(),
                    m3_solid.clone(),
                    make_area_eraser([12., 12.], [18., 18.]),
                ],
            ),
            (
                "eraser_on_empty_space_avoids_mask",
                vec![
                    m1_stroke.clone(),
                    m2_stroke.clone(),
                    make_freehand_eraser([8., 44.], [20., 54.]),
                ],
            ),
        ];
        let mut nodes = AnnotationNodes::default();
        let mosaics = MosaicTextures::new();
        let context = window.pango_context();
        let mask = gdk::RGBA::new(0., 0., 0., 0.44);
        let overlay_style = CaptureStyle {
            border_width: 0.,
            mask,
            anchors: false,
            guides: false,
            crosshair: false,
            ..Default::default()
        };
        let mut worst = 0u8;
        let mut worst_case = String::new();
        let mut worst_px = [0u8; 8];
        let mut empty_eraser_mask_free = false;
        let mut localized_parity_worst = 0u8;
        let mut preview_parity_worst = 0u8;
        for (sname, marks) in &scenarios {
            let node = nodes.compose(marks, &context, &mosaics).unwrap();
            if *sname == "eraser_on_empty_space_avoids_mask" {
                empty_eraser_mask_free = node.downcast_ref::<gtk::gsk::MaskNode>().is_none();
            }
            if let Some((last, rest)) = marks.split_last() {
                let mut preview_nodes = AnnotationNodes::default();
                let preview = preview_nodes
                    .compose_preview(rest, Some(last), &context, &mosaics)
                    .unwrap();
                let s_loc = gtk::Snapshot::new();
                s_loc.append_color(&gdk::RGBA::BLUE, &graphene::Rect::new(0., 0., 64., 64.));
                s_loc.append_node(&node);
                let s_pre = gtk::Snapshot::new();
                s_pre.append_color(&gdk::RGBA::BLUE, &graphene::Rect::new(0., 0., 64., 64.));
                s_pre.append_node(&preview);
                let diff = download(s_loc)
                    .iter()
                    .zip(&download(s_pre))
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap();
                preview_parity_worst = preview_parity_worst.max(diff);
            }
            // Compare localized composition against naive full-scene MaskNode composition.
            let mut naive: Option<gtk::gsk::RenderNode> = None;
            let mut single = AnnotationNodes::default();
            for mark in marks {
                let proxy = if mark.kind == Kind::Eraser {
                    let mut m = mark.clone();
                    m.style.color = "#000000".into();
                    m.style.alpha = 255;
                    if mark.style.fill == Fill::Rect {
                        m.kind = Kind::Rect;
                        m.style.fill = Fill::Solid;
                    } else {
                        m.kind = Kind::Pen;
                        m.style.fill = Fill::Stroke;
                    }
                    m
                } else {
                    mark.clone()
                };
                let Some(n) =
                    single.compose_preview(std::iter::empty(), Some(&proxy), &context, &mosaics)
                else {
                    continue;
                };
                naive = match naive {
                    None if mark.kind == Kind::Eraser => None,
                    None => Some(n),
                    Some(prev) if mark.kind == Kind::Eraser => Some(
                        gtk::gsk::MaskNode::new(&prev, &n, gtk::gsk::MaskMode::InvertedAlpha)
                            .upcast(),
                    ),
                    Some(prev) => Some(gtk::gsk::ContainerNode::new(&[prev, n]).upcast()),
                };
            }
            if let Some(naive_node) = naive {
                let s_loc = gtk::Snapshot::new();
                s_loc.append_color(&gdk::RGBA::BLUE, &graphene::Rect::new(0., 0., 64., 64.));
                s_loc.append_node(&node);
                let s_ref = gtk::Snapshot::new();
                s_ref.append_color(&gdk::RGBA::BLUE, &graphene::Rect::new(0., 0., 64., 64.));
                s_ref.append_node(&naive_node);
                let diff = download(s_loc)
                    .iter()
                    .zip(&download(s_ref))
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap();
                localized_parity_worst = localized_parity_worst.max(diff);
            }
            for doc_scale in [1.0f32, 1.5f32] {
                for (label, drag_focus) in [
                    (
                        "m2_outside_right",
                        Bounds {
                            x: 4.,
                            y: 4.,
                            width: 28.,
                            height: 28.,
                        },
                    ),
                    (
                        "m2_crossing_right",
                        Bounds {
                            x: 20.,
                            y: 4.,
                            width: 28.,
                            height: 28.,
                        },
                    ),
                    (
                        "m2_inside_eraser_left",
                        Bounds {
                            x: 34.,
                            y: 4.,
                            width: 26.,
                            height: 28.,
                        },
                    ),
                    (
                        "m2_split_top_and_right",
                        Bounds {
                            x: 4.,
                            y: 16.,
                            width: 44.,
                            height: 32.,
                        },
                    ),
                    (
                        "all_outside_selection_below",
                        Bounds {
                            x: 4.,
                            y: 34.,
                            width: 24.,
                            height: 24.,
                        },
                    ),
                ] {
                    let mut selection = Selection::new(
                        vec![Output {
                            rect: viewport,
                            scale: f64::from(doc_scale),
                        }],
                        vec![],
                        None,
                    )
                    .unwrap();
                    selection.selected = true;
                    selection.rect = drag_focus;
                    let expected = gtk::Snapshot::new();
                    expected.append_color(&gdk::RGBA::BLUE, &graphene::Rect::new(0., 0., 64., 64.));
                    expected.save();
                    expected.scale(1. / doc_scale, 1. / doc_scale);
                    expected.append_node(&node);
                    expected.restore();
                    for band in clivuu_core::capture::outside(drag_focus, viewport) {
                        if band.width > 0. && band.height > 0. {
                            expected.append_color(
                                &mask,
                                &graphene::Rect::new(
                                    band.x as f32,
                                    band.y as f32,
                                    band.width as f32,
                                    band.height as f32,
                                ),
                            );
                        }
                    }
                    let actual = gtk::Snapshot::new();
                    capture_draw::backdrop_base(
                        &actual,
                        &base_img,
                        &selection,
                        &overlay_style,
                        viewport,
                    );
                    actual.save();
                    actual.scale(1. / doc_scale, 1. / doc_scale);
                    actual.append_node(&node);
                    actual.restore();
                    capture_draw::overlay(&actual, &selection, &overlay_style, [0., 0.], viewport);
                    let expected = download(expected);
                    let actual = download(actual);
                    for (i, (e, a)) in expected
                        .chunks_exact(4)
                        .zip(actual.chunks_exact(4))
                        .enumerate()
                    {
                        let err = e.iter().zip(a).map(|(x, y)| x.abs_diff(*y)).max().unwrap();
                        if err > worst {
                            worst = err;
                            worst_case =
                                format!("{sname}@{doc_scale}x/{label}@({},{})", i % 64, i / 64);
                            worst_px = [e[0], e[1], e[2], e[3], a[0], a[1], a[2], a[3]];
                        }
                    }
                }
            }
        }
        results.push(json!({"name":"erased annotations stay visible without seams across dragged selection regions at 1x and 1.5x","passed":worst<=1&&localized_parity_worst<=1&&preview_parity_worst<=1&&empty_eraser_mask_free,"maxError":worst,"localizedParityError":localized_parity_worst,"previewParityError":preview_parity_worst,"emptyEraserMaskFree":empty_eraser_mask_free,"worstCase":worst_case,"expectedActual":worst_px}));
    }
    // Verify overlapping opaque Pen + translucent Highlight + Eraser preserves
    // exact draw order both inside and outside dragged selection rects when baked.
    {
        use clivuu_core::annotation::{Fill, Kind, Mark};
        use clivuu_desktop::editor::render::{AnnotationNodes, MosaicTextures, source_texture};
        use std::sync::Arc;
        let mut pen = Mark::new(Kind::Pen, [10., 32.], 1.);
        pen.points = Arc::new(vec![[10., 32.], [54., 32.]]);
        pen.style.color = "#ff0000".into();
        pen.style.alpha = 255;
        pen.style.width = 8.;
        pen.style.fill = Fill::Stroke;

        let mut highlight = Mark::new(Kind::Highlight, [32., 10.], 1.);
        highlight.points = Arc::new(vec![[32., 10.], [32., 54.]]);
        highlight.style.color = "#00ff00".into();
        highlight.style.alpha = 82;
        highlight.style.width = 16.;
        highlight.style.fill = Fill::Stroke;

        let mut eraser_on_pen = Mark::new(Kind::Eraser, [14., 26.], 1.);
        eraser_on_pen.points = Arc::new(vec![[14., 26.], [18., 38.]]);
        eraser_on_pen.style.width = 8.;
        eraser_on_pen.style.fill = Fill::Stroke;

        let mut eraser_on_hi = Mark::new(Kind::Eraser, [28., 14.], 1.);
        eraser_on_hi.points = Arc::new(vec![[28., 14.], [36., 18.]]);
        eraser_on_hi.style.width = 8.;
        eraser_on_hi.style.fill = Fill::Stroke;

        let context = window.pango_context();
        let mosaics = MosaicTextures::new();
        let viewport = Bounds {
            x: 0.,
            y: 0.,
            width: 64.,
            height: 64.,
        };
        let mask = gdk::RGBA::new(0., 0., 0., 0.44);
        let overlay_style = CaptureStyle {
            border_width: 0.,
            mask,
            anchors: false,
            guides: false,
            crosshair: false,
            ..Default::default()
        };
        let base_img =
            FrozenImage::new(RgbaImage::from_pixel(64, 64, Rgba([0, 0, 255, 255]))).unwrap();
        let mut max_err = 0u8;
        let mut worst_info = String::new();
        let mut worst_px = [0u8; 8];
        let mut z_order_ok = true;
        let mut bad_z = String::new();
        for (name, marks, expect_hi_over_pen) in [
            (
                "pen_then_hi_erase_pen",
                vec![pen.clone(), highlight.clone(), eraser_on_pen.clone()],
                true,
            ),
            (
                "pen_then_hi_erase_hi",
                vec![pen.clone(), highlight.clone(), eraser_on_hi.clone()],
                true,
            ),
            (
                "hi_then_pen_erase_hi",
                vec![highlight.clone(), pen.clone(), eraser_on_hi.clone()],
                false,
            ),
            (
                "hi_then_pen_erase_pen",
                vec![highlight, pen, eraser_on_pen],
                false,
            ),
        ] {
            let mut nodes = AnnotationNodes::default();
            let node = nodes.compose(&marks, &context, &mosaics).unwrap();
            let baked =
                renderer.render_texture(&node, Some(&graphene::Rect::new(0., 0., 64., 64.)));
            for (sel_w, expect_inside) in [(56., true), (20., false)] {
                let mut selection = Selection::new(
                    vec![Output {
                        rect: viewport,
                        scale: 1.,
                    }],
                    vec![],
                    None,
                )
                .unwrap();
                selection.selected = true;
                selection.rect = Bounds {
                    x: 0.,
                    y: 0.,
                    width: sel_w,
                    height: 64.,
                };
                let expected = gtk::Snapshot::new();
                expected.append_color(&gdk::RGBA::BLUE, &graphene::Rect::new(0., 0., 64., 64.));
                expected.append_node(&node);
                for band in clivuu_core::capture::outside(selection.rect, viewport) {
                    if band.width > 0. && band.height > 0. {
                        expected.append_color(
                            &mask,
                            &graphene::Rect::new(
                                band.x as f32,
                                band.y as f32,
                                band.width as f32,
                                band.height as f32,
                            ),
                        );
                    }
                }
                let actual = gtk::Snapshot::new();
                capture_draw::backdrop_base(
                    &actual,
                    &base_img,
                    &selection,
                    &overlay_style,
                    viewport,
                );
                source_texture(
                    &actual,
                    &baked,
                    &graphene::Rect::new(0., 0., 64., 64.),
                    true,
                );
                capture_draw::overlay(&actual, &selection, &overlay_style, [0., 0.], viewport);
                let exp = download(expected);
                let act = download(actual);
                for (idx, (a, b)) in exp.chunks_exact(4).zip(act.chunks_exact(4)).enumerate() {
                    let err = a.iter().zip(b).map(|(x, y)| x.abs_diff(*y)).max().unwrap();
                    if err > max_err {
                        max_err = err;
                        worst_info = format!("{name}/sel={sel_w}@({},{})", idx % 64, idx / 64);
                        worst_px = [a[0], a[1], a[2], a[3], b[0], b[1], b[2], b[3]];
                    }
                }
                let off = (32 * 64 + 32) * 4;
                let [b, g, r, _] = [act[off], act[off + 1], act[off + 2], act[off + 3]];
                let ok = if expect_hi_over_pen {
                    (expect_inside && r > 150 && g > 70 && b == 0)
                        || (!expect_inside && r > 80 && g > 35 && b == 0)
                } else {
                    g == 0
                        && b == 0
                        && if expect_inside {
                            r == 255
                        } else {
                            r > 120 && r < 160
                        }
                };
                if !ok {
                    z_order_ok = false;
                    bad_z = format!(
                        "{name}/sel={sel_w}: act=({r},{g},{b}) exp=({},{},{})",
                        exp[off],
                        exp[off + 1],
                        exp[off + 2]
                    );
                }
            }
        }
        results.push(json!({"name":"pen and translucent highlight keep invariant z-order with baked eraser across dragged selection bounds","passed":max_err<=1&&z_order_ok,"maxError":max_err,"worstCase":worst_info,"worstPx":worst_px,"zOrderOk":z_order_ok,"badZ":bad_z}));
    }
    for scale in [1.5, 2.] {
        let original = RgbaImage::from_fn(8, 8, |x, y| {
            let c = if (x + y) % 2 == 0 { 0 } else { 255 };
            Rgba([c, c, c, 255])
        });
        let backing = clivuu_core::image::imageops::resize(
            &original,
            (8. * scale) as u32,
            (8. * scale) as u32,
            clivuu_core::image::imageops::FilterType::Nearest,
        );
        let root = Viewport::new();
        let limit = Bounds {
            x: -30.,
            y: 10.,
            width: 8.,
            height: 8.,
        };
        let document = CaptureDocument::new(
            FrozenImage::new(backing).unwrap(),
            Selection::new(
                vec![Output {
                    rect: limit,
                    scale: 1.,
                }],
                vec![],
                Some(limit),
            )
            .unwrap(),
            &window.pango_context(),
            pool.clone(),
            |_| {},
        );
        let view = CaptureView::new(document.clone(), &root, limit, Default::default());
        let raster = document.raster().unwrap();
        let result = export::frame_sized(
            view.editor().scene(),
            &renderer,
            Some(raster.source),
            raster.size,
            &pool,
            &cancel,
        )
        .await
        .unwrap();
        results.push(json!({"name":format!("interactive export recovers exact native checkerboard pixels from {scale}x uniform backing"),"passed":*result.pixels==original}));
    }
    results
}

// Exercise the same capture view used by a union X11 host and per-output
// layer-shell hosts, rather than just testing the placement helper.
fn monitor_controls(window: &gtk::Window) -> Vec<Value> {
    use clivuu_desktop::{
        editor::capture_view::{CaptureDocument, CaptureView},
        infrastructure::tasks::TaskPool,
        ui::viewport::Viewport,
    };
    use gtk::gdk;
    use std::rc::Rc;
    let mut results = vec![];
    let pool = Rc::new(TaskPool::new(1, 4).unwrap());
    for scale in [1., 2.] {
        let primary = Output {
            rect: Bounds {
                x: 2880. / scale - 90.,
                y: -40.,
                width: 1920.,
                height: 1080.,
            },
            scale: 1.,
        };
        let secondary = Output {
            rect: Bounds {
                x: -90.,
                y: -40.,
                width: 2880. / scale,
                height: 1800. / scale,
            },
            scale,
        };
        let selection =
            Selection::new(vec![primary, secondary], vec![], Some(primary.rect)).unwrap();
        let limit = selection.limit;
        let image = FrozenImage::new(RgbaImage::from_pixel(
            (limit.width * scale) as u32,
            (limit.height * scale) as u32,
            Rgba([30, 50, 90, 255]),
        ))
        .unwrap();
        let document = CaptureDocument::new(
            image,
            selection,
            &window.pango_context(),
            pool.clone(),
            |_| {},
        );
        for host in [limit, primary.rect, secondary.rect] {
            let root = Viewport::new();
            let view = CaptureView::new(document.clone(), &root, host, Default::default());
            view.editor().layout();
            let bar = view.editor().panels().unwrap().toolbar;
            let expected_visible = host.contains(primary.rect.center(), 0.);
            results.push(json!({"name":format!("primary capture toolbar fits its actual monitor at {scale}x with host {host:?}"), "passed":bar.x>=primary.rect.x&&bar.y>=primary.rect.y&&bar.x+bar.width<=primary.rect.x+primary.rect.width&&bar.y+bar.height<=primary.rect.y+primary.rect.height&&view.editor().toolbar().widget().is_visible()==expected_visible&&view.editor().hint().text()=="1920 × 1080 px"&&root.cursor().and_then(|c|c.name()).is_some_and(|n|n=="default"), "toolbar":bar}));
            if host == limit {
                let inset = Bounds {
                    x: primary.rect.x + 100.,
                    y: primary.rect.y + 100.,
                    width: 100.,
                    height: 100.,
                };
                document.set_area(inset);
                view.editor()
                    .scene()
                    .key_pressed(gdk::Key::Left, gdk::ModifierType::empty());
                let moved = document.selection().rect.x == inset.x - 1.;
                document.set_area(primary.rect);
                view.editor()
                    .scene()
                    .key_pressed(gdk::Key::Left, gdk::ModifierType::SHIFT_MASK);
                let shrunk = document.selection().rect.width == primary.rect.width - 1.;
                results.push(json!({"name":format!("capture movement and edge adjustment use native primary pixels at {scale}x backing density"),"passed":moved&&shrunk}));
                document.set_area(primary.rect);
            }
        }
    }
    results
}

fn feedback_controls(window: &gtk::Window) -> Vec<Value> {
    use clivuu_desktop::{
        editor::capture_view::{CaptureDocument, CaptureView},
        infrastructure::tasks::TaskPool,
        ui::viewport::Viewport,
    };
    use gtk::gdk;
    use std::rc::Rc;
    let root = Viewport::new();
    let rect = Bounds {
        x: -300.,
        y: 20.,
        width: 800.,
        height: 600.,
    };
    let doc = CaptureDocument::new(
        FrozenImage::new(RgbaImage::from_pixel(800, 600, Rgba([100, 100, 100, 255]))).unwrap(),
        Selection::new(vec![Output { rect, scale: 1. }], vec![], None).unwrap(),
        &window.pango_context(),
        Rc::new(TaskPool::new(1, 2).unwrap()),
        |_| {},
    );
    let view = CaptureView::new(doc.clone(), &root, rect, Default::default());
    let scene = view.editor().scene();
    let mut style = CaptureStyle {
        crosshair: true,
        ..Default::default()
    };
    doc.set_style(style.clone());
    let initial = doc.feedback_style().crosshair && !doc.feedback_style().guides;
    doc.set_area(Bounds {
        x: -200.,
        y: 100.,
        width: 300.,
        height: 200.,
    });
    doc.refresh();
    let selected = !doc.feedback_style().crosshair && !doc.feedback_style().guides;
    scene.key_pressed(gdk::Key::Alt_L, gdk::ModifierType::ALT_MASK);
    scene.key_pressed(gdk::Key::Alt_L, gdk::ModifierType::ALT_MASK);
    scene.key_pressed(gdk::Key::Alt_R, gdk::ModifierType::ALT_MASK);
    scene.key_released(gdk::Key::Alt_L);
    scene.key_released(gdk::Key::Alt_R);
    let persistent = doc.feedback_style().guides;
    style.guides = false;
    style.tips = false;
    doc.set_style(style.clone());
    let disabled = !doc.feedback_style().guides;
    let tips = std::iter::successors(root.first_child(), |w| w.next_sibling())
        .find(|w| w.widget_name() == "capture-key-tips")
        .unwrap();
    let hidden = !tips.is_visible() && !tips.can_target();
    style.guides = true;
    style.tips = true;
    doc.set_style(style);
    let restored = doc.feedback_style().guides && tips.is_visible();
    doc.set_area(Bounds {
        x: -295.,
        y: 400.,
        width: 300.,
        height: 215.,
    });
    doc.refresh();
    let overlap_hidden = !tips.is_visible();
    doc.set_area(Bounds {
        x: 100.,
        y: 100.,
        width: 200.,
        height: 150.,
    });
    doc.refresh();
    let overlap_restored = tips.is_visible();
    scene.key_pressed(gdk::Key::Alt_L, gdk::ModifierType::ALT_MASK);
    scene.key_released(gdk::Key::Alt_L);
    vec![
        json!({"name":"Alt guides persist after release and ignore key repeats and a second Alt key", "passed":initial&&selected&&persistent&&!doc.feedback_style().guides}),
        json!({"name":"disabled auxiliary guides remain authoritative and key tips are independent non-input feedback", "passed":disabled&&hidden&&restored&&overlap_hidden&&overlap_restored}),
    ]
}

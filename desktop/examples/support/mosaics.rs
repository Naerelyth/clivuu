use clivuu_core::{
    annotation::{Bounds, Fill, StylePatch},
    image::{
        RgbaImage,
        effects::{Effect, TileSpec},
    },
};
use clivuu_desktop::{
    editor::{
        Action, EditorController, ImageMapping, Tool,
        render::{AnnotationNodes, mosaic_spec},
        scene::EditorDocument,
    },
    infrastructure::tasks::TaskPool,
};
use gtk::{glib, prelude::*};
use serde_json::{Value, json};
use std::{rc::Rc, time::Duration};
async fn ready(model: &Rc<EditorDocument>) {
    for _ in 0..600 {
        if model.mosaics().is_idle() {
            assert!(
                model.mosaics().error().is_none(),
                "{:?}",
                model.mosaics().error()
            );
            return;
        }
        glib::timeout_future(Duration::from_millis(5)).await;
    }
    panic!("background effect completion timed out");
}
pub async fn checks(window: &gtk::Window) -> Vec<Value> {
    let mut result = vec![];
    let pool = Rc::new(TaskPool::new(1, 1).unwrap());
    let (gate, rx) = async_channel::bounded::<()>(1);
    let (started, wait) = async_channel::bounded::<()>(1);
    let busy = pool
        .submit(move |_| {
            started.send_blocking(()).unwrap();
            rx.recv_blocking().unwrap();
            Ok(())
        })
        .unwrap();
    wait.recv().await.unwrap();
    let queued = pool.submit(|_| Ok(())).unwrap();
    let model = EditorDocument::new(EditorController::new(ImageMapping {
        display: Bounds {
            x: 0.,
            y: 0.,
            width: 64.,
            height: 64.,
        },
        document: [64., 64.],
    }));
    let source = RgbaImage::from_fn(64, 64, image_pixel);
    model.set_source(source.clone(), pool.clone());
    model.dispatch(Action::Tool(Tool::Mosaic));
    model.dispatch(Action::Style(StylePatch {
        fill: Some(Fill::Rect),
        ..Default::default()
    }));
    model.dispatch(Action::Begin {
        point: [7., 9.],
        shift: false,
    });
    model.dispatch(Action::Move {
        point: [57., 55.],
        shift: false,
    });
    model.dispatch(Action::End);
    // A full pool must leave the GTK loop responsive and retain the source.
    glib::timeout_future(Duration::from_millis(35)).await;
    result.push(json!({"name":"full effect worker queue yields to GTK without losing the source","passed":!model.mosaics().is_idle()&&model.mosaics().textures().entries.is_empty()}));
    gate.send(()).await.unwrap();
    busy.result().await.unwrap();
    queued.result().await.unwrap();
    ready(&model).await;
    let mark = model.borrow().session().marks()[0].clone();
    let spec = mosaic_spec(&mark, [64, 64]).unwrap();
    let expected = clivuu_core::image::effects::render(&source, spec, || false).unwrap();
    {
        let textures = model.mosaics().textures();
        let texture = &textures.entries[&spec].texture;
        let mut bytes = vec![0; spec.bytes()];
        texture.download(&mut bytes, spec.width as usize * 4);
        result.push(json!({"name":"document-owned background tile publishes exact native premultiplied pixels without PNG","passed":bytes==expected.pixels}));
        let context = window.pango_context();
        let mut nodes = AnnotationNodes::default();
        let node = nodes.compose([&mark], &context, &textures).unwrap();
        let frame = window
            .renderer()
            .unwrap()
            .render_texture(&node, Some(&gtk::graphene::Rect::new(0., 0., 64., 64.)));
        let mut pixels = vec![0; 64 * 64 * 4];
        frame.download(&mut pixels, 64 * 4);
        let x = 25usize;
        let y = 25usize;
        let b = spec.bounds();
        let column = ((x as f64 - b.x) / b.width * spec.width as f64).floor() as usize;
        let row = ((y as f64 - b.y) / b.height * spec.height as f64).floor() as usize;
        let offset = (row * spec.width as usize + column) * 4;
        result.push(json!({"name":"shared GSK mosaic uses global tile coordinates and clips to the annotation","passed":pixels[(y*64+x)*4..(y*64+x)*4+4]==expected.pixels[offset..offset+4]&&pixels[..4]==[0,0,0,0]}));
    }
    let default_level = Effect::Pixelate.strength_range()[2];
    for level in (default_level + 1)..=24 {
        model.dispatch(Action::Style(StylePatch {
            effect_level: Some(f64::from(level)),
            ..Default::default()
        }));
    }
    ready(&model).await;
    result.push(json!({"name":"rapid strength changes coalesce to the final shared-document state","passed":model.mosaics().textures().entries.keys().any(|spec|spec.level==24)&&!model.mosaics().textures().entries.keys().any(|spec|(default_level + 1..24).contains(&spec.level))}));
    for level in 3..=23 {
        model.dispatch(Action::Style(StylePatch {
            effect_level: Some(f64::from(level)),
            ..Default::default()
        }));
        ready(&model).await;
    }
    result.push(json!({"name":"obsolete effect textures are evicted while the active tile is retained","passed":model.mosaics().textures().entries.len()<=13&&model.mosaics().textures().entries.keys().any(|spec|spec.level==23)}));
    // Replace while a queued request belongs to the preceding frozen image.
    model.dispatch(Action::Style(StylePatch {
        effect: Some("blur".into()),
        effect_level: Some(23.),
        ..Default::default()
    }));
    let stopped = pool.clone();
    model.set_source(RgbaImage::from_pixel(64, 64, image_pixel(0, 0)), pool);
    ready(&model).await;
    {
        let textures = model.mosaics().textures();
        let spec = TileSpec::plan(
            [64, 64],
            Effect::Blur,
            23,
            Bounds {
                x: 7.,
                y: 9.,
                width: 50.,
                height: 46.,
            },
        )
        .unwrap();
        let tile = &textures.entries[&spec].texture;
        let mut pixels = vec![0; spec.bytes()];
        tile.download(&mut pixels, spec.width as usize * 4);
        let expected = clivuu_core::image::effects::render(
            &RgbaImage::from_pixel(64, 64, image_pixel(0, 0)),
            spec,
            || false,
        )
        .unwrap();
        result.push(json!({"name":"source replacement cannot publish stale work into the new image","passed":pixels==expected.pixels&&textures.entries.len()==1}));
    }
    let wrong = TileSpec::plan(
        [63, 64],
        Effect::Blur,
        6,
        Bounds {
            x: 1.,
            y: 1.,
            width: 20.,
            height: 20.,
        },
    )
    .unwrap();
    model.mosaics().request([wrong]);
    for _ in 0..100 {
        if model.mosaics().is_idle() {
            break;
        }
        glib::timeout_future(Duration::from_millis(5)).await;
    }
    let failed = model.mosaics().error().is_some();
    model.dispatch(Action::Tool(Tool::Select));
    ready(&model).await;
    result.push(json!({"name":"an obsolete preview tile failure does not poison the next valid request","passed":failed&&model.mosaics().error().is_none()}));
    stopped.close();
    let a = TileSpec::plan(
        [64, 64],
        Effect::Pixelate,
        8,
        Bounds {
            x: 1.,
            y: 1.,
            width: 20.,
            height: 20.,
        },
    )
    .unwrap();
    let b = TileSpec::plan(
        [64, 64],
        Effect::Blur,
        7,
        Bounds {
            x: 1.,
            y: 1.,
            width: 20.,
            height: 20.,
        },
    )
    .unwrap();
    let fork = model.mosaics().fork(vec![a, b]).unwrap();
    for _ in 0..100 {
        if fork.is_idle() {
            break;
        }
        glib::timeout_future(Duration::from_millis(5)).await;
    }
    result.push(json!({"name":"a stopped pool terminates a frozen two-tile request with an error","passed":fork.is_idle()&&fork.error().is_some()}));
    let export_model = EditorDocument::new(EditorController::new(ImageMapping {
        display: Bounds {
            x: 0.,
            y: 0.,
            width: 32.,
            height: 32.,
        },
        document: [32., 32.],
    }));
    for (tool, fill, a, b) in [
        (Tool::Rect, Fill::Solid, [5., 5.], [20., 20.]),
        (Tool::Eraser, Fill::Rect, [10., 10.], [15., 15.]),
    ] {
        export_model.dispatch(Action::Tool(tool));
        export_model.dispatch(Action::Style(StylePatch {
            fill: Some(fill),
            color: Some("#ff0000".into()),
            ..Default::default()
        }));
        export_model.dispatch(Action::Begin {
            point: a,
            shift: false,
        });
        export_model.dispatch(Action::Move {
            point: b,
            shift: false,
        });
        export_model.dispatch(Action::End);
    }
    let white = gtk::gdk::MemoryTexture::new(
        32,
        32,
        gtk::gdk::MemoryFormat::R8g8b8a8,
        &glib::Bytes::from_owned(vec![255u8; 32 * 32 * 4]),
        32 * 4,
    )
    .upcast();
    let export_scene = clivuu_desktop::editor::scene::Scene::new(export_model, &white);
    let cancel = gtk::gio::Cancellable::new();
    let pool = Rc::new(TaskPool::new(1, 1).unwrap());
    let png = clivuu_desktop::editor::export::png(
        &export_scene,
        &window.renderer().unwrap(),
        Some(Bounds {
            x: 4.,
            y: 4.,
            width: 18.,
            height: 18.,
        }),
        &pool,
        &cancel,
    )
    .await
    .unwrap();
    let decoded = gtk::gdk::Texture::from_bytes(&glib::Bytes::from_owned(png)).unwrap();
    let mut pixels = vec![0; 18 * 18 * 4];
    decoded.download(&mut pixels, 18 * 4);
    let pixel = |x: usize, y: usize| {
        u32::from_ne_bytes(
            pixels[(y * 18 + x) * 4..(y * 18 + x) * 4 + 4]
                .try_into()
                .unwrap(),
        )
    };
    result.push(json!({"name":"background PNG export shares cropped annotation and eraser pixels and excludes editor feedback","passed":decoded.width()==18&&decoded.height()==18&&pixel(2,2)==0xffff0000&&pixel(8,8)==0xffffffff&&pixel(0,0)==0xffffffff}));
    export_scene.dispatch(Action::Tool(Tool::Rect));
    export_scene.dispatch(Action::Begin {
        point: [22., 22.],
        shift: false,
    });
    export_scene.dispatch(Action::Move {
        point: [30., 30.],
        shift: false,
    });
    let png = clivuu_desktop::editor::export::png(
        &export_scene,
        &window.renderer().unwrap(),
        None,
        &pool,
        &cancel,
    )
    .await
    .unwrap();
    let decoded = gtk::gdk::Texture::from_bytes(&glib::Bytes::from_owned(png)).unwrap();
    let mut pixels = vec![0; 32 * 32 * 4];
    decoded.download(&mut pixels, 32 * 4);
    let offset = (26 * 32 + 26) * 4;
    result.push(json!({"name":"export includes the currently visible drawing draft without adding a history entry","passed":u32::from_ne_bytes(pixels[offset..offset+4].try_into().unwrap())==0xffff0000&&export_scene.controller().borrow().session().marks().len()==2}));
    let frozen = clivuu_desktop::editor::frozen_image::FrozenImage::new(source.clone()).unwrap();
    let lazy = EditorDocument::new(EditorController::new(ImageMapping {
        display: Bounds {
            x: 0.,
            y: 0.,
            width: 64.,
            height: 64.,
        },
        document: [64., 64.],
    }));
    lazy.dispatch(Action::Tool(Tool::Mosaic));
    lazy.dispatch(Action::Style(StylePatch {
        fill: Some(Fill::Rect),
        ..Default::default()
    }));
    lazy.dispatch(Action::Begin {
        point: [7., 9.],
        shift: false,
    });
    lazy.dispatch(Action::Move {
        point: [57., 55.],
        shift: false,
    });
    lazy.dispatch(Action::End);
    lazy.set_shared_source(frozen.pixels.clone(), pool.clone());
    let lazy_scene = clivuu_desktop::editor::scene::Scene::new(lazy.clone(), &frozen.texture);
    let renderer = window.renderer().unwrap();
    let mut pending = Box::pin(clivuu_desktop::editor::export::png(
        &lazy_scene,
        &renderer,
        None,
        &pool,
        &cancel,
    ));
    std::future::poll_fn(|cx| {
        assert!(std::future::Future::poll(pending.as_mut(), cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    lazy.dispatch(Action::Undo);
    let png = pending.await.unwrap();
    let decoded = gtk::gdk::Texture::from_bytes(&glib::Bytes::from_owned(png)).unwrap();
    let mut pixels = vec![0; 64 * 64 * 4];
    decoded.download(&mut pixels, 64 * 4);
    let p = source.get_pixel(25, 25);
    let plain = 0xff000000 | (u32::from(p[0]) << 16) | (u32::from(p[1]) << 8) | u32::from(p[2]);
    let offset = (25 * 64 + 25) * 4;
    result.push(json!({"name":"frozen export independently prepares lazy effects after the live annotation is undone","passed":lazy.borrow().session().marks().is_empty()&&u32::from_ne_bytes(pixels[offset..offset+4].try_into().unwrap())!=plain}));
    cancel.cancel();
    let cancelled = clivuu_desktop::editor::export::png(
        &export_scene,
        &window.renderer().unwrap(),
        None,
        &pool,
        &cancel,
    )
    .await;
    result.push(json!({"name":"cancelled export does not create an image job","passed":matches!(cancelled,Err(clivuu_desktop::editor::export::Error::Cancelled))}));
    result.extend(brush_strength_checks(window).await);
    for scale in [0.5, 1., 1.25, 1.5, 2., 3.] {
        let mut editor = EditorController::new(ImageMapping {
            display: Bounds {
                x: 0.,
                y: 0.,
                width: 256. / scale,
                height: 192. / scale,
            },
            document: [256., 192.],
        });
        editor.dispatch(Action::Tool(Tool::Mosaic));
        editor.dispatch(Action::Begin {
            point: [24. / scale, 24. / scale],
            shift: false,
        });
        editor.dispatch(Action::Move {
            point: [80. / scale, 80. / scale],
            shift: false,
        });
        editor.dispatch(Action::End);
        let mark = &editor.session().marks()[0];
        let spec = mosaic_spec(mark, [256, 192]).unwrap();
        let imported: clivuu_core::annotation::Mark =
            serde_json::from_value(serde_json::to_value(mark).unwrap()).unwrap();
        result.push(json!({"name":format!("shared mosaic strength retains source density {scale} through export/import"),
            "passed":mark.style.effect_scale==scale&&spec.size==(f64::from(Effect::Pixelate.strength_range()[2])*scale).round() as u32&&mosaic_spec(&imported,[256,192])==Some(spec)}));
    }
    result.extend(continuity_checks(window).await);
    result.extend(transparent_continuity_checks(window).await);
    result
}
fn image_pixel(x: u32, y: u32) -> clivuu_core::image::Rgba<u8> {
    clivuu_core::image::Rgba([
        (x * 37 + y * 17) as u8,
        (x * 13 + y * 41) as u8,
        (x * 53 + y * 3) as u8,
        255,
    ])
}

async fn brush_strength_checks(window: &gtk::Window) -> Vec<Value> {
    let mut result = vec![];
    for (effect, name) in [(Effect::Pixelate, "pixelate"), (Effect::Blur, "blur")] {
        let model = EditorDocument::new(EditorController::new(ImageMapping {
            display: Bounds {
                x: 0.,
                y: 0.,
                width: 64.,
                height: 64.,
            },
            document: [64., 64.],
        }));
        let source = RgbaImage::from_fn(64, 64, image_pixel);
        let frozen =
            clivuu_desktop::editor::frozen_image::FrozenImage::new(source.clone()).unwrap();
        model.set_source(source, Rc::new(TaskPool::new(1, 1).unwrap()));
        model.dispatch(Action::Tool(Tool::Mosaic));
        let [min, max, _] = effect.strength_range();
        model.dispatch(Action::Style(StylePatch {
            fill: Some(Fill::Stroke),
            effect: Some(name.into()),
            effect_level: Some(f64::from(min)),
            width: Some(12.),
            ..Default::default()
        }));
        model.dispatch(Action::Begin {
            point: [16., 32.],
            shift: false,
        });
        model.dispatch(Action::Move {
            point: [48., 32.],
            shift: false,
        });
        model.dispatch(Action::End);
        ready(&model).await;
        let pixels = |composite| {
            let mut nodes = AnnotationNodes::default();
            let mark = model.borrow().session().marks().last().unwrap().clone();
            let node = nodes
                .compose(
                    [&mark],
                    &window.pango_context(),
                    &model.mosaics().textures(),
                )
                .unwrap();
            let snapshot = gtk::Snapshot::new();
            if composite {
                snapshot
                    .append_texture(&frozen.texture, &gtk::graphene::Rect::new(0., 0., 64., 64.));
            }
            snapshot.append_node(&node);
            let node = snapshot.to_node().unwrap();
            let texture = window
                .renderer()
                .unwrap()
                .render_texture(&node, Some(&gtk::graphene::Rect::new(0., 0., 64., 64.)));
            let mut pixels = vec![0; 64 * 64 * 4];
            texture.download(&mut pixels, 64 * 4);
            pixels
        };
        let weak = pixels(false);
        model.dispatch(Action::Style(StylePatch {
            effect_level: Some(f64::from(max)),
            ..Default::default()
        }));
        ready(&model).await;
        let strong = pixels(false);
        result.push(json!({"name":format!("freehand {name} strength changes actual GSK pixels independently of brush coverage"),"passed":weak!=strong&&weak.chunks_exact(4).zip(strong.chunks_exact(4)).all(|(a,b)|a[3]==b[3])&&model.borrow().session().marks()[0].style.width==12.}));
        model.dispatch(Action::Style(StylePatch {
            width: Some(24.),
            ..Default::default()
        }));
        ready(&model).await;
        result.push(json!({"name":format!("freehand {name} width control preserves committed pixels and sets the next stroke width"),"passed":pixels(false)==strong&&model.borrow().session().marks()[0].style.width==12.&&model.borrow().appearance().unwrap().snapshot().style.width==24.}));
        model.dispatch(Action::Begin {
            point: [16., 32.],
            shift: false,
        });
        model.dispatch(Action::Move {
            point: [48., 32.],
            shift: false,
        });
        model.dispatch(Action::End);
        ready(&model).await;
        let wide = pixels(false);
        result.push(json!({"name":format!("next freehand {name} stroke uses new coverage while retaining strength and center sampling"),"passed":wide.chunks_exact(4).filter(|p|p[3]>0).count()>strong.chunks_exact(4).filter(|p|p[3]>0).count()&&wide[(32*64+32)*4..(32*64+32)*4+4]==strong[(32*64+32)*4..(32*64+32)*4+4]&&model.borrow().session().marks()[0].style.width==12.&&model.borrow().session().marks()[1].style.width==24.&&model.borrow().session().marks()[1].style.mosaic_level()==max}));
        let preview = pixels(true);
        // Export freezes the strong state, then the live session is undone.
        let scene = clivuu_desktop::editor::scene::Scene::new(model.clone(), &frozen.texture);
        let pool = Rc::new(TaskPool::new(1, 1).unwrap());
        let renderer = window.renderer().unwrap();
        let cancel = gtk::gio::Cancellable::new();
        let mut pending = Box::pin(clivuu_desktop::editor::export::png(
            &scene, &renderer, None, &pool, &cancel,
        ));
        std::future::poll_fn(|cx| {
            assert!(std::future::Future::poll(pending.as_mut(), cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        model.dispatch(Action::Undo);
        model.dispatch(Action::Undo);
        let png = pending.await.unwrap();
        let texture = gtk::gdk::Texture::from_bytes(&glib::Bytes::from_owned(png)).unwrap();
        let mut exported = vec![0; 64 * 64 * 4];
        texture.download(&mut exported, 64 * 4);
        result.push(json!({"name":format!("frozen freehand {name} export retains strength after live undo and matches preview"),"passed":exported[(32*64+32)*4..(32*64+32)*4+4]==preview[(32*64+32)*4..(32*64+32)*4+4]}));
    }
    result
}

/// Hold the worker so every captured frame exercises retained presentation,
/// rather than accidentally passing because a tiny fixture finished in time.
async fn continuity_checks(window: &gtk::Window) -> Vec<Value> {
    let mut result = vec![];
    let pixel =
        |frame: &[u8], x: usize, y: usize| frame[(y * 256 + x) * 4..(y * 256 + x) * 4 + 4].to_vec();
    for (effect, name) in [(Effect::Pixelate, "pixelate"), (Effect::Blur, "blur")] {
        for fill in [Fill::Stroke, Fill::Rect] {
            let pool = Rc::new(TaskPool::new(1, 2).unwrap());
            let model = EditorDocument::new(EditorController::new(ImageMapping {
                display: Bounds {
                    x: 0.,
                    y: 0.,
                    width: 256.,
                    height: 192.,
                },
                document: [256., 192.],
            }));
            model.set_source(RgbaImage::from_fn(256, 192, image_pixel), pool.clone());
            model.dispatch(Action::Tool(Tool::Mosaic));
            model.dispatch(Action::Style(StylePatch {
                fill: Some(fill),
                effect: Some(name.into()),
                effect_level: Some(f64::from(effect.strength_range()[0])),
                width: Some(12.),
                ..Default::default()
            }));
            model.dispatch(Action::Begin {
                point: [48., 64.],
                shift: false,
            });
            model.dispatch(Action::Move {
                point: [80., 96.],
                shift: false,
            });
            ready(&model).await;
            let frame = || {
                let mut nodes = AnnotationNodes::default();
                let controller = model.borrow();
                let node = nodes.compose_preview(
                    controller.session().displayed(),
                    controller.drawing(),
                    &window.pango_context(),
                    &model.mosaics().textures(),
                );
                let snapshot = gtk::Snapshot::new();
                if let Some(node) = node {
                    snapshot.append_node(&node);
                }
                let texture = window.renderer().unwrap().render_texture(
                    snapshot.to_node().unwrap_or_else(|| {
                        gtk::gsk::ColorNode::new(
                            &gtk::gdk::RGBA::TRANSPARENT,
                            &gtk::graphene::Rect::new(0., 0., 256., 192.),
                        )
                        .upcast()
                    }),
                    Some(&gtk::graphene::Rect::new(0., 0., 256., 192.)),
                );
                let mut bytes = vec![0; 256 * 192 * 4];
                texture.download(&mut bytes, 256 * 4);
                bytes
            };
            let first = frame();
            let (release, held) = async_channel::bounded::<()>(1);
            let (started, wait) = async_channel::bounded::<()>(1);
            let busy = pool
                .submit(move |_| {
                    started.send_blocking(()).unwrap();
                    held.recv_blocking().unwrap();
                    Ok(())
                })
                .unwrap();
            wait.recv().await.unwrap();
            let mut old_pixels_retained = true;
            let mut new_pixels_covered = true;
            for x in (96..=208).step_by(8) {
                model.dispatch(Action::Move {
                    point: [x as f64, 96.],
                    shift: false,
                });
                let pixels = frame();
                old_pixels_retained &= pixel(&pixels, 64, 80) == pixel(&first, 64, 80);
                new_pixels_covered &= pixel(&pixels, x as usize - 2, 95)[3] == 255;
                glib::timeout_future(Duration::from_millis(1)).await;
            }
            result.push(json!({"name":format!("growing {name} {fill:?} retains completed pixels on every blocked-worker frame"),"passed":old_pixels_retained,"first":pixel(&first,64,80),"last":pixel(&frame(),64,80)}));
            result.push(json!({"name":format!("growing {name} {fill:?} covers new mask pixels while precise work is pending"),"passed":new_pixels_covered}));
            model.dispatch(Action::End);
            release.send(()).await.unwrap();
            busy.result().await.unwrap();
            ready(&model).await;
            let completed = frame();
            let (release, held) = async_channel::bounded::<()>(1);
            let (started, wait) = async_channel::bounded::<()>(1);
            let busy = pool
                .submit(move |_| {
                    started.send_blocking(()).unwrap();
                    held.recv_blocking().unwrap();
                    Ok(())
                })
                .unwrap();
            wait.recv().await.unwrap();
            let mut retained = true;
            for level in effect.strength_range()[0]..=effect.strength_range()[1] {
                model.dispatch(Action::Style(StylePatch {
                    effect_level: Some(f64::from(level)),
                    ..Default::default()
                }));
                retained &= pixel(&frame(), 64, 80) == pixel(&completed, 64, 80);
                glib::timeout_future(Duration::from_millis(1)).await;
            }
            result.push(json!({"name":format!("rapid {name} {fill:?} strength changes retain prior pixels despite changing sampling halos"),"passed":retained}));
            model.dispatch(Action::Deselect);
            let mut covered = true;
            for y in [120., 140., 160.] {
                model.dispatch(Action::Begin {
                    point: [120., y],
                    shift: false,
                });
                model.dispatch(Action::Move {
                    point: [144., y + 8.],
                    shift: false,
                });
                model.dispatch(Action::End);
                covered &= pixel(&frame(), 132, y as usize + 4)[3] == 255;
                model.dispatch(Action::Deselect);
            }
            result.push(json!({"name":format!("rapid consecutive {name} {fill:?} placements remain covered with a saturated worker"),"passed":covered}));
            release.send(()).await.unwrap();
            busy.result().await.unwrap();
            ready(&model).await;
            let specs: Vec<_> = model
                .borrow()
                .session()
                .displayed()
                .filter_map(|mark| mosaic_spec(mark, [256, 192]))
                .collect();
            result.push(json!({"name":format!("{name} {fill:?} converges to every exact final tile after stress"),"passed":specs.iter().all(|spec|model.mosaics().textures().entries.contains_key(spec))}));
        }
    }
    result
}

async fn transparent_continuity_checks(window: &gtk::Window) -> Vec<Value> {
    let mut result = vec![];
    for (name, level) in [("pixelate", 3), ("pixelate", 8), ("blur", 3), ("blur", 8)] {
        let pool = Rc::new(TaskPool::new(1, 2).unwrap());
        let model = EditorDocument::new(EditorController::new(ImageMapping {
            display: Bounds {
                x: 0.,
                y: 0.,
                width: 256.,
                height: 192.,
            },
            document: [256., 192.],
        }));
        model.set_source(
            RgbaImage::from_pixel(256, 192, clivuu_core::image::Rgba([100, 110, 120, 128])),
            pool.clone(),
        );
        model.dispatch(Action::Tool(Tool::Mosaic));
        model.dispatch(Action::Style(StylePatch {
            effect: Some(name.into()),
            effect_level: Some(f64::from(level)),
            fill: Some(Fill::Stroke),
            width: Some(12.),
            ..Default::default()
        }));
        model.dispatch(Action::Begin {
            point: [32., 64.],
            shift: false,
        });
        model.dispatch(Action::Move {
            point: [64., 64.],
            shift: false,
        });
        ready(&model).await;
        let (release, held) = async_channel::bounded::<()>(1);
        let (started, wait) = async_channel::bounded::<()>(1);
        let busy = pool
            .submit(move |_| {
                started.send_blocking(()).unwrap();
                held.recv_blocking().unwrap();
                Ok(())
            })
            .unwrap();
        wait.recv().await.unwrap();
        model.dispatch(Action::Move {
            point: [176., 64.],
            shift: false,
        });
        let pixels = || {
            let mut nodes = AnnotationNodes::default();
            let editor = model.borrow();
            let node = nodes
                .compose_preview(
                    editor.session().displayed(),
                    editor.drawing(),
                    &window.pango_context(),
                    &model.mosaics().textures(),
                )
                .unwrap();
            let frame = window
                .renderer()
                .unwrap()
                .render_texture(&node, Some(&gtk::graphene::Rect::new(0., 0., 256., 192.)));
            let mut bytes = vec![0; 256 * 192 * 4];
            frame.download(&mut bytes, 256 * 4);
            bytes
        };
        let pending = pixels();
        release.send(()).await.unwrap();
        busy.result().await.unwrap();
        ready(&model).await;
        let exact = pixels();
        let stable = (34..174).all(|x| {
            pending[(64 * 256 + x) * 4..(64 * 256 + x) * 4 + 4]
                == exact[(64 * 256 + x) * 4..(64 * 256 + x) * 4 + 4]
                && exact[(64 * 256 + x) * 4 + 3] == 128
        });
        result.push(json!({"name":format!("pending {name} level {level} ROI replaces preview without stacking alpha or leaving seams"),"passed":stable,"mismatches":(34..174).filter(|&x|pending[(64*256+x)*4..(64*256+x)*4+4]!=exact[(64*256+x)*4..(64*256+x)*4+4]).take(8).map(|x|json!({"x":x,"pending":&pending[(64*256+x)*4..(64*256+x)*4+4],"exact":&exact[(64*256+x)*4..(64*256+x)*4+4]})).collect::<Vec<_>>()}));
    }
    result
}

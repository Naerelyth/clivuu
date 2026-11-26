use super::*;
#[test]
fn imported_mosaic_levels_follow_the_same_rounding_default_and_limits_as_edits() {
    let mut mark = Mark::new(Kind::Mosaic, [0., 0.], 1.);
    for (value, expected) in [
        (6.9, 7),
        (0., 8),
        (1., 3),
        (3., 3),
        (18.9, 19),
        (24.9, 24),
        (99., 24),
    ] {
        mark.style.effect_level = value;
        assert_eq!(mark.style.mosaic_level(), expected);
    }
}
use std::sync::Arc;

fn shape(kind: Kind) -> Mark {
    let mut mark = Mark::new(kind, [10., 20.], 1.);
    mark.points = Arc::new(vec![[10., 20.], [100., 70.]]);
    mark
}
#[test]
fn separate_elements_and_wheel_run_have_separate_undo_steps() {
    let mut e = EditorSession::new(1., 1.);
    e.add(shape(Kind::Rect));
    e.add(shape(Kind::Ellipse));
    assert!(!e.state().can_redo);
    let initial = e.current().unwrap().style.width;
    for _ in 0..8 {
        e.wheel(Kind::Ellipse, -1.);
    }
    assert_eq!(e.current().unwrap().style.width, initial + 8.);
    e.undo();
    assert_eq!(e.marks().len(), 2);
    assert_eq!(e.current().unwrap().style.width, initial);
    e.undo();
    assert_eq!(e.marks().len(), 1);
    e.redo();
    assert_eq!(e.marks().len(), 2);
    e.redo();
    assert_eq!(e.current().unwrap().style.width, initial + 8.);
}
#[test]
fn wheel_return_to_origin_and_identity_drag_do_not_consume_undo() {
    let mut e = EditorSession::new(1., 1.);
    e.add(shape(Kind::Rect));
    e.wheel(Kind::Rect, -1.);
    e.wheel(Kind::Rect, 1.);
    assert!(e.begin([10., 20.], None, 1.));
    e.preview([10., 20.], false);
    e.end();
    e.undo();
    assert!(e.marks().is_empty());
}
#[test]
fn transforming_preview_reuses_points_and_is_not_an_undo_operation() {
    let mut e = EditorSession::new(1., 1.);
    e.add(shape(Kind::Rect));
    let points = e.marks()[0].points.clone();
    assert!(e.begin([10., 20.], None, 1.));
    for i in 1..100 {
        e.preview([10. - i as f64, 20. - i as f64], false);
        assert!(Arc::ptr_eq(&points, &e.current().unwrap().points));
    }
    assert_eq!(e.marks()[0].transform, None);
    e.end();
    e.undo();
    assert_eq!(e.marks()[0].transform, None);
    e.undo();
    assert!(e.marks().is_empty());
}
#[test]
fn text_restyle_measures_before_atomic_commit_and_preserves_world_origin() {
    let mut e = EditorSession::new(1., 1.)
        .with_text_measurer(|mark, _| [if mark.style.bold { 140. } else { 45. }, 30.]);
    let mut m = Mark::new(Kind::Text, [80., 40.], 1.);
    m.text = Some("placed".into());
    m.transform = Some(Transform {
        angle: 0.6,
        sx: 0.8,
        sy: 0.8,
        x: 13.,
        y: -7.,
    });
    e.add(m);
    let before = e.current().unwrap().clone();
    let origin = before.map_point(before.points[0], 1., false);
    e.change(
        Kind::Text,
        &StylePatch {
            bold: Some(true),
            ..Default::default()
        },
        None,
    );
    assert_eq!(e.current().unwrap().bounds(1.).width, 140.);
    let after = e.current().unwrap().map_point(before.points[0], 1., false);
    assert!((origin[0] - after[0]).abs() < 1e-9 && (origin[1] - after[1]).abs() < 1e-9);
    e.undo();
    assert_eq!(e.current(), Some(&before));
    e.undo();
    assert!(e.marks().is_empty());
}
#[test]
fn malformed_imported_text_is_rejected_before_it_can_reach_transform_operations() {
    let mut e = EditorSession::new(1., 1.);
    let mut m = Mark::new(Kind::Text, [0., 0.], 1.);
    m.text = None;
    assert!(!e.add(m));
    let mut m = Mark::new(Kind::Text, [0., 0.], 1.);
    m.text = Some("a".into());
    m.style.font_size = -1.;
    m.style.font_unit = "px".into();
    assert!(!e.add(m));
    assert!(!e.begin_text([0., 0.], Some(0)));
    assert!(e.marks().is_empty());
}
#[test]
fn freehand_elements_have_no_resize_and_arrows_only_two_endpoints() {
    for kind in [Kind::Pen, Kind::Highlight, Kind::Mosaic, Kind::Eraser] {
        assert!(shape(kind).handles(1., 1.).is_empty());
    }
    let m = shape(Kind::Arrow);
    let handles = m.handles(1., 1.);
    assert_eq!(handles.len(), 2);
    assert!(handles.iter().all(|h| h.kind == HandleKind::Endpoint));
}
#[test]
fn a_new_edit_after_undo_invalidates_redo_and_text_drafts_commit_once() {
    let mut e = EditorSession::new(1., 1.);
    e.add(shape(Kind::Rect));
    e.add(shape(Kind::Ellipse));
    e.undo();
    e.begin_text([60., 60.], None);
    e.update_text("a", 0);
    e.update_text("ab", 100);
    e.update_text("abc", 200);
    assert_eq!(e.marks().len(), 1);
    assert!(e.text_history(false));
    assert_eq!(e.current().unwrap().text.as_deref(), Some(""));
    e.text_history(true);
    e.commit_text();
    assert_eq!(e.marks().len(), 2);
    assert!(!e.state().can_redo);
    e.undo();
    assert_eq!(e.marks().len(), 1);
}

#[test]
fn zero_width_pango_metrics_remain_importable_for_blank_lines() {
    let mut session = EditorSession::new(1., 1.).with_text_measurer(|_, _| [0., 30.]);
    let mut mark = Mark::new(Kind::Text, [5., 7.], 1.);
    mark.text = Some("\n".into());
    mark.text_size = Some([0., 30.]);
    assert!(session.add(mark));
    let exported = session.marks()[0].clone();
    assert!(exported.valid());
    assert_eq!(exported.bounds(1.).width, 1.);
    let mut restored = EditorSession::new(1., 1.);
    assert!(restored.add(exported));
}

#[test]
fn mosaic_supports_only_freehand_and_rectangular_regions_at_every_model_entry() {
    let mut mark = shape(Kind::Mosaic);
    mark.style.fill = Fill::Ellipse;
    assert!(!mark.valid());
    let mut session = EditorSession::new(1., 1.);
    assert!(!session.add(mark.clone()));
    assert!(serde_json::from_value::<Mark>(serde_json::to_value(&mark).unwrap()).is_err());
    let initial = session.style(Kind::Mosaic).style.fill;
    let removed = StylePatch {
        fill: Some(Fill::Ellipse),
        ..Default::default()
    };
    session.change(Kind::Mosaic, &removed, None);
    assert_eq!(session.style(Kind::Mosaic).style.fill, initial);
    mark.style.fill = Fill::Rect;
    assert!(session.add(mark.clone()));
    session.change(Kind::Mosaic, &removed, None);
    assert_eq!(session.current(), Some(&mark));
    let mut highlight = shape(Kind::Highlight);
    highlight.style.fill = Fill::Ellipse;
    assert!(highlight.valid());
    assert!(serde_json::from_value::<Mark>(serde_json::to_value(&highlight).unwrap()).is_ok());
    assert!(session.add(highlight));
}

#[test]
fn mosaic_mode_changes_choose_future_geometry_without_rewriting_existing_marks() {
    let mut e = EditorSession::new(1., 1.);
    let stroke = shape(Kind::Mosaic);
    assert_eq!(stroke.style.fill, Fill::Stroke);
    e.add(stroke.clone());
    e.change(
        Kind::Mosaic,
        &StylePatch {
            fill: Some(Fill::Rect),
            ..Default::default()
        },
        None,
    );
    assert!(e.current().is_none());
    assert_eq!(e.marks(), std::slice::from_ref(&stroke));
    let mut region = e.create(Kind::Mosaic, [20., 30.]);
    region.points = Arc::new(vec![[20., 30.], [80., 90.]]);
    assert_eq!(region.style.fill, Fill::Rect);
    e.add(region.clone());
    e.change(
        Kind::Mosaic,
        &StylePatch {
            fill: Some(Fill::Stroke),
            ..Default::default()
        },
        None,
    );
    assert!(e.current().is_none());
    assert_eq!(e.marks(), &[stroke, region]);
    // Tool changes never introduce a synthetic undo step.
    e.undo();
    assert_eq!(e.marks().len(), 1);
    e.undo();
    assert!(e.marks().is_empty());
}

#[test]
fn freehand_strokes_are_fixed_while_geometric_shapes_and_lines_remain_movable() {
    for kind in [Kind::Pen, Kind::Highlight, Kind::Mosaic, Kind::Eraser] {
        let mut e = EditorSession::new(1., 1.);
        let stroke = shape(kind);
        assert!(stroke.freehand());
        e.add(stroke);
        let before = e.marks().to_vec();
        for tool in [None, Some(kind)] {
            assert!(!e.begin([55., 45.], tool, 1.));
            assert_eq!(e.cursor([55., 45.], tool, 1.), None);
        }
        assert_eq!(e.marks(), before);
    }
    for (kind, fill) in [
        (Kind::Mosaic, Fill::Rect),
        (Kind::Highlight, Fill::Line),
        (Kind::Highlight, Fill::Rect),
        (Kind::Highlight, Fill::Ellipse),
        (Kind::Eraser, Fill::Rect),
    ] {
        let mut e = EditorSession::new(1., 1.);
        let mut region = shape(kind);
        region.style.fill = fill;
        assert!(!region.freehand());
        e.add(region);
        assert!(e.begin([55., 45.], None, 1.));
        e.preview([65., 55.], false);
        e.end();
        assert_eq!(e.current().unwrap().transform().x, 10.);
    }
}

#[test]
fn mosaic_wheel_changes_width_or_effect_by_geometry_and_coalesces_history() {
    for scale in [1., 2.] {
        for fill in [Fill::Stroke, Fill::Rect] {
            let mut e = EditorSession::new(1., scale);
            e.change(
                Kind::Mosaic,
                &StylePatch {
                    fill: Some(fill),
                    ..Default::default()
                },
                None,
            );
            let mut m = shape(Kind::Mosaic);
            m.style.fill = fill;
            e.add(m.clone());
            let width = e.input_width(Kind::Mosaic);
            for _ in 0..3 {
                e.wheel(Kind::Mosaic, -1.);
            }
            let edited = e.current().unwrap().clone();
            if fill == Fill::Stroke {
                assert_eq!(edited, m);
                assert_eq!(e.input_width(Kind::Mosaic), width + 3. * scale);
                assert_eq!(edited.style.mosaic_level(), m.style.mosaic_level());
            } else {
                assert_eq!(edited.style.width, m.style.width);
                assert_eq!(edited.style.mosaic_level(), m.style.mosaic_level() + 3);
            }
            e.undo();
            if fill == Fill::Stroke {
                assert!(e.marks().is_empty());
            } else {
                assert_eq!(e.current(), Some(&m));
            }
            e.redo();
            assert_eq!(e.current(), Some(&edited));
            // Templates use exactly the same rule before a new mark is drawn.
            e.select(None);
            let initial = e.style(Kind::Mosaic).clone();
            e.wheel(Kind::Mosaic, 1.);
            let style = e.style(Kind::Mosaic);
            if fill == Fill::Stroke {
                assert_eq!(style.style.width, initial.style.width - scale);
            } else {
                assert_eq!(style.style.mosaic_level(), initial.style.mosaic_level() - 1);
            }
        }
    }
}

#[test]
fn mosaic_effect_defaults_ranges_and_width_are_independent_in_both_geometries() {
    for fill in [Fill::Stroke, Fill::Rect] {
        let mut e = EditorSession::new(1., 1.);
        let mut m = shape(Kind::Mosaic);
        m.style.fill = fill;
        e.add(m.clone());
        e.change(
            Kind::Mosaic,
            &StylePatch {
                effect: Some("blur".into()),
                ..Default::default()
            },
            None,
        );
        assert_eq!(e.current().unwrap().style.mosaic_level(), 8);
        assert_eq!(e.current().unwrap().style.width, m.style.width);
        e.change(
            Kind::Mosaic,
            &StylePatch {
                effect_level: Some(80.),
                ..Default::default()
            },
            None,
        );
        assert_eq!(e.current().unwrap().style.mosaic_level(), 24);
        e.change(
            Kind::Mosaic,
            &StylePatch {
                effect_level: Some(1.),
                ..Default::default()
            },
            None,
        );
        assert_eq!(e.current().unwrap().style.mosaic_level(), 3);
        e.undo();
        assert_eq!(e.current().unwrap().style.mosaic_level(), 24);
        e.change(
            Kind::Mosaic,
            &StylePatch {
                width: Some(24.),
                ..Default::default()
            },
            None,
        );
        assert_eq!(e.current().unwrap().style.mosaic_level(), 24);
        assert_eq!(e.input_width(Kind::Mosaic), 24.);
        if fill == Fill::Rect {
            e.undo();
        }
        assert_eq!(e.current().unwrap().style.width, m.style.width);
        e.undo();
        assert_eq!(e.current().unwrap().style.mosaic_level(), 8);
        e.undo();
        assert_eq!(e.current(), Some(&m));
        e.change(
            Kind::Mosaic,
            &StylePatch {
                effect: Some("blur".into()),
                effect_level: Some(19.),
                ..Default::default()
            },
            None,
        );
        assert_eq!(e.current().unwrap().style.mosaic_level(), 19);
        e.change(
            Kind::Mosaic,
            &StylePatch {
                effect: Some("pixelate".into()),
                ..Default::default()
            },
            None,
        );
        assert_eq!(e.current().unwrap().style.mosaic_level(), 19);
    }
    let source = serde_json::json!({"kind":"mosaic","points":[[0,0],[20,20]],"effect":"blur"});
    let imported: Mark = serde_json::from_value(source.clone()).unwrap();
    assert_eq!(imported.style.effect_level, 8.);
    let mut explicit = source;
    explicit["effectLevel"] = serde_json::json!(23);
    assert_eq!(
        serde_json::from_value::<Mark>(explicit)
            .unwrap()
            .style
            .effect_level,
        23.
    );
}

#[test]
fn committed_freehand_width_is_fixed_while_future_width_and_other_properties_are_editable() {
    for scale in [1., 2.] {
        for kind in [Kind::Pen, Kind::Highlight, Kind::Mosaic, Kind::Eraser] {
            let mut e = EditorSession::new(1., scale);
            let mut mark = e.create(kind, [10., 20.]);
            mark.points = Arc::new(vec![[10., 20.], [100., 70.]]);
            assert!(mark.freehand());
            e.add(mark.clone());
            let future = mark.style.width + 5. * scale;
            e.change(
                kind,
                &StylePatch {
                    width: Some(future),
                    ..Default::default()
                },
                None,
            );
            for _ in 0..3 {
                e.wheel(kind, -1.);
            }
            assert_eq!(e.marks(), std::slice::from_ref(&mark));
            assert_eq!(e.input_width(kind), future + 3. * scale);
            assert_eq!(e.current(), Some(&mark));
            // A mixed patch must still edit color/effect while preserving ink width.
            e.change(
                kind,
                &StylePatch {
                    width: Some(future),
                    color: Some("#0000ff".into()),
                    ..Default::default()
                },
                None,
            );
            assert_eq!(e.current().unwrap().style.width, mark.style.width);
            assert_eq!(e.current().unwrap().style.color, "#0000ff");
            e.undo();
            assert_eq!(e.current(), Some(&mark));
            e.redo();
            assert_eq!(e.current().unwrap().style.width, mark.style.width);
            let mut next = e.create(kind, [120., 20.]);
            assert_eq!(next.style.width, future);
            next.points = Arc::new(vec![[120., 20.], [150., 70.]]);
            e.add(next);
            e.undo();
            e.undo();
            assert_eq!(e.marks(), &[mark]);
            e.undo();
            assert!(
                e.marks().is_empty(),
                "width controls must not add undo entries"
            );
        }
    }
}

#[test]
fn highlight_and_eraser_freehand_strokes_never_convert_to_or_from_shapes() {
    for (kind, shapes) in [
        (
            Kind::Highlight,
            &[Fill::Line, Fill::Rect, Fill::Ellipse][..],
        ),
        (Kind::Eraser, &[Fill::Rect][..]),
    ] {
        for &target_fill in shapes {
            let mut e = EditorSession::new(1., 1.);
            let mut stroke = e.create(kind, [10., 20.]);
            stroke.points = Arc::new(vec![[10., 20.], [40., 35.], [90., 65.]]);
            assert_eq!(stroke.style.fill, Fill::Stroke);
            assert!(e.add(stroke.clone()));
            e.change(
                kind,
                &StylePatch {
                    fill: Some(target_fill),
                    ..Default::default()
                },
                None,
            );
            assert!(e.current().is_none());
            assert_eq!(e.marks(), std::slice::from_ref(&stroke));

            let mut geom = e.create(kind, [20., 30.]);
            geom.points = Arc::new(vec![[20., 30.], [110., 85.]]);
            assert_eq!(geom.style.fill, target_fill);
            assert!(e.add(geom.clone()));
            e.change(
                kind,
                &StylePatch {
                    fill: Some(Fill::Stroke),
                    ..Default::default()
                },
                None,
            );
            assert!(e.current().is_none());
            assert_eq!(e.marks(), &[stroke, geom]);
            e.undo();
            assert_eq!(e.marks().len(), 1);
            e.undo();
            assert!(e.marks().is_empty());
        }
    }
}

#[test]
fn deselected_non_text_and_non_sequence_marks_lock_permanently_even_in_cursor_mode() {
    for (kind, fill, hit_pt) in [
        (Kind::Rect, Fill::Stroke, [55., 20.]),
        (Kind::Rect, Fill::Solid, [55., 45.]),
        (Kind::Ellipse, Fill::Stroke, [55., 20.]),
        (Kind::Arrow, Fill::Stroke, [55., 45.]),
        (Kind::Highlight, Fill::Line, [55., 45.]),
        (Kind::Highlight, Fill::Rect, [55., 45.]),
        (Kind::Highlight, Fill::Ellipse, [55., 45.]),
        (Kind::Mosaic, Fill::Rect, [55., 45.]),
        (Kind::Eraser, Fill::Rect, [55., 45.]),
    ] {
        let mut e = EditorSession::new(1., 1.);
        let mut m = shape(kind);
        m.style.fill = fill;
        assert!(!m.reselectable());
        assert!(e.add(m));
        // Still active right after creation: handles and move remain available.
        assert!(e.target(hit_pt, None, 1.).is_some());
        // Once finalized / deselected, cannot be re-selected, moved, or resized.
        e.select(None);
        assert!(e.target(hit_pt, None, 1.).is_none());
        assert!(e.target([10., 20.], None, 1.).is_none());
        assert_eq!(e.cursor(hit_pt, None, 1.), None);
        assert!(!e.begin(hit_pt, None, 1.));
        assert!(e.current().is_none());
    }

    // Text and Sequence remain reselectable, and baked shapes above them do not shadow them.
    let mut e = EditorSession::new(1., 1.).with_text_measurer(|_, _| [80., 30.]);
    let mut txt = Mark::new(Kind::Text, [30., 30.], 1.);
    txt.text = Some("editable".into());
    assert!(txt.reselectable());
    assert!(e.add(txt));
    let seq = Mark::new(Kind::Sequence, [150., 50.], 1.);
    assert!(seq.reselectable());
    assert!(e.add(seq));
    let mut cover = shape(Kind::Rect);
    cover.points = Arc::new(vec![[10., 10.], [200., 100.]]);
    cover.style.fill = Fill::Solid;
    assert!(e.add(cover));
    e.select(None);

    // Clicking covered Text in Cursor mode selects Text (index 0), not baked Rect (index 2).
    assert_eq!(e.hit_kind([45., 40.], Kind::Text, 6.), Some(0));
    assert!(e.hover([45., 40.], None, 1.).is_some());
    assert!(e.begin([45., 40.], None, 1.));
    assert_eq!(e.active(), Some(0));
    e.preview([55., 50.], false);
    e.end();
    assert_eq!(e.marks()[0].transform().x, 10.);

    // Clicking covered Sequence in Cursor mode selects Sequence (index 1).
    e.select(None);
    assert!(e.begin([150., 50.], None, 1.));
    assert_eq!(e.active(), Some(1));
}

#[test]
fn polyline_arrow_vertices_edit_independently_while_active_and_lock_when_deselected() {
    let mut e = EditorSession::new(1., 1.);
    let mut arrow = Mark::new(Kind::Arrow, [20., 20.], 1.);
    arrow.points = Arc::new(vec![[20., 20.], [100., 20.], [100., 90.]]);
    assert!(e.add(arrow));
    assert_eq!(e.current().unwrap().handles(1., 1.).len(), 3);

    // Drag middle vertex (index 1) with Shift 45° snap relative to vertex 0 ([20, 20]).
    assert!(e.begin([100., 20.], None, 1.));
    e.preview([100., 35.], true);
    e.end();
    assert!((e.current().unwrap().points[1][1] - 20.).abs() < 1e-9);
    assert_eq!(e.current().unwrap().points[2], [100., 90.]);

    e.select(None);
    assert!(!e.begin([100., 20.], None, 1.));
}

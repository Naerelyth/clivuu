//! Test adapter only: compare the migrating model with the frozen JS baseline.
use clivuu_core::annotation::{EditorSession, Kind, Mark, StylePatch};
use serde_json::{Value, json};
use std::{
    io::{self, BufRead},
    sync::Arc,
};
fn main() {
    for line in io::stdin().lock().lines() {
        let line = line.unwrap();
        let input: Value = serde_json::from_str(&line).unwrap();
        let unit = input["unit"].as_f64().unwrap_or(1.);
        let scale = input["scale"].as_f64().unwrap_or(unit);
        let mut editor = EditorSession::new(unit, scale);
        if input["metrics"].as_bool() == Some(true) {
            editor = editor.with_text_measurer(|m, unit| {
                let chars = m.text.as_deref().unwrap_or("").encode_utf16().count() as f64;
                [
                    (chars
                        * if m.style.font_family == "Wide" {
                            12.
                        } else {
                            5.
                        }
                        + m.style.font_size * 3.
                        + if m.style.bold { 19. } else { 0. })
                        / unit,
                    (23. + m.style.font_size * 2.) / unit,
                ]
            });
        }
        let mut states = vec![];
        for a in input["actions"].as_array().unwrap() {
            let point = || serde_json::from_value(a["point"].clone()).unwrap();
            let kind = || serde_json::from_value::<Kind>(a["kind"].clone()).unwrap();
            let index = || a["index"].as_u64().map(|v| v as usize);
            let tool = || {
                if a["tool"].as_str() == Some("select") || a["tool"].is_null() {
                    None
                } else {
                    Some(serde_json::from_value(a["tool"].clone()).unwrap())
                }
            };
            match a["op"].as_str().unwrap() {
                "add" => {
                    let points: Vec<[f64; 2]> =
                        serde_json::from_value(a["points"].clone()).unwrap();
                    let mut mark = editor.create(kind(), points[0]);
                    mark.points = Arc::new(points);
                    if let Some(text) = a["text"].as_str() {
                        mark.text = Some(text.into());
                    }
                    let mut value = serde_json::to_value(mark).unwrap();
                    if let Some(patch) = a["patch"].as_object() {
                        value.as_object_mut().unwrap().extend(patch.clone());
                    }
                    let mark: Mark = serde_json::from_value(value).unwrap();
                    assert!(editor.add(mark));
                }
                "select" => editor.select(index()),
                "undo" => editor.undo(),
                "redo" => editor.redo(),
                "remove" => editor.remove(),
                "clear" => editor.clear(),
                "wheel" => editor.wheel(kind(), a["delta"].as_f64().unwrap()),
                "change" => {
                    let patch: StylePatch = serde_json::from_value(a["patch"].clone()).unwrap();
                    editor.change(kind(), &patch, None);
                }
                "shape" => {
                    editor.change_shape(kind());
                }
                "begin" => {
                    editor.begin(point(), tool(), a["screenScale"].as_f64().unwrap_or(1.));
                }
                "move" => editor.preview(point(), a["shift"].as_bool().unwrap_or(false)),
                "end" => editor.end(),
                "beginText" => {
                    editor.begin_text(point(), index());
                }
                "updateText" => editor.update_text(
                    a["value"].as_str().unwrap(),
                    a["time"].as_u64().unwrap_or(0),
                ),
                "commitText" => {
                    editor.commit_text();
                }
                "cancelText" => editor.cancel_text(),
                "textHistory" => {
                    editor.text_history(a["redo"].as_bool().unwrap_or(false));
                }
                "preedit" => editor.set_text_preedit(if a["size"].is_null() {
                    None
                } else {
                    Some(serde_json::from_value(a["size"].clone()).unwrap())
                }),
                _ => panic!("unknown test action"),
            }
            let mut state = serde_json::to_value(editor.state()).unwrap();
            state.as_object_mut().unwrap().remove("textEditing");
            state.as_object_mut().unwrap().remove("transforming");
            state["displayMarks"] = json!(editor.display_marks());
            state["styles"] = json!(
                Kind::ALL
                    .map(|k| (k, editor.style(k).style.clone()))
                    .into_iter()
                    .collect::<std::collections::HashMap<_, _>>()
            );
            let sample = [27., 31.];
            state["hit"] = json!(editor.hit(sample, 6.).map_or(-1, |i| i as i64));
            if let Some(mark) = editor.current() {
                state["geometry"] = json!({"bounds":mark.bounds(unit),"handles":mark.handles(unit,1.),"visible":mark.visible_handles(Some(sample),unit,1.),"mapped":mark.map_point(sample,unit,false),"inverse":mark.map_point(sample,unit,true)});
            }
            states.push(state);
        }
        println!("{}", json!(states));
    }
}

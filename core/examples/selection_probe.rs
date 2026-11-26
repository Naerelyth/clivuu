//! Differential adapter for the actual pre-migration selection methods.
use clivuu_core::{
    annotation::Bounds,
    capture::{Output, Selection},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::{self, BufRead};
#[derive(Deserialize)]
struct Case {
    outputs: Vec<Output>,
    windows: Vec<Bounds>,
    preset: Option<Bounds>,
    dimensions: [u32; 2],
    actions: Vec<Value>,
}
fn main() {
    for line in io::stdin().lock().lines() {
        let case: Case = serde_json::from_str(&line.unwrap()).unwrap();
        let mut state = Selection::new(case.outputs, case.windows, case.preset).unwrap();
        let mut snapshots = vec![];
        for a in case.actions {
            let point = || serde_json::from_value(a["point"].clone()).unwrap();
            match a["op"].as_str().unwrap() {
                "motion" => state.motion(point()),
                "begin" => state.begin(point(), 8., false),
                "end" => state.end(),
                "scroll" => state.scroll_target(a["delta"].as_i64().unwrap() as i32),
                "full" => state.fullscreen(),
                "cycle" => state.cycle_output(a["backwards"].as_bool().unwrap_or(false)),
                "nudge" => state.nudge(point(), a["resize"].as_bool().unwrap_or(false)),
                _ => panic!("unknown operation"),
            }
            let pixel = state.lens_pixel(case.dimensions).unwrap();
            snapshots.push(json!({"rect":state.rect,"selected":state.selected,"lens":state.show_lens(),"pixel":pixel,"lensRect":state.lens_rect(case.dimensions,pixel)}));
        }
        println!("{}", json!(snapshots));
    }
}

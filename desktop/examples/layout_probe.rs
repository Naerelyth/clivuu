//! Test-only JSON-lines bridge for the existing independent geometry baseline.
use clivuu_core::annotation::Bounds;
use clivuu_desktop::editor::layout::{self, Size};
use std::io::{self, BufRead};
#[derive(serde::Deserialize)]
struct Request {
    rect: Bounds,
    viewport: Bounds,
    toolbar: Size,
    hint: Size,
    options: Size,
}
fn main() {
    for line in io::stdin().lock().lines() {
        let r: Request = serde_json::from_str(&line.unwrap()).unwrap();
        let panels = layout::selection(r.rect, r.viewport, r.toolbar, r.hint);
        let options = layout::annotation(panels.toolbar, r.viewport, r.options);
        println!("{}", serde_json::json!({"panels":panels,"options":options}));
    }
}

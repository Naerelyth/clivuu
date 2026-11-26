//! Compare an ordered sequence of captured PNGs without a desktop connection.
use clivuu_core::{
    image_ops,
    scrolling::{Axis, Document},
    stitch::{Profile, find_match},
};
use std::time::Instant;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut previous = None;
    let mut document = Document::new(Axis::Vertical, 200);
    for path in std::env::args().skip(1) {
        let image = image_ops::load(&path)?;
        let current = Profile::new(image.clone());
        let start = Instant::now();
        let matched = previous.as_ref().map(|p| {
            find_match(p, &current)
                .map(|m| serde_json::json!(m))
                .map_err(|e| e.to_string())
        });
        let pushed = document.push(image).map_err(|e| e.to_string());
        println!(
            "{}",
            serde_json::json!({"path":path,"match":matched,"document":pushed,"ms":start.elapsed().as_millis()})
        );
        previous = Some(current);
    }
    Ok(())
}

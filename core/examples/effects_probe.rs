//! Headless pixel calibration and timings for the shared region effect pipeline.
use clivuu_core::{
    annotation::Bounds,
    image::effects::{self, Effect, TileSpec},
};
use serde::Deserialize;
use std::io::{self, BufRead};
#[derive(Deserialize)]
struct Case {
    source: String,
    output: String,
    effect: String,
    level: u8,
    roi: Bounds,
    #[serde(default = "one")]
    scale: f64,
    #[serde(default = "once")]
    repeat: usize,
}
fn one() -> f64 {
    1.
}
fn once() -> usize {
    1
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        let case: Case = serde_json::from_str(&line?)?;
        let source = image::open(&case.source)?.into_rgba8();
        let spec = TileSpec::plan_scaled(
            [source.width(), source.height()],
            if case.effect == "blur" {
                Effect::Blur
            } else {
                Effect::Pixelate
            },
            case.level,
            case.roi,
            case.scale,
        )
        .unwrap();
        let prepare = std::time::Instant::now();
        let source = effects::Source::prepare(&source, || false).unwrap();
        let prepare_ms = prepare.elapsed().as_secs_f64() * 1000.;
        let mut times = vec![];
        let mut tile = None;
        for _ in 0..case.repeat.max(1) {
            let start = std::time::Instant::now();
            tile = Some(effects::render_prepared(&source, spec, || false).unwrap());
            times.push(start.elapsed().as_secs_f64() * 1000.);
        }
        times.sort_by(f64::total_cmp);
        eprintln!(
            "{}",
            serde_json::json!({"effect":case.effect,"level":case.level,"scale":case.scale,
            "size":spec.size,"tile":[spec.width,spec.height],"prepare_ms":prepare_ms,"median_ms":times[times.len()/2]})
        );
        let tile = tile.unwrap();
        let image = image::RgbaImage::from_raw(spec.width, spec.height, tile.rgba()).unwrap();
        image.save(&case.output)?;
    }
    Ok(())
}

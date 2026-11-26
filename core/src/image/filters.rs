//! Bounded area sampling and normalized separable Gaussian filtering. No GUI
//! or native image-library dependency; temporaries are bounded by the ROI tile.
use super::effects::{Effect, Error, Source, Tile, TileSpec};
// Observe cancellation within wide rows as well as between rows.
fn batches(
    w: u32,
    h: u32,
    cancelled: &impl Fn() -> bool,
    mut draw: impl FnMut(u32, u32, u32, u32) -> Result<(), Error>,
) -> Result<(), Error> {
    for y in 0..h {
        for x in (0..w).step_by(2048) {
            if cancelled() {
                return Err(Error::Cancelled);
            }
            draw(x, y, (w - x).min(2048), 1)?;
        }
    }
    if cancelled() {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}
pub(super) fn render(
    source: &Source,
    spec: TileSpec,
    cancelled: impl Fn() -> bool,
) -> Result<Tile, Error> {
    let (sx, sy) = spec.step();
    let [w, h] = source.dimensions;
    let mut pixels = Vec::with_capacity(spec.bytes());
    // Each output sample integrates its whole source footprint. Native BEST
    // sampling can cap its taps under large reductions and alias dense content
    // into low frequencies that a subsequent blur can no longer remove.
    for y in 0..spec.height {
        if cancelled() {
            return Err(Error::Cancelled);
        }
        let top = f64::from(spec.top + y) * sy;
        let bottom = f64::from(spec.top + y + 1) * sy;
        for x in 0..spec.width {
            let left = f64::from(spec.left + x) * sx;
            let right = f64::from(spec.left + x + 1) * sx;
            let mut sum = [0.; 4];
            for iy in (top.floor() as u32).min(h - 1)..(bottom.ceil() as u32).min(h) {
                if iy % 64 == 0 && cancelled() {
                    return Err(Error::Cancelled);
                }
                let end = if iy == h - 1 {
                    bottom
                } else {
                    bottom.min(f64::from(iy + 1))
                };
                let wy = end - top.max(f64::from(iy));
                for ix in (left.floor() as u32).min(w - 1)..(right.ceil() as u32).min(w) {
                    if ix % 2048 == 0 && cancelled() {
                        return Err(Error::Cancelled);
                    }
                    let end = if ix == w - 1 {
                        right
                    } else {
                        right.min(f64::from(ix + 1))
                    };
                    let weight = (end - left.max(f64::from(ix))) * wy;
                    let offset = (iy as usize * w as usize + ix as usize) * 4;
                    for (total, &value) in sum.iter_mut().zip(&source.pixels[offset..offset + 4]) {
                        *total += f64::from(value) * weight;
                    }
                }
            }
            // PAD the image edge rather than sampling transparent black.
            pixels.extend(sum.map(|value| (value / (sx * sy)).round().clamp(0., 255.) as u8));
        }
    }
    if spec.effect == Effect::Blur {
        gaussian(&mut pixels, spec, &cancelled)?;
    }
    if cancelled() {
        return Err(Error::Cancelled);
    }
    Ok(Tile { spec, pixels })
}
/// Normalize the kernel before filtering premultiplied channels. Quantizing
/// every tap via Cairo paint_with_alpha loses opacity and leaks sharp source
/// details through even the strongest blur. A unit-sum fixed-point kernel keeps
/// constant color/alpha exact and rounds only once per output channel/pass.
fn gaussian(
    pixels: &mut Vec<u8>,
    spec: TileSpec,
    cancelled: &impl Fn() -> bool,
) -> Result<(), Error> {
    let factor = (f64::from(spec.size) / 3.).max(1.);
    let radius = (f64::from(spec.size) / factor).ceil() as i32;
    let sigma = (f64::from(spec.size) / factor / 2.2).max(0.5);
    const UNIT: i32 = 1 << 16;
    let weights: Vec<_> = (-radius..=radius)
        .map(|i| (-f64::from(i * i) / (2. * sigma * sigma)).exp())
        .collect();
    let sum: f64 = weights.iter().sum();
    let mut weights: Vec<i32> = weights
        .into_iter()
        .map(|w| (w / sum * f64::from(UNIT)).round() as i32)
        .collect();
    weights[radius as usize] += UNIT - weights.iter().sum::<i32>();
    let (w, h) = (spec.width as usize, spec.height as usize);
    let mut next = vec![0; pixels.len()];
    for vertical in [false, true] {
        batches(spec.width, spec.height, cancelled, |left, y, count, _| {
            let y = y as usize;
            for x in left as usize..(left + count) as usize {
                let mut channels = [UNIT / 2; 4];
                for (i, weight) in (-radius..=radius).zip(&weights) {
                    let sx = if vertical {
                        x
                    } else {
                        (x as i32 + i).clamp(0, w as i32 - 1) as usize
                    };
                    let sy = if vertical {
                        (y as i32 + i).clamp(0, h as i32 - 1) as usize
                    } else {
                        y
                    };
                    for (channel, &value) in
                        channels.iter_mut().zip(&pixels[(sy * w + sx) * 4..][..4])
                    {
                        *channel += i32::from(value) * weight;
                    }
                }
                for (value, channel) in next[(y * w + x) * 4..][..4].iter_mut().zip(channels) {
                    *value = (channel / UNIT).clamp(0, 255) as u8;
                }
            }
            Ok(())
        })?;
        std::mem::swap(pixels, &mut next);
    }
    Ok(())
}

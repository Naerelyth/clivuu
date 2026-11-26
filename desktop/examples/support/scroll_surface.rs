//! Native compositor control fixture. Pixel acquisition is synthetic; this
//! verifies real layer-shell input separately from portal/provider integration.
use clivuu_core::{
    annotation::Bounds,
    image::{Rgba, RgbaImage},
    scrolling::{Axis, Document},
};
use clivuu_desktop::ui::scroll_capture::{Appearance, Command, Controls};
use gtk::{gdk, glib};
use std::{cell::RefCell, path::Path, rc::Rc, sync::Arc, time::Duration};
fn document(axis: Axis) -> Document {
    let mut document = Document::new(axis, 10);
    for offset in [0, 80, 160] {
        let image = RgbaImage::from_fn(620, 320, |x, y| {
            let v = (x.wrapping_mul(1973) ^ (y + offset).wrapping_mul(9277)).wrapping_mul(26699);
            Rgba([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255])
        });
        document
            .push(match axis {
                Axis::Vertical => image,
                Axis::Horizontal => clivuu_core::image::imageops::rotate270(&image),
            })
            .unwrap();
    }
    document
}
pub async fn run(directory: &Path) {
    let display = gdk::Display::default().unwrap();
    let status = Rc::new(RefCell::new(None));
    let (tx, rx) = async_channel::bounded(16);
    let controls = Controls::new(
        &display,
        Bounds {
            x: 70.,
            y: 150.,
            width: 620.,
            height: 320.,
        },
        Appearance {
            dark: true,
            accent: "#bd93f9".into(),
            display: Default::default(),
        },
        status.clone(),
        || {},
        move |command| {
            let _ = tx.try_send(command);
        },
    )
    .unwrap();
    let mut axis = Axis::Vertical;
    let mut document = document(axis);
    controls.state(
        document.progress(false),
        axis,
        true,
        false,
        document.auto_crop(),
    );
    controls.preview(
        Arc::new(document.preview().unwrap()),
        document.progress(false),
    );
    loop {
        if let Some(state) = status.borrow().as_ref() {
            let pending = directory.join("layer-scroll.json.tmp");
            std::fs::write(&pending, state.to_string()).unwrap();
            std::fs::rename(pending, directory.join("layer-scroll.json")).unwrap();
        }
        if let Ok(command) = rx.try_recv() {
            match command {
                Command::Cancel => break,
                Command::Output(_) => {
                    document
                        .image()
                        .unwrap()
                        .save(directory.join("layer-scroll-output.png"))
                        .unwrap();
                    break;
                }
                Command::AutoCrop(enabled) => {
                    document.set_auto_crop(enabled);
                }
                Command::Reset => {
                    let auto_crop = document.auto_crop();
                    document = self::document(axis);
                    document.set_auto_crop(auto_crop);
                }
                Command::Axis(value) => {
                    axis = value;
                    let auto_crop = document.auto_crop();
                    document = self::document(axis);
                    document.set_auto_crop(auto_crop);
                }
                Command::TrimStart | Command::TrimEnd => {
                    document
                        .crop_to_view(matches!(command, Command::TrimStart))
                        .unwrap();
                }
                _ => {}
            }
            controls.state(
                document.progress(false),
                axis,
                true,
                false,
                document.auto_crop(),
            );
            controls.preview(
                Arc::new(document.preview().unwrap()),
                document.progress(false),
            );
        }
        glib::timeout_future(Duration::from_millis(30)).await;
    }
    controls.dismiss();
    drop(controls);
    std::fs::write(directory.join("layer-scroll-complete"), "done").unwrap();
    while !directory.join("layer-scroll-next").exists() {
        glib::timeout_future(Duration::from_millis(30)).await;
    }
}

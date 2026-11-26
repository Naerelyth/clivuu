//! Recording codecs and native-pixel cropping, shared by every capture backend.
use super::{Stream, contains};
use clivuu_core::{annotation::Bounds, i18n::t};
use gst::prelude::*;

pub(super) fn description(fps: u32) -> String {
    let threads = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(8);
    format!(
        "pipewiresrc name=source do-timestamp=true ! queue max-size-buffers=4 max-size-bytes=0 max-size-time=0 ! videocrop name=crop ! videoconvert ! videorate ! video/x-raw,framerate={fps}/1,format=Y444 ! vp9enc cpu-used=8 deadline=1 threads={threads} row-mt=true lag-in-frames=0 target-bitrate=0 end-usage=cq cq-level=0 min-quantizer=0 max-quantizer=0 ! matroskamux name=mux ! filesink name=output"
    )
}

fn audio_description(inputs: usize) -> String {
    // Each named output remains an independent track on the common pipeline clock.
    (0..inputs).map(|n| format!(
        "pulsesrc name=input{n} provide-clock=false do-timestamp=true ! queue max-size-buffers=0 max-size-bytes=0 max-size-time=200000000 ! audioconvert ! audioresample ! audio/x-raw,format=S16LE,rate=48000,channels=2 ! flacenc name=audio{n} quality=5 ! queue name=track{n} max-size-buffers=0 max-size-bytes=0 max-size-time=200000000 "
    )).collect()
}

pub(super) fn attach_audio(pipeline: &gst::Pipeline, devices: &[String]) -> Result<(), String> {
    if devices.is_empty() {
        return Ok(());
    }
    let audio = gst::parse::bin_from_description(&audio_description(devices.len()), false)
        .map_err(|e| e.to_string())?;
    for (n, device) in devices.iter().enumerate() {
        let input = audio
            .by_name(&format!("input{n}"))
            .ok_or_else(|| t("Missing audio input"))?;
        input.set_property("device", device);
        input.set_property("client-name", "Clivuu recording");
        let src = audio
            .by_name(&format!("track{n}"))
            .unwrap()
            .static_pad("src")
            .unwrap();
        let ghost = gst::GhostPad::builder_with_target(&src)
            .map_err(|e| e.to_string())?
            .name(format!("track{n}"))
            .build();
        audio.add_pad(&ghost).map_err(|e| e.to_string())?;
    }
    pipeline.add(&audio).map_err(|e| e.to_string())?;
    let mux = pipeline
        .by_name("mux")
        .ok_or_else(|| t("Missing recording muxer"))?;
    for n in 0..devices.len() {
        let sink = mux
            .request_pad_simple("audio_%u")
            .ok_or_else(|| t("No audio muxer pad"))?;
        audio
            .static_pad(&format!("track{n}"))
            .unwrap()
            .link(&sink)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(super) fn configure_crop(
    pipeline: &gst::Pipeline,
    stream: &Stream,
    rect: Bounds,
) -> Result<(), String> {
    let outer = stream.rect.filter(|r| contains(*r, rect)).ok_or_else(|| {
        t("The authorized monitor does not contain the region, or the portal omitted logical coordinates. Select the monitor containing the region.")
    })?;
    let crop = pipeline
        .by_name("crop")
        .ok_or_else(|| t("Cannot start the GStreamer recording pipeline"))?;
    let pad = crop
        .static_pad("sink")
        .ok_or_else(|| t("Cannot start the GStreamer recording pipeline"))?;
    let weak = crop.downgrade();
    pad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_, info| {
        let Some(event) = info.event() else {
            return gst::PadProbeReturn::Ok;
        };
        let gst::EventView::Caps(caps) = event.view() else {
            return gst::PadProbeReturn::Ok;
        };
        let Some(crop) = weak.upgrade() else {
            return gst::PadProbeReturn::Drop;
        };
        let margins = gst_video::VideoInfo::from_caps(caps.caps())
            .ok()
            .and_then(|v| pixel_margins(outer, rect, v.width(), v.height()));
        if let Some([left, top, right, bottom]) = margins {
            // Set all margins before the CAPS event negotiates the crop's
            // output. Never derive density from a GDK/XWayland surface scale.
            crop.set_properties(&[
                ("left", &left),
                ("top", &top),
                ("right", &right),
                ("bottom", &bottom),
            ]);
            gst::PadProbeReturn::Ok
        } else {
            gst::element_error!(
                crop,
                gst::StreamError::Format,
                ("{}", t("Invalid monitor layout"))
            );
            gst::PadProbeReturn::Drop
        }
    });
    Ok(())
}

fn pixel_margins(outer: Bounds, rect: Bounds, width: u32, height: u32) -> Option<[i32; 4]> {
    if !contains(outer, rect)
        || width == 0
        || height == 0
        || width > i32::MAX as u32
        || height > i32::MAX as u32
    {
        return None;
    }
    let sx = f64::from(width) / outer.width;
    let sy = f64::from(height) / outer.height;
    let left = ((rect.x - outer.x) * sx)
        .round()
        .clamp(0., f64::from(width)) as i32;
    let top = ((rect.y - outer.y) * sy)
        .round()
        .clamp(0., f64::from(height)) as i32;
    let end_x = ((rect.x + rect.width - outer.x) * sx)
        .round()
        .clamp(0., f64::from(width)) as i32;
    let end_y = ((rect.y + rect.height - outer.y) * sy)
        .round()
        .clamp(0., f64::from(height)) as i32;
    (end_x > left && end_y > top).then_some([
        left,
        top,
        width as i32 - end_x,
        height as i32 - end_y,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use clivuu_core::image::{Rgba, RgbaImage};
    use clivuu_core::recording::Format;
    use std::sync::{Arc, Mutex};
    use std::{fs, path::Path, process::Command};

    fn stream(rect: Bounds) -> Stream {
        Stream {
            node: 1,
            rect: Some(rect),
            properties: Default::default(),
        }
    }
    fn source(pipeline: &gst::Pipeline, width: u32, height: u32) -> gst_app::AppSrc {
        let src = pipeline
            .by_name("source")
            .unwrap()
            .downcast::<gst_app::AppSrc>()
            .unwrap();
        src.set_caps(Some(
            &gst_video::VideoInfo::builder(gst_video::VideoFormat::Rgba, width, height)
                .fps(gst::Fraction::new(24, 1))
                .build()
                .unwrap()
                .to_caps()
                .unwrap(),
        ));
        src
    }
    fn push(src: &gst_app::AppSrc, pixels: Vec<u8>, frame: u64) {
        let mut buffer = gst::Buffer::from_mut_slice(pixels);
        let b = buffer.get_mut().unwrap();
        b.set_pts(gst::ClockTime::from_nseconds(frame * 1_000_000_000 / 24));
        b.set_duration(gst::ClockTime::from_nseconds(1_000_000_000 / 24));
        src.push_buffer(buffer).unwrap();
    }
    fn finished(pipeline: &gst::Pipeline) {
        let message = pipeline
            .bus()
            .unwrap()
            .timed_pop_filtered(
                gst::ClockTime::from_seconds(30),
                &[gst::MessageType::Eos, gst::MessageType::Error],
            )
            .expect("encoder did not drain");
        pipeline.set_state(gst::State::Null).unwrap();
        if let gst::MessageView::Error(error) = message.view() {
            panic!("{} {:?}", error.error(), error.debug());
        }
    }
    #[test]
    fn delayed_live_audio_inputs_survive_start_pause_resume_and_eos() {
        gst::init().unwrap();
        let path = std::env::temp_dir().join(format!(
            "clivuu-delayed-audio-{}.webm",
            gtk::glib::uuid_string_random()
        ));
        let pipeline = gst::parse::launch(&description(24).replace(
            "pipewiresrc name=source do-timestamp=true",
            "videotestsrc name=source is-live=true",
        ))
        .unwrap()
        .downcast::<gst::Pipeline>()
        .unwrap();
        pipeline
            .by_name("output")
            .unwrap()
            .set_property("location", path.to_str().unwrap());
        let mut audio = audio_description(2);
        for n in 0..2 {
            audio = audio.replace(
                &format!("pulsesrc name=input{n} provide-clock=false do-timestamp=true"),
                &format!("audiotestsrc name=input{n} is-live=true ! identity sleep-time=300000"),
            );
        }
        let bin = gst::parse::bin_from_description(&audio, false).unwrap();
        for n in 0..2 {
            let pad = bin
                .by_name(&format!("track{n}"))
                .unwrap()
                .static_pad("src")
                .unwrap();
            let ghost = gst::GhostPad::builder_with_target(&pad)
                .unwrap()
                .name(format!("track{n}"))
                .build();
            bin.add_pad(&ghost).unwrap();
        }
        pipeline.add(&bin).unwrap();
        for n in 0..2 {
            bin.static_pad(&format!("track{n}"))
                .unwrap()
                .link(
                    &pipeline
                        .by_name("mux")
                        .unwrap()
                        .request_pad_simple("audio_%u")
                        .unwrap(),
                )
                .unwrap();
        }
        for target in [gst::State::Playing, gst::State::Paused, gst::State::Playing] {
            pipeline.set_state(target).unwrap();
            let (result, state, _) = pipeline.state(gst::ClockTime::from_seconds(3));
            result.unwrap();
            assert_eq!(state, target);
            assert!(
                !bin.by_name("track0")
                    .unwrap()
                    .static_pad("src")
                    .unwrap()
                    .pad_flags()
                    .contains(gst::PadFlags::EOS),
                "slow live inputs were incorrectly treated as end of stream"
            );
        }
        assert!(pipeline.send_event(gst::event::Eos::new()));
        finished(&pipeline);
        assert!(fs::metadata(&path).unwrap().len() > 0);
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn negotiated_native_crop_keeps_every_pixel_at_integer_and_fractional_densities() {
        gst::init().unwrap();
        let outer = Bounds {
            x: -192.,
            y: -40.,
            width: 192.,
            height: 128.,
        };
        let rect = Bounds {
            x: -181.,
            y: -33.,
            width: 99.,
            height: 61.,
        };
        let pipe = gst::parse::launch("appsrc name=source format=time ! videocrop name=crop ! video/x-raw,format=RGBA ! appsink name=frames sync=false").unwrap().downcast::<gst::Pipeline>().unwrap();
        configure_crop(&pipe, &stream(outer), rect).unwrap();
        let sink = pipe
            .by_name("frames")
            .unwrap()
            .downcast::<gst_app::AppSink>()
            .unwrap();
        pipe.set_state(gst::State::Playing).unwrap();
        // One stream renegotiates through 100%, 125%, 150%, 200% and back.
        // Expected edges are independent, explicit physical-pixel fixtures.
        for (n, (w, h, x, y, out_w, out_h)) in [
            (192, 128, 11, 7, 99, 61),
            (240, 160, 14, 9, 124, 76),
            (288, 192, 17, 11, 148, 91),
            (384, 256, 22, 14, 198, 122),
            (192, 128, 11, 7, 99, 61),
        ]
        .into_iter()
        .enumerate()
        {
            let pixels = RgbaImage::from_fn(w, h, |x, y| {
                Rgba([(x % 251) as u8, (y % 251) as u8, ((x + y) % 251) as u8, 255])
            });
            let src = source(&pipe, w, h);
            push(&src, pixels.clone().into_raw(), n as u64);
            let sample = sink
                .try_pull_sample(gst::ClockTime::from_seconds(5))
                .expect("cropped sample");
            let info = gst_video::VideoInfo::from_caps(sample.caps().unwrap()).unwrap();
            assert_eq!((info.width(), info.height()), (out_w, out_h));
            let frame =
                gst_video::VideoFrameRef::from_buffer_ref_readable(sample.buffer().unwrap(), &info)
                    .unwrap();
            let plane = frame.plane_data(0).unwrap();
            use gst_video::prelude::VideoFrameExt;
            let stride = frame.plane_stride()[0] as usize;
            for row in 0..out_h as usize {
                let offset = ((y as usize + row) * w as usize + x as usize) * 4;
                assert_eq!(
                    &plane[row * stride..row * stride + out_w as usize * 4],
                    &pixels.as_raw()[offset..offset + out_w as usize * 4],
                    "native source pixels were resampled"
                );
            }
        }
        pipe.set_state(gst::State::Null).unwrap();
        assert!(configure_crop(&pipe, &stream(outer), Bounds { x: 1., ..rect }).is_err());
        assert!(pixel_margins(outer, rect, 0, 128).is_none());
        assert!(
            pixel_margins(
                outer,
                Bounds {
                    width: f64::NAN,
                    ..rect
                },
                192,
                128
            )
            .is_none()
        );
    }

    #[test]
    fn fractional_mutter_area_with_odd_pixel_crop_encodes_plays_and_exports() {
        use crate::infrastructure::{recording_file::RecordingFile, tasks::TaskPool};
        use crate::platform::capture::session::mutter_area;
        gst::init().unwrap();
        let root = std::env::temp_dir().join(format!(
            "clivuu-fractional-odd-{}",
            gtk::glib::uuid_string_random()
        ));
        fs::create_dir(&root).unwrap();
        // 1.5x fractional selection: 1px and 2px physical offsets, 125x81 physical pixels.
        let scale = 1.5;
        let output = clivuu_core::capture::Output {
            rect: Bounds {
                x: 0.,
                y: 0.,
                width: 1920.,
                height: 1080.,
            },
            scale,
        };
        let rect = Bounds {
            x: 10. + 1. / scale,
            y: 20. + 2. / scale,
            width: 125. / scale,
            height: 81. / scale,
        };
        let (_, _, _, _, area) = mutter_area(rect, &[output]).unwrap();
        let stream_w = (area.width * scale).round() as u32;
        let stream_h = (area.height * scale).round() as u32;
        let off_x = ((rect.x - area.x) * scale).round() as u32;
        let off_y = ((rect.y - area.y) * scale).round() as u32;
        let source_frame = RgbaImage::from_fn(stream_w, stream_h, |x, y| {
            if (off_x..off_x + 125).contains(&x) && (off_y..off_y + 81).contains(&y) {
                Rgba([36, 104, 172, 255])
            } else {
                Rgba([220, 20, 40, 255])
            }
        });
        let pool = TaskPool::new(1, 2).unwrap();
        for format in [Format::Webm, Format::Mp4, Format::Gif] {
            let destination = root.join(format!("odd.{}", format.extension()));
            let file = RecordingFile::prepare(destination.clone(), true, 32, false).unwrap();
            let pipe = gst::parse::launch(&description(24).replace(
                "pipewiresrc name=source do-timestamp=true",
                "appsrc name=source format=time block=true",
            ))
            .unwrap()
            .downcast::<gst::Pipeline>()
            .unwrap();
            configure_crop(&pipe, &stream(area), rect).unwrap();
            pipe.by_name("output")
                .unwrap()
                .set_property("location", file.path.to_str().unwrap());
            let src = source(&pipe, stream_w, stream_h);
            pipe.set_state(gst::State::Playing).unwrap();
            for n in 0..16 {
                push(&src, source_frame.as_raw().clone(), n);
            }
            src.end_of_stream().unwrap();
            finished(&pipe);
            let info = super::super::export::probe(&file.path, &[]).unwrap();
            assert_eq!((info.width, info.height), (125, 81));
            let task = pool
                .submit(move |token| {
                    super::super::export::publish(&file, format, &[], &token)
                        .map_err(crate::infrastructure::tasks::TaskError::Failed)
                })
                .unwrap();
            gtk::glib::MainContext::new()
                .block_on(task.result())
                .unwrap();
            assert!(fs::metadata(&destination).unwrap().len() > 0);
        }
        fs::remove_dir_all(root).unwrap();
    }

    fn ffmpeg(args: &[&str]) -> std::process::Output {
        let out = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "info",
                "-nostdin",
                "-y",
                "-threads",
                "2",
                "-filter_complex_threads",
                "1",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        out
    }
    fn text_frame(frame: u32) -> RgbaImage {
        use gtk::cairo;
        let mut surface = cairo::ImageSurface::create(cairo::Format::Rgb24, 1440, 864).unwrap();
        {
            let cr = cairo::Context::new(&surface).unwrap();
            cr.set_source_rgb(0.11, 0.11, 0.13);
            cr.paint().unwrap();
            let layout = pangocairo::functions::create_layout(&cr);
            layout.set_font_description(Some(&gtk::pango::FontDescription::from_string("Sans 11")));
            for row in 0..17 {
                let y = f64::from(row * 64) - f64::from(frame * 7 % 64);
                for col in 0..5 {
                    let x = f64::from(col * 280 + 20);
                    cr.set_source_rgb(0.85, 0.70, 0.40);
                    cr.rectangle(x, y, 32., 24.);
                    cr.fill().unwrap();
                    layout.set_text(&format!(
                        "Project {:02}-{col} notes 2026.txt",
                        row + frame / 9
                    ));
                    cr.set_source_rgb(0.92, 0.92, 0.94);
                    cr.move_to(x + 40., y + 3.);
                    pangocairo::functions::show_layout(&cr, &layout);
                }
            }
        }
        surface.flush();
        let stride = surface.stride() as usize;
        let data = surface.data().unwrap();
        RgbaImage::from_fn(1440, 864, |x, y| {
            let i = y as usize * stride + x as usize * 4;
            let value = u32::from_ne_bytes(data[i..i + 4].try_into().unwrap());
            Rgba([(value >> 16) as u8, (value >> 8) as u8, value as u8, 255])
        })
    }
    fn ssim(reference: &Path, recorded: &Path, format: &str, dimensions: &str) -> f64 {
        let out = ffmpeg(&[
            "-f",
            "rawvideo",
            "-pixel_format",
            format,
            "-video_size",
            dimensions,
            "-framerate",
            "24",
            "-i",
            reference.to_str().unwrap(),
            "-i",
            recorded.to_str().unwrap(),
            "-lavfi",
            "[0:v]settb=AVTB,setpts=N/(24*TB)[r];[1:v]settb=AVTB,setpts=N/(24*TB)[v];[r][v]ssim",
            "-f",
            "null",
            "-",
        ]);
        let stderr = String::from_utf8_lossy(&out.stderr);
        stderr
            .split("All:")
            .nth(1)
            .expect("SSIM measurement")
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap()
    }
    fn checksums(pipeline: &gst::Pipeline) -> Arc<Mutex<Vec<String>>> {
        let samples = Arc::new(Mutex::new(Vec::new()));
        let output = samples.clone();
        pipeline
            .by_name("oracle")
            .unwrap()
            .downcast::<gst_app::AppSink>()
            .unwrap()
            .set_callbacks(
                gst_app::AppSinkCallbacks::builder()
                    .new_sample(move |sink| {
                        let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                        let buffer = sample.buffer().ok_or(gst::FlowError::Error)?;
                        let pixels = buffer.map_readable().map_err(|_| gst::FlowError::Error)?;
                        let mut checksum =
                            gtk::glib::Checksum::new(gtk::glib::ChecksumType::Sha256).unwrap();
                        checksum.update(pixels.as_slice());
                        output
                            .lock()
                            .unwrap()
                            .push(checksum.string().unwrap().to_string());
                        Ok(gst::FlowSuccess::Ok)
                    })
                    .build(),
            );
        samples
    }
    #[test]
    #[ignore = "requires GStreamer VP8/VP9, FFmpeg and screen fonts, writes quality artifacts under /tmp"]
    fn readable_text_and_lossless_intermediate_survive_all_final_formats() {
        use crate::infrastructure::{recording_file::RecordingFile, tasks::TaskPool};
        use std::io::Write;
        gst::init().unwrap();
        let root = std::env::temp_dir().join(format!(
            "clivuu-recording-quality-{}",
            gtk::glib::uuid_string_random()
        ));
        fs::create_dir(&root).unwrap();
        let reference = root.join("reference.rgba");
        let mut raw = fs::File::create(&reference).unwrap();
        let frames: Vec<_> = (0..48).map(text_frame).collect();
        for frame in &frames {
            raw.write_all(
                clivuu_core::image::imageops::crop_imm(frame, 80, 64, 1280, 720)
                    .to_image()
                    .as_raw(),
            )
            .unwrap();
        }
        drop(raw);
        let pool = TaskPool::new(1, 2).unwrap();
        let mut measurements = Vec::new();
        for format in [Format::Webm, Format::Mp4, Format::Gif] {
            let destination = root.join(format!("output.{}", format.extension()));
            let file = RecordingFile::prepare(destination.clone(), true, 128, false).unwrap();
            let mut description = description(24).replace(
                "pipewiresrc name=source do-timestamp=true",
                "appsrc name=source format=time block=true",
            );
            if format != Format::Webm {
                description = description.replace(" ! vp9enc ",
                    " ! tee name=reference reference. ! queue ! appsink name=oracle sync=false reference. ! queue ! vp9enc ");
            }
            let pipe = gst::parse::launch(&description)
                .unwrap()
                .downcast::<gst::Pipeline>()
                .unwrap();
            let original = (format != Format::Webm).then(|| checksums(&pipe));
            let outer = Bounds {
                x: -720.,
                y: -100.,
                width: 720.,
                height: 432.,
            };
            let rect = Bounds {
                x: -680.,
                y: -68.,
                width: 640.,
                height: 360.,
            };
            configure_crop(&pipe, &stream(outer), rect).unwrap();
            pipe.by_name("output")
                .unwrap()
                .set_property("location", file.path.to_str().unwrap());
            let src = source(&pipe, 1440, 864);
            pipe.set_state(gst::State::Playing).unwrap();
            for (n, frame) in frames.iter().enumerate() {
                push(&src, frame.as_raw().clone(), n as u64);
            }
            src.end_of_stream().unwrap();
            finished(&pipe);
            if format != Format::Webm {
                // Compare every decoded Y444 byte with the same source color
                // conversion, including frame order, rather than a size check.
                let decode = gst::parse::launch(
                    "filesrc name=file ! matroskademux ! vp9dec ! appsink name=oracle sync=false",
                )
                .unwrap()
                .downcast::<gst::Pipeline>()
                .unwrap();
                decode
                    .by_name("file")
                    .unwrap()
                    .set_property("location", file.path.to_str().unwrap());
                let decoded = checksums(&decode);
                decode.set_state(gst::State::Playing).unwrap();
                finished(&decode);
                let original = original.unwrap();
                // Videorate may close a fractional-nanosecond final segment
                // with one repeated frame; the codec must preserve it too.
                assert!(
                    (frames.len()..=frames.len() + 1).contains(&original.lock().unwrap().len())
                );
                assert_eq!(
                    *original.lock().unwrap(),
                    *decoded.lock().unwrap(),
                    "intermediate codec changed source pixels or frame order"
                );
            }
            let task = pool
                .submit(move |token| {
                    super::super::export::publish(&file, format, &[], &token)
                        .map_err(crate::infrastructure::tasks::TaskError::Failed)
                })
                .unwrap();
            gtk::glib::MainContext::new()
                .block_on(task.result())
                .unwrap();
            let score = ssim(&reference, &destination, "rgba", "1280x720");
            assert!(
                score > 0.99,
                "{format:?} lost readable text detail: {score}"
            );
            measurements.push(serde_json::json!({"format":format.label(),"ssim":score,"bytes":fs::metadata(&destination).unwrap().len()}));
        }
        fs::write(
            root.join("results.json"),
            serde_json::to_vec_pretty(&measurements).unwrap(),
        )
        .unwrap();
        fs::remove_file(reference).unwrap();
        eprintln!("recording quality artifacts: {}", root.display());
    }
}

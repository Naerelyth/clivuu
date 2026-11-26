//! GStreamer playback, independent of surfaces. At most one decoded frame waits
//! for GTK, seeking and pipeline state transitions run on the bounded worker.
use clivuu_core::{
    i18n::t,
    recording::{MaterialInfo, PlaybackEdit},
};
use gst::prelude::*;
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

pub(super) struct Frame {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub stride: usize,
}
pub(super) struct Player {
    pipeline: gst::Pipeline,
    frames: async_channel::Receiver<Frame>,
    playing: AtomicBool,
}
impl Player {
    pub fn open(path: &Path, info: &MaterialInfo) -> Result<Self, String> {
        Self::open_with_sink(path, info, "autoaudiosink")
    }
    fn open_with_sink(path: &Path, info: &MaterialInfo, audio_sink: &str) -> Result<Self, String> {
        gst::init().map_err(|e| e.to_string())?;
        // Demuxed tracks must buffer together. Independent small queues can
        // block the demuxer before a delayed audio branch receives its preroll
        // after a seek. Multiqueue coordinates tracks with a bounded byte cap.
        let mut description = String::from(
            "filesrc name=input ! matroskademux name=demux multiqueue name=streams max-size-bytes=16777216 max-size-time=2000000000 max-size-buffers=0 demux.video_0 ! streams.sink_0 streams.src_0 ! vp9dec ! videoconvert ! video/x-raw,format=RGBA ! appsink name=frames max-buffers=1 drop=true sync=true ",
        );
        if !info.tracks.is_empty() {
            description.push_str(&format!(
                "audiomixer name=mixer ! audioconvert ! audioresample ! scaletempo ! {audio_sink} "
            ));
            for n in 0..info.tracks.len() {
                let pad = n + 1;
                description.push_str(&format!("demux.audio_{n} ! streams.sink_{pad} streams.src_{pad} ! flacparse ! flacdec ! audioconvert ! audioresample ! audio/x-raw,format=F32LE,rate=48000,channels=2 ! volume name=gain{n} ! mixer. "));
            }
        }
        let pipeline = gst::parse::launch(&description)
            .map_err(|e| e.to_string())?
            .downcast::<gst::Pipeline>()
            .map_err(|_| t("Cannot open recording playback"))?;
        let result = (|| {
            pipeline.by_name("input").unwrap().set_property(
                "location",
                path.to_str()
                    .ok_or_else(|| t("Cannot open recording playback"))?,
            );
            let sink = pipeline
                .by_name("frames")
                .unwrap()
                .downcast::<gst_app::AppSink>()
                .unwrap();
            let (tx, rx) = async_channel::bounded(1);
            let old = rx.clone();
            let push = move |sample: gst::Sample| -> Result<gst::FlowSuccess, gst::FlowError> {
                let info =
                    gst_video::VideoInfo::from_caps(sample.caps().ok_or(gst::FlowError::Error)?)
                        .map_err(|_| gst::FlowError::Error)?;
                let buffer = sample.buffer().ok_or(gst::FlowError::Error)?;
                let map = buffer.map_readable().map_err(|_| gst::FlowError::Error)?;
                let frame = Frame {
                    bytes: map.as_slice().to_vec(),
                    width: info.width(),
                    height: info.height(),
                    stride: info.stride()[0] as usize,
                };
                let _ = old.try_recv();
                let _ = tx.try_send(frame);
                Ok(gst::FlowSuccess::Ok)
            };
            let preroll = push.clone();
            sink.set_callbacks(
                gst_app::AppSinkCallbacks::builder()
                    .new_preroll(move |s| {
                        preroll(s.pull_preroll().map_err(|_| gst::FlowError::Eos)?)
                    })
                    .new_sample(move |s| push(s.pull_sample().map_err(|_| gst::FlowError::Eos)?))
                    .build(),
            );
            pipeline
                .set_state(gst::State::Paused)
                .map_err(|e| e.to_string())?;
            let (state, current, _) = pipeline.state(gst::ClockTime::from_seconds(10));
            state.map_err(|e| {
                pipeline
                    .bus()
                    .and_then(|b| b.pop_filtered(&[gst::MessageType::Error]))
                    .and_then(|m| match m.view() {
                        gst::MessageView::Error(e) => Some(e.error().to_string()),
                        _ => None,
                    })
                    .unwrap_or_else(|| e.to_string())
            })?;
            if current != gst::State::Paused {
                return Err(t("Cannot open recording playback"));
            }
            Ok(Self {
                pipeline: pipeline.clone(),
                frames: rx,
                playing: AtomicBool::new(false),
            })
        })();
        if result.is_err() {
            let _ = pipeline.set_state(gst::State::Null);
        }
        result
    }
    pub fn frame(&self) -> Option<Frame> {
        self.frames.try_recv().ok()
    }
    pub fn position(&self) -> u64 {
        self.pipeline
            .query_position::<gst::ClockTime>()
            .map_or(0, |t| t.nseconds())
    }
    pub fn set_playing(&self, playing: bool) -> Result<(), String> {
        self.pipeline
            .set_state(if playing {
                gst::State::Playing
            } else {
                gst::State::Paused
            })
            .map_err(|e| e.to_string())?;
        self.playing.store(playing, Ordering::Relaxed);
        Ok(())
    }
    pub fn edit(
        &self,
        info: &MaterialInfo,
        edit: &PlaybackEdit,
        position: u64,
    ) -> Result<(), String> {
        edit.validate(info).map_err(t)?;
        self.pipeline
            .seek(
                edit.rate,
                gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE,
                gst::SeekType::Set,
                gst::ClockTime::from_nseconds(
                    position.clamp(edit.start_ns, edit.end_ns.saturating_sub(1)),
                ),
                gst::SeekType::Set,
                gst::ClockTime::from_nseconds(edit.end_ns),
            )
            .map_err(|e| e.to_string())
    }
    pub fn gains(&self, info: &MaterialInfo, edit: &PlaybackEdit) {
        for (n, role) in info.tracks.iter().enumerate() {
            if let Some(volume) = self.pipeline.by_name(&format!("gain{n}")) {
                volume.set_property("volume", edit.audio[role.index()].volume);
                volume.set_property("mute", edit.audio[role.index()].muted);
            }
        }
    }
    pub fn poll(&self) -> Result<bool, String> {
        let Some(bus) = self.pipeline.bus() else {
            return Ok(false);
        };
        let mut ended = false;
        while let Some(message) =
            bus.pop_filtered(&[gst::MessageType::Eos, gst::MessageType::Error])
        {
            match message.view() {
                gst::MessageView::Eos(_) => ended = true,
                gst::MessageView::Error(error) => return Err(error.error().to_string()),
                _ => {}
            }
        }
        Ok(ended)
    }
    pub fn stop(&self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}
impl Drop for Player {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clivuu_core::recording::AudioRole;
    use std::{
        process::Command,
        time::{Duration, Instant},
    };
    #[test]
    fn independent_flac_tracks_preroll_seek_gain_and_deliver_end_of_playback() {
        let path = std::env::temp_dir().join(format!(
            "clivuu-player-{}.mkv",
            gtk::glib::uuid_string_random()
        ));
        let out = Command::new("ffmpeg")
            .args([
                "-nostdin",
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=96x64:rate=12:duration=2",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:sample_rate=48000:duration=2",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=880:sample_rate=48000:duration=2",
                "-map",
                "0:v",
                "-map",
                "1:a",
                "-map",
                "2:a",
                "-c:v",
                "libvpx-vp9",
                "-lossless",
                "1",
                "-pix_fmt",
                "yuv444p",
                "-threads:v",
                "2",
                "-c:a",
                "flac",
            ])
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let info = super::super::export::probe(&path, &[AudioRole::System, AudioRole::Microphone])
            .unwrap();
        // This is the production playback graph with only the physical output
        // replaced by a clocked fake sink; no device discovery or capture occurs.
        let player = Player::open_with_sink(&path, &info, "fakesink sync=true").unwrap();
        let frame = player
            .frame()
            .expect("paused player must preroll its native-resolution frame");
        assert_eq!((frame.width, frame.height), (96, 64));
        let mut edit = PlaybackEdit::new(&info);
        edit.start_ns = 500_000_000;
        edit.end_ns = 1_500_000_000;
        edit.rate = 2.;
        edit.audio[0].volume = 0.25;
        edit.audio[1].muted = true;
        player.gains(&info, &edit);
        assert_eq!(
            player
                .pipeline
                .by_name("gain0")
                .unwrap()
                .property::<f64>("volume"),
            0.25
        );
        assert!(
            player
                .pipeline
                .by_name("gain1")
                .unwrap()
                .property::<bool>("mute")
        );
        player.edit(&info, &edit, edit.start_ns).unwrap();
        player.set_playing(true).unwrap();
        let start = Instant::now();
        loop {
            if player.poll().unwrap() {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "end event was lost or trimmed playback did not finish"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        player.set_playing(false).unwrap();
        player.edit(&info, &edit, edit.start_ns).unwrap();
        player.stop();
        drop(player);
        std::fs::remove_file(path).unwrap();
    }
}

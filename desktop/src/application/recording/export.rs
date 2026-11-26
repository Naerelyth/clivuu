//! Bounded, cancellable export of immutable recording material.
use super::*;
use clivuu_core::recording::{AudioRole, Format, MaterialInfo, PlaybackEdit};
use std::{
    ffi::OsString,
    fs,
    io::{Read, Seek, SeekFrom},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};
pub(super) fn encoders() -> Result<&'static str, String> {
    static ENCODERS: OnceLock<Result<String, String>> = OnceLock::new();
    ENCODERS
        .get_or_init(|| {
            let out = Command::new("ffmpeg")
                .args(["-hide_banner", "-encoders"])
                .stdin(Stdio::null())
                .output()
                .map_err(|_| t("Recording export requires FFmpeg"))?;
            if !out.status.success() {
                return Err(t("Recording export requires FFmpeg"));
            }
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        })
        .as_deref()
        .map_err(Clone::clone)
}
fn has_encoder(name: &str) -> Result<bool, String> {
    Ok(encoders()?
        .lines()
        .any(|l| l.split_whitespace().nth(1) == Some(name)))
}
fn mp4_encoder() -> Result<&'static str, String> {
    for name in ["libx264", "libopenh264", "mpeg4"] {
        if has_encoder(name)? {
            return Ok(name);
        }
    }
    Err(t("FFmpeg provides no supported MP4 encoder"))
}
pub(super) fn check(format: Format) -> Result<Format, String> {
    encoders()?;
    match format {
        Format::Mp4 => {
            mp4_encoder()?;
        }
        Format::Webm if !has_encoder("libvpx-vp9")? => {
            return Err(t("FFmpeg provides no supported WebM encoder"));
        }
        _ => {}
    }
    Ok(format)
}
pub(super) fn check_audio(format: Format) -> Result<(), String> {
    let encoder = match format {
        Format::Mp4 => "aac",
        Format::Webm => "libopus",
        Format::Gif => return Ok(()),
    };
    if !has_encoder(encoder)? {
        return Err(t("FFmpeg provides no supported recording audio encoder"));
    }
    Ok(())
}
struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
/// Metadata is bounded and queried on the worker after acquisition has closed.
pub(super) fn probe(path: &Path, roles: &[AudioRole]) -> Result<MaterialInfo, String> {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type,width,height:format=duration",
            "-of",
            "json",
        ])
        .arg(path)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() || out.stdout.len() > 65536 {
        return Err(t("Cannot read recording material"));
    }
    let value: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    let streams = value["streams"]
        .as_array()
        .ok_or_else(|| t("Cannot read recording material"))?;
    let video = streams
        .iter()
        .find(|s| s["codec_type"] == "video")
        .ok_or_else(|| t("Cannot read recording material"))?;
    let duration = value["format"]["duration"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|s| s.is_finite() && *s > 0. && *s <= 86400.)
        .ok_or_else(|| t("Cannot read recording material"))?;
    let width = video["width"]
        .as_u64()
        .filter(|w| *w > 0 && *w <= 32768)
        .ok_or_else(|| t("Cannot read recording material"))? as u32;
    let height = video["height"]
        .as_u64()
        .filter(|h| *h > 0 && *h <= 32768)
        .ok_or_else(|| t("Cannot read recording material"))? as u32;
    if streams
        .iter()
        .filter(|s| s["codec_type"] == "audio")
        .count()
        != roles.len()
    {
        return Err(t(
            "Recording audio tracks do not match the material manifest",
        ));
    }
    Ok(MaterialInfo {
        width,
        height,
        duration_ns: (duration * 1e9).round() as u64,
        tracks: roles.to_vec(),
    })
}
fn run(
    args: &[OsString],
    file: &RecordingFile,
    target: &Path,
    token: &crate::infrastructure::tasks::Cancellation,
    progress: &AtomicU64,
    duration_ns: u64,
) -> Result<(), String> {
    token.check().map_err(|e| e.to_string())?;
    let log = target.with_file_name("encode.log");
    let report = target.with_file_name("encode.progress");
    let stderr = fs::File::create(&log).map_err(|e| e.to_string())?;
    let mut child = Process(
        Command::new("ffmpeg")
            .args([
                "-nostdin",
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-threads",
                "2",
                "-filter_threads",
                "1",
                "-filter_complex_threads",
                "1",
                "-progress",
            ])
            .arg(&report)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(stderr)
            .spawn()
            .map_err(|e| e.to_string())?,
    );
    let result = loop {
        if let Err(e) = token.check() {
            break Err(e.to_string());
        }
        if let Err(e) = file.export_space(target) {
            break Err(e);
        }
        if let Ok(mut f) = fs::File::open(&report) {
            let _ = f
                .seek(SeekFrom::End(-8192))
                .or_else(|_| f.seek(SeekFrom::Start(0)));
            let mut text = String::new();
            let _ = f.take(8192).read_to_string(&mut text);
            if let Some(us) = text
                .lines()
                .filter_map(|l| {
                    l.strip_prefix("out_time_us=")
                        .and_then(|v| v.parse::<u64>().ok())
                })
                .next_back()
            {
                progress.store(
                    us.saturating_mul(1000)
                        .saturating_mul(1000)
                        .checked_div(duration_ns)
                        .unwrap_or(0)
                        .min(999),
                    Ordering::Relaxed,
                );
            }
        }
        match child.0.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    break Ok(());
                }
                let mut message = String::new();
                let _ =
                    fs::File::open(&log).and_then(|f| f.take(8192).read_to_string(&mut message));
                break Err(format!(
                    "{}: {}",
                    t("Recording export failed"),
                    message.trim()
                ));
            }
            Err(e) => break Err(e.to_string()),
            _ => std::thread::sleep(Duration::from_millis(50)),
        }
    };
    drop(child);
    let _ = fs::remove_file(log);
    let _ = fs::remove_file(report);
    result
}
fn tempo(rate: f64) -> String {
    let mut rate = rate;
    let mut filters = vec![];
    while rate < 0.5 {
        filters.push("atempo=0.5".to_owned());
        rate /= 0.5;
    }
    filters.push(format!("atempo={rate}"));
    filters.join(",")
}
fn video_filter(edit: &PlaybackEdit) -> String {
    format!(
        "trim=start={}:end={},setpts=(PTS-{}/TB)/{},crop={}:{}:{}:{}",
        edit.start_ns as f64 / 1e9,
        edit.end_ns as f64 / 1e9,
        edit.start_ns as f64 / 1e9,
        edit.rate,
        edit.crop.width,
        edit.crop.height,
        edit.crop.x,
        edit.crop.y
    )
}
fn audio_filter(info: &MaterialInfo, edit: &PlaybackEdit) -> String {
    let mut filters = String::new();
    for (n, role) in info.tracks.iter().enumerate() {
        filters.push_str(&format!(
            ";[0:a:{n}]atrim=start={}:end={},asetpts=PTS-{}/TB,aresample=async=1:first_pts=0,volume={},{}[a{n}]",
            edit.start_ns as f64 / 1e9,
            edit.end_ns as f64 / 1e9,
            edit.start_ns as f64 / 1e9,
            edit.audio[role.index()].effective(),
            tempo(edit.rate)
        ));
    }
    if !info.tracks.is_empty() {
        filters.push(';');
        for n in 0..info.tracks.len() {
            filters.push_str(&format!("[a{n}]"));
        }
        filters.push_str(&format!(
            "amix=inputs={}:normalize=0:duration=longest[a]",
            info.tracks.len()
        ));
    }
    filters
}
/// One private destination-volume staging directory per export, independent of
/// the capture medium and ceiling. Drop runs on the worker, including failures.
struct Workspace(PathBuf);
impl Workspace {
    fn new(destination: &Path) -> Result<Self, String> {
        use std::os::unix::fs::DirBuilderExt;
        let parent = destination
            .parent()
            .filter(|_| destination.is_absolute())
            .ok_or_else(|| t("The recording output must be an absolute path"))?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let path = parent.join(format!(".clivuu-export-{}", glib::uuid_string_random()));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&path)
            .map_err(|e| e.to_string())?;
        Ok(Self(path))
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
pub(super) struct Export<'a> {
    pub info: &'a MaterialInfo,
    pub edit: &'a PlaybackEdit,
    pub format: Format,
    pub destination: &'a Path,
    pub replace: bool,
    pub progress: &'a AtomicU64,
}
/// Successful exports commit a new file, never consume or rewrite the material.
pub(super) fn encode(
    file: &RecordingFile,
    export: Export<'_>,
    token: &crate::infrastructure::tasks::Cancellation,
) -> Result<PathBuf, String> {
    let Export {
        info,
        edit,
        format,
        destination,
        replace,
        progress,
    } = export;
    edit.validate(info).map_err(t)?;
    check(format)?;
    if !info.tracks.is_empty() {
        check_audio(format)?;
    }
    token.check().map_err(|e| e.to_string())?;
    let workspace = Workspace::new(destination)?;
    let target = workspace.0.join(format!("export.{}", format.extension()));
    let palette = workspace.0.join("palette.png");
    let values = |v: &[&str]| v.iter().map(OsString::from).collect::<Vec<_>>();
    let mut args = values(&["-i"]);
    args.push(file.path.clone().into_os_string());
    let duration_ns = edit.output_duration().as_nanos().min(u64::MAX as u128) as u64;
    let result = (|| {
        if format == Format::Gif {
            let mut a = args.clone();
            a.extend(values(&[
                "-vf",
                &format!("{},palettegen=stats_mode=diff", video_filter(edit)),
                "-an",
                "-frames:v",
                "1",
            ]));
            a.push(palette.clone().into_os_string());
            run(&a, file, &target, token, progress, duration_ns)?;
            args.extend(values(&["-i"]));
            args.push(palette.clone().into_os_string());
            args.extend(values(&[
                "-filter_complex",
                &format!(
                    "[0:v]{}[v];[v][1:v]paletteuse=dither=sierra2_4a[out]",
                    video_filter(edit)
                ),
                "-map",
                "[out]",
                "-an",
                "-loop",
                "0",
            ]));
        } else {
            let graph = format!(
                "[0:v]{}{}[v]{}",
                video_filter(edit),
                if format == Format::Mp4 {
                    ",pad=ceil(iw/2)*2:ceil(ih/2)*2"
                } else {
                    ""
                },
                audio_filter(info, edit)
            );
            args.extend(values(&["-filter_complex", &graph, "-map", "[v]"]));
            if !info.tracks.is_empty() {
                args.extend(values(&[
                    "-map",
                    "[a]",
                    "-c:a",
                    if format == Format::Mp4 {
                        "aac"
                    } else {
                        "libopus"
                    },
                    "-b:a",
                    "192k",
                ]));
            }
            if format == Format::Mp4 {
                let encoder = mp4_encoder()?;
                args.extend(values(&[
                    "-c:v",
                    encoder,
                    "-pix_fmt",
                    "yuv420p",
                    "-movflags",
                    "+faststart",
                ]));
                args.extend(if encoder == "libx264" {
                    values(&["-preset", "fast", "-crf", "20"])
                } else if encoder == "libopenh264" {
                    values(&["-b:v", "6000k", "-qmin", "18", "-qmax", "26"])
                } else {
                    values(&["-q:v", "3"])
                });
            } else {
                args.extend(values(&[
                    "-c:v",
                    "libvpx-vp9",
                    "-b:v",
                    "0",
                    "-crf",
                    "17",
                    "-deadline",
                    "good",
                    "-cpu-used",
                    "4",
                    "-pix_fmt",
                    "yuv444p",
                ]));
            }
        }
        args.extend(values(&["-threads:v", "2"]));
        args.push(target.clone().into_os_string());
        run(&args, file, &target, token, progress, duration_ns)?;
        token.check().map_err(|e| e.to_string())?;
        let path = file.publish_export(&target, destination, replace)?;
        progress.store(1000, Ordering::Relaxed);
        Ok(path)
    })();
    let _ = fs::remove_file(palette);
    let _ = fs::remove_file(target);
    result
}
pub(super) fn publish(
    file: &RecordingFile,
    format: Format,
    roles: &[AudioRole],
    token: &crate::infrastructure::tasks::Cancellation,
) -> Result<PathBuf, String> {
    let info = probe(&file.path, roles)?;
    let edit = PlaybackEdit::new(&info);
    let path = encode(
        file,
        Export {
            info: &info,
            edit: &edit,
            format,
            destination: &file.destination,
            replace: false,
            progress: &AtomicU64::new(0),
        },
        token,
    )?;
    file.discard()?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn command(args: &[&str]) -> Vec<u8> {
        let out = Command::new("ffmpeg")
            .args([
                "-nostdin",
                "-y",
                "-v",
                "error",
                "-threads",
                "2",
                "-filter_threads",
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
        out.stdout
    }
    fn amplitude(path: &Path, frequency: f64) -> f64 {
        let bytes = command(&[
            "-i",
            path.to_str().unwrap(),
            "-vn",
            "-ac",
            "1",
            "-ar",
            "48000",
            "-f",
            "f32le",
            "-",
        ]);
        let samples = bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()) as f64)
            .collect::<Vec<_>>();
        let samples = &samples[4800..samples.len().min(43200)];
        let (sin, cos) = samples.iter().enumerate().fold((0., 0.), |(a, b), (n, x)| {
            let phase = std::f64::consts::TAU * frequency * n as f64 / 48000.;
            (a + x * phase.sin(), b + x * phase.cos())
        });
        2. * sin.hypot(cos) / samples.len() as f64
    }
    #[test]
    fn delayed_audio_keeps_its_offset_from_the_common_trim_origin() {
        let root = std::env::temp_dir().join(format!(
            "clivuu-delayed-track-{}",
            glib::uuid_string_random()
        ));
        fs::create_dir(&root).unwrap();
        let file =
            Arc::new(RecordingFile::prepare(root.join("edited.mp4"), true, 16, false).unwrap());
        command(&[
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=160x96:rate=12:duration=3",
            "-itsoffset",
            "1",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:sample_rate=48000:duration=2",
            "-map",
            "0:v",
            "-map",
            "1:a",
            "-c:v",
            "libvpx-vp9",
            "-lossless",
            "1",
            "-threads:v",
            "2",
            "-c:a",
            "flac",
            file.path.to_str().unwrap(),
        ]);
        let info = probe(&file.path, &[AudioRole::Microphone]).unwrap();
        let mut edit = PlaybackEdit::new(&info);
        edit.start_ns = 500_000_000;
        edit.end_ns = 2_500_000_000;
        edit.rate = 2.;
        edit.audio[0].muted = true;
        edit.audio[1].volume = 0.5;
        let owned = file.clone();
        let destination = root.join("edited.mp4");
        let dest = destination.clone();
        let pool = TaskPool::new(1, 1).unwrap();
        let task = pool
            .submit(move |token| {
                encode(
                    &owned,
                    Export {
                        info: &info,
                        edit: &edit,
                        format: Format::Mp4,
                        destination: &dest,
                        replace: false,
                        progress: &AtomicU64::new(0),
                    },
                    &token,
                )
                .map_err(TaskError::Failed)
            })
            .unwrap();
        glib::MainContext::new().block_on(task.result()).unwrap();
        let bytes = command(&[
            "-i",
            destination.to_str().unwrap(),
            "-vn",
            "-ac",
            "1",
            "-ar",
            "48000",
            "-f",
            "f32le",
            "-",
        ]);
        let samples = bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()) as f64)
            .collect::<Vec<_>>();
        let rms = |a: usize, b: usize| {
            (samples[a..b].iter().map(|v| v * v).sum::<f64>() / (b - a) as f64).sqrt()
        };
        assert!(
            rms(0, 7200) < 0.001,
            "delayed microphone shifted before its common timeline offset"
        );
        assert!(
            rms(24000, 38400) > 0.025,
            "typed microphone gain or delayed content was lost"
        );
        file.discard().unwrap();
        drop(file);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn independent_tracks_trim_crop_speed_gain_and_gif_exports_preserve_the_material() {
        let root = std::env::temp_dir().join(format!(
            "clivuu-editable-record-{}",
            glib::uuid_string_random()
        ));
        fs::create_dir(&root).unwrap();
        let file =
            Arc::new(RecordingFile::prepare(root.join("quick.mp4"), true, 128, false).unwrap());
        command(&[
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=160x96:rate=12:duration=4",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=4",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:sample_rate=48000:duration=4",
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
            file.path.to_str().unwrap(),
        ]);
        let roles = [AudioRole::System, AudioRole::Microphone];
        let info = probe(&file.path, &roles).unwrap();
        assert_eq!((info.width, info.height, info.tracks.len()), (160, 96, 2));
        let original = fs::read(&file.path).unwrap();
        let mut edit = PlaybackEdit::new(&info);
        edit.start_ns = 1_000_000_000;
        edit.end_ns = 3_000_000_000;
        edit.rate = 2.;
        edit.crop = clivuu_core::recording::Crop {
            x: 7,
            y: 9,
            width: 100,
            height: 60,
        };
        edit.audio[0].volume = 0.25;
        edit.audio[1].muted = true;
        let pool = TaskPool::new(1, 2).unwrap();
        for format in [Format::Gif, Format::Mp4, Format::Webm] {
            let file = file.clone();
            let info = info.clone();
            let edit = edit.clone();
            let destination = root.join(format!("edited.{}", format.extension()));
            let out = destination.clone();
            let task = pool
                .submit(move |token| {
                    encode(
                        &file,
                        Export {
                            info: &info,
                            edit: &edit,
                            format,
                            destination: &out,
                            replace: false,
                            progress: &AtomicU64::new(0),
                        },
                        &token,
                    )
                    .map_err(TaskError::Failed)
                })
                .unwrap();
            glib::MainContext::new().block_on(task.result()).unwrap();
            let meta = Command::new("ffprobe")
                .args([
                    "-v",
                    "error",
                    "-show_streams",
                    "-show_format",
                    "-of",
                    "json",
                ])
                .arg(&destination)
                .output()
                .unwrap();
            let meta: serde_json::Value = serde_json::from_slice(&meta.stdout).unwrap();
            let video = &meta["streams"][0];
            assert_eq!(
                (video["width"].as_u64(), video["height"].as_u64()),
                (Some(100), Some(60))
            );
            let duration = meta["format"]["duration"]
                .as_str()
                .unwrap()
                .parse::<f64>()
                .unwrap();
            assert!((duration - 1.).abs() < 0.2, "{format:?}: {duration}");
            if format != Format::Gif {
                assert_eq!(meta["streams"].as_array().unwrap().len(), 2);
                assert!(amplitude(&destination, 440.) > 0.025);
                assert!(amplitude(&destination, 880.) < 0.003);
            }
        }
        assert_eq!(fs::read(&file.path).unwrap(), original);
        assert_eq!(probe(&file.path, &roles).unwrap().tracks, roles);
        let protected = root.join("existing.mp4");
        fs::write(&protected, b"keep me").unwrap();
        let owned = file.clone();
        let info2 = info.clone();
        let edit2 = edit.clone();
        let dest = protected.clone();
        let task = pool
            .submit(move |token| {
                token.cancel();
                encode(
                    &owned,
                    Export {
                        info: &info2,
                        edit: &edit2,
                        format: Format::Mp4,
                        destination: &dest,
                        replace: true,
                        progress: &AtomicU64::new(0),
                    },
                    &token,
                )
                .map_err(TaskError::Failed)
            })
            .unwrap();
        assert!(glib::MainContext::new().block_on(task.result()).is_err());
        assert_eq!(fs::read(protected).unwrap(), b"keep me");
        assert_eq!(fs::read(&file.path).unwrap(), original);
        file.discard().unwrap();
        drop(file);
        fs::remove_dir_all(root).unwrap();
    }
}

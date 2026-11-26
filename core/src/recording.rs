//! Recording controls and timeline, independent of display and encoder adapters.
use std::time::Duration;

pub const FRAME_RATES: [u32; 5] = [5, 16, 24, 30, 60];
pub const DELAYS: [u32; 6] = [0, 1, 2, 3, 5, 10];
pub const PLAYBACK_RATES: [f64; 10] = [0.2, 0.25, 0.5, 0.75, 0.8, 1., 1.25, 1.5, 1.75, 2.];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Normal,
    Quick,
}
impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Normal recording",
            Self::Quick => "Quick recording",
        }
    }
}

/// Audio track indices follow this manifest, never device availability at playback.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AudioRole {
    System,
    Microphone,
}
impl AudioRole {
    pub fn index(self) -> usize {
        match self {
            Self::System => 0,
            Self::Microphone => 1,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System audio",
            Self::Microphone => "Microphone",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Crop {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct MaterialInfo {
    pub width: u32,
    pub height: u32,
    pub duration_ns: u64,
    pub tracks: Vec<AudioRole>,
}
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Gain {
    pub volume: f64,
    pub muted: bool,
}
impl Default for Gain {
    fn default() -> Self {
        Self {
            volume: 1.,
            muted: false,
        }
    }
}
impl Gain {
    pub fn effective(self) -> f64 {
        if self.muted { 0. } else { self.volume }
    }
}
/// One immutable snapshot drives both live playback and final encoding.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PlaybackEdit {
    pub start_ns: u64,
    pub end_ns: u64,
    pub rate: f64,
    pub crop: Crop,
    pub audio: [Gain; 2],
}
impl PlaybackEdit {
    pub fn new(info: &MaterialInfo) -> Self {
        Self {
            start_ns: 0,
            end_ns: info.duration_ns,
            rate: 1.,
            crop: Crop {
                x: 0,
                y: 0,
                width: info.width,
                height: info.height,
            },
            audio: [Gain::default(); 2],
        }
    }
    pub fn validate(&self, info: &MaterialInfo) -> Result<(), &'static str> {
        if self.start_ns >= self.end_ns
            || self.end_ns > info.duration_ns
            || !self.rate.is_finite()
            || !(0.2..=2.).contains(&self.rate)
            || self.crop.width == 0
            || self.crop.height == 0
            || self
                .crop
                .x
                .checked_add(self.crop.width)
                .is_none_or(|r| r > info.width)
            || self
                .crop
                .y
                .checked_add(self.crop.height)
                .is_none_or(|b| b > info.height)
            || self
                .audio
                .iter()
                .any(|g| !g.volume.is_finite() || !(0.0..=2.0).contains(&g.volume))
        {
            return Err("Invalid recording playback settings");
        }
        Ok(())
    }
    pub fn output_duration(&self) -> Duration {
        Duration::from_secs_f64(self.end_ns.saturating_sub(self.start_ns) as f64 / 1e9 / self.rate)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AudioAvailability {
    pub system: bool,
    pub microphone: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Webm,
    Mp4,
    Gif,
}
impl Format {
    pub fn label(self) -> &'static str {
        match self {
            Self::Webm => "WebM",
            Self::Mp4 => "MP4",
            Self::Gif => "GIF",
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            Self::Webm => "webm",
            Self::Mp4 => "mp4",
            Self::Gif => "gif",
        }
    }
    pub fn from_path(path: &std::path::Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?;
        [Self::Webm, Self::Mp4, Self::Gif]
            .into_iter()
            .find(|f| ext.eq_ignore_ascii_case(f.extension()))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub fps: u32,
    pub delay: u32,
    pub mode: Mode,
    pub cursor: bool,
    pub system_audio: bool,
    pub microphone: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            fps: 24,
            delay: 0,
            mode: Mode::Normal,
            cursor: true,
            system_audio: true,
            microphone: true,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Ready,
    Preparing,
    Countdown(u32),
    Starting,
    Recording,
    Paused,
    Finishing,
    Exporting,
}
impl Phase {
    pub fn name(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Preparing => "preparing",
            Self::Countdown(_) => "countdown",
            Self::Starting => "starting",
            Self::Recording => "recording",
            Self::Paused => "paused",
            Self::Finishing => "finishing",
            Self::Exporting => "exporting",
        }
    }
    pub fn editable(self) -> bool {
        self == Self::Ready
    }
    /// Keep overlapping controls out before requesting the first stream frame.
    pub fn captures(self) -> bool {
        matches!(
            self,
            Self::Preparing
                | Self::Countdown(_)
                | Self::Starting
                | Self::Recording
                | Self::Finishing
        )
    }
}

/// Monotonic active time, excluding every paused span. The caller supplies time
/// so model tests do not need a real clock or sleeps.
#[derive(Default)]
pub struct Timeline {
    elapsed: Duration,
    since: Option<Duration>,
}
impl Timeline {
    pub fn resume(&mut self, now: Duration) {
        self.since.get_or_insert(now);
    }
    pub fn pause(&mut self, now: Duration) {
        if let Some(since) = self.since.take() {
            self.elapsed += now.saturating_sub(since);
        }
    }
    pub fn elapsed(&self, now: Duration) -> Duration {
        self.elapsed
            + self
                .since
                .map_or(Duration::ZERO, |since| now.saturating_sub(since))
    }
}
pub fn elapsed_label(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    if seconds >= 3600 {
        format!(
            "{:02}:{:02}:{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        )
    } else {
        format!("{:02}:{:02}", seconds / 60, seconds % 60)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn playback_edits_preserve_material_and_validate_native_crop_and_timeline() {
        let info = MaterialInfo {
            width: 1921,
            height: 1081,
            duration_ns: 10_000_000_000,
            tracks: vec![AudioRole::Microphone],
        };
        let mut edit = PlaybackEdit::new(&info);
        edit.start_ns = 2_000_000_000;
        edit.end_ns = 8_000_000_000;
        edit.rate = 0.2;
        edit.audio[AudioRole::Microphone.index()] = Gain {
            volume: 1.5,
            muted: true,
        };
        assert_eq!(edit.output_duration().as_secs(), 30);
        assert_eq!(edit.audio[1].effective(), 0.);
        assert!(edit.validate(&info).is_ok());
        assert_eq!(info.tracks, [AudioRole::Microphone]);
        edit.crop.x = u32::MAX;
        assert!(edit.validate(&info).is_err());
        edit.crop.x = 0;
        edit.rate = f64::NAN;
        assert!(edit.validate(&info).is_err());
        edit.rate = 1.;
        edit.start_ns = edit.end_ns;
        assert!(edit.validate(&info).is_err());
    }
    #[test]
    fn paused_time_is_excluded_and_transitions_are_idempotent() {
        let mut clock = Timeline::default();
        clock.resume(Duration::from_secs(10));
        clock.resume(Duration::from_secs(12));
        clock.pause(Duration::from_secs(14));
        clock.pause(Duration::from_secs(18));
        assert_eq!(clock.elapsed(Duration::from_secs(50)).as_secs(), 4);
        clock.resume(Duration::from_secs(50));
        assert_eq!(clock.elapsed(Duration::from_secs(53)).as_secs(), 7);
        assert_eq!(elapsed_label(Duration::from_secs(3671)), "01:01:11");
    }
}

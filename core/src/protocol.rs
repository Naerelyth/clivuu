//! Versioned JSON messages transported over Desktop1.Execute(s) -> s.
use crate::i18n::t;
use serde::{Deserialize, Serialize};

pub const VERSION: u32 = 1;
pub const INTERFACE: &str = "io.github.naerelyth.Clivuu.Desktop1";
pub const OBJECT_PATH: &str = "/io/github/naerelyth/Clivuu/Desktop";
pub const DESKTOP_DESTINATION: &str = "io.github.naerelyth.Clivuu.Desktop";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: i64,
    pub y: i64,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub fn validate(self) -> crate::Result<Self> {
        if self.width < 2
            || self.height < 2
            || self.x.checked_add(i64::from(self.width)).is_none()
            || self.y.checked_add(i64::from(self.height)).is_none()
        {
            return Err(t("Invalid selection dimensions or coordinate overflow").into());
        }
        Ok(self)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Capabilities {
    pub protocol: u32,
    pub backend: String,
    pub capture: bool,
    pub non_focusing_pins: bool,
    pub smart_windows: bool,
    pub scrolling: bool,
    pub recording: bool,
    pub settings: bool,
}

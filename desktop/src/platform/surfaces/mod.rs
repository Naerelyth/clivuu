//! Platform-owned desktop surfaces. Editors are children, never managed windows.
mod backend;
pub mod capture;
mod layer_shell;
mod runtime;
mod x11;
pub use backend::{MenuWatch, PinRole};
pub use x11::CursorThemeWatch;

pub mod pins;

pub mod authorization;

pub mod menu;

//! Shared editor state. No toolkit objects, display connections, timers or IPC.
//! Coordinates are document units; adapters supply text metrics and input time.
pub mod arrow;
pub mod color;
mod geometry;
mod history;
mod model;
pub mod sequence;
mod session;

pub use geometry::{Bounds, Handle, HandleKind, PointMapping, distance};
pub use history::History;
pub use model::{Fill, Kind, Mark, Point, Style, Transform};
pub use session::{EditorSession, EditorState, StylePatch};
#[cfg(test)]
mod tests;

//! Image processing without a compositor, display, GTK, Qt or D-Bus connection.
pub mod annotation;
pub mod capture;
pub mod compose;
pub mod config;
pub mod i18n;
pub mod image;
pub mod image_ops;
pub mod ocr;
pub mod pins;
pub mod protocol;
pub mod stitch;

pub use image_ops::Result;

pub mod scrolling;

pub mod saving;

pub mod paste;

pub mod recording;

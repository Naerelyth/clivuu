//! Application-provided services; widgets do not choose platform APIs.
use crate::{platform::portal, ui::color::ColorServices};
use std::rc::Rc;
pub fn color_services() -> ColorServices {
    ColorServices {
        pick: Some(Rc::new(|cancel| Box::pin(portal::pick_color(cancel)))),
        ..Default::default()
    }
}

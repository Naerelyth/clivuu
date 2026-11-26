//! Reversible display conversion shared by pins, preview, export and storage.
use gtk::{graphene, prelude::*};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    pub grayscale: bool,
    pub invert: bool,
}
impl Filter {
    pub fn push(self, s: &gtk::Snapshot) -> bool {
        if !self.grayscale && !self.invert {
            return false;
        }
        let sign = if self.invert { -1. } else { 1. };
        let (r, g, b) = (0.2126, 0.7152, 0.0722);
        let mut m = if self.grayscale {
            [r, r, r, 0., g, g, g, 0., b, b, b, 0., 0., 0., 0., 1.]
        } else {
            [
                1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
            ]
        };
        for v in &mut m[..12] {
            *v *= sign;
        }
        let offset = if self.invert { 1. } else { 0. };
        s.push_color_matrix(
            &graphene::Matrix::from_float(m),
            &graphene::Vec4::new(offset, offset, offset, 0.),
        );
        true
    }
}

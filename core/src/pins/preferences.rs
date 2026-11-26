//! Pin preferences and geometry policy, without display or toolkit objects.
use super::Pin;
use serde_json::{Map, Value};

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct TextOptions {
    pub enabled: bool,
    pub ignore_html: bool,
    pub skip_paths: bool,
    pub margin: u32,
    pub max_width: u32,
    pub font: String,
    pub background: String,
    pub foreground: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Preferences {
    pub shadow: bool,
    pub shadow_tint: String,
    pub opacity: f64,
    pub max_size: f64,
    pub thumbnail: [f64; 2],
    pub use_snip_position: bool,
    pub zoom_at_cursor: bool,
    pub smooth: bool,
    pub reset_thumbnail: bool,
    pub translucent_flash: bool,
    pub flash_group: bool,
    pub hide_on_flash: bool,
    pub preserve_drag: bool,
    pub confirm_close: bool,
    pub confirm_destroy: bool,
    pub flash_new: bool,
    pub activate_new: bool,
    pub gnome_focus: bool,
    pub closed_limit: usize,
    pub text: TextOptions,
    pub mouse: super::mouse::Bindings,
}
impl Default for Preferences {
    fn default() -> Self {
        Self::from_values(&Map::new())
    }
}
impl Preferences {
    pub fn from_values(values: &Map<String, Value>) -> Self {
        let schema = crate::config::schema();
        let get = |key: &str| values.get(key).unwrap_or(&schema[key]["default"]);
        let b = |key| get(key).as_bool().expect("validated pin preference");
        let n = |key| get(key).as_u64().expect("validated pin preference") as u32;
        let s = |key| {
            get(key)
                .as_str()
                .expect("validated pin preference")
                .to_owned()
        };
        Self {
            shadow: b("pin-shadow"),
            shadow_tint: s("pin-shadow-tint"),
            opacity: n("pin-opacity") as f64 / 100.,
            max_size: n("pin-max-size") as f64,
            thumbnail: [
                n("pin-thumbnail-width") as f64,
                n("pin-thumbnail-height") as f64,
            ],
            use_snip_position: b("pin-use-snip-position"),
            zoom_at_cursor: b("pin-zoom-at-cursor"),
            smooth: b("pin-smooth-scaling"),
            reset_thumbnail: b("pin-reset-thumbnail"),
            translucent_flash: b("pin-translucent-flash"),
            flash_group: b("pin-flash-group"),
            hide_on_flash: b("pin-hide-on-flash"),
            preserve_drag: b("pin-preserve-drag"),
            confirm_close: b("pin-confirm-close"),
            confirm_destroy: b("pin-confirm-destroy"),
            flash_new: b("pin-flash-new"),
            activate_new: b("pin-activate-new"),
            gnome_focus: b("pin-gnome-focus"),
            closed_limit: n("pin-closed-limit") as usize,
            mouse: super::mouse::Bindings::from_values(values),
            text: TextOptions {
                enabled: b("pin-text-enabled"),
                ignore_html: b("pin-text-ignore-html"),
                skip_paths: b("pin-text-skip-paths"),
                margin: n("pin-text-margin"),
                max_width: n("pin-text-max-width"),
                font: s("pin-text-font"),
                background: s("pin-text-background"),
                foreground: s("pin-text-foreground"),
            },
        }
    }
    pub fn initialize(&self, pin: &mut Pin) {
        pin.opacity = self.opacity;
        pin.shadow = self.shadow;
        pin.smooth = self.smooth;
        self.constrain(pin);
    }
    pub fn constrain(&self, pin: &mut Pin) -> bool {
        let before = (pin.max_size, pin.zoom, pin.thumbnail_zoom);
        pin.max_size = self.max_size;
        pin.zoom = pin.zoom.clamp(pin.minimum_zoom(), pin.maximum_zoom());
        if let Some(zoom) = pin.thumbnail_zoom.as_mut() {
            *zoom = zoom.min(self.max_size / pin.width.max(pin.height));
        }
        before != (pin.max_size, pin.zoom, pin.thumbnail_zoom)
    }
    pub fn zoom(&self, pin: &mut Pin, zoom: f64, point: [f64; 2]) {
        let before = pin.rect();
        pin.zoom = zoom.clamp(pin.minimum_zoom(), pin.maximum_zoom());
        let after = pin.rect();
        if self.zoom_at_cursor {
            pin.x = before.x - (point[0] - before.x) * (after.width / before.width - 1.);
            pin.y = before.y - (point[1] - before.y) * (after.height / before.height - 1.);
        }
    }
    pub fn thumbnail(&self, pin: &mut Pin) {
        if let Some(zoom) = pin.thumbnail_zoom.take() {
            pin.zoom = zoom.clamp(pin.minimum_zoom(), pin.maximum_zoom());
        } else {
            pin.thumbnail_zoom = Some(pin.zoom);
            pin.zoom = (self.thumbnail[0] / pin.width)
                .min(self.thumbnail[1] / pin.height)
                .min(1.);
        }
    }
    pub fn reset(&self, pin: &mut Pin) {
        if pin.thumbnail_zoom.is_none() || self.reset_thumbnail {
            pin.thumbnail_zoom = None;
            pin.zoom = 1f64.min(pin.maximum_zoom());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_and_limits_are_applied_to_new_pins_only() {
        let mut board = super::super::Board::default();
        let id = board
            .add(crate::annotation::Bounds {
                x: 0.,
                y: 0.,
                width: 5000.,
                height: 2500.,
            })
            .unwrap();
        let mut values = Map::new();
        values.insert("pin-opacity".into(), serde_json::json!(60));
        values.insert("pin-max-size".into(), serde_json::json!(1000));
        values.insert("pin-shadow".into(), serde_json::json!(false));
        let settings = Preferences::from_values(&values);
        let pin = board.get_mut(id).unwrap();
        settings.initialize(pin);
        assert_eq!(pin.opacity, 0.6);
        assert!(!pin.shadow);
        assert_eq!(pin.rect().width, 1000.);
        settings.thumbnail(pin);
        assert_eq!(pin.rect().width, 100.);
        assert!(pin.valid());
        settings.thumbnail(pin);
        assert_eq!(pin.rect().width, 1000.);
        Preferences::default().zoom(pin, 100., [0., 0.]);
        assert_eq!(pin.rect().width, 1000.);
    }
    #[test]
    fn retention_keeps_only_the_newest_closed_pins_and_never_active_pins() {
        let mut board = super::super::Board::default();
        let ids: Vec<_> = (0..4)
            .map(|_| {
                board
                    .add(crate::annotation::Bounds {
                        x: 0.,
                        y: 0.,
                        width: 100.,
                        height: 50.,
                    })
                    .unwrap()
            })
            .collect();
        for id in &ids[..3] {
            board.close(*id, *id);
        }
        assert_eq!(board.obsolete_closed(1), vec![ids[1], ids[0]]);
        assert_eq!(board.obsolete_closed(0).len(), 3);
        assert!(board.obsolete_closed(4).is_empty());
        assert!(!board.get(ids[3]).unwrap().closed);
    }
    #[test]
    fn preferences_anchor_zoom_and_preserve_thumbnail_reset_rules() {
        let mut board = super::super::Board::default();
        let id = board
            .add(crate::annotation::Bounds {
                x: 10.,
                y: 20.,
                width: 400.,
                height: 200.,
            })
            .unwrap();
        let p = board.get_mut(id).unwrap();
        let mut settings = Preferences::default();
        settings.initialize(p);
        settings.zoom_at_cursor = true;
        settings.zoom(p, 2., [110., 70.]);
        assert_eq!([p.x, p.y], [-90., -30.]);
        settings.thumbnail(p);
        assert_eq!([p.rect().width, p.rect().height], [100., 50.]);
        settings.reset_thumbnail = false;
        settings.reset(p);
        assert!(p.thumbnail_zoom.is_some());
        settings.reset_thumbnail = true;
        settings.reset(p);
        assert_eq!(p.zoom, 1.);
        assert!(p.thumbnail_zoom.is_none());
        settings.zoom_at_cursor = false;
        settings.zoom(p, 2., [0., 0.]);
        assert_eq!([p.x, p.y], [-90., -30.]);
    }
}

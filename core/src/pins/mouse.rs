//! Portable pin bindings. Native adapters supply button counts and modifiers;
//! settings and dispatch use the same action order and binding grammar.
use serde_json::{Map, Value};

pub const CONTROL: u8 = 1;
pub const SHIFT: u8 = 2;
pub const ALT: u8 = 4;
pub const SUPER: u8 = 8;
pub const MODIFIERS: [(&str, u8); 4] = [
    ("ctrl", CONTROL),
    ("shift", SHIFT),
    ("alt", ALT),
    ("super", SUPER),
];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    None,
    Wheel,
    DoubleLeft,
    Middle,
    Right,
}
impl Trigger {
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Wheel => "wheel",
            Self::DoubleLeft => "double-left",
            Self::Middle => "middle",
            Self::Right => "right",
        }
    }
    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "none" => Self::None,
            "wheel" => Self::Wheel,
            "double-left" => Self::DoubleLeft,
            "middle" => Self::Middle,
            "right" => Self::Right,
            _ => return None,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding {
    pub modifiers: u8,
    pub trigger: Trigger,
}
impl Binding {
    pub fn parse(value: &str) -> Option<Self> {
        let mut parts = value.rsplitn(2, '+');
        let trigger = Trigger::parse(parts.next()?)?;
        let mut modifiers = 0;
        if let Some(prefix) = parts.next() {
            for part in prefix.split('+') {
                let flag = MODIFIERS.iter().find(|(name, _)| *name == part)?.1;
                if modifiers & flag != 0 {
                    return None;
                }
                modifiers |= flag;
            }
        }
        (modifiers.count_ones() <= 2).then_some(Self { modifiers, trigger })
    }
    pub fn encode(self) -> String {
        MODIFIERS
            .iter()
            .filter(|(_, flag)| self.modifiers & flag != 0)
            .map(|(name, _)| *name)
            .chain([self.trigger.name()])
            .collect::<Vec<_>>()
            .join("+")
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Zoom,
    Opacity,
    Close,
    Reset,
    Thumbnail,
    CopyText,
    Destroy,
}
impl Action {
    /// Earlier rows win when users assign the same gesture to multiple actions.
    pub const ALL: [Self; 7] = [
        Self::Zoom,
        Self::Opacity,
        Self::Close,
        Self::Reset,
        Self::Thumbnail,
        Self::CopyText,
        Self::Destroy,
    ];
    pub fn key(self) -> &'static str {
        match self {
            Self::Zoom => "pin-mouse-zoom",
            Self::Opacity => "pin-mouse-opacity",
            Self::Close => "pin-mouse-close",
            Self::Reset => "pin-mouse-reset",
            Self::Thumbnail => "pin-mouse-thumbnail",
            Self::CopyText => "pin-mouse-copy-text",
            Self::Destroy => "pin-mouse-destroy",
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Zoom => "Scale pin",
            Self::Opacity => "Adjust pin opacity",
            Self::Close => "Close pin",
            Self::Reset => "Reset size and opacity",
            Self::Thumbnail => "Thumbnail / restore",
            Self::CopyText => "Copy original text",
            Self::Destroy => "Destroy pin",
        }
    }
    pub fn keys_key(self) -> Option<&'static str> {
        match self {
            Self::Zoom => Some("pin-mouse-zoom-keys"),
            Self::Opacity => Some("pin-mouse-opacity-keys"),
            _ => None,
        }
    }
    pub fn accepts(self, binding: Binding) -> bool {
        if self.keys_key().is_some() {
            matches!(binding.trigger, Trigger::None | Trigger::Wheel)
        } else {
            !matches!(binding.trigger, Trigger::Wheel)
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Bindings {
    bindings: [Binding; 7],
    keys: [bool; 2],
}
impl Bindings {
    pub fn from_values(values: &Map<String, Value>) -> Self {
        let schema = crate::config::schema();
        let get = |key: &str| values.get(key).unwrap_or(&schema[key]["default"]);
        Self {
            bindings: Action::ALL.map(|action| {
                Binding::parse(get(action.key()).as_str().expect("validated mouse binding"))
                    .expect("validated mouse binding")
            }),
            keys: ["pin-mouse-zoom-keys", "pin-mouse-opacity-keys"]
                .map(|key| get(key).as_bool().expect("validated mouse keys")),
        }
    }
    pub fn binding(&self, action: Action) -> Binding {
        self.bindings[action as usize]
    }
    pub fn gesture(&self, trigger: Trigger, modifiers: u8) -> Option<Action> {
        if trigger == Trigger::None {
            return None;
        }
        Action::ALL
            .into_iter()
            .find(|action| self.binding(*action) == Binding { trigger, modifiers })
    }
    pub fn key(&self, modifiers: u8) -> Option<Action> {
        [Action::Zoom, Action::Opacity]
            .into_iter()
            .enumerate()
            .find(|(i, action)| self.keys[*i] && self.binding(*action).modifiers == modifiers)
            .map(|(_, action)| action)
    }
}
impl Default for Bindings {
    fn default() -> Self {
        Self::from_values(&Map::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_and_conflicting_assignments_have_one_winner() {
        let defaults = Bindings::default();
        assert_eq!(
            defaults.gesture(Trigger::DoubleLeft, 0),
            Some(Action::Close)
        );
        assert_eq!(
            defaults.gesture(Trigger::DoubleLeft, CONTROL),
            Some(Action::Destroy)
        );
        assert_eq!(
            defaults.gesture(Trigger::DoubleLeft, SHIFT),
            Some(Action::Thumbnail)
        );
        assert_eq!(
            defaults.gesture(Trigger::Right, SHIFT),
            Some(Action::CopyText)
        );
        assert_eq!(defaults.gesture(Trigger::Wheel, CONTROL | SHIFT), None);
        assert_eq!(defaults.key(CONTROL), Some(Action::Opacity));
        let mut values = Map::new();
        values.insert(Action::Destroy.key().into(), Value::from("double-left"));
        assert_eq!(
            Bindings::from_values(&values).gesture(Trigger::DoubleLeft, 0),
            Some(Action::Close)
        );
        values.insert(Action::Close.key().into(), Value::from("none"));
        assert_eq!(
            Bindings::from_values(&values).gesture(Trigger::DoubleLeft, 0),
            Some(Action::Destroy)
        );
    }
    #[test]
    fn bindings_validate_and_store_only_nondefault_overrides() {
        use crate::config;
        for (key, value) in [
            ("pin-mouse-close", "ctrl+wheel"),
            ("pin-mouse-zoom", "middle"),
            ("pin-mouse-destroy", "ctrl+ctrl+double-left"),
        ] {
            assert!(config::validate(key, &Value::from(value)).is_err());
        }
        let directory =
            std::path::PathBuf::from(format!("/tmp/clivuu-mouse-config-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("config.ini");
        config::set_file(&path, Action::Close.key(), Value::from("alt+right")).unwrap();
        assert_eq!(
            Bindings::from_values(&config::read_file(&path).unwrap()).gesture(Trigger::Right, ALT),
            Some(Action::Close)
        );
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("[control.mouse]")
        );
        config::set_file(&path, Action::Close.key(), Value::from("double-left")).unwrap();
        assert!(
            !std::fs::read_to_string(&path)
                .unwrap()
                .contains("control.mouse")
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn binding_validation_and_serialization_preserve_modifiers() {
        let b = Binding::parse("alt+ctrl+right").unwrap();
        assert_eq!(b.encode(), "ctrl+alt+right");
        for text in [
            "ctrl+ctrl+right",
            "shift+alt+ctrl+wheel",
            "left",
            "ctrl+unknown",
        ] {
            assert!(Binding::parse(text).is_none());
        }
        assert!(!Action::Close.accepts(Binding::parse("wheel").unwrap()));
        assert!(!Action::Zoom.accepts(Binding::parse("middle").unwrap()));
        assert!(Action::Zoom.accepts(Binding::parse("ctrl+none").unwrap()));
    }
}

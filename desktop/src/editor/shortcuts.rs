//! Local editor keys, shared by capture and pin input adapters. Editable GTK
//! controls/IME consume text before these bindings; lock modifiers are ignored.
use super::{Action, Tool};
use crate::ui::toolbar::Output;
use gtk::gdk::{Key, ModifierType as Mods};

/// One table owns tool labels, numbers and key dispatch for both editors.
pub const TOOLS: [(Tool, Key, &str, &str); 9] = [
    (Tool::Select, Key::_1, "Cursor", "Ctrl+1"),
    (Tool::Rect, Key::_2, "Shapes", "Ctrl+2"),
    (Tool::Arrow, Key::_3, "Arrow", "Ctrl+3"),
    (Tool::Pen, Key::_4, "Pen", "Ctrl+4"),
    (Tool::Highlight, Key::_5, "Highlight", "Ctrl+5"),
    (Tool::Sequence, Key::_6, "Sequence", "Ctrl+6"),
    (Tool::Mosaic, Key::_7, "Mosaic", "Ctrl+7"),
    (Tool::Text, Key::_8, "Text", "Ctrl+8"),
    (Tool::Eraser, Key::e, "Eraser", "E"),
];
pub fn tool_label(tool: Tool) -> (&'static str, &'static str) {
    let tool = if tool == Tool::Ellipse {
        Tool::Rect
    } else {
        tool
    };
    TOOLS
        .iter()
        .find(|b| b.0 == tool)
        .map(|b| (b.2, b.3))
        .unwrap_or(("Crop", ""))
}
pub fn tooltip(title: &str, keys: &str) -> String {
    let title = clivuu_core::i18n::t(title);
    if keys.is_empty() {
        title
    } else {
        format!("{title} ({keys})")
    }
}
pub fn modifiers(state: Mods) -> Mods {
    state
        & (Mods::CONTROL_MASK
            | Mods::SHIFT_MASK
            | Mods::ALT_MASK
            | Mods::SUPER_MASK
            | Mods::META_MASK)
}
pub fn output(key: Key, state: Mods) -> Option<Output> {
    let m = modifiers(state);
    match (key.to_lower(), m) {
        (Key::c, Mods::CONTROL_MASK) => Some(Output::Copy),
        (Key::s, Mods::CONTROL_MASK) => Some(Output::Save),
        (Key::s, m) if m == Mods::CONTROL_MASK | Mods::SHIFT_MASK => Some(Output::QuickSave),
        _ => None,
    }
}
pub fn edit(key: Key, state: Mods, tool: Tool) -> Option<Action> {
    let editing = tool.kind().is_some();
    let m = modifiers(state);
    let key = key.to_lower();
    if m == Mods::CONTROL_MASK {
        if let Some((tool, _, _, _)) = TOOLS
            .iter()
            .filter(|b| b.0 != Tool::Eraser)
            .find(|b| b.1 == key)
        {
            return Some(if *tool == Tool::Rect {
                Action::UseShapes
            } else {
                Action::Tool(*tool)
            });
        }
        return match key {
            Key::z => Some(Action::Undo),
            Key::y => Some(Action::Redo),
            _ => None,
        };
    }
    if m == Mods::CONTROL_MASK | Mods::SHIFT_MASK && key == Key::z {
        return Some(Action::Clear);
    }
    if !m.is_empty() {
        return None;
    }
    match key {
        Key::z if tool == Tool::Sequence => Some(Action::SequenceNext(1)),
        Key::x if tool == Tool::Sequence => Some(Action::SequenceNext(-1)),
        Key::e => Some(Action::Tool(Tool::Eraser)),
        Key::_1 | Key::bracketleft if editing => Some(Action::Wheel(1.)),
        Key::_2 | Key::bracketright if editing => Some(Action::Wheel(-1.)),
        Key::Delete | Key::BackSpace => Some(Action::Delete),
        _ => None,
    }
}
/// Scene commands that remain available while typing an inline annotation.
/// Clipboard/text history, plain characters and Enter/Escape stay with GTK.
pub fn while_typing(key: Key, state: Mods, pin: bool) -> bool {
    let m = modifiers(state);
    let key = key.to_lower();
    if m == Mods::CONTROL_MASK {
        (TOOLS
            .iter()
            .filter(|b| b.0 != Tool::Eraser)
            .any(|b| b.1 == key)
            || key == Key::s)
            || if pin { key == Key::w } else { key == Key::t }
    } else if m == Mods::CONTROL_MASK | Mods::SHIFT_MASK {
        key == Key::s || pin && matches!(key, Key::p | Key::c)
    } else {
        pin && m == Mods::SHIFT_MASK && key == Key::Escape
    }
}
pub fn direction(key: Key) -> Option<[f64; 2]> {
    match key {
        Key::Left => Some([-1., 0.]),
        Key::Right => Some([1., 0.]),
        Key::Up => Some([0., -1.]),
        Key::Down => Some([0., 1.]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::{EditorController, ImageMapping};
    use clivuu_core::annotation::Bounds;
    fn controller() -> EditorController {
        EditorController::new(ImageMapping {
            display: Bounds {
                x: 0.,
                y: 0.,
                width: 300.,
                height: 200.,
            },
            document: [600., 400.],
        })
    }
    fn press(c: &mut EditorController, k: Key, m: Mods) {
        if let Some(a) = edit(k, m, c.tool()) {
            c.dispatch(a);
        }
    }
    #[test]
    fn tool_numbers_dispatch_the_same_tools_the_toolbar_advertises() {
        for (tool, key, title, chord) in TOOLS.into_iter().filter(|b| b.0 != Tool::Eraser) {
            let mut c = controller();
            press(&mut c, key, Mods::CONTROL_MASK);
            assert_eq!(c.tool(), tool);
            assert_eq!(tool_label(tool), (title, chord));
            assert!(while_typing(key, Mods::CONTROL_MASK, true));
        }
        assert!(edit(Key::_0, Mods::CONTROL_MASK, Tool::Select).is_none());
        assert!(!while_typing(Key::_0, Mods::CONTROL_MASK, true));
    }
    #[test]
    fn shortcuts_select_shared_tools_and_width_changes_preserve_committed_strokes() {
        let mut c = controller();
        press(&mut c, Key::_4, Mods::CONTROL_MASK);
        assert_eq!(c.tool(), Tool::Pen);
        c.dispatch(Action::Begin {
            point: [20., 20.],
            shift: false,
        });
        c.dispatch(Action::Move {
            point: [50., 50.],
            shift: false,
        });
        c.dispatch(Action::End);
        let ink = c.session().marks()[0].clone();
        let width = c.appearance().unwrap().snapshot().style.width;
        press(&mut c, Key::_2, Mods::empty());
        press(&mut c, Key::bracketright, Mods::empty());
        assert!(c.appearance().unwrap().snapshot().style.width > width);
        assert_eq!(c.session().marks()[0], ink);
        press(&mut c, Key::z, Mods::CONTROL_MASK);
        assert!(c.session().marks().is_empty());
        press(&mut c, Key::y, Mods::CONTROL_MASK);
        assert_eq!(c.session().marks()[0], ink);
        press(&mut c, Key::z, Mods::CONTROL_MASK | Mods::SHIFT_MASK);
        assert!(c.session().marks().is_empty());
        assert!(!c.session().can_undo());
        assert!(!c.session().can_redo());
        press(&mut c, Key::y, Mods::CONTROL_MASK);
        assert!(c.session().marks().is_empty());
    }
    #[test]
    fn sequence_counter_keys_do_not_edit_selected_badges_or_steal_typing() {
        let mut c = controller();
        press(&mut c, Key::_6, Mods::CONTROL_MASK);
        c.dispatch(Action::Begin {
            point: [40., 40.],
            shift: false,
        });
        c.dispatch(Action::End);
        let before = c.session().marks()[0].clone();
        let index = |c: &EditorController| {
            c.session()
                .style(clivuu_core::annotation::Kind::Sequence)
                .style
                .sequence
                .index
        };
        assert_eq!(index(&c), 2);
        press(&mut c, Key::z, Mods::empty());
        press(&mut c, Key::z, Mods::empty());
        press(&mut c, Key::x, Mods::LOCK_MASK);
        assert_eq!(index(&c), 3);
        assert_eq!(c.session().marks()[0], before);
        for key in [Key::z, Key::x] {
            assert!(!while_typing(key, Mods::empty(), true));
            assert!(!while_typing(key, Mods::empty(), false));
            for mods in [Mods::SHIFT_MASK, Mods::ALT_MASK, Mods::SUPER_MASK] {
                assert!(edit(key, mods, Tool::Sequence).is_none());
            }
            assert!(edit(key, Mods::empty(), Tool::Text).is_none());
        }
        c.dispatch(Action::SequenceIndex(50));
        assert_eq!(index(&c), 50);
        assert_eq!(c.session().marks()[0], before);
        press(&mut c, Key::z, Mods::CONTROL_MASK);
        assert!(c.session().marks().is_empty());
        press(&mut c, Key::y, Mods::CONTROL_MASK);
        assert_eq!(c.session().marks()[0], before);
        c.dispatch(Action::SequenceIndex(1));
        press(&mut c, Key::x, Mods::empty());
        assert_eq!(index(&c), 1);
    }
    #[test]
    fn modified_chords_do_not_trigger_ordinary_commands() {
        assert!(edit(Key::_1, Mods::ALT_MASK, Tool::Pen).is_none());
        assert!(edit(Key::_1, Mods::CONTROL_MASK | Mods::SHIFT_MASK, Tool::Pen).is_none());
        assert!(output(Key::c, Mods::CONTROL_MASK | Mods::SHIFT_MASK).is_none());
        assert!(output(Key::s, Mods::CONTROL_MASK | Mods::ALT_MASK).is_none());
        assert!(while_typing(Key::_1, Mods::CONTROL_MASK, true));
        assert!(!while_typing(Key::_1, Mods::empty(), true));
        assert!(!while_typing(Key::e, Mods::empty(), true));
        assert!(!while_typing(Key::v, Mods::CONTROL_MASK, true));
        assert!(!while_typing(Key::a, Mods::CONTROL_MASK, true));
        let mut c = controller();
        press(&mut c, Key::e, Mods::LOCK_MASK);
        assert_eq!(c.tool(), Tool::Eraser);
    }
}

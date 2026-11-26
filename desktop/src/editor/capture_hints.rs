//! Contextual, non-interactive capture help, separate from magnifier help.
use super::{EditorController, Tool};
use crate::ui::viewport::Viewport;
use clivuu_core::{
    annotation::Bounds,
    capture::Selection,
    i18n::{self, t},
};
use gtk::prelude::*;
use std::cell::Cell;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Hover,
    Selected,
    Editing(Tool, bool),
}
#[derive(Clone, Copy)]
enum Token {
    Key(&'static str),
    ChordJoin,
    AltSep,
}
pub(super) struct Hints {
    widget: gtk::Grid,
    phase: Cell<Option<(Phase, bool, i18n::Language)>>,
    measured: Cell<Option<(i32, i32, i32, i32)>>,
}
impl Hints {
    pub fn new() -> Self {
        let widget = gtk::Grid::builder()
            .column_spacing(12)
            .row_spacing(6)
            .build();
        widget.add_css_class("clivuu-capture-keys");
        widget.set_widget_name("capture-key-tips");
        widget.set_can_target(false);
        Self {
            widget,
            phase: Cell::new(None),
            measured: Cell::new(None),
        }
    }
    pub fn widget(&self) -> &gtk::Grid {
        &self.widget
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Contextual hints depend on output viewport, selection state, active tool and cursor-toggle capability"
    )]
    pub fn sync(
        &self,
        root: &Viewport,
        output: Bounds,
        viewport: Bounds,
        visible: bool,
        can_toggle_cursor: bool,
        selection: &Selection,
        editor: &EditorController,
    ) {
        let base_visible = visible
            && selection.drag.is_none()
            && !editor.has_active_gesture()
            && viewport.contains(output.center(), 0.);
        self.widget.set_visible(base_visible);
        if !base_visible {
            return;
        }
        let tool = editor.tool();
        let phase = if !selection.selected {
            Phase::Hover
        } else if tool == Tool::Select {
            Phase::Selected
        } else {
            let session = editor.session();
            let selected_after_commit = session.current().is_some_and(|mark| {
                !session.is_text_editing()
                    || mark.text.as_ref().is_some_and(|text| !text.is_empty())
            });
            Phase::Editing(tool, selected_after_commit)
        };
        let state = (phase, can_toggle_cursor, i18n::current_language());
        if self.phase.replace(Some(state)) != Some(state) {
            self.measured.set(None);
            while let Some(child) = self.widget.first_child() {
                self.widget.remove(&child);
            }
            use Token::{AltSep, ChordJoin, Key};
            let rows: &[(&[Token], &str)] = match phase {
                Phase::Hover => &[
                    (
                        &[Key("Ctrl"), ChordJoin, Key("A")],
                        "Select current screen / full desktop",
                    ),
                    (&[Key("R")], "Use last successful capture region"),
                    (
                        &[Key("Page Up"), AltSep, Key("Page Down")],
                        "Previous / next region",
                    ),
                    (
                        &[Key(","), AltSep, Key(".")],
                        "Previous / next captured image",
                    ),
                ],
                Phase::Selected => &[
                    (
                        &[Key("↑"), Key("↓"), Key("←"), Key("→")],
                        "Move selection by one pixel",
                    ),
                    (
                        &[Key("Ctrl"), ChordJoin, Key("A")],
                        "Select current screen / full desktop",
                    ),
                    (&[Key("R")], "Use last successful capture region"),
                    (
                        &[Key("Page Up"), AltSep, Key("Page Down")],
                        "Previous / next region",
                    ),
                    (
                        &[Key(","), AltSep, Key(".")],
                        "Previous / next captured image",
                    ),
                    (&[Key("Alt")], "Show magnifier / toggle capture guides"),
                ],
                Phase::Editing(_, _) => &[(
                    &[
                        Key("1"),
                        Key("2"),
                        AltSep,
                        Key("["),
                        Key("]"),
                        AltSep,
                        Key("Wheel"),
                    ],
                    "Adjust stroke size or rectangular mosaic strength",
                )],
            };
            let mut rows = rows.to_vec();
            if can_toggle_cursor && matches!(phase, Phase::Hover | Phase::Selected) {
                rows.push((&[Key("`"), AltSep, Key("!")], "Show / hide captured cursor"));
            }
            if matches!(
                phase,
                Phase::Editing(Tool::Rect | Tool::Ellipse | Tool::Arrow, _)
            ) {
                rows.push((&[Key("Shift")], "Constrain shapes and arrows"));
            }
            if let Phase::Editing(_, selected) = phase {
                rows.push((
                    &[Key("Right-click")],
                    if selected {
                        "Deselect annotation"
                    } else {
                        "Reset region / cancel capture"
                    },
                ));
            }
            for (row_idx, (tokens, description)) in rows.into_iter().enumerate() {
                let caps = gtk::Box::new(gtk::Orientation::Horizontal, 3);
                caps.set_halign(gtk::Align::End);
                caps.set_valign(gtk::Align::Center);
                for token in tokens {
                    match token {
                        Key(key) => {
                            let label = gtk::Label::new(Some(&t(key)));
                            label.add_css_class("clivuu-keycap");
                            label.set_halign(gtk::Align::Center);
                            label.set_valign(gtk::Align::Center);
                            caps.append(&label);
                        }
                        ChordJoin => {
                            let join = gtk::Label::new(Some("+"));
                            join.add_css_class("clivuu-key-join");
                            join.set_valign(gtk::Align::Center);
                            caps.append(&join);
                        }
                        AltSep => {
                            let sep = gtk::Label::new(Some("/"));
                            sep.add_css_class("clivuu-key-alt");
                            sep.set_valign(gtk::Align::Center);
                            caps.append(&sep);
                        }
                    }
                }
                let label = gtk::Label::builder()
                    .label(t(description))
                    .xalign(0.)
                    .valign(gtk::Align::Center)
                    .wrap(true)
                    .max_width_chars(42)
                    .build();
                label.add_css_class("clivuu-key-desc");
                self.widget.attach(&caps, 0, row_idx as i32, 1, 1);
                self.widget.attach(&label, 1, row_idx as i32, 1, 1);
            }
        }
        let available = (output.width - 24.).max(1.) as i32;
        let (minimum, width, height) = match self.measured.get() {
            Some((cached_avail, cached_min, cached_w, cached_h)) if cached_avail == available => {
                (cached_min, cached_w, cached_h)
            }
            _ => {
                let (minimum, natural, _, _) =
                    self.widget.measure(gtk::Orientation::Horizontal, -1);
                let width = natural.min(available);
                let height = if minimum <= available {
                    self.widget.measure(gtk::Orientation::Vertical, width).1
                } else {
                    0
                };
                self.measured.set(Some((available, minimum, width, height)));
                (minimum, width, height)
            }
        };
        if minimum > available || f64::from(height) > output.height - 24. {
            self.widget.set_visible(false);
            return;
        }
        let tip_x = output.x + 12.;
        let tip_y = (output.y + output.height - f64::from(height) - 12.).max(output.y + 12.);
        let tip_rect = Bounds {
            x: tip_x,
            y: tip_y,
            width: f64::from(width),
            height: f64::from(height),
        };
        let overlaps_selection = selection.selected
            && tip_rect.x < selection.rect.x + selection.rect.width
            && selection.rect.x < tip_rect.x + tip_rect.width
            && tip_rect.y < selection.rect.y + selection.rect.height
            && selection.rect.y < tip_rect.y + tip_rect.height;
        root.set_child_size(&self.widget, Some((width, height)));
        root.move_(&self.widget, tip_x - viewport.x, tip_y - viewport.y);
        self.widget.set_visible(!overlaps_selection);
    }
}

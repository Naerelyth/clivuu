//! Pin-only commands and focus routing. Surface backends supply native input;
//! all bindings and editor mutations remain independent of X11/layer-shell.
use super::*;
use crate::editor::shortcuts;
impl Pins {
    pub(super) fn key(self: &Rc<Self>, id: u64, key: gdk::Key, state: gdk::ModifierType) -> bool {
        // Both the shared editor and ordinary host enter here. A menu owns
        // keys before annotation/output commands, regardless of controller order.
        if self.menu.borrow().is_some() {
            if key == gdk::Key::Escape {
                self.close_menu();
            }
            return true;
        }
        let m = shortcuts::modifiers(state);
        if self.pin_has_active_ocr(id) {
            if m == gdk::ModifierType::CONTROL_MASK {
                match key.to_lower() {
                    gdk::Key::c => {
                        if self.copy_pin_ocr_text(id, true) {
                            return true;
                        }
                    }
                    gdk::Key::a => {
                        let changed = self
                            .data
                            .borrow_mut()
                            .get_mut(&id)
                            .is_some_and(|d| d.ocr.selection.select_all());
                        if changed {
                            self.sync();
                        }
                        return true;
                    }
                    _ => {}
                }
            } else if m.is_empty() && key == gdk::Key::Escape {
                let cleared = self
                    .data
                    .borrow_mut()
                    .get_mut(&id)
                    .is_some_and(|d| d.ocr.selection.deselect());
                if cleared {
                    self.sync();
                } else {
                    self.deactivate_pin_ocr(id);
                }
                return true;
            }
        }
        if (m.is_empty()
            || (m & !(gdk::ModifierType::ALT_MASK | gdk::ModifierType::SHIFT_MASK)).is_empty()
                && matches!(key, gdk::Key::c | gdk::Key::C)
            || matches!(
                key,
                gdk::Key::Alt_L | gdk::Key::Alt_R | gdk::Key::Shift_L | gdk::Key::Shift_R
            ))
            && self.lens_key(id, key, false)
        {
            return true;
        }
        let tool = (self.active.get() == Some(id))
            .then(|| {
                self.data
                    .borrow()
                    .get(&id)
                    .and_then(|d| d.editor.as_ref())
                    .map(|c| c.borrow().tool())
            })
            .flatten()
            .unwrap_or(crate::editor::Tool::Select);
        let editing = tool.kind().is_some();
        if !editing
            && let Some(increase) = match key {
                gdk::Key::plus | gdk::Key::equal | gdk::Key::KP_Add => Some(true),
                gdk::Key::minus | gdk::Key::KP_Subtract => Some(false),
                _ => None,
            }
        {
            let action = {
                let settings = self.preferences.borrow();
                let modifiers = super::input::mouse_modifiers(state);
                settings.mouse.key(modifiers).or_else(|| {
                    // A physical + commonly carries Shift. Preserve an explicitly
                    // configured Shift binding before trying the unshifted action.
                    (key == gdk::Key::plus)
                        .then(|| {
                            settings
                                .mouse
                                .key(modifiers & !clivuu_core::pins::mouse::SHIFT)
                        })
                        .flatten()
                })
            };
            if let Some(action) = action {
                let point = self
                    .pointer
                    .get()
                    .or_else(|| self.board.borrow().get(id).map(|p| p.rect().center()))
                    .unwrap_or([0., 0.]);
                self.mouse_action(id, action, if increase { -1. } else { 1. }, point);
                return true;
            }
        }
        if let Some(action) = shortcuts::edit(key, state, tool) {
            if self.active.get() != Some(id) {
                self.toolbar(Some(id));
            }
            let doc = self.data.borrow().get(&id).and_then(|d| d.editor.clone());
            if let Some(doc) = doc {
                doc.dispatch(action);
            }
            return true;
        }
        if let Some(direction) = shortcuts::direction(key) {
            if !editing && m.is_empty() {
                let ids = self.shortcut_targets(id);
                let mut board = self.board.borrow_mut();
                for id in ids {
                    if let Some(p) = board.get_mut(id) {
                        p.x += direction[0];
                        p.y += direction[1];
                    }
                }
                drop(board);
                self.changed();
                self.sync();
                return true;
            }
            return false;
        }
        let command = if let Some(action) = shortcuts::output(key, state) {
            Some(action)
        } else if m == gdk::ModifierType::CONTROL_MASK {
            match key.to_lower() {
                gdk::Key::w => Some(Output::ClosePin),
                gdk::Key::a => {
                    self.selected_pins.replace(
                        self.board
                            .borrow()
                            .pins
                            .iter()
                            .filter(|p| self.board.borrow().visible(p))
                            .map(|p| p.id)
                            .collect(),
                    );
                    self.sync();
                    return true;
                }
                gdk::Key::v => Some(Output::ReplacePin),
                _ => None,
            }
        } else if m == gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK {
            match key.to_lower() {
                gdk::Key::c => Some(Output::CopyText),
                gdk::Key::p => Some(Output::Preferences),
                _ => None,
            }
        } else if m == gdk::ModifierType::SHIFT_MASK && key == gdk::Key::Escape {
            Some(Output::DestroyPin)
        } else if m.is_empty() {
            match key {
                gdk::Key::space => {
                    if self.active.get() == Some(id) {
                        (self.callback)(Event::Output(id, Output::Apply));
                    } else {
                        self.toolbar(Some(id));
                    }
                    return true;
                }
                gdk::Key::Escape => Some(if self.active.get() == Some(id) {
                    Output::Apply
                } else {
                    Output::ClosePin
                }),
                gdk::Key::Return | gdk::Key::KP_Enter if self.active.get() == Some(id) => {
                    Some(Output::Apply)
                }
                gdk::Key::o | gdk::Key::O if !editing => Some(Output::Ocr),
                gdk::Key::_1 if !editing => Some(Output::RotateRight),
                gdk::Key::_2 if !editing => Some(Output::RotateLeft),
                gdk::Key::_3 if !editing => Some(Output::FlipHorizontal),
                gdk::Key::_4 if !editing => Some(Output::FlipVertical),
                gdk::Key::_5 if !editing => {
                    let ids = self.shortcut_targets(id);
                    for id in ids {
                        if let Some(p) = self.board.borrow_mut().get_mut(id) {
                            p.grayscale = !p.grayscale;
                        }
                    }
                    self.changed();
                    self.sync();
                    return true;
                }
                gdk::Key::_6 if !editing => {
                    let ids = self.shortcut_targets(id);
                    for id in ids {
                        if let Some(p) = self.board.borrow_mut().get_mut(id) {
                            p.inverted = !p.inverted;
                        }
                    }
                    self.changed();
                    self.sync();
                    return true;
                }
                _ => None,
            }
        } else {
            None
        };
        if let Some(command) = command {
            (self.callback)(Event::Output(id, command));
            true
        } else {
            false
        }
    }
    pub fn replace_source(
        self: &Rc<Self>,
        id: u64,
        image: FrozenImage,
        rect: Bounds,
    ) -> Result<(), String> {
        let editing = self.active.get() == Some(id);
        {
            let mut data = self.data.borrow_mut();
            let d = data
                .get_mut(&id)
                .ok_or_else(|| t("Invalid saved pin data"))?;
            d.version += d.editor.as_ref().map_or(0, |c| c.borrow().revision());
            d.views.clear();
            d.editor = None;
            d.baseline = None;
            for widget in &d.widgets {
                widget.source(&image.texture);
            }
            d.ocr.deactivate();
            d.image = image;
            d.version += 1;
            d.stored = None;
        }
        {
            let mut board = self.board.borrow_mut();
            let p = board
                .get_mut(id)
                .ok_or_else(|| t("Invalid saved pin data"))?;
            p.width = rect.width;
            p.height = rect.height;
            p.zoom = 1.;
            p.grayscale = false;
            p.inverted = false;
        }
        if editing {
            self.ensure_editor(id)?;
        }
        self.changed();
        self.sync();
        Ok(())
    }
    pub async fn selection_png(
        self: &Rc<Self>,
        id: u64,
        cancel: &gio::Cancellable,
    ) -> Result<(Vec<u8>, Bounds, Option<u64>), String> {
        let ids = self.shortcut_targets(id);
        let layout = {
            let board = self.board.borrow();
            board
                .pins
                .iter()
                .filter(|p| ids.contains(&p.id) && board.visible(p))
                .map(|p| (p.id, p.rect(), p.opacity))
                .collect::<Vec<_>>()
        };
        if layout.len() == 1 {
            let (id, rect, _) = layout[0];
            return Ok((self.png(id, cancel).await?, rect, Some(id)));
        }
        let mut layers = vec![];
        for (id, rect, opacity) in layout {
            layers.push((self.png(id, cancel).await?, rect, opacity));
        }
        let task = self
            .pool
            .submit(move |token| {
                let mut decoded = vec![];
                for (bytes, rect, opacity) in layers {
                    token.check()?;
                    decoded.push((
                        clivuu_core::image_ops::decode(&bytes)
                            .map_err(|e| TaskError::Failed(e.to_string()))?,
                        rect,
                        opacity,
                    ));
                }
                token.check()?;
                let (image, rect) = clivuu_core::compose::pins(&decoded)
                    .map_err(|e| TaskError::Failed(e.to_string()))?;
                token.check()?;
                let mut png = vec![];
                clivuu_core::image::export::write(
                    clivuu_core::image::export::Source::Pixels(&image),
                    std::path::Path::new("pins.png"),
                    100,
                    &mut png,
                )
                .map_err(|e| TaskError::Failed(e.to_string()))?;
                Ok((png, rect, None))
            })
            .map_err(|e| e.to_string())?;
        gio::CancellableFuture::new(task.result(), cancel.clone())
            .await
            .map_err(|_| t("Operation cancelled"))?
            .map_err(|e| e.to_string())
    }
    pub fn shortcut_targets(&self, id: u64) -> Vec<u64> {
        let board = self.board.borrow();
        let selected = self.selected_pins.borrow();
        if selected.contains(&id) {
            selected
                .iter()
                .copied()
                .filter(|id| board.get(*id).is_some_and(|p| board.visible(p)))
                .collect()
        } else {
            vec![id]
        }
    }
}

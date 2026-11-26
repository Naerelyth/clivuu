//! Lazy closed-pin lifecycle. Only active/unsaved images retain decoded pixels;
//! repository access and decoding run on the bounded worker pool.
use super::*;
impl Pins {
    pub fn restore(
        self: &Rc<Self>,
        restored: crate::infrastructure::pin_store::Restored,
        repository: Arc<Mutex<crate::infrastructure::pin_store::PinStore>>,
    ) -> Result<(), String> {
        if let Some(message) = restored.recovery {
            eprintln!("{message}");
        }
        let mut data = HashMap::new();
        for (id, pixels) in restored.images {
            let image = FrozenImage::from_shared(pixels.clone())
                .ok_or_else(|| t("Invalid saved pin data"))?;
            data.insert(id, Data::new(image, Some((0, pixels)), 0));
        }
        self.storage.replace(Some(repository));
        self.archived
            .replace(restored.closed.into_iter().map(|id| (id, 0)).collect());
        self.data.replace(data);
        self.board.replace(restored.board);
        self.apply_limits();
        self.prune_closed();
        if self.board.borrow().pins.iter().any(|p| !p.closed) {
            self.ensure_hosts()?;
        }
        self.sync();
        Ok(())
    }
    pub async fn snapshot(
        self: &Rc<Self>,
        cancel: &gio::Cancellable,
    ) -> Result<Option<crate::infrastructure::pin_store::Snapshot>, String> {
        if self.drag.borrow().is_some()
            || self.data.borrow().values().any(|d| {
                d.editor.as_ref().is_some_and(|c| {
                    let c = c.borrow();
                    c.has_active_gesture() || c.session().is_text_editing()
                })
            })
        {
            return Ok(None);
        }
        let board = self.board.borrow().clone();
        let mut images = vec![];
        for pin in &board.pins {
            if !self.data.borrow().contains_key(&pin.id) {
                let revision = self
                    .archived
                    .borrow()
                    .get(&pin.id)
                    .copied()
                    .ok_or_else(|| t("Invalid saved pin data"))?;
                images.push((pin.id, revision, None));
                continue;
            }
            let (revision, image, cached, scene) = {
                let data = self.data.borrow();
                let d = data
                    .get(&pin.id)
                    .ok_or_else(|| t("Invalid saved pin data"))?;
                (
                    d.version + d.editor.as_ref().map_or(0, |c| c.borrow().revision()),
                    d.image.pixels.clone(),
                    d.stored.clone(),
                    d.views.first().map(|v| v.scene().clone()),
                )
            };
            let pixels = if let Some((_, pixels)) = cached.filter(|(r, _)| *r == revision) {
                pixels
            } else if revision == 0 || scene.is_none() {
                image
            } else if let Some(scene) = scene {
                let renderer = self
                    .hosts
                    .borrow()
                    .as_ref()
                    .and_then(|h| h.surfaces.first())
                    .and_then(|s| s.window.renderer())
                    .ok_or_else(|| t("The desktop service is unavailable"))?;
                editor::export::frame_unfiltered(&scene, &renderer, None, &self.pool, cancel)
                    .await
                    .map_err(|e| format!("{e:?}"))?
                    .pixels
            } else {
                return Err(t("Invalid saved pin data"));
            };
            // Keep the export revision with its exact immutable pixels. A later
            // edit cannot cause metadata to claim that stale pixels are current.
            if let Some(d) = self.data.borrow_mut().get_mut(&pin.id) {
                d.stored = Some((revision, pixels.clone()));
            }
            images.push((pin.id, revision, Some(pixels)));
        }
        Ok(Some(crate::infrastructure::pin_store::Snapshot {
            board,
            images,
        }))
    }

    /// Repository publication is the commit point. Retain unsaved pixels on
    /// failure; release closed views only when their exact revision is durable.
    pub fn persisted(self: &Rc<Self>, revisions: Vec<(u64, u64)>) {
        for (id, revision) in revisions {
            if self.board.borrow().get(id).is_none() {
                continue;
            }
            self.archived.borrow_mut().insert(id, revision);
            let closed = self.board.borrow().get(id).is_some_and(|p| p.closed);
            let unchanged = self.data.borrow().get(&id).is_some_and(|d| {
                d.version + d.editor.as_ref().map_or(0, |c| c.borrow().revision()) == revision
            });
            if closed && unchanged {
                self.release_data(id);
            }
        }
        self.sync();
    }
    pub async fn reopen(
        self: &Rc<Self>,
        all: bool,
        cancel: &gio::Cancellable,
    ) -> Result<usize, String> {
        let planned = {
            let mut board = self.board.borrow().clone();
            board.reopen(all);
            board
        };
        let ids: Vec<_> = {
            let board = self.board.borrow();
            planned
                .pins
                .iter()
                .filter(|p| !p.closed && board.get(p.id).is_some_and(|p| p.closed))
                .map(|p| p.id)
                .collect()
        };
        self.reopen_ids(ids, cancel).await
    }
    pub async fn reveal(
        self: &Rc<Self>,
        id: u64,
        cancel: &gio::Cancellable,
    ) -> Result<bool, String> {
        let closed = self.board.borrow().get(id).map(|p| p.closed);
        let Some(closed) = closed else {
            return Ok(false);
        };
        if closed && self.reopen_ids(vec![id], cancel).await? == 0 {
            return Ok(false);
        }
        let group_changed = self
            .board
            .borrow()
            .get(id)
            .is_some_and(|p| p.group != self.board.borrow().group);
        {
            let mut b = self.board.borrow_mut();
            let Some(index) = b.pins.iter().position(|p| p.id == id) else {
                return Ok(false);
            };
            let mut pin = b.pins.remove(index);
            pin.hidden = false;
            if b.group != pin.group {
                b.previous = b.group.clone();
                b.group = pin.group.clone();
            }
            b.hidden = false;
            b.selected = Some(id);
            b.pins.push(pin);
        }
        self.changed();
        self.sync();
        if self.preferences.borrow().activate_new {
            self.toolbar(None);
            self.highlight(Some(id));
            self.focus_pin(id);
        }
        if group_changed {
            self.group_changed();
        }
        self.flash(vec![id]);
        Ok(true)
    }
    async fn reopen_ids(
        self: &Rc<Self>,
        ids: Vec<u64>,
        cancel: &gio::Cancellable,
    ) -> Result<usize, String> {
        if !ids.is_empty() {
            self.ensure_hosts()?;
        }
        let mut restored = 0;
        for id in ids {
            if !self.data.borrow().contains_key(&id) {
                let repository = self
                    .storage
                    .borrow()
                    .clone()
                    .ok_or_else(|| t("Invalid saved pin data"))?;
                let task = self
                    .pool
                    .submit(move |token| {
                        token.check()?;
                        let result = repository
                            .lock()
                            .unwrap()
                            .image(id)
                            .map_err(TaskError::Failed)?;
                        token.check()?;
                        Ok(result)
                    })
                    .map_err(|e| e.to_string())?;
                let (revision, pixels) = gio::CancellableFuture::new(task.result(), cancel.clone())
                    .await
                    .map_err(|_| t("Operation cancelled"))?
                    .map_err(|e| e.to_string())?;
                if !self.board.borrow().get(id).is_some_and(|p| p.closed) {
                    continue;
                }
                // Another reopen may have edited and closed the pin while this
                // worker was decoding. Never replace a newer resident document.
                if !self.data.borrow().contains_key(&id) {
                    if self.archived.borrow().get(&id).copied() != Some(revision) {
                        continue;
                    }
                    let image = FrozenImage::from_shared(pixels.clone())
                        .ok_or_else(|| t("Invalid saved pin data"))?;
                    // Capacity can change across await. Commit the Board first;
                    // a rejected reopen must not retain decoded closed pixels.
                    if !self.board.borrow_mut().reopen_id(id) {
                        continue;
                    }
                    self.data
                        .borrow_mut()
                        .insert(id, Data::new(image, Some((revision, pixels)), revision));
                    restored += 1;
                    self.changed();
                    self.sync();
                    continue;
                }
            }
            if self.board.borrow_mut().reopen_id(id) {
                restored += 1;
                self.changed();
                self.sync();
            }
        }
        Ok(restored)
    }
}

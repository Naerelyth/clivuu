//! One effect pipeline per document, shared by every output view. Source
//! conversion and Cairo work stay on the bounded worker pool; only texture
//! publication happens on GTK's thread. Pointer input never waits for a job.
use super::render::{MosaicTexture, MosaicTextures, related};
use crate::infrastructure::tasks::{Cancellation, TaskError, TaskPool};
use clivuu_core::image::{
    RgbaImage,
    effects::{self, Effect, Source, TileSpec},
};
use gtk::{gdk, glib, prelude::*};
use std::{
    cell::{Cell, Ref, RefCell},
    collections::{HashMap, HashSet, VecDeque},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

const STALE_BYTES: usize = 16 * 1024 * 1024;
const STALE_ENTRIES: usize = 12;
#[derive(Clone, Copy, PartialEq, Eq)]
enum Work {
    Prepare,
    Preview(TileSpec),
    Tile(TileSpec),
}
struct Running {
    work: Work,
    cancel: Cancellation,
}
#[derive(Default)]
struct State {
    image: Option<Arc<RgbaImage>>,
    source: Option<Source>,
    wanted: Vec<TileSpec>,
    warm: Option<Effect>,
    presentation: bool,
    preview_failed: HashSet<Effect>,
    order: VecDeque<TileSpec>,
    running: Option<Running>,
    failed: HashMap<TileSpec, String>,
    error: Option<String>,
}
impl Drop for State {
    fn drop(&mut self) {
        if let Some(running) = &self.running {
            running.cancel.cancel();
        }
    }
}
#[derive(Default)]
struct Inner {
    state: RefCell<State>,
    textures: RefCell<MosaicTextures>,
    pool: RefCell<Option<Rc<TaskPool>>>,
    generation: Cell<u64>,
    queued: Cell<bool>,
    changed: RefCell<Option<Box<dyn Fn()>>>,
}
#[derive(Clone, Default)]
pub struct Mosaics(Rc<Inner>);
impl Mosaics {
    pub fn textures(&self) -> Ref<'_, MosaicTextures> {
        self.0.textures.borrow()
    }
    pub fn dimensions(&self) -> [u32; 2] {
        self.0.textures.borrow().source
    }
    pub fn error(&self) -> Option<String> {
        let state = self.0.state.borrow();
        state.error.clone().or_else(|| {
            state
                .wanted
                .iter()
                .find_map(|spec| state.failed.get(spec).cloned())
        })
    }
    pub fn generation(&self) -> u64 {
        self.0.generation.get()
    }
    pub fn fork(&self, wanted: Vec<TileSpec>) -> Option<Self> {
        let state = self.0.state.borrow();
        let source = state.source.clone();
        let image = state.image.clone();
        if source.is_none() && image.is_none() {
            return None;
        }
        let pool = self.0.pool.borrow().clone()?;
        let this = Self::default();
        this.0.pool.replace(Some(pool));
        this.0.state.borrow_mut().source = source;
        this.0.state.borrow_mut().image = image;
        let mut textures = self.textures().clone();
        textures.entries.retain(|key, _| wanted.contains(key));
        this.0.state.borrow_mut().order = textures.entries.keys().copied().collect();
        this.0.textures.replace(textures);
        this.request(wanted);
        Some(this)
    }
    pub fn is_idle(&self) -> bool {
        let state = self.0.state.borrow();
        state.running.is_none()
            && !self.0.queued.get()
            && (state.error.is_some()
                || state.wanted.is_empty()
                || state.image.is_none()
                    && state.wanted.iter().all(|spec| {
                        self.0.textures.borrow().entries.contains_key(spec)
                            || state.failed.contains_key(spec)
                    }))
    }
    pub fn set_source(&self, image: RgbaImage, pool: Rc<TaskPool>, changed: impl Fn() + 'static) {
        self.set_shared_source(Arc::new(image), pool, changed);
    }
    pub fn set_shared_source(
        &self,
        image: Arc<RgbaImage>,
        pool: Rc<TaskPool>,
        changed: impl Fn() + 'static,
    ) {
        self.0
            .generation
            .set(self.0.generation.get().wrapping_add(1));
        // Dropping the old state cancels its work before releasing source data.
        let mut state = State::default();
        state.image = Some(image);
        state.presentation = true;
        self.0.state.replace(state);
        let dimensions = self
            .0
            .state
            .borrow()
            .image
            .as_ref()
            .map(|image| [image.width(), image.height()])
            .unwrap();
        self.0.textures.replace(MosaicTextures {
            source: dimensions,
            ..MosaicTextures::default()
        });
        self.0.pool.replace(Some(pool));
        self.0.changed.replace(Some(Box::new(changed)));
        self.publish();
        self.schedule();
    }
    pub fn request(&self, wanted: impl IntoIterator<Item = TileSpec>) {
        let mut unique = HashSet::new();
        let wanted: Vec<_> = wanted
            .into_iter()
            .filter(|spec| unique.insert(*spec))
            .collect();
        {
            let mut state = self.0.state.borrow_mut();
            if state.wanted == wanted {
                return;
            }
            // Finish useful ROI work through brush growth/slider input. Cancelling
            // every superseded key starves presentation under continuous motion.
            if let Some(Running {
                work: Work::Tile(spec),
                cancel,
            }) = &state.running
                && !wanted.iter().any(|wanted| related(*spec, *wanted))
            {
                cancel.cancel();
            }
            state.wanted = wanted;
            // Touch cache hits in request order; stale entries are evicted
            // by last use rather than by the time they were first generated.
            let hits: Vec<_> = state
                .wanted
                .iter()
                .filter(|spec| self.0.textures.borrow().entries.contains_key(spec))
                .copied()
                .collect();
            for key in hits {
                state.order.retain(|value| *value != key);
                state.order.push_back(key);
            }
        }
        self.trim();
        self.schedule();
    }
    /// Warm only the active effect when its tool is selected. No effect work
    /// is added to ordinary capture or pin startup when Mosaic is never used.
    pub fn warm(&self, effect: Option<Effect>) {
        let mut state = self.0.state.borrow_mut();
        if state.warm == effect {
            return;
        }
        state.warm = effect;
        drop(state);
        self.schedule();
    }
    fn publish(&self) {
        if let Some(callback) = self.0.changed.borrow().as_ref() {
            callback();
        }
    }
    fn trim(&self) {
        let mut state = self.0.state.borrow_mut();
        let mut active: HashSet<_> = state.wanted.iter().copied().collect();
        let mut textures = self.0.textures.borrow_mut();
        // At most one retained ROI per pending request lives outside the stale
        // LRU budget. It becomes evictable as soon as its replacement arrives.
        active.extend(
            state
                .wanted
                .iter()
                .filter_map(|spec| textures.retained(*spec)),
        );
        let mut stale_bytes = state
            .order
            .iter()
            .filter(|key| !active.contains(key))
            .map(|key| key.bytes())
            .sum::<usize>();
        let mut stale_count = state
            .order
            .iter()
            .filter(|key| !active.contains(key))
            .count();
        state.order.retain(|key| {
            if !active.contains(key) && (stale_count > STALE_ENTRIES || stale_bytes > STALE_BYTES) {
                stale_count -= 1;
                stale_bytes -= key.bytes();
                textures.entries.remove(key);
                false
            } else {
                true
            }
        });
        state.failed.retain(|key, _| active.contains(key));
    }
    fn schedule(&self) {
        if self.0.queued.replace(true) {
            return;
        }
        let weak = Rc::downgrade(&self.0);
        glib::idle_add_local_once(move || {
            if let Some(inner) = weak.upgrade() {
                inner.queued.set(false);
                Self(inner).start();
            }
        });
    }
    fn start(&self) {
        let state = self.0.state.borrow();
        if state.running.is_some() || state.error.is_some() {
            return;
        }
        let pool = self.0.pool.borrow();
        let Some(pool) = pool.as_ref() else {
            return;
        };
        let work = if state.image.is_some() && (!state.wanted.is_empty() || state.warm.is_some()) {
            Work::Prepare
        } else if state.source.is_some() {
            let textures = self.0.textures.borrow();
            let preview = state
                .presentation
                .then(|| {
                    state
                        .wanted
                        .iter()
                        .map(|spec| spec.effect)
                        .chain(state.warm)
                        .find(|effect| {
                            !textures.previews.contains_key(effect)
                                && !state.preview_failed.contains(effect)
                        })
                })
                .flatten();
            if let Some(effect) = preview {
                let [w, h] = textures.source;
                let Some(spec) = TileSpec::plan(
                    [w, h],
                    effect,
                    effect.strength_range()[1],
                    clivuu_core::annotation::Bounds {
                        x: 0.,
                        y: 0.,
                        width: f64::from(w),
                        height: f64::from(h),
                    },
                ) else {
                    return;
                };
                Work::Preview(spec)
            } else {
                let Some(spec) = state.wanted.iter().find(|spec| {
                    !textures.entries.contains_key(spec) && !state.failed.contains_key(spec)
                }) else {
                    return;
                };
                Work::Tile(*spec)
            }
        } else {
            return;
        };
        let image = state.image.clone();
        let source = state.source.clone();
        let task = pool.submit(move |cancel| {
            match work {
                Work::Prepare => {
                    Source::prepare(&image.unwrap(), || cancel.cancelled()).map(ResultValue::Source)
                }
                Work::Tile(spec) | Work::Preview(spec) => {
                    effects::render_prepared(&source.unwrap(), spec, || cancel.cancelled())
                        .map(ResultValue::Tile)
                }
            }
            .map_err(|error| {
                if error == effects::Error::Cancelled {
                    TaskError::Cancelled
                } else {
                    TaskError::Failed(format!("{error:?}"))
                }
            })
        });
        drop(state);
        let task = match task {
            Ok(task) => task,
            Err(TaskError::Busy) => {
                // Back pressure is asynchronous; retain the latest request,
                // never spin or block the GTK thread when the pool is full.
                if !self.0.queued.replace(true) {
                    let weak = Rc::downgrade(&self.0);
                    glib::timeout_add_local_once(Duration::from_millis(25), move || {
                        if let Some(inner) = weak.upgrade() {
                            inner.queued.set(false);
                            Self(inner).start();
                        }
                    });
                }
                return;
            }
            Err(error) => {
                self.fail(work, error);
                self.schedule();
                return;
            }
        };
        self.0.state.borrow_mut().running = Some(Running {
            work,
            cancel: task.cancellation(),
        });
        let generation = self.0.generation.get();
        let weak = Rc::downgrade(&self.0);
        glib::MainContext::default().spawn_local(async move {
            let result = task.result().await;
            let Some(inner) = weak
                .upgrade()
                .filter(|inner| inner.generation.get() == generation)
            else {
                return;
            };
            let this = Self(inner);
            this.0.state.borrow_mut().running = None;
            match result {
                Ok(ResultValue::Source(source)) => {
                    let mut state = this.0.state.borrow_mut();
                    state.image = None;
                    state.source = Some(source);
                }
                Ok(ResultValue::Tile(tile)) => {
                    if matches!(work, Work::Preview(_))
                        || this
                            .0
                            .state
                            .borrow()
                            .wanted
                            .iter()
                            .any(|spec| related(tile.spec, *spec))
                    {
                        let spec = tile.spec;
                        let format = if cfg!(target_endian = "little") {
                            gdk::MemoryFormat::B8g8r8a8Premultiplied
                        } else {
                            gdk::MemoryFormat::A8r8g8b8Premultiplied
                        };
                        let texture = gdk::MemoryTexture::new(
                            spec.width as i32,
                            spec.height as i32,
                            format,
                            &glib::Bytes::from_owned(tile.pixels),
                            spec.width as usize * 4,
                        )
                        .upcast();
                        let tile = MosaicTexture { texture, spec };
                        if matches!(work, Work::Preview(_)) {
                            this.0
                                .textures
                                .borrow_mut()
                                .previews
                                .insert(spec.effect, tile);
                        } else {
                            this.0.textures.borrow_mut().entries.insert(spec, tile);
                            this.0.state.borrow_mut().order.retain(|key| *key != spec);
                            this.0.state.borrow_mut().order.push_back(spec);
                        }
                        this.trim();
                        this.publish();
                    }
                }
                Err(TaskError::Cancelled) => (),
                Err(error) => this.fail(work, error),
            }
            this.schedule();
        });
    }
    fn fail(&self, work: Work, error: TaskError) {
        let mut state = self.0.state.borrow_mut();
        let error = format!("{error:?}");
        match work {
            Work::Prepare => state.error = Some(error),
            Work::Preview(spec) => {
                state.preview_failed.insert(spec.effect);
            }
            Work::Tile(spec) => {
                state.failed.insert(spec, error);
            }
        }
        drop(state);
        self.publish();
    }
}
enum ResultValue {
    Source(Source),
    Tile(effects::Tile),
}

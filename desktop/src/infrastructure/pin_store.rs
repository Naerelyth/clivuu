//! Worker-only immutable image revisions; publish the manifest last. The stable
//! lease prevents two backends from writing the same board. Never scan/delete
//! files that this instance did not load or create.
use clivuu_core::{i18n::t, image::RgbaImage, pins::Board};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::Arc,
};
type Result<T> = std::result::Result<T, String>;
struct Directory {
    path: PathBuf,
    memory: bool,
    _lease: File,
}
impl Directory {
    fn open(memory: bool) -> Result<Self> {
        let path = if memory {
            PathBuf::from(format!("/tmp/clivuu-pins-{}", uuid()))
        } else {
            clivuu_core::config::directory("XDG_DATA_HOME", ".local/share")
                .map_err(|e| e.to_string())?
                .join("pins")
        };
        Self::at(path, memory)
    }
    fn at(path: PathBuf, memory: bool) -> Result<Self> {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&path)
            .map_err(|e| e.to_string())?;
        let lease = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(path.join("owner.lock"))
            .map_err(|e| e.to_string())?;
        lease
            .try_lock()
            .map_err(|_| t("Pin storage is already in use by another Clivuu backend"))?;
        Ok(Self {
            path,
            memory,
            _lease: lease,
        })
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        if self.memory {
            // The memory directory is newly created and exclusively ours.
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
pub struct Snapshot {
    pub board: Board,
    pub images: Vec<(u64, u64, Option<Arc<RgbaImage>>)>,
}
pub struct Restored {
    pub board: Board,
    pub images: Vec<(u64, Arc<RgbaImage>)>,
    pub closed: Vec<u64>,
    pub recovery: Option<String>,
}
pub struct PinStore {
    directory: Directory,
    entries: HashMap<u64, (u64, String)>,
    owned: HashSet<PathBuf>,
    last: String,
}
impl PinStore {
    pub fn open(memory: bool) -> Result<(Self, Restored)> {
        Self::load(Directory::open(memory)?)
    }
    fn load(directory: Directory) -> Result<(Self, Restored)> {
        let path = directory.path.join("pins.json");
        let loaded = (|| -> Result<Board> {
            let contents = match File::open(&path) {
                Ok(f) => f,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Board::default()),
                Err(e) => return Err(e.to_string()),
            };
            let mut bytes = Vec::new();
            contents
                .take(1_000_001)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() > 1_000_000 {
                return Err(t("Invalid saved pin data"));
            }
            let mut board: Board =
                serde_json::from_slice(&bytes).map_err(|_| t("Invalid saved pin data"))?;
            board.restore()?;
            if board.pins.iter().any(|p| !image_name(&p.image)) {
                return Err(t("Invalid saved pin data"));
            }
            Ok(board)
        })();
        let (mut board, mut recovery) = match loaded {
            Ok(b) => (b, None),
            Err(e) => (Board::default(), Some(e)),
        };
        let mut images = vec![];
        let mut closed = vec![];
        let mut entries = HashMap::new();
        let mut owned = HashSet::new();
        board.pins.retain(|p| {
            let file = directory.path.join(&p.image);
            let result = file
                .to_str()
                .ok_or_else(|| t("Invalid saved pin data"))
                .and_then(|f| {
                    if p.closed {
                        clivuu_core::image_ops::dimensions(f)
                            .map(|_| None)
                            .map_err(|e| e.to_string())
                    } else {
                        clivuu_core::image_ops::load(f)
                            .map(|image| Some(Arc::new(image)))
                            .map_err(|e| e.to_string())
                    }
                });
            match result {
                Ok(image) => {
                    if let Some(image) = image {
                        images.push((p.id, image));
                    } else {
                        closed.push(p.id);
                    }
                    entries.insert(p.id, (0, p.image.clone()));
                    owned.insert(file);
                    true
                }
                Err(error) => {
                    recovery = Some(error);
                    false
                }
            }
        });
        if recovery.is_some() && path.is_file() {
            // Keep original bytes and unknown revisions for manual recovery.
            fs::copy(
                &path,
                directory
                    .path
                    .join(format!("pins-recovery-{}.json", uuid())),
            )
            .map_err(|e| e.to_string())?;
        }
        let restored = Restored {
            board,
            images,
            recovery,
            closed,
        };
        Ok((
            Self {
                directory,
                entries,
                owned,
                last: String::new(),
            },
            restored,
        ))
    }
    pub fn directory(&self) -> &Path {
        &self.directory.path
    }
    pub fn image(&self, id: u64) -> Result<(u64, Arc<RgbaImage>)> {
        let (revision, name) = self
            .entries
            .get(&id)
            .ok_or_else(|| t("Invalid saved pin data"))?;
        let path = self.directory.path.join(name);
        let pixels =
            clivuu_core::image_ops::load(path.to_str().ok_or_else(|| t("Invalid saved pin data"))?)
                .map_err(|e| e.to_string())?;
        Ok((*revision, Arc::new(pixels)))
    }
    pub fn publish(&mut self, snapshot: Snapshot, memory: bool) -> Result<Vec<(u64, u64)>> {
        let next = if memory != self.directory.memory {
            Some(Directory::open(memory)?)
        } else {
            None
        };
        let directory = next.as_ref().unwrap_or(&self.directory);
        let mut board = snapshot.board;
        let images: HashMap<_, _> = snapshot
            .images
            .into_iter()
            .map(|(id, revision, image)| (id, (revision, image)))
            .collect();
        let mut entries = HashMap::new();
        let mut created = vec![];
        let contents = (|| -> Result<String> {
            for pin in &mut board.pins {
                let (revision, pixels) = images
                    .get(&pin.id)
                    .ok_or_else(|| t("Invalid saved pin data"))?;
                let existing = self.entries.get(&pin.id).filter(|(r, _)| r == revision);
                let name = if next.is_none() {
                    existing.map(|(_, s)| s.clone())
                } else {
                    None
                };
                let name = match name {
                    Some(s) => s,
                    None => {
                        let name = format!("{}.png", uuid());
                        let path = directory.path.join(&name);
                        // Register before writing so partial files are rolled back.
                        created.push(path.clone());
                        if let Some((_, old)) = existing {
                            fs::copy(self.directory.path.join(old), &path)
                                .map_err(|e| e.to_string())?;
                            use std::os::unix::fs::PermissionsExt;
                            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
                                .map_err(|e| e.to_string())?;
                            // A durable manifest must never precede the copied
                            // image data when migrating from memory to disk.
                            fs::File::open(&path)
                                .and_then(|file| file.sync_all())
                                .map_err(|e| e.to_string())?;
                        } else {
                            let mut file = OpenOptions::new()
                                .write(true)
                                .create_new(true)
                                .mode(0o600)
                                .open(&path)
                                .map_err(|e| e.to_string())?;
                            let pixels =
                                pixels.as_ref().ok_or_else(|| t("Invalid saved pin data"))?;
                            let mut png = std::io::Cursor::new(Vec::new());
                            pixels
                                .write_to(&mut png, clivuu_core::image::ImageFormat::Png)
                                .map_err(|e| e.to_string())?;
                            if png.get_ref().len() as u64 > clivuu_core::image_ops::MAX_BYTES {
                                return Err(t("The image file exceeds 64 MiB"));
                            }
                            file.write_all(png.get_ref()).map_err(|e| e.to_string())?;
                            file.sync_all().map_err(|e| e.to_string())?;
                        }
                        name
                    }
                };
                pin.image = name.clone();
                entries.insert(pin.id, (*revision, name));
            }
            let contents = serde_json::to_string(&board).map_err(|e| e.to_string())?;
            if (contents != self.last || next.is_some())
                && let Some(warning) =
                    atomic(&directory.path.join("pins.json"), contents.as_bytes())?
            {
                eprintln!("{warning}");
            }
            Ok(contents)
        })();
        let contents = match contents {
            Ok(c) => c,
            Err(e) => {
                for file in created {
                    let _ = fs::remove_file(file);
                }
                return Err(e);
            }
        };
        self.owned.extend(created);
        self.entries = entries;
        self.last = contents;
        let old = next.map(|directory| std::mem::replace(&mut self.directory, directory));
        let retained: HashSet<_> = self
            .entries
            .values()
            .map(|(_, s)| self.directory.path.join(s))
            .collect();
        self.owned.retain(|p| {
            if retained.contains(p) {
                true
            } else {
                let _ = fs::remove_file(p);
                false
            }
        });
        if let Some(old) = old {
            let _ = fs::remove_file(old.path.join("pins.json"));
        }
        Ok(self
            .entries
            .iter()
            .map(|(id, (revision, _))| (*id, *revision))
            .collect())
    }
}
fn image_name(s: &str) -> bool {
    s.len() == 40
        && s.ends_with(".png")
        && s.as_bytes()[..36]
            .iter()
            .copied()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase() || b == b'-')
}
fn uuid() -> String {
    gtk::glib::uuid_string_random().to_string()
}
fn atomic(path: &Path, bytes: &[u8]) -> Result<Option<String>> {
    atomic_with_sync(path, bytes, |p| File::open(p)?.sync_all())
}
fn atomic_with_sync(
    path: &Path,
    bytes: &[u8],
    sync: impl FnOnce(&Path) -> std::io::Result<()>,
) -> Result<Option<String>> {
    let temporary = path.with_file_name(format!(".pins-{}.tmp", uuid()));
    let result = (|| {
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result.map_err(|e: std::io::Error| e.to_string())?;
    // Rename already committed this manifest. A durability warning must never
    // cause publish() to roll back images referenced by the new manifest.
    Ok(sync(path.parent().unwrap()).err().map(|e| e.to_string()))
}
#[cfg(test)]
mod tests {
    use super::*;
    use clivuu_core::{annotation::Bounds, image::Rgba};
    #[test]
    fn successful_rename_is_not_rolled_back_after_directory_sync_error() {
        let path = PathBuf::from(format!("/tmp/clivuu-store-test-{}", uuid()));
        fs::create_dir(&path).unwrap();
        let manifest = path.join("pins.json");
        assert!(
            atomic_with_sync(&manifest, b"published", |_| Err(std::io::Error::other(
                "injected sync failure"
            )))
            .unwrap()
            .is_some()
        );
        assert_eq!(fs::read(&manifest).unwrap(), b"published");
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn immutable_revisions_manifest_and_lease() {
        let path = PathBuf::from(format!("/tmp/clivuu-store-test-{}", uuid()));
        let (mut store, _) = PinStore::load(Directory::at(path.clone(), false).unwrap()).unwrap();
        assert!(Directory::at(path.clone(), false).is_err());
        let mut board = Board::default();
        let id = board
            .add(Bounds {
                x: 0.,
                y: 0.,
                width: 100.,
                height: 80.,
            })
            .unwrap();
        let image = Arc::new(RgbaImage::from_pixel(100, 80, Rgba([32, 64, 128, 255])));
        let snap = |board: Board, revision| Snapshot {
            board,
            images: vec![(id, revision, Some(image.clone()))],
        };
        store.publish(snap(board.clone(), 1), false).unwrap();
        let first = fs::read_to_string(path.join("pins.json")).unwrap();
        store.publish(snap(board.clone(), 1), false).unwrap();
        assert_eq!(fs::read_to_string(path.join("pins.json")).unwrap(), first);
        let old: Board = serde_json::from_str(&first).unwrap();
        store.publish(snap(board.clone(), 2), false).unwrap();
        assert!(!path.join(&old.pins[0].image).exists());
        board.close(id, 1);
        store.publish(snap(board, 2), false).unwrap();
        drop(store);
        let (store, restored) =
            PinStore::load(Directory::at(path.clone(), false).unwrap()).unwrap();
        assert!(restored.board.pins[0].closed);
        assert!(restored.images.is_empty());
        assert_eq!(restored.closed, vec![id]);
        assert_eq!(store.image(id).unwrap().1.as_ref(), image.as_ref());
        drop(store);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn archived_snapshots_reuse_revisions_without_decoding() {
        let path = PathBuf::from(format!("/tmp/clivuu-store-test-{}", uuid()));
        let (mut store, _) = PinStore::load(Directory::at(path.clone(), false).unwrap()).unwrap();
        let mut board = Board::default();
        let pixels = Arc::new(RgbaImage::new(16, 16));
        let mut images = vec![];
        for n in 0..80 {
            let id = board
                .add(Bounds {
                    x: 0.,
                    y: 0.,
                    width: 16.,
                    height: 16.,
                })
                .unwrap();
            board.close(id, n);
            images.push((id, 0, Some(pixels.clone())));
        }
        store
            .publish(
                Snapshot {
                    board: board.clone(),
                    images,
                },
                false,
            )
            .unwrap();
        assert_eq!(Arc::strong_count(&pixels), 1);
        drop(store);
        let (mut store, restored) =
            PinStore::load(Directory::at(path.clone(), false).unwrap()).unwrap();
        assert_eq!(restored.closed.len(), 80);
        assert!(restored.images.is_empty());
        let files: Vec<_> = store
            .entries
            .values()
            .map(|(_, name)| name.clone())
            .collect();
        let images = restored.closed.iter().map(|id| (*id, 0, None)).collect();
        store
            .publish(
                Snapshot {
                    board: restored.board,
                    images,
                },
                false,
            )
            .unwrap();
        assert!(files.iter().all(|name| path.join(name).is_file()));
        assert_eq!(
            store.image(board.pins[79].id).unwrap().1.dimensions(),
            (16, 16)
        );
        drop(store);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn corrupt_manifest_is_preserved_and_traversal_rejected() {
        let path = PathBuf::from(format!("/tmp/clivuu-store-test-{}", uuid()));
        let directory = Directory::at(path.clone(), false).unwrap();
        fs::write(path.join("pins.json"), b"broken").unwrap();
        let (store, r) = PinStore::load(directory).unwrap();
        assert!(r.recovery.is_some());
        assert!(r.board.pins.is_empty());
        assert!(fs::read_dir(&path).unwrap().any(|p| {
            p.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("pins-recovery-")
        }));
        assert!(!image_name("../../../../../../../../../../../test.png"));
        drop(store);
        fs::remove_dir_all(path).unwrap();
    }
}

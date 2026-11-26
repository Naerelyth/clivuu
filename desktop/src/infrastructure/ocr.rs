//! Out-of-process single-shot OCR execution and explicit user-triggered model/runtime downloads.
//!
//! Every recognition spawns a short-lived GUI-free worker (`clivuu worker ocr-document` or
//! `clivuu-desktop --ocr-worker`) over anonymous in-memory file descriptors (`memfd_create`) with
//! `PR_SET_PDEATHSIG = SIGKILL`, and reaps the child immediately on completion or cancellation.
use crate::infrastructure::tasks::{Cancellation, TaskError, TaskPool};
use clivuu_core::{
    i18n::{t, tf},
    image::RgbaImage,
    ocr::{
        OcrDocument, OcrSettings,
        assets::{
            self, Asset, DETECTOR, MODELS, RuntimeDistribution, asset_on_disk,
            current_runtime_distribution, extract_runtime_archive, find_model_spec,
            find_onnxruntime_library, mirror_url_candidates, models_dir, runtime_root,
            verify_asset,
        },
        worker,
    },
};
use gtk::{gio, glib};
use std::{
    cell::RefCell,
    collections::HashMap,
    fs::{self, File},
    io::{self, Seek, SeekFrom},
    os::{
        fd::FromRawFd,
        unix::{fs::PermissionsExt, process::CommandExt},
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const WORKER_TIMEOUT: Duration = Duration::from_secs(90);

enum WorkerEntry {
    Cli(PathBuf),
    Desktop(PathBuf),
    InProcessFallback,
}

fn resolve_worker_entry() -> WorkerEntry {
    let is_exec = |p: &Path| {
        p.is_absolute()
            && fs::metadata(p).is_ok_and(|m| m.is_file() && (m.permissions().mode() & 0o111) != 0)
    };

    if let Some(appdir) = std::env::var_os("APPDIR").map(PathBuf::from) {
        let bundled_cli = appdir.join("usr/bin/clivuu");
        if is_exec(&bundled_cli) {
            return WorkerEntry::Cli(bundled_cli);
        }
    }
    if let Some(cli) = std::env::var_os("CLIVUU_BINARY").map(PathBuf::from)
        && !cli
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("AppImage"))
        && is_exec(&cli)
    {
        return WorkerEntry::Cli(cli);
    }
    if let Ok(exe) = std::env::current_exe() {
        let sibling_cli = exe.with_file_name("clivuu");
        if is_exec(&sibling_cli) {
            return WorkerEntry::Cli(sibling_cli);
        }
        // In `/usr/libexec/clivuu-desktop` installs, check `../bin/clivuu` or `/usr/bin/clivuu`.
        if let Some(prefix) = exe.parent().and_then(Path::parent) {
            let prefix_cli = prefix.join("bin/clivuu");
            if is_exec(&prefix_cli) {
                return WorkerEntry::Cli(prefix_cli);
            }
        }
        let name = exe.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name == "clivuu" && is_exec(&exe) {
            return WorkerEntry::Cli(exe);
        }
        if name == "clivuu-desktop" && is_exec(&exe) {
            return WorkerEntry::Desktop(exe);
        }
    }
    WorkerEntry::InProcessFallback
}

fn create_anonymous_file(label: &std::ffi::CStr) -> io::Result<File> {
    // SAFETY: `label` is a valid NUL-terminated C string; `0` leaves fd inheritable across `dup2`
    // for child `Stdio`, while Rust's `File` closes the parent copy on drop.
    let fd = unsafe { libc::memfd_create(label.as_ptr(), libc::MFD_CLOEXEC) };
    if fd >= 0 {
        // SAFETY: fresh uniquely owned file descriptor returned by `memfd_create`.
        return Ok(unsafe { File::from_raw_fd(fd) });
    }
    let dir = std::env::temp_dir();
    let unique = format!(
        ".clivuu-ocr-{}-{}",
        std::process::id(),
        glib::uuid_string_random()
    );
    let path = dir.join(unique);
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)?;
    let _ = fs::remove_file(&path);
    Ok(file)
}

pub fn recognize_blocking(
    image: &RgbaImage,
    settings: &OcrSettings,
    cancel: &Cancellation,
) -> Result<OcrDocument, TaskError> {
    cancel.check()?;
    let entry = resolve_worker_entry();
    let mut command = match entry {
        WorkerEntry::Cli(bin) => {
            let mut cmd = Command::new(bin);
            cmd.args(["worker", "ocr-document"]);
            cmd
        }
        WorkerEntry::Desktop(bin) => {
            let mut cmd = Command::new(bin);
            cmd.arg("--ocr-worker");
            cmd
        }
        WorkerEntry::InProcessFallback => {
            let doc = clivuu_core::ocr::recognize_document(image, settings)
                .map_err(|e| TaskError::Failed(e.to_string()))?;
            cancel.check()?;
            return Ok(doc);
        }
    };

    let mut input =
        create_anonymous_file(c"clivuu-ocr-in").map_err(|e| TaskError::Failed(e.to_string()))?;
    worker::write_request(&mut input, image, settings)
        .map_err(|e| TaskError::Failed(e.to_string()))?;
    input
        .seek(SeekFrom::Start(0))
        .map_err(|e| TaskError::Failed(e.to_string()))?;
    let mut output =
        create_anonymous_file(c"clivuu-ocr-out").map_err(|e| TaskError::Failed(e.to_string()))?;
    let out_child = output
        .try_clone()
        .map_err(|e| TaskError::Failed(e.to_string()))?;

    if let Ok(preload) = std::env::var("LD_PRELOAD") {
        command.env(
            "LD_PRELOAD",
            crate::platform::display::without_layer_shell(&preload),
        );
    }
    command
        .stdin(Stdio::from(input))
        .stdout(Stdio::from(out_child))
        .stderr(Stdio::null());

    let parent = std::process::id() as libc::pid_t;
    // SAFETY: async-signal-safe syscalls (`prctl`, `getppid`) only between `fork` and `exec`.
    unsafe {
        command.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) < 0 {
                return Err(io::Error::last_os_error());
            }
            if libc::getppid() != parent {
                return Err(io::Error::from_raw_os_error(libc::ESRCH));
            }
            Ok(())
        });
    }

    let mut child = command
        .spawn()
        .map_err(|e| TaskError::Failed(e.to_string()))?;

    let deadline = Instant::now() + WORKER_TIMEOUT;
    let outcome = loop {
        if cancel.cancelled() {
            break Err(TaskError::Cancelled);
        }
        if Instant::now() >= deadline {
            break Err(TaskError::Failed(t("OCR recognition timed out")));
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    break Err(TaskError::Failed(t("OCR recognition failed")));
                }
                if let Err(e) = output.seek(SeekFrom::Start(0)) {
                    break Err(TaskError::Failed(e.to_string()));
                }
                break worker::read_reply(&mut output)
                    .map_err(|e| TaskError::Failed(e.to_string()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(15)),
            Err(e) => break Err(TaskError::Failed(e.to_string())),
        }
    };

    if outcome.is_err() {
        let _ = child.kill();
    }
    let _ = child.wait();
    outcome
}

pub async fn recognize(
    image: Arc<RgbaImage>,
    settings: OcrSettings,
    pool: &Rc<TaskPool>,
    cancel: &gio::Cancellable,
) -> Result<OcrDocument, String> {
    let task = pool
        .submit(move |token| recognize_blocking(&image, &settings, &token))
        .map_err(|e| e.to_string())?;
    gio::CancellableFuture::new(task.result(), cancel.clone())
        .await
        .map_err(|_| t("Operation cancelled"))?
        .map_err(|e| match e {
            TaskError::Cancelled => t("Operation cancelled"),
            other => other.to_string(),
        })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DownloadState {
    Idle,
    Downloading(u8),
    Installed,
    Failed(String),
}

struct ActiveJob {
    cancel: Arc<AtomicBool>,
}

/// Shared UI-thread coordinator for explicit user-initiated OCR model and ONNX Runtime downloads.
pub struct OcrDownloadManager {
    jobs: RefCell<HashMap<String, ActiveJob>>,
    states: RefCell<HashMap<String, DownloadState>>,
    listeners: RefCell<Vec<Box<dyn Fn()>>>,
}

impl Default for OcrDownloadManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for OcrDownloadManager {
    fn drop(&mut self) {
        self.cancel_all();
    }
}

impl OcrDownloadManager {
    pub fn new() -> Self {
        let this = Self {
            jobs: RefCell::new(HashMap::new()),
            states: RefCell::new(HashMap::new()),
            listeners: RefCell::new(Vec::new()),
        };
        this.refresh_states();
        this
    }

    pub fn shared() -> Rc<Self> {
        thread_local! {
            static INSTANCE: Rc<OcrDownloadManager> = Rc::new(OcrDownloadManager::new());
        }
        INSTANCE.with(Rc::clone)
    }

    pub fn connect_changed(&self, callback: impl Fn() + 'static) {
        self.listeners.borrow_mut().push(Box::new(callback));
    }

    fn notify(&self) {
        for listener in self.listeners.borrow().iter() {
            listener();
        }
    }

    pub fn refresh_states(&self) {
        let mut states = self.states.borrow_mut();
        let active_jobs = self.jobs.borrow();
        let rt_installed = find_onnxruntime_library().is_some();
        if !active_jobs.contains_key("runtime") {
            states.insert(
                "runtime".to_owned(),
                if rt_installed {
                    DownloadState::Installed
                } else {
                    DownloadState::Idle
                },
            );
        }
        for spec in MODELS {
            if active_jobs.contains_key(spec.id) {
                continue;
            }
            let installed = assets::is_model_installed(spec.id);
            let next_state = if installed {
                DownloadState::Installed
            } else {
                match states.get(spec.id) {
                    Some(DownloadState::Failed(msg)) => DownloadState::Failed(msg.clone()),
                    _ => DownloadState::Idle,
                }
            };
            states.insert(spec.id.to_owned(), next_state);
        }
    }

    pub fn state(&self, id: &str) -> DownloadState {
        self.states
            .borrow()
            .get(id)
            .cloned()
            .unwrap_or(DownloadState::Idle)
    }

    /// Total download bytes needed for `model_id` (including shared detector and ONNX Runtime if missing).
    pub fn bundle_download_bytes(&self, model_id: &str, include_runtime_if_missing: bool) -> u64 {
        let Some(spec) = find_model_spec(model_id) else {
            return 0;
        };
        let mut bytes = 0u64;
        if let Ok(dir) = models_dir() {
            if !asset_on_disk(&dir, &DETECTOR) {
                bytes += DETECTOR.size;
            }
            if !asset_on_disk(&dir, &spec.recognizer) {
                bytes += spec.recognizer.size;
            }
            if !asset_on_disk(&dir, &spec.dict) {
                bytes += spec.dict.size;
            }
        } else {
            bytes += DETECTOR.size + spec.recognizer.size + spec.dict.size;
        }
        if include_runtime_if_missing
            && find_onnxruntime_library().is_none()
            && let Some(dist) = current_runtime_distribution()
        {
            bytes += dist.archive.size;
        }
        bytes
    }

    /// Explicitly start downloading `model_id` (and ONNX Runtime if missing when `ensure_runtime` is true).
    pub fn start_model_download(
        self: &Rc<Self>,
        model_id: &str,
        ensure_runtime: bool,
        mirror: &str,
    ) {
        if self.jobs.borrow().contains_key(model_id) {
            return;
        }
        let Some(spec) = find_model_spec(model_id) else {
            return;
        };
        let need_runtime = ensure_runtime && find_onnxruntime_library().is_none();
        let cancel = Arc::new(AtomicBool::new(false));
        let key = model_id.to_owned();
        self.jobs.borrow_mut().insert(
            key.clone(),
            ActiveJob {
                cancel: cancel.clone(),
            },
        );
        self.states
            .borrow_mut()
            .insert(key.clone(), DownloadState::Downloading(0));
        if need_runtime {
            self.states
                .borrow_mut()
                .insert("runtime".to_owned(), DownloadState::Downloading(0));
        }
        self.notify();

        let mirror = mirror.to_owned();
        let (tx, rx) = async_channel::unbounded::<Result<u8, String>>();
        let cancel_worker = cancel.clone();
        std::thread::spawn(move || {
            let outcome = run_bundle_download(spec, need_runtime, &mirror, &cancel_worker, |pct| {
                let _ = tx.send_blocking(Ok(pct.min(99)));
            });
            match outcome {
                Ok(()) => {
                    let _ = tx.send_blocking(Ok(100));
                }
                Err(err) => {
                    let _ = tx.send_blocking(Err(err));
                }
            }
        });

        let manager = Rc::clone(self);
        glib::MainContext::default().spawn_local(async move {
            while let Ok(msg) = rx.recv().await {
                let cancelled = cancel.load(Ordering::Relaxed);
                match msg {
                    Ok(100) => {
                        manager.jobs.borrow_mut().remove(&key);
                        manager.refresh_states();
                        manager.notify();
                        break;
                    }
                    Ok(pct) => {
                        if !cancelled {
                            manager
                                .states
                                .borrow_mut()
                                .insert(key.clone(), DownloadState::Downloading(pct));
                            if need_runtime && find_onnxruntime_library().is_none() {
                                manager
                                    .states
                                    .borrow_mut()
                                    .insert("runtime".to_owned(), DownloadState::Downloading(pct));
                            }
                            manager.notify();
                        }
                    }
                    Err(err) => {
                        manager.jobs.borrow_mut().remove(&key);
                        if cancelled {
                            manager
                                .states
                                .borrow_mut()
                                .insert(key.clone(), DownloadState::Idle);
                        } else {
                            manager
                                .states
                                .borrow_mut()
                                .insert(key.clone(), DownloadState::Failed(err));
                        }
                        manager.refresh_states();
                        manager.notify();
                        break;
                    }
                }
            }
        });
    }

    /// Explicitly start downloading ONNX Runtime standalone.
    pub fn start_runtime_download(self: &Rc<Self>, mirror: &str) {
        if self.jobs.borrow().contains_key("runtime") {
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.jobs.borrow_mut().insert(
            "runtime".to_owned(),
            ActiveJob {
                cancel: cancel.clone(),
            },
        );
        self.states
            .borrow_mut()
            .insert("runtime".to_owned(), DownloadState::Downloading(0));
        self.notify();

        let mirror = mirror.to_owned();
        let (tx, rx) = async_channel::unbounded::<Result<u8, String>>();
        let cancel_worker = cancel.clone();
        std::thread::spawn(move || {
            let outcome = install_runtime_only(&mirror, &cancel_worker, |pct| {
                let _ = tx.send_blocking(Ok(pct.min(99)));
            });
            match outcome {
                Ok(()) => {
                    let _ = tx.send_blocking(Ok(100));
                }
                Err(err) => {
                    let _ = tx.send_blocking(Err(err));
                }
            }
        });

        let manager = Rc::clone(self);
        glib::MainContext::default().spawn_local(async move {
            while let Ok(msg) = rx.recv().await {
                let cancelled = cancel.load(Ordering::Relaxed);
                match msg {
                    Ok(100) => {
                        manager.jobs.borrow_mut().remove("runtime");
                        manager.refresh_states();
                        manager.notify();
                        break;
                    }
                    Ok(pct) => {
                        if !cancelled {
                            manager
                                .states
                                .borrow_mut()
                                .insert("runtime".to_owned(), DownloadState::Downloading(pct));
                            manager.notify();
                        }
                    }
                    Err(err) => {
                        manager.jobs.borrow_mut().remove("runtime");
                        manager.states.borrow_mut().insert(
                            "runtime".to_owned(),
                            if cancelled {
                                DownloadState::Idle
                            } else {
                                DownloadState::Failed(err)
                            },
                        );
                        manager.refresh_states();
                        manager.notify();
                        break;
                    }
                }
            }
        });
    }

    pub fn cancel(&self, id: &str) {
        if let Some(job) = self.jobs.borrow_mut().remove(id) {
            job.cancel.store(true, Ordering::Relaxed);
        }
        self.states
            .borrow_mut()
            .insert(id.to_owned(), DownloadState::Idle);
        self.refresh_states();
        self.notify();
    }

    pub fn cancel_all(&self) {
        for (_, job) in self.jobs.borrow_mut().drain() {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }

    pub fn remove_model(&self, model_id: &str) -> Result<(), String> {
        self.cancel(model_id);
        let Some(spec) = find_model_spec(model_id) else {
            return Ok(());
        };
        let dir = models_dir().map_err(|e| e.to_string())?;
        for file in [spec.recognizer.file, spec.dict.file] {
            let p = dir.join(file);
            if p.exists() {
                fs::remove_file(&p).map_err(|e| e.to_string())?;
            }
        }
        let any_other = MODELS.iter().any(|m| {
            m.id != model_id && asset_on_disk(&dir, &m.recognizer) && asset_on_disk(&dir, &m.dict)
        });
        if !any_other {
            let det = dir.join(DETECTOR.file);
            if det.exists() {
                let _ = fs::remove_file(det);
            }
        }
        self.states
            .borrow_mut()
            .insert(model_id.to_owned(), DownloadState::Idle);
        self.refresh_states();
        self.notify();
        Ok(())
    }
}

fn install_runtime_only(
    mirror: &str,
    cancel: &AtomicBool,
    on_progress: impl Fn(u8),
) -> Result<(), String> {
    let dist = current_runtime_distribution()
        .ok_or_else(|| t("ONNX Runtime is not supported on this CPU architecture"))?;
    download_and_extract_runtime(&dist, mirror, cancel, |done, total| {
        on_progress(((done * 95) / total.max(1)) as u8);
    })?;
    on_progress(100);
    Ok(())
}

fn run_bundle_download(
    spec: &assets::ModelSpec,
    need_runtime: bool,
    mirror: &str,
    cancel: &AtomicBool,
    on_progress: impl Fn(u8),
) -> Result<(), String> {
    let model_dir = models_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&model_dir).map_err(|e| e.to_string())?;

    let rt_dist = if need_runtime {
        Some(
            current_runtime_distribution()
                .ok_or_else(|| t("ONNX Runtime is not supported on this CPU architecture"))?,
        )
    } else {
        None
    };

    let model_assets = [DETECTOR, spec.recognizer, spec.dict];
    let total_bytes: u64 = model_assets.iter().map(|a| a.size).sum::<u64>()
        + rt_dist.as_ref().map_or(0, |d| d.archive.size);
    let mut completed_bytes = 0u64;

    if let Some(dist) = &rt_dist {
        download_and_extract_runtime(dist, mirror, cancel, |done, _| {
            let pct = (((completed_bytes + done) * 98) / total_bytes.max(1)) as u8;
            on_progress(pct);
        })?;
        completed_bytes += dist.archive.size;
    }

    for asset in model_assets {
        if cancel.load(Ordering::Relaxed) {
            return Err(t("Operation cancelled"));
        }
        let final_path = model_dir.join(asset.file);
        if verify_asset(&final_path, &asset).is_ok() {
            completed_bytes += asset.size;
            let pct = ((completed_bytes * 99) / total_bytes.max(1)) as u8;
            on_progress(pct);
            continue;
        }
        let original = format!("{}/{}", assets::MODEL_BASE_URL, asset.file);
        let urls = mirror_url_candidates(&original, mirror);
        fetch_verified_asset(&urls, &asset, &model_dir, cancel, |bytes| {
            let pct = (((completed_bytes + bytes) * 99) / total_bytes.max(1)) as u8;
            on_progress(pct);
        })?;
        completed_bytes += asset.size;
    }
    Ok(())
}

fn download_and_extract_runtime(
    dist: &RuntimeDistribution,
    mirror: &str,
    cancel: &AtomicBool,
    on_bytes: impl Fn(u64, u64),
) -> Result<(), String> {
    let root = runtime_root().map_err(|e| e.to_string())?;
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let installed = root.join("installed");
    if assets::verify_runtime_dir(&installed, dist).is_ok() {
        on_bytes(dist.archive.size, dist.archive.size);
        return Ok(());
    }
    let original = format!("{}/{}", dist.base_url, dist.archive.file);
    let urls = mirror_url_candidates(&original, mirror);
    let archive_path = fetch_verified_asset(&urls, &dist.archive, &root, cancel, |bytes| {
        on_bytes(bytes, dist.archive.size);
    })?;

    let stage = root.join(format!("stage-{}", glib::uuid_string_random()));
    let extract_res = extract_runtime_archive(&archive_path, &stage, dist, cancel);
    if let Err(err) = extract_res {
        let _ = fs::remove_dir_all(&stage);
        return Err(err.to_string());
    }
    if cancel.load(Ordering::Relaxed) {
        let _ = fs::remove_dir_all(&stage);
        return Err(t("Operation cancelled"));
    }
    if installed.exists() {
        let _ = fs::remove_dir_all(&installed);
    }
    fs::rename(&stage, &installed).map_err(|e| {
        let _ = fs::remove_dir_all(&stage);
        e.to_string()
    })?;
    let _ = fs::remove_file(&archive_path);
    Ok(())
}

fn fetch_verified_asset(
    urls: &[String],
    asset: &Asset,
    dir: &Path,
    cancel: &AtomicBool,
    on_bytes: impl Fn(u64),
) -> Result<PathBuf, String> {
    let curl = glib::find_program_in_path("curl")
        .ok_or_else(|| t("Install curl to download OCR models, or place model files manually"))?;
    let final_path = dir.join(asset.file);
    let part_path = dir.join(format!("{}.part", asset.file));

    let mut last_error = t("Download failed");
    for url in urls {
        for attempt in 0..2 {
            if cancel.load(Ordering::Relaxed) {
                return Err(t("Operation cancelled"));
            }
            let existing_len = fs::metadata(&part_path).map_or(0, |m| m.len());
            if existing_len > asset.size {
                let _ = fs::remove_file(&part_path);
            }
            let resume = attempt == 0 && existing_len > 0 && existing_len < asset.size;

            let mut cmd = Command::new(&curl);
            cmd.args([
                "--fail",
                "--location",
                "--silent",
                "--show-error",
                "--connect-timeout",
                "15",
                "--speed-time",
                "25",
                "--speed-limit",
                "1024",
            ]);
            if resume {
                cmd.args(["-C", "-"]);
            }
            cmd.arg("-o").arg(&part_path).arg(url);
            cmd.stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped());

            let parent = std::process::id() as libc::pid_t;
            // SAFETY: async-signal-safe `prctl` + `getppid` before `exec`.
            unsafe {
                cmd.pre_exec(move || {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) < 0 {
                        return Err(io::Error::last_os_error());
                    }
                    if libc::getppid() != parent {
                        return Err(io::Error::from_raw_os_error(libc::ESRCH));
                    }
                    Ok(())
                });
            }

            let mut child = match cmd.spawn() {
                Ok(c) => c,
                Err(e) => {
                    last_error = tf("Failed to start curl: {0}", &[&e.to_string()]);
                    break;
                }
            };

            let status = loop {
                if cancel.load(Ordering::Relaxed) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(t("Operation cancelled"));
                }
                let current_bytes = fs::metadata(&part_path)
                    .map_or(0, |m| m.len())
                    .min(asset.size);
                on_bytes(current_bytes);
                match child.try_wait() {
                    Ok(Some(st)) => break Ok(st),
                    Ok(None) => std::thread::sleep(Duration::from_millis(80)),
                    Err(e) => break Err(e.to_string()),
                }
            };

            match status {
                Ok(st) if st.success() => match verify_asset(&part_path, asset) {
                    Ok(()) => {
                        fs::rename(&part_path, &final_path).map_err(|e| e.to_string())?;
                        on_bytes(asset.size);
                        return Ok(final_path);
                    }
                    Err(verify_err) => {
                        let _ = fs::remove_file(&part_path);
                        last_error = verify_err.to_string();
                        if attempt == 0 {
                            continue;
                        }
                    }
                },
                Ok(_) => {
                    let mut stderr_msg = String::new();
                    if let Some(mut err_pipe) = child.stderr.take() {
                        let _ = io::Read::read_to_string(&mut err_pipe, &mut stderr_msg);
                    }
                    let _ = child.wait();
                    last_error = if stderr_msg.trim().is_empty() {
                        tf("Failed to download {0}", &[asset.file])
                    } else {
                        stderr_msg.trim().to_owned()
                    };
                    break;
                }
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    last_error = e;
                    break;
                }
            }
        }
    }
    Err(last_error)
}

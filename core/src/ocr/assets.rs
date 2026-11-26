//! Pinned PP-OCRv5 Mobile & ONNX Runtime manifests, SHA-256 verification, and archive extraction.
use crate::{
    Result,
    i18n::{t, tf},
};
use flate2::read::{DeflateDecoder, GzDecoder};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{self, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

pub const MODEL_BASE_URL: &str = "https://github.com/GreatV/oar-ocr/releases/download/v0.3.0";
pub const ORT_VERSION: &str = "1.28.0";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Asset {
    pub file: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct ModelSpec {
    pub id: &'static str,
    pub glyph: &'static str,
    pub name_key: &'static str,
    pub note_key: &'static str,
    pub recognizer: Asset,
    pub dict: Asset,
}

impl ModelSpec {
    pub fn localized_name(&self) -> String {
        t(self.name_key)
    }

    pub fn localized_note(&self) -> String {
        t(self.note_key)
    }
}

pub const DETECTOR: Asset = Asset {
    file: "pp-ocrv5_mobile_det.onnx",
    size: 4_826_518,
    sha256: "1eb7b4f7ab657ebd1c66d5f79bca7497f29768a2e3c15e52daecbba1a8e4a039",
};

pub const MODELS: &[ModelSpec] = &[
    ModelSpec {
        id: "chinese",
        glyph: "文",
        name_key: "Chinese, Japanese & English",
        note_key: "Simplified and Traditional Chinese, Japanese, English, and digits",
        recognizer: Asset {
            file: "pp-ocrv5_mobile_rec.onnx",
            size: 16_562_373,
            sha256: "243a0f06d826761323e9045e9b113ab2c191c3aa50565585e628300b8eda0224",
        },
        dict: Asset {
            file: "ppocrv5_dict.txt",
            size: 74_012,
            sha256: "d1979e9f794c464c0d2e0b70a7fe14dd978e9dc644c0e71f14158cdf8342af1b",
        },
    },
    ModelSpec {
        id: "latin",
        glyph: "Aa",
        name_key: "Latin & European",
        note_key: "English, French, German, Spanish, and other Latin scripts",
        recognizer: Asset {
            file: "latin_pp-ocrv5_mobile_rec.onnx",
            size: 8_069_614,
            sha256: "e3a6bfeea1c8a01d6fccfd480a0bd363fd907f8c65931e228bb2736f5c3e142f",
        },
        dict: Asset {
            file: "ppocrv5_latin_dict.txt",
            size: 2_616,
            sha256: "ccbcc45730b3fbbd9050c5bc74db6a99067141ef1035e3d14889a84a6b9b1aff",
        },
    },
    ModelSpec {
        id: "korean",
        glyph: "한",
        name_key: "Korean",
        note_key: "Korean Hangul, English, and digits",
        recognizer: Asset {
            file: "korean_pp-ocrv5_mobile_rec.onnx",
            size: 13_446_374,
            sha256: "2d7ed96308065a86103325d22af07a88c4d06afc009f21602a4882342c0cc054",
        },
        dict: Asset {
            file: "ppocrv5_korean_dict.txt",
            size: 47_451,
            sha256: "a88071c68c01707489baa79ebe0405b7beb5cca229f4fc94cc3ef992328802d7",
        },
    },
    ModelSpec {
        id: "cyrillic",
        glyph: "Я",
        name_key: "Cyrillic",
        note_key: "Russian, Ukrainian, Bulgarian, and English",
        recognizer: Asset {
            file: "cyrillic_pp-ocrv5_mobile_rec.onnx",
            size: 8_076_390,
            sha256: "a18d96d7c8d73d90f2ed056549caa1de3a8e6cb744cccba16cd593ea8cd2d569",
        },
        dict: Asset {
            file: "ppocrv5_cyrillic_dict.txt",
            size: 2_781,
            sha256: "db40aa52ceb112055be80c694afdf655d5d2c4f7873704524cc16a447ca913ba",
        },
    },
    ModelSpec {
        id: "arabic",
        glyph: "ع",
        name_key: "Arabic",
        note_key: "Arabic, Persian, and Urdu",
        recognizer: Asset {
            file: "arabic_pp-ocrv5_mobile_rec.onnx",
            size: 8_026_538,
            sha256: "2768206d9a0ce48eba45b59619184e18161dde8f44115f029920ca17a9dc0384",
        },
        dict: Asset {
            file: "ppocrv5_arabic_dict.txt",
            size: 2_369,
            sha256: "7f92f7dbb9b75a4787a83bfb4f6d14a8ab515525130c9d40a9036f61cf6999e9",
        },
    },
    ModelSpec {
        id: "greek",
        glyph: "Ω",
        name_key: "Greek",
        note_key: "Modern Greek and English",
        recognizer: Asset {
            file: "el_pp-ocrv5_mobile_rec.onnx",
            size: 7_836_326,
            sha256: "5a4a020e48e8783e035e1af135423c2161a363acab9ef16e48238c3d181f0f71",
        },
        dict: Asset {
            file: "ppocrv5_el_dict.txt",
            size: 1_103,
            sha256: "31defc62c0c3ad3674a82da6192226a2ba98ef4ff014a7045cb88d59f9c3de31",
        },
    },
    ModelSpec {
        id: "thai",
        glyph: "ก",
        name_key: "Thai",
        note_key: "Thai script and English",
        recognizer: Asset {
            file: "th_pp-ocrv5_mobile_rec.onnx",
            size: 7_918_606,
            sha256: "5f6ee21242691681261fee01bc39867da9cc8ff9b889f2f048b3cb7f74380217",
        },
        dict: Asset {
            file: "ppocrv5_th_dict.txt",
            size: 1_767,
            sha256: "57f5406f94bb6688fb7077f7be65f08bbd71cecf48c01ea26c522cb5c4836b7a",
        },
    },
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelFiles {
    pub detector: PathBuf,
    pub recognizer: PathBuf,
    pub dict: PathBuf,
}

#[derive(Clone, Copy, Debug)]
pub struct RuntimeDistribution {
    pub platform: &'static str,
    pub base_url: &'static str,
    pub archive: Asset,
    pub library: Asset,
    pub shared: Asset,
    pub legacy_library: Asset,
}

pub fn current_runtime_distribution() -> Option<RuntimeDistribution> {
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some(RuntimeDistribution {
            platform: "linux-x64",
            base_url: "https://files.pythonhosted.org/packages/03/88/9432428273356ad3c8aa01f52c1b3e7f53c4c0192748f41ad983872b436b",
            archive: Asset {
                file: "onnxruntime-1.28.0-cp314-cp314-manylinux_2_27_x86_64.manylinux_2_28_x86_64.whl",
                size: 19_214_924,
                sha256: "6afdc83f1317c136e92fc29f5ee9f058de59d87c0b22cee3fdbfbaa0ccc2098a",
            },
            library: Asset {
                file: "libonnxruntime.so.1.28.0",
                size: 24_272_944,
                sha256: "aa4079d18f4ea7a5f3a94d80cd4bbe0f2740436626622d64d793803a20381083",
            },
            shared: Asset {
                file: "libonnxruntime_providers_shared.so",
                size: 14_632,
                sha256: "086ec1d5388f64153d9c63470d126693db9a182c8ce236d3a1119068471b8a0d",
            },
            legacy_library: Asset {
                file: "libonnxruntime.so.1.28.0",
                size: 24_268_848,
                sha256: "1461ef7cc3d9e49982591721683cc3e3a55580aeca9a5254e7aac47b75ee4bab",
            },
        })
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        Some(RuntimeDistribution {
            platform: "linux-aarch64",
            base_url: "https://files.pythonhosted.org/packages/3f/e9/139180b4dd810329aaa42c238b4e6383c906202d98609ae29d66eb7c32b1",
            archive: Asset {
                file: "onnxruntime-1.28.0-cp314-cp314-manylinux_2_27_aarch64.manylinux_2_28_aarch64.whl",
                size: 17_051_950,
                sha256: "bc2565e487b4896fb988d6383577d875d958e071fc5f6c3550bd5d02ae98264b",
            },
            library: Asset {
                file: "libonnxruntime.so.1.28.0",
                size: 20_657_248,
                sha256: "23a37c2994bc273e1d59a4f49af6cba5bf1cfbc29f04907ec27df375aba50a7d",
            },
            shared: Asset {
                file: "libonnxruntime_providers_shared.so",
                size: 198_792,
                sha256: "3b6be288fbfb7dff8770d08a23defdde18e8f7e0f5a2b344a0e5e238c999ea88",
            },
            legacy_library: Asset {
                file: "libonnxruntime.so.1.28.0",
                size: 20_591_712,
                sha256: "f1ec1a08eb99bd6e5401340f0a2b101381bf4694415480291dc13bcaa30f9ec7",
            },
        })
    } else {
        None
    }
}

pub fn ocr_root() -> Result<PathBuf> {
    Ok(crate::config::directory("XDG_DATA_HOME", ".local/share")?.join("ocr"))
}

pub fn models_dir() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("CLIVUU_OCR_MODELS_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
    {
        return Ok(dir);
    }
    Ok(ocr_root()?.join("models"))
}

pub fn runtime_root() -> Result<PathBuf> {
    let dist = current_runtime_distribution()
        .ok_or_else(|| t("ONNX Runtime is not supported on this CPU architecture"))?;
    Ok(ocr_root()?
        .join("runtime")
        .join(format!("onnxruntime-{ORT_VERSION}-{}", dist.platform)))
}

pub fn runtime_dir() -> Result<PathBuf> {
    Ok(runtime_root()?.join("installed"))
}

pub fn find_model_spec(id: &str) -> Option<&'static ModelSpec> {
    MODELS.iter().find(|m| m.id == id)
}

pub fn recommended_model_id() -> &'static str {
    let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|var| std::env::var(var).ok().filter(|v| !v.is_empty()))
        .unwrap_or_default();
    let lang = locale.split(['_', '.', '@']).next().unwrap_or_default();
    match lang {
        "zh" | "ja" => "chinese",
        "ko" => "korean",
        "ru" | "uk" | "be" | "bg" | "sr" | "mk" | "kk" | "ky" | "tg" | "mn" => "cyrillic",
        "ar" | "fa" | "ur" | "ps" | "ug" => "arabic",
        "el" => "greek",
        "th" => "thai",
        _ => "chinese",
    }
}

pub fn asset_on_disk(dir: &Path, asset: &Asset) -> bool {
    fs::metadata(dir.join(asset.file)).is_ok_and(|m| m.is_file() && m.len() == asset.size)
}

pub fn is_model_installed(model_id: &str) -> bool {
    let Ok(dir) = models_dir() else {
        return false;
    };
    let Some(spec) = find_model_spec(model_id) else {
        return false;
    };
    asset_on_disk(&dir, &DETECTOR)
        && asset_on_disk(&dir, &spec.recognizer)
        && asset_on_disk(&dir, &spec.dict)
}

pub fn installed_models() -> Vec<&'static ModelSpec> {
    let Ok(dir) = models_dir() else {
        return Vec::new();
    };
    if !asset_on_disk(&dir, &DETECTOR) {
        return Vec::new();
    }
    MODELS
        .iter()
        .filter(|spec| asset_on_disk(&dir, &spec.recognizer) && asset_on_disk(&dir, &spec.dict))
        .collect()
}

pub fn resolve_model_files(preferred_id: &str) -> Option<(&'static ModelSpec, ModelFiles)> {
    let dir = models_dir().ok()?;
    if !asset_on_disk(&dir, &DETECTOR) {
        return None;
    }
    let spec = find_model_spec(preferred_id)
        .filter(|s| asset_on_disk(&dir, &s.recognizer) && asset_on_disk(&dir, &s.dict))
        .or_else(|| {
            MODELS
                .iter()
                .find(|s| asset_on_disk(&dir, &s.recognizer) && asset_on_disk(&dir, &s.dict))
        })?;
    Some((
        spec,
        ModelFiles {
            detector: dir.join(DETECTOR.file),
            recognizer: dir.join(spec.recognizer.file),
            dict: dir.join(spec.dict.file),
        },
    ))
}

pub fn verify_asset(path: &Path, asset: &Asset) -> Result<()> {
    let meta = fs::metadata(path).map_err(|_| {
        tf(
            "OCR model file is missing: {0}",
            &[&path.display().to_string()],
        )
    })?;
    if !meta.is_file() || meta.len() != asset.size {
        return Err(tf("OCR asset size mismatch: {0}", &[asset.file]).into());
    }
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hex_lower(&hasher.finalize());
    if digest != asset.sha256 {
        return Err(tf("OCR asset checksum mismatch: {0}", &[asset.file]).into());
    }
    Ok(())
}

pub fn verify_model_files(spec: &ModelSpec, files: &ModelFiles) -> Result<()> {
    verify_asset(&files.detector, &DETECTOR)?;
    verify_asset(&files.recognizer, &spec.recognizer)?;
    verify_asset(&files.dict, &spec.dict)?;
    Ok(())
}

pub fn verify_runtime_dir(dir: &Path, dist: &RuntimeDistribution) -> Result<PathBuf> {
    verify_asset(&dir.join(dist.shared.file), &dist.shared)?;
    let lib_path = dir.join(dist.library.file);
    verify_asset(&lib_path, &dist.library)
        .or_else(|_| verify_asset(&lib_path, &dist.legacy_library))?;
    Ok(lib_path)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeSource {
    EnvOverride(PathBuf),
    System(PathBuf),
    Downloaded(PathBuf),
}

impl RuntimeSource {
    pub fn path(&self) -> &Path {
        match self {
            Self::EnvOverride(p) | Self::System(p) | Self::Downloaded(p) => p,
        }
    }
}

/// Discover a usable `libonnxruntime` shared library without network activity.
pub fn find_onnxruntime_library() -> Option<RuntimeSource> {
    if let Some(custom) = std::env::var_os("CLIVUU_ORT_LIB").map(PathBuf::from)
        && custom.is_file()
    {
        return Some(RuntimeSource::EnvOverride(custom));
    }

    const SYSTEM_CANDIDATES: &[&str] = &[
        "/usr/lib64/libonnxruntime.so.1",
        "/usr/lib/libonnxruntime.so.1",
        "/usr/lib/x86_64-linux-gnu/libonnxruntime.so.1",
        "/usr/lib/aarch64-linux-gnu/libonnxruntime.so.1",
        "/usr/local/lib64/libonnxruntime.so.1",
        "/usr/local/lib/libonnxruntime.so.1",
        "/usr/lib64/libonnxruntime.so",
        "/usr/lib/libonnxruntime.so",
    ];
    if std::env::var_os("CLIVUU_IGNORE_SYSTEM_ORT").is_none() {
        for candidate in SYSTEM_CANDIDATES {
            let path = Path::new(candidate);
            if path.is_file() {
                return Some(RuntimeSource::System(path.to_path_buf()));
            }
        }
    }

    let dist = current_runtime_distribution()?;
    let root = runtime_root().ok()?;
    for candidate_dir in [root.join("installed"), root] {
        let lib = candidate_dir.join(dist.library.file);
        if (asset_on_disk(&candidate_dir, &dist.library)
            || asset_on_disk(&candidate_dir, &dist.legacy_library))
            && asset_on_disk(&candidate_dir, &dist.shared)
        {
            return Some(RuntimeSource::Downloaded(lib));
        }
    }
    None
}

/// Build download candidate URLs, optionally prepending a user-configured GitHub mirror prefix
/// for `https://github.com/...` assets (PyPI CDN URLs stay direct first).
pub fn mirror_url_candidates(original: &str, configured_mirror: &str) -> Vec<String> {
    let mirror = std::env::var("CLIVUU_OCR_MIRROR")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| configured_mirror.trim().to_owned());
    let mut urls = Vec::new();
    if !mirror.is_empty() && original.starts_with("https://github.com/") {
        let base = mirror.trim_end_matches('/');
        if base.starts_with("https://") || base.starts_with("http://") {
            urls.push(format!("{base}/{original}"));
        }
    }
    urls.push(original.to_owned());
    urls
}

/// Extract only the allowlisted ONNX Runtime `.so` and license files from a `.whl` (ZIP) or `.tgz`
/// archive into `dest_dir`, verifying exact byte counts and SHA-256 digests before returning.
pub fn extract_runtime_archive(
    archive_path: &Path,
    dest_dir: &Path,
    dist: &RuntimeDistribution,
    cancel: &AtomicBool,
) -> Result<()> {
    fs::create_dir_all(dest_dir)?;
    let name = archive_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    if name.ends_with(".whl") || name.ends_with(".zip") {
        extract_zip_allowlist(archive_path, dest_dir, dist, cancel)?;
    } else {
        extract_tgz_allowlist(archive_path, dest_dir, dist, cancel)?;
    }
    verify_runtime_dir(dest_dir, dist)?;
    // Create SONAME symlink `libonnxruntime.so.1 -> libonnxruntime.so.1.28.0` if absent.
    #[cfg(unix)]
    {
        let soname = dest_dir.join("libonnxruntime.so.1");
        if !soname.exists() {
            let _ = std::os::unix::fs::symlink(dist.library.file, &soname);
        }
    }
    Ok(())
}

fn allowlisted_limit(file_name: &str, dist: &RuntimeDistribution) -> Option<u64> {
    if file_name == dist.library.file {
        Some(dist.library.size.max(dist.legacy_library.size))
    } else if file_name == dist.shared.file {
        Some(dist.shared.size)
    } else if matches!(file_name, "LICENSE" | "ThirdPartyNotices.txt") {
        Some(2 * 1024 * 1024)
    } else {
        None
    }
}

fn copy_bounded(
    reader: &mut dyn Read,
    out_path: &Path,
    expected_size: u64,
    limit: u64,
    cancel: &AtomicBool,
) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        return Err(t("Operation cancelled").into());
    }
    if expected_size > limit {
        return Err(t("Runtime archive contains an unexpectedly large entry").into());
    }
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(out_path)?;
    let mut buf = [0u8; 64 * 1024];
    let mut copied = 0u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(t("Operation cancelled").into());
        }
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        copied += n as u64;
        if copied > limit {
            return Err(t("Runtime archive contains an unexpectedly large entry").into());
        }
        io::Write::write_all(&mut output, &buf[..n])?;
    }
    if copied != expected_size {
        return Err(t("Runtime archive entry is truncated").into());
    }
    output.sync_all()?;
    Ok(())
}

/// Minimal pure-Rust ZIP central-directory reader supporting Store (0) and Deflate (8) entries.
fn extract_zip_allowlist(
    archive_path: &Path,
    dest_dir: &Path,
    dist: &RuntimeDistribution,
    cancel: &AtomicBool,
) -> Result<()> {
    let mut file = File::open(archive_path)?;
    let total_len = file.metadata()?.len();
    if total_len < 22 {
        return Err(t("Invalid runtime archive").into());
    }
    let search_len = total_len.min(65_557) as usize;
    file.seek(SeekFrom::End(-(search_len as i64)))?;
    let mut tail = vec![0u8; search_len];
    file.read_exact(&mut tail)?;

    let eocd_pos = (0..=tail.len() - 22)
        .rev()
        .find(|&i| tail[i..i + 4] == [0x50, 0x4b, 0x05, 0x06])
        .ok_or_else(|| t("Invalid runtime archive"))?;
    let eocd = &tail[eocd_pos..];
    let entry_count = u16::from_le_bytes([eocd[10], eocd[11]]) as usize;
    let cd_size = u32::from_le_bytes([eocd[12], eocd[13], eocd[14], eocd[15]]) as u64;
    let cd_offset = u32::from_le_bytes([eocd[16], eocd[17], eocd[18], eocd[19]]) as u64;
    if cd_offset + cd_size > total_len || cd_size > 16 * 1024 * 1024 {
        return Err(t("Invalid runtime archive").into());
    }

    file.seek(SeekFrom::Start(cd_offset))?;
    let mut cd = vec![0u8; cd_size as usize];
    file.read_exact(&mut cd)?;

    let mut pos = 0usize;
    for _ in 0..entry_count {
        if pos + 46 > cd.len() || cd[pos..pos + 4] != [0x50, 0x4b, 0x01, 0x02] {
            return Err(t("Invalid runtime archive").into());
        }
        let method = u16::from_le_bytes([cd[pos + 10], cd[pos + 11]]);
        let comp_size =
            u32::from_le_bytes([cd[pos + 20], cd[pos + 21], cd[pos + 22], cd[pos + 23]]) as u64;
        let uncomp_size =
            u32::from_le_bytes([cd[pos + 24], cd[pos + 25], cd[pos + 26], cd[pos + 27]]) as u64;
        let name_len = u16::from_le_bytes([cd[pos + 28], cd[pos + 29]]) as usize;
        let extra_len = u16::from_le_bytes([cd[pos + 30], cd[pos + 31]]) as usize;
        let comment_len = u16::from_le_bytes([cd[pos + 32], cd[pos + 33]]) as usize;
        let local_offset =
            u32::from_le_bytes([cd[pos + 42], cd[pos + 43], cd[pos + 44], cd[pos + 45]]) as u64;

        let name_end = pos + 46 + name_len;
        if name_end + extra_len + comment_len > cd.len() {
            return Err(t("Invalid runtime archive").into());
        }
        let raw_name = std::str::from_utf8(&cd[pos + 46..name_end]).unwrap_or("");
        pos = name_end + extra_len + comment_len;

        if raw_name.ends_with('/') || raw_name.contains("..") {
            continue;
        }
        let Some(base_name) = Path::new(raw_name).file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(limit) = allowlisted_limit(base_name, dist) else {
            continue;
        };

        file.seek(SeekFrom::Start(local_offset))?;
        let mut lfh = [0u8; 30];
        file.read_exact(&mut lfh)?;
        if lfh[0..4] != [0x50, 0x4b, 0x03, 0x04] {
            return Err(t("Invalid runtime archive").into());
        }
        let lfh_name_len = u16::from_le_bytes([lfh[26], lfh[27]]) as u64;
        let lfh_extra_len = u16::from_le_bytes([lfh[28], lfh[29]]) as u64;
        let data_offset = local_offset + 30 + lfh_name_len + lfh_extra_len;
        if data_offset + comp_size > total_len {
            return Err(t("Invalid runtime archive").into());
        }
        file.seek(SeekFrom::Start(data_offset))?;
        let bounded = (&mut file).take(comp_size);
        let out_path = dest_dir.join(base_name);
        match method {
            0 => {
                let mut reader = bounded;
                copy_bounded(&mut reader, &out_path, uncomp_size, limit, cancel)?;
            }
            8 => {
                let mut decoder = DeflateDecoder::new(bounded);
                copy_bounded(&mut decoder, &out_path, uncomp_size, limit, cancel)?;
            }
            _ => return Err(t("Unsupported compression method in runtime archive").into()),
        }
    }
    Ok(())
}

/// Minimal pure-Rust `.tar.gz` reader for legacy GitHub ONNX Runtime tarballs.
fn extract_tgz_allowlist(
    archive_path: &Path,
    dest_dir: &Path,
    dist: &RuntimeDistribution,
    cancel: &AtomicBool,
) -> Result<()> {
    let file = File::open(archive_path)?;
    let mut gz = GzDecoder::new(file);
    let mut header = [0u8; 512];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(t("Operation cancelled").into());
        }
        let mut read = 0usize;
        while read < 512 {
            let n = gz.read(&mut header[read..])?;
            if n == 0 {
                if read == 0 {
                    return Ok(());
                }
                return Err(t("Invalid runtime archive").into());
            }
            read += n;
        }
        if header.iter().all(|&b| b == 0) {
            return Ok(());
        }
        let typeflag = header[156];
        let size_str = std::str::from_utf8(&header[124..136])
            .unwrap_or("")
            .trim_matches(['\0', ' ']);
        let size = if size_str.is_empty() {
            0
        } else {
            u64::from_str_radix(size_str, 8).map_err(|_| t("Invalid runtime archive"))?
        };
        let name_raw = &header[0..100];
        let name_end = name_raw.iter().position(|&b| b == 0).unwrap_or(100);
        let name = std::str::from_utf8(&name_raw[..name_end]).unwrap_or("");
        let padded = size.div_ceil(512) * 512;

        let base_name = Path::new(name).file_name().and_then(|s| s.to_str());
        if (typeflag == b'0' || typeflag == 0)
            && !name.contains("..")
            && let Some(base) = base_name
            && let Some(limit) = allowlisted_limit(base, dist)
        {
            let mut entry = (&mut gz).take(size);
            copy_bounded(&mut entry, &dest_dir.join(base), size, limit, cancel)?;
            let rem = padded - size;
            if rem > 0 {
                io::copy(&mut (&mut gz).take(rem), &mut io::sink())?;
            }
        } else if padded > 0 {
            io::copy(&mut (&mut gz).take(padded), &mut io::sink())?;
        }
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_dir_points_to_installed_subdir_under_runtime_root() {
        if current_runtime_distribution().is_some() {
            let root = runtime_root().unwrap();
            let dir = runtime_dir().unwrap();
            assert_eq!(dir, root.join("installed"));
            assert_ne!(dir, models_dir().unwrap());
        }
    }
}

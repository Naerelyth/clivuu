//! Optional Tesseract 5 C API backend with TSV word-box layout and CJK space normalization.
//! ABI: https://github.com/tesseract-ocr/tesseract/blob/main/include/tesseract/capi.h
use crate::{
    Result,
    i18n::{t, tf},
    image::RgbaImage,
};
use std::ffi::{CStr, CString, c_char, c_int, c_void};

use super::{
    layout::{advance, is_cjk_no_space, modelled_boundaries},
    model::{OcrDocument, OcrEngineUsed, OcrLine, RectF},
};

#[link(name = "dl")]
unsafe extern "C" {
    fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> c_int;
}

struct Library(*mut c_void);

impl Drop for Library {
    fn drop(&mut self) {
        // SAFETY: opened via `dlopen`, uniquely owned here.
        unsafe {
            dlclose(self.0);
        }
    }
}

struct Api {
    _library: Library,
    handle: *mut c_void,
    delete: unsafe extern "C" fn(*mut c_void),
}

impl Drop for Api {
    fn drop(&mut self) {
        // SAFETY: created by `TessBaseAPICreate` and dropped before `_library`.
        unsafe {
            (self.delete)(self.handle);
        }
    }
}

fn open_library() -> Option<Library> {
    if std::env::var_os("CLIVUU_DISABLE_TESSERACT").is_some() {
        return None;
    }
    for name in [
        c"libtesseract.so.5",
        c"libtesseract.so.5.5",
        c"libtesseract.so",
    ] {
        // SAFETY: NUL-terminated library names; RTLD_NOW is 2 on Linux.
        let handle = unsafe { dlopen(name.as_ptr(), 2) };
        if !handle.is_null() {
            return Some(Library(handle));
        }
    }
    None
}

pub fn is_available() -> bool {
    open_library().is_some()
}

/// Curated Tesseract ISO 639-2 language codes and their localization message keys.
pub const KNOWN_LANGUAGES: &[(&str, &str)] = &[
    ("eng", "English"),
    ("chi_sim", "Simplified Chinese"),
    ("chi_tra", "Traditional Chinese"),
    ("jpn", "Japanese"),
    ("kor", "Korean"),
    ("fra", "French"),
    ("deu", "German"),
    ("spa", "Spanish"),
    ("ita", "Italian"),
    ("por", "Portuguese"),
    ("rus", "Russian"),
    ("ukr", "Ukrainian"),
    ("ara", "Arabic"),
    ("ell", "Greek"),
    ("tha", "Thai"),
    ("vie", "Vietnamese"),
];

const IGNORED_TESSDATA_STEMS: &[&str] = &["osd", "equ", "snum", "pdf"];

pub fn language_display_name(code: &str) -> String {
    KNOWN_LANGUAGES
        .iter()
        .find(|&&(c, _)| c == code)
        .map(|&(_, name_key)| t(name_key))
        .unwrap_or_else(|| code.to_owned())
}

pub fn parse_language_codes(spec: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in spec.split('+') {
        let code = raw.trim();
        if !code.is_empty()
            && code
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "_-".contains(c))
            && !out.iter().any(|existing| existing == code)
        {
            out.push(code.to_owned());
        }
    }
    out
}

/// Scan `$TESSDATA_PREFIX` and standard Linux `tessdata` directories for installed `*.traineddata`
/// language packs without initializing a Tesseract recognition session.
pub fn installed_languages() -> Vec<String> {
    let mut dirs = Vec::new();
    if let Some(prefix) = std::env::var_os("TESSDATA_PREFIX") {
        let p = std::path::PathBuf::from(prefix);
        dirs.push(p.clone());
        dirs.push(p.join("tessdata"));
    }
    for path in [
        "/usr/share/tesseract/tessdata",
        "/usr/share/tessdata",
        "/usr/share/tesseract-ocr/5/tessdata",
        "/usr/share/tesseract-ocr/4.00/tessdata",
        "/usr/local/share/tessdata",
    ] {
        dirs.push(std::path::PathBuf::from(path));
    }

    let mut found = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("traineddata") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if stem.is_empty()
                || IGNORED_TESSDATA_STEMS.contains(&stem)
                || !stem
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "_-".contains(c))
            {
                continue;
            }
            if !found.iter().any(|existing| existing == stem) {
                found.push(stem.to_owned());
            }
        }
    }

    found.sort_by(|a, b| {
        let rank_a = KNOWN_LANGUAGES
            .iter()
            .position(|&(c, _)| c == a)
            .unwrap_or(usize::MAX);
        let rank_b = KNOWN_LANGUAGES
            .iter()
            .position(|&(c, _)| c == b)
            .unwrap_or(usize::MAX);
        rank_a.cmp(&rank_b).then_with(|| a.cmp(b))
    });
    found
}

/// Suggest a copyable package-manager command for installing a missing Tesseract `traineddata` pack.
pub fn distro_install_hint(code: &str) -> String {
    let os_release = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    let lower = os_release.to_ascii_lowercase();
    if lower.contains("fedora") || lower.contains("rhel") || lower.contains("centos") {
        format!("sudo dnf install tesseract-langpack-{code}")
    } else if lower.contains("debian")
        || lower.contains("ubuntu")
        || lower.contains("linuxmint")
        || lower.contains("pop")
    {
        format!("sudo apt install tesseract-ocr-{}", code.replace('_', "-"))
    } else if lower.contains("arch") || lower.contains("manjaro") || lower.contains("endeavouros") {
        format!("sudo pacman -S tesseract-data-{code}")
    } else if lower.contains("opensuse") || lower.contains("suse") {
        format!("sudo zypper install tesseract-ocr-traineddata-{code}")
    } else {
        format!("tesseract-langpack-{code} ({code}.traineddata)")
    }
}

pub fn validate_languages(languages: &str) -> Result<CString> {
    if languages.is_empty()
        || languages.len() > 128
        || !languages
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "+_-".contains(c))
    {
        return Err(t("Invalid OCR language format; use a value such as eng+chi_sim").into());
    }
    Ok(CString::new(languages)?)
}

pub fn recognize_document(image: &RgbaImage, languages: &str) -> Result<OcrDocument> {
    if image.width() == 0
        || image.height() == 0
        || u64::from(image.width()) * u64::from(image.height()) > crate::image_ops::MAX_PIXELS
    {
        return Err(t("The image exceeds 48 megapixels or has invalid dimensions").into());
    }
    let language = validate_languages(languages)?;
    let library = open_library()
        .ok_or_else(|| t("Install system libtesseract 5 and eng/chi_sim language data"))?;

    macro_rules! symbol {
        ($name:literal, $ty:ty) => {{
            let name = CString::new($name)?;
            // SAFETY: valid library handle and NUL-terminated symbol name.
            let pointer = unsafe { dlsym(library.0, name.as_ptr()) };
            if pointer.is_null() {
                return Err(tf("Tesseract C ABI missing: {0}", &[$name]).into());
            }
            // SAFETY: concrete types match `tesseract/capi.h`.
            unsafe { std::mem::transmute::<*mut c_void, $ty>(pointer) }
        }};
    }

    let create = symbol!("TessBaseAPICreate", unsafe extern "C" fn() -> *mut c_void);
    let delete = symbol!("TessBaseAPIDelete", unsafe extern "C" fn(*mut c_void));
    let init = symbol!(
        "TessBaseAPIInit3",
        unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int
    );
    let loaded = symbol!(
        "TessBaseAPIGetLoadedLanguagesAsVector",
        unsafe extern "C" fn(*mut c_void) -> *mut *mut c_char
    );
    let delete_array = symbol!(
        "TessDeleteTextArray",
        unsafe extern "C" fn(*mut *mut c_char)
    );
    let set_image = symbol!(
        "TessBaseAPISetImage",
        unsafe extern "C" fn(*mut c_void, *const u8, c_int, c_int, c_int, c_int)
    );
    let get_utf8 = symbol!(
        "TessBaseAPIGetUTF8Text",
        unsafe extern "C" fn(*mut c_void) -> *mut c_char
    );
    let delete_text = symbol!("TessDeleteText", unsafe extern "C" fn(*const c_char));
    let get_tsv: Option<unsafe extern "C" fn(*mut c_void, c_int) -> *mut c_char> = unsafe {
        let ptr = dlsym(library.0, c"TessBaseAPIGetTsvText".as_ptr());
        if ptr.is_null() {
            None
        } else {
            Some(std::mem::transmute::<
                *mut c_void,
                unsafe extern "C" fn(*mut c_void, c_int) -> *mut c_char,
            >(ptr))
        }
    };

    // White-matte transparent RGBA pixels so dark glyphs on alpha=0 stay legible.
    let matted = super::paddle::rgba_to_white_matted_rgb(image);

    // SAFETY: documented Tesseract C API calls with bounded image dimensions.
    unsafe {
        let handle = create();
        if handle.is_null() {
            return Err(t("Cannot create the OCR engine").into());
        }
        let api = Api {
            _library: library,
            handle,
            delete,
        };
        if init(api.handle, std::ptr::null(), language.as_ptr()) != 0 {
            return Err(tf(
                "Cannot load OCR language {0}; install the corresponding traineddata",
                &[languages],
            )
            .into());
        }
        let vector = loaded(api.handle);
        let mut available = Vec::new();
        if !vector.is_null() {
            let mut item = vector;
            while !(*item).is_null() {
                available.push(CStr::from_ptr(*item).to_string_lossy().into_owned());
                item = item.add(1);
            }
            delete_array(vector);
        }
        for requested in languages.split('+') {
            if !available.iter().any(|s| s == requested) {
                return Err(tf(
                    "OCR language {0} is not installed; install its traineddata or change the language setting",
                    &[requested],
                )
                .into());
            }
        }
        set_image(
            api.handle,
            matted.as_ptr(),
            matted.width() as c_int,
            matted.height() as c_int,
            3,
            (matted.width() * 3) as c_int,
        );

        if let Some(tsv_fn) = get_tsv {
            let raw_tsv = tsv_fn(api.handle, 0);
            if !raw_tsv.is_null() {
                let tsv = CStr::from_ptr(raw_tsv).to_string_lossy().into_owned();
                delete_text(raw_tsv);
                let lines = parse_tsv_lines(&tsv, image.width() as f32, image.height() as f32);
                return Ok(OcrDocument::from_lines(
                    lines,
                    image.width(),
                    image.height(),
                    OcrEngineUsed::Tesseract,
                ));
            }
        }

        let raw_text = get_utf8(api.handle);
        if raw_text.is_null() {
            return Err(t("OCR recognition failed").into());
        }
        let utf8 = CStr::from_ptr(raw_text).to_string_lossy().into_owned();
        delete_text(raw_text);
        let lines = fallback_lines_from_text(&utf8, image.width(), image.height());
        Ok(OcrDocument::from_lines(
            lines,
            image.width(),
            image.height(),
            OcrEngineUsed::Tesseract,
        ))
    }
}

#[derive(Clone, Debug)]
struct TsvWord {
    key: (i32, i32, i32, i32),
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
    text: String,
}

/// Parse `TessBaseAPIGetTsvText` word entries into `OcrLine`s with character-level `char_x`
/// and automatic CJK inter-character space removal.
pub fn parse_tsv_lines(tsv: &str, max_w: f32, max_h: f32) -> Vec<OcrLine> {
    let mut words: Vec<TsvWord> = Vec::new();
    for row in tsv.lines() {
        let mut cols = row.splitn(12, '\t');
        let Some(level) = cols.next().and_then(|s| s.parse::<i32>().ok()) else {
            continue;
        };
        if level != 5 {
            continue;
        }
        let page = cols.next().and_then(|s| s.parse::<i32>().ok()).unwrap_or(0);
        let block = cols.next().and_then(|s| s.parse::<i32>().ok()).unwrap_or(0);
        let par = cols.next().and_then(|s| s.parse::<i32>().ok()).unwrap_or(0);
        let line = cols.next().and_then(|s| s.parse::<i32>().ok()).unwrap_or(0);
        let _word_num = cols.next();
        let Some(left) = cols.next().and_then(|s| s.parse::<f32>().ok()) else {
            continue;
        };
        let Some(top) = cols.next().and_then(|s| s.parse::<f32>().ok()) else {
            continue;
        };
        let Some(width) = cols.next().and_then(|s| s.parse::<f32>().ok()) else {
            continue;
        };
        let Some(height) = cols.next().and_then(|s| s.parse::<f32>().ok()) else {
            continue;
        };
        let conf = cols
            .next()
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(-1.0);
        let text = cols.next().unwrap_or("").trim();
        if conf < 0.0 || text.is_empty() || width <= 0.0 || height <= 0.0 {
            continue;
        }
        let right = (left + width).min(max_w);
        let bottom = (top + height).min(max_h);
        let left = left.clamp(0.0, right);
        let top = top.clamp(0.0, bottom);
        if right <= left || bottom <= top {
            continue;
        }
        words.push(TsvWord {
            key: (page, block, par, line),
            left,
            top,
            right,
            bottom,
            text: text.to_owned(),
        });
    }

    let mut lines = Vec::new();
    let mut i = 0usize;
    while i < words.len() {
        let key = words[i].key;
        let mut j = i + 1;
        while j < words.len() && words[j].key == key {
            j += 1;
        }
        if let Some(ocr_line) = build_line_from_words(&words[i..j]) {
            lines.push(ocr_line);
        }
        i = j;
    }
    lines
}

fn build_line_from_words(group: &[TsvWord]) -> Option<OcrLine> {
    if group.is_empty() {
        return None;
    }
    let mut min_l = f32::MAX;
    let mut min_t = f32::MAX;
    let mut max_r = f32::MIN;
    let mut max_b = f32::MIN;
    let mut text = String::new();
    let mut char_x = Vec::new();

    for (idx, w) in group.iter().enumerate() {
        min_l = min_l.min(w.left);
        min_t = min_t.min(w.top);
        max_r = max_r.max(w.right);
        max_b = max_b.max(w.bottom);

        if idx > 0 {
            let prev_last = text.chars().next_back();
            let next_first = w.text.chars().next();
            let omit_space = match (prev_last, next_first) {
                (Some(a), Some(b)) => is_cjk_no_space(a) && is_cjk_no_space(b),
                _ => false,
            };
            if !omit_space {
                text.push(' ');
                if let Some(last_x) = char_x.last().copied() {
                    char_x.push(w.left.max(last_x));
                }
            }
        }

        let widths: Vec<f32> = w.text.chars().map(advance).collect();
        let word_xs = modelled_boundaries(w.left, w.right.max(w.left), &widths);
        if char_x.is_empty() {
            char_x.extend_from_slice(&word_xs);
        } else {
            let prev = *char_x.last().unwrap();
            for &x in &word_xs[1..] {
                char_x.push(x.max(prev));
            }
        }
        text.push_str(&w.text);
    }

    let bounds = RectF::from_ltrb(min_l, min_t, max_r, max_b)?;
    if char_x.len() != text.chars().count() + 1 {
        let widths: Vec<f32> = text.chars().map(advance).collect();
        char_x = modelled_boundaries(bounds.left(), bounds.right(), &widths);
    } else {
        char_x[0] = bounds.left();
        if let Some(last) = char_x.last_mut() {
            *last = bounds.right();
        }
        for k in 0..char_x.len().saturating_sub(1) {
            char_x[k + 1] = char_x[k + 1].max(char_x[k]);
        }
    }
    Some(OcrLine {
        text,
        bounds,
        char_x,
    })
}

fn fallback_lines_from_text(text: &str, width: u32, height: u32) -> Vec<OcrLine> {
    let raw_lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if raw_lines.is_empty() {
        return Vec::new();
    }
    let step_h = (height as f32 / raw_lines.len() as f32).max(1.0);
    raw_lines
        .into_iter()
        .enumerate()
        .filter_map(|(i, raw)| {
            let cleaned = strip_cjk_spaces(raw);
            let bounds = RectF::from_xywh(
                0.0,
                i as f32 * step_h,
                width as f32,
                step_h.min(height as f32),
            )?;
            let widths: Vec<f32> = cleaned.chars().map(advance).collect();
            let char_x = modelled_boundaries(bounds.left(), bounds.right(), &widths);
            Some(OcrLine {
                text: cleaned,
                bounds,
                char_x,
            })
        })
        .collect()
}

pub fn strip_cjk_spaces(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    for (i, &c) in chars.iter().enumerate() {
        if c == ' '
            && i > 0
            && i + 1 < chars.len()
            && is_cjk_no_space(chars[i - 1])
            && is_cjk_no_space(chars[i + 1])
        {
            continue;
        }
        out.push(c);
    }
    out
}

//! Minimal `dlopen` binding to the stable ONNX Runtime C API (`OrtGetApiBase`).
//! Keeps `clivuu-core` free of heavy ML build dependencies while disabling ORT's CPU arena.
use crate::{
    Result,
    i18n::{t, tf},
};
use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    fs,
    path::Path,
};

#[link(name = "dl")]
unsafe extern "C" {
    fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> c_int;
    fn dlerror() -> *const c_char;
}

const RTLD_NOW: c_int = 2;
const ORT_API_VERSION: u32 = 17;
const ORT_LOGGING_LEVEL_ERROR: u32 = 3;
const ORT_ENABLE_ALL: u32 = 99;
const ONNX_TENSOR_ELEMENT_DATA_TYPE_FLOAT: u32 = 1;

#[repr(C)]
struct OrtApiBase {
    get_api: unsafe extern "C" fn(version: u32) -> *const *const c_void,
    get_version_string: unsafe extern "C" fn() -> *const c_char,
}

// Verified 64-bit pointer slots (`byte_offset / 8`) in `struct OrtApi` (append-only ABI).
const SLOT_GET_ERROR_MESSAGE: usize = 0x10 / 8; // 2
const SLOT_CREATE_ENV: usize = 0x18 / 8; // 3
const SLOT_CREATE_SESSION_FROM_ARRAY: usize = 0x40 / 8; // 8
const SLOT_RUN: usize = 0x48 / 8; // 9
const SLOT_CREATE_SESSION_OPTIONS: usize = 0x50 / 8; // 10
const SLOT_DISABLE_MEM_PATTERN: usize = 0x88 / 8; // 17
const SLOT_DISABLE_CPU_MEM_ARENA: usize = 0x98 / 8; // 19
const SLOT_SET_SESSION_LOG_SEVERITY_LEVEL: usize = 0xb0 / 8; // 22
const SLOT_SET_SESSION_GRAPH_OPT_LEVEL: usize = 0xb8 / 8; // 23
const SLOT_SET_INTRA_OP_NUM_THREADS: usize = 0xc0 / 8; // 24
const SLOT_SET_INTER_OP_NUM_THREADS: usize = 0xc8 / 8; // 25
const SLOT_CREATE_TENSOR_WITH_DATA: usize = 0x188 / 8; // 49
const SLOT_IS_TENSOR: usize = 0x190 / 8; // 50
const SLOT_GET_TENSOR_MUTABLE_DATA: usize = 0x198 / 8; // 51
const SLOT_GET_TENSOR_ELEMENT_TYPE: usize = 0x1e0 / 8; // 60
const SLOT_GET_DIMENSIONS_COUNT: usize = 0x1e8 / 8; // 61
const SLOT_GET_DIMENSIONS: usize = 0x1f0 / 8; // 62
const SLOT_GET_TENSOR_SHAPE_ELEMENT_COUNT: usize = 0x200 / 8; // 64
const SLOT_GET_TENSOR_TYPE_AND_SHAPE: usize = 0x208 / 8; // 65
const SLOT_CREATE_CPU_MEMORY_INFO: usize = 0x228 / 8; // 69
const SLOT_RELEASE_ENV: usize = 0x2e0 / 8; // 92
const SLOT_RELEASE_STATUS: usize = 0x2e8 / 8; // 93
const SLOT_RELEASE_MEMORY_INFO: usize = 0x2f0 / 8; // 94
const SLOT_RELEASE_SESSION: usize = 0x2f8 / 8; // 95
const SLOT_RELEASE_VALUE: usize = 0x300 / 8; // 96
const SLOT_RELEASE_TENSOR_TYPE_AND_SHAPE_INFO: usize = 0x318 / 8; // 99
const SLOT_RELEASE_SESSION_OPTIONS: usize = 0x320 / 8; // 100
const SLOT_ADD_SESSION_CONFIG_ENTRY: usize = 0x410 / 8; // 130

type FnGetErrorMessage = unsafe extern "C" fn(*const c_void) -> *const c_char;
type FnRelease = unsafe extern "C" fn(*mut c_void);
type FnCreateEnv = unsafe extern "C" fn(u32, *const c_char, *mut *mut c_void) -> *mut c_void;
type FnCreateSessionOptions = unsafe extern "C" fn(*mut *mut c_void) -> *mut c_void;
type FnOptFlag = unsafe extern "C" fn(*mut c_void) -> *mut c_void;
type FnOptI32 = unsafe extern "C" fn(*mut c_void, c_int) -> *mut c_void;
type FnOptU32 = unsafe extern "C" fn(*mut c_void, u32) -> *mut c_void;
type FnAddConfigEntry =
    unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> *mut c_void;
type FnCreateSessionFromArray = unsafe extern "C" fn(
    *const c_void,
    *const c_void,
    usize,
    *const c_void,
    *mut *mut c_void,
) -> *mut c_void;
type FnCreateCpuMemoryInfo = unsafe extern "C" fn(c_int, c_int, *mut *mut c_void) -> *mut c_void;
type FnCreateTensorWithData = unsafe extern "C" fn(
    *const c_void,
    *mut c_void,
    usize,
    *const i64,
    usize,
    u32,
    *mut *mut c_void,
) -> *mut c_void;
type FnRun = unsafe extern "C" fn(
    *mut c_void,
    *const c_void,
    *const *const c_char,
    *const *const c_void,
    usize,
    *const *const c_char,
    usize,
    *mut *mut c_void,
) -> *mut c_void;
type FnIsTensor = unsafe extern "C" fn(*const c_void, *mut c_int) -> *mut c_void;
type FnGetTensorTypeAndShape = unsafe extern "C" fn(*const c_void, *mut *mut c_void) -> *mut c_void;
type FnGetTensorElementType = unsafe extern "C" fn(*const c_void, *mut u32) -> *mut c_void;
type FnGetDimensionsCount = unsafe extern "C" fn(*const c_void, *mut usize) -> *mut c_void;
type FnGetDimensions = unsafe extern "C" fn(*const c_void, *mut i64, usize) -> *mut c_void;
type FnGetElementCount = unsafe extern "C" fn(*const c_void, *mut usize) -> *mut c_void;
type FnGetTensorMutableData = unsafe extern "C" fn(*mut c_void, *mut *mut c_void) -> *mut c_void;

struct DynLibrary(*mut c_void);

impl Drop for DynLibrary {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: handle returned by `dlopen` and dropped after all ORT objects.
            unsafe {
                dlclose(self.0);
            }
        }
    }
}

#[inline]
unsafe fn slot<T: Copy>(api: *const *const c_void, index: usize) -> Result<T> {
    // SAFETY: `api` is a valid `OrtApi` table for `ORT_API_VERSION >= 17` (>= 170 entries).
    let ptr = unsafe { *api.add(index) };
    if ptr.is_null() {
        return Err(t("Incompatible ONNX Runtime C API version").into());
    }
    // SAFETY: caller supplies the exact C function signature for `index`.
    Ok(unsafe { std::mem::transmute_copy::<*const c_void, T>(&ptr) })
}

pub struct OrtRuntime {
    api: *const *const c_void,
    env: *mut c_void,
    mem_info: *mut c_void,
    release_env: FnRelease,
    release_mem_info: FnRelease,
    release_status: FnRelease,
    get_error_message: FnGetErrorMessage,
    _shared_provider_lib: Option<DynLibrary>,
    _library: DynLibrary,
}

impl Drop for OrtRuntime {
    fn drop(&mut self) {
        // SAFETY: reverse creation order before `dlclose` runs on `_library`.
        unsafe {
            if !self.mem_info.is_null() {
                (self.release_mem_info)(self.mem_info);
            }
            if !self.env.is_null() {
                (self.release_env)(self.env);
            }
        }
    }
}

impl OrtRuntime {
    pub fn load(path: &Path) -> Result<Self> {
        let mut shared_provider = None;
        if let Some(parent) = path.parent() {
            let sibling = parent.join("libonnxruntime_providers_shared.so");
            if sibling.is_file()
                && let Ok(c_sib) = CString::new(sibling.to_string_lossy().as_bytes())
            {
                // SAFETY: optional companion provider library in the same verified directory.
                let h = unsafe { dlopen(c_sib.as_ptr(), RTLD_NOW) };
                if !h.is_null() {
                    shared_provider = Some(DynLibrary(h));
                }
            }
        }

        let c_path = CString::new(path.to_string_lossy().as_bytes())?;
        // SAFETY: NUL-terminated library path; clears prior `dlerror`.
        let handle = unsafe {
            let _ = dlerror();
            dlopen(c_path.as_ptr(), RTLD_NOW)
        };
        if handle.is_null() {
            let detail = unsafe {
                let err = dlerror();
                if err.is_null() {
                    String::from("dlopen failed")
                } else {
                    CStr::from_ptr(err).to_string_lossy().into_owned()
                }
            };
            return Err(tf("Cannot load ONNX Runtime: {0}", &[&detail]).into());
        }
        let library = DynLibrary(handle);

        // SAFETY: `OrtGetApiBase` is the sole public entry point of ONNX Runtime.
        let get_base: unsafe extern "C" fn() -> *const OrtApiBase = unsafe {
            let sym = dlsym(library.0, c"OrtGetApiBase".as_ptr());
            if sym.is_null() {
                return Err(t("ONNX Runtime symbol OrtGetApiBase is missing").into());
            }
            std::mem::transmute::<*mut c_void, unsafe extern "C" fn() -> *const OrtApiBase>(sym)
        };

        // SAFETY: calls `OrtGetApiBase()->GetApi(17)`.
        let api = unsafe {
            let base = get_base();
            if base.is_null() {
                return Err(t("ONNX Runtime returned a null API base").into());
            }
            ((*base).get_api)(ORT_API_VERSION)
        };
        if api.is_null() {
            return Err(t("ONNX Runtime version is older than API v17").into());
        }

        // SAFETY: slots match `struct OrtApi` v17+.
        let get_error_message: FnGetErrorMessage = unsafe { slot(api, SLOT_GET_ERROR_MESSAGE)? };
        let release_status: FnRelease = unsafe { slot(api, SLOT_RELEASE_STATUS)? };
        let release_env: FnRelease = unsafe { slot(api, SLOT_RELEASE_ENV)? };
        let release_mem_info: FnRelease = unsafe { slot(api, SLOT_RELEASE_MEMORY_INFO)? };
        let create_env: FnCreateEnv = unsafe { slot(api, SLOT_CREATE_ENV)? };
        let create_mem_info: FnCreateCpuMemoryInfo =
            unsafe { slot(api, SLOT_CREATE_CPU_MEMORY_INFO)? };

        let mut env = std::ptr::null_mut();
        // SAFETY: valid output pointer and static C string log ID.
        let status =
            unsafe { create_env(ORT_LOGGING_LEVEL_ERROR, c"clivuu-ocr".as_ptr(), &mut env) };
        check_status(status, get_error_message, release_status)?;

        let mut mem_info = std::ptr::null_mut();
        // `1` = `OrtDeviceAllocator` (direct malloc/free without arena), `0` = `OrtMemTypeDefault`.
        let status = unsafe { create_mem_info(1, 0, &mut mem_info) };
        if let Err(err) = check_status(status, get_error_message, release_status) {
            unsafe {
                if !env.is_null() {
                    release_env(env);
                }
            }
            return Err(err);
        }

        Ok(Self {
            api,
            env,
            mem_info,
            release_env,
            release_mem_info,
            release_status,
            get_error_message,
            _shared_provider_lib: shared_provider,
            _library: library,
        })
    }

    fn check(&self, status: *mut c_void) -> Result<()> {
        check_status(status, self.get_error_message, self.release_status)
    }
}

fn check_status(
    status: *mut c_void,
    get_error_message: FnGetErrorMessage,
    release_status: FnRelease,
) -> Result<()> {
    if status.is_null() {
        return Ok(());
    }
    // SAFETY: non-null `OrtStatusPtr` owned by caller until `ReleaseStatus`.
    let message = unsafe {
        let ptr = get_error_message(status);
        let msg = if ptr.is_null() {
            String::from("ONNX Runtime error")
        } else {
            CStr::from_ptr(ptr).to_string_lossy().into_owned()
        };
        release_status(status);
        msg
    };
    Err(message.into())
}

pub struct OrtModelSession<'a> {
    rt: &'a OrtRuntime,
    session: *mut c_void,
    release_session: FnRelease,
    input_name: CString,
    output_name: CString,
}

impl Drop for OrtModelSession<'_> {
    fn drop(&mut self) {
        if !self.session.is_null() {
            // SAFETY: session created from `self.rt.env`, dropped before `self.rt`.
            unsafe {
                (self.release_session)(self.session);
            }
        }
    }
}

impl<'a> OrtModelSession<'a> {
    pub fn from_file(rt: &'a OrtRuntime, model_path: &Path, intra_threads: usize) -> Result<Self> {
        let model_bytes = fs::read(model_path)?;
        let (input_name, output_name) = parse_onnx_io_names(&model_bytes).unwrap_or_else(|| {
            (
                CString::new("x").unwrap(),
                CString::new("fetch_name_0").unwrap(),
            )
        });

        // SAFETY: slots verified in `struct OrtApi`.
        let create_opts: FnCreateSessionOptions =
            unsafe { slot(rt.api, SLOT_CREATE_SESSION_OPTIONS)? };
        let release_opts: FnRelease = unsafe { slot(rt.api, SLOT_RELEASE_SESSION_OPTIONS)? };
        let disable_mem_pattern: FnOptFlag = unsafe { slot(rt.api, SLOT_DISABLE_MEM_PATTERN)? };
        let disable_cpu_arena: FnOptFlag = unsafe { slot(rt.api, SLOT_DISABLE_CPU_MEM_ARENA)? };
        let set_log_level: FnOptI32 = unsafe { slot(rt.api, SLOT_SET_SESSION_LOG_SEVERITY_LEVEL)? };
        let set_opt_level: FnOptU32 = unsafe { slot(rt.api, SLOT_SET_SESSION_GRAPH_OPT_LEVEL)? };
        let set_intra: FnOptI32 = unsafe { slot(rt.api, SLOT_SET_INTRA_OP_NUM_THREADS)? };
        let set_inter: FnOptI32 = unsafe { slot(rt.api, SLOT_SET_INTER_OP_NUM_THREADS)? };
        let add_cfg: FnAddConfigEntry = unsafe { slot(rt.api, SLOT_ADD_SESSION_CONFIG_ENTRY)? };
        let create_session: FnCreateSessionFromArray =
            unsafe { slot(rt.api, SLOT_CREATE_SESSION_FROM_ARRAY)? };
        let release_session: FnRelease = unsafe { slot(rt.api, SLOT_RELEASE_SESSION)? };

        let mut opts = std::ptr::null_mut();
        rt.check(unsafe { create_opts(&mut opts) })?;
        struct OptGuard(*mut c_void, FnRelease);
        impl Drop for OptGuard {
            fn drop(&mut self) {
                if !self.0.is_null() {
                    unsafe { (self.1)(self.0) };
                }
            }
        }
        let _opts_guard = OptGuard(opts, release_opts);

        rt.check(unsafe { set_log_level(opts, ORT_LOGGING_LEVEL_ERROR as c_int) })?;
        rt.check(unsafe { set_opt_level(opts, ORT_ENABLE_ALL) })?;
        rt.check(unsafe { disable_mem_pattern(opts) })?;
        rt.check(unsafe { disable_cpu_arena(opts) })?;
        rt.check(unsafe { set_intra(opts, intra_threads.clamp(1, 16) as c_int) })?;
        rt.check(unsafe { set_inter(opts, 1) })?;
        let _ = rt.check(unsafe {
            add_cfg(
                opts,
                c"session.intra_op.allow_spinning".as_ptr(),
                c"0".as_ptr(),
            )
        });

        let mut session = std::ptr::null_mut();
        rt.check(unsafe {
            create_session(
                rt.env,
                model_bytes.as_ptr().cast::<c_void>(),
                model_bytes.len(),
                opts,
                &mut session,
            )
        })?;

        Ok(Self {
            rt,
            session,
            release_session,
            input_name,
            output_name,
        })
    }

    pub fn run_f32(&self, shape: &[i64], input: &[f32]) -> Result<(Vec<i64>, Vec<f32>)> {
        let expected_elems: usize = shape
            .iter()
            .try_fold(1usize, |acc, &d| usize::try_from(d).ok()?.checked_mul(acc))
            .ok_or_else(|| t("Invalid ONNX input tensor dimensions"))?;
        if expected_elems != input.len() {
            return Err(t("ONNX input buffer size mismatch").into());
        }

        // SAFETY: slots verified in `struct OrtApi`.
        let create_tensor: FnCreateTensorWithData =
            unsafe { slot(self.rt.api, SLOT_CREATE_TENSOR_WITH_DATA)? };
        let release_value: FnRelease = unsafe { slot(self.rt.api, SLOT_RELEASE_VALUE)? };
        let run_fn: FnRun = unsafe { slot(self.rt.api, SLOT_RUN)? };
        let is_tensor: FnIsTensor = unsafe { slot(self.rt.api, SLOT_IS_TENSOR)? };
        let get_type_shape: FnGetTensorTypeAndShape =
            unsafe { slot(self.rt.api, SLOT_GET_TENSOR_TYPE_AND_SHAPE)? };
        let release_type_shape: FnRelease =
            unsafe { slot(self.rt.api, SLOT_RELEASE_TENSOR_TYPE_AND_SHAPE_INFO)? };
        let get_elem_type: FnGetTensorElementType =
            unsafe { slot(self.rt.api, SLOT_GET_TENSOR_ELEMENT_TYPE)? };
        let get_dims_count: FnGetDimensionsCount =
            unsafe { slot(self.rt.api, SLOT_GET_DIMENSIONS_COUNT)? };
        let get_dims: FnGetDimensions = unsafe { slot(self.rt.api, SLOT_GET_DIMENSIONS)? };
        let get_elem_count: FnGetElementCount =
            unsafe { slot(self.rt.api, SLOT_GET_TENSOR_SHAPE_ELEMENT_COUNT)? };
        let get_mutable_data: FnGetTensorMutableData =
            unsafe { slot(self.rt.api, SLOT_GET_TENSOR_MUTABLE_DATA)? };

        struct ValueGuard(*mut c_void, FnRelease);
        impl Drop for ValueGuard {
            fn drop(&mut self) {
                if !self.0.is_null() {
                    unsafe { (self.1)(self.0) };
                }
            }
        }

        let mut input_val = std::ptr::null_mut();
        self.rt.check(unsafe {
            create_tensor(
                self.rt.mem_info,
                input.as_ptr().cast_mut().cast::<c_void>(),
                std::mem::size_of_val(input),
                shape.as_ptr(),
                shape.len(),
                ONNX_TENSOR_ELEMENT_DATA_TYPE_FLOAT,
                &mut input_val,
            )
        })?;
        let _in_guard = ValueGuard(input_val, release_value);

        let in_names = [self.input_name.as_ptr()];
        let in_vals = [input_val.cast_const()];
        let out_names = [self.output_name.as_ptr()];
        let mut out_val: *mut c_void = std::ptr::null_mut();

        self.rt.check(unsafe {
            run_fn(
                self.session,
                std::ptr::null(),
                in_names.as_ptr(),
                in_vals.as_ptr(),
                1,
                out_names.as_ptr(),
                1,
                &mut out_val,
            )
        })?;
        let _out_guard = ValueGuard(out_val, release_value);

        let mut flag: c_int = 0;
        self.rt.check(unsafe { is_tensor(out_val, &mut flag) })?;
        if flag == 0 {
            return Err(t("ONNX model output is not a tensor").into());
        }

        let mut shape_info: *mut c_void = std::ptr::null_mut();
        self.rt
            .check(unsafe { get_type_shape(out_val, &mut shape_info) })?;
        struct ShapeGuard(*mut c_void, FnRelease);
        impl Drop for ShapeGuard {
            fn drop(&mut self) {
                if !self.0.is_null() {
                    unsafe { (self.1)(self.0) };
                }
            }
        }
        let _shape_guard = ShapeGuard(shape_info, release_type_shape);

        let mut elem_type: u32 = 0;
        self.rt
            .check(unsafe { get_elem_type(shape_info, &mut elem_type) })?;
        if elem_type != ONNX_TENSOR_ELEMENT_DATA_TYPE_FLOAT {
            return Err(t("ONNX model output tensor is not float32").into());
        }

        let mut dim_count: usize = 0;
        self.rt
            .check(unsafe { get_dims_count(shape_info, &mut dim_count) })?;
        let mut out_dims = vec![0i64; dim_count];
        if dim_count > 0 {
            self.rt
                .check(unsafe { get_dims(shape_info, out_dims.as_mut_ptr(), dim_count) })?;
        }
        let mut elem_count: usize = 0;
        self.rt
            .check(unsafe { get_elem_count(shape_info, &mut elem_count) })?;

        let mut raw_ptr: *mut c_void = std::ptr::null_mut();
        self.rt
            .check(unsafe { get_mutable_data(out_val, &mut raw_ptr) })?;
        if raw_ptr.is_null() && elem_count > 0 {
            return Err(t("ONNX model returned an empty tensor buffer").into());
        }
        let slice = if elem_count == 0 {
            &[]
        } else {
            // SAFETY: `raw_ptr` points to `elem_count` contiguous `f32` values owned by `out_val`.
            unsafe { std::slice::from_raw_parts(raw_ptr.cast::<f32>(), elem_count) }
        };
        Ok((out_dims, slice.to_vec()))
    }
}

/// Parse the primary runtime graph input and output tensor names directly from serialized ONNX
/// `ModelProto` bytes so we never rely on hard-coded output tensor names.
pub fn parse_onnx_io_names(bytes: &[u8]) -> Option<(CString, CString)> {
    let mut pos = 0usize;
    while pos < bytes.len() {
        let (tag, next) = read_varint(bytes, pos)?;
        pos = next;
        let field_num = tag >> 3;
        let wire_type = tag & 7;
        if field_num == 7 && wire_type == 2 {
            let (len, start) = read_varint(bytes, pos)?;
            let end = start.checked_add(len as usize)?;
            if end > bytes.len() {
                return None;
            }
            return parse_graph_proto(&bytes[start..end]);
        }
        pos = skip_field(bytes, pos, wire_type)?;
    }
    None
}

fn parse_graph_proto(graph: &[u8]) -> Option<(CString, CString)> {
    let mut initializers = Vec::new();
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    let mut pos = 0usize;
    while pos < graph.len() {
        let (tag, next) = read_varint(graph, pos)?;
        pos = next;
        let field_num = tag >> 3;
        let wire_type = tag & 7;
        if wire_type == 2 {
            let (len, start) = read_varint(graph, pos)?;
            let end = start.checked_add(len as usize)?;
            if end > graph.len() {
                return None;
            }
            let sub = &graph[start..end];
            match field_num {
                // `TensorProto` initializer: field 8 (`name: string`)
                5 => {
                    if let Some(name) = parse_string_field(sub, 8) {
                        initializers.push(name);
                    }
                }
                // `ValueInfoProto` input: field 1 (`name: string`)
                11 => {
                    if let Some(name) = parse_string_field(sub, 1) {
                        inputs.push(name);
                    }
                }
                // `ValueInfoProto` output: field 1 (`name: string`)
                12 => {
                    if let Some(name) = parse_string_field(sub, 1) {
                        outputs.push(name);
                    }
                }
                _ => {}
            }
            pos = end;
        } else {
            pos = skip_field(graph, pos, wire_type)?;
        }
    }
    let input = inputs
        .into_iter()
        .find(|n| !initializers.iter().any(|init| init == n))?;
    let output = outputs.into_iter().next()?;
    Some((CString::new(input).ok()?, CString::new(output).ok()?))
}

fn parse_string_field(msg: &[u8], target_field: u64) -> Option<String> {
    let mut pos = 0usize;
    while pos < msg.len() {
        let (tag, next) = read_varint(msg, pos)?;
        pos = next;
        let field_num = tag >> 3;
        let wire_type = tag & 7;
        if field_num == target_field && wire_type == 2 {
            let (len, start) = read_varint(msg, pos)?;
            let end = start.checked_add(len as usize)?;
            if end > msg.len() {
                return None;
            }
            return std::str::from_utf8(&msg[start..end])
                .ok()
                .map(str::to_owned);
        }
        pos = skip_field(msg, pos, wire_type)?;
    }
    None
}

fn read_varint(buf: &[u8], mut pos: usize) -> Option<(u64, usize)> {
    let mut value = 0u64;
    let mut shift = 0u32;
    loop {
        let &byte = buf.get(pos)?;
        pos += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some((value, pos));
        }
        shift += 7;
        if shift >= 64 {
            return None;
        }
    }
}

fn skip_field(buf: &[u8], pos: usize, wire_type: u64) -> Option<usize> {
    match wire_type {
        0 => read_varint(buf, pos).map(|(_, next)| next),
        1 => pos.checked_add(8).filter(|&end| end <= buf.len()),
        2 => {
            let (len, start) = read_varint(buf, pos)?;
            start
                .checked_add(len as usize)
                .filter(|&end| end <= buf.len())
        }
        5 => pos.checked_add(4).filter(|&end| end <= buf.len()),
        _ => None,
    }
}

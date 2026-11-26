//! Process-lived native API bindings. Each adapter loads only its own library.
use clivuu_core::i18n::t;
use std::ffi::{CStr, c_char, c_int, c_void};
#[link(name = "dl")]
unsafe extern "C" {
    fn dlopen(name: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> c_int;
}
pub(super) struct Library(*mut c_void);
impl Library {
    pub(super) fn open(name: &CStr) -> Result<Self, String> {
        // Layer-shell registers GTypes and process-long monitor callbacks even
        // when is_supported returns false. Never unload that code on session
        // teardown. Linux RTLD_NODELETE keeps it resident after dlclose.
        // SAFETY: static NUL-terminated name, RTLD_NOW | RTLD_NODELETE.
        let handle = unsafe { dlopen(name.as_ptr(), 2 | 0x1000) };
        if handle.is_null() {
            Err(t("The desktop service is unavailable"))
        } else {
            Ok(Self(handle))
        }
    }
    /// SAFETY: caller supplies the exact C ABI function signature for the symbol.
    pub(super) unsafe fn symbol<T: Copy>(&self, name: &CStr) -> Result<T, String> {
        let p = unsafe { dlsym(self.0, name.as_ptr()) };
        if p.is_null() || std::mem::size_of::<T>() != std::mem::size_of_val(&p) {
            return Err(t("The desktop service is unavailable"));
        }
        Ok(unsafe { std::mem::transmute_copy(&p) })
    }
}
impl Drop for Library {
    fn drop(&mut self) {
        unsafe {
            dlclose(self.0);
        }
    }
}

//! Optional XWayland surface setup. Xlib is loaded only by the platform worker.
//! Ordinary pins reject focus. Explicit capture and pin-editing overlays use
//! a temporary keyboard input mode for shortcuts and text entry.
use clivuu_core::i18n::{t, tf};
use std::ffi::{c_char, c_int, c_long, c_ulong, c_void};

#[link(name = "dl")]
unsafe extern "C" {
    fn dlopen(name: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> c_int;
}

#[repr(C)]
#[derive(Default)]
struct Attributes {
    background_pixmap: c_ulong,
    background_pixel: c_ulong,
    border_pixmap: c_ulong,
    border_pixel: c_ulong,
    bit_gravity: c_int,
    win_gravity: c_int,
    backing_store: c_int,
    backing_planes: c_ulong,
    backing_pixel: c_ulong,
    save_under: c_int,
    event_mask: c_long,
    do_not_propagate_mask: c_long,
    override_redirect: c_int,
    colormap: c_ulong,
    cursor: c_ulong,
}

#[repr(C)]
#[derive(Default)]
struct Hints {
    flags: c_long,
    input: c_int,
    initial_state: c_int,
    icon_pixmap: c_ulong,
    icon_window: c_ulong,
    icon_x: c_int,
    icon_y: c_int,
    icon_mask: c_ulong,
    window_group: c_ulong,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ExposeEvent {
    kind: c_int,
    serial: c_ulong,
    send_event: c_int,
    display: *mut c_void,
    window: c_ulong,
    x: c_int,
    y: c_int,
    width: c_int,
    height: c_int,
    count: c_int,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ClientMessageEvent {
    kind: c_int,
    serial: c_ulong,
    send_event: c_int,
    display: *mut c_void,
    window: c_ulong,
    message_type: c_ulong,
    format: c_int,
    data: [c_long; 5],
}

#[repr(C)]
union Event {
    expose: ExposeEvent,
    client_message: ClientMessageEvent,
    padding: [c_long; 24],
}

struct Library(*mut c_void);
impl Drop for Library {
    fn drop(&mut self) {
        unsafe {
            if !self.0.is_null() {
                dlclose(self.0);
            }
        }
    }
}

#[repr(C)]
struct XFixesCursorImage {
    x: i16,
    y: i16,
    width: u16,
    height: u16,
    xhot: u16,
    yhot: u16,
    serial: c_ulong,
    pixels: *mut c_ulong,
    atom: c_ulong,
    name: *const c_char,
}

pub struct CursorImage {
    pub x: i32,
    pub y: i32,
    pub xhot: u16,
    pub yhot: u16,
    pub width: u16,
    pub height: u16,
    pub pixels: Vec<u8>,
}

fn cursor_rgba(pixel: c_ulong) -> [u8; 4] {
    let alpha = ((pixel >> 24) & 255) as u32;
    let channel = |shift: u32| {
        if alpha == 0 {
            0
        } else {
            ((((pixel >> shift) & 255) as u32 * 255 + alpha / 2) / alpha).min(255) as u8
        }
    };
    [channel(16), channel(8), channel(0), alpha as u8]
}

/// Read the XFixes cursor separately from the static screenshot. No image of
/// the desktop, input grab, focus change or streaming session is requested.
pub fn cursor_image() -> Result<CursorImage, String> {
    // SAFETY: signatures/layout match Xlib and Xfixes headers; pixels are
    // copied before XFree and the display closes before either library unloads.
    unsafe {
        let xlib = Library(dlopen(c"libX11.so.6".as_ptr(), 2));
        let fixes = Library(dlopen(c"libXfixes.so.3".as_ptr(), 2));
        if xlib.0.is_null() || fixes.0.is_null() {
            return Err(t("Cannot read the XWayland cursor image"));
        }
        let open = dlsym(xlib.0, c"XOpenDisplay".as_ptr());
        let close = dlsym(xlib.0, c"XCloseDisplay".as_ptr());
        let free = dlsym(xlib.0, c"XFree".as_ptr());
        let get = dlsym(fixes.0, c"XFixesGetCursorImage".as_ptr());
        if [open, close, free, get].iter().any(|p| p.is_null()) {
            return Err(t("Cannot read the XWayland cursor image"));
        }
        let open: unsafe extern "C" fn(*const c_char) -> *mut c_void = std::mem::transmute(open);
        let close: unsafe extern "C" fn(*mut c_void) -> c_int = std::mem::transmute(close);
        let free: unsafe extern "C" fn(*mut c_void) -> c_int = std::mem::transmute(free);
        let get: unsafe extern "C" fn(*mut c_void) -> *mut XFixesCursorImage =
            std::mem::transmute(get);
        let display = open(std::ptr::null());
        if display.is_null() {
            return Err(t("Cannot connect to the XWayland display"));
        }
        let image = get(display);
        if image.is_null() {
            close(display);
            return Err(t("Cannot read the XWayland cursor image"));
        }
        let result = (|| {
            let image = &*image;
            if image.width == 0
                || image.height == 0
                || image.width > 1024
                || image.height > 1024
                || image.pixels.is_null()
            {
                return Err(t("Cannot read the XWayland cursor image"));
            }
            let pixels = std::slice::from_raw_parts(
                image.pixels,
                usize::from(image.width) * usize::from(image.height),
            );
            Ok(CursorImage {
                x: i32::from(image.x),
                y: i32::from(image.y),
                xhot: image.xhot,
                yhot: image.yhot,
                width: image.width,
                height: image.height,
                pixels: pixels.iter().flat_map(|p| cursor_rgba(*p)).collect(),
            })
        })();
        free(image.cast());
        close(display);
        result
    }
}

/// Read the root pointer position without changing focus or grabbing input.
pub fn pointer_position() -> Result<(i32, i32), String> {
    // SAFETY: function signatures match Xlib.h. No pointer outlives the library.
    unsafe {
        let handle = dlopen(c"libX11.so.6".as_ptr(), 2);
        if handle.is_null() {
            return Err(t("System libX11.so.6 is unavailable"));
        }
        let library = Library(handle);
        let names = [
            c"XOpenDisplay",
            c"XCloseDisplay",
            c"XDefaultRootWindow",
            c"XQueryPointer",
        ];
        let pointers = names.map(|name| dlsym(library.0, name.as_ptr()));
        if pointers.iter().any(|p| p.is_null()) {
            return Err(t("Cannot connect to the XWayland display"));
        }
        let open: unsafe extern "C" fn(*const c_char) -> *mut c_void =
            std::mem::transmute(pointers[0]);
        let close: unsafe extern "C" fn(*mut c_void) -> c_int = std::mem::transmute(pointers[1]);
        let root: unsafe extern "C" fn(*mut c_void) -> c_ulong = std::mem::transmute(pointers[2]);
        let query: unsafe extern "C" fn(
            *mut c_void,
            c_ulong,
            *mut c_ulong,
            *mut c_ulong,
            *mut c_int,
            *mut c_int,
            *mut c_int,
            *mut c_int,
            *mut u32,
        ) -> c_int = std::mem::transmute(pointers[3]);
        let display = open(std::ptr::null());
        if display.is_null() {
            return Err(t("Cannot connect to the XWayland display"));
        }
        let (mut root_window, mut child) = (0, 0);
        let (mut x, mut y, mut window_x, mut window_y, mut mask) = (0, 0, 0, 0, 0);
        let ok = query(
            display,
            root(display),
            &mut root_window,
            &mut child,
            &mut x,
            &mut y,
            &mut window_x,
            &mut window_y,
            &mut mask,
        );
        close(display);
        if ok == 0 {
            return Err(t("Cannot connect to the XWayland display"));
        }
        Ok((x, y))
    }
}

/// Observe only pointer state, Escape, and X focus while a menu exists.
/// Native Wayland keyboard events may be absent: never treat this as a global
/// keyboard hook, and never change input focus or grab the keyboard.
pub fn watch_menu_input(
    changed: impl FnMut((i32, i32, u32, bool, c_ulong)) -> Result<(), String>,
) -> Result<(), String> {
    use std::io::Read;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let stopped = Arc::new(AtomicBool::new(false));
    let eof = stopped.clone();
    std::thread::spawn(move || {
        let _ = std::io::stdin().read(&mut [0u8]);
        eof.store(true, Ordering::Relaxed);
    });
    watch_input(|| stopped.load(Ordering::Relaxed), changed)
}
/// Cancellable in-process observer used by the Rust desktop. Never reads stdin.
pub fn watch_input(
    stopped: impl Fn() -> bool,
    mut changed: impl FnMut((i32, i32, u32, bool, c_ulong)) -> Result<(), String>,
) -> Result<(), String> {
    // SAFETY: all signatures and buffers follow Xlib.h. This connection is
    // private to the observer, closes before dlclose, and owns no input grabs.
    unsafe {
        let handle = dlopen(c"libX11.so.6".as_ptr(), 2);
        if handle.is_null() {
            return Err(t("System libX11.so.6 is unavailable"));
        }
        let library = Library(handle);
        macro_rules! symbol {
            ($name:literal,$ty:ty) => {{
                let p = dlsym(library.0, concat!($name, "\0").as_ptr().cast());
                if p.is_null() {
                    return Err(tf("Missing Xlib symbol: {0}", &[$name]));
                }
                std::mem::transmute::<*mut c_void, $ty>(p)
            }};
        }
        let open = symbol!(
            "XOpenDisplay",
            unsafe extern "C" fn(*const c_char) -> *mut c_void
        );
        let close = symbol!("XCloseDisplay", unsafe extern "C" fn(*mut c_void) -> c_int);
        let root = symbol!(
            "XDefaultRootWindow",
            unsafe extern "C" fn(*mut c_void) -> c_ulong
        );
        let query = symbol!(
            "XQueryPointer",
            unsafe extern "C" fn(
                *mut c_void,
                c_ulong,
                *mut c_ulong,
                *mut c_ulong,
                *mut c_int,
                *mut c_int,
                *mut c_int,
                *mut c_int,
                *mut u32,
            ) -> c_int
        );
        let keymap = symbol!(
            "XQueryKeymap",
            unsafe extern "C" fn(*mut c_void, *mut u8) -> c_int
        );
        let keycode = symbol!(
            "XKeysymToKeycode",
            unsafe extern "C" fn(*mut c_void, c_ulong) -> u8
        );
        let focus = symbol!(
            "XGetInputFocus",
            unsafe extern "C" fn(*mut c_void, *mut c_ulong, *mut c_int) -> c_int
        );
        let display = open(std::ptr::null());
        if display.is_null() {
            return Err(t("Cannot connect to the XWayland display"));
        }
        let escape = keycode(display, 0xff1b) as usize;
        let mut previous = None;
        let result = (|| {
            while !stopped() {
                let (mut r, mut c, mut x, mut y, mut wx, mut wy, mut mask) = (0, 0, 0, 0, 0, 0, 0);
                if query(
                    display,
                    root(display),
                    &mut r,
                    &mut c,
                    &mut x,
                    &mut y,
                    &mut wx,
                    &mut wy,
                    &mut mask,
                ) == 0
                {
                    break;
                }
                let mut keys = [0u8; 32];
                keymap(display, keys.as_mut_ptr());
                let (mut window, mut revert) = (0, 0);
                focus(display, &mut window, &mut revert);
                let state = (
                    x,
                    y,
                    mask,
                    keys[escape / 8] & (1 << (escape % 8)) != 0,
                    window,
                );
                if previous != Some(state) {
                    changed(state)?;
                    previous = Some(state);
                }
                std::thread::sleep(std::time::Duration::from_millis(16));
            }
            Ok(())
        })();
        close(display);
        result
    }
}

struct CachedDisplay {
    display_name: Option<std::ffi::OsString>,
    display: *mut c_void,
    close: unsafe extern "C" fn(*mut c_void) -> c_int,
    attributes: unsafe extern "C" fn(*mut c_void, c_ulong, c_ulong, *const Attributes) -> c_int,
    hints: unsafe extern "C" fn(*mut c_void, c_ulong, *const Hints) -> c_int,
    property: unsafe extern "C" fn(
        *mut c_void,
        c_ulong,
        c_ulong,
        c_ulong,
        c_int,
        c_int,
        *const u8,
        c_int,
    ) -> c_int,
    move_resize: unsafe extern "C" fn(*mut c_void, c_ulong, c_int, c_int, u32, u32) -> c_int,
    focus: unsafe extern "C" fn(*mut c_void, c_ulong, c_int, c_ulong) -> c_int,
    flush: unsafe extern "C" fn(*mut c_void) -> c_int,
    sync: unsafe extern "C" fn(*mut c_void, c_int) -> c_int,
    geometry: unsafe extern "C" fn(
        *mut c_void,
        c_ulong,
        *mut c_ulong,
        *mut c_int,
        *mut c_int,
        *mut u32,
        *mut u32,
        *mut u32,
        *mut u32,
    ) -> c_int,
    unmap: unsafe extern "C" fn(*mut c_void, c_ulong) -> c_int,
    map: unsafe extern "C" fn(*mut c_void, c_ulong) -> c_int,
    create_pixmap: unsafe extern "C" fn(*mut c_void, c_ulong, u32, u32, u32) -> c_ulong,
    free_pixmap: unsafe extern "C" fn(*mut c_void, c_ulong) -> c_int,
    create_gc: unsafe extern "C" fn(*mut c_void, c_ulong, c_ulong, *const c_void) -> *mut c_void,
    free_gc: unsafe extern "C" fn(*mut c_void, *mut c_void) -> c_int,
    exposures: unsafe extern "C" fn(*mut c_void, *mut c_void, c_int) -> c_int,
    copy: unsafe extern "C" fn(
        *mut c_void,
        c_ulong,
        c_ulong,
        *mut c_void,
        c_int,
        c_int,
        u32,
        u32,
        c_int,
        c_int,
    ) -> c_int,
    foreground: unsafe extern "C" fn(*mut c_void, *mut c_void, c_ulong) -> c_int,
    fill: unsafe extern "C" fn(*mut c_void, c_ulong, *mut c_void, c_int, c_int, u32, u32) -> c_int,
    send: unsafe extern "C" fn(*mut c_void, c_ulong, c_int, c_long, *const Event) -> c_int,
    states: [c_ulong; 3],
    net_wm_state: c_ulong,
    net_wm_user_time: c_ulong,
    net_wm_frame_timings: c_ulong,
    wm_protocols: c_ulong,
    wm_delete_window: c_ulong,
    _library: Library,
}

// SAFETY: Every access to the Xlib Display* is serialized through SHARED_X11.
unsafe impl Send for CachedDisplay {}

impl Drop for CachedDisplay {
    fn drop(&mut self) {
        unsafe {
            if !self.display.is_null() {
                (self.close)(self.display);
            }
        }
    }
}

static SHARED_X11: std::sync::Mutex<Option<CachedDisplay>> = std::sync::Mutex::new(None);

fn with_display<R>(f: impl FnOnce(&CachedDisplay) -> Result<R, String>) -> Result<R, String> {
    let mut guard = SHARED_X11.lock().unwrap_or_else(|e| e.into_inner());
    let wanted = std::env::var_os("DISPLAY");
    if guard.as_ref().is_none_or(|c| c.display_name != wanted) {
        *guard = None;
        // SAFETY: NUL-terminated library/symbol names and Xlib.h signatures.
        let created = unsafe {
            let handle = dlopen(c"libX11.so.6".as_ptr(), 2);
            if handle.is_null() {
                return Err(t("System libX11.so.6 is unavailable"));
            }
            let library = Library(handle);
            macro_rules! symbol {
                ($name:literal, $ty:ty) => {{
                    let pointer = dlsym(library.0, concat!($name, "\0").as_ptr().cast());
                    if pointer.is_null() {
                        return Err(tf("Missing Xlib symbol: {0}", &[$name]));
                    }
                    std::mem::transmute::<*mut c_void, $ty>(pointer)
                }};
            }
            let open = symbol!(
                "XOpenDisplay",
                unsafe extern "C" fn(*const c_char) -> *mut c_void
            );
            let close = symbol!("XCloseDisplay", unsafe extern "C" fn(*mut c_void) -> c_int);
            let atom = symbol!(
                "XInternAtom",
                unsafe extern "C" fn(*mut c_void, *const c_char, c_int) -> c_ulong
            );
            let display = open(std::ptr::null());
            if display.is_null() {
                return Err(t("Cannot connect to the XWayland display"));
            }
            let states = [
                atom(display, c"_NET_WM_STATE_ABOVE".as_ptr(), 0),
                atom(display, c"_NET_WM_STATE_SKIP_TASKBAR".as_ptr(), 0),
                atom(display, c"_NET_WM_STATE_SKIP_PAGER".as_ptr(), 0),
            ];
            let net_wm_state = atom(display, c"_NET_WM_STATE".as_ptr(), 0);
            let net_wm_user_time = atom(display, c"_NET_WM_USER_TIME".as_ptr(), 0);
            let net_wm_frame_timings = atom(display, c"_NET_WM_FRAME_TIMINGS".as_ptr(), 0);
            let wm_protocols = atom(display, c"WM_PROTOCOLS".as_ptr(), 0);
            let wm_delete_window = atom(display, c"WM_DELETE_WINDOW".as_ptr(), 0);
            CachedDisplay {
                display_name: wanted,
                display,
                close,
                attributes: symbol!(
                    "XChangeWindowAttributes",
                    unsafe extern "C" fn(*mut c_void, c_ulong, c_ulong, *const Attributes) -> c_int
                ),
                hints: symbol!(
                    "XSetWMHints",
                    unsafe extern "C" fn(*mut c_void, c_ulong, *const Hints) -> c_int
                ),
                property: symbol!(
                    "XChangeProperty",
                    unsafe extern "C" fn(
                        *mut c_void,
                        c_ulong,
                        c_ulong,
                        c_ulong,
                        c_int,
                        c_int,
                        *const u8,
                        c_int,
                    ) -> c_int
                ),
                move_resize: symbol!(
                    "XMoveResizeWindow",
                    unsafe extern "C" fn(*mut c_void, c_ulong, c_int, c_int, u32, u32) -> c_int
                ),
                focus: symbol!(
                    "XSetInputFocus",
                    unsafe extern "C" fn(*mut c_void, c_ulong, c_int, c_ulong) -> c_int
                ),
                flush: symbol!("XFlush", unsafe extern "C" fn(*mut c_void) -> c_int),
                sync: symbol!("XSync", unsafe extern "C" fn(*mut c_void, c_int) -> c_int),
                geometry: symbol!(
                    "XGetGeometry",
                    unsafe extern "C" fn(
                        *mut c_void,
                        c_ulong,
                        *mut c_ulong,
                        *mut c_int,
                        *mut c_int,
                        *mut u32,
                        *mut u32,
                        *mut u32,
                        *mut u32,
                    ) -> c_int
                ),
                unmap: symbol!(
                    "XUnmapWindow",
                    unsafe extern "C" fn(*mut c_void, c_ulong) -> c_int
                ),
                map: symbol!(
                    "XMapWindow",
                    unsafe extern "C" fn(*mut c_void, c_ulong) -> c_int
                ),
                create_pixmap: symbol!(
                    "XCreatePixmap",
                    unsafe extern "C" fn(*mut c_void, c_ulong, u32, u32, u32) -> c_ulong
                ),
                free_pixmap: symbol!(
                    "XFreePixmap",
                    unsafe extern "C" fn(*mut c_void, c_ulong) -> c_int
                ),
                create_gc: symbol!(
                    "XCreateGC",
                    unsafe extern "C" fn(
                        *mut c_void,
                        c_ulong,
                        c_ulong,
                        *const c_void,
                    ) -> *mut c_void
                ),
                free_gc: symbol!(
                    "XFreeGC",
                    unsafe extern "C" fn(*mut c_void, *mut c_void) -> c_int
                ),
                exposures: symbol!(
                    "XSetGraphicsExposures",
                    unsafe extern "C" fn(*mut c_void, *mut c_void, c_int) -> c_int
                ),
                copy: symbol!(
                    "XCopyArea",
                    unsafe extern "C" fn(
                        *mut c_void,
                        c_ulong,
                        c_ulong,
                        *mut c_void,
                        c_int,
                        c_int,
                        u32,
                        u32,
                        c_int,
                        c_int,
                    ) -> c_int
                ),
                foreground: symbol!(
                    "XSetForeground",
                    unsafe extern "C" fn(*mut c_void, *mut c_void, c_ulong) -> c_int
                ),
                fill: symbol!(
                    "XFillRectangle",
                    unsafe extern "C" fn(
                        *mut c_void,
                        c_ulong,
                        *mut c_void,
                        c_int,
                        c_int,
                        u32,
                        u32,
                    ) -> c_int
                ),
                send: symbol!(
                    "XSendEvent",
                    unsafe extern "C" fn(
                        *mut c_void,
                        c_ulong,
                        c_int,
                        c_long,
                        *const Event,
                    ) -> c_int
                ),
                states,
                net_wm_state,
                net_wm_user_time,
                net_wm_frame_timings,
                wm_protocols,
                wm_delete_window,
                _library: library,
            }
        };
        *guard = Some(created);
    }
    f(guard.as_ref().unwrap())
}

/// Populate `GdkFrameTimings.refresh_interval` on an override-redirect GTK X11
/// surface via EWMH `_NET_WM_FRAME_TIMINGS`. Without WM frame messages, GTK4's
/// `GdkFrameClockIdle` hardcodes a 16667 us (60 Hz) sleep and pauses motion
/// delivery during that interval even on 90/120/144/240 Hz displays.
pub fn send_frame_timings(xid: u64, refresh_interval_us: i32) -> Result<(), String> {
    if xid == 0 || xid > u32::MAX.into() || refresh_interval_us <= 0 {
        return Err(t("Invalid X11 window identifier"));
    }
    let window = c_ulong::try_from(xid).map_err(|_| t("Invalid X11 window identifier"))?;
    with_display(|cx| unsafe {
        let mut event = Event { padding: [0; 24] };
        event.client_message = ClientMessageEvent {
            kind: 33,
            serial: 0,
            send_event: 1,
            display: cx.display,
            window,
            message_type: cx.net_wm_frame_timings,
            format: 32,
            data: [0, 0, 0, c_long::from(refresh_interval_us), 0],
        };
        (cx.send)(cx.display, window, 0, 0, &event);
        (cx.flush)(cx.display);
        Ok(())
    })
}

/// Request keyboard focus for an already configured, mapped capture overlay.
pub fn focus_overlay(xid: u64) -> Result<(), String> {
    if xid == 0 || xid > u32::MAX.into() {
        return Err(t("Invalid X11 window identifier"));
    }
    let window = c_ulong::try_from(xid).map_err(|_| t("Invalid X11 window identifier"))?;
    with_display(|cx| unsafe {
        (cx.focus)(cx.display, window, 1, 0);
        (cx.sync)(cx.display, 0);
        Ok(())
    })
}

/// Configure an existing, not-yet-mapped GTK X11 surface. Coordinates are X11
/// pixels. The caller retains ownership and maps it only after this returns.
pub fn configure(xid: u64, x: i32, y: i32, width: u32, height: u32) -> Result<(), String> {
    configure_surface(xid, x, y, width, height, None, false)
}

/// Release keyboard focus without destroying the client's GdkSurface.
pub fn release_focus(xid: u64, x: i32, y: i32, width: u32, height: u32) -> Result<(), String> {
    configure_surface(xid, x, y, width, height, None, true)
}

/// Overlays accept keyboard input during explicit capture or pin editing.
/// `focus` must remain false until the caller has mapped the surface.
pub fn configure_overlay(
    xid: u64,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    focus: bool,
) -> Result<(), String> {
    configure_surface(xid, x, y, width, height, Some(focus), false)
}

fn configure_surface(
    xid: u64,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    input: Option<bool>,
    remap: bool,
) -> Result<(), String> {
    if xid == 0
        || xid > u32::MAX.into()
        || width == 0
        || height == 0
        || width > 65535
        || height > 65535
        || i16::try_from(x).is_err()
        || i16::try_from(y).is_err()
    {
        return Err(t("Invalid X11 surface geometry"));
    }
    let window = c_ulong::try_from(xid).map_err(|_| t("Invalid X11 window identifier"))?;
    with_display(|cx| unsafe {
        let display = cx.display;
        // A raw remap allocates new Composite storage even when GTK keeps its
        // GdkSurface, texture and render tree. Retain the *server-side pixels*
        // before any resize/unmap so the newly mapped surface is already drawn
        // while GTK is waiting for its next frame. No CPU image readback.
        let mut old_root = 0;
        let (mut old_x, mut old_y) = (0, 0);
        let (mut old_width, mut old_height, mut border, mut depth) = (0, 0, 0, 0);
        let known_geometry = (cx.geometry)(
            display,
            window,
            &mut old_root,
            &mut old_x,
            &mut old_y,
            &mut old_width,
            &mut old_height,
            &mut border,
            &mut depth,
        ) != 0;
        let retained = if remap && known_geometry && old_width > 0 && old_height > 0 {
            let pixmap = (cx.create_pixmap)(display, window, old_width, old_height, depth);
            let gc = (cx.create_gc)(display, window, 0, std::ptr::null());
            (cx.exposures)(display, gc, 0);
            (cx.copy)(
                display, window, pixmap, gc, 0, 0, old_width, old_height, 0, 0,
            );
            Some((pixmap, gc))
        } else {
            None
        };
        (cx.attributes)(
            display,
            window,
            1 << 9,
            &Attributes {
                override_redirect: 1,
                ..Default::default()
            },
        );
        (cx.hints)(
            display,
            window,
            &Hints {
                flags: 1,
                input: c_int::from(input.is_some()),
                ..Default::default()
            },
        );
        (cx.property)(
            display,
            window,
            cx.net_wm_state,
            4,
            32,
            0,
            cx.states.as_ptr().cast(),
            3,
        );
        let time: c_ulong = 0;
        (cx.property)(
            display,
            window,
            cx.net_wm_user_time,
            6,
            32,
            0,
            (&time as *const c_ulong).cast(),
            1,
        );
        // Do not advertise WM_TAKE_FOCUS, including when GTK initially set it.
        let protocols = [cx.wm_delete_window];
        (cx.property)(
            display,
            window,
            cx.wm_protocols,
            4,
            32,
            0,
            protocols.as_ptr().cast(),
            1,
        );
        if !known_geometry || (old_x, old_y, old_width, old_height) != (x, y, width, height) {
            (cx.move_resize)(display, window, x, y, width, height);
        }
        if input == Some(true) {
            // RevertToPointerRoot, CurrentTime. Passive surfaces never request focus.
            (cx.focus)(display, window, 1, 0);
        }
        if remap {
            (cx.unmap)(display, window);
            (cx.map)(display, window);
            if let Some((pixmap, gc)) = retained {
                if (old_width, old_height) != (width, height) {
                    (cx.foreground)(display, gc, 0);
                    (cx.fill)(display, window, gc, 0, 0, width, height);
                }
                (cx.copy)(
                    display,
                    pixmap,
                    window,
                    gc,
                    0,
                    0,
                    old_width.min(width),
                    old_height.min(height),
                    0,
                    0,
                );
                (cx.free_gc)(display, gc);
                (cx.free_pixmap)(display, pixmap);
            }
            // queue_draw/queue_render alone allow GSK to reuse unchanged nodes
            // and repaint only the removed controls. Explicit full Expose tells
            // GDK that *all* backing pixels need a fresh frame after the copy.
            // Unlike XClearArea, this does not erase the retained image first.
            let mut event = Event { padding: [0; 24] };
            event.expose = ExposeEvent {
                kind: 12,
                serial: 0,
                send_event: 1,
                display,
                window,
                x: 0,
                y: 0,
                width: width as c_int,
                height: height as c_int,
                count: 0,
            };
            (cx.send)(display, window, 0, 1 << 15, &event);
        }
        (cx.sync)(display, 0);
        Ok(())
    })
}

#[cfg(test)]
mod cursor_tests {
    use super::*;
    #[test]
    fn premultiplied_argb_becomes_straight_rgba() {
        assert_eq!(cursor_rgba(0xff123456), [0x12, 0x34, 0x56, 255]);
        assert_eq!(cursor_rgba(0x80402010), [128, 64, 32, 128]);
        assert_eq!(cursor_rgba(0x00123456), [0, 0, 0, 0]);
    }
}

//! X11/XWayland surfaces and passive menu observation. No layer-shell calls.
use super::{backend::Signals, runtime::Library};
use clivuu_core::{annotation::Bounds, i18n::t};
use gtk::{gdk, glib, glib::translate::*, prelude::*};
use std::{
    cell::{Cell, RefCell},
    ffi::{c_char, c_int, c_ulong},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
type Surface = *mut gdk::ffi::GdkSurface;

/// Keep X11's named-cursor cache in sync with the display's surface scale.
/// GTK normalizes XSettings cursor size to logical pixels, so a scale change
/// can leave that setting unchanged and skip rebuilding physical cursors.
pub struct CursorThemeWatch {
    display: gdk::Display,
    changed: Option<glib::SignalHandlerId>,
    pending: Rc<RefCell<Option<glib::SourceId>>>,
    _library: Rc<Library>,
}
impl CursorThemeWatch {
    pub fn new(display: &gdk::Display) -> Result<Option<Self>, String> {
        if display.type_().name() != "GdkX11Display" {
            return Ok(None);
        }
        let library = Rc::new(Library::open(c"libgtk-4.so.1")?);
        type SetTheme = unsafe extern "C" fn(*mut gdk::ffi::GdkDisplay, *const c_char, c_int);
        // SAFETY: public GDK X11 API, invoked only on a GdkX11Display.
        let set_theme: SetTheme = unsafe { library.symbol(c"gdk_x11_display_set_cursor_theme")? };
        let pending = Rc::new(RefCell::new(None));
        let queued = pending.clone();
        let api = library.clone();
        let changed = display.connect_setting_changed(move |display, name| {
            if name != "gdk-window-scaling-factor" || queued.borrow().is_some() {
                return;
            }
            let display = display.clone();
            let pending = queued.clone();
            let api = api.clone();
            // XSettings updates scale before emitting its remaining settings.
            // Read the final theme and size once that batch has completed.
            let id = glib::idle_add_local_once(move || {
                let _api = api;
                pending.borrow_mut().take();
                let settings = gtk::Settings::for_display(&display);
                if let Some(theme) = settings.gtk_cursor_theme_name() {
                    // GDK applies the current X11 surface scale and refreshes
                    // already installed named cursors, including popup/grab ones.
                    unsafe {
                        set_theme(
                            display.to_glib_none().0,
                            theme.to_glib_none().0,
                            settings.gtk_cursor_theme_size(),
                        );
                    }
                }
            });
            queued.replace(Some(id));
        });
        Ok(Some(Self {
            display: display.clone(),
            changed: Some(changed),
            pending,
            _library: library,
        }))
    }
}
impl Drop for CursorThemeWatch {
    fn drop(&mut self) {
        if let Some(changed) = self.changed.take() {
            self.display.disconnect(changed);
        }
        if let Some(pending) = self.pending.borrow_mut().take() {
            pending.remove();
        }
    }
}

fn frame_interval_us(_display: &gdk::Display) -> i32 {
    // Override-redirect X11/XWayland surfaces receive no WM frame phase, while
    // GTK4's Vulkan/GL backends present with MAILBOX/swap-interval-0 semantics.
    // A sub-millisecond interval (500 us -> 0 ms GdkFrameClockIdle timer delay)
    // still drains and coalesces each pending X11 MotionNotify batch at event
    // priority before painting immediately for the very next hardware vblank,
    // without queuing idle timers when no redraw is requested.
    500
}

pub(super) struct Backend {
    _library: Library,
    xid: unsafe extern "C" fn(Surface) -> c_ulong,
    taskbar: unsafe extern "C" fn(Surface, c_int),
    pager: unsafe extern "C" fn(Surface, c_int),
    sync: unsafe extern "C" fn(Surface, c_int),
    time: unsafe extern "C" fn(Surface, u32),
}
impl Backend {
    pub fn load() -> Result<Self, String> {
        let lib = Library::open(c"libgtk-4.so.1")?;
        // SAFETY: GDK X11 signatures; loaded only for GdkX11Display.
        unsafe {
            Ok(Self {
                xid: lib.symbol(c"gdk_x11_surface_get_xid")?,
                taskbar: lib.symbol(c"gdk_x11_surface_set_skip_taskbar_hint")?,
                pager: lib.symbol(c"gdk_x11_surface_set_skip_pager_hint")?,
                sync: lib.symbol(c"gdk_x11_surface_set_frame_sync_enabled")?,
                time: lib.symbol(c"gdk_x11_surface_set_user_time")?,
                _library: lib,
            })
        }
    }
    pub fn prepare_capture(
        self: &Rc<Self>,
        window: &gtk::Window,
        rect: Bounds,
        notify: Rc<dyn Fn(Option<String>)>,
        signals: Signals,
    ) -> Result<(), String> {
        window.set_resizable(false);
        window.set_size_request(rect.width.ceil() as i32, rect.height.ceil() as i32);
        gtk::prelude::WidgetExt::realize(window);
        self.capture(window, rect, false)?;
        let api = self.clone();
        let map_signals = signals.clone();
        let map_id = window.connect_map(move |w| {
            api.report_frame_timings(w);
            // MapNotify precedes Mutter's XWayland/Wayland focus reconciliation.
            let Some(clock) = w.frame_clock() else { return };
            let weak = w.downgrade();
            let api = api.clone();
            let notify = notify.clone();
            let focused = Cell::new(false);
            let id = clock.connect_after_paint(move |_| {
                let Some(w) = weak.upgrade().filter(|w| w.is_mapped()) else {
                    return;
                };
                api.report_frame_timings(&w);
                if !focused.replace(true)
                    && let Err(error) = api.focus_capture(&w)
                {
                    notify(Some(error));
                }
            });
            map_signals.borrow_mut().push((clock.upcast(), id));
        });
        signals.borrow_mut().push((window.clone().upcast(), map_id));
        Ok(())
    }
    pub fn report_frame_timings(&self, window: &gtk::Window) {
        let Some(surface) = window.surface() else {
            return;
        };
        let xid = unsafe { (self.xid)(surface.to_glib_none().0) };
        let interval = frame_interval_us(&gtk::prelude::WidgetExt::display(window));
        let _ = clivuu_x11::send_frame_timings(xid, interval);
    }
    fn focus_capture(&self, window: &gtk::Window) -> Result<(), String> {
        let surface = window
            .surface()
            .ok_or_else(|| t("The desktop service is unavailable"))?;
        let xid = unsafe { (self.xid)(surface.to_glib_none().0) };
        clivuu_x11::focus_overlay(xid)
    }
    fn capture(
        &self,
        window: &gtk::Window,
        rect: clivuu_core::annotation::Bounds,
        focus: bool,
    ) -> Result<(), String> {
        let api = self;
        let surface = window
            .surface()
            .ok_or_else(|| t("The desktop service is unavailable"))?;
        let scale = f64::from(window.scale_factor());
        // SAFETY: this backend was selected using the concrete X11 display.
        // GDK retains ownership; synchronize before the private Xlib connection.
        let xid = unsafe {
            let s = surface.to_glib_none().0;
            (api.taskbar)(s, 1);
            (api.pager)(s, 1);
            (api.sync)(s, 0);
            (api.time)(s, 0);
            gtk::prelude::WidgetExt::display(window).sync();
            (api.xid)(s)
        };
        clivuu_x11::configure_overlay(
            xid,
            (rect.x * scale).round() as i32,
            (rect.y * scale).round() as i32,
            (rect.width * scale).round() as u32,
            (rect.height * scale).round() as u32,
            focus,
        )
    }
    pub fn pin(
        &self,
        window: &gtk::Window,
        canvas: clivuu_core::annotation::Bounds,
        keyboard: bool,
        focus: bool,
        release: bool,
    ) -> Result<(), String> {
        let api = self;
        let surface = window
            .surface()
            .ok_or_else(|| t("The desktop service is unavailable"))?;
        let scale = f64::from(window.scale_factor());
        let xid = unsafe {
            let s = surface.to_glib_none().0;
            (api.taskbar)(s, 1);
            (api.pager)(s, 1);
            (api.sync)(s, 0);
            (api.time)(s, 0);
            gtk::prelude::WidgetExt::display(window).sync();
            (api.xid)(s)
        };
        let (x, y, w, h) = (
            (canvas.x * scale).round() as i32,
            (canvas.y * scale).round() as i32,
            (canvas.width * scale).round() as u32,
            (canvas.height * scale).round() as u32,
        );
        if release {
            clivuu_x11::release_focus(xid, x, y, w, h)
        } else if keyboard {
            clivuu_x11::configure_overlay(xid, x, y, w, h, focus)
        } else {
            clivuu_x11::configure(xid, x, y, w, h)
        }
    }
}

/// A passive observer owns no grabs and cancels all queued work on dismissal.
pub(super) struct MenuWatch(Arc<AtomicBool>);
impl Drop for MenuWatch {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}
pub(super) fn watch_menu(dismiss: impl Fn() + 'static) -> MenuWatch {
    let stop = Arc::new(AtomicBool::new(false));
    let lease = stop.clone();
    let (tx, rx) = async_channel::bounded(8);
    std::thread::spawn(move || {
        let _ = clivuu_x11::watch_input(
            || lease.load(Ordering::Relaxed),
            move |state| {
                let _ = tx.try_send(state);
                Ok(())
            },
        );
    });
    let lease = stop.clone();
    glib::MainContext::default().spawn_local(async move {
        let mut first = None;
        while let Ok((_, _, mask, escape, focus)) = rx.recv().await {
            if lease.load(Ordering::Relaxed) {
                break;
            }
            if let Some(before) = first {
                if before != focus || escape || mask & 0x48 != 0 {
                    dismiss();
                    break;
                }
            } else {
                first = Some(focus);
            }
        }
    });
    MenuWatch(stop)
}

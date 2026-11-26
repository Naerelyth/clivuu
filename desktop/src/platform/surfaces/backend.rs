//! Surface dispatch only. Native ABIs and their focus policies live in adapters.
use super::{layer_shell, x11};
use clivuu_core::{
    annotation::Bounds,
    capture::{Output, Selection},
    i18n::t,
    pins::host_rect,
};
use gtk::{gdk, glib, prelude::*};
use std::{cell::RefCell, rc::Rc};
pub(super) type Signals = Rc<RefCell<Vec<(glib::Object, glib::SignalHandlerId)>>>;
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PinRole {
    Pin,
    Menu,
    Controls,
}
#[derive(Clone)]
pub(super) enum Backend {
    X11(Rc<x11::Backend>),
    Layer(Rc<layer_shell::Backend>),
}
pub struct MenuWatch {
    _x11: Option<x11::MenuWatch>,
}
impl Backend {
    pub fn detect(display: &gdk::Display) -> Result<Self, String> {
        match display.type_().name() {
            "GdkX11Display" => Ok(Self::X11(Rc::new(x11::Backend::load()?))),
            "GdkWaylandDisplay" => Ok(Self::Layer(Rc::new(layer_shell::Backend::load()?))),
            _ => Err(t("The desktop service is unavailable")),
        }
    }
    pub fn uses_native_focus(&self) -> bool {
        matches!(self, Self::Layer(_))
    }
    pub fn capture_rectangles(&self, outputs: &[Output]) -> Result<Vec<Bounds>, String> {
        match self {
            Self::X11(_) => Ok(vec![
                Selection::new(outputs.to_vec(), vec![], None)
                    .ok_or_else(|| t("Invalid monitor layout"))?
                    .limit,
            ]),
            Self::Layer(_) => Ok(outputs.iter().map(|o| o.rect).collect()),
        }
    }
    pub fn prepare_capture(
        &self,
        window: &gtk::Window,
        rect: Bounds,
        monitor: Option<&gdk::Monitor>,
        notify: Rc<dyn Fn(Option<String>)>,
        signals: Signals,
    ) -> Result<(), String> {
        match self {
            Self::X11(api) => api.prepare_capture(window, rect, notify, signals),
            Self::Layer(api) => {
                api.capture(window, monitor.ok_or_else(|| t("Invalid monitor layout"))?);
                Ok(())
            }
        }
    }
    pub fn pin_canvas(&self, rect: Bounds, outputs: &[Output]) -> Bounds {
        match self {
            Self::X11(_) => host_rect(rect, outputs),
            Self::Layer(_) => rect,
        }
    }
    pub fn prepare_pin(
        &self,
        window: &gtk::Window,
        canvas: Bounds,
        monitor: &gdk::Monitor,
        role: PinRole,
    ) -> Result<(), String> {
        match self {
            Self::X11(api) => {
                window.set_focusable(role != PinRole::Menu);
                window.set_resizable(false);
                window.set_size_request(canvas.width as i32, canvas.height as i32);
                gtk::prelude::WidgetExt::realize(window);
                api.pin(window, canvas, false, false, false)
            }
            Self::Layer(api) => api.pin(window, monitor, role),
        }
    }
    pub fn set_pin_keyboard(
        &self,
        window: &gtk::Window,
        canvas: Bounds,
        role: PinRole,
        enabled: bool,
        focus: bool,
        release: bool,
    ) -> Result<(), String> {
        match self {
            Self::X11(api) => api.pin(window, canvas, enabled, focus, release),
            Self::Layer(api) => {
                api.edit(window, role, enabled);
                Ok(())
            }
        }
    }
    pub fn report_frame_timings(&self, window: &gtk::Window) {
        if let Self::X11(api) = self {
            api.report_frame_timings(window);
        }
    }
    pub fn watch_menu(&self, dismiss: impl Fn() + 'static) -> MenuWatch {
        MenuWatch {
            _x11: match self {
                Self::X11(_) => Some(x11::watch_menu(dismiss)),
                Self::Layer(_) => None,
            },
        }
    }
}

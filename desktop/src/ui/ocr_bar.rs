//! Compact 32px floating OCR control bar and inline model-setup prompt.
use super::{floating_bar, icons, popup::PopupLayer};
use crate::{
    editor::ocr_layer::{OcrOverlayState, OcrPhase},
    infrastructure::ocr::{DownloadState, OcrDownloadManager},
};
use clivuu_core::{
    i18n::{t, tf},
    ocr::{
        OcrEngineChoice,
        assets::{MODELS, find_model_spec, is_model_installed},
        tesseract,
    },
};
use gtk::prelude::*;
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Debug)]
pub enum OcrBarAction {
    SelectEngine(OcrEngineChoice, String),
    ToggleParagraphs,
    SelectAll,
    CopyText,
    PinText,
    Retry,
    Cancel,
    OpenSettings,
}

type ActionCallback = Rc<dyn Fn(OcrBarAction)>;

#[derive(Clone, Debug, PartialEq, Eq)]
enum PhaseSignature {
    NeedsSetup(DownloadState, u64),
    Scanning,
    Failed(String),
    Ready {
        empty: bool,
        has_selection: bool,
        copied: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BarSignature {
    language: clivuu_core::i18n::Language,
    engine: OcrEngineChoice,
    model_id: String,
    download_mirror: String,
    smart_paragraphs: bool,
    phase: PhaseSignature,
}

struct Inner {
    widget: gtk::Box,
    scroller: gtk::ScrolledWindow,
    row: gtk::Box,
    popups: PopupLayer,
    pin_mode: bool,
    signature: RefCell<Option<BarSignature>>,
    on_action: RefCell<Option<ActionCallback>>,
}

#[derive(Clone)]
pub struct OcrBar(Rc<Inner>);

impl OcrBar {
    pub fn new(popups: &PopupLayer, pin_mode: bool) -> Self {
        let widget = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .halign(gtk::Align::Start)
            .valign(gtk::Align::Start)
            .visible(false)
            .build();
        widget.add_css_class("clivuu-capture-toolbar");
        widget.set_size_request(-1, floating_bar::TOOLBAR_HEIGHT);
        let (scroller, row) = floating_bar::scrollable_bar_row();
        widget.append(&scroller);

        Self(Rc::new(Inner {
            widget,
            scroller,
            row,
            popups: popups.clone(),
            pin_mode,
            signature: RefCell::new(None),
            on_action: RefCell::new(None),
        }))
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.0.widget
    }

    pub fn row(&self) -> &gtk::Box {
        &self.0.row
    }

    pub fn set_max_content_width(&self, width: i32) {
        floating_bar::sync_scroller_width(&self.0.scroller, &self.0.row, (width - 8).max(1));
    }

    pub fn connect_action(&self, callback: impl Fn(OcrBarAction) + 'static) {
        self.0.on_action.replace(Some(Rc::new(callback)));
    }

    fn emit(&self, action: OcrBarAction) {
        if let Some(cb) = self.0.on_action.borrow().as_ref().cloned() {
            cb(action);
        }
    }

    pub fn sync(&self, state: &OcrOverlayState) {
        if !state.is_active() {
            if self.0.signature.replace(None).is_some() {
                while let Some(child) = self.0.row.first_child() {
                    self.0.row.remove(&child);
                }
            }
            self.0.widget.set_visible(false);
            return;
        }

        let manager = OcrDownloadManager::shared();
        manager.refresh_states();
        let dl_state = manager.state(&state.settings.model_id);
        let next_sig = BarSignature {
            language: clivuu_core::i18n::current_language(),
            engine: state.settings.engine,
            model_id: state.settings.model_id.clone(),
            download_mirror: state.settings.download_mirror.clone(),
            smart_paragraphs: state.settings.smart_paragraphs,
            phase: match &state.phase {
                OcrPhase::Inactive => unreachable!(),
                OcrPhase::NeedsSetup => PhaseSignature::NeedsSetup(
                    dl_state.clone(),
                    manager.bundle_download_bytes(&state.settings.model_id, true),
                ),
                OcrPhase::Scanning { .. } => PhaseSignature::Scanning,
                OcrPhase::Failed(err) => PhaseSignature::Failed(err.clone()),
                OcrPhase::Ready => PhaseSignature::Ready {
                    empty: !state.selection.is_active(),
                    has_selection: state.selection.has_selection(),
                    copied: state.recently_copied(),
                },
            },
        };
        if self.0.signature.borrow().as_ref() == Some(&next_sig) {
            self.0.widget.set_visible(true);
            return;
        }
        self.0.signature.replace(Some(next_sig));
        while let Some(child) = self.0.row.first_child() {
            self.0.row.remove(&child);
        }
        self.0.widget.set_visible(true);

        match &state.phase {
            OcrPhase::Inactive => {
                self.0.widget.set_visible(false);
            }
            OcrPhase::NeedsSetup => {
                self.append_engine_selector(state);
                self.0.row.append(&floating_bar::tool_separator());
                match dl_state {
                    DownloadState::Downloading(pct) => {
                        let status = floating_bar::bar_label(&tf(
                            "Downloading OCR model ({0}%)…",
                            &[&pct.to_string()],
                        ));
                        status.set_margin_start(4);
                        status.set_margin_end(4);
                        self.0.row.append(&status);

                        let (cancel_btn, _) = floating_bar::text_button(&t("Cancel"));
                        let model_id = state.settings.model_id.clone();
                        cancel_btn.connect_clicked(move |_| {
                            OcrDownloadManager::shared().cancel(&model_id);
                        });
                        self.0.row.append(&cancel_btn);
                    }
                    DownloadState::Failed(err) => {
                        let status = floating_bar::bar_label(&t("Model download failed"));
                        status.set_tooltip_text(Some(&err));
                        status.set_margin_start(4);
                        status.set_margin_end(4);
                        self.0.row.append(&status);

                        let (retry_btn, _) = floating_bar::text_button(&t("Retry"));
                        retry_btn.add_css_class("suggested-action");
                        let model_id = state.settings.model_id.clone();
                        let mirror = state.settings.download_mirror.clone();
                        retry_btn.connect_clicked(move |_| {
                            OcrDownloadManager::shared()
                                .start_model_download(&model_id, true, &mirror);
                        });
                        self.0.row.append(&retry_btn);
                    }
                    DownloadState::Idle | DownloadState::Installed => {
                        let bytes = manager.bundle_download_bytes(&state.settings.model_id, true);
                        let mib = (bytes as f64 / (1024.0 * 1024.0)).max(0.1);
                        let (dl_btn, _) = floating_bar::text_button(&tf(
                            "Download model ({0} MB)",
                            &[&format!("{mib:.0}")],
                        ));
                        dl_btn.add_css_class("suggested-action");
                        let model_id = state.settings.model_id.clone();
                        let mirror = state.settings.download_mirror.clone();
                        dl_btn.connect_clicked(move |_| {
                            OcrDownloadManager::shared()
                                .start_model_download(&model_id, true, &mirror);
                        });
                        self.0.row.append(&dl_btn);
                    }
                }
                self.append_settings_button();
            }
            OcrPhase::Scanning { .. } => {
                let status = floating_bar::bar_label(&t("Recognizing text…"));
                status.set_margin_start(6);
                status.set_margin_end(4);
                self.0.row.append(&status);

                let (cancel_btn, _) = floating_bar::text_button(&t("Cancel"));
                let bar = self.clone();
                cancel_btn.connect_clicked(move |_| bar.emit(OcrBarAction::Cancel));
                self.0.row.append(&cancel_btn);
            }
            OcrPhase::Failed(err) => {
                self.append_engine_selector(state);
                self.0.row.append(&floating_bar::tool_separator());
                let status = floating_bar::bar_label(&t("Recognition failed"));
                status.set_tooltip_text(Some(err));
                status.set_margin_start(4);
                status.set_margin_end(4);
                self.0.row.append(&status);

                let (retry_btn, _) = floating_bar::text_button(&t("Retry"));
                retry_btn.add_css_class("suggested-action");
                let bar = self.clone();
                retry_btn.connect_clicked(move |_| bar.emit(OcrBarAction::Retry));
                self.0.row.append(&retry_btn);
                self.append_settings_button();
            }
            OcrPhase::Ready => {
                self.append_engine_selector(state);
                self.0.row.append(&floating_bar::tool_separator());

                if !state.selection.is_active() {
                    let empty = floating_bar::bar_label(&t("No text found"));
                    empty.set_margin_start(6);
                    empty.set_margin_end(6);
                    self.0.row.append(&empty);
                    self.append_settings_button();
                    return;
                }

                let (para_btn, _) = floating_bar::text_button(&t("Paragraphs"));
                para_btn.set_tooltip_text(Some(&t("Preserve paragraph breaks when copying")));
                if state.settings.smart_paragraphs {
                    para_btn.add_css_class("suggested-action");
                }
                {
                    let bar = self.clone();
                    para_btn.connect_clicked(move |_| bar.emit(OcrBarAction::ToggleParagraphs));
                }
                self.0.row.append(&para_btn);

                let (all_btn, _) = floating_bar::text_button(&t("Select all"));
                all_btn.set_tooltip_text(Some("Ctrl+A"));
                {
                    let bar = self.clone();
                    all_btn.connect_clicked(move |_| bar.emit(OcrBarAction::SelectAll));
                }
                self.0.row.append(&all_btn);

                if !self.0.pin_mode {
                    let (pin_btn, _) = floating_bar::text_button(&t("Pin text"));
                    pin_btn.set_tooltip_text(Some(&t("Create a text pin from recognized text")));
                    let bar = self.clone();
                    pin_btn.connect_clicked(move |_| bar.emit(OcrBarAction::PinText));
                    self.0.row.append(&pin_btn);
                }

                let copy_label = if state.recently_copied() {
                    t("Copied")
                } else if state.selection.has_selection() {
                    t("Copy selected")
                } else {
                    t("Copy all")
                };
                let (copy_btn, _) = floating_bar::text_button(&copy_label);
                copy_btn.add_css_class("suggested-action");
                copy_btn.set_tooltip_text(Some("Ctrl+C"));
                {
                    let bar = self.clone();
                    copy_btn.connect_clicked(move |_| bar.emit(OcrBarAction::CopyText));
                }
                self.0.row.append(&copy_btn);
                self.append_settings_button();
            }
        }
    }

    fn append_engine_selector(&self, state: &OcrOverlayState) {
        let current_label = match state.settings.engine {
            OcrEngineChoice::Tesseract => t("System Tesseract"),
            OcrEngineChoice::Auto | OcrEngineChoice::Paddle => {
                find_model_spec(&state.settings.model_id)
                    .map(|m| m.localized_name())
                    .unwrap_or_else(|| t("Chinese, Japanese & English"))
            }
        };
        let (btn, _) = floating_bar::choice_button(&current_label);
        btn.set_tooltip_text(Some(&t("OCR language & engine")));
        let bar = self.clone();
        let active_engine = state.settings.engine;
        let active_model = state.settings.model_id.clone();
        let anchor = btn.clone();
        btn.connect_clicked(move |_| {
            bar.open_engine_menu(&anchor, active_engine, &active_model);
        });
        self.0.row.append(&btn);
    }

    fn open_engine_menu(
        &self,
        anchor: &gtk::Button,
        active_engine: OcrEngineChoice,
        active_model: &str,
    ) {
        let list = floating_bar::choice_list();
        for spec in MODELS {
            let installed = is_model_installed(spec.id);
            let label = if installed {
                spec.localized_name()
            } else {
                tf("{0} (Download)", &[&spec.localized_name()])
            };
            let selected = active_engine != OcrEngineChoice::Tesseract && active_model == spec.id;
            let item = floating_bar::choice_item(&label, selected);
            let bar = self.clone();
            let popups = self.0.popups.clone();
            let id = spec.id.to_owned();
            item.connect_clicked(move |_| {
                popups.close();
                bar.emit(OcrBarAction::SelectEngine(
                    OcrEngineChoice::Paddle,
                    id.clone(),
                ));
            });
            list.append(&item);
        }
        if tesseract::is_available() {
            list.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
            let selected = active_engine == OcrEngineChoice::Tesseract;
            let item = floating_bar::choice_item(&t("System Tesseract"), selected);
            let bar = self.clone();
            let popups = self.0.popups.clone();
            let current_model = active_model.to_owned();
            item.connect_clicked(move |_| {
                popups.close();
                bar.emit(OcrBarAction::SelectEngine(
                    OcrEngineChoice::Tesseract,
                    current_model.clone(),
                ));
            });
            list.append(&item);
        }
        self.0.popups.show(anchor, &list, || {});
    }

    fn append_settings_button(&self) {
        let btn = gtk::Button::builder()
            .child(&icons::icon("preferences"))
            .focusable(false)
            .valign(gtk::Align::Center)
            .tooltip_text(t("OCR settings"))
            .build();
        let bar = self.clone();
        btn.connect_clicked(move |_| bar.emit(OcrBarAction::OpenSettings));
        self.0.row.append(&btn);
    }
}

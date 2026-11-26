//! Sequence options operate on a future counter independently of selected badges.
use super::*;
use crate::editor::capture_view::Canvas;
use crate::editor::render;
use clivuu_core::annotation::sequence::{Format, MAX_INDEX, Patch, Shape};

impl Subbar {
    fn sequence_sample(&self, shape: Option<Shape>) -> Canvas {
        let sample = Canvas::new(|_| {});
        sample.set_size_request(24, 24);
        sample.set_valign(gtk::Align::Center);
        let current = Rc::new(RefCell::new(clivuu_core::annotation::Style::for_kind(
            Kind::Sequence,
            1.,
        )));
        let style = current.clone();
        let weak = sample.downgrade();
        sample.set_painter(move |snapshot| {
            let Some(sample) = weak.upgrade() else { return };
            let mut m = Mark::new(Kind::Sequence, [12., 12.], 1.);
            m.style = style.borrow().clone();
            m.style.width = 6.;
            if let Some(shape) = shape {
                m.style.sequence.shape = shape;
            }
            render::sequence_badge(snapshot, &m, &sample.pango_context());
        });
        let scene = self.0.scene.clone();
        let picture = sample.clone();
        self.update(move |m, _| {
            let mut style = m.style.clone();
            if let Some(scene) = scene.upgrade() {
                style = scene
                    .controller()
                    .borrow()
                    .session()
                    .style(Kind::Sequence)
                    .style
                    .clone();
            }
            if *current.borrow() != style {
                *current.borrow_mut() = style;
                picture.queue_draw();
            }
        });
        sample
    }
    fn sequence_next(&self) {
        let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let spin = gtk::SpinButton::with_range(1., f64::from(MAX_INDEX), 1.);
        spin.set_numeric(true);
        content.append(&spin);
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let reset = gtk::Button::with_label(&t("Reset"));
        let apply = gtk::Button::with_label(&t("Confirm"));
        apply.set_tooltip_text(Some(&t("Confirm")));
        apply.add_css_class("suggested-action");
        actions.append(&reset);
        actions.append(&apply);
        content.append(&actions);
        // Native SpinButton activation first commits an edited value, then
        // activates the default on a second Enter. This picker submits both
        // steps together, through the same confirmation button as a click.
        let accept = apply.downgrade();
        let panels = self.0.panels.clone();
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        keys.connect_key_pressed(move |_, key, _, _| match key {
            gdk::Key::Return | gdk::Key::KP_Enter => {
                if let Some(button) = accept.upgrade() {
                    button.emit_clicked();
                }
                glib::Propagation::Stop
            }
            gdk::Key::Escape => {
                panels.close();
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        });
        spin.add_controller(keys);
        let scene = self.0.scene.clone();
        let input = spin.clone();
        self.popup_with(
            "Next sequence number",
            &self.sequence_sample(None),
            false,
            &content,
            move || {
                if let Some(scene) = scene.upgrade() {
                    input.set_value(f64::from(
                        scene
                            .controller()
                            .borrow()
                            .session()
                            .style(Kind::Sequence)
                            .style
                            .sequence
                            .index,
                    ));
                    input.grab_focus();
                    input.select_region(0, -1);
                }
            },
            || {},
        );
        let scene = self.0.scene.clone();
        let panels = self.0.panels.clone();
        apply.connect_clicked(move |_| {
            spin.update();
            if let Some(scene) = scene.upgrade() {
                scene.dispatch(Action::SequenceIndex(spin.value_as_int() as u32));
            }
            panels.close();
        });
        let scene = self.0.scene.clone();
        let panels = self.0.panels.clone();
        reset.connect_clicked(move |_| {
            if let Some(scene) = scene.upgrade() {
                scene.dispatch(Action::SequenceIndex(1));
            }
            panels.close();
        });
        let steps = gtk::Box::new(gtk::Orientation::Vertical, 0);
        steps.set_valign(gtk::Align::Center);
        for (delta, icon, label) in [
            (1, "pan-up-symbolic", "Increase next number (Z)"),
            (-1, "pan-down-symbolic", "Decrease next number (X)"),
        ] {
            let step = gtk::Button::builder()
                .focusable(false)
                .tooltip_text(t(label))
                .child(&gtk::Image::builder().icon_name(icon).pixel_size(8).build())
                .build();
            step.add_css_class("clivuu-sequence-step");
            let scene = self.0.scene.clone();
            step.connect_clicked(move |_| {
                if let Some(scene) = scene.upgrade() {
                    scene.dispatch(Action::SequenceNext(delta));
                }
            });
            steps.append(&step);
        }
        self.0.row.append(&steps);
    }
    fn sequence_shape(&self) {
        let grid = gtk::Grid::builder()
            .row_spacing(3)
            .column_spacing(3)
            .build();
        for (i, shape) in Shape::ALL.into_iter().enumerate() {
            let button = gtk::ToggleButton::builder()
                .child(&self.sequence_sample(Some(shape)))
                .focusable(false)
                .tooltip_text(t(shape.label()))
                .build();
            button.set_size_request(42, 36);
            grid.attach(&button, i as i32 % 3, i as i32 / 3, 1, 1);
            let weak = Rc::downgrade(&self.0);
            button.connect_clicked(move |_| {
                if let Some(inner) = weak.upgrade() {
                    let this = Self(inner);
                    this.patch(StylePatch {
                        sequence: Some(Box::new(Patch {
                            shape: Some(shape),
                            ..Default::default()
                        })),
                        ..Default::default()
                    });
                    this.0.panels.close();
                }
            });
            self.update(move |m, _| button.set_active(m.style.sequence.shape == shape));
        }
        let sample = Canvas::new(|_| {});
        sample.set_size_request(24, 24);
        sample.set_valign(gtk::Align::Center);
        let active_style = Rc::new(RefCell::new(clivuu_core::annotation::Style::for_kind(
            Kind::Sequence,
            1.,
        )));
        let style_ref = active_style.clone();
        let weak = sample.downgrade();
        sample.set_painter(move |snapshot| {
            let Some(sample) = weak.upgrade() else { return };
            let mut m = Mark::new(Kind::Sequence, [12., 12.], 1.);
            m.style = style_ref.borrow().clone();
            m.style.width = 6.;
            m.style.sequence.index = 1;
            render::sequence_badge(snapshot, &m, &sample.pango_context());
        });
        let picture = sample.clone();
        self.update(move |m, _| {
            if *active_style.borrow() != m.style {
                *active_style.borrow_mut() = m.style.clone();
                picture.queue_draw();
            }
        });
        self.popup("Badge shape", &sample, true, &grid);
    }
    pub(super) fn sequence_controls(&self) {
        self.sequence_next();
        self.sequence_shape();
        self.separator();
        let sample = gtk::Label::new(None);
        self.update({
            let sample = sample.clone();
            move |m, _| sample.set_label(m.style.sequence.format.sample())
        });
        let formats = gtk::Box::new(gtk::Orientation::Vertical, 2);
        self.popup("Numbering format", &sample, true, &formats);
        for format in Format::ALL {
            let button = gtk::Button::builder()
                .label(format.sample())
                .focusable(false)
                .build();
            formats.append(&button);
            let weak = Rc::downgrade(&self.0);
            button.connect_clicked(move |_| {
                if let Some(inner) = weak.upgrade() {
                    let this = Self(inner);
                    this.patch(StylePatch {
                        sequence: Some(Box::new(Patch {
                            format: Some(format),
                            ..Default::default()
                        })),
                        ..Default::default()
                    });
                    this.0.panels.close();
                }
            });
            self.update(move |m, _| {
                if m.style.sequence.format == format {
                    button.add_css_class("selected");
                } else {
                    button.remove_css_class("selected");
                }
            });
        }
        self.separator();
        let Some(scene) = self.0.scene.upgrade() else {
            return;
        };
        let scale = scene.controller().borrow().session().stroke_scale();
        let range = Kind::Sequence.stroke_range(scale);
        self.spin_popup(
            SpinKind::MarkerSize,
            "Marker size (image pixels)",
            range,
            |m, _| m.style.width,
            |v| StylePatch {
                width: Some(v),
                ..Default::default()
            },
        );
        self.sequence_foreground();
    }
    fn sequence_foreground(&self) {
        let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
        let automatic = gtk::CheckButton::with_label(&t("Automatic"));
        content.append(&automatic);
        let weak = Rc::downgrade(&self.0);
        let panels = self.0.panels.clone();
        let picker = super::super::color::ColorPanel::new(
            clivuu_core::annotation::color::ColorDraft::parse("#ffffff", 255)
                .expect("built-in fallback color"),
            self.0.color_services.clone(),
            false,
            move |patch| {
                if let Some(inner) = weak.upgrade() {
                    Self(inner).patch(StylePatch {
                        sequence: Some(Box::new(Patch {
                            text_color: patch
                                .color
                                .map(|rgb| format!("{rgb}{:02x}", patch.alpha.unwrap_or(255))),
                            ..Default::default()
                        })),
                        ..Default::default()
                    });
                }
            },
            move || panels.close(),
        );
        content.append(picker.widget());
        let scene = self.0.scene.clone();
        let open_picker = picker.clone();
        let close_picker = picker;
        let button = self.popup_with(
            "Number color",
            &icons::icon("text"),
            true,
            &content,
            move || {
                if let Some(scene) = scene.upgrade()
                    && let Some(m) = scene.controller().borrow().appearance()
                    && let Some(draft) = clivuu_core::annotation::color::ColorDraft::parse(
                        m.template
                            .style
                            .sequence
                            .text_color
                            .as_deref()
                            .unwrap_or("#ffffff"),
                        255,
                    )
                {
                    open_picker.reset(draft);
                }
            },
            move || close_picker.dismiss(),
        );
        self.update(move |m, _| {
            let enabled = m.style.sequence.shape == Shape::BorderedCircle;
            button.set_sensitive(enabled);
            button.set_tooltip_text(Some(&t(if enabled {
                "Number color"
            } else {
                "Available for bordered circles"
            })));
        });
        let weak = Rc::downgrade(&self.0);
        let scene = self.0.scene.clone();
        automatic.connect_toggled(move |b| {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let this = Self(inner);
            if this.0.refreshing.get() {
                return;
            }
            if b.is_active() {
                this.patch(StylePatch {
                    sequence: Some(Box::new(Patch {
                        text_color: Some(String::new()),
                        ..Default::default()
                    })),
                    ..Default::default()
                });
            } else if let Some(scene) = scene.upgrade()
                && scene
                    .controller()
                    .borrow()
                    .appearance()
                    .is_some_and(|m| m.template.style.sequence.text_color.is_none())
            {
                this.patch(StylePatch {
                    sequence: Some(Box::new(Patch {
                        text_color: Some("#ffffffff".into()),
                        ..Default::default()
                    })),
                    ..Default::default()
                });
            }
        });
        self.update(move |m, _| automatic.set_active(m.style.sequence.text_color.is_none()));
    }
}

//! Mutter's logical layout is independent of the selected GTK display backend.
//! On other desktops GDK is authoritative. No screen recording is opened here.
use crate::platform::{portal::Results, surfaces::capture::gdk_outputs};
use clivuu_core::{annotation::Bounds, capture::Output, i18n::t};
use gtk::{gdk, gio, glib};
pub async fn outputs(
    connection: &gio::DBusConnection,
    display: &gdk::Display,
    cancel: &gio::Cancellable,
) -> Result<Vec<Output>, String> {
    if !std::env::var("XDG_CURRENT_DESKTOP")
        .is_ok_and(|s| s.split(':').any(|s| s.eq_ignore_ascii_case("GNOME")))
    {
        return Ok(gdk_outputs(display));
    }
    mutter_outputs(connection, cancel).await
}
pub async fn mutter_outputs(
    connection: &gio::DBusConnection,
    cancel: &gio::Cancellable,
) -> Result<Vec<Output>, String> {
    let reply = gio::CancellableFuture::new(
        connection.call_future(
            Some("org.gnome.Mutter.DisplayConfig"),
            "/org/gnome/Mutter/DisplayConfig",
            "org.gnome.Mutter.DisplayConfig",
            "GetCurrentState",
            None,
            None,
            gio::DBusCallFlags::NO_AUTO_START,
            5000,
        ),
        cancel.clone(),
    )
    .await
    .map_err(|_| t("Operation cancelled"))?
    .map_err(|e| e.to_string())?;
    parse(&reply)
}
fn parse(state: &glib::Variant) -> Result<Vec<Output>, String> {
    let invalid = || t("The captured image does not match the monitor layout");
    if state.n_children() != 4 {
        return Err(invalid());
    }
    let physical = state.child_value(1);
    let logical = state.child_value(2);
    let properties = state.child_value(3).get::<Results>().ok_or_else(invalid)?;
    let physical_layout = properties.get("layout-mode").and_then(|v| v.get::<u32>()) == Some(2);
    if logical.n_children() == 0 || logical.n_children() > 16 {
        return Err(invalid());
    }
    let mut outputs = vec![];
    for i in 0..logical.n_children() {
        let (x, y, scale, transform, primary, monitors, _) = logical
            .child_value(i)
            .get::<(
                i32,
                i32,
                f64,
                u32,
                bool,
                Vec<(String, String, String, String)>,
                Results,
            )>()
            .ok_or_else(invalid)?;
        if !scale.is_finite() || scale <= 0. {
            return Err(invalid());
        }
        let connector = &monitors.first().ok_or_else(invalid)?.0;
        let divisor = if physical_layout { 1. } else { scale };
        let mut dimensions = None;
        for j in 0..physical.n_children() {
            let monitor = physical.child_value(j);
            if monitor.n_children() != 3 {
                return Err(invalid());
            }
            let spec = monitor
                .child_value(0)
                .get::<(String, String, String, String)>()
                .ok_or_else(invalid)?;
            if &spec.0 != connector {
                continue;
            }
            let modes = monitor.child_value(1);
            for k in 0..modes.n_children() {
                let mode = modes.child_value(k);
                if mode.n_children() != 7 {
                    return Err(invalid());
                }
                let props = mode.child_value(6).get::<Results>().ok_or_else(invalid)?;
                if props.get("is-current").and_then(|v| v.get::<bool>()) == Some(true)
                    || props
                        .get("flags")
                        .and_then(|v| v.get::<u32>())
                        .is_some_and(|f| f & 2 != 0)
                {
                    let w = mode.child_value(1).get::<i32>().ok_or_else(invalid)?;
                    let h = mode.child_value(2).get::<i32>().ok_or_else(invalid)?;
                    dimensions = Some(((w as f64 / divisor).round(), (h as f64 / divisor).round()));
                    break;
                }
            }
        }
        let (mut width, mut height) = dimensions.ok_or_else(invalid)?;
        if transform % 2 != 0 {
            std::mem::swap(&mut width, &mut height);
        }
        outputs.push((
            primary,
            Output {
                rect: Bounds {
                    x: x as f64,
                    y: y as f64,
                    width,
                    height,
                },
                scale: divisor,
            },
        ));
    }
    outputs.sort_by_key(|(primary, _)| !primary);
    let outputs: Vec<_> = outputs.into_iter().map(|(_, o)| o).collect();
    clivuu_core::capture::Selection::new(outputs.clone(), vec![], None).ok_or_else(invalid)?;
    Ok(outputs)
}
#[cfg(test)]
mod tests {
    use super::*;
    use gtk::prelude::ToVariant;
    #[test]
    fn logical_layout_respects_scale_rotation_and_negative_origins() {
        let spec = (
            "DP-1".to_string(),
            "vendor".to_string(),
            "model".to_string(),
            "serial".to_string(),
        );
        let mode = (
            "mode".to_string(),
            2880i32,
            1800i32,
            60f64,
            1.5f64,
            vec![1f64, 1.5],
            Results::from_iter([("is-current".into(), true.to_variant())]),
        );
        let state = (
            1u32,
            vec![(spec.clone(), vec![mode], Results::new())],
            vec![(
                -1200i32,
                0i32,
                1.5f64,
                1u32,
                true,
                vec![spec],
                Results::new(),
            )],
            Results::new(),
        )
            .to_variant();
        let layout = parse(&state).unwrap();
        assert_eq!(
            layout[0].rect,
            Bounds {
                x: -1200.,
                y: 0.,
                width: 1200.,
                height: 1920.
            }
        );
        assert_eq!(layout[0].scale, 1.5);
    }
}

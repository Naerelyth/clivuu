//! Compositor-supplied regions only. Never infer niri pixel origins from column
//! indexes, or tie the choice of capture API to the type of the pin surface.
use crate::infrastructure::process;
use clivuu_core::{annotation::Bounds, capture::Output};
use gtk::{gdk, gio, prelude::*};
use serde_json::Value;
#[derive(Clone, Copy)]
pub struct Window {
    pub rect: Bounds,
    pub focused: bool,
}
fn rect(value: &Value) -> Option<Bounds> {
    let r = Bounds {
        x: value["x"].as_f64()?,
        y: value["y"].as_f64()?,
        width: value["width"].as_f64()?,
        height: value["height"].as_f64()?,
    };
    [r.x, r.y, r.width, r.height]
        .iter()
        .all(|v| v.is_finite())
        .then_some(r)
        .filter(|r| r.width > 0. && r.height > 0.)
}
async fn json(argv: &[&str], cancel: &gio::Cancellable) -> Result<Value, String> {
    serde_json::from_str(&process::output(argv, cancel).await?).map_err(|e| e.to_string())
}
pub async fn query(
    display: &gdk::Display,
    outputs: &[Output],
    cancel: &gio::Cancellable,
) -> Result<Vec<Window>, String> {
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
        let clients = json(&["hyprctl", "-j", "clients"], cancel).await?;
        let monitors = json(&["hyprctl", "-j", "monitors"], cancel).await?;
        let visible: Vec<_> = monitors
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|m| {
                [
                    m["activeWorkspace"]["id"].clone(),
                    m["specialWorkspace"]["id"].clone(),
                ]
            })
            .filter(|v| v.as_i64().is_some_and(|n| n != 0))
            .collect();
        return Ok(clients.as_array().into_iter().flatten().filter(|w|w["mapped"].as_bool()!=Some(false)&&w["hidden"].as_bool()!=Some(true)&&visible.contains(&w["workspace"]["id"])).filter_map(|w|rect(&serde_json::json!({"x":w["at"][0],"y":w["at"][1],"width":w["size"][0],"height":w["size"][1]})).map(|rect|Window{rect,focused:w["focusHistoryID"].as_i64()==Some(0)})).collect());
    }
    if std::env::var_os("NIRI_SOCKET").is_some() {
        let windows = json(&["niri", "msg", "--json", "windows"], cancel).await?;
        let workspaces = json(&["niri", "msg", "--json", "workspaces"], cancel).await?;
        let monitors = display.monitors();
        let connectors: Vec<_> = (0..monitors.n_items())
            .filter_map(|i| {
                monitors
                    .item(i)
                    .and_downcast::<gdk::Monitor>()
                    .and_then(|m| Some((m.connector()?.to_string(), outputs.get(i as usize)?.rect)))
            })
            .collect();
        return Ok(windows.as_array().into_iter().flatten().filter_map(|w|{
   let ws=workspaces.as_array()?.iter().find(|ws|ws["is_active"].as_bool()==Some(true)&&ws["id"]==w["workspace_id"])?;
   let output=connectors.iter().find(|(name,_)|Some(name.as_str())==ws["output"].as_str())?.1;let layout=&w["layout"];
   let x=layout["tile_pos_in_workspace_view"][0].as_f64()?;let y=layout["tile_pos_in_workspace_view"][1].as_f64()?;
   let rect=rect(&serde_json::json!({"x":(output.x+x+layout["window_offset_in_tile"][0].as_f64().unwrap_or(0.)).round(),"y":(output.y+y+layout["window_offset_in_tile"][1].as_f64().unwrap_or(0.)).round(),"width":layout["window_size"][0],"height":layout["window_size"][1]}))?;
   Some(Window{rect,focused:w["is_focused"].as_bool()==Some(true)})
  }).collect());
    }
    Ok(vec![])
}
/// Availability describes installed native compositor providers, not Shell code.
pub fn available() -> bool {
    std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some()
        || std::env::var_os("NIRI_SOCKET").is_some()
}

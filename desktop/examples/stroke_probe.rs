//! Render the real subbar indicator and vector cursor through their actual
//! host mappings, before/after wheel edits. No replica of either renderer.
use clivuu_core::annotation::{Bounds, StylePatch};
use clivuu_desktop::{
    editor::{
        Action, EditorController, ImageMapping, Tool, cursor::ToolCursor, scene::EditorDocument,
        view::EditorView,
    },
    ui::{color::ColorServices, viewport::Viewport},
};
use gtk::{gdk, glib, graphene, prelude::*};
use serde_json::json;
use std::{path::PathBuf, time::Duration};
fn find(root: &gtk::Widget) -> Option<gtk::Widget> {
    if root.widget_name() == "stroke-width-preview" {
        return Some(root.clone());
    }
    let mut child = root.first_child();
    while let Some(w) = child {
        child = w.next_sibling();
        if let Some(w) = find(&w) {
            return Some(w);
        }
    }
    None
}
fn bounds(window: &gtk::Window, node: &gtk::gsk::RenderNode, width: i32, height: i32) -> [i32; 2] {
    let texture = window.renderer().unwrap().render_texture(
        node,
        Some(&graphene::Rect::new(0., 0., width as f32, height as f32)),
    );
    let mut bytes = vec![0; width as usize * height as usize * 4];
    texture.download(&mut bytes, width as usize * 4);
    let (mut x0, mut y0, mut x1, mut y1) = (width, height, -1, -1);
    for y in 0..height {
        for x in 0..width {
            if bytes[((y * width + x) * 4 + 3) as usize] > 32 {
                x0 = x0.min(x);
                x1 = x1.max(x);
                y0 = y0.min(y);
                y1 = y1.max(y);
            }
        }
    }
    [x1 - x0 + 1, y1 - y0 + 1]
}
fn main() {
    adw::init().unwrap();
    let context = glib::MainContext::default();
    context.block_on(async {
    let directory=PathBuf::from(std::env::var("CLIVUU_TEST_DIR").unwrap());let mut results=vec![];
    let window=gtk::Window::builder().title("Clivuu stroke feedback probe").default_width(1200).default_height(750).build();
    let root=Viewport::new();window.set_child(Some(&root));window.present();glib::timeout_future(Duration::from_millis(150)).await;
    let texture=gdk::MemoryTexture::new(200,160,gdk::MemoryFormat::R8g8b8a8,&glib::Bytes::from_owned(vec![255u8;200*160*4]),800).upcast::<gdk::Texture>();
    for pin in [false,true]{
      let mapping=|scale| ImageMapping{display:Bounds{x:80.,y:100.,width:200.*scale,height:160.*scale},document:[200.,160.]};
      let document=EditorDocument::new(EditorController::new(mapping(1.)));
      let view=EditorView::new(&root,document.clone(),&texture,Bounds{x:0.,y:0.,width:1200.,height:750.},[0.,0.],pin,ColorServices::default(),|_|{});
      for scale in [0.5,0.8,1.,1.25,2.] {
        document.set_mapping(mapping(scale));view.set_selection(mapping(scale).display,true);
        for tool in [Tool::Pen,Tool::Highlight,Tool::Mosaic,Tool::Eraser,Tool::Arrow]{
          document.dispatch(Action::Tool(tool));document.dispatch(Action::Style(StylePatch{width:Some(4.),..Default::default()}));
          for delta in [-1.,-1.,1.,1.] {
            document.dispatch(Action::Wheel(delta));view.options().refresh();view.layout();glib::timeout_future(Duration::from_millis(80)).await;
            let indicator=find(view.options().row().upcast_ref()).unwrap();let w=indicator.width();let h=indicator.height();
            let mut rendered=None;
            for _ in 0..20 {
                let snapshot=gtk::Snapshot::new();gtk::WidgetPaintable::new(Some(&indicator)).snapshot(&snapshot,w as f64,h as f64);
                rendered=snapshot.to_node();
                if rendered.is_some(){break;}
                glib::timeout_future(Duration::from_millis(30)).await;
            }
            let icon=bounds(&window,&rendered.unwrap_or_else(||panic!("indicator: pin={pin}, tool={tool:?}, scale={scale}, mapped={}, visible={}, size={w}x{h}",indicator.is_mapped(),indicator.is_visible())),w,h);
            let cursor=ToolCursor::for_editor(&document.borrow(),[140.,140.]).unwrap();let snapshot=gtk::Snapshot::new();snapshot.scale(scale as f32,scale as f32);
            // Exclude arrow rays to compare its central dot with the indicator.
            ToolCursor{crosshair:false,..cursor}.snapshot(&snapshot,[16./scale,16./scale],scale);
            let dot=bounds(&window,&snapshot.to_node().unwrap(),32,32);
            let width=document.borrow().appearance().unwrap().snapshot().style.width;
            results.push(json!({"name":format!("{} {tool:?} wheel {delta} scale {scale} keeps the real width indicator and cursor footprint equal",if pin{"pin"}else{"capture"}),"passed":(icon[0]-dot[0]).abs()<=1&&(icon[1]-dot[1]).abs()<=1&&dot[0]>0,"width":width,"indicator":icon,"cursor":dot}));
          }
        }
      }
      document.dispatch(Action::Tool(Tool::Select));drop(view);glib::timeout_future(Duration::from_millis(30)).await;
    }
    window.close();std::fs::write(directory.join("stroke-feedback.json"),json!(results).to_string()).unwrap();
 });
}

use clivuu_core::annotation::{
    Kind, Mark,
    sequence::{Format, Shape},
};
use clivuu_desktop::editor::render::{AnnotationNodes, MosaicTextures};
use gtk::{gdk, graphene, prelude::*};
use serde_json::json;
use std::sync::Arc;

pub fn check(window: &gtk::Window) -> Vec<serde_json::Value> {
    let bounds = graphene::Rect::new(0., 0., 120., 80.);
    let context = window.pango_context();
    let render = |mark: &Mark| {
        let mut nodes = AnnotationNodes::default();
        let node = nodes
            .compose(std::slice::from_ref(mark), &context, &MosaicTextures::new())
            .unwrap();
        let reused = nodes
            .compose(std::slice::from_ref(mark), &context, &MosaicTextures::new())
            .unwrap();
        assert_eq!(node.as_ptr(), reused.as_ptr());
        let snapshot = gtk::Snapshot::new();
        snapshot.append_color(&gdk::RGBA::WHITE, &bounds);
        snapshot.append_node(node);
        let texture = window
            .renderer()
            .unwrap()
            .render_texture(snapshot.to_node().unwrap(), Some(&bounds));
        let mut bytes = vec![0; 120 * 80 * 4];
        texture.download(&mut bytes, 120 * 4);
        bytes
    };
    let mut mark = Mark::new(Kind::Sequence, [35., 40.], 1.);
    mark.style.color = "#ff0000".into();
    let mut results = vec![];
    for shape in Shape::ALL {
        mark.style.sequence.shape = shape;
        let mut images = vec![];
        for format in Format::ALL {
            mark.style.sequence.format = format;
            let first = render(&mark);
            mark.style.sequence.index = 2;
            let second = render(&mark);
            mark.style.sequence.index = 1;
            let restored: Mark =
                serde_json::from_value(serde_json::to_value(&mark).unwrap()).unwrap();
            results.push(json!({"name":format!("sequence {shape:?} / {format:?} retains editable labels and identical persisted export pixels"),"passed":first!=second&&first==render(&restored)}));
            images.push(first);
        }
        // Decimal, Roman and Chinese labels have distinct glyphs at index one.
        results.push(json!({"name":format!("sequence {shape:?} renders the selected numbering alphabet"),"passed":images[0]!=images[1]&&images[0]!=images[4]}));
    }
    mark.style.sequence.shape = Shape::FilledCircle;
    mark.style.sequence.format = Format::Decimal;
    let plain = render(&mark);
    mark.points = Arc::new(vec![[35., 40.], [110., 40.]]);
    let leader = render(&mark);
    let at = (40 * 120 + 80) * 4;
    results.push(json!({"name":"sequence drag adds a rendered leader outside the badge, with editable hit geometry","passed":plain!=leader&&leader[at+1]<200&&mark.hit([80.,40.],1.,0.)}));
    mark.points = Arc::new(vec![[35., 40.]]);
    mark.style.sequence.shape = Shape::BorderedCircle;
    mark.style.sequence.text_color = Some("#ff000000".into());
    let hidden_foreground = render(&mark);
    mark.style.sequence.text_color = Some("#00ff00ff".into());
    results.push(json!({"name":"bordered sequence foreground preserves its independent RGBA","passed":hidden_foreground!=render(&mark)}));
    mark.style.sequence.shape = Shape::Text;
    let text = render(&mark);
    mark.style.sequence.text_color = Some("#0000ff00".into());
    results.push(json!({"name":"outline and number-only sequence marks use their main color","passed":text==render(&mark)&&text.iter().any(|b|*b!=255)}));
    results
}

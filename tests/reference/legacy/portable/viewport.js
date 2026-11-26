import Gtk from 'gi://Gtk?version=4.0';
import GObject from 'gi://GObject';
import Graphene from 'gi://Graphene';

// Desktop children may cross output edges. Their positions and natural sizes
// must never enlarge the stationary desktop surface or rescale its contents.
// GtkWidget delegates measurement to its layout manager when one is installed;
// overriding only Gtk.Fixed.vfunc_measure leaves GtkFixedLayout's minimum in
// effect (e.g. a full-size pin plus shadow enlarges the native host).
const ViewportLayout=GObject.registerClass(class ClivuuViewportLayout extends Gtk.FixedLayout {
    vfunc_measure(){return [0,0,-1,-1];}
});
export const Viewport=GObject.registerClass(class ClivuuViewport extends Gtk.Fixed {
    vfunc_snapshot(snapshot){
        if(!this.clipRect){super.vfunc_snapshot(snapshot);return;}
        const r=this.clipRect;
        snapshot.push_clip(new Graphene.Rect({origin:new Graphene.Point({x:r.x,y:r.y}),size:new Graphene.Size({width:r.width,height:r.height})}));
        super.vfunc_snapshot(snapshot);snapshot.pop();
    }
});
// Gtk.Fixed caches its manager during initialization. Install the class default
// before creating instances rather than replacing that cached instance later.
Viewport.set_layout_manager_type(ViewportLayout);

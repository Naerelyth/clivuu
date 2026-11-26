import Gtk from 'gi://Gtk?version=4.0';
import Gdk from 'gi://Gdk?version=4.0';
import GObject from 'gi://GObject';
import {clamp,contains} from '../common/geometry.js';

// Editable popup content shares the desktop host's native surface. IBus can
// then translate the caret to that surface exactly once (no popup origin).
export const InlinePopup=GObject.registerClass({Signals:{closed:{}}},class ClivuuInlinePopup extends Gtk.Frame {
    _init(owner,anchor,child){
        super._init({child,visible:false,halign:Gtk.Align.START,valign:Gtk.Align.START});
        this.owner=owner;this.anchor=anchor;this.add_css_class('clivuu-inline-popover');
        this.bindings=[];
        anchor.connect('toggled',()=>anchor.active?this.popup():this.popdown());
    }
    popup(){
        if(this.visible)return;
        const view=this.owner.overlay.views.find(v=>this.anchor.get_root()===v.window);
        if(!view)return;
        view.fixed.put(this,0,0);this.view=view;this.visible=true;this.anchor.active=true;
        const outside=new Gtk.EventControllerLegacy({propagation_phase:Gtk.PropagationPhase.CAPTURE});
        outside.connect('event',(_c,event)=>{
            if(event.get_event_type()!==Gdk.EventType.BUTTON_PRESS)return false;
            const [valid,x,y]=event.get_position();if(!valid)return false;
            const picked=view.fixed.pick(x,y,Gtk.PickFlags.DEFAULT);
            if(picked&&[this,this.anchor].some(widget=>picked===widget||picked.is_ancestor(widget)))return false;
            this.popdown();return false;
        });view.fixed.add_controller(outside);
        const inactive=view.window.connect('notify::is-active',()=>{if(!view.window.is_active)this.popdown();});
        this.bindings=[()=>view.fixed.remove_controller(outside),()=>view.window.disconnect(inactive)];
        this.reposition();this.owner._update();
    }
    reposition(){
        if(!this.visible||!this.view)return;
        const {view}=this,[ok,anchor]=this.anchor.compute_bounds(view.fixed);
        if(!ok)return;
        const margin=8,availableWidth=Math.max(1,view.rect.width-2*margin),availableHeight=Math.max(1,view.rect.height-2*margin);
        this.set_size_request(-1,-1);
        const [,naturalWidth]=this.measure(Gtk.Orientation.HORIZONTAL,-1),width=Math.min(availableWidth,naturalWidth);
        const [,naturalHeight]=this.measure(Gtk.Orientation.VERTICAL,width),height=Math.min(availableHeight,naturalHeight);
        const below=anchor.get_y()+anchor.get_height()+4,above=anchor.get_y()-height-4;
        const x=clamp(anchor.get_x(),margin,view.rect.width-width-margin);
        const y=clamp(below+height<=view.rect.height-margin?below:above,margin,view.rect.height-height-margin);
        this.set_size_request(width,height);view.fixed.move(this,x,y);
        this.rect={x:x+view.rect.x,y:y+view.rect.y,width,height};
        this.owner.embedded?.pins._inputRegion(view.surface);
    }
    containsPoint(x,y){return this.visible&&this.rect&&contains(this.rect,x,y);}
    popdown(){
        if(!this.visible)return;
        this.visible=false;this.anchor.active=false;
        for(const dispose of this.bindings)dispose();this.bindings=[];
        const view=this.view;this.view=null;this.rect=null;this.get_parent()?.remove(this);
        this.emit('closed');if(view?.surface)this.owner.embedded?.pins._inputRegion(view.surface);
    }
    dispose(){this.popdown();}
});

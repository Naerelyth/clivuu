import Gtk from 'gi://Gtk?version=4.0';
import Gdk from 'gi://Gdk?version=4.0';
import Gsk from 'gi://Gsk?version=4.0';
import Graphene from 'gi://Graphene';
import GLib from 'gi://GLib';
import GObject from 'gi://GObject';
import {mapPoint,transform} from '../common/annotation-model.js';
import {Viewport} from './viewport.js';
import {drawTextCaret} from '../common/annotation-render.js';
import {textPixelSize,textOutlineColor} from '../common/text-style.js';

const FeedbackTextView=GObject.registerClass(class ClivuuFeedbackTextView extends Gtk.TextView {
    vfunc_snapshot(snapshot){
        super.vfunc_snapshot(snapshot);
        if(!this.has_focus||!this.caretOn||this.buffer.get_has_selection())return;
        const [bufferRect]=this.get_cursor_locations(null),[x,y]=this.buffer_to_window_coords(Gtk.TextWindowType.WIDGET,bufferRect.x,bufferRect.y);
        const rect={x,y,height:bufferRect.height},width=1/Math.max(.01,this.caretScale??1);
        if(rect.height<=0)return;
        const region=new Graphene.Rect({origin:new Graphene.Point({x:rect.x-width,y:rect.y}),size:new Graphene.Size({width:2*width,height:rect.height})});
        const ctx=snapshot.append_cairo(region);drawTextCaret(ctx,rect,this.caretScale??1);ctx.$dispose();
    }
});

const TextViewport=GObject.registerClass(class ClivuuTextViewport extends Viewport {
    vfunc_contains(x,y){
        if(this.textTarget?.(x,y))return false;
        const child=this.get_first_child();if(!child)return false;
        const [ok,box]=child.compute_bounds(this);
        return ok&&x>=box.get_x()&&x<box.get_x()+box.get_width()&&y>=box.get_y()&&y<box.get_y()+box.get_height();
    }
});

let serial=0;
// A native IME-capable text document on the existing capture/pin surfaces.
// The shared editor owns the draft, history and geometry; this layer only
// supplies GTK input and positions it in each output's coordinate system.
export class InlineText {
    constructor(owner){this.owner=owner;this.views=[];this.className=`clivuu-inline-${++serial}`;}
    begin(point,index=-1){
        this.owner._commitText();
        if(!this.owner.editor.beginText(point,index))return;
        this.buffer=new Gtk.TextBuffer();this.syncing=true;
        this.buffer.text=this.owner.editor.current.text;
        this.syncing=false;
        this.changed=this.buffer.connect('changed',()=>{
            if(this.syncing)return;
            this.pending=true;
            if(!this.idle)this.idle=GLib.idle_add(GLib.PRIORITY_DEFAULT,()=>{this.idle=0;this.owner._update();return GLib.SOURCE_REMOVE;});
        });
        this.actionEnd=this.buffer.connect('end-user-action',()=>{if(this.pending)this.owner._update();});
        for(const view of this.owner.overlay.views){
            const widget=new FeedbackTextView({buffer:this.buffer,wrap_mode:Gtk.WrapMode.NONE,
                accepts_tab:false,left_margin:0,right_margin:0,top_margin:0,bottom_margin:0,
                pixels_above_lines:0,pixels_below_lines:0,hexpand:false,vexpand:false});
            const wheel=new Gtk.EventControllerScroll({flags:Gtk.EventControllerScrollFlags.VERTICAL,propagation_phase:Gtk.PropagationPhase.CAPTURE});
            wheel.connect('scroll',(_controller,_dx,dy)=>{this.flush();return this.owner._scroll(dy);});widget.add_controller(wheel);
            widget.connect('preedit-changed',(_widget,value)=>{
                this.preeditText=value;this.flush();const editor=this.owner.editor;if(!editor.textEdit)return;
                const offset=this.buffer.get_iter_at_mark(this.buffer.get_insert()).get_offset(),chars=Array.from(this.buffer.text),composed=chars.slice(0,offset).join('')+value+chars.slice(offset).join('');
                editor.setTextPreedit(value?editor.decorate({...editor.current,text:composed}).textSize:null);this.owner._update();
            });
            const clip=new TextViewport({overflow:Gtk.Overflow.HIDDEN});clip.put(widget,0,0);
            widget.add_css_class(this.className);view.fixed.put(clip,0,0);
            let imageLayer=view.drawing;
            while(imageLayer.get_parent()&&imageLayer.get_parent()!==view.fixed)imageLayer=imageLayer.get_parent();
            clip.insert_after(view.fixed,imageLayer);
            // Capture geometry gestures before TextView's own selection drag.
            // Returning false keeps ordinary caret/IME input native.
            const geometry=new Gtk.EventControllerLegacy({propagation_phase:Gtk.PropagationPhase.CAPTURE});
            geometry.connect('event',(_controller,event)=>{
                if(!this.owner.editor.textEdit)return false;
                const type=event.get_event_type(),[valid,x,y]=event.get_position();if(!valid)return false;
                const world=[x+view.rect.x,y+view.rect.y],pin=this.owner.embedded,
                    r=pin?pin.pins._rect(pin.pin):null,
                    point=r?[(world[0]-r.x)*this.owner.limit.width/r.width+this.owner.limit.x,(world[1]-r.y)*this.owner.limit.height/r.height+this.owner.limit.y]:world;
                if(type===Gdk.EventType.MOTION_NOTIFY){this.owner.pointer=point;this.owner._queueUpdate();}
                if(type===Gdk.EventType.BUTTON_PRESS&&event.get_button()===1&&this.owner.editor.textTarget(point,this.owner.scale??1)){
                    const picked=view.fixed.pick(x,y,Gtk.PickFlags.DEFAULT);
                    if(picked&&[this.owner.toolbar,this.owner.options.widget,...(this.owner.options.popupWidgets?.()??[])].some(panel=>picked===panel||picked.is_ancestor(panel)))return false;
                    this.owner._press(point);return true;
                }
                if(this.owner.editor.edit?.text){
                    if(type===Gdk.EventType.MOTION_NOTIFY){this.owner._motion(point,Boolean(event.get_modifier_state()&Gdk.ModifierType.SHIFT_MASK));return true;}
                    if(type===Gdk.EventType.BUTTON_RELEASE&&event.get_button()===1){this.owner.editor.end();this.owner._update();return true;}
                }
                return false;
            });
            view.fixed.add_controller(geometry);
            this.views.push({view,widget,clip,geometry});
        }
        this.provider=new Gtk.CssProvider();
        Gtk.StyleContext.add_provider_for_display(Gdk.Display.get_default(),this.provider,Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION+1);
        this.layout();
        const position=this._position(),chosen=this.views.find(({view})=>position.x>=view.rect.x&&position.x<view.rect.x+view.rect.width&&position.y>=view.rect.y&&position.y<view.rect.y+view.rect.height)??this.views[0];
        this.focus=chosen.widget;this.owner.window=chosen.view.window;
        chosen.widget.grab_focus();this.buffer.place_cursor(this.buffer.get_end_iter());
        this.owner._update();
        const repaint=()=>{for(const {widget} of this.views)widget.queue_draw();};
        this.cursorSignal=this.buffer.connect('mark-set',()=>{for(const {widget} of this.views)widget.caretOn=true;repaint();});
        for(const {widget} of this.views)widget.caretOn=true;
        this.caretTimer=GLib.timeout_add(GLib.PRIORITY_DEFAULT,500,()=>{for(const {widget} of this.views)widget.caretOn=!widget.caretOn;repaint();return GLib.SOURCE_CONTINUE;});
    }
    _position(){
        const {owner}=this,mark=owner.editor.controlMark,[x,y]=mapPoint(mark,mark.points[0],owner.editor.unit),t=transform(mark);
        if(!owner.embedded)return {x,y,sx:t.sx,sy:t.sy};
        const r=owner.embedded.pins._rect(owner.embedded.pin),zx=r.width/owner.limit.width,zy=r.height/owner.limit.height;
        return {x:r.x+(x-owner.limit.x)*zx,y:r.y+(y-owner.limit.y)*zy,sx:t.sx*zx,sy:t.sy*zy};
    }
    layout(){
        if(!this.buffer||!this.owner.editor.textEdit)return;
        const {owner}=this;
        if(this.preeditText&&owner.editor.textEdit.preedit){
            const offset=this.buffer.get_iter_at_mark(this.buffer.get_insert()).get_offset(),chars=Array.from(this.buffer.text),text=chars.slice(0,offset).join('')+this.preeditText+chars.slice(offset).join('');
            owner.editor.setTextPreedit(owner.editor.decorate({...owner.editor.current,text}).textSize);
        }
        const mark=owner.editor.controlMark,p=this._position(),unit=owner.editor.unit,size=textPixelSize(mark,1);
        const family=JSON.stringify(mark.fontFamily??'Sans'),css=`.${this.className}, .${this.className} text { background:transparent; color:${!owner.editor.textEdit.preedit?'transparent':mark.color}; caret-color:transparent; border:0; border-radius:0; box-shadow:none; padding:0; margin:0; font-family:${family}; font-size:${size}px; font-weight:${mark.bold?'bold':'normal'}; font-style:${mark.italic?'italic':'normal'}; }`;
        if(this.css!==css){this.provider.load_from_string(css);this.css=css;}
        const pin=owner.embedded?.pin,visible=!pin||(!owner.embedded.pins.obscured&&!pin.hidden&&pin.group===owner.embedded.pins.group);
        const crop=owner.embedded?owner.embedded.pins._rect(pin):owner.rect;
        const r=owner.embedded?{x:crop.x+owner.rect.x*crop.width/owner.limit.width,y:crop.y+owner.rect.y*crop.height/owner.limit.height,width:owner.rect.width*crop.width/owner.limit.width,height:owner.rect.height*crop.height/owner.limit.height}:crop;
        for(const {view,widget,clip} of this.views){
            clip.textTarget=(x,y)=>{
                const point=owner.embedded?[(r.x+x-crop.x)*owner.limit.width/crop.width+owner.limit.x,(r.y+y-crop.y)*owner.limit.height/crop.height+owner.limit.y]:[r.x+x,r.y+y];
                return owner.editor.textTarget(point,owner.scale??1);
            };
            clip.visible=visible;widget.caretScale=Math.abs(p.sx/unit);widget.opacity=owner.editor.textEdit.preedit?((mark.alpha??255)/255)*(pin?.opacity??1):1;
            view.fixed.move(clip,r.x-view.rect.x,r.y-view.rect.y);clip.set_size_request(r.width,r.height);
            widget.set_size_request(Math.max(12,(mark.textSize?.[0]??0)*unit+4),Math.max(size*1.3,(mark.textSize?.[1]??0)*unit));
            const matrix=Gsk.Transform.new().translate(new Graphene.Point({x:p.x-r.x,y:p.y-r.y})).rotate(transform(mark).angle*180/Math.PI).scale(p.sx/unit,p.sy/unit);
            clip.set_child_transform(widget,matrix);
        }
    }
    containsFocus(focus){return this.views.some(({widget})=>focus===widget||Boolean(focus?.is_ancestor(widget)));}
    flush(){
        if(!this.pending||!this.buffer)return;
        this.pending=false;this.owner.editor.updateText(this.buffer.text);
        if(this.buffer.text!==this.owner.editor.current.text)this._syncBuffer();
    }
    _syncBuffer(){this.syncing=true;this.buffer.text=this.owner.editor.current.text;this.buffer.place_cursor(this.buffer.get_end_iter());this.syncing=false;}
    history(redo){this.flush();this.owner.editor.textHistory(redo);this._syncBuffer();this.owner._update();}
    newlineAtLineEnd(){const iter=this.buffer.get_iter_at_mark(this.buffer.get_insert());iter.forward_to_line_end();this.buffer.insert(iter,'\n',-1);this.buffer.place_cursor(iter);}
    destroy(){
        if(!this.buffer)return;
        this.buffer.disconnect(this.changed);this.buffer.disconnect(this.actionEnd);this.buffer.disconnect(this.cursorSignal);
        if(this.caretTimer){GLib.source_remove(this.caretTimer);this.caretTimer=0;}
        if(this.idle){GLib.source_remove(this.idle);this.idle=0;}this.pending=false;
        for(const {view,widget,clip,geometry} of this.views){view.fixed.remove_controller(geometry);clip.remove(widget);clip.get_parent()?.remove(clip);}
        this.views=[];this.buffer=null;this.focus=null;this.preeditText=null;
        Gtk.StyleContext.remove_provider_for_display(Gdk.Display.get_default(),this.provider);this.provider=null;this.css=null;
    }
}

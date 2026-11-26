import Gtk from 'gi://Gtk?version=4.0';
import {parseColor} from '../common/color-model.js';
export const colorWidgets={
    box:vertical=>new Gtk.Box({orientation:vertical?Gtk.Orientation.VERTICAL:Gtk.Orientation.HORIZONTAL,spacing:4}),
    role:(widget,role)=>{
        widget.add_css_class(`clivuu-color-${role}`);
        if(role==='actions'){widget.halign=Gtk.Align.END;widget.spacing=8;widget.margin_top=4;}
        if(role.endsWith('action')){widget.add_css_class('clivuu-color-action');widget.add_css_class('pill');}
        if(role==='primary-action')widget.add_css_class('suggested-action');
    },
    hint:(widget,text)=>widget.tooltip_text=text,
    add:(parent,child)=>parent.append(child),label:text=>new Gtk.Label({label:text,xalign:0}),
    text:(label,text)=>label.label=text,
    button:(label,action)=>{const b=new Gtk.Button({label});b.connect('clicked',action);return b;},
    number:(max,action)=>{
        const widget=Gtk.SpinButton.new_with_range(0,max,1);widget.width_chars=3;widget.max_width_chars=3;
        widget.connect('value-changed',()=>action(widget.value));
        return {widget,set:(v,force)=>{if(force||!widget.has_focus)widget.value=v;},flush:()=>{
            const text=widget.get_text().trim(),v=Number(text);
            if(!/^\d+$/.test(text)||v>max)return false;
            widget.update();return true;
        }};
    },
    entry:(action,activate)=>{
        const widget=new Gtk.Entry({width_chars:9,max_width_chars:9,max_length:9});let published='';
        const flush=()=>{const text=widget.text.trim();if(text===published)return true;if(action(text)===false)return false;published=text;return true;};
        widget.connect('activate',()=>{if(flush())activate?.();});
        return {widget,flush,set:(v,force)=>{const focus=widget.get_root()?.get_focus();if(force||(focus!==widget&&!focus?.is_ancestor(widget)))widget.text=published=v;}};
    },
    canvas:(width,height,draw,change)=>{
        const c=new Gtk.DrawingArea({content_width:width,content_height:height});c.set_draw_func((_c,ctx,w,h)=>draw(ctx,w,h));
        if(change){let start;const drag=new Gtk.GestureDrag({button:1});const update=(x,y)=>change(Math.max(0,Math.min(c.get_width(),x)),Math.max(0,Math.min(c.get_height(),y)),c.get_width(),c.get_height());drag.connect('drag-begin',(_g,x,y)=>{start=[x,y];update(x,y);});drag.connect('drag-update',(_g,x,y)=>update(start[0]+x,start[1]+y));c.add_controller(drag);}
        return c;
    },redraw:c=>c.queue_draw(),
    swatch:(color,action)=>{const b=new Gtk.Button({width_request:17,height_request:17,focusable:false});b.add_css_class('clivuu-swatch');const c=new Gtk.DrawingArea({content_width:14,content_height:14});b.swatch=color;c.set_draw_func((_c,ctx,w,h)=>{const p=parseColor(b.swatch);ctx.setSourceRGBA(...p.rgb.map(v=>v/255),p.alpha/255);ctx.rectangle(0,0,w,h);ctx.fill();});b.set_child(c);b.connect('clicked',action);return b;},
    swatchColor:(b,color,selected)=>{b.swatch=color;b[selected?'add_css_class':'remove_css_class']('clivuu-swatch-selected');b.get_child().queue_draw();},
};

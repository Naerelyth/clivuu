import Gtk from 'gi://Gtk?version=4.0';
import GObject from 'gi://GObject';
import Gdk from 'gi://Gdk?version=4.0';
import Graphene from 'gi://Graphene';
import {intersect} from '../common/geometry.js';
import {rgb,anchorPoints,drawCaptureFrame} from '../common/capture-options.js';

const box=r=>new Graphene.Rect({origin:new Graphene.Point({x:r.x,y:r.y}),size:new Graphene.Size({width:r.width,height:r.height})});
const color=(hex,alpha=1)=>{const [red,green,blue]=rgb(hex);return new Gdk.RGBA({red,green,blue,alpha});};

// Keep the frozen image as a retained texture. Pointer movement changes small
// render nodes instead of repainting/uploading the entire screenshot in Cairo.
export const CaptureCanvas=GObject.registerClass(class ClivuuCaptureCanvas extends Gtk.Widget {
    vfunc_snapshot(snapshot){this.render?.(snapshot);}
});

export function snapshotCapture(snapshot,selection,viewport){
    const {limit:l,rect:r,capture:o}=selection;
    const local=r=>({...r,x:r.x-viewport.x,y:r.y-viewport.y});
    const fill=(r,c)=>{const clipped=intersect(r,viewport);if(clipped.width>0&&clipped.height>0)snapshot.append_color(c,box(local(clipped)));};
    snapshot.append_texture(selection.texture,box(local(l)));
    const left=Math.max(l.x,Math.min(l.x+l.width,r.x)),right=Math.max(left,Math.min(l.x+l.width,r.x+r.width));
    const top=Math.max(l.y,Math.min(l.y+l.height,r.y)),bottom=Math.max(top,Math.min(l.y+l.height,r.y+r.height));
    const mask=color(o.maskColor,o.maskOpacity);
    for(const [x,y,width,height] of [[l.x,l.y,l.width,top-l.y],[l.x,bottom,l.width,l.y+l.height-bottom],
        [l.x,top,left-l.x,bottom-top],[right,top,l.x+l.width-right,bottom-top]])fill({x,y,width,height},mask);
    const outline=(r,width,c)=>{
        for(const b of [{x:r.x-width,y:r.y-width,width:r.width+2*width,height:width},
            {x:r.x-width,y:r.y+r.height,width:r.width+2*width,height:width},
            {x:r.x-width,y:r.y,width,height:r.height},{x:r.x+r.width,y:r.y,width,height:r.height}])fill(b,c);
    };
    if(!o.disableGuides){
        if(o.crosshair&&selection.pointer){const [x,y]=selection.pointer,c=color(o.borderColor,.5);
            fill({x:l.x,y,width:l.width,height:1},c);fill({x,y:l.y,width:1,height:l.height},c);}
        if(selection.monitors.length>1)for(const monitor of selection.monitors)outline(monitor,.5,new Gdk.RGBA({red:.65,green:.8,blue:1,alpha:.65}));
    }
    outline(r,o.borderWidth,color(o.borderColor));
    if(selection.selected&&o.anchors)for(const [x,y] of anchorPoints(r)){
        const region=intersect({x:x-4,y:y-4,width:8,height:8},viewport);
        if(region.width<=0||region.height<=0)continue;
        const ctx=snapshot.append_cairo(box(local(region)));ctx.translate(-viewport.x,-viewport.y);
        drawCaptureFrame(ctx,r,{...o,borderWidth:0});ctx.$dispose();
    }
    if(selection.marks.length||selection.mark||selection.editor?.current){
        const controls=selection.controlRects?.(1)??[],xs=[r.x,r.x+r.width,...controls.flatMap(b=>[b.x,b.x+b.width])],ys=[r.y,r.y+r.height,...controls.flatMap(b=>[b.y,b.y+b.height])];
        const region=intersect({x:Math.min(...xs),y:Math.min(...ys),width:Math.max(...xs)-Math.min(...xs),height:Math.max(...ys)-Math.min(...ys)},viewport);
        if(region.width>0&&region.height>0){const ctx=snapshot.append_cairo(box(local(region)));
            ctx.translate(-viewport.x,-viewport.y);selection._drawMarks(ctx);ctx.$dispose();}
    }
}

// Controls may extend beyond the image without creating another surface.
export const HandleCanvas=GObject.registerClass(class ClivuuHandleCanvas extends Gtk.Widget {
    vfunc_contains(x,y){return Boolean(this.hit?.(x,y));}
    vfunc_snapshot(snapshot){this.render?.(snapshot);}
});
export function snapshotHandles(snapshot,selection,viewport,screenRect){
    const scale=screenRect.width/selection.limit.width,rects=selection.controlRects(scale);
    if(!rects.length)return;
    const xs=rects.flatMap(r=>[r.x,r.x+r.width]),ys=rects.flatMap(r=>[r.y,r.y+r.height]);
    const region=intersect({x:screenRect.x+(Math.min(...xs)-selection.limit.x)*scale,y:screenRect.y+(Math.min(...ys)-selection.limit.y)*scale,
        width:(Math.max(...xs)-Math.min(...xs))*scale,height:(Math.max(...ys)-Math.min(...ys))*scale},viewport);
    if(region.width<=0||region.height<=0)return;
    const ctx=snapshot.append_cairo(box({...region,x:region.x-viewport.x,y:region.y-viewport.y}));
    ctx.translate(screenRect.x-viewport.x-selection.limit.x*scale,screenRect.y-viewport.y-selection.limit.y*scale);ctx.scale(scale,scale);
    selection._drawControls(ctx);ctx.$dispose();
}

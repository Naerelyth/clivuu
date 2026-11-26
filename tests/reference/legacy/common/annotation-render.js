import {DEFAULT_ACCENT,accentInk} from './toolbar-theme.js';
import Cairo from 'cairo';
import Pango from 'gi://Pango';
import PangoCairo from 'gi://PangoCairo';
import {defaultStyle,markBounds,markControlBounds,markHandles,visibleMarkHandles,mapPoint,transform,isLineMark,isFreehand} from './annotation-model.js';
import {endStyle,lineDashes,mosaicLevel,mosaicEffect,strokeIndicator} from './annotation-options-model.js';
import {textPixelSize,textOutlineColor} from './text-style.js';

export function textLayout(ctx,mark,unit=1){
    const layout=PangoCairo.create_layout(ctx),font=new Pango.FontDescription();
    font.set_family(mark.fontFamily??'Sans');font.set_absolute_size(textPixelSize(mark,unit)*Pango.SCALE);
    font.set_weight(mark.bold?Pango.Weight.BOLD:Pango.Weight.NORMAL);font.set_style(mark.italic?Pango.Style.ITALIC:Pango.Style.NORMAL);
    layout.set_font_description(font);layout.set_text(mark.text??'',-1);return layout;
}
export function measureText(mark,unit=1){
    const image=new Cairo.ImageSurface(Cairo.Format.ARGB32,1,1),ctx=new Cairo.Context(image);
    // Shape once in source pixels; logical-size hinting otherwise changes
    // glyph advances between a scaled capture and its exported image.
    const size=textLayout(ctx,mark,1).get_pixel_size();ctx.$dispose();image.finish();return size.map(v=>v/unit);
}
export function drawStrokeIndicator(ctx,w,h,width,ink='#404040'){
    const indicator=strokeIndicator(width,Math.min(w,h)-4);
    ctx.save();ctx.setSourceRGB(...[1,3,5].map(i=>parseInt(ink.slice(i,i+2),16)/255));
    if(indicator.text){
        const layout=textLayout(ctx,{text:indicator.text,fontFamily:'Sans',fontSize:10,bold:false}),[tw,th]=layout.get_pixel_size();
        ctx.moveTo((w-tw)/2,(h-th)/2);PangoCairo.show_layout(ctx,layout);
    }else{ctx.arc(w/2,h/2,indicator.diameter/2,0,Math.PI*2);ctx.fill();}
    ctx.restore();return indicator;
}
function path(ctx,mark,width,unit){
    const b=markBounds(mark),[x,y]=mark.points[0],[ex,ey]=mark.points.at(-1);
    if(isLineMark(mark)){ctx.moveTo(x,y);ctx.lineTo(ex,ey);return;}
    const shape=['rect','ellipse'].includes(mark.kind)?mark.kind:mark.fill;
    if(shape==='rect'){ctx.rectangle(b.x,b.y,b.width,b.height);return;}
    if(shape==='ellipse'){
        ctx.save();ctx.translate(b.x+b.width/2,b.y+b.height/2);ctx.scale(b.width/2,b.height/2);ctx.arc(0,0,1,0,Math.PI*2);ctx.restore();return;
    }
    if(['mosaic','eraser'].includes(mark.kind)){
        for(let i=0;i<mark.points.length;i++){
            const p=mark.points[i];ctx.newSubPath();ctx.arc(p[0],p[1],width/2,0,Math.PI*2);
            if(i){const a=mark.points[i-1],angle=Math.atan2(p[1]-a[1],p[0]-a[0]),dx=Math.sin(angle)*width/2,dy=-Math.cos(angle)*width/2;
                ctx.moveTo(a[0]+dx,a[1]+dy);ctx.lineTo(p[0]+dx,p[1]+dy);ctx.lineTo(p[0]-dx,p[1]-dy);ctx.lineTo(a[0]-dx,a[1]-dy);ctx.closePath();}
        }return;
    }
    ctx.moveTo(x,y);for(const p of mark.points.slice(1))ctx.lineTo(...p);
}
export function drawArrow(ctx,mark,width,unit=1){
    const a=mark.points[0],b=mark.points.at(-1),length=Math.hypot(b[0]-a[0],b[1]-a[1]);
    if(!length)return;
    const size=Math.min(length*.45,Math.max(width*4,10/unit)),ends=[endStyle(mark,true),endStyle(mark)],angle=Math.atan2(b[1]-a[1],b[0]-a[0]);
    const inset=style=>['circle','circle-outline','diamond','diamond-outline'].includes(style)?size:style.startsWith('triangle')?size*.8:0;
    ctx.setDash(lineDashes(mark.lineStyle,width),0);ctx.moveTo(a[0]+Math.cos(angle)*inset(ends[0]),a[1]+Math.sin(angle)*inset(ends[0]));ctx.lineTo(b[0]-Math.cos(angle)*inset(ends[1]),b[1]-Math.sin(angle)*inset(ends[1]));ctx.stroke();ctx.setDash([],0);
    for(let i=0;i<2;i++){
        const style=ends[i];if(style==='none')continue;
        ctx.save();ctx.translate(...(i?b:a));ctx.rotate(angle+(i?0:Math.PI));ctx.newPath();
        if(style.startsWith('circle'))ctx.arc(-size/2,0,size/2,0,Math.PI*2);
        else if(style.startsWith('diamond')){ctx.moveTo(0,0);ctx.lineTo(-size/2,-size*.4);ctx.lineTo(-size,0);ctx.lineTo(-size/2,size*.4);ctx.closePath();}
        else {ctx.moveTo(-size,size*.5);ctx.lineTo(0,0);ctx.lineTo(-size,-size*.5);if(style!=='open')ctx.closePath();}
        if(style==='open'||style.endsWith('-outline'))ctx.stroke();else ctx.fill();ctx.restore();
    }
}
// Compact previews reserve the caret area and emphasise the edited endpoint.
export function drawLineOption(ctx,w,h,key,value,ink='#404040'){
    const rgb=[1,3,5].map(i=>parseInt(ink.slice(i,i+2),16)/255),y=h/2,left=5,right=w-13;
    ctx.save();ctx.setSourceRGB(...rgb);ctx.setLineWidth(2.2);ctx.setLineCap(Cairo.LineCap.ROUND);
    if(key==='lineStyle'){
        ctx.setLineCap(Cairo.LineCap.BUTT);ctx.setDash(lineDashes(value,.85),0);ctx.moveTo(left,y);ctx.lineTo(right,y);ctx.stroke();ctx.setDash([],0);
    }else{
        drawArrow(ctx,{points:[[left,y],[right,y]],startStyle:key==='startStyle'?value:'none',endStyle:key==='endStyle'?value:'none'},2.2);
        if(value==='none'){
            const x=key==='startStyle'?left:right;ctx.setLineWidth(2.5);ctx.moveTo(x,y-5);ctx.lineTo(x,y+5);ctx.stroke();
        }
    }
    ctx.setSourceRGB(...rgb);ctx.moveTo(w-9,y-2);ctx.lineTo(w-3,y-2);ctx.lineTo(w-6,y+2);ctx.closePath();ctx.fill();ctx.restore();
}
export function drawEffectIcon(ctx,w,h,effect,ink='#404040'){
    const rgb=[1,3,5].map(i=>parseInt(ink.slice(i,i+2),16)/255),size=Math.min(w,h)*.72,x=(w-size)/2,y=(h-size)/2;
    ctx.save();
    if(effect==='blur'){
        for(let r=size/2;r>=1;r--){ctx.setSourceRGBA(...rgb,.075);ctx.arc(w/2,h/2,r,0,Math.PI*2);ctx.fill();}
    }else for(let row=0;row<3;row++)for(let column=0;column<3;column++){
        ctx.setSourceRGBA(...rgb,[.85,.3,.65,.35,.6,.2,.7,.15,.9][row*3+column]);ctx.rectangle(x+column*size/3,y+row*size/3,size/3,size/3);ctx.fill();
    }
    ctx.restore();
}
function mosaicTile(source,block,cache,effect,region){
    const factor=effect==='blur'?Math.max(1,block/3):block;
    const columns=Math.max(1,Math.ceil(source.getWidth()/factor)),rows=Math.max(1,Math.ceil(source.getHeight()/factor));
    const stepX=effect==='blur'?source.getWidth()/columns:factor,stepY=effect==='blur'?source.getHeight()/rows:factor;
    // Work only on the visible mask plus a filter halo. Keep the full-image
    // sampling phase, so crop, movement and DPR never shift the pixel grid.
    const halo=effect==='blur'?Math.ceil(block/factor)+2:2;
    const left=Math.max(0,Math.floor(region.x/stepX)-halo),top=Math.max(0,Math.floor(region.y/stepY)-halo);
    const right=Math.min(columns,Math.ceil((region.x+region.width)/stepX)+halo),bottom=Math.min(rows,Math.ceil((region.y+region.height)/stepY)+halo);
    if(right<=left||bottom<=top)return null;
    const w=right-left,h=bottom-top,key=[effect,block,left,top,w,h].join(':');
    if(cache.has(key))return cache.get(key);
    const tile=new Cairo.ImageSurface(Cairo.Format.ARGB32,w,h),ctx=new Cairo.Context(tile);
    ctx.scale(1/stepX,1/stepY);ctx.translate(-left*stepX,-top*stepY);
    ctx.setSourceSurface(source,0,0);ctx.getSource().setFilter(Cairo.Filter.BEST);ctx.getSource().setExtend(Cairo.Extend.PAD);ctx.paint();ctx.$dispose();
    let result=tile;
    if(effect==='blur'){
        // Bounded, separable Gaussian on a native Cairo downsample; no JS copy
        // or per-pixel loops over the full desktop on every pointer movement.
        const sigma=Math.max(.5,block/factor/2.2),r=Math.ceil(block/factor),weights=Array.from({length:r*2+1},(_,i)=>Math.exp(-((i-r)**2)/(2*sigma*sigma))),sum=weights.reduce((a,b)=>a+b,0);
        for(const axis of [0,1]){
            const next=new Cairo.ImageSurface(Cairo.Format.ARGB32,w,h),c=new Cairo.Context(next);c.setOperator(Cairo.Operator.ADD);
            for(let i=-r;i<=r;i++){c.setSourceSurface(result,axis===0?i:0,axis===1?i:0);c.getSource().setExtend(Cairo.Extend.PAD);c.paintWithAlpha(weights[i+r]/sum);}
            c.$dispose();result.finish();result=next;
        }
    }
    const value={surface:result,x:left*stepX,y:top*stepY,stepX,stepY,bytes:w*h*4};
    let bytes=value.bytes;for(const entry of cache.values())bytes+=entry.bytes;
    while(cache.size&&(cache.size>=12||bytes>16*1024*1024)){
        const oldest=cache.keys().next().value,entry=cache.get(oldest);bytes-=entry.bytes;entry.surface.finish();cache.delete(oldest);
    }
    cache.set(key,value);return value;
}
export function clearMosaicCache(cache){for(const tile of cache?.values()??[])tile.surface.finish();cache?.clear();}
export function drawAnnotations(ctx,marks,{unit=1,source=null,sourceRect=null,cache=new Map()}={}){
    // Erasers affect preceding annotations only. An isolated group preserves
    // the original image in every capture, pin and export rendering path.
    const masked=marks.some(mark=>mark.kind==='eraser');if(masked)ctx.pushGroup();
    for(const value of marks){
        const mark={...defaultStyle(value.kind),...value};
        if(value.kind==='text'&&!value.fontUnit)mark.fontUnit='px';
        if(!mark.points?.length)continue;
        if(mark.kind==='mosaic'){if(!source)continue;mark.effect=mosaicEffect(mark.effect);}
        const t=transform(mark),b=markBounds(mark,unit),cx=b.x+b.width/2,cy=b.y+b.height/2;
        ctx.save();ctx.translate(cx+t.x,cy+t.y);ctx.rotate(t.angle);ctx.scale(t.sx,t.sy);ctx.translate(-cx,-cy);
        ctx.newPath();
        const color=mark.color??'#ff6058',alpha=(mark.alpha??255)/255;
        ctx.setSourceRGBA(...[1,3,5].map(i=>parseInt(color.slice(i,i+2),16)/255),alpha);
        const width=(mark.width??(mark.kind==='highlight'?14:3))/unit/Math.sqrt(t.sx*t.sy);
        ctx.setLineWidth(width);ctx.setLineCap(Cairo.LineCap.ROUND);ctx.setLineJoin(Cairo.LineJoin.ROUND);
        if(mark.kind==='eraser'){
            ctx.setOperator(Cairo.Operator.DEST_OUT);ctx.setSourceRGBA(0,0,0,1);path(ctx,mark,width,unit);ctx.fill();
        }
        else if(mark.kind==='arrow'){
            ctx.pushGroup();ctx.setSourceRGBA(...[1,3,5].map(i=>parseInt(color.slice(i,i+2),16)/255),1);
            drawArrow(ctx,mark,width,unit);ctx.popGroupToSource();ctx.paintWithAlpha(alpha);
        }
        else if(mark.kind==='text'){
            ctx.scale(1/unit,1/unit);const point=mark.points[0].map(v=>v*unit),layout=textLayout(ctx,mark,1),outline=textOutlineColor(mark);
            if(outline){
                // Composite alpha once for both the glyph and its outer stroke.
                ctx.pushGroup();ctx.moveTo(...point);PangoCairo.layout_path(ctx,layout);
                ctx.setSourceRGB(...[1,3,5].map(i=>parseInt(outline.slice(i,i+2),16)/255));
                ctx.setLineWidth(2*(mark.outlineWidth??1)*(mark.fontScale??1));ctx.stroke();
                ctx.setSourceRGB(...[1,3,5].map(i=>parseInt(color.slice(i,i+2),16)/255));
                ctx.moveTo(...point);PangoCairo.show_layout(ctx,layout);ctx.popGroupToSource();ctx.paintWithAlpha(alpha);
            }else{ctx.moveTo(...point);PangoCairo.show_layout(ctx,layout);}
        }
        else {
            path(ctx,mark,width,unit);
            if(['rect','ellipse'].includes(mark.kind)&&mark.fill!=='solid'){
                // Cairo keeps the path across save/restore. Stroke the scaled
                // geometry in the original image coordinate system so an
                // asymmetric resize cannot stretch the pen itself.
                ctx.restore();ctx.save();
                ctx.setSourceRGBA(...[1,3,5].map(i=>parseInt(color.slice(i,i+2),16)/255),alpha);
                ctx.setLineWidth(mark.width/unit);ctx.setLineCap(Cairo.LineCap.ROUND);ctx.setLineJoin(Cairo.LineJoin.ROUND);ctx.stroke();
            }else if(mark.kind==='mosaic'){
                ctx.clip();
                // The mask moves with the object; sampled desktop pixels do not.
                ctx.translate(cx,cy);ctx.scale(1/t.sx,1/t.sy);ctx.rotate(-t.angle);ctx.translate(-cx-t.x,-cy-t.y);
                const r=sourceRect??{x:0,y:0,width:source.getWidth(),height:source.getHeight()},sx=source.getWidth()/r.width,sy=source.getHeight()/r.height;
                const [x1,y1,x2,y2]=ctx.clipExtents(),region={x:(x1-r.x)*sx,y:(y1-r.y)*sy,width:(x2-x1)*sx,height:(y2-y1)*sy};
                const tile=mosaicTile(source,mosaicLevel(mark.effectLevel),cache,mark.effect,region);
                // Integer-sized source-pixel cells: never squeeze the whole
                // grid to fit a non-divisible desktop width or crop.
                if(tile){ctx.translate(r.x+tile.x/sx,r.y+tile.y/sy);ctx.scale(tile.stepX/sx,tile.stepY/sy);ctx.setSourceSurface(tile.surface,0,0);ctx.getSource().setFilter(mark.effect==='blur'?Cairo.Filter.BILINEAR:Cairo.Filter.NEAREST);ctx.paintWithAlpha(alpha);}
            }else if(mark.fill==='solid'||(mark.kind==='highlight'&&['rect','ellipse'].includes(mark.fill)))ctx.fill();
            else ctx.stroke();
        }
        ctx.newPath();ctx.restore();
    }
    if(masked){ctx.popGroupToSource();ctx.paint();}
}
export function drawEraserOption(ctx,w,h,mode,width,ink){
    if(mode==='stroke'){drawStrokeIndicator(ctx,w,h,width,ink);return;}
    ctx.save();ctx.setSourceRGB(...[1,3,5].map(i=>parseInt(ink.slice(i,i+2),16)/255));
    const size=Math.min(w,h)*.66,step=size/8,x=(w-size)/2,y=(h-size)/2;
    for(let row=0;row<8;row++)for(let col=0;col<8;col++)if((row+col)%2===0)ctx.rectangle(x+col*step,y+row*step,step,step);ctx.fill();ctx.restore();
}
// Overlay layers cannot invert pixels in an earlier compositor node. Use a
// fixed one-pixel black/white pattern, visible on either light or dark images.
function drawFeedbackFrame(ctx,mark,unit,screenScale){
    const scale=Math.max(.01,screenScale),b=markControlBounds(mark,unit,scale),corners=[[b.x,b.y],[b.x+b.width,b.y],[b.x+b.width,b.y+b.height],[b.x,b.y+b.height]].map(p=>mapPoint(mark,p,unit));
    ctx.save();ctx.setLineWidth(1/scale);ctx.setLineCap(Cairo.LineCap.BUTT);ctx.setSourceRGB(0,0,0);
    ctx.moveTo(...corners[0]);for(const p of corners.slice(1))ctx.lineTo(...p);ctx.closePath();ctx.strokePreserve();
    ctx.setSourceRGB(1,1,1);ctx.setDash([3/scale,3/scale],0);ctx.stroke();ctx.restore();
}
export function drawTextCaret(ctx,rect,screenScale=1){
    if(!rect||rect.height<=0)return;
    const width=1/Math.max(.01,Math.abs(screenScale));
    ctx.save();ctx.setSourceRGB(0,0,0);ctx.rectangle(rect.x-width,rect.y,width,rect.height);ctx.fill();
    ctx.setSourceRGB(1,1,1);ctx.rectangle(rect.x,rect.y,width,rect.height);ctx.fill();ctx.restore();
}
export function drawDraftBounds(ctx,mark,unit=1,screenScale=1){
    if(mark?.kind==='eraser'&&mark.fill==='rect'&&mark.points.length>=2)drawFeedbackFrame(ctx,mark,unit,screenScale);
}
export function drawElementHover(ctx,mark,unit=1,screenScale=1,color='#ffffff'){
    if(!mark)return;
    if(mark.kind==='text'){drawFeedbackFrame(ctx,mark,unit,screenScale);return;}
    const b=markBounds(mark,unit),corners=[[b.x,b.y],[b.x+b.width,b.y],[b.x+b.width,b.y+b.height],[b.x,b.y+b.height]].map(p=>mapPoint(mark,p,unit));
    ctx.save();ctx.setLineWidth(1/Math.max(.01,screenScale));ctx.setSourceRGB(...[1,3,5].map(i=>parseInt(color.slice(i,i+2),16)/255));
    ctx.setDash([3/screenScale,3/screenScale],0);ctx.moveTo(...corners[0]);for(const p of corners.slice(1))ctx.lineTo(...p);ctx.closePath();ctx.stroke();ctx.restore();
}
export function drawElementSelection(ctx,mark,unit=1,screenScale=1,color='#ffffff',accent=DEFAULT_ACCENT,pointer=null){
    if(!mark)return;
    if(mark.kind==='text'){drawTextSelection(ctx,mark,unit,screenScale,accent,pointer);return;}
    const b=markBounds(mark,unit),corners=[[b.x,b.y],[b.x+b.width,b.y],[b.x+b.width,b.y+b.height],[b.x,b.y+b.height]].map(p=>mapPoint(mark,p,unit));
    ctx.save();ctx.setLineWidth(1/Math.max(.01,screenScale));ctx.setSourceRGB(.9,.92,.95);
    if(['mosaic','eraser'].includes(mark.kind)&&!isFreehand(mark)){
        ctx.setDash([4/Math.max(.01,screenScale),4/Math.max(.01,screenScale)],0);
        ctx.moveTo(...corners[0]);for(const p of corners.slice(1))ctx.lineTo(...p);ctx.closePath();ctx.stroke();ctx.setDash([],0);
    }
    const handles=markHandles(mark,unit,screenScale);
    const size=6/Math.max(.01,screenScale);
    for(const h of handles){ctx.newPath();if(h.kind==='rotate'){drawRotationHandle(ctx,h.point,screenScale,accent);continue;}if(h.kind==='delete'){
            ctx.setSourceRGB(...[1,3,5].map(i=>parseInt(color.slice(i,i+2),16)/255));
            ctx.moveTo(h.point[0]-size/2,h.point[1]-size/2);ctx.lineTo(h.point[0]+size/2,h.point[1]+size/2);ctx.moveTo(h.point[0]+size/2,h.point[1]-size/2);ctx.lineTo(h.point[0]-size/2,h.point[1]+size/2);ctx.stroke();continue;
        }ctx.rectangle(h.point[0]-size/2,h.point[1]-size/2,size,size);
        ctx.setSourceRGB(...[1,3,5].map(i=>parseInt(color.slice(i,i+2),16)/255));ctx.stroke();}
    ctx.restore();
}

// Keep the rotation badge upright and use the same glyph for every element.
function drawRotationHandle(ctx,point,screenScale,accent){
    const scale=1/Math.max(.01,screenScale),rgb=c=>[1,3,5].map(i=>parseInt(c.slice(i,i+2),16)/255);
    ctx.save();ctx.translate(...point);ctx.scale(scale,scale);ctx.newPath();
    ctx.setSourceRGB(...rgb(accent));ctx.arc(0,0,6,0,Math.PI*2);ctx.fill();
    ctx.setSourceRGB(1,1,1);ctx.setLineWidth(.9);ctx.arc(0,0,5.5,0,Math.PI*2);ctx.stroke();
    ctx.setLineWidth(1.1);ctx.setLineCap(Cairo.LineCap.ROUND);ctx.setLineJoin(Cairo.LineJoin.ROUND);
    // A continuous outward spiral matches the reference's rotation affordance
    // without an ambiguous or disconnected arrowhead at this small size.
    for(let i=0;i<=64;i++){
        const progress=i/64,angle=-Math.PI/2+progress*Math.PI*3.2,radius=.25+progress*3.3;
        const x=Math.cos(angle)*radius,y=Math.sin(angle)*radius;
        if(i===0)ctx.moveTo(x,y);else ctx.lineTo(x,y);
    }
    ctx.stroke();ctx.restore();
}

function drawTextSelection(ctx,mark,unit,screenScale,accent,pointer){
    drawElementHover(ctx,mark,unit,screenScale,accent);
    if(mark.preeditFrame)return;
    const size=12/Math.max(.01,screenScale),ink=accentInk(accent),rgb=c=>[1,3,5].map(i=>parseInt(c.slice(i,i+2),16)/255);
    for(const handle of visibleMarkHandles(mark,pointer,unit,screenScale)){
        if(handle.kind==='rotate'){drawRotationHandle(ctx,handle.point,screenScale,accent);continue;}
        ctx.save();ctx.translate(...handle.point);ctx.rotate(transform(mark).angle);ctx.scale(size/12,size/12);
        ctx.setSourceRGB(...rgb(accent));
        ctx.rectangle(-6,-6,12,12);ctx.fill();
        ctx.setLineWidth(1);ctx.setSourceRGB(...rgb(ink));ctx.setLineCap(Cairo.LineCap.ROUND);ctx.setLineJoin(Cairo.LineJoin.ROUND);
        if(handle.kind==='delete'){
            ctx.moveTo(-2.5,-2.5);ctx.lineTo(2.5,2.5);ctx.moveTo(2.5,-2.5);ctx.lineTo(-2.5,2.5);
        }else{
            if(handle.index===6)ctx.rotate(Math.PI/2);
            ctx.moveTo(-3,-3);ctx.lineTo(3,3);ctx.moveTo(-3,0);ctx.lineTo(-3,-3);ctx.lineTo(0,-3);ctx.moveTo(0,3);ctx.lineTo(3,3);ctx.lineTo(3,0);
        }
        ctx.stroke();ctx.restore();
    }
}

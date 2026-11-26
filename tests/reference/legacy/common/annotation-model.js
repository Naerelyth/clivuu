import {AnnotationHistory, cloneMark} from './annotation-history.js';
import {endStyle,mosaicLevel,mosaicEffect,strokeWidth,strokeRange} from './annotation-options-model.js';
import {FONT_SIZE,TEXT_SIZE,fontPoints,textPixelSize,textOutlineColor,textSizeScale,textSizePixels,textSizeValue} from './text-style.js';
const clamp = (v, a, b) => Math.max(a, Math.min(b, v));
export const PALETTE = ['#ff6058','#000000','#919191','#800000','#ff0000','#ff9f1c','#ffd24a','#4caf50','#4f9ded','#7048e8',
    '#ffffff','#c7c7c7','#b17a55','#ffb9bf','#ffc78a','#f5da81','#e9efad','#b6dc8a','#95cad0','#cbb6df'];
export const DRAW_TOOLS = ['rect','ellipse','arrow','pen','highlight','mosaic','text','eraser'];
export const PRIMARY_TOOLS=[['select','Select¹'],['rect','Shapes²'],['arrow','Arrow⁴'],['pen','Pen⁵'],['highlight','Highlight⁶'],['mosaic','Mosaic⁷'],['text','Text⁸'],['eraser','Eraser⁹']];
export const TOOL_SHORTCUTS=['select','rect','ellipse','arrow','pen','highlight','mosaic','text','eraser'];
export const isFreehand=mark=>mark.kind==='pen'||(['highlight','mosaic','eraser'].includes(mark.kind)&&mark.fill==='stroke');
export const isCompleteMark=mark=>Boolean(mark?.points?.length&&(mark.kind==='eraser'?(mark.fill==='stroke'||(mark.points.length>1&&mark.points[0][0]!==mark.points.at(-1)[0]&&mark.points[0][1]!==mark.points.at(-1)[1])):mark.points.length>1));
export const isLineMark=mark=>mark.kind==='arrow'||(mark.kind==='highlight'&&mark.fill==='line');
export function defaultStyle(kind) {
    return {color:'#ff6058', width:['highlight','eraser'].includes(kind) ? 14 : kind === 'mosaic' ? 20 : 3,
        fill:'stroke', effect:'pixelate', effectLevel:6, alpha:kind==='highlight'?82:255,
        arrowStart:false, arrowEnd:true, lineStyle:'solid',
        fontFamily:'Sans', fontSize:FONT_SIZE.default, fontUnit:'pt',fontScale:1,bold:false, italic:false,
        outlineMode:'none',outlineLastMode:'auto',outlineWidth:1,outlineColor:'#000000'};
}
export function markBounds(mark, unit = 1) {
    if (mark.kind === 'text') return {x:mark.points[0][0],y:mark.points[0][1],
        width:Math.max(1,mark.textSize?.[0] ?? (mark.text??'').length * textPixelSize(mark,unit) * .6),
        height:Math.max(1,mark.textSize?.[1] ?? textPixelSize(mark,unit) * 1.3)};
    const xs=mark.points.map(p=>p[0]),ys=mark.points.map(p=>p[1]);
    return {x:Math.min(...xs),y:Math.min(...ys),width:Math.max(1,Math.max(...xs)-Math.min(...xs)),height:Math.max(1,Math.max(...ys)-Math.min(...ys))};
}
export function transform(mark) { return {x:0,y:0,sx:1,sy:1,angle:0,...mark.transform}; }
export function mapPoint(mark, point, unit=1, inverse=false) {
    const b=markBounds(mark,unit),cx=b.x+b.width/2,cy=b.y+b.height/2,t=transform(mark),c=Math.cos(t.angle),s=Math.sin(t.angle);
    if(inverse){const x=point[0]-cx-t.x,y=point[1]-cy-t.y;return [cx+(x*c+y*s)/t.sx,cy+(-x*s+y*c)/t.sy];}
    const x=(point[0]-cx)*t.sx,y=(point[1]-cy)*t.sy;return [cx+t.x+x*c-y*s,cy+t.y+x*s+y*c];
}
// Keep tiny text controls apart in display pixels without changing glyph
// metrics, transform pivots, saved annotation data or exported content.
export function markControlBounds(mark,unit=1,screenScale=1) {
    const b=markBounds(mark,unit);
    if(mark.kind!=='text')return b;
    const t=transform(mark),scale=Math.max(.01,screenScale);
    const width=Math.max(b.width,28/(scale*Math.max(.01,Math.abs(t.sx))));
    const height=Math.max(b.height,28/(scale*Math.max(.01,Math.abs(t.sy))));
    return {x:b.x+(b.width-width)/2,y:b.y+(b.height-height)/2,width,height};
}
export function markHandles(mark,unit=1,screenScale=1) {
    if(isFreehand(mark))return [];
    if(isLineMark(mark))return [0,mark.points.length-1].map((index,i)=>({kind:'endpoint',index,point:mapPoint(mark,mark.points[index],unit),endpoint:i}));
    const b=markControlBounds(mark,unit,screenScale),{x,y,width:w,height:h}=b;
    const points=[[x,y],[x+w/2,y],[x+w,y],[x+w,y+h/2],[x+w,y+h],[x+w/2,y+h],[x,y+h],[x,y+h/2]];
    const result=points.map((p,index)=>({kind:mark.kind==='text'&&index===2?'delete':'resize',index,point:mapPoint(mark,p,unit),local:p})).filter(h=>mark.kind!=='text'||h.index%2===0);
    const t=transform(mark);
    result.push({kind:'rotate',point:mapPoint(mark,[x+w/2,y-22/Math.max(.01,screenScale*t.sy)],unit)});
    return result;
}
function nearestHandle(mark,point,unit,scale,tolerance){
    return markHandles(mark,unit,scale).map(handle=>({handle,distance:Math.hypot(handle.point[0]-point[0],handle.point[1]-point[1])}))
        .filter(value=>value.distance<=tolerance).sort((a,b)=>a.distance-b.distance||(a.handle.kind==='delete'?-1:b.handle.kind==='delete'?1:0))[0]?.handle;
}
export function visibleMarkHandles(mark,point,unit=1,screenScale=1){
    const handles=markHandles(mark,unit,screenScale);
    if(mark.kind!=='text')return handles;
    const b=markControlBounds(mark,unit,screenScale),t=transform(mark),p=point?mapPoint(mark,point,unit,true):null;
    const mx=8/(screenScale*Math.abs(t.sx)),my=8/(screenScale*Math.abs(t.sy));
    const hovered=p&&p[0]>=b.x-mx&&p[0]<=b.x+b.width+mx&&p[1]>=b.y-my&&p[1]<=b.y+b.height+my;
    return hovered?handles:handles.filter(h=>h.kind==='rotate');
}
function distanceToSegment(p,a,b){const dx=b[0]-a[0],dy=b[1]-a[1],v=clamp(((p[0]-a[0])*dx+(p[1]-a[1])*dy)/(dx*dx+dy*dy||1),0,1);return Math.hypot(p[0]-a[0]-v*dx,p[1]-a[1]-v*dy);}
function hitShape(mark,point,unit,tolerance){
    const b=markBounds(mark,unit),t=transform(mark),dx=point[0]-b.x-b.width/2-t.x,dy=point[1]-b.y-b.height/2-t.y,c=Math.cos(t.angle),s=Math.sin(t.angle);
    const x=Math.abs(dx*c+dy*s),y=Math.abs(-dx*s+dy*c),rx=b.width*Math.abs(t.sx)/2,ry=b.height*Math.abs(t.sy)/2;
    const filled=mark.fill==='solid',margin=tolerance+(filled?0:(mark.width??3)/(2*unit));
    if(x>rx+margin||y>ry+margin)return false;
    if(mark.kind==='rect'){
        if(x<=rx&&y<=ry)return filled||Math.min(rx-x,ry-y)<=margin;
        return Math.hypot(Math.max(0,x-rx),Math.max(0,y-ry))<=margin;
    }
    if(filled&&Math.hypot(x/rx,y/ry)<=1)return true;
    // Find the nearest ellipse point in its unrotated world coordinates.
    // This keeps hit testing aligned with the constant-width Cairo stroke,
    // including very unequal radii, rather than scaling the hit margin.
    const distance=a=>(rx*Math.cos(a)-x)**2+(ry*Math.sin(a)-y)**2,k=(Math.sqrt(5)-1)/2;
    let lo=0,hi=Math.PI/2,a=hi-k*(hi-lo),z=lo+k*(hi-lo),fa=distance(a),fz=distance(z);
    for(let i=0;i<32;i++){
        if(fa<fz){hi=z;z=a;fz=fa;a=hi-k*(hi-lo);fa=distance(a);}
        else{lo=a;a=z;fa=fz;z=lo+k*(hi-lo);fz=distance(z);}
    }
    return Math.min(fa,fz,distance(0),distance(Math.PI/2))<=margin*margin;
}
export function hitMark(mark,point,unit=1,tolerance=6){
    if(['rect','ellipse'].includes(mark.kind))return hitShape(mark,point,unit,tolerance);
    const p=mapPoint(mark,point,unit,true),b=markBounds(mark,unit),t=transform(mark),m=tolerance/Math.max(.05,Math.min(t.sx,t.sy))+(mark.width??3)/(2*unit*Math.sqrt(t.sx*t.sy));
    if(mark.kind==='arrow'){
        const a=mark.points[0],z=mark.points.at(-1),head=Math.min(Math.hypot(z[0]-a[0],z[1]-a[1])*.45,Math.max((mark.width??3)*4/unit,10/unit));
        return distanceToSegment(p,a,z)<=m || [a,z].some((v,i)=>endStyle(mark,i===0)!=='none'&&Math.hypot(p[0]-v[0],p[1]-v[1])<=head+m);
    }
    if(isLineMark(mark))return distanceToSegment(p,mark.points[0],mark.points.at(-1))<=m;
    if(p[0]<b.x-m||p[0]>b.x+b.width+m||p[1]<b.y-m||p[1]>b.y+b.height+m)return false;
    if(mark.kind==='text')return true;
    const shape=['rect','ellipse'].includes(mark.kind)?mark.kind:mark.fill;
    if(shape==='rect')return mark.fill==='solid'||['highlight','mosaic','eraser'].includes(mark.kind)||Math.min(Math.abs(p[0]-b.x),Math.abs(p[0]-b.x-b.width),Math.abs(p[1]-b.y),Math.abs(p[1]-b.y-b.height))<=m;
    if(shape==='ellipse'){
        const r=Math.hypot((p[0]-b.x-b.width/2)/(b.width/2),(p[1]-b.y-b.height/2)/(b.height/2));
        return mark.fill==='solid'||['highlight','mosaic','eraser'].includes(mark.kind)?r<=1+m/Math.min(b.width,b.height):Math.abs(r-1)<=m*2/Math.min(b.width,b.height);
    }
    if(mark.points.length===1)return Math.hypot(p[0]-mark.points[0][0],p[1]-mark.points[0][1])<=m;
    return mark.points.some((v,i)=>i>0&&distanceToSegment(p,mark.points[i-1],v)<=m);
}
export class AnnotationEditor {
    constructor(unit=1,measureText=null,strokeScale=unit){
        this.unit=unit;this.strokeScale=Math.max(.01,strokeScale);this.history=new AnnotationHistory();
        this.styles=new Map(DRAW_TOOLS.map(k=>{const style=defaultStyle(k);if(k==='text'){style.fontScale=this.strokeScale;style.transform={sx:textSizeScale(TEXT_SIZE.default),sy:textSizeScale(TEXT_SIZE.default)};}else style.width=strokeWidth(k,style.width*this.strokeScale,this.strokeScale);return [k,style];}));
        this.styles.set('ellipse',this.styles.get('rect'));this.measureText=measureText;this.active=-1;
    }
    get marks(){return this.history.marks;}
    get current(){return this.edit?.after??this.textEdit?.draft??this.marks[this.active];}
    get controlMark(){return this.edit?.after??this.textEdit?.frame??this.current;}
    setTextPreedit(size=null,force=false){
        const session=this.textEdit;if(!session)return false;
        const active=Boolean(size),key=active?size.join(':'):'';
        if(!force&&Boolean(session.preedit)===active&&session.preeditKey===key)return false;
        session.preedit=active;session.preeditKey=key;session.frame=null;
        if(active){
            const draft=session.draft,frame=cloneMark(draft),origin=mapPoint(draft,draft.points[0],this.unit);
            frame.textSize=size.map(v=>Math.max(1,v));frame.text=frame.text||' ';frame.preeditFrame=true;
            const after=mapPoint(frame,frame.points[0],this.unit),t=transform(frame);frame.transform={...t,x:t.x+origin[0]-after[0],y:t.y+origin[1]-after[1]};
            session.frame=frame;
        }
        return true;
    }
    get displayMarks(){
        if(this.textEdit){
            const {index,preedit}=this.textEdit;
            const marks=this.marks.flatMap((m,i)=>i===index?(preedit?[]:[this.current]):[m]);
            if(index<0&&!preedit)marks.push(this.current);return marks;
        }
        return this.edit?this.marks.map((m,i)=>i===this.edit.index?this.edit.after:m):this.marks;
    }
    style(tool){return this.styles.get(tool)??defaultStyle(tool);}
    strokeRange(tool){return strokeRange(tool,this.strokeScale);}
    select(index){this.wheelGroup=null;this.active=index;return this.current;}
    hit(point,tolerance=6){for(let i=this.marks.length-1;i>=0;i--)if(hitMark(this.marks[i],point,this.unit,tolerance))return i;return -1;}
    decorate(mark){
        if(mark.kind==='text'&&this.measureText)mark.textSize=this.measureText(mark,this.unit);
        if(isLineMark(mark)&&mark.points.length>2){
            // Removing curve extrema changes the transform's bounds centre.
            // Compensate its translation so both world endpoints stay fixed.
            const first=mapPoint(mark,mark.points[0],this.unit);
            mark.points=[mark.points[0],mark.points.at(-1)];
            if(mark.transform){const next=mapPoint(mark,mark.points[0],this.unit),t=transform(mark);mark.transform={...t,x:t.x+first[0]-next[0],y:t.y+first[1]-next[1]};}
        }
        return mark;
    }
    create(kind,point,text='') {this.wheelGroup=null;this.active=-1;return this.decorate({kind,...this.style(kind),points:[point],...(kind==='text'?{text}:{})});}
    add(mark){this.wheelGroup=null;this.history.add(this.decorate(mark));this.active=this.history.selected;}
    beginText(point,index=-1){
        this.commitText();this.edit=null;this.wheelGroup=null;
        const before=index>=0?this.marks[index]:null;
        if(before&&before.kind!=='text')return false;
        const draft=before?cloneMark(before):this.create('text',point);
        this.active=index;
        this.textEdit={index,draft,undo:[],redo:[],run:null};return true;
    }
    _textValue(patch){
        const session=this.textEdit;if(!session)return;
        session.draft=this.restyle(session.draft,patch);
        if(session.preedit)this.setTextPreedit(session.frame.textSize,true);
    }
    restyle(before,patch){
        const anchor=before.kind==='text'?mapPoint(before,before.points[0],this.unit):null;
        const next=this.decorate({...cloneMark(before),...patch});
        // Changing a transformed text document's dimensions changes its pivot.
        // Preserve its world origin while typing or adjusting its font.
        if(anchor&&next.transform){const after=mapPoint(next,next.points[0],this.unit),t=transform(next);next.transform={...t,x:t.x+anchor[0]-after[0],y:t.y+anchor[1]-after[1]};}
        return next;
    }
    updateText(value){
        if(this.edit?.text)this.end();
        const s=this.textEdit;if(!s)return;
        value=value.slice(0,4000);const before=s.draft.text;if(value===before)return;
        let start=0;while(start<before.length&&start<value.length&&before[start]===value[start])start++;
        let end=0;while(end<before.length-start&&end<value.length-start&&before.at(-1-end)===value.at(-1-end))end++;
        const removed=before.slice(start,before.length-end),added=value.slice(start,value.length-end);
        const kind=removed?'delete':'insert',now=Date.now(),single=(removed.length+added.length===1&&!/[\n\r]/.test(removed+added));
        const merge=single&&s.run?.kind===kind&&now-s.run.time<1000&&(start===s.run.next||(kind==='delete'&&start===s.run.next-1))&&!s.redo.length;
        if(!merge)s.undo.push(before);
        s.redo.length=0;s.run=single?{kind,time:now,next:start+added.length}:null;
        this._textValue({text:value});
    }
    textHistory(redo=false){
        const s=this.textEdit;if(!s)return false;
        const source=redo?s.redo:s.undo,target=redo?s.undo:s.redo;
        if(!source.length)return false;
        target.push(s.draft.text);s.run=null;this._textValue({text:source.pop()});return true;
    }
    commitText(){
        if(this.edit?.text)this.end();
        const s=this.textEdit;if(!s)return false;
        this.textEdit=null;this.wheelGroup=null;
        if(s.draft.text){if(s.index<0){this.history.add(s.draft);this.active=this.history.selected;}else{this.history.replace(s.index,s.draft);this.active=s.index;}}
        else if(s.index>=0){this.history.remove(s.index);this.active=-1;}
        else this.active=-1;
        return true;
    }
    cancelText(){if(!this.textEdit)return;if(this.edit?.text)this.edit=null;this.active=this.textEdit.index;this.textEdit=null;this.wheelGroup=null;}
    change(tool,patch,group=null){
        if(patch.fontSizeValue!==undefined){this.setTextSize(patch.fontSizeValue,group);patch={...patch};delete patch.fontSizeValue;if(!Object.keys(patch).length)return;}
        if(patch.outlineMode&&patch.outlineMode!=='none')patch={...patch,outlineLastMode:patch.outlineMode};
        if(patch.outlineWidth!==undefined)patch={...patch,outlineWidth:clamp(Number(patch.outlineWidth)||0,0,8)};
        if(patch.fontSize!==undefined)patch={...patch,fontSize:fontPoints(patch.fontSize),fontUnit:'pt',fontScale:this.current?.fontScale??this.strokeScale};
        if(patch.width!==undefined)patch={...patch,width:strokeWidth(tool,patch.width,this.strokeScale)};
        if(tool==='mosaic'&&patch.effect!==undefined)patch={...patch,effect:mosaicEffect(patch.effect)};
        if(patch.effectLevel!==undefined)patch={...patch,effectLevel:mosaicLevel(patch.effectLevel)};
        if(!group)this.wheelGroup=null;
        patch={...patch};
        if(Object.hasOwn(patch,'arrowStart')&&!Object.hasOwn(patch,'startStyle'))patch.startStyle=patch.arrowStart?'open':'none';
        if(Object.hasOwn(patch,'arrowEnd')&&!Object.hasOwn(patch,'endStyle'))patch.endStyle=patch.arrowEnd?'open':'none';
        if(Object.hasOwn(patch,'startStyle'))patch.arrowStart=patch.startStyle!=='none';
        if(Object.hasOwn(patch,'endStyle'))patch.arrowEnd=patch.endStyle!=='none';
        Object.assign(this.style(tool),patch);
        if(this.textEdit&&tool==='text'){this.textEdit.run=null;this._textValue(patch);}
        else if(this.current&&this.current.kind===tool)this.history.replace(this.active,this.restyle(this.current,patch),group);
    }
    setTextSize(value,group=null){
        if(this.edit)return;
        if(!group)this.wheelGroup=null;
        const scale=textSizeScale(value),style=this.style('text');
        Object.assign(style,{fontSize:FONT_SIZE.default,fontUnit:'pt',fontScale:this.strokeScale,transform:{sx:scale,sy:scale}});
        if(this.current?.kind!=='text')return;
        const next=cloneMark(this.current),base=textPixelSize(next,1)/(next.fontScale??this.strokeScale),sx=textSizePixels(value)/base;
        next.transform={...transform(next),sx,sy:sx};
        if(this.textEdit){this.textEdit.run=null;this.textEdit.draft=next;if(this.textEdit.preedit)this.setTextPreedit(this.textEdit.frame.textSize,true);}
        else this.history.replace(this.active,next,group);
    }
    changeShape(kind){
        if(!['rect','ellipse'].includes(kind)||this.edit||(this.current&&!['rect','ellipse'].includes(this.current.kind)))return false;
        this.wheelGroup=null;
        if(this.current){
            const current=this.current;
            for(const key of ['color','width','fill','alpha'])if(current[key]!==undefined)this.style(kind)[key]=current[key];
            if(current.kind!==kind)this.history.replace(this.active,{...cloneMark(current),kind});
        }
        return true;
    }
    undo(){this.commitText();this.wheelGroup=null;this.edit=null;this.history.undo();this.active=this.history.selected;}
    redo(){this.commitText();this.wheelGroup=null;this.edit=null;this.history.restore();this.active=this.history.selected;}
    remove(){this.wheelGroup=null;this.history.remove(this.active);this.active=-1;}
    clear(){this.cancelText();this.wheelGroup=null;this.edit=null;this.active=-1;this.history.clear();}
    textTarget(point,screenScale=1){
        if(this.textEdit?.preedit)return null;
        const mark=this.textEdit&&this.current;if(!mark)return null;
        const tolerance=7/Math.max(.01,screenScale),handle=nearestHandle(mark,point,this.unit,screenScale,tolerance);
        if(handle)return {index:this.textEdit.index,handle,text:true};
        const p=mapPoint(mark,point,this.unit,true),b=markControlBounds(mark,this.unit,screenScale),t=transform(mark),mx=5/(screenScale*Math.abs(t.sx)),my=5/(screenScale*Math.abs(t.sy));
        const inside=p[0]>=b.x-mx&&p[0]<=b.x+b.width+mx&&p[1]>=b.y-my&&p[1]<=b.y+b.height+my;
        const edge=Math.min(Math.abs(p[0]-b.x)/mx,Math.abs(p[0]-b.x-b.width)/mx,Math.abs(p[1]-b.y)/my,Math.abs(p[1]-b.y-b.height)/my)<=1;
        return inside&&edge?{index:this.textEdit.index,handle:{kind:'move'},text:true}:null;
    }
    target(point,tool,screenScale=1){
        if(this.textEdit)return this.textTarget(point,screenScale);
        if(tool==='eraser'&&this.style('eraser').fill==='stroke')return null;
        const mark=this.current,tolerance=7/Math.max(.01,screenScale);
        const handle=mark&&nearestHandle(mark,point,this.unit,screenScale,tolerance);
        const index=handle?this.active:this.hit(point,tolerance);
        return index<0||(tool!=='select'&&index!==this.active&&!handle&&!(tool==='text'&&this.marks[index].kind==='text'))?null:{index,handle:handle??{kind:'move'}};
    }
    hover(point,tool,screenScale=1){
        if(this.edit||!['select','text'].includes(tool))return null;
        const index=this.hit(point,1/Math.max(.01,screenScale));
        return index!==this.active&&this.marks[index]?.kind==='text'?this.marks[index]:null;
    }
    cursor(point,tool,screenScale=1){
        const target=this.edit??this.target(point,tool,screenScale);
        if(!target)return null;
        const {handle}=target;
        if(handle.kind==='delete')return 'pointer';
        if(handle.kind==='move')return 'move';
        if(handle.kind==='endpoint'){
            const mark=this.edit?.after??this.marks[target.index]??this.current,points=[mark.points[0],mark.points.at(-1)].map(p=>mapPoint(mark,p,this.unit));
            const p=points[handle.endpoint],other=points[1-handle.endpoint];
            const direction=((Math.round(Math.atan2(p[1]-other[1],p[0]-other[0])/(Math.PI/4))%8)+8)%8;
            return ['e','se','s','sw','w','nw','n','ne'][direction]+'-resize';
        }
        if(handle.kind==='rotate')return this.edit?'grabbing':'grab';
        const angle=transform(this.edit?.before??this.marks[target.index]??this.current).angle;
        const direction=((Math.round((handle.index-3)+angle/(Math.PI/4))%8)+8)%8;
        return ['e','se','s','sw','w','nw','n','ne'][direction]+'-resize';
    }
    begin(point,tool,screenScale=1){
        this.wheelGroup=null;
        const target=this.target(point,tool,screenScale);
        if(!target)return false;
        let {index}=target;const {handle}=target;
        if(handle.kind==='delete'){
            if(this.textEdit&&!this.current.text){if(this.textEdit.index<0)this.cancelText();else this.commitText();return true;}
            if(this.textEdit){this.commitText();index=this.active;}
            this.active=index;this.remove();return true;
        }
        if(target.text){
            if(!this.current.text){
                const before=cloneMark(this.current);
                this.edit={index,before,after:cloneMark(before),point,handle,text:true,draftOnly:true};return true;
            }
            // Publish the typed document before a transform, keeping its native
            // input widget and document undo stack alive during the gesture.
            const session=this.textEdit;this.commitText();index=this.active;
            this.textEdit={...session,index,draft:cloneMark(this.marks[index]),run:null};
        }
        this.active=index;
        const before=cloneMark(this.marks[index]);
        this.edit={index,before,after:cloneMark(before),point,handle,text:Boolean(target.text)};return true;
    }
    move(point,shift=false){
        if(!this.edit)return;
        const {before,handle,point:start}=this.edit,b=markBounds(before,this.unit),t=transform(before),next=cloneMark(before);
        next.transform={...t};
        if(handle.kind==='endpoint'){
            if(Math.hypot(point[0]-start[0],point[1]-start[1])<1e-9){this.edit.after=cloneMark(before);return;}
            const ends=[before.points[0],before.points.at(-1)].map(p=>mapPoint(before,p,this.unit));
            const i=handle.endpoint,other=ends[1-i];
            let p=[ends[i][0]+point[0]-start[0],ends[i][1]+point[1]-start[1]];
            if(shift){const dx=p[0]-other[0],dy=p[1]-other[1],r=Math.hypot(dx,dy),a=Math.round(Math.atan2(dy,dx)/(Math.PI/4))*(Math.PI/4);p=[other[0]+r*Math.cos(a),other[1]+r*Math.sin(a)];}
            ends[i]=p;next.points=ends;delete next.transform;
        }else if(handle.kind==='move'){next.transform.x+=point[0]-start[0];next.transform.y+=point[1]-start[1];}
        else if(handle.kind==='rotate'){
            const center=mapPoint(before,[b.x+b.width/2,b.y+b.height/2],this.unit);
            let a=t.angle+Math.atan2(point[1]-center[1],point[0]-center[0])-Math.atan2(start[1]-center[1],start[0]-center[0]);
            if(shift)a=Math.round(a/(Math.PI/12))*(Math.PI/12);next.transform.angle=a;
        }else if(before.kind==='text'){
            // The reference scales text radially about its fixed centre.
            // Retain the exact initial grab offset and do not reflow glyphs.
            const center=mapPoint(before,[b.x+b.width/2,b.y+b.height/2],this.unit),h=handle.point;
            const dx=point[0]-start[0]+h[0]-center[0],dy=point[1]-start[1]+h[1]-center[1];
            const base=textPixelSize(before,1)/(before.fontScale??this.strokeScale),ratio=Math.hypot(dx,dy)/Math.max(.01,Math.hypot(h[0]-center[0],h[1]-center[1]));
            const sx=clamp(t.sx*ratio,textSizePixels(TEXT_SIZE.min)/base,textSizePixels(TEXT_SIZE.max)/base);
            next.transform={...t,sx,sy:sx};
        } else {
            const h=handle.local,opposite=markHandles(before,this.unit)[(handle.index+4)%8].local;
            // Preserve the initial grab offset, including rotated shapes.
            const p=mapPoint(before,point,this.unit,true),p0=mapPoint(before,start,this.unit,true);
            let fx=h[0]===opposite[0]?1:(h[0]+p[0]-p0[0]-opposite[0])/(h[0]-opposite[0]);
            let fy=h[1]===opposite[1]?1:(h[1]+p[1]-p0[1]-opposite[1])/(h[1]-opposite[1]);
            if(shift){const factor=Math.abs(fx-1)>Math.abs(fy-1)?fx:fy;fx=factor;fy=factor;}
            const sx=clamp(t.sx*fx,.05,100),sy=clamp(t.sy*fy,.05,100),cx=b.x+b.width/2,cy=b.y+b.height/2;
            const dx=(opposite[0]-cx)*(t.sx-sx),dy=(opposite[1]-cy)*(t.sy-sy);
            next.transform={...t,sx,sy,x:t.x+dx*Math.cos(t.angle)-dy*Math.sin(t.angle),y:t.y+dx*Math.sin(t.angle)+dy*Math.cos(t.angle)};
        }
        this.edit.after=next;
    }
    end(){
        if(!this.edit)return;
        const {index,before,after,text,draftOnly}=this.edit;this.edit=null;
        // Selecting an element, or dragging back to its starting point, must
        // not add an identity transform and consume the next Undo operation.
        const from=transform(before),to=transform(after);
        if(Object.keys(from).every(key=>Math.abs(from[key]-to[key])<1e-9)){
            if(before.transform)after.transform=cloneMark(before.transform);else delete after.transform;
        }
        if(!draftOnly)this.history.replace(index,after);
        if(text&&this.textEdit)this.textEdit.draft=cloneMark(after);
        if(after.kind==='text'){const s=textSizeScale(textSizeValue(after,this.strokeScale));Object.assign(this.style('text'),{fontSize:FONT_SIZE.default,fontUnit:'pt',fontScale:this.strokeScale,transform:{sx:s,sy:s}});}
    }
    wheel(tool,delta){
        if(!Number.isFinite(delta)||delta===0||this.edit)return;
        const current=this.current?.kind===tool?this.current:this.style(tool),key=tool==='text'?'fontSizeValue':'width';
        if(!this.wheelGroup||this.wheelGroup.index!==this.active||this.wheelGroup.tool!==tool)
            this.wheelGroup={index:this.active,tool};
        const next=(key==='fontSizeValue'?textSizeValue(current,this.strokeScale):(current[key]??3))-Math.sign(delta)*(key==='width'?this.strokeRange(tool).step:1);
        this.change(tool,{[key]:key==='width'?strokeWidth(tool,next,this.strokeScale):next},this.wheelGroup);
    }
}

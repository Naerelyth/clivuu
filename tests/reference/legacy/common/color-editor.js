import {ColorDraft,hsvToRgb,hex,parseColor} from './color-model.js';
import {t} from './i18n.js';
import {request} from '../portable/portal.js';
import Gio from 'gi://Gio';
// Shared controls, draft lifecycle and color math. Adapters only create widgets.
export function colorEditor(ui,{style,settings,apply,close,pick=null,alphaEnabled=true}){
    const draft=new ColorDraft(),widget=ui.box(true),top=ui.box(false),left=ui.box(true),right=ui.box(true),fields={},canvases=[],customButtons=[];
    let syncing=false,slot=0,closed=false,picker=null,generation=0;
    ui.role?.(widget,'editor');
    const dismiss=()=>{generation++;picker?.cancel();picker=null;};
    const finish=()=>{dismiss();close();};
    ui.add(widget,top);ui.add(top,left);ui.add(top,right);
    const getCustom=()=>{try{return settings?.get_strv('custom-colors')??[];}catch{return [];}};
    const paintSwatch=(ctx,w,h,color,alpha=255)=>{
        for(let y=0;y<h;y+=6)for(let x=0;x<w;x+=6){ctx.setSourceRGB(...Array(3).fill((x/6+y/6)%2?.65:.9));ctx.rectangle(x,y,6,6);ctx.fill();}
        const p=parseColor(color);ctx.setSourceRGBA(...(p?.rgb??[255,255,255]).map(v=>v/255),alpha/255);ctx.rectangle(0,0,w,h);ctx.fill();
    };
    ui.add(left,ui.label(t('Basic colors')));
    const palette=ui.box(true);ui.add(left,palette);
    const basics=['#000000','#800000','#008000','#808000','#000080','#800080','#008080','#c0c0c0',
        '#808080','#ff0000','#00ff00','#ffff00','#0000ff','#ff00ff','#00ffff','#ffffff',
        ...Array.from({length:32},(_,i)=>hex(hsvToRgb((i%8)*45,[255,170,100,55][Math.floor(i/8)],255)))];
    for(let y=0;y<6;y++){const row=ui.box(false);ui.add(palette,row);for(let x=0;x<8;x++){
        const color=basics[y*8+x],b=ui.swatch(color,()=>{draft.set(color,draft.alpha);sync();});ui.add(row,b);
    }}
    const status=ui.label('');status.visible=false;
    const sample=ui.button(t('Pick screen color'),async()=>{
        if(picker||closed)return;
        const token=generation,cancellation=new Gio.Cancellable();picker=cancellation;
        try{
            const rgb=pick?await pick(cancellation): (await request('Screenshot','PickColor','(sa{sv})',[''],{},cancellation)).color?.map(v=>Math.round(v*255));
            if(closed||token!==generation)return;
            if(rgb?.length===3){draft.set(hex(rgb),draft.alpha);sync();}
        }catch{if(!closed&&token===generation){ui.text(status,t('Color picking cancelled'));status.visible=true;}}
        finally{if(picker===cancellation)picker=null;}
    });ui.add(left,sample);
    ui.add(left,ui.label(t('Custom colors')));
    for(let y=0;y<2;y++){const row=ui.box(false);ui.add(left,row);for(let x=0;x<8;x++){
        const index=y*8+x,b=ui.swatch('#ffffff',()=>{slot=index;const color=getCustom()[slot];if(color)draft.set(color);sync();});customButtons.push(b);ui.add(row,b);
    }}
    const add=ui.button(t('Add to custom colors'),()=>{
        if(!flush())return;
        const values=getCustom();while(values.length<=slot)values.push('#ffffff');values[slot]=draft.color+draft.alpha.toString(16).padStart(2,'0');
        settings?.set_strv('custom-colors',values);sync();
    });ui.add(left,add);
    const plane=ui.canvas(192,128,(ctx,w,h)=>{
        for(let x=0;x<w;x+=3)for(let y=0;y<h;y+=3){ctx.setSourceRGB(...hsvToRgb(x/(w-1)*359,(1-y/(h-1))*255,255).map(v=>v/255));ctx.rectangle(x,y,3,3);ctx.fill();}
        const x=draft.hsv[0]/359*w,y=(1-draft.hsv[1]/255)*h;ctx.setSourceRGB(0,0,0);ctx.setLineWidth(1);ctx.moveTo(x-5,y);ctx.lineTo(x+5,y);ctx.moveTo(x,y-5);ctx.lineTo(x,y+5);ctx.stroke();
    },(x,y,w,h)=>{draft.channel('hsv',0,x/w*359);draft.channel('hsv',1,(1-y/h)*255);sync();});
    const value=ui.canvas(14,128,(ctx,w,h)=>{
        for(let y=0;y<h;y++){ctx.setSourceRGB(...hsvToRgb(draft.hsv[0],draft.hsv[1],(1-y/h)*255).map(v=>v/255));ctx.rectangle(0,y,w,1);ctx.fill();}
        ctx.setSourceRGB(1,1,1);ctx.setLineWidth(2);const y=(1-draft.hsv[2]/255)*h;ctx.moveTo(0,y);ctx.lineTo(w,y);ctx.stroke();
    },(_x,y,_w,h)=>{draft.channel('hsv',2,(1-y/h)*255);sync();});
    const graphic=ui.box(false);ui.add(graphic,plane);ui.add(graphic,value);ui.add(right,graphic);canvases.push(plane,value);
    const preview=ui.canvas(210,20,(ctx,w,h)=>paintSwatch(ctx,w,h,draft.color,draft.alpha));ui.add(right,preview);canvases.push(preview);
    const labels=[['h',t('Hue'),'hsv',0,359],['r',t('Red'),'rgb',0,255],['s',t('Saturation'),'hsv',1,255],['g',t('Green'),'rgb',1,255],['v',t('Value'),'hsv',2,255],['b',t('Blue'),'rgb',2,255]];
    for(let i=0;i<labels.length;i+=2){const row=ui.box(false);ui.add(right,row);for(const [key,label,space,index,max] of labels.slice(i,i+2)){
        const caption=ui.label(key.toUpperCase());ui.hint?.(caption,label);ui.add(row,caption);fields[key]=ui.number(max,v=>{if(!syncing){draft.channel(space,index,v);sync();}});ui.add(row,fields[key].widget);
    }}
    const alphaRow=ui.box(false);ui.add(right,alphaRow);ui.add(alphaRow,ui.label(t('Alpha')));fields.a=ui.number(255,v=>{if(!syncing){draft.alpha=Math.round(v);sync();}});ui.add(alphaRow,fields.a.widget);
    alphaRow.visible=alphaEnabled;
    const htmlRow=ui.box(false),html=ui.entry(value=>{if(!draft.set(value,draft.alpha))return false;sync();return true;},()=>commit());ui.add(right,htmlRow);ui.add(htmlRow,ui.label('HTML'));ui.add(htmlRow,html.widget);
    const buttons=ui.box(false),cancel=ui.button(t('Cancel'),finish),ok=ui.button(t('OK'),()=>commit());ui.role?.(buttons,'actions');ui.role?.(cancel,'action');ui.role?.(ok,'primary-action');ui.add(buttons,cancel);ui.add(buttons,ok);ui.add(widget,status);ui.add(widget,buttons);
    function sync(force=false){
        syncing=true;
        if(!alphaEnabled)draft.alpha=255;
        for(const [key,index] of [['h',0],['s',1],['v',2]])fields[key].set(Math.round(draft.hsv[index]),force);
        for(const [key,index] of [['r',0],['g',1],['b',2]])fields[key].set(draft.rgb[index],force);fields.a.set(draft.alpha,force);html.set(draft.color,force);
        const saved=getCustom();customButtons.forEach((b,i)=>ui.swatchColor(b,saved[i]??'#ffffff',slot===i));
        canvases.forEach(c=>ui.redraw(c));syncing=false;
    }
    function flush(){
        if(!Object.values(fields).every(field=>field.flush())||!html.flush()){
            ui.text(status,t('Enter valid color values'));status.visible=true;return false;
        }
        status.visible=false;return true;
    }
    function commit(){if(!flush())return;apply(draft.patch);finish();}
    return {widget,draft,fields,html:html.widget,sample,add,ok,cancel,plane,value,preview,open(){dismiss();const s=style();draft.set(s.color??'#ff6058',s.alpha??255);ui.text(status,'');status.visible=false;sync(true);},sync,dismiss,destroy(){closed=true;dismiss();}};
}

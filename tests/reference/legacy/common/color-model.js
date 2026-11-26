const clamp=(v,a,b)=>Math.max(a,Math.min(b,Number(v)||0));
export const byte=v=>Math.round(clamp(v,0,255));
export const hex=rgb=>'#'+rgb.slice(0,3).map(v=>byte(v).toString(16).padStart(2,'0')).join('');
export function parseColor(text) {
    if(!/^#(?:[\da-f]{6}|[\da-f]{8})$/i.test(text))return null;
    const values=[1,3,5].map(i=>parseInt(text.slice(i,i+2),16));
    return {rgb:values,alpha:text.length===9?parseInt(text.slice(7,9),16):255};
}
export function hsvToRgb(h,s,v) {
    h=((Number(h)||0)%360+360)%360/60;s=clamp(s,0,255)/255;v=clamp(v,0,255);
    const c=v*s,x=c*(1-Math.abs(h%2-1)),m=v-c;
    return [[c,x,0],[x,c,0],[0,c,x],[0,x,c],[x,0,c],[c,0,x]][Math.floor(h)].map(n=>byte(n+m));
}
export function rgbToHsv(rgb,previousHue=0) {
    const [r,g,b]=rgb.map(byte),v=Math.max(r,g,b),d=v-Math.min(r,g,b);
    let h=previousHue;
    if(d)h=60*((v===r?(g-b)/d+(g<b?6:0):v===g?(b-r)/d+2:(r-g)/d+4));
    return [h,v?255*d/v:0,v];
}
// A color dialog edits a draft; one Apply creates one history operation.
export class ColorDraft {
    constructor(color='#ff6058',alpha=255){this.set(color,alpha);}
    set(color,alpha=255){const p=parseColor(color);if(!p)return false;this.rgb=p.rgb;this.alpha=color.length===9?p.alpha:byte(alpha);this.hsv=rgbToHsv(this.rgb,this.hsv?.[0]);return true;}
    channel(space,index,value){
        if(space==='hsv'){this.hsv[index]=clamp(value,0,index===0?359:255);this.rgb=hsvToRgb(...this.hsv);}
        else {this.rgb[index]=byte(value);this.hsv=rgbToHsv(this.rgb,this.hsv[0]);}
    }
    get color(){return hex(this.rgb);}
    get patch(){return {color:this.color,alpha:this.alpha};}
}

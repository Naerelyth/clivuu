// Annotation font sizes are typographic points, independent of output DPR.
// fontScale converts the document's logical pixels to its source-image pixels.
export const FONT_SIZE={min:1,max:300,default:18};
// Snipaste's size field is a scale level, not a typographic point count.
// A 24px base font uses scale .25 + (level - 1) * .125.
export const TEXT_SIZE={min:1,max:72,default:4,basePixels:24,minScale:.25,step:.125};
export const textSizeLevel=value=>Math.max(TEXT_SIZE.min,Math.min(TEXT_SIZE.max,Number(value)||TEXT_SIZE.min));
export const textSizeScale=value=>TEXT_SIZE.minScale+(textSizeLevel(value)-1)*TEXT_SIZE.step;
export const textSizePixels=value=>TEXT_SIZE.basePixels*textSizeScale(value);
export function textSizeValue(mark,scale=1){
    const pixels=textPixelSize(mark,1)*(mark.transform?.sx??1)/(mark.fontScale??scale);
    return (pixels/TEXT_SIZE.basePixels-TEXT_SIZE.minScale)/TEXT_SIZE.step+1;
}
export const textSizeLabel=value=>String(Number(value.toPrecision(value>=10?4:3)));
export const fontPoints=value=>Math.max(FONT_SIZE.min,Math.min(FONT_SIZE.max,Number(value)||FONT_SIZE.default));
export const fontPointSize=(mark,scale=1)=>mark.fontUnit==='pt'?fontPoints(mark.fontSize):(mark.fontSize??24)*72/96/(mark.fontScale??scale);
export function textPixelSize(mark,unit=1){
    if(mark.fontUnit!=='pt')return (mark.fontSize??24)/unit;
    return fontPoints(mark.fontSize)*96/72*(mark.fontScale??1)/unit;
}

export const TEXT_OUTLINES=['auto','white','black','custom'];
export const toggleTextOutline=mark=>mark.outlineMode&&mark.outlineMode!=='none'?'none':mark.outlineLastMode??'auto';
export function textOutlineColor(mark){
    if(!mark.outlineMode||mark.outlineMode==='none')return null;
    if(mark.outlineMode==='custom')return mark.outlineColor??'#000000';
    if(mark.outlineMode==='white')return '#ffffff';
    if(mark.outlineMode==='black')return '#000000';
    const [r,g,b]=[1,3,5].map(i=>parseInt(mark.color.slice(i,i+2),16)/255);
    return .2126*r+.7152*g+.0722*b<=.76?'#ffffff':'#000000';
}

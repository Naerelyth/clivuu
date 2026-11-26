import {t} from './i18n.js';
// The only definition of tool capabilities. Both desktop adapters and both
// capture/pin entry points consume this schema and the same AnnotationEditor.
export const END_STYLES = ['none','open','triangle','triangle-outline','circle','circle-outline','diamond','diamond-outline'];
export const LINE_STYLES = ['solid','dash','dot','dash-dot','dash-dot-dot'];
export const MOSAIC_EFFECTS = ['pixelate','blur'];
export const mosaicEffect=value=>value==='blur'?'blur':'pixelate';
export const MOSAIC_LEVEL = {min:3,max:24,default:6};
export const mosaicLevel=value=>Math.max(MOSAIC_LEVEL.min,Math.min(MOSAIC_LEVEL.max,Math.round(Number(value)||MOSAIC_LEVEL.default)));
// Reference pen-width limits at the base image scale. The UI and wheel share
// these image-pixel values; neither desktop adapter defines its own limits.
export const strokeRange=(kind,scale=1)=>({min:['highlight','mosaic','eraser'].includes(kind)?Math.max(1,Math.floor(4*scale)):1,max:Math.max(1,Math.ceil((['pen','highlight','mosaic','eraser'].includes(kind)?50:16)*scale)),step:['pen','highlight','mosaic','eraser'].includes(kind)?Math.max(1,Math.floor(scale)):1});
export const strokeWidth=(kind,value,scale=1)=>{const {min,max}=strokeRange(kind,scale);return Math.max(min,Math.min(max,Number(value)||min));};
export function strokeIndicator(width,available=18){return width<=available?{diameter:Math.max(1,width),text:null}:{diameter:null,text:String(Math.round(width*100)/100)};}
export function fillIcon(kind,fill){
    if(['rect','ellipse'].includes(kind)&&fill==='solid')return {kind:'fill-pattern',filled:false};
    return {kind:fill==='stroke'&&kind==='mosaic'?'brush':kind==='highlight'&&['stroke','line'].includes(fill)?`highlight-${fill==='stroke'?'free':'line'}`:fill==='ellipse'||kind==='ellipse'?'ellipse':'rect',filled:fill!=='stroke'};
}
export function fillLabel(kind,fill){if(kind==='eraser')return fill==='stroke'?t('Freehand eraser'):t('Area eraser');return kind==='highlight'&&fill==='stroke'?t('Freehand highlight'):fill==='line'?t('Line highlight'):({stroke:t('Outline'),solid:t('Filled'),rect:t('Rectangle'),ellipse:t('Ellipse')})[fill];}
export function optionLabel(value){return ({none:t('No marker'),open:t('Open arrow'),triangle:t('Filled arrow'),'triangle-outline':t('Outline arrow'),circle:t('Filled circle'),'circle-outline':t('Outline circle'),diamond:t('Filled diamond'),'diamond-outline':t('Outline diamond'),solid:t('Solid line'),dash:t('Dashed line'),dot:t('Dotted line'),'dash-dot':t('Dash-dot line'),'dash-dot-dot':t('Dash-dot-dot line')})[value]??value;}
export function toolOptions(kind, style={}) {
    return {
        width:kind!=='text', text:kind==='text', arrow:kind==='arrow',
        shapes:['rect','ellipse'].includes(kind)?['rect','ellipse']:[],
        fillToggle:['rect','ellipse'].includes(kind),
        fills:['rect','ellipse'].includes(kind)?['stroke','solid']:kind==='highlight'?['stroke','line','rect','ellipse']:kind==='mosaic'?['stroke','rect','ellipse']:kind==='eraser'?['stroke','rect']:[],
        effects:kind==='mosaic'?MOSAIC_EFFECTS:[],
        color:!['mosaic','eraser'].includes(kind),
        strength:kind==='mosaic',
        lineStyles:kind==='arrow'?LINE_STYLES:[], endStyles:kind==='arrow'?END_STYLES:[],
    };
}
export function endStyle(mark, start=false) {
    return mark[start?'startStyle':'endStyle'] ?? (mark[start?'arrowStart':'arrowEnd'] === (start?true:false) ? (start?'open':'none') : (start?'none':'open'));
}
export const lineDashes = (style,width) => ({dash:[4,3],dot:[1,2], 'dash-dot':[4,2,1,2], 'dash-dot-dot':[4,2,1,2,1,2]}[style]??[]).map(v=>v*width);

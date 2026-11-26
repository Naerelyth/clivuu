import {DRAW_TOOLS} from './annotation-model.js';

// One group definition for capture and pin editing, in both UI adapters.
export function toolbarGroup(action){
    if(action==='grip')return null;
    if(action==='select'||DRAW_TOOLS.includes(action))return 'annotation';
    if(['undo','redo'].includes(action))return 'history';
    if(action==='crop'||action?.startsWith('rotate-')||action?.startsWith('flip-'))return 'transform';
    if(['ocr_clipboard','scroll','record'].includes(action))return 'capture';
    return 'output';
}

export function toolbarActions(actions){
    const order=['ocr_clipboard','scroll','record',null,'replace','pin','quick-save','clipboard'];
    return [...actions].sort((a,b)=>(order.includes(a[0])?order.indexOf(a[0]):99)-(order.includes(b[0])?order.indexOf(b[0]):99));
}

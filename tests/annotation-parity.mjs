// Differential migration test. Production never launches this adapter or Node.
import {readFileSync} from 'node:fs';
import assert from 'node:assert/strict';
import {AnnotationEditor,DRAW_TOOLS,markBounds,markHandles,visibleMarkHandles,mapPoint} from './reference/legacy/common/annotation-model.js';
const fixture=[];
const add=(kind,points,patch={})=>({op:'add',kind,points,patch});
for(const scale of [1,1.5,2]){
    const actions=[add('rect',[[10,20],[100,70]]),add('ellipse',[[20,30],[80,100]]),{op:'undo'},{op:'redo'},
        {op:'wheel',kind:'ellipse',delta:-1},{op:'wheel',kind:'ellipse',delta:-1},{op:'wheel',kind:'ellipse',delta:-1},{op:'undo'},{op:'redo'},
        {op:'select',index:0},{op:'wheel',kind:'rect',delta:-1},{op:'wheel',kind:'rect',delta:1},{op:'undo'},{op:'redo'},
        {op:'select',index:1},{op:'shape',kind:'rect'},{op:'undo'},
        {op:'select',index:0},{op:'begin',point:[10,20],tool:'select'},{op:'move',point:[-10,-5]},{op:'end'},{op:'undo'},{op:'redo'},
        add('arrow',[[10,20],[40,80],[100,100]],{transform:{x:3,y:9,sx:1.3,sy:.8,angle:.3}}),
        {op:'begin',tool:'select',point:[100,100]},{op:'move',point:[120,160],shift:true},{op:'end'},
        add('pen',[[1,2],[3,4],[5,6]]),add('highlight',[[30,40],[50,60]],{fill:'line'}),
        add('mosaic',[[20,20],[200,70]],{fill:'rect'}),{op:'change',kind:'mosaic',patch:{effect:'blur',effectLevel:14}},
        {op:'beginText',point:[40,50]},{op:'updateText',value:'a',time:1},{op:'updateText',value:'ab',time:20},
        {op:'updateText',value:'abc',time:30},{op:'textHistory'},{op:'textHistory',redo:true},
        {op:'updateText',value:'中文\nemoji🙂',time:1400},{op:'preedit',size:[80,50]},{op:'preedit',size:null},
        {op:'commitText'},{op:'wheel',kind:'text',delta:-1},{op:'wheel',kind:'text',delta:-1},{op:'undo'},{op:'redo'},
        {op:'change',kind:'text',patch:{fontSizeValue:1}},{op:'change',kind:'text',patch:{outlineMode:'white',bold:true}},
        {op:'beginText',point:[40,50],index:6},{op:'cancelText'},{op:'remove'},{op:'undo'},{op:'redo'},{op:'clear'}];
    fixture.push({unit:scale,scale,actions});
}
// Reproducible generated histories exercise interleaved element operations,
// freehand paths and scale-dependent widths without copying either algorithm.
let seed=0x2b71;const random=()=>((seed=Math.imul(seed,1664525)+1013904223>>>0)/2**32);
for(let n=0;n<8;n++){
    const actions=[];for(let i=0;i<40;i++){
        const kind=DRAW_TOOLS[Math.floor(random()*7)],choice=Math.floor(random()*7);
        if(choice<3)actions.push(add(kind,[[random()*200,random()*200],[random()*200,random()*200]],{fill:kind==='mosaic'?'rect':'stroke'}));
        else if(choice===3)actions.push({op:'undo'});else if(choice===4)actions.push({op:'redo'});
        else if(choice===5)actions.push({op:'wheel',kind,delta:random()>.5?1:-1});
        else actions.push({op:'change',kind,patch:{width:random()*160,color:'#19af3e'}});
    }fixture.push({unit:1+n%3*.5,scale:1+n%3*.5,actions});
}
let now=0;Date.now=()=>now;
fixture.push({unit:2,scale:2,metrics:true,actions:[
 {op:'beginText',point:[30,20]},{op:'updateText',value:'abc',time:1},{op:'preedit',size:[80,35]},
 {op:'change',kind:'text',patch:{bold:true}},{op:'updateText',value:'中文',time:2},{op:'preedit',size:null},{op:'commitText'},
 {op:'change',kind:'text',patch:{fontFamily:'Wide'}},{op:'change',kind:'text',patch:{fontSize:24}},
 {op:'undo'},{op:'redo'},{op:'beginText',point:[0,0],index:0},{op:'updateText',value:'edited',time:5000},
 {op:'textHistory'},{op:'cancelText'},{op:'undo'}]});
function baseline(input){
 const measure=input.metrics?((m,unit)=>[(m.text.length*(m.fontFamily==='Wide'?12:5)+m.fontSize*3+(m.bold?19:0))/unit,(23+m.fontSize*2)/unit]):null;
 const e=new AnnotationEditor(input.unit,measure,input.scale),states=[];
 for(const a of input.actions){
  switch(a.op){
   case 'add':{const m=e.create(a.kind,a.points[0],a.text??'');m.points=a.points;Object.assign(m,a.patch);e.add(m);break;}
   case 'select':e.select(a.index??-1);break;case 'undo':e.undo();break;case 'redo':e.redo();break;case 'remove':e.remove();break;case 'clear':e.clear();break;
   case 'wheel':e.wheel(a.kind,a.delta);break;case 'change':e.change(a.kind,a.patch);break;case 'shape':e.changeShape(a.kind);break;
   case 'begin':e.begin(a.point,a.tool??'select',a.screenScale??1);break;case 'move':e.move(a.point,a.shift??false);break;case 'end':e.end();break;
   case 'beginText':e.beginText(a.point,a.index??-1);break;case 'updateText':now=a.time;e.updateText(a.value);break;
   case 'commitText':e.commitText();break;case 'cancelText':e.cancelText();break;case 'textHistory':e.textHistory(a.redo??false);break;
   case 'preedit':e.setTextPreedit(a.size);break;
  }
  const state={marks:e.marks,active:e.active,current:e.current??null,canUndo:e.history.canUndo,canRedo:e.history.canRedo,displayMarks:e.displayMarks,
    styles:Object.fromEntries(DRAW_TOOLS.map(k=>{const s={...e.style(k)};delete s.transform;return [k,s];})),hit:e.hit([27,31],6)};
  if(e.current)state.geometry={bounds:markBounds(e.current,input.unit),handles:markHandles(e.current,input.unit),visible:visibleMarkHandles(e.current,[27,31],input.unit),mapped:mapPoint(e.current,[27,31],input.unit),inverse:mapPoint(e.current,[27,31],input.unit,true)};
  states.push(JSON.parse(JSON.stringify(state)));
 }return states;
}
if(process.argv[2]==='--emit'){console.log(fixture.map(i=>JSON.stringify(i)).join('\n'));process.exit(0);}
const actual=readFileSync(process.argv[2],'utf8').trim().split('\n').map(JSON.parse);
let checks=0;
function compare(a,b,path){
 if(path.endsWith('.transform')){a={x:0,y:0,sx:1,sy:1,angle:0,...a};b={x:0,y:0,sx:1,sy:1,angle:0,...b};}
 if(typeof a==='number'&&typeof b==='number'){assert.ok(Math.abs(a-b)<=1e-7*Math.max(1,Math.abs(a),Math.abs(b)),`${path}: ${a} != ${b}`);checks++;return;}
 if(a&&b&&typeof a==='object'&&typeof b==='object'){
  assert.deepEqual(Object.keys(a).sort(),Object.keys(b).sort(),path);for(const key of Object.keys(a))compare(a[key],b[key],`${path}.${key}`);return;
 }assert.equal(a,b,path);checks++;
}
fixture.forEach((input,i)=>compare(actual[i],baseline(input),`scenario ${i}`));
console.log(`Rust/JS annotation parity: ${fixture.length} scenarios, ${fixture.reduce((n,s)=>n+s.actions.length,0)} operations, ${checks} field comparisons passed`);

// Compare positions to the unmodified, previously verified GJS geometry.
import fs from 'node:fs';
import {selectionPanelLayout,annotationPanelPosition} from './reference/legacy/common/geometry.js';
let seed=12345;
const random=()=>{seed=(1664525*seed+1013904223)>>>0;return seed/4294967296;};
const scenarios=[];
for(let i=0;i<1000;i++){
 const viewport={x:Math.floor(random()*4000)-2000,y:Math.floor(random()*2000)-1000,width:Math.floor(random()*2000)+40,height:Math.floor(random()*1200)+40};
 const x=Math.round(random()*(viewport.width-1)),y=Math.round(random()*(viewport.height-1));
 const rect=i%10?{x:viewport.x+x,y:viewport.y+y,width:random()*(viewport.width-x),height:random()*(viewport.height-y)}:{...viewport};
 scenarios.push({rect,viewport,toolbar:{width:300+random()*700,height:32},hint:{width:70+random()*200,height:26},options:{width:150+random()*500,height:32}});
}
if(process.argv[2]==='--emit')for(const c of scenarios)console.log(JSON.stringify(c));
else{
 const actual=fs.readFileSync(process.argv[2],'utf8').trim().split('\n').map(JSON.parse);
 const compare=(a,b,path)=>{if(typeof a==='object'){if(Object.keys(a).sort().join()!==Object.keys(b).sort().join())throw Error(`keys ${path}`);for(const k in a)compare(a[k],b[k],`${path}.${k}`);}else if(typeof a==='number'?Math.abs(a-b)>1e-8:a!==b)throw Error(`${path}: ${a} != ${b}`);};
 if(actual.length!==scenarios.length)throw Error('scenario count');
 scenarios.forEach((c,i)=>{const panels=selectionPanelLayout(c.rect,c.viewport,c.toolbar,c.hint);compare({panels,options:annotationPanelPosition(panels.toolbar,c.viewport,c.options)},actual[i],String(i));});
 console.log(`Toolbar layout parity: ${scenarios.length} independent random/fullscreen/negative-origin cases passed`);
}

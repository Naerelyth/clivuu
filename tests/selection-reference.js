// Headless fixture: calls real old Selection methods, never its GUI constructor.
import GLib from 'gi://GLib';
import Gdk from 'gi://Gdk?version=4.0';
import {Selection} from './reference/legacy/portable/selection.js';
import {bounds,magnifierPixel,magnifierSelectionRect} from './reference/legacy/common/geometry.js';
import {showCaptureMagnifier} from './reference/legacy/common/capture-options.js';
const cases=JSON.parse(new TextDecoder().decode(GLib.file_get_contents(ARGV[0])[1]));
const output=[];
for(const c of cases){
 const s=Object.create(Selection.prototype);
 Object.assign(s,{monitors:c.outputs,windows:c.windows,limit:bounds(c.outputs),selected:!!c.preset,drag:null,pointer:null,level:0,capture:{detectWindows:true},tool:'select',scale:1,
  editor:{textEdit:null,edit:null,begin:()=>false,hit:()=>-1,select:()=>{}},_update(){},_queueUpdate(){},_commitText(){}});
 s.rect={...(c.preset??s.limit)};
 const snapshots=[];
 for(const a of c.actions){
  if(a.op==='motion') {s._motion(a.point);if(!s.drag&&!s.selected)s._hover(a.point);}
  else if(a.op==='begin')s._press(a.point);
  else if(a.op==='end'){if(s.drag){s.selected=true;s.drag=null;}}
  else if(a.op==='scroll')s._scroll(a.delta);
  else if(a.op==='full')s._key(Gdk.KEY_f,0);
  else if(a.op==='cycle')s._key(Gdk.KEY_Tab,a.backwards?Gdk.ModifierType.SHIFT_MASK:0);
  else if(a.op==='nudge'){
   const [x,y]=a.point;const key=x<0?Gdk.KEY_Left:x>0?Gdk.KEY_Right:y<0?Gdk.KEY_Up:Gdk.KEY_Down;
   s._key(key,a.resize?Gdk.ModifierType.SHIFT_MASK:0);
  }
  const pixel=magnifierPixel(s.pointer??[s.rect.x,s.rect.y],s.rect,s.limit,...c.dimensions,s.drag);
  const {x,y,width,height}=s.rect;
  snapshots.push({rect:{x,y,width,height},selected:s.selected,lens:showCaptureMagnifier(s.selected,s.drag),pixel,lensRect:magnifierSelectionRect(s.rect,s.limit,...c.dimensions,...pixel)});
 }
 output.push(snapshots);
}
GLib.file_set_contents(ARGV[1],JSON.stringify(output));

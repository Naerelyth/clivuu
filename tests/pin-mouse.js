// The same real gestures exercise GNOME's X11 adapter and native layer-shell.
import GLib from 'gi://GLib';
import Clutter from 'gi://Clutter';
import {pause} from './common/process.js';
export async function pinMouse({execute,setting,move,pointer,keyboard,check,wait,nativeFocus=false}) {
 const status=()=>execute({command:'status'});
 const mod=(key,pressed)=>keyboard.notify_keyval(GLib.get_monotonic_time(),key,pressed?Clutter.KeyState.PRESSED:Clutter.KeyState.RELEASED);
 const point=async()=>{const r=(await status()).images[0].rect;move(r.x+r.width/2,r.y+r.height/2);await pause(150);};
 const gesture=async(button,twice=false,mods=[])=>{
  await pause(500);await point();for(const key of mods)mod(key,true);
  await pause(100);try{for(let n=0;n<(twice?2:1);n++){pointer.notify_button(GLib.get_monotonic_time(),button,Clutter.ButtonState.PRESSED);await pause(30);pointer.notify_button(GLib.get_monotonic_time(),button,Clutter.ButtonState.RELEASED);await pause(65);}}
  finally{for(const key of mods.toReversed())mod(key,false);}await pause(250);
 };
 const wheel=async(ctrl=false)=>{await point();if(ctrl){mod(Clutter.KEY_Control_L,true);await pause(100);}for(let i=0;i<2;i++){pointer.notify_discrete_scroll(GLib.get_monotonic_time(),ctrl?Clutter.ScrollDirection.DOWN:Clutter.ScrollDirection.UP,Clutter.ScrollSource.WHEEL);await pause(120);}if(ctrl){await pause(100);mod(Clutter.KEY_Control_L,false);}await pause(250);};
 const tap=async(k,mods=[])=>{for(const m of mods)mod(m,true);await pause(80);mod(k,true);mod(k,false);await pause(100);for(const m of mods.toReversed())mod(m,false);await pause(150);};
 const select=async()=>{
  if(nativeFocus){await gesture(1);return;}
  await gesture(3);const b=(await status()).pinUi.menu.buttons.find(b=>b.label==='Show toolbar');move(b.rect.x+b.rect.width/2,b.rect.y+b.rect.height/2);await pause(100);
  pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(40);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(200);
  await tap(Clutter.KEY_1,[Clutter.KEY_Control_L]);await wait(async()=>(await status()).pinUi.toolbar?.tool==='select');
 };
 await setting('pin-confirm-destroy',false);await execute({command:'empty-group'});
 await execute({command:'paste',text:'Pin mouse binding fixture',position:[240,180]});
 let p=(await status()).images[0];const initial=p.rect.width,id=p.id;
 // Logical activation outlives pointer hover and includes editor controls.
 await gesture(1);check('pin click establishes logical activation',(await status()).pinUi.highlighted===id);
 move(950,650);await pause(300);
 check('pointer leaving an activated pin preserves its activation',(await status()).pinUi.highlighted===id);
 await gesture(3);await wait(async()=>(await status()).pinUi.menu);
 const show=(await status()).pinUi.menu.buttons.find(b=>b.label==='Show toolbar');
 move(show.rect.x+show.rect.width/2,show.rect.y+show.rect.height/2);await pause(120);
 check('pin menu hover preserves its activation',(await status()).pinUi.highlighted===id);
 pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(40);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);
 await wait(async()=>(await status()).pinUi.toolbar);
 const selectButton=() => status().then(s=>s.pinUi.toolbar.buttons.find(b=>b.label.startsWith('Cursor (')));
 let button=await selectButton();move(button.rect.x+button.rect.width/2,button.rect.y+button.rect.height/2);await pause(200);
 check('pin toolbar hover preserves its activation',(await status()).pinUi.highlighted===id);
 pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(50);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(250);
 check('pin toolbar click preserves its activation',(await status()).pinUi.highlighted===id);
 move(950,650);await pause(250);check('pointer leaving the open editor preserves pin activation',(await status()).pinUi.highlighted===id);
 pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(70);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);
 await wait(async()=>(await status()).pinUi.highlighted===null);
 check('external focus transfer deactivates the pin without closing its toolbar',!!(await status()).pinUi.toolbar);
 button=await selectButton();move(button.rect.x+button.rect.width/2,button.rect.y+button.rect.height/2);await pause(200);
 check('hovering an inactive toolbar does not activate its pin',(await status()).pinUi.highlighted===null);
 pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(70);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);
 await wait(async()=>(await status()).pinUi.highlighted===id);
 check('clicking an inactive toolbar reactivates its owning pin',true);
 await execute({command:'paste',text:'Second pin selection fixture',position:[720,180]});
 // Importing another pin ends the previous editing session.
 await gesture(3);await wait(async()=>(await status()).pinUi.menu);
 const reopen=(await status()).pinUi.menu.buttons.find(b=>b.label==='Show toolbar').rect;
 move(reopen.x+reopen.width/2,reopen.y+reopen.height/2);await pause(120);
 pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(70);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);
 await wait(async()=>(await status()).pinUi.toolbar);await tap(Clutter.KEY_a,[Clutter.KEY_Control_L]);
 check('Ctrl+A selects both visible pins',(await status()).pinUi.selectedPins.length===2);
 button=await selectButton();move(button.rect.x+button.rect.width/2,button.rect.y+button.rect.height/2);await pause(100);
 pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(70);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(150);
 check('toolbar activation preserves the multiple-pin output selection',(await status()).pinUi.selectedPins.length===2&&(await status()).pinUi.highlighted===id);
 const second=(await status()).images[1].rect;
 await pause(500);move(second.x+second.width/2,second.y+second.height/2);await pause(150);
 mod(Clutter.KEY_Control_L,true);await pause(100);
 for(let n=0;n<2;n++){pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(40);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(70);}
 mod(Clutter.KEY_Control_L,false);
 await wait(async()=>(await status()).pins===1);await gesture(1);
 await gesture(3);await wait(async()=>(await status()).pinUi.menu);await tap(Clutter.KEY_Escape);await wait(async()=>!(await status()).pinUi.menu);
 check('Escape dismisses the pin menu without deactivating its pin',(await status()).pinUi.highlighted===id);
 check('Escape dismisses only the menu while the editor remains open',!!(await status()).pinUi.toolbar);
 await gesture(3);await wait(async()=>(await status()).pinUi.menu);
 const bottom=(await status()).pinUi.menu.buttons.at(-1).rect;move(bottom.x+bottom.width/2,bottom.y+bottom.height/2);await pause(120);
 pointer.notify_button(GLib.get_monotonic_time(),3,Clutter.ButtonState.PRESSED);await pause(60);pointer.notify_button(GLib.get_monotonic_time(),3,Clutter.ButtonState.RELEASED);
 await wait(async()=>!(await status()).pinUi.menu);
 check('right-clicking inside the menu preserves its owning pin activation',(await status()).pinUi.highlighted===id);
 await tap(Clutter.KEY_space);await wait(async()=>!(await status()).pinUi.toolbar);
 check('hiding the toolbar preserves logical pin activation',(await status()).pinUi.highlighted===id);
 for(let cycle=1;cycle<=3;cycle++){
  await gesture(3);await wait(async()=>(await status()).pinUi.menu);move(950,650);await pause(150);
  check(`moving outside menu ${cycle} does not deactivate its pin`,(await status()).pinUi.highlighted===id);
  pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(70);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);
  await wait(async()=>!(await status()).pinUi.menu);
  check(`external menu dismissal ${cycle} deactivates its pin`,(await status()).pinUi.highlighted===null);
 }
 const feedback=async(kind)=>{
  const s=await status(),p=s.images[0],f=s.pinUi.feedback;
  const percent=Math.round((kind==='Size'?p.zoom:p.opacity)*100),r=f.rects[0];
  check(`${kind} feedback reports the current percentage at the pin top left`,f.id===id&&f.text===`${kind}: ${percent}%`&&r&&Math.abs(r.x-p.rect.x-2)<1&&Math.abs(r.y-p.rect.y-2)<1);
 };
 await wheel();check('configured plain wheel scales the pin',(await status()).images[0].rect.width>initial);
 await feedback('Size');await pause(1100);check('percentage feedback expires without further input',(await status()).pinUi.feedback.id===null&&(await status()).pinUi.feedback.rects.length===0);
 const edge=(await status()).images[0].rect;move(edge.x+edge.width-2,edge.y+edge.height/2);await pause(150);
 pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(90);
 move(edge.x+edge.width+30,edge.y+edge.height/2);await pause(200);await feedback('Size');
 pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(120);
 await select();await wheel(true);check('configured Ctrl+wheel changes opacity',(await status()).images[0].opacity<1);
 await feedback('Opacity');await gesture(3);await wait(async()=>(await status()).pinUi.menu?.opacity.length===3);
 const opacityMatches=async()=>{const s=await status();return s.pinUi.menu.opacity[1].label===`Opacity: ${Math.round(s.images[0].opacity*100)}%`;};
 check('opening the menu reads the wheel-adjusted opacity',await opacityMatches());
 for(const [index,delta] of [[2,1],[0,-1],[1,1]]){
  const s=await status(),before=s.images[0].opacity,r=s.pinUi.menu.opacity[index].rect;
  move(r.x+r.width/2,r.y+r.height/2);await pause(100);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(60);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(150);
  check(`opacity menu control ${index} updates opacity and its number together`,await opacityMatches()&&(delta>0?(await status()).images[0].opacity>before:(await status()).images[0].opacity<before));
  await feedback('Opacity');
 }
 await gesture(3);await wait(async()=>!(await status()).pinUi.menu);
 const sized=(await status()).images[0].rect.width;await tap(Clutter.KEY_equal);check('alternative + scales the focused pin',(await status()).images[0].rect.width>sized);
 await setting('pin-mouse-zoom-keys',false);const fixed=(await status()).images[0].rect.width;await tap(Clutter.KEY_equal);check('disabled alternative keys leave pin size unchanged',(await status()).images[0].rect.width===fixed);await setting('pin-mouse-zoom-keys',true);
 await gesture(2);p=(await status()).images[0];check('middle-click resets both size and opacity',Math.abs(p.rect.width-initial)<0.01&&p.opacity===1);
 await gesture(1,true,[Clutter.KEY_Shift_L]);check('Shift+double-click creates a thumbnail',(await status()).images[0].rect.width<=100);
 await gesture(2);check('middle-click restores the thumbnail',(await status()).images[0].rect.width===initial);
 await gesture(3,false,[Clutter.KEY_Shift_L]);const imported=await execute({command:'paste'});
 check('Shift+right-click copies original text without opening a menu',imported.ids?.includes(id)&&(await status()).pins===1&&!(await status()).pinUi.menu);
 await gesture(1,true);await wait(async()=>(await status()).pins===0);check('double-click closes recoverably',(await status()).closedPins===1);
 await execute({command:'reopen-pin'});await select();await gesture(1,true,[Clutter.KEY_Control_L]);await wait(async()=>(await status()).pins===0);
 check('Ctrl+double-click destroys rather than closes',(await status()).closedPins===0);
 await execute({command:'paste',text:'Conflicting mouse bindings',position:[240,180]});
 await select();await setting('pin-mouse-close','alt+right');await setting('pin-mouse-destroy','alt+right');
 await gesture(3,false,[Clutter.KEY_Alt_L]);await wait(async()=>(await status()).pins===0);check('earlier mouse row wins conflicts',(await status()).closedPins===1);
 await execute({command:'reopen-pin'});await select();await setting('pin-mouse-close','none');
 await gesture(3,false,[Clutter.KEY_Alt_L]);await wait(async()=>(await status()).pins===0);check('disabled earlier binding allows the next matching row',(await status()).closedPins===0);
 await setting('pin-mouse-close','double-left');await setting('pin-mouse-destroy','ctrl+double-left');
}

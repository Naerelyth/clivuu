import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Clutter from 'gi://Clutter';
import GdkPixbuf from 'gi://GdkPixbuf';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {pause,run} from './common/process.js';
import {pinMouse} from './pin-mouse.js';
export async function probe(observer) {
 const directory=GLib.getenv('CLIVUU_TEST_DIR'),results=[];let service,fixture;
 const execute=r=>new Promise((resolve,reject)=>Gio.DBus.session.call('io.github.naerelyth.Clivuu.Desktop','/io/github/naerelyth/Clivuu/Desktop','io.github.naerelyth.Clivuu.Desktop1','Execute',new GLib.Variant('(s)',[JSON.stringify({protocol:1,...r})]),null,0,30000,null,(c,r)=>{try{resolve(JSON.parse(c.call_finish(r).deep_unpack()[0]));}catch(e){reject(e);}}));
 const wait=async f=>{for(let i=0;i<240;i++){if(await f())return;await pause(50);}throw new Error('Mouse test timed out');};
 const check=(name,passed)=>{results.push({name,passed:!!passed});GLib.file_set_contents(`${directory}/mouse-progress.json`,JSON.stringify(results));if(!passed)throw new Error(name);};
 try {
  fixture=Gio.Subprocess.new(['gjs','-m',`${directory}/fixture.js`],0);await pause(1800);
  Main.overview.hide();
  let fixtureWindow;await wait(()=>fixtureWindow=global.get_window_actors().map(a=>a.meta_window).find(w=>w.get_title()==='Clivuu focus fixture'));
  fixtureWindow.activate(global.get_current_time());await pause(800);
  const cli=observer._workerPath(),launch=new Gio.SubprocessLauncher({flags:0});launch.setenv('GDK_BACKEND','x11',true);launch.setenv('CLIVUU_LANGUAGE','en',true);launch.set_stderr_file_path(`${directory}/mouse-service.log`);
  if(GLib.getenv('CLIVUU_TEST_APPIMAGE')==='1')service=launch.spawnv([cli]);else service=launch.spawnv([GLib.getenv('CLIVUU_TEST_DESKTOP_BINARY')]);
  await wait(async()=>{try{return(await execute({command:'status'})).ready;}catch{return false;}});
  const pointer=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE),keyboard=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
  const move=(x,y)=>pointer.notify_absolute_motion(GLib.get_monotonic_time(),x,y);
  const setting=async(k,v)=>{await run([cli,'config','set',k,String(v)]);await pause(200);};
  const click=async(r,button=1)=>{move(r.x+r.width/2,r.y+r.height/2);await pause(150);pointer.notify_button(GLib.get_monotonic_time(),button,Clutter.ButtonState.PRESSED);await pause(60);pointer.notify_button(GLib.get_monotonic_time(),button,Clutter.ButtonState.RELEASED);await pause(200);};
  const tap=async(key,ctrl=false)=>{if(ctrl)keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Control_L,Clutter.KeyState.PRESSED);await pause(80);keyboard.notify_keyval(GLib.get_monotonic_time(),key,Clutter.KeyState.PRESSED);keyboard.notify_keyval(GLib.get_monotonic_time(),key,Clutter.KeyState.RELEASED);if(ctrl)keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Control_L,Clutter.KeyState.RELEASED);await pause(200);};
  const status=()=>execute({command:'status'});
  await execute({command:'paste',text:'Default focused pin',position:[240,180]});
  let image=(await status()).images[0].rect;
  await wait(()=>global.display.focus_window?.get_title()==='Clivuu pins');
  check('GNOME pins take keyboard focus by default with the toolbar hidden',!(await status()).pinUi.toolbar);
  await tap(Clutter.KEY_1);await wait(async()=>(await status()).images[0].rect.width===image.height&&(await status()).images[0].rect.height===image.width);
  check('ordinary focused pin receives transform shortcuts',true);
  move(950,650);await pause(200);check('focused pin remains active when the pointer leaves',(await status()).pinUi.highlighted!==null);
  await click({x:950,y:650,width:1,height:1});await wait(async()=>(await status()).pinUi.highlighted===null);
  check('native Wayland outside click deactivates an ordinary GNOME pin',global.display.focus_window===fixtureWindow);
  await click((await status()).images[0].rect);await wait(()=>global.display.focus_window?.get_title()==='Clivuu pins');
  check('clicking the ordinary pin restores keyboard focus',true);
  await setting('pin-confirm-destroy',false);await execute({command:'empty-group'});
  await pinMouse({execute,setting,move,pointer,keyboard,check,wait});
  await run(['python3',`${directory}/x11-root-cursor.py`]);
  await execute({command:'paste',text:'Native popup cursor fixture',position:[350,220]});
  image=(await status()).images[0].rect;
  move(image.x+30,image.y+20);await pause(150);pointer.notify_button(GLib.get_monotonic_time(),3,Clutter.ButtonState.PRESSED);await pause(60);pointer.notify_button(GLib.get_monotonic_time(),3,Clutter.ButtonState.RELEASED);
  let b;await wait(async()=>b=(await execute({command:'status'})).pinUi.menu?.buttons.find(b=>b.label==='Show toolbar'));await click(b.rect);
  keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Control_L,Clutter.KeyState.PRESSED);keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_2,Clutter.KeyState.PRESSED);keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_2,Clutter.KeyState.RELEASED);keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Control_L,Clutter.KeyState.RELEASED);
  for(const label of ['Stroke width (image pixels)','Choose color']){
   await wait(async()=>b=(await execute({command:'status'})).pinUi.toolbar?.options.find(b=>b.label===label));await click(b.rect);
   let pop;await wait(async()=>pop=(await execute({command:'status'})).pinUi.toolbar?.popovers[0]);
   const x=pop.x+pop.width-(label==='Choose color'?45:20),y=label==='Choose color'?pop.y+pop.height-25:pop.y+pop.height/2-8;
   const size=()=>{const s=global.backend.get_cursor_tracker().get_sprite();return s?[s.get_width(),s.get_height()]:[];};
   move(x,y);await pause(180);const hover=size(),held=[];
   pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);
   for(const ms of [20,40,80,160]){await pause(ms);held.push(size());}
   move(pop.x+8,pop.y+8);await pause(80);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(120);
   results.push({name:`${label} native popup keeps the themed cursor before and during a press`,passed:[hover,...held].every(s=>s.length===2&&Math.max(...s)<=64),hover,held});
   keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Escape,Clutter.KeyState.PRESSED);keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Escape,Clutter.KeyState.RELEASED);await pause(200);
  }
  await tap(Clutter.KEY_8,true);await click({x:image.x+20,y:image.y+20,width:1,height:1});await tap(Clutter.KEY_a);
  await wait(async()=>(await status()).pinUi.toolbar.current?.text==='a');
  const blank={x:image.x+image.width-10,y:image.y+image.height-10,width:1,height:1};
  await click(blank,3); // The first secondary click commits/deselects the current text.
  if(!(await status()).pinUi.menu)await click(blank,3);
  await wait(async()=>(await status()).pinUi.menu);
  await tap(Clutter.KEY_9);check('pin menu leaves the underlying text annotation unchanged',(await status()).pinUi.toolbar.marks[0]?.text==='a');
  await tap(Clutter.KEY_Escape);await wait(async()=>!(await status()).pinUi.menu);
  check('menu Escape keeps the text annotation editor open',(await status()).pinUi.toolbar?.marks[0]?.text==='a');
  await tap(Clutter.KEY_2,true);
  keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Shift_L,Clutter.KeyState.PRESSED);await tap(Clutter.KEY_z,true);keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Shift_L,Clutter.KeyState.RELEASED);
  await wait(async()=>(await status()).pinUi.toolbar.marks.length===0);
  // The GNOME opt-out applies to both ordinary pins and the full editor.
  await setting('pin-gnome-focus',false);fixtureWindow.activate(global.get_current_time());await wait(()=>global.display.focus_window===fixtureWindow);await pause(1000);
  const focus=global.display.focus_window,panel=()=>JSON.stringify(global.dashToPanel?.panels.map(p=>p.panelBox.visible)||[]),baseline=panel();
  const focusedOff=()=>global.display.focus_window===focus&&panel()===baseline;
  GLib.file_set_contents(`${directory}/mouse-off-baseline.json`,JSON.stringify({focus:focus?.get_title(),panel:JSON.parse(baseline)}));
  const id=(await status()).images[0].id;
  await click(image);
  check('disabling focus keeps the open editor and its owner active',!!(await status()).pinUi.toolbar&&(await status()).pinUi.highlighted===id&&focusedOff());
  const before=(await status()).pinUi.toolbar.tool;await tap(Clutter.KEY_3,true);await tap(Clutter.KEY_space);
  check('focus-disabled editor ignores keyboard shortcuts',(await status()).pinUi.toolbar?.tool===before&&focusedOff());
  const draw=async(a,b)=>{move(...a);await pause(100);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(100);move(...b);await pause(150);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(150);};
  await draw([image.x+10,image.y+8],[image.x+60,image.y+26]);
  await draw([image.x+80,image.y+8],[image.x+130,image.y+26]);
  await wait(async()=>(await status()).pinUi.toolbar.marks.length===2);
  const toolbarAction=async(label)=>{let b;await wait(async()=>b=(await status()).pinUi.toolbar?.buttons.find(b=>b.label.startsWith(label)));await click(b.rect);};
  await toolbarAction('Undo');check('focus-disabled editor still draws and undoes one mark with the toolbar',(await status()).pinUi.toolbar.marks.length===1&&focusedOff());
  move(950,650);await pause(200);check('focus-disabled editing keeps its owner active',(await status()).pinUi.highlighted===id);
  await toolbarAction('Apply changes');await wait(async()=>!(await status()).pinUi.toolbar);
  await click((await status()).images[0].rect);check('focus-disabled ordinary pin activates on click',(await status()).pinUi.highlighted===id&&focusedOff());
  await tap(Clutter.KEY_1);check('focus-disabled ordinary pin ignores transform shortcuts',JSON.stringify((await status()).images[0].rect)===JSON.stringify(image)&&focusedOff());
  move(950,650);await wait(async()=>(await status()).pinUi.highlighted===null);
  check('focus-disabled ordinary pin deactivates on pointer leave',focusedOff());
  for(let cycle=1;cycle<=3;cycle++){
   await click((await status()).images[0].rect,3);await wait(async()=>(await status()).pinUi.menu);
   let b=(await status()).pinUi.menu.buttons[0];move(b.rect.x+b.rect.width/2,b.rect.y+b.rect.height/2);await pause(120);
   check(`focus-disabled menu ${cycle} keeps panel and focus unchanged`,focusedOff());
   await click({x:950,y:650,width:1,height:1});await wait(async()=>!(await status()).pinUi.menu);
   check(`focus-disabled menu ${cycle} releases its input surface`,(await status()).pinUi.highlighted===null&&focusedOff());
  }
  await click((await status()).images[0].rect,3);await wait(async()=>(await status()).pinUi.menu);
  const last=(await status()).pinUi.menu.buttons.at(-1).rect;await click(last,3);await wait(async()=>!(await status()).pinUi.menu);
  check('focus-disabled in-menu dismissal deactivates when the pointer is outside the pin',(await status()).pinUi.highlighted===null&&focusedOff());
  const first=(await status()).images[0].rect;
  await execute({command:'paste',text:'Adjacent unfocused pin',position:[first.x+first.width-12,first.y]});
  await click({x:first.x+8,y:first.y+8,width:1,height:1});check('adjacent pin fixture activates its first pin',(await status()).pinUi.highlighted===id);
  const neighbor=(await status()).images[1].rect;move(neighbor.x+neighbor.width-10,neighbor.y+neighbor.height/2);await wait(async()=>(await status()).pinUi.highlighted===null);
  check('moving between adjacent pins deactivates the old unfocused pin without a host leave',focusedOff());
  await setting('pin-gnome-focus',true);await click((await status()).images[0].rect);await wait(()=>global.display.focus_window?.get_title()==='Clivuu pins');
  check('re-enabling focus updates the existing pin without reopening it',(await status()).images[0].id===id);
  const sampleMenuBg=async tag=>{
   const item=(await status()).pinUi.menu.buttons.find(b=>b.label==='Recognize text').rect;
   const sx=Math.round(item.x+item.width-8),sy=Math.round(item.y+item.height/2);
   const p=`${directory}/pin-menu-${tag}.png`;
   await observer.captureArea({x:sx,y:sy,width:1,height:1},p);
   return [...GdkPixbuf.Pixbuf.new_from_file(p).get_pixels()].slice(0,3);
  };
  await click((await status()).images[0].rect,3);await wait(async()=>(await status()).pinUi.menu);
  move(950,650);await pause(120);
  await run(['gsettings','set','org.gnome.desktop.interface','color-scheme','prefer-light']);
  let lightMenu=false;for(let n=0;n<60;n++){const rgb=await sampleMenuBg(`light-${n}`);if(rgb[0]>230&&rgb[1]>230&&rgb[2]>235){lightMenu=true;break;}await pause(50);}
  await run(['gsettings','set','org.gnome.desktop.interface','color-scheme','prefer-dark']);
  let darkMenu=false;for(let n=0;n<60;n++){const rgb=await sampleMenuBg(`dark-${n}`);if(rgb[0]<65&&rgb[1]<65&&rgb[2]<75){darkMenu=true;break;}await pause(50);}
  await run(['gsettings','set','org.gnome.desktop.interface','color-scheme','default']);
  let restoredMenu=false;for(let n=0;n<60;n++){const rgb=await sampleMenuBg(`restored-${n}`);if(rgb[0]>230&&rgb[1]>230&&rgb[2]>235){restoredMenu=true;break;}await pause(50);}
  check('pin context menu follows live light and dark color-scheme changes',lightMenu&&darkMenu&&restoredMenu);
  await tap(Clutter.KEY_Escape);await wait(async()=>!(await status()).pinUi.menu);
  await execute({command:'exit'});
 } catch(e) {
  try{GLib.file_set_contents(`${directory}/mouse-failure.json`,JSON.stringify({state:await execute({command:'status'}),pointer:global.get_pointer(),focus:global.display.focus_window?.get_title(),panel:global.dashToPanel?.panels.map(p=>p.panelBox.visible),windows:global.get_window_actors().map(a=>({title:a.meta_window.get_title(),rect:a.meta_window.get_frame_rect()}))}));await observer.captureArea({x:0,y:0,width:1200,height:700},`${directory}/mouse-failure.png`);}catch{}
  results.push({name:'mouse exception',passed:false,error:String(e.stack??e)});
 }finally{service?.force_exit();fixture?.force_exit();GLib.file_set_contents(`${directory}/results.json`,JSON.stringify(results));}
}

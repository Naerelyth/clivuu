// Real Nautilus regression: repeated folder icons, unique labels and fixed chrome.
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Clutter from 'gi://Clutter';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {pause, run} from './common/process.js';
import {saveChooser} from './save-chooser.js';
import {authorizeScreenshot} from './screenshot-access.js';
export async function probe(observer) {
 const directory=GLib.getenv('CLIVUU_TEST_DIR'),results=[],trace=[];let child,files;
 const check=(name,passed)=>{results.push({name,passed:!!passed});if(!passed)throw Error(name);};
 const wait=async fn=>{for(let n=0;n<240;n++){if(await fn())return;await pause(50);}throw Error('Nautilus scrolling timed out');};
 const execute=request=>new Promise((resolve,reject)=>Gio.DBus.session.call('io.github.naerelyth.Clivuu.Desktop','/io/github/naerelyth/Clivuu/Desktop','io.github.naerelyth.Clivuu.Desktop1','Execute',new GLib.Variant('(s)',[JSON.stringify({protocol:1,...request})]),null,Gio.DBusCallFlags.NO_AUTO_START,120000,null,(c,r)=>{try{resolve(JSON.parse(c.call_finish(r).deep_unpack()[0]));}catch(e){reject(e);}}));
 const seat=Clutter.get_default_backend().get_default_seat(),pointer=seat.create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
 const keyboard=seat.create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
 const key=async k=>{keyboard.notify_keyval(GLib.get_monotonic_time(),k,Clutter.KeyState.PRESSED);keyboard.notify_keyval(GLib.get_monotonic_time(),k,Clutter.KeyState.RELEASED);await pause(150);};
 const move=(x,y)=>pointer.notify_absolute_motion(GLib.get_monotonic_time(),x,y);
 const click=async r=>{move(r.x+r.width/2,r.y+r.height/2);await pause(80);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(50);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);};
 try {
  await run(['gsettings','set','org.gnome.nautilus.preferences','default-folder-viewer','icon-view']);
  files=Gio.Subprocess.new(['nautilus','--new-window',`${directory}/nautilus-grid`],0);
  let window;await wait(()=>window=global.get_window_actors().map(a=>a.meta_window).find(w=>w.get_title()==='nautilus-grid'));window.maximize();Main.overview.hide();await pause(1600);
  const launcher=new Gio.SubprocessLauncher({flags:0});launcher.setenv('GDK_BACKEND','x11',true);launcher.setenv('CLIVUU_DESKTOP_DIR','',true);launcher.set_stderr_file_path(`${directory}/grid-service.log`);child=launcher.spawnv([observer._workerPath()]);
  await wait(async()=>{try{return(await execute({command:'status'})).ready;}catch{return false;}});
  await run([observer._workerPath(),'config','set','language','en']);
  await authorizeScreenshot(()=>execute({command:'snip',rect:{x:300,y:200,width:100,height:100},output:'silent'}));
  const rect={x:245,y:0,width:815,height:650};
  for(const [name,label,reverse] of [['save','Save image',false],['copy','Copy image',true],['pin','Pin image',false]]) {
   window.activate(global.get_current_time());move(600,380);await pause(200);
   keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Control_L,Clutter.KeyState.PRESSED);
   await key(reverse?Clutter.KEY_End:Clutter.KEY_Home);
   keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Control_L,Clutter.KeyState.RELEASED);
   await pause(900);
   let pending;
   if(name==='save'||name==='pin') {
    pending=execute({command:'snip'});pending.catch(()=>{});
    await wait(()=>global.get_window_actors().some(a=>a.meta_window.get_title()==='Clivuu capture overlay'));
    move(rect.x,rect.y);await pause(100);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);
    for(let n=1;n<=12;n++){move(rect.x+rect.width*n/12,rect.y+rect.height*n/12);await pause(20);}
    pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(200);await click({x:891,y:662,width:24,height:24});
   } else {pending=execute({command:'scroll',rect});pending.catch(()=>{});}
   await wait(async()=> (await execute({command:'status'})).scrolling?.frames===1);move(600,380);await pause(600);
   const samples=[];
   for(let n=0;n<29;n++) {
    if(n)pointer.notify_discrete_scroll(GLib.get_monotonic_time(),reverse?Clutter.ScrollDirection.UP:Clutter.ScrollDirection.DOWN,Clutter.ScrollSource.WHEEL);
    await pause(600);
    const path=`${directory}/grid-${name}-${String(n).padStart(2,'0')}.png`;
    await observer.captureArea(rect,path);
    // The first live read may still be in flight under software rendering.
    // Synchronize the committed navigation before pairing it with this still;
    // the independent full-row oracle below still checks its exact position.
    if(n)await wait(async()=>{
     const s=(await execute({command:'status'})).scrolling,previous=samples.at(-1).state;
     return s.height!==previous.height||Math.abs(s.position-previous.position)>1e-6;
    });
    samples.push({path,state:(await execute({command:'status'})).scrolling});
   }
   await pause(800);const state=(await execute({command:'status'})).scrolling;
   trace.push({name,reverse,rect,samples,last:state,focus:global.display.focus_window?.get_title(),pointer:global.get_pointer()});GLib.file_set_contents(`${directory}/grid-trace.json`,JSON.stringify(trace));
   check(`${name}: repeated Nautilus grid remains tracked over multiple viewports`,state.height>rect.height*2&&state.tracking&&!state.message);
   if(name==='save')check('capture toolbar starts a live preview and stitched document',state.preview?.width>0&&state.height>rect.height*2);
   if(name==='pin')check('capture editor enters the same scrolling session with a live preview',state.preview?.width>0&&state.height>rect.height*2);
   await click(state.buttons.find(b=>b.label===label).rect);
   const output=`${directory}/grid-${name}-output.png`;
   if(name==='save')await saveChooser(output);
   await pause(1200);
   GLib.file_set_contents(`${directory}/grid-${name}-after.json`,JSON.stringify(await execute({command:'status'})));
   await observer.captureArea(rect,`${directory}/grid-${name}-after.png`);
   await wait(async()=>!(await execute({command:'status'})).busy);const delivered=pending?await pending:{};
   check(`${name}: finishes without a failed terminal overlap`,!delivered.cancelled);
   if(name==='copy')await run(['gjs','-m',`${directory}/clipboard-read.js`,output],null,null,{GDK_BACKEND:'x11'});
   if(name==='pin') {
    let asset;await wait(()=>{try{const base=`${GLib.getenv('XDG_DATA_HOME')}/clivuu/pins`,board=JSON.parse(new TextDecoder().decode(GLib.file_get_contents(`${base}/pins.json`)[1])),pin=board.pins.find(p=>!p.closed&&p.image);asset=pin&&Gio.File.new_for_path(`${base}/${pin.image}`);return asset?.query_exists(null);}catch{return false;}});
    asset.copy(Gio.File.new_for_path(output),Gio.FileCopyFlags.OVERWRITE,null,null);await execute({command:'empty-group'});
   }
  }
  await execute({command:'exit'});
 } catch(e) {try {GLib.file_set_contents(`${directory}/grid-failure.json`,JSON.stringify(await execute({command:'status'})));}catch{}results.push({name:e.message,passed:false,error:e.stack});}
 finally {child?.force_exit();files?.force_exit();GLib.file_set_contents(`${directory}/results.json`,JSON.stringify(results));}
}

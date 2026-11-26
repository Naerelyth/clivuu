// Pin settings through real native preferences, clipboard, surfaces and lifecycle.
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';
import Clutter from 'gi://Clutter';
import GdkPixbuf from 'gi://GdkPixbuf';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {pause,run} from './common/process.js';
export async function probe(observer){
 const directory=GLib.getenv('CLIVUU_TEST_DIR'),results=[];let service,owner,dropTarget;
 const check=(name,passed)=>{results.push({name,passed:!!passed});GLib.file_set_contents(`${directory}/pin-progress.json`,JSON.stringify(results));if(!passed)throw new Error(name);};
 const wait=async f=>{for(let i=0;i<240;i++){if(await f())return;await pause(50);}throw new Error('Pin settings timed out');};
 const execute=r=>new Promise((resolve,reject)=>Gio.DBus.session.call('io.github.naerelyth.Clivuu.Desktop','/io/github/naerelyth/Clivuu/Desktop','io.github.naerelyth.Clivuu.Desktop1','Execute',new GLib.Variant('(s)',[JSON.stringify({protocol:1,...r})]),null,0,30000,null,(c,r)=>{try{resolve(JSON.parse(c.call_finish(r).deep_unpack()[0]));}catch(e){reject(e);}}));
 const status=()=>execute({command:'status'}),paste=r=>execute({command:'paste',...r});
 const pointer=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
 const keyboard=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
 const move=(x,y)=>pointer.notify_absolute_motion(GLib.get_monotonic_time(),x,y);
 const click=async(r,button=1)=>{move(r.x+r.width/2,r.y+r.height/2);await pause(100);pointer.notify_button(GLib.get_monotonic_time(),button,Clutter.ButtonState.PRESSED);await pause(40);pointer.notify_button(GLib.get_monotonic_time(),button,Clutter.ButtonState.RELEASED);await pause(200);};
 const key=async k=>{keyboard.notify_keyval(GLib.get_monotonic_time(),k,Clutter.KeyState.PRESSED);keyboard.notify_keyval(GLib.get_monotonic_time(),k,Clutter.KeyState.RELEASED);await pause(150);};
 const menu=async label=>{const p=(await status()).images.find(p=>!p.closed&&!p.hidden);await click(p.rect,3);await wait(async()=>(await status()).pinUi.menu);const b=(await status()).pinUi.menu.buttons.find(b=>b.label===label);if(!b)throw new Error('missing '+label);await click(b.rect);};
 try{
  Main.overview.hide();
  await run(['/tmp/clivuu-target/release/examples/pin_preferences_probe'],null,null,{GDK_BACKEND:'wayland',G_DEBUG:'fatal-warnings'});
  results.push(...JSON.parse(new TextDecoder().decode(GLib.file_get_contents(`${directory}/pin-ui-results.json`)[1])));
  await run(['/tmp/clivuu-target/release/examples/pin_preferences_probe'],null,null,{GDK_BACKEND:'wayland',G_DEBUG:'fatal-warnings',XDG_CURRENT_DESKTOP:'niri'});
  results.push(...JSON.parse(new TextDecoder().decode(GLib.file_get_contents(`${directory}/pin-ui-results.json`)[1])).map(r=>({...r,name:`non-GNOME settings: ${r.name}`})));

  const cli=observer._workerPath();const setting=async(k,v)=>{await run([cli,'config','set',k,typeof v==='string'?v:JSON.stringify(v)]);await pause(160);};
  await setting('pin-flash-new',false);await setting('pin-confirm-destroy',false);
  const launcher=new Gio.SubprocessLauncher({flags:0});launcher.setenv('GDK_BACKEND','x11',true);launcher.setenv('CLIVUU_LANGUAGE','en',true);launcher.set_stderr_file_path(`${directory}/pin-service.log`);
  if(GLib.getenv('CLIVUU_TEST_APPIMAGE')==='1'){launcher.setenv('CLIVUU_DESKTOP_DIR','',true);service=launcher.spawnv([cli]);}else service=launcher.spawnv([GLib.getenv('CLIVUU_TEST_DESKTOP_BINARY')]);
  await wait(async()=>{try{return(await status()).ready;}catch{return false;}});
  const own=new Gio.SubprocessLauncher({flags:0});own.setenv('GDK_BACKEND','x11',true);owner=own.spawnv(['gjs','-m',`${directory}/clipboard-owner.js`]);
  const publish=async formats=>{const serial=GLib.get_monotonic_time();GLib.file_set_contents(`${directory}/clipboard-request.json`,JSON.stringify({serial,formats}));await wait(()=>{try{return new TextDecoder().decode(GLib.file_get_contents(`${directory}/clipboard-ready`)[1])===String(serial);}catch{return false;}});};
  const clear=()=>execute({command:'empty-group'});
  await setting('pin-opacity',63);await setting('pin-shadow',false);await setting('pin-smooth-scaling',false);await setting('pin-max-size',400);
  const image=GdkPixbuf.Pixbuf.new(GdkPixbuf.Colorspace.RGB,false,8,800,400);image.fill(0xaabbccff);image.savev(`${directory}/pin-source.png`,'png',[],[]);
  const a=await paste({files:[`${directory}/pin-source.png`],position:[100,100]});let s=await status(),p=s.images.find(p=>p.id===a.ids[0]);
  check('new pin applies opacity shadow interpolation and maximum size',p.opacity===0.63&&!p.shadow&&!p.smooth&&p.rect.width===400);
  await setting('pin-opacity',100);await setting('pin-shadow',true);check('defaults do not rewrite existing pin appearance',(await status()).images[0].opacity===0.63&&!(await status()).images[0].shadow);
  await setting('pin-max-size',300);check('maximum size also constrains an existing pin',(await status()).images[0].rect.width===300);await setting('pin-max-size',400);
  await setting('pin-thumbnail-width',80);await setting('pin-thumbnail-height',60);await menu('Thumbnail / restore');check('thumbnail fits configured bounds',(await status()).images[0].rect.width===80);
  await setting('pin-reset-thumbnail',false);await menu('Reset size');check('disabled thumbnail reset leaves thumbnail size',(await status()).images[0].rect.width===80);
  await setting('pin-reset-thumbnail',true);await menu('Reset size');check('enabled thumbnail reset restores bounded original size',(await status()).images[0].rect.width===400);await clear();
  const highlighted=await paste({files:[`${directory}/pin-source.png`],position:[100,100]});
  check('new pin is logically active for accent-shadow testing',(await status()).pinUi.highlighted===highlighted.ids[0]);
  const shadowPixel=async(name)=>{move(5,750);await pause(250);const path=`${directory}/shadow-${name}.png`;await observer.captureArea({x:95,y:150,width:1,height:1},path);return [...GdkPixbuf.Pixbuf.new_from_file(path).get_pixels()].slice(0,3);};
  await setting('accent-color','#ff0000');const redShadow=await shadowPixel('red');
  await setting('accent-color','#00ff00');const greenShadow=await shadowPixel('green');
  check('active pin shadow follows live accent changes rather than a fixed blue',redShadow[0]>greenShadow[0]&&greenShadow[1]>redShadow[1]);
  await setting('accent-color','#bd93f9');await clear();
  await setting('pin-text-enabled',false);await publish([{mime:'text/plain',text:'disabled text'}]);check('disabled text conversion produces no pin',(await paste({})).pinned===0&&(await status()).pins===0);
  await publish([{mime:'text/plain',text:'#ffffff'}]);check('color cards remain independent of text conversion',(await paste({})).pinned===1);await clear();await setting('pin-text-enabled',true);
  GLib.file_set_contents(`${directory}/notes.txt`,'notes');await publish([{mime:'text/uri-list',text:Gio.File.new_for_path(`${directory}/notes.txt`).get_uri()},{mime:'text/plain',text:`${directory}/notes.txt`}]);
  check('file path exclusion suppresses file manager text fallback',(await paste({})).pinned===0);await setting('pin-text-skip-paths',false);check('file path conversion can be enabled',(await paste({})).pinned===1);await clear();
  const html='<span style="color:red;font-size:40pt">AaOo12</span>';
  await setting('pin-text-ignore-html',false);await paste({text:html,html:true});const rich=(await status()).images[0].rect;await clear();
  await setting('pin-text-ignore-html',true);await setting('pin-text-margin',30);await setting('pin-text-font','Sans 12');await paste({text:html,html:true});check('plain conversion uses configured font and margin rather than HTML styling',(await status()).images[0].rect.height<rich.height);await clear();
  await setting('pin-closed-limit',1);await paste({text:'first',position:[100,100]});await menu('Close pin');await paste({text:'second',position:[100,100]});await menu('Close pin');check('closing another pin prunes the oldest recoverable image',(await status()).closedPins===1);await execute({command:'reopen-pin'});check('newest closed pin remains recoverable',(await status()).pins===1);await clear();
  await setting('pin-flash-new',true);await setting('pin-hide-on-flash',true);await paste({text:'flash-hide'});const hidden=await paste({text:'must not create'});check('paste while flashing hides all pins instead of importing',hidden.hidden&&(await status()).pins===1&&(await status()).images[0].hidden===false);await execute({command:'show-images'});await setting('pin-hide-on-flash',false);await setting('pin-flash-new',false);await clear();
  await paste({text:'one',position:[100,100]});await paste({text:'two',position:[300,200]});
  const pending=execute({command:'close-pins'});let response;pending.then(r=>response=r);await wait(()=>global.get_window_actors().some(a=>a.meta_window.get_title()==='Close these pins?'));
  const confirmation=global.get_window_actors().map(a=>a.meta_window).find(w=>w.get_title()==='Close these pins?');check('bulk confirmation is native Wayland',confirmation.get_client_type()===Meta.WindowClientType.WAYLAND);confirmation.activate(global.get_current_time());await pause(250);await key(Clutter.KEY_Escape);await pending;check('bulk confirmation cancel preserves all pins',response.cancelled&&(await status()).pins===2);
  await setting('pin-confirm-close',false);await execute({command:'close-pins'});check('confirmed bulk close uses the configured retention policy',(await status()).pins===0&&(await status()).closedPins===1);
  await setting('pin-closed-limit',3);await execute({command:'reopen-pin'});await paste({text:'confirmation-race'});await execute({command:'close-pins'});await setting('pin-confirm-destroy',true);
  const destruction=execute({command:'destroy-closed-pins'});await wait(()=>global.get_window_actors().some(a=>a.meta_window.get_title()==='Destroy these pins?'));
  await execute({command:'reopen-pin'});const warning=global.get_window_actors().map(a=>a.meta_window).find(w=>w.get_title()==='Destroy these pins?');warning.activate(global.get_current_time());await pause(200);
  const wr=warning.get_frame_rect();await click({x:wr.x+wr.width*0.75,y:wr.y+wr.height-45,width:1,height:1});
  const destroyed=await destruction;check('bulk destroy revalidates closed status after confirmation',destroyed.destroyed===1&&(await status()).pins===1&&(await status()).closedPins===0);await setting('pin-confirm-destroy',false);await clear();
  const launch=new Gio.SubprocessLauncher({flags:0});launch.setenv('GDK_BACKEND','wayland',true);dropTarget=launch.spawnv(['/tmp/clivuu-target/release/examples/pin_drop_probe']);
  await wait(()=>global.get_window_actors().some(a=>a.meta_window.get_title()==='Clivuu drag source fixture'));
  const source=global.get_window_actors().map(a=>a.meta_window).find(w=>w.get_title()==='Clivuu drag source fixture');source.move_frame(false,700,300);await pause(300);
  for(let n=0;n<2;n++){
   await setting('pin-preserve-drag',n===1);await paste({text:'drop destination',position:[100,100]});const original=(await status()).images.at(-1);
   GLib.file_set_contents(`${directory}/pin-drop-mime`,n===0?'image/png':'text/uri-list');
   const r=source.get_frame_rect(),from=[r.x+r.width/2,r.y+r.height/2],to=[original.rect.x+original.rect.width/2,original.rect.y+original.rect.height/2];
   move(...from);await pause(200);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(120);
   for(let i=1;i<=25;i++){move(from[0]+(to[0]-from[0])*i/25,from[1]+(to[1]-from[1])*i/25);await pause(30);}await pause(300);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);
   await wait(async()=>(await status()).images.length===2);const images=(await status()).images;
   check('incoming '+(n===0?'PNG':'URI')+' uses shared import and physical dimensions',images.at(-1).rect.width===400&&images.at(-1).rect.height===200);
   check('incoming drop preserves destination according to setting '+n,images.find(p=>p.id===original.id).hidden===(n===0));await clear();
  }
  dropTarget.force_exit();dropTarget=null;
  await execute({command:'exit'});
 }catch(e){try{GLib.file_set_contents(`${directory}/pin-failure.json`,JSON.stringify(await status()));await observer.captureArea({x:0,y:0,width:1200,height:700},`${directory}/pin-failure.png`);}catch{}results.push({name:'pin preferences exception',passed:false,error:`${e.message}\n${e.stack}`});}
 finally{dropTarget?.force_exit();owner?.force_exit();service?.force_exit();GLib.file_set_contents(`${directory}/results.json`,JSON.stringify(results));}
}

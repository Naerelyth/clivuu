// Real compositor cursor pixels and source teardown, never a fabricated pointer.
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Clutter from 'gi://Clutter';
import Meta from 'gi://Meta';
import GdkPixbuf from 'gi://GdkPixbuf';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {pause,run} from './common/process.js';
import {authorizeScreenshot} from './screenshot-access.js';
export async function probe(observer){
 const directory=GLib.getenv('CLIVUU_TEST_DIR'),results=[],handles=[];let child,fixture,signal;
 const check=(name,passed)=>{results.push({name,passed:!!passed});GLib.file_set_contents(`${directory}/cursor-progress.json`,JSON.stringify(results));if(!passed)throw new Error(name);};
 const wait=async(predicate,label,attempts=300)=>{for(let n=0;n<attempts;n++){if(await predicate())return;await pause(50);}throw new Error(`Cursor capture timed out: ${label}`);};
 const execute=r=>new Promise((resolve,reject)=>Gio.DBus.session.call('io.github.naerelyth.Clivuu.Desktop','/io/github/naerelyth/Clivuu/Desktop','io.github.naerelyth.Clivuu.Desktop1','Execute',new GLib.Variant('(s)',[JSON.stringify({protocol:1,...r})]),null,0,120000,null,(c,res)=>{try{resolve(JSON.parse(c.call_finish(res).deep_unpack()[0]));}catch(e){reject(e);}}));
 const pointer=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
 const keyboard=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
 const move=async(x,y)=>{pointer.notify_absolute_motion(GLib.get_monotonic_time(),x,y);await pause(200);};
 const overlay=()=>global.get_window_actors().some(a=>a.meta_window.get_title()==='Clivuu capture overlay');
 try{
  signal=global.backend.get_remote_access_controller().connect('new-handle',(_,handle)=>{const state={handle,stopped:false};handle.connect('stopped',()=>state.stopped=true);handles.push(state);});
  GLib.file_set_contents(`${directory}/fixture-color`,'#2468ac');fixture=Gio.Subprocess.new(['gjs','-m',`${directory}/fixture.js`],0);await pause(1200);Main.overview.hide();await pause(300);
  const window=global.get_window_actors().map(a=>a.meta_window).find(w=>w.get_title()==='Clivuu focus fixture');check('cursor oracle is a native Wayland application',window.get_client_type()===Meta.WindowClientType.WAYLAND);window.activate(global.get_current_time());await pause(300);
  const packaged=GLib.getenv('CLIVUU_TEST_APPIMAGE')==='1',cli=packaged?observer._workerPath():'/tmp/clivuu-target/release/clivuu';
  const launcher=new Gio.SubprocessLauncher({flags:0});launcher.setenv('GDK_BACKEND','x11',true);launcher.setenv('CLIVUU_LANGUAGE','en',true);launcher.set_stderr_file_path(`${directory}/cursor-service.log`);
  if(packaged){launcher.setenv('CLIVUU_DESKTOP_DIR','',true);child=launcher.spawnv([cli]);}else child=launcher.spawnv(['/tmp/clivuu-target/release/clivuu-desktop']);
  await wait(async()=>{try{return(await execute({command:'status'})).ready;}catch{return false;}},'service');
  const capture=async(name)=>execute({command:'snip',mode:'full',output:`${directory}/${name}.png`});
  await authorizeScreenshot(()=>capture('static-baseline'));
  check('ordinary screenshot creates no sharing stream',handles.length===0);
  await run([cli,'config','set','capture-cursor','true']);
  const cases=[];
  for(const [x,y] of [[470,330],[820,540],[320,590]]){
   window.activate(global.get_current_time());await pause(100);await move(x,y);
   const name=`cursor-${x}-${y}`,before=handles.length;await capture(name);
   await wait(()=>handles.slice(before).every(h=>h.stopped),'one-frame sharing session closed');
   const image=GdkPixbuf.Pixbuf.new_from_file(`${directory}/${name}.png`),data=image.get_pixels();
   const sx=image.width/global.screen_width,sy=image.height/global.screen_height;
   let count=0,minX=Infinity,minY=Infinity,maxX=-1,maxY=-1;
   for(let py=Math.round(180*sy);py<Math.round(650*sy);py++)for(let px=Math.round(100*sx);px<Math.round(1150*sx);px++){
    const i=py*image.rowstride+px*image.n_channels;
    if(Math.max(Math.abs(data[i]-36),Math.abs(data[i+1]-104),Math.abs(data[i+2]-172))>12){count++;minX=Math.min(minX,px);minY=Math.min(minY,py);maxX=Math.max(maxX,px);maxY=Math.max(maxY,py);}
   }
   cases.push({x,y,width:image.width,height:image.height,scale:[sx,sy],count,bounds:[minX,minY,maxX,maxY],handles:handles.length-before});
   check(`native pointer is embedded at its real position (${x},${y})`,count>0&&minX<=x*sx+20&&maxX>=x*sx-20&&minY<=y*sy+20&&maxY>=y*sy-20&&(maxX-minX)<80*sx&&(maxY-minY)<80*sy);
   check(`pointer capture closes its sharing handle before returning (${x},${y})`,handles.length===before+1&&handles.at(-1).stopped);
  }
  if(Main.layoutManager.monitors.length>1){
   const monitor=Main.layoutManager.monitors[1];window.move_to_monitor(1);window.activate(global.get_current_time());await pause(350);
   const x=monitor.x+300,y=monitor.y+300;await move(x,y);const before=handles.length;await capture('cursor-second-monitor');
   const image=GdkPixbuf.Pixbuf.new_from_file(`${directory}/cursor-second-monitor.png`),data=image.get_pixels(),sx=image.width/global.screen_width,sy=image.height/global.screen_height;
   let count=0;for(let py=Math.round((y-24)*sy);py<Math.round((y+30)*sy);py++)for(let px=Math.round((x-20)*sx);px<Math.round((x+20)*sx);px++){
    const i=py*image.rowstride+px*image.n_channels;if(Math.max(Math.abs(data[i]-36),Math.abs(data[i+1]-104),Math.abs(data[i+2]-172))>12)count++;
   }
   cases.push({name:'second-monitor',x,y,count,width:image.width,height:image.height});
   check('embedded pointer uses the second monitor compositor origin',count>50&&count<2000);
   await wait(()=>handles.slice(before).every(h=>h.stopped),'second monitor source closed');
  }
  GLib.file_set_contents(`${directory}/cursor-pixels.json`,JSON.stringify(cases,null,2));
  const dragArea=async(x1,y1,x2,y2)=>{
   pointer.notify_absolute_motion(GLib.get_monotonic_time(),x1,y1);await pause(100);
   pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(80);
   for(let i=1;i<=6;i++){pointer.notify_absolute_motion(GLib.get_monotonic_time(),x1+(x2-x1)*i/6,y1+(y2-y1)*i/6);await pause(20);}
   pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(200);
  };
  const countCursorPixels=path=>{
   const img=GdkPixbuf.Pixbuf.new_from_file(path),d=img.get_pixels();let n=0,minX=Infinity,minY=Infinity,maxX=-1,maxY=-1;
   for(let py=0;py<img.height;py++)for(let px=0;px<img.width;px++){
    const i=py*img.rowstride+px*img.n_channels;
    if(Math.max(Math.abs(d[i]-36),Math.abs(d[i+1]-104),Math.abs(d[i+2]-172))>12){n++;minX=Math.min(minX,px);minY=Math.min(minY,py);maxX=Math.max(maxX,px);maxY=Math.max(maxY,py);}
   }
   return {n,minX,minY,maxX,maxY,sx:img.width/240,sy:img.height/240};
  };
  window.activate(global.get_current_time());await pause(100);await move(520,380);
  const beforeInteractive=handles.length;
  const hiddenOut=`${directory}/cursor-hidden-post-snip.png`;
  const pendingHide=execute({command:'snip',output:hiddenOut});
  await wait(overlay,'selector after pointer frame (hide)');
  check('interactive pointer capture uses one sharing session and stops it before selection begins',handles.length===beforeInteractive+1&&handles.every(h=>h.stopped));
  await wait(async()=>{const s=await execute({command:'status'});return s.captureWindows.some(w=>w.mapped&&w.active)&&s.captureCursor?.canToggle&&s.captureCursor?.visible===true;},'cursor toggle ready');
  await dragArea(400,260,640,500);
  keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_grave,Clutter.KeyState.PRESSED);
  keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_grave,Clutter.KeyState.RELEASED);
  await wait(async()=>(await execute({command:'status'})).captureCursor?.visible===false,'cursor hidden via backtick');
  keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Return,Clutter.KeyState.PRESSED);
  keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Return,Clutter.KeyState.RELEASED);
  await pendingHide;
  check('hiding captured cursor after selection exports clean un-cursored background pixels',countCursorPixels(hiddenOut).n===0);

  window.activate(global.get_current_time());await pause(100);await move(520,380);
  const restoredOut=`${directory}/cursor-restored-post-snip.png`;
  const pendingRestore=execute({command:'snip',output:restoredOut});
  await wait(overlay,'selector after pointer frame (restore)');
  await wait(async()=>(await execute({command:'status'})).captureWindows.some(w=>w.mapped&&w.active),'selector ready (restore)');
  await dragArea(400,260,640,500);
  keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_grave,Clutter.KeyState.PRESSED);
  keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_grave,Clutter.KeyState.RELEASED);
  await wait(async()=>(await execute({command:'status'})).captureCursor?.visible===false,'cursor hidden before restore');
  keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Shift_L,Clutter.KeyState.PRESSED);
  keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_exclam,Clutter.KeyState.PRESSED);
  keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_exclam,Clutter.KeyState.RELEASED);
  keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Shift_L,Clutter.KeyState.RELEASED);
  await wait(async()=>(await execute({command:'status'})).captureCursor?.visible===true,'cursor restored via !');
  keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Return,Clutter.KeyState.PRESSED);
  keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Return,Clutter.KeyState.RELEASED);
  await pendingRestore;
  const restored=countCursorPixels(restoredOut);
  check('re-showing captured cursor via ! exports the real compositor cursor at its exact capture position',restored.n>0&&restored.minX<=120*restored.sx+20&&restored.maxX>=120*restored.sx-20&&restored.minY<=120*restored.sy+20&&restored.maxY>=120*restored.sy-20);

  const pending=execute({command:'snip'});pending.catch(()=>{});await wait(overlay,'selector after pointer frame');
  check('pointer capture stops sharing before selection begins',handles.every(h=>h.stopped));
  await wait(async()=>(await execute({command:'status'})).captureWindows.some(w=>w.mapped&&w.active),'selector ready for keyboard');
  keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Escape,Clutter.KeyState.PRESSED);keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Escape,Clutter.KeyState.RELEASED);
  check('pointer capture selector cancels normally',(await pending).cancelled);
  await run([cli,'config','set','capture-cursor','false']);const before=handles.length;await capture('static-after');
  check('turning pointer capture off returns to static Screenshot',handles.length===before);
  await execute({command:'exit'});
 }catch(e){results.push({name:'cursor capture exception',passed:false,error:`${e.message}\n${e.stack}`});}
 finally{if(signal)global.backend.get_remote_access_controller().disconnect(signal);child?.force_exit();fixture?.force_exit();GLib.file_set_contents(`${directory}/results.json`,JSON.stringify(results));}
}

// Preferences are real GTK windows. Launcher fixtures use standard desktop
// metadata only, so both native packages and renamed portable installs apply.
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import Meta from 'gi://Meta';
import Clutter from 'gi://Clutter';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {pause,run} from './common/process.js';
export async function probe(observer){
 const directory=GLib.getenv('CLIVUU_TEST_DIR'),results=[];let child;
 const check=(name,passed)=>{results.push({name,passed:!!passed});GLib.file_set_contents(`${directory}/preferences-progress.json`,JSON.stringify(results));if(!passed)throw new Error(name);};
 const wait=async(predicate,label,attempts=200)=>{for(let n=0;n<attempts;n++){if(await predicate())return;await pause(50);}throw new Error(`Preferences timed out: ${label}`);};
 const windows=()=>global.get_window_actors().map(a=>a.meta_window).filter(w=>['Clivuu Preferences','Clivuu 设置'].includes(w.get_title()));
 const appId=w=>Shell.WindowTracker.get_default().get_window_app(w)?.get_id();
 const applications=`${GLib.getenv('XDG_DATA_HOME')}/applications`;
 const icon=`${GLib.getenv('CLIVUU_DESKTOP_DIR')}/icons/clivoo.svg`;
 GLib.mkdir_with_parents(applications,0o700);
 const launcher=(id,wmclass)=>GLib.file_set_contents(`${applications}/${id}`,`[Desktop Entry]\nType=Application\nName=Clivuu\nExec=/tmp/clivuu-target/release/clivuu\nIcon=${icon}\n${wmclass?'StartupWMClass=io.github.naerelyth.clivuu\n':''}`);
 const execute=r=>new Promise((resolve,reject)=>Gio.DBus.session.call('io.github.naerelyth.Clivuu.Desktop','/io/github/naerelyth/Clivuu/Desktop','io.github.naerelyth.Clivuu.Desktop1','Execute',new GLib.Variant('(s)',[JSON.stringify({protocol:1,...r})]),null,0,30000,null,(c,res)=>{try{resolve(JSON.parse(c.call_finish(res).deep_unpack()[0]));}catch(e){reject(e);}}));
 try{
  Main.overview.hide();await pause(400);
  for(const [backend,id,wmclass] of [['wayland','io.github.naerelyth.clivuu.desktop',true]]){
   if(backend==='wayland')GLib.unlink(`${applications}/io.github.naerelyth.clivuu.desktop`);
   launcher(id,wmclass);await wait(()=>Shell.AppSystem.get_default().lookup_app(id),'launcher registration');
   const prefix=`preferences-${backend}`;
   const probe=run(['/tmp/clivuu-target/release/examples/preferences_probe'],null,null,{GDK_BACKEND:backend,G_DEBUG:'fatal-warnings'});
   let failed;probe.catch(e=>failed=e);
   await wait(()=>failed||GLib.file_test(`${directory}/${prefix}-ready`,GLib.FileTest.EXISTS),'GTK checks',500);if(failed)throw failed;
   GLib.file_set_contents(`${directory}/${prefix}-identity.json`,JSON.stringify(global.get_window_actors().map(a=>({title:a.meta_window.get_title(),wmclass:a.meta_window.get_wm_class(),gtk:a.meta_window.get_gtk_application_id(),app:appId(a.meta_window)}))));
   await wait(()=>windows().length===1&&appId(windows()[0])===id,'taskbar association');
   const app=Shell.WindowTracker.get_default().get_window_app(windows()[0]);
   check(`${backend} preferences associate with the installed desktop file`,app.get_id()===id);
   check(`${backend} taskbar uses the installed Clivuu icon`,app.get_app_info().get_icon().to_string()===icon);
   const pointer=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
   const move=(x,y)=>pointer.notify_absolute_motion(GLib.get_monotonic_time(),x,y);
   const button=down=>pointer.notify_button(GLib.get_monotonic_time(),1,down?Clutter.ButtonState.PRESSED:Clutter.ButtonState.RELEASED);
   const input=()=>JSON.parse(new TextDecoder().decode(GLib.file_get_contents(`${directory}/${prefix}-input.json`)[1]));
   await wait(()=>GLib.file_test(`${directory}/${prefix}-input.json`,GLib.FileTest.EXISTS),'cursor observation');
   const w=windows()[0];w.activate(global.get_current_time());await pause(200);
   const tracker=global.backend.get_cursor_tracker();
   const sprite=async name=>{
    const s=tracker.get_sprite();if(!s)return null;
    const stream=Gio.MemoryOutputStream.new_resizable();
    const image=await new Promise((resolve,reject)=>Shell.Screenshot.composite_to_stream(s,0,0,s.get_width(),s.get_height(),1,null,0,0,1,stream,(_,r)=>{try{resolve(Shell.Screenshot.composite_to_stream_finish(r));}catch(e){reject(e);}}));
    stream.close(null);if(name)GLib.file_set_contents(`${directory}/${prefix}-cursor-${name}.png`,stream.steal_as_bytes().get_data());
    return [s.get_width(),s.get_height(),...tracker.get_hot(),GLib.compute_checksum_for_data(GLib.ChecksumType.SHA256,image.get_pixels())];
   };
   let r=w.get_frame_rect();move(r.x+250,r.y+90);await pause(200);const normalSprite=await sprite('normal');
   move(r.x+r.width+2,r.y+r.height/2);await pause(200);const borderSprite=await sprite('border');
   check(`${backend} resize cursor differs visibly from the normal cursor`,JSON.stringify(borderSprite)!==JSON.stringify(normalSprite));
   const border=input();button(true);await pause(100);
   for(let n=1;n<=8;n++){move(r.x+r.width+2+80*n/8,r.y+r.height/2);await pause(30);}
   button(false);await pause(250);r=w.get_frame_rect();
   move(r.x+250,r.y+90);await pause(250);const inside=input();
   GLib.file_set_contents(`${directory}/${prefix}-cursor-baseline.json`,JSON.stringify({border,inside,frame:{x:r.x,y:r.y,width:r.width,height:r.height},globalPointer:global.get_pointer(),normalSprite,borderSprite,insideSprite:await sprite('inside')}));
   check(`${backend} native border drag actually resizes preferences`,inside.width>border.width+40);
   check(`${backend} internal hover clears the resize cursor without clicking`,JSON.stringify(await sprite())===JSON.stringify(normalSprite));
   for(const [x,y] of [[500,250],[500,550],[250,90]]){move(r.x+x,r.y+y);await pause(180);check(`${backend} continued internal hover keeps the normal cursor (${x},${y})`,JSON.stringify(await sprite())===JSON.stringify(normalSprite));}
   if(GLib.getenv('CLIVUU_TEST_MONITOR_SIZE')){
    const trace=[];
    for(const [name,fx,fy,dx,dy] of [['east',1,.5,30,0],['west',0,.5,-30,0],['south',.5,1,0,30],['north',.5,0,0,-30],['south-east',1,1,20,20],['north-west',0,0,-20,-20],['south-west',0,1,-20,20],['north-east',1,0,20,-20]]){
     r=w.get_frame_rect();const x=r.x+(fx===0?-2:fx===1?r.width+2:r.width/2),y=r.y+(fy===0?-2:fy===1?r.height+2:r.height/2);
     move(x,y);await pause(150);const edge=await sprite(name+'-edge');button(true);await pause(80);
     for(let n=1;n<=6;n++){move(x+dx*n/6,y+dy*n/6);await pause(25);}
     button(false);await pause(150);const resized=w.get_frame_rect();
     move(resized.x+250,resized.y+90);await pause(180);const actual=await sprite(name+'-inside');
     trace.push({name,before:{x:r.x,y:r.y,width:r.width,height:r.height},after:{x:resized.x,y:resized.y,width:resized.width,height:resized.height},edge,actual,input:input()});
     GLib.file_set_contents(`${directory}/${prefix}-resize-trace.json`,JSON.stringify(trace));
     check(`${backend} ${name} native resize changes geometry`,r.width!==resized.width||r.height!==resized.height);
     check(`${backend} ${name} resize restores the actual cursor pixels on internal hover`,JSON.stringify(actual)===JSON.stringify(normalSprite));
    }
    r=w.get_frame_rect();
   }
   const save=input().save;move(r.x+save.x+save.width/2,r.y+save.y+save.height/2);await pause(100);button(true);button(false);await pause(250);
   const entry=input().manual_text;move(r.x+entry.x+entry.width/2,r.y+entry.y+entry.height/2);await pause(100);button(true);button(false);await pause(200);
   move(r.x+entry.x+10,r.y+entry.y+entry.height/2);await pause(250);
   check(`${backend} text fields keep the text cursor after resize`,input().cursor==='text');
   GLib.file_set_contents(`${directory}/${prefix}-observed`,'observed');await probe;
   const state=JSON.parse(new TextDecoder().decode(GLib.file_get_contents(`${directory}/${prefix}.json`)[1]));
   for(const field of ['settings_font_scope_and_fallback','shared_rgba_color_dialog','startup_os_state','native_language_names','live_language','inline_accent_reset','section_scoped_reset','reset_confirmation','save_sections_reset','filename_tip_naming_rules','save_sections_live_language','selectable_naming_rules','concise_reset_confirmation','reset_removes_override','fresh_general_window','naming_help_initially_unselected'])check(`${backend} ${field}`,state[field]);
  }
  GLib.unlink(`${applications}/io.github.naerelyth.clivuu.desktop`);
  launcher('clivuu.desktop',false);await wait(()=>Shell.AppSystem.get_default().lookup_app('clivuu.desktop'),'renamed launcher');
  const packaged=GLib.getenv('CLIVUU_TEST_APPIMAGE')==='1';
  const cli=packaged?observer._workerPath():'/tmp/clivuu-target/release/clivuu';
  const launch=new Gio.SubprocessLauncher({flags:0});launch.setenv('GDK_BACKEND','x11',true);launch.setenv('CLIVUU_LANGUAGE','en',true);launch.set_stderr_file_path(`${directory}/preferences-service.log`);
  if(packaged){launch.setenv('CLIVUU_DESKTOP_DIR','',true);child=launch.spawnv([cli]);}else child=launch.spawnv(['/tmp/clivuu-target/release/clivuu-desktop']);
  await wait(async()=>{try{return(await execute({command:'status'})).ready;}catch{return false;}},'service');
  await execute({command:'open-preferences'});await wait(()=>windows().length===1,'first window');
  const first=windows()[0],sequence=first.get_stable_sequence();
  check('X11 pin service opens native Wayland preferences',first.get_client_type()===Meta.WindowClientType.WAYLAND);
  check('service preferences use renamed installed launcher',appId(first)==='clivuu.desktop');
  await execute({command:'open-preferences'});check('reopening while visible presents one preferences window',windows().length===1);
  const keyboard=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
  const key=async k=>{keyboard.notify_keyval(GLib.get_monotonic_time(),k,Clutter.KeyState.PRESSED);keyboard.notify_keyval(GLib.get_monotonic_time(),k,Clutter.KeyState.RELEASED);await pause(180);};
  const pointer=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
  first.activate(global.get_current_time());await pause(400);
  let r=first.get_frame_rect();
  pointer.notify_absolute_motion(GLib.get_monotonic_time(),r.x+820,r.y+138);await pause(80);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(180);
  await key(Clutter.KEY_Escape);check('Escape dismisses a settings combo before closing the window',windows().length===1);
  await key(Clutter.KEY_Escape);await wait(()=>windows().length===0,'Escape closes preferences');check('Escape closes independent preferences normally',true);
  await execute({command:'open-preferences'});await wait(()=>windows().length===1,'reopen after Escape');
  // Change page and window geometry, then close through the window manager.
  const current=windows()[0];r=current.get_frame_rect();
  pointer.notify_absolute_motion(GLib.get_monotonic_time(),r.x+95,r.y+160);await pause(60);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(200);
  current.move_resize_frame(false,30,30,1100,650);await pause(200);current.delete(global.get_current_time());await wait(()=>windows().length===0,'destroy closed window');
  await execute({command:'open-preferences'});await wait(()=>windows().length===1&&windows()[0].get_stable_sequence()!==sequence,'fresh window');
  check('service destroys closed preferences and creates a fresh independent window',windows()[0].get_stable_sequence()!==sequence&&windows()[0].get_transient_for()===null);
  check('service discards resized window geometry',windows()[0].get_frame_rect().width!==1100);
  for(let n=0;n<5;n++){
   windows()[0].delete(global.get_current_time());await wait(()=>windows().length===0,'rapid close');
   await execute({command:'open-preferences'});await wait(()=>windows().length===1,'rapid reopen');
  }
  check('five immediate close and reopen cycles survive process ownership transitions',true);
  const edited=windows()[0];edited.activate(global.get_current_time());
  await wait(()=>global.get_window_actors().some(a=>a.meta_window===edited&&a.mapped),'preferences maps before editing');await pause(100);
  await wait(()=>global.get_window_actors().some(a=>a.meta_window===edited&&a.mapped&&a.opacity===255&&Math.abs(a.scale_x-1)<.001&&Math.abs(a.scale_y-1)<.001),'preferences map animation');r=edited.get_frame_rect();
  const ocr=JSON.parse(new TextDecoder().decode(GLib.file_get_contents(`${directory}/preferences-wayland-ocr-layout.json`)[1]));
  const tab=ocr.tab;
  pointer.notify_absolute_motion(GLib.get_monotonic_time(),r.x+tab.x+tab.width/2,r.y+tab.y+tab.height/2);await pause(80);
  pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(200);
  pointer.notify_absolute_motion(GLib.get_monotonic_time(),r.x+500,r.y+400);await pause(60);
  for(let s=0;s<8;s++){pointer.notify_discrete_scroll(GLib.get_monotonic_time(),Clutter.ScrollDirection.DOWN,Clutter.ScrollSource.WHEEL);await pause(25);}
  await pause(300);
  GLib.file_set_contents(`${directory}/pending-edit-input.json`,JSON.stringify({frame:{x:r.x,y:r.y,width:r.width,height:r.height},ocr,point:[r.x+ocr.x+ocr.width/2,r.y+ocr.y+ocr.height/2]}));
  await observer.captureArea({x:0,y:0,width:1280,height:800},`${directory}/preferences-before-pending-edit.png`);
  pointer.notify_absolute_motion(GLib.get_monotonic_time(),r.x+ocr.x+ocr.width/2,r.y+ocr.y+ocr.height/2);await pause(80);
  pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(100);
  keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Control_L,Clutter.KeyState.PRESSED);await key(Clutter.KEY_a);keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Control_L,Clutter.KeyState.RELEASED);
  for(const k of [Clutter.KEY_h,Clutter.KEY_t,Clutter.KEY_t,Clutter.KEY_p,Clutter.KEY_s,Clutter.KEY_colon,Clutter.KEY_slash,Clutter.KEY_slash,Clutter.KEY_a,Clutter.KEY_period,Clutter.KEY_b])await key(k);
  keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Return,Clutter.KeyState.PRESSED);keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Return,Clutter.KeyState.RELEASED);
  await observer.captureArea({x:0,y:0,width:1280,height:800},`${directory}/preferences-after-pending-edit.png`);
  // Deliver the virtual key event before the compositor's close request,
  // still well inside the configuration writer's 120 ms coalescing window.
  await pause(30);edited.delete(global.get_current_time());await wait(()=>windows().length===0,'close immediately after apply');
  await wait(async()=>JSON.parse(await run([cli,'config','get','ocr-download-mirror']))==='https://a.b','preferences process flushes pending edit');
  check('closing native preferences immediately after apply preserves shared configuration',true);
  await execute({command:'open-preferences'});await wait(()=>windows().length===1,'reopen after pending write');
  await execute({command:'exit'});await wait(()=>windows().length===0,'service closes preferences cleanly');
  check('service shutdown closes independent preferences',true);
 }catch(e){results.push({name:'preferences exception',passed:false,error:`${e.message}\n${e.stack}`});}
 finally{child?.force_exit();GLib.file_set_contents(`${directory}/results.json`,JSON.stringify(results));}
}

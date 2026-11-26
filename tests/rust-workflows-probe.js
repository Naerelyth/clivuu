// End-to-end entry/confirmation/output tests: no rectangle or destination override.
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Clutter from 'gi://Clutter';
import Meta from 'gi://Meta';
import GdkPixbuf from 'gi://GdkPixbuf';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {pause,run} from './common/process.js';
import {authorizeScreenshot} from './screenshot-access.js';
import {saveChooser} from './save-chooser.js';
export async function probe(observer){
 const directory=GLib.getenv('CLIVUU_TEST_DIR'),results=[];let child,fixture,handleSignal;const handles=[],stoppedHandles=new Set();
 const check=(name,passed)=>{results.push({name,passed:!!passed});GLib.file_set_contents(`${directory}/workflow-progress.json`,JSON.stringify(results));if(!passed)throw new Error(name);};
 const wait=async(predicate,label='condition')=>{for(let n=0;n<200;n++){if(await predicate())return;await pause(60);}throw new Error(`Workflow timed out: ${label}`);};
 const BUS='io.github.naerelyth.Clivuu.Desktop',PATH='/io/github/naerelyth/Clivuu/Desktop',IFACE=`${BUS}1`;
 const call=(path,iface,method,args,timeout=30000)=>new Promise((resolve,reject)=>Gio.DBus.session.call(BUS,path,iface,method,args,null,Gio.DBusCallFlags.NO_AUTO_START,timeout,null,(c,r)=>{try{resolve(c.call_finish(r).deep_unpack());}catch(e){reject(e);}}));
 const execute=async r=>JSON.parse((await call(PATH,IFACE,'Execute',new GLib.Variant('(s)',[JSON.stringify({protocol:1,...r})]),120000))[0]);
 const status=()=>execute({command:'status'});
 const tray=async label=>{const [items]=await call('/MenuBar','com.canonical.dbusmenu','GetGroupProperties',new GLib.Variant('(aias)',[[],[]]));const item=items.find(([,props])=>props.label?.deep_unpack()===label);if(!item)throw new Error(`Missing tray entry: ${label}`);await call('/MenuBar','com.canonical.dbusmenu','Event',new GLib.Variant('(isvu)',[item[0],'clicked',new GLib.Variant('i',0),0]));};
 const pointer=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE),keyboard=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
 const move=(x,y)=>pointer.notify_absolute_motion(GLib.get_monotonic_time(),x,y);
 const click=async(x,y,b=1)=>{move(x,y);await pause(80);pointer.notify_button(GLib.get_monotonic_time(),b,Clutter.ButtonState.PRESSED);await pause(60);pointer.notify_button(GLib.get_monotonic_time(),b,Clutter.ButtonState.RELEASED);await pause(120);};
 const drag=async(a,b)=>{move(...a);await pause(100);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(60);for(let n=1;n<=10;n++){move(a[0]+(b[0]-a[0])*n/10,a[1]+(b[1]-a[1])*n/10);await pause(20);}pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(150);};
 const key=async(k,ctrl=false)=>{if(ctrl)keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Control_L,Clutter.KeyState.PRESSED);keyboard.notify_keyval(GLib.get_monotonic_time(),k,Clutter.KeyState.PRESSED);keyboard.notify_keyval(GLib.get_monotonic_time(),k,Clutter.KeyState.RELEASED);if(ctrl)keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Control_L,Clutter.KeyState.RELEASED);await pause(120);};
 const overlay=()=>global.get_window_actors().some(a=>a.meta_window.get_title()==='Clivuu capture overlay');
 const action=async label=>{let b;await wait(async()=>b=(await status()).scrolling?.buttons.find(b=>b.label===label&&b.rect.width>0),label);const r=b.rect;await click(r.x+r.width/2,r.y+r.height/2);};
 const clipboard=async name=>{const path=`${directory}/${name}.png`;await run(['gjs','-m',`${directory}/clipboard-read.js`,path],null,null,{GDK_BACKEND:'x11'});return GdkPixbuf.Pixbuf.new_from_file(path);};
 try{
  handleSignal=global.backend.get_remote_access_controller().connect('new-handle',(_,handle)=>{handles.push(handle);handle.connect('stopped',()=>stoppedHandles.add(handle));});
  GLib.file_set_contents(`${directory}/fixture-color`,'#2468ac');fixture=Gio.Subprocess.new(['gjs','-m',`${directory}/fixture.js`],0);await pause(1700);Main.overview.hide();await pause(500);
  const window=global.get_window_actors().map(a=>a.meta_window).find(w=>w.get_title()==='Clivuu focus fixture');window.activate(global.get_current_time());await pause(500);
  const packaged=GLib.getenv('CLIVUU_TEST_APPIMAGE')==='1';const cli=observer._workerPath();const desktop=GLib.getenv('CLIVUU_TEST_DESKTOP_BINARY')||GLib.build_filenamev([GLib.path_get_dirname(cli),'clivuu-desktop']);
  const launcher=new Gio.SubprocessLauncher({flags:0});launcher.unsetenv('APPIMAGE');launcher.setenv('GDK_BACKEND','x11',true);launcher.setenv('CLIVUU_LANGUAGE','en',true);launcher.set_stderr_file_path(`${directory}/rust-workflows.log`);if(packaged){launcher.setenv('CLIVUU_DESKTOP_DIR','',true);child=launcher.spawnv([cli]);}else child=launcher.spawnv([desktop]);await wait(async()=>{try{return(await status()).ready;}catch{return false;}});
  // Settings use a separate native display, then the pin service continues.
  await execute({command:'open-preferences'});let preferences;await wait(()=>preferences=global.get_window_actors().map(a=>a.meta_window).find(w=>w.get_title()==='Clivuu Preferences'),'preferences before captures');
  preferences.delete(global.get_current_time());await wait(()=>!global.get_window_actors().some(a=>a.meta_window===preferences),'preferences destroyed');window.activate(global.get_current_time());await pause(200);
  check('closing settings leaves the service ready for capture and pin workflows',!(await status()).busy);
  // Grant the first static request through the real portal permission UI.
  await authorizeScreenshot(()=>execute({command:'snip',rect:{x:100,y:160,width:500,height:300},output:'silent'}));
  await execute({command:'open-preferences'});
  await wait(()=>preferences=global.get_window_actors().map(a=>a.meta_window).find(w=>w.get_title()==='Clivuu Preferences'),'Wayland preferences');
  check('settings remain native Wayland beside GNOME X11 pins',preferences.get_client_type()===Meta.WindowClientType.WAYLAND);
  preferences.activate(global.get_current_time());await pause(250);
  const pr=preferences.get_frame_rect();await click(pr.x+820,pr.y+138);
  const fromSettings=execute({command:'snip'});fromSettings.catch(()=>{});
  await wait(overlay,'capture with settings popup open');await drag([180,180],[700,480]);await key(Clutter.KEY_Escape);
  check('capture dismisses native settings popup and accepts input immediately',(await fromSettings).cancelled&&!(await status()).busy);
  preferences.delete(global.get_current_time());await wait(()=>!global.get_window_actors().some(a=>a.meta_window===preferences),'preferences closed after capture');
  window.activate(global.get_current_time());await pause(200);
  const [entries]=await call('/MenuBar','com.canonical.dbusmenu','GetGroupProperties',new GLib.Variant('(aias)',[[],[]]));
  const labels=entries.map(([,props])=>props.label?.deep_unpack()).filter(Boolean);
  check('tray removes scrolling capture while keeping capture, recording and preferences entries',!labels.includes('Scrolling capture')&&['Capture','Window capture','Region recording','Stop recording','Preferences','Quit'].every(label=>labels.includes(label)));
  const picker=()=>global.get_window_actors().map(a=>a.meta_window).find(w=>w.get_title()==='Share Screen');
  const sharingStopped=()=>handles.every(h=>stoppedHandles.has(h));
  await tray('Window capture');await wait(picker,'system window sharing picker');
  check('tray window capture opens only the single-window sharing picker',!overlay()&&!Main.screenshotUI.visible&&sharingStopped());
  await wait(()=>global.display.focus_window===picker(),'initial picker focus');await pause(250);await key(Clutter.KEY_Escape);await wait(async()=>!(await status()).busy,'cancel system window picker');
  check('cancelling system window selection leaves the service ready',!overlay());
  window.unmaximize(Meta.MaximizeFlags.BOTH);window.move_resize_frame(false,160,120,620,440);await pause(600);
  const frame=window.get_frame_rect(),buffer=window.get_buffer_rect();
  const chooseWindow=async()=>{
   let dialog;await wait(()=>dialog=picker(),'window sharing selection');await wait(()=>global.display.focus_window===dialog,'picker focus');await pause(350);
   const r=dialog.get_frame_rect();await observer.captureArea(r,`${directory}/window-picker.png`);
   await click(r.x+r.width/2,r.y+145);await pause(200);await click(r.x+r.width-50,r.y+24);
   await wait(()=>!picker(),'picker accepted');
  };
  const nativePath=`${directory}/streamed-window.png`;
  const directWindow=execute({command:'snip',mode:'active-window',output:nativePath});directWindow.catch(()=>{});await chooseWindow();await directWindow;
  const nativeWindow=GdkPixbuf.Pixbuf.new_from_file(nativePath);
  GLib.file_set_contents(`${directory}/window-capture-details.json`,JSON.stringify({frame,current:window.get_frame_rect(),buffer:window.get_buffer_rect(),image:[nativeWindow.width,nativeWindow.height],handles:handles.length,stopped:sharingStopped(),overlay:overlay()}));
  check('one-frame window sharing exports the selected window pixels and ends the stream',[[frame.width,frame.height],[buffer.width,buffer.height]].some(([w,h])=>nativeWindow.width===w&&nativeWindow.height===h)&&!overlay()&&handles.length===1&&sharingStopped()&&!Main.screenshotUI.visible);
  // Cursor preference is passed to the compositor, with no XFixes overlay.
  await run([cli,'config','set','capture-cursor','true']);
  const editedWindow=execute({command:'snip',mode:'window'});editedWindow.catch(()=>{});await chooseWindow();await wait(overlay,'shared window image editor');check('window sharing is already closed when the editor appears',sharingStopped()&&handles.length===2);
  keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Control_L,Clutter.KeyState.PRESSED);await key(Clutter.KEY_2);keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Control_L,Clutter.KeyState.RELEASED);await drag([450,300],[650,400]);
  keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Control_L,Clutter.KeyState.PRESSED);await key(Clutter.KEY_z);keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Control_L,Clutter.KeyState.RELEASED);
  await key(Clutter.KEY_Return);await editedWindow;const edited=await clipboard('edited-window');
  check('window stream stops before shared annotation undo and default confirmation',edited.width===nativeWindow.width&&edited.height===nativeWindow.height&&edited.get_pixels().slice(240*edited.rowstride+240*edited.n_channels,240*edited.rowstride+240*edited.n_channels+3).join(',')==='36,104,172'&&sharingStopped());
  const pinnedWindow=execute({command:'snip',mode:'window'});pinnedWindow.catch(()=>{});await chooseWindow();await wait(overlay,'window editor pin');await key(Clutter.KEY_t,true);await pinnedWindow;
  const windowPin=(await status()).images.at(-1);
  check('window editor Pin uses the shared output path',windowPin.rect.width===edited.width&&windowPin.rect.height===edited.height&&(await status()).pins===1&&sharingStopped());
  await execute({command:'empty-group'});await wait(async()=>(await status()).pins===0,'remove window pin');
  const savedWindow=execute({command:'snip',mode:'window'});savedWindow.catch(()=>{});await chooseWindow();await wait(overlay,'window editor save');await key(Clutter.KEY_s,true);
  await saveChooser(`${directory}/edited-window-save.png`);const windowFile=await savedWindow;const savedPixels=GdkPixbuf.Pixbuf.new_from_file(windowFile.path);
  check('window editor Save uses the shared native Wayland chooser',savedPixels.width===edited.width&&savedPixels.height===edited.height&&!overlay()&&sharingStopped());
  const cancelledWindow=execute({command:'snip',mode:'window'});cancelledWindow.catch(()=>{});await chooseWindow();await wait(overlay,'window editor cancel');await key(Clutter.KEY_Escape);
  check('Escape from the window editor cancels and tears down all capture surfaces',(await cancelledWindow).cancelled&&!overlay()&&!(await status()).busy);
  const pictures=GLib.build_filenamev([GLib.getenv('HOME'),'Pictures']);check('window sharing never invokes system Screenshot UI or saves a system screenshot',!Main.screenshotUI.visible&&!Gio.File.new_for_path(pictures).query_exists(null));
  await run([cli,'config','set','capture-cursor','false']);window.maximize(Meta.MaximizeFlags.BOTH);window.activate(global.get_current_time());await pause(500);
  await tray('Capture');await wait(overlay,'ordinary selector before toolbar long capture');await run([cli,'config','set','theme','light']);await pause(250);await drag([180,180],[850,580]);await key(Clutter.KEY_l);await wait(async()=>(await status()).scrolling?.frames>=1,'enter long capture');
  check('screenshot toolbar action starts a real long-capture session with the current region',!overlay()&&(await status()).scrolling.width===670&&!(await status()).recording);
  // The completed first viewport must export despite later source changes.
  GLib.file_set_contents(`${directory}/fixture-color`,'#ad3824');await pause(400);
  const pinStart=GLib.get_monotonic_time();await action('Pin image');await wait(async()=>!(await status()).busy,'pin long capture');let state=await status();
  check('long capture Pin without an output override finishes and creates a pin',state.pins===1&&!state.scrolling);
  check('complete long capture output does not wait for a new capture reader',(GLib.get_monotonic_time()-pinStart)/1e6<2);
  let rect=state.images[0].rect;check('long pin retains its selected origin and dimensions',rect.x===180&&rect.y===180&&rect.width===670&&rect.height===400);
  const menu=async label=>{await click(rect.x+40,rect.y+40,3);let b;await wait(async()=>b=(await status()).pinUi.menu?.buttons.find(b=>b.label===label));await click(b.rect.x+b.rect.width/2,b.rect.y+b.rect.height/2);};
  await menu('Copy image');const pinned=await clipboard('committed-long');check('export uses committed image pixels rather than a new changed viewport',pinned.get_pixels().slice(0,3).join(',')==='36,104,172');
  await click(rect.x+40,rect.y+40);check('clicking a pin activates its desktop keyboard context',(await status()).pinUi.highlighted!==null&&global.display.focus_window?.get_title()==='Clivuu pins');
  move(1150,700);await pause(300);check('leaving a focused XWayland pin preserves activation',(await status()).pins===1&&(await status()).pinUi.highlighted!==null);
  await menu('Close pin');await wait(async()=>(await status()).pins===0); // closing is separate from destroying
  GLib.file_set_contents(`${directory}/fixture-color`,'#2468ac');await pause(300);
  for(const [label,name] of [['Copy image','copy'],['Save image','save'],[null,'quick save']]){
   let pendingError;const pending=execute({command:'scroll',rect:{x:100,y:160,width:500,height:300}});pending.catch(e=>pendingError=e);await wait(async()=>(await status()).scrolling?.frames>=1,`start ${name}`);
   if(label)await action(label);else {keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Control_L,Clutter.KeyState.PRESSED);keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Shift_L,Clutter.KeyState.PRESSED);await key(Clutter.KEY_S);keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Shift_L,Clutter.KeyState.RELEASED);keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Control_L,Clutter.KeyState.RELEASED);}
   if(name==='save')await saveChooser(`${directory}/workflow-save.png`);
   await wait(async()=>!(await status()).busy,`finish ${name}`);if(pendingError)throw pendingError;const result=await pending;
   const image=name==='copy'?await clipboard('workflow-copy'):GdkPixbuf.Pixbuf.new_from_file(result.path);
   check(`long capture ${name} without an output override exports successfully`,image.width===500&&image.height===300&&!(await status()).scrolling);
  }
  const selected=execute({command:'scroll'});selected.catch(()=>{});await wait(overlay,'CLI scrolling region selector');await key(Clutter.KEY_Escape);
  check('CLI scrolling without a rectangle retains its shared region selector', (await selected).cancelled&&!(await status()).busy);
  const retained=execute({command:'scroll',rect:{x:100,y:160,width:500,height:300}});retained.catch(()=>{});await wait(async()=>(await status()).scrolling?.frames>=1,'capture before source revocation');
  const last=handles.at(-1);if(!last?.stop)throw new Error('No real remote-access handle available to revoke');last.stop();
  await wait(async()=>{const s=(await status()).scrolling;return s&&!s.capturing&&s.message;},'recoverable source revocation');
  check('revoking the capture source preserves the committed image',(await status()).busy&&(await status()).scrolling.frames>=1);
  await action('Copy image');await retained;check('committed long capture remains exportable after source permission ends',!(await status()).busy&&!(await status()).scrolling);
  await tray('Region recording');await wait(overlay,'recording region selector');await drag([250,220],[650,470]);await key(Clutter.KEY_Return);await wait(async()=>(await status()).recording,'recording start');
  check('tray recording confirmation starts encoding rather than copying a screenshot',!overlay()&&!(await status()).scrolling);
  await pause(700);await tray('Stop recording');await wait(async()=>!(await status()).recording,'recording stop');
  // Stop returns the retained completed destination even if the tray stopped first.
  const stopped=await execute({command:'stop-recording'});const media=JSON.parse(await run(['ffprobe','-v','error','-show_entries','stream=width,height','-of','json',stopped.path]));
  check('tray region recording encodes exactly the selected dimensions',media.streams[0].width===400&&media.streams[0].height===250);
  const beforeRecordingRestart=await status(),restartVideo=`${directory}/restart-recording.webm`;
  await execute({command:'record',rect:{x:250,y:220,width:400,height:250},output:restartVideo});await pause(800);
  await execute({command:'restart'});await wait(async()=>{try{const s=await status();return s.ready&&s.pid!==beforeRecordingRestart.pid;}catch{return false;}},'recording restart');
  const restartedMedia=JSON.parse(await run(['ffprobe','-v','error','-show_entries','stream=width,height','-of','json',restartVideo]));
  check('restart finalizes the active recording before replacing the service',restartedMedia.streams[0].width===400&&restartedMedia.streams[0].height===250&&!(await status()).recording);
  await tray('Capture');await wait(overlay,'regular capture');await drag([140,170],[540,420]);await key(Clutter.KEY_Return);await wait(async()=>!(await status()).busy,'ordinary capture output');const normal=await clipboard('normal-capture');check('regular tray capture still confirms to a static clipboard image',normal.width===400&&normal.height===250&&!(await status()).recording&&!(await status()).scrolling);
  check('pin surfaces remain override-redirect rather than managed desktop windows',global.get_window_actors().map(a=>a.meta_window).filter(w=>w.get_title()==='Clivuu pins').every(w=>w.get_window_type()===Meta.WindowType.OVERRIDE_OTHER));
  await execute({command:'exit'});
 }catch(e){try{GLib.file_set_contents(`${directory}/workflow-failure.json`,JSON.stringify(await status()));}catch{}results.push({name:'native workflow exception',passed:false,error:`${e.message}\n${e.stack}`});}
 finally{if(handleSignal)global.backend.get_remote_access_controller().disconnect(handleSignal);child?.force_exit();fixture?.force_exit();GLib.file_set_contents(`${directory}/results.json`,JSON.stringify(results));}
}

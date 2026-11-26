// Real static portal + real platform hosts in the private desktop. No Shell
// capture API/runtime is supplied to the service; Shell only observes/input.
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Clutter from 'gi://Clutter';
import Meta from 'gi://Meta';
import GdkPixbuf from 'gi://GdkPixbuf';
import Shell from 'gi://Shell';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {pause,run} from './common/process.js';
import {saveChooser} from './save-chooser.js';
import {authorizeScreenshot} from './screenshot-access.js';
const BUS='io.github.naerelyth.Clivuu.Desktop',PATH='/io/github/naerelyth/Clivuu/Desktop',IFACE=`${BUS}1`;
export async function probe(observer){
 const directory=GLib.getenv('CLIVUU_TEST_DIR'),results=[];let child,fixture,signal,legacy,handles=0,legacyCalls=0;
 const check=(name,passed)=>{results.push({name,passed:Boolean(passed)});GLib.file_set_contents(`${directory}/native-service-progress.json`,JSON.stringify(results));if(!passed)throw new Error(name);};
 const wait=async predicate=>{for(let i=0;i<180;i++){if(await predicate())return;await pause(50);}throw new Error('Native service timed out');};
 const execute=(request)=>new Promise((resolve,reject)=>Gio.DBus.session.call(BUS,PATH,IFACE,'Execute',new GLib.Variant('(s)',[JSON.stringify({protocol:1,...request})]),null,Gio.DBusCallFlags.NO_AUTO_START,130000,null,(c,r)=>{try{resolve(JSON.parse(c.call_finish(r).deep_unpack()[0]));}catch(e){reject(e);}}));
 const overlays=()=>global.get_window_actors().map(a=>a.meta_window).filter(w=>w.get_title()==='Clivuu capture overlay');
 const pointer=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
 const keyboard=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
 const move=(x,y)=>pointer.notify_absolute_motion(GLib.get_monotonic_time(),x,y);
 const button=pressed=>pointer.notify_button(GLib.get_monotonic_time(),1,pressed?Clutter.ButtonState.PRESSED:Clutter.ButtonState.RELEASED);
 const key=async(k,control=false)=>{if(control)keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Control_L,Clutter.KeyState.PRESSED);keyboard.notify_keyval(GLib.get_monotonic_time(),k,Clutter.KeyState.PRESSED);keyboard.notify_keyval(GLib.get_monotonic_time(),k,Clutter.KeyState.RELEASED);if(control)keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Control_L,Clutter.KeyState.RELEASED);await pause(150);};
 const drag=async(a,b)=>{move(...a);await pause(120);button(true);await pause(80);for(let i=1;i<=12;i++){move(a[0]+(b[0]-a[0])*i/12,a[1]+(b[1]-a[1])*i/12);await pause(16);}button(false);await pause(200);};
 const readiness=[];
 const inputReady=async()=>{await wait(()=>overlays().length===1&&global.display.focus_window===overlays()[0]);let first=true;await wait(async()=>{const state=(await execute({command:'status'})).captureWindows;if(first){readiness.push(state);GLib.file_set_contents(`${directory}/input-readiness.json`,JSON.stringify(readiness));first=false;}return state?.some(w=>w.mapped&&w.active&&w.focus==='ClivuuEditorScene');});};
 const image=path=>GdkPixbuf.Pixbuf.new_from_file(path);
 const pixelCache=new WeakMap();
 const pixel=(p,x,y)=>{let data=pixelCache.get(p);if(!data){data={bytes:p.get_pixels(),stride:p.rowstride,channels:p.n_channels};pixelCache.set(p,data);}const offset=y*data.stride+x*data.channels;return [...data.bytes.slice(offset,offset+3)];};
 try{
  legacy=Gio.DBusExportedObject.wrapJSObject(`<node><interface name="io.github.naerelyth.Clivuu.Shell1"><method name="Configure"><arg type="s" direction="in"/><arg type="s" direction="out"/></method><method name="State"><arg type="s" direction="out"/></method><signal name="Available"/></interface></node>`,{
   Configure(){legacyCalls++;return JSON.stringify({protocol:1});},State(){legacyCalls++;return JSON.stringify({protocol:1,windows:[]});}
  });legacy.export(Gio.DBus.session,'/io/github/naerelyth/Clivuu/Shell');
  signal=global.backend.get_remote_access_controller().connect('new-handle',()=>handles++);
  GLib.file_set_contents(`${directory}/fixture-color`,'#2468ac');
  fixture=Gio.Subprocess.new(['gjs','-m',`${directory}/fixture.js`],Gio.SubprocessFlags.NONE);await pause(1600);Main.overview.hide();await pause(400);
  const launcher=new Gio.SubprocessLauncher({flags:0});launcher.unsetenv('APPIMAGE');launcher.setenv('GDK_BACKEND','x11',true);launcher.unsetenv('CLIVUU_LANGUAGE');launcher.set_stderr_file_path(`${directory}/rust-service.log`);
  child=launcher.spawnv([GLib.getenv('CLIVUU_TEST_DESKTOP_BINARY')??'/tmp/clivuu-target/release/clivuu-desktop']);
  await wait(async()=>{try{return (await execute({command:'status'})).backend==='rust';}catch{return false;}});
  const status=await execute({command:'status'});await execute({command:'start'});await pause(300);
  check('native service startup only starts the service, without capture UI or a stream',status.running&&!status.busy&&overlays().length===0&&handles===0);
  check('migration capabilities expose native pins and recording',status.capabilities.capture&&status.capabilities.non_focusing_pins&&status.capabilities.recording);
  const cli=JSON.parse(await run([observer._workerPath(),'status'],null,null,{}));check('existing lightweight CLI communicates with the Rust Desktop1 endpoint',cli.backend==='rust'&&cli.protocol===1);
  let invalid=false;try{await execute({command:'snip',protocol:999});}catch{invalid=true;}check('native D-Bus endpoint rejects mismatched protocol versions',invalid);
  let rejected=false;try{await execute({command:'snip',output:123});}catch{rejected=true;}
  check('invalid output type is rejected before capture',rejected&&overlays().length===0&&handles===0);
  await run([observer._workerPath(),'config','set','language','zh-CN']);
  let translated=false;try{await execute({command:'snip',output:123});}catch(e){translated=e.message.includes('无效');}
  check('configured language replaces an earlier protocol error locale',translated);
  await run([observer._workerPath(),'config','set','language','en']);
  const baseline=handles,full=`${directory}/rust-service-full.png`;
  const fullResult=await authorizeScreenshot(()=>execute({command:'snip',mode:'full',output:full}));
  const captured=image(full);check('native full screenshot uses the real authorized static portal',captured.width===global.stage.width&&captured.height===global.stage.height&&fullResult.path===full);
  check('static source preserves the native Wayland fixture pixels',pixel(captured,300,250).join(',')==='36,104,172');
  check('ordinary native screenshot creates no screen-sharing handle',handles===baseline);
  legacy.emit_signal('Available',null);await pause(150);check('installed legacy Shell endpoint is never configured or queried by Rust',legacyCalls===0);
  const output=`${directory}/rust-service-region.png`;const selected=execute({command:'snip',output});
  await wait(()=>overlays().length===1);const overlay=overlays()[0];
  check('capture editor uses an override-redirect desktop surface, not a managed editor window',overlay.get_window_type()===Meta.WindowType.OVERRIDE_OTHER&&overlay.get_frame_rect().width===global.stage.width);
  let busy=false;try{await execute({command:'snip'});}catch{busy=true;}check('concurrent capture is rejected while status remains responsive',busy&&(await execute({command:'status'})).busy);
  move(1000,650);await pause(100);await drag([200,200],[700,500]);
  await key(Clutter.KEY_2);await drag([280,260],[350,330]);await key(Clutter.KEY_z,true);await key(Clutter.KEY_c,true);
  const region=await selected;await wait(()=>overlays().length===0);const cropped=image(output);
  check('real native selection and common Undo export the exact original region',region.rect.x===200&&region.rect.y===200&&cropped.width===500&&cropped.height===300&&pixel(cropped,100,100).join(',')==='36,104,172');
  check('completed capture destroys its platform host and clears busy state',!(await execute({command:'status'})).busy&&overlays().length===0);
  const cancel=execute({command:'snip'});let cancelDone=false;cancel.then(()=>{cancelDone=true;},()=>{cancelDone=true;});await wait(()=>overlays().length===1&&global.display.focus_window===overlays()[0]);GLib.file_set_contents(`${directory}/cancel-focus.json`,JSON.stringify({focus:global.display.focus_window?.get_title(),stage:global.stage.get_key_focus()?.toString(),windows:global.get_window_actors().map(a=>({title:a.meta_window.get_title(),type:a.meta_window.get_window_type()}))}));await key(Clutter.KEY_Escape);await wait(()=>cancelDone);const cancelled=await cancel;await wait(()=>overlays().length===0);
  check('Escape cancels the native capture and releases the desktop surface',cancelled.cancelled&&!(await execute({command:'status'})).busy);
  for(let cycle=0;cycle<12;cycle++){
   const repeated=execute({command:'snip'});let done=false;repeated.then(()=>{done=true;},()=>{done=true;});
   await inputReady();await key(Clutter.KEY_Escape);await wait(()=>done);
   check(`repeated capture ${cycle+1} accepts Escape without an initial click`,(await repeated).cancelled&&overlays().length===0);
  }
  const clicks=()=>{try{return JSON.parse(new TextDecoder().decode(GLib.file_get_contents(`${directory}/fixture-last-click`)[1])).serial;}catch{return 0;}};
  const serial=clicks();move(500,400);await pause(100);button(true);await pause(70);button(false);await wait(()=>clicks()>serial);
  check('native Wayland applications receive clicks again after capture teardown',clicks()>serial);
  check('interactive native screenshots also create no screen-sharing indicator',handles===baseline);
  const clipboard=await execute({command:'snip',rect:{x:200,y:200,width:500,height:300},output:'clipboard'}),clipboardFile=`${directory}/rust-service-clipboard.png`;
  await run(['gjs','-m',`${directory}/clipboard-read.js`,clipboardFile],null,null,{GDK_BACKEND:'x11'});
  const copied=image(clipboardFile);check('native service owns a readable image/png clipboard after capture returns',clipboard.action==='clipboard'&&copied.width===500&&copied.height===300&&pixel(copied,100,100).join(',')==='36,104,172');
  const chosenPath=`${directory}/rust-service-chosen.png`,saving=execute({command:'snip'});await wait(()=>overlays().length===1);await drag([200,200],[700,500]);await key(Clutter.KEY_s);
  await saveChooser(chosenPath);const saved=await saving;const chosen=image(chosenPath);
  check('Save opens the system file chooser and writes only the chosen image location',saved.path===chosenPath&&chosen.width===500&&chosen.height===300);
  const dismiss=execute({command:'snip'});await wait(()=>overlays().length===1);await drag([200,200],[700,500]);await key(Clutter.KEY_s);await saveChooser();
  check('dismissing the system save chooser cancels without leaving the service busy',(await dismiss).cancelled&&!(await execute({command:'status'})).busy);
  const reservedDialog=execute({command:'snip',mode:'full',output:'file-dialog'});await saveChooser();
  check('file-dialog protocol action opens a chooser rather than a literal file',(await reservedDialog).cancelled);
  for(const token of ['silent','no-auto-save','success']){
   const discarded=await execute({command:'snip',mode:'full',output:token});
   check(`${token} protocol action succeeds without a file or chooser`,discarded.ok&&!discarded.path);
  }
  const explicitCopy=execute({command:'snip',output:'clipboard'});await wait(()=>overlays().length===1);await drag([200,200],[700,500]);await key(Clutter.KEY_s);
  check('explicit clipboard destination overrides a toolbar Save action',(await explicitCopy).action==='clipboard');
  const explicitPath=`${directory}/rust-service-override.png`,explicitPin=execute({command:'snip',output:explicitPath});await wait(()=>overlays().length===1);await drag([200,200],[700,500]);await key(Clutter.KEY_p);
  check('explicit file path overrides a toolbar Pin action while honoring explicit output priority',(await explicitPin).path===explicitPath&&image(explicitPath).width===500);
  await run([observer._workerPath(),'config','set','capture-cursor','true']);move(450,350);await pause(200);
  const cursorPath=`${directory}/rust-service-cursor.png`;await execute({command:'snip',mode:'full',output:cursorPath});const withCursor=image(cursorPath);
  let pointerPixels=0;for(let y=340;y<410;y++)for(let x=440;x<510;x++)if(pixel(withCursor,x,y).join(',')!=='36,104,172')pointerPixels++;
  check('GNOME pointer capture uses one compositor stream with the original hotspot',pointerPixels>10&&pixel(withCursor,300,250).join(',')==='36,104,172'&&handles===baseline+1);
  await run([observer._workerPath(),'config','set','capture-cursor','false']);
  if(global.stage.width>1280){
   const crossPath=`${directory}/rust-service-cross.png`,crossing=execute({command:'snip',output:crossPath});await wait(()=>overlays().length===1);
   await drag([1100,200],[1450,500]);await key(Clutter.KEY_c,true);const result=await crossing;const cross=image(crossPath);
   check('native capture selection crosses outputs in desktop coordinates',result.rect.x===1100&&result.rect.width===350&&cross.width===350&&cross.height===300);
   check('cross-output export preserves both original monitor samples',pixel(cross,50,100).join(',')===pixel(captured,1150,300).join(',')&&pixel(cross,300,100).join(',')===pixel(captured,1400,300).join(','));
  }
  await run([observer._workerPath(),'config','set','ocr-languages','eng']);GLib.file_set_contents(`${directory}/fixture-color`,'');await pause(500);
  const recognized=await execute({command:'snip',mode:'full',output:'ocr_clipboard'});
  check('native OCR processes the static source on bounded workers and returns recognized text',recognized.text.includes('Clivuu')&&recognized.text.includes('Scroll capture test'));
  const toolbarOcr=execute({command:'snip'});await inputReady();await drag([60,100],[1050,600]);await key(Clutter.KEY_o);
  await wait(async()=>{const s=(await execute({command:'status'})).captureOcr;return s?.phase==='ready'&&s?.lines>0;});
  await key(Clutter.KEY_Return);
  const recognizedSelection=await toolbarOcr;
  check('shared native toolbar OCR enters in-place selection and confirms text cleanly',recognizedSelection.text.includes('Clivuu')&&overlays().length===0&&!(await execute({command:'status'})).busy);
  const quickOcr=execute({command:'snip'});await inputReady();await drag([60,100],[1050,600]);
  keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Shift_L,Clutter.KeyState.PRESSED);await key(Clutter.KEY_O);keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Shift_L,Clutter.KeyState.RELEASED);
  const quickRecognized=await quickOcr;
  check('Shift+O quick-copy OCR copies text and closes immediately',quickRecognized.text.includes('Clivuu')&&overlays().length===0&&!(await execute({command:'status'})).busy);
  await execute({command:'exit'});
 }catch(e){try{GLib.file_set_contents(`${directory}/failure-xfocus.txt`,await run(['python3','-c','import ctypes; x=ctypes.CDLL("libX11.so.6"); x.XOpenDisplay.restype=ctypes.c_void_p; x.XGetInputFocus.argtypes=[ctypes.c_void_p,ctypes.POINTER(ctypes.c_ulong),ctypes.POINTER(ctypes.c_int)]; d=x.XOpenDisplay(None); w=ctypes.c_ulong(); r=ctypes.c_int(); x.XGetInputFocus(d,ctypes.byref(w),ctypes.byref(r)); print(w.value,r.value)']));}catch{}try{GLib.file_set_contents(`${directory}/failure-native.json`,JSON.stringify(await execute({command:'status'})));}catch{}GLib.file_set_contents(`${directory}/failure-context.json`,JSON.stringify({stage:global.stage.get_key_focus()?.toString(),modal:Main.modalCount,mute:global.display._muteInput,focus:global.display.focus_window?.get_title(),windows:global.get_window_actors().map(a=>({title:a.meta_window.get_title(),type:a.meta_window.get_window_type()}))}));results.push({name:'native service exception',passed:false,error:`${e.message}\n${e.stack}`});const stream=Gio.File.new_for_path(`${directory}/rust-service-failure.png`).replace(null,false,Gio.FileCreateFlags.PRIVATE,null);await new Shell.Screenshot().screenshot_area(0,0,1280,800,stream);stream.close(null);}
 finally{legacy?.unexport();if(signal)global.backend.get_remote_access_controller().disconnect(signal);child?.force_exit();fixture?.force_exit();GLib.file_set_contents(`${directory}/results.json`,JSON.stringify(results));}
}

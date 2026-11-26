// Installed identity with system-only consent UI, tray + CLI capture.
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Clutter from 'gi://Clutter';
import GdkPixbuf from 'gi://GdkPixbuf';
import St from 'gi://St';
import Shell from 'gi://Shell';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {Flashspot} from 'resource:///org/gnome/shell/ui/screenshot.js';
import {pause,run} from './common/process.js';
import {authorizeScreenshot} from './screenshot-access.js';
export async function probe(observer){
 const directory=GLib.getenv('CLIVUU_TEST_DIR'),results=[];let child,fixture;const handles=[];let remote;
 const check=(name,passed)=>{results.push({name,passed:!!passed});GLib.file_set_contents(`${directory}/consent-progress.json`,JSON.stringify(results));if(!passed)throw new Error(name);};
 const wait=async(fn,label)=>{for(let n=0;n<250;n++){if(await fn())return;await pause(60);}throw new Error(`Consent timed out: ${label}`);};
 const BUS='io.github.naerelyth.Clivuu.Desktop',PATH='/io/github/naerelyth/Clivuu/Desktop',IFACE=BUS+'1';
 const call=(path,iface,method,args)=>new Promise((resolve,reject)=>Gio.DBus.session.call(BUS,path,iface,method,args,null,0,120000,null,(c,r)=>{try{resolve(c.call_finish(r).deep_unpack());}catch(e){reject(e);}}));
 const execute=async r=>JSON.parse((await call(PATH,IFACE,'Execute',new GLib.Variant('(s)',[JSON.stringify({protocol:1,...r})])))[0]);
 const status=()=>execute({command:'status'});
 const tray=async()=>{const[items]=await call('/MenuBar','com.canonical.dbusmenu','GetGroupProperties',new GLib.Variant('(aias)',[[],[]]));const item=items.find(([,p])=>p.label?.deep_unpack()==='Capture');await call('/MenuBar','com.canonical.dbusmenu','Event',new GLib.Variant('(isvu)',[item[0],'clicked',new GLib.Variant('i',0),0]));};
 const windows=title=>global.get_window_actors().map(a=>a.meta_window).filter(w=>w.get_title()===title);
 const keyboard=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
 const key=async k=>{keyboard.notify_keyval(GLib.get_monotonic_time(),k,Clutter.KeyState.PRESSED);keyboard.notify_keyval(GLib.get_monotonic_time(),k,Clutter.KeyState.RELEASED);await pause(180);};
 const walk=function*(actor){yield actor;for(const child of actor.get_children())yield*walk(child);};
 const nativeAllow=()=>[...walk(global.stage)].find(a=>a instanceof St.Button&&a.mapped&&/^(Allow|Allow Screenshots)$/.test(a.label));
 const notifications=()=>Main.messageTray.getSources().flatMap(source=>source.notifications??[]);
 let flashes=0;const fire=Flashspot.prototype.fire;
 Flashspot.prototype.fire=function(...args){flashes++;return fire.apply(this,args);};
 try{
  remote=global.backend.get_remote_access_controller().connect('new-handle',(_,h)=>handles.push(h));
  GLib.file_set_contents(`${directory}/fixture-color`,'#2468ac');fixture=Gio.Subprocess.new(['gjs','-m',`${directory}/fixture.js`],0);await pause(1700);Main.overview.hide();await pause(300);
  const cli=observer._workerPath(),packaged=GLib.getenv('CLIVUU_TEST_APPIMAGE')==='1';
  const launcher=new Gio.SubprocessLauncher({flags:0});launcher.unsetenv('APPIMAGE');launcher.setenv('GDK_BACKEND','x11',true);launcher.setenv('CLIVUU_LANGUAGE','en',true);launcher.set_stderr_file_path(`${directory}/consent-cli.log`);launcher.setenv('CLIVUU_DESKTOP_LOG',`${directory}/consent-service.log`,true);
  if(packaged){launcher.setenv('CLIVUU_DESKTOP_DIR','',true);}else launcher.setenv('CLIVUU_DESKTOP_BINARY',GLib.getenv('CLIVUU_TEST_DESKTOP_BINARY')||'/tmp/clivuu-target/release/clivuu-desktop',true);
  let command=[cli];
  if(GLib.getenv('CLIVUU_TEST_AUTOSTART')==='1'){
   const folder=`${GLib.getenv('XDG_CONFIG_HOME')}/autostart`;GLib.mkdir_with_parents(folder,0o700);
   const startup=`${folder}/io.github.naerelyth.clivuu.desktop`,file=new GLib.KeyFile();
   for(const [key,value] of Object.entries({Type:'Application',Name:'Clivuu',StartupWMClass:'io.github.naerelyth.clivuu',Exec:`"${cli.replace(/[\\`"$]/g,c=>'\\'+c).replaceAll('%','%%')}"`}))file.set_string('Desktop Entry',key,value);
   file.save_to_file(startup);command=['gio','launch',startup];
  }
  child=launcher.spawnv(command);await wait(async()=>{try{return(await status()).ready;}catch{return false;}},'service');
  check('cold startup shows no capture, authorization window or stream',!windows('Clivuu capture overlay').length&&!windows('Screenshot permission').length&&!handles.length);
  if(GLib.getenv('CLIVUU_TEST_STRICT_FOCUS')==='1')await run(['gsettings','set','org.gnome.desktop.wm.preferences','focus-new-windows','strict']);
  const firstCli=GLib.getenv('CLIVUU_TEST_FIRST_CLI')==='1',firstPath=`${directory}/first-authorized.png`;
  let firstCapture;
  if(firstCli){firstCapture=run([cli,'snip','--full','-o',firstPath]);firstCapture.catch(()=>{});}else await tray();
  await wait(nativeAllow,'native system authorization');
  check('first request directly opens the desktop permission dialog without application confirmation',!!nativeAllow()&&!windows('Screenshot permission').length);
  const anonymous=GLib.getenv('CLIVUU_TEST_ANONYMOUS')==='1';
  const owners=windows('Clivuu');
  check('system consent uses only a tiny internal focus owner',owners.length===(anonymous?0:1)&&owners.every(w=>{const r=w.get_frame_rect();return r.width<=2&&r.height<=2;}));
  const stream=Gio.File.new_for_path(`${directory}/native-permission.png`).replace(null,false,Gio.FileCreateFlags.PRIVATE,null);
  await new Shell.Screenshot().screenshot_area(0,0,1280,800,stream);stream.close(null);
  if(GLib.getenv('CLIVUU_TEST_CONSENT')==='deny'){
   const deny=[...walk(global.stage)].find(a=>a instanceof St.Button&&a.mapped&&a.label==='Deny');
   const pointer=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
   const[x,y]=deny.get_transformed_position(),[w,h]=deny.get_transformed_size();pointer.notify_absolute_motion(GLib.get_monotonic_time(),x+w/2,y+h/2);await pause(100);
   pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(60);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);
   await wait(async()=>!(await status()).busy,'deny native authorization');
   if(!firstCli){
    await wait(()=>notifications().some(n=>(n.body??'').includes('The system denied the capture or recording request')),'native denial notification');
    check('tray authorization failure posts a visible native notification',true);
   }
   check('native denial closes the invisible owner and releases capture state',!windows('Clivuu').length&&!windows('Clivuu capture overlay').length&&!nativeAllow());
   await tray();await wait(async()=>!(await status()).busy,'repeat denied capture');
   check('system denial is respected without another permission prompt',!nativeAllow()&&!windows('Clivuu').length&&!windows('Clivuu capture overlay').length);
   check('denying static capture never starts a sharing stream',handles.length===0);
   check('denying authorization never captures or flashes',flashes===0);
   await execute({command:'exit'});return;
  }
  await authorizeScreenshot(async()=>{if(firstCli)await firstCapture;else await wait(()=>windows('Clivuu capture overlay').length,'tray selector after grant');});
  check('system approval completes the requested capture and removes its internal owner',!windows('Clivuu').length&&(firstCli?GLib.file_test(firstPath,GLib.FileTest.EXISTS):windows('Clivuu capture overlay').length===1));
  GLib.file_set_contents(`${directory}/first-shutter-count.json`,JSON.stringify({flashes}));
  check('first screenshot including native authorization flashes exactly once',flashes===1);
  if(firstCli){
   const firstImage=GdkPixbuf.Pixbuf.new_from_file(firstPath),data=firstImage.get_pixels();let clean=true;
   for(let y=150;y<=650;y+=40)for(let x=100;x<=1180;x+=40){const n=y*firstImage.rowstride+x*firstImage.n_channels;clean&&=[...data.slice(n,n+3)].join(',')==='36,104,172';}
   check('first authorized image contains no permission dialog, backdrop or owner pixels',clean);
  }else{
   await wait(async()=>(await status()).captureWindows?.some(w=>w.mapped&&w.active&&w.focus==='ClivuuEditorScene'),'selector keyboard readiness');
   await key(Clutter.KEY_Escape);await wait(async()=>!(await status()).busy,'cancel selector');
  }
  const permissions=await new Promise((resolve,reject)=>Gio.DBus.session.call('org.freedesktop.impl.portal.PermissionStore','/org/freedesktop/impl/portal/PermissionStore','org.freedesktop.impl.portal.PermissionStore','Lookup',new GLib.Variant('(ss)',['screenshot','screenshot']),null,0,10000,null,(c,r)=>{try{resolve(c.call_finish(r).deep_unpack()[0]);}catch(e){reject(e);}}));
  const expected=anonymous?'':'renamed-clivuu';
  check('real consent belongs to the resolved Clivuu launcher identity',permissions[expected]?.[0]==='yes');
  const imagePath=`${directory}/consent-cli.png`;
  const output=JSON.parse(await run([cli,'snip','--full','-o',imagePath],null,null,packaged?{}:{CLIVUU_DESKTOP_BINARY:GLib.getenv('CLIVUU_TEST_DESKTOP_BINARY')||'/tmp/clivuu-target/release/clivuu-desktop'}));
  const image=GdkPixbuf.Pixbuf.new_from_file(imagePath),bytes=image.get_pixels(),at=400*image.rowstride+600*image.n_channels;
  check('CLI snip works after real consent without another prompt',output.path===imagePath&&!windows('Clivuu').length&&!nativeAllow());
  check('subsequent screenshot flashes exactly once',flashes===2);
  check('authorization parent is absent from captured pixels',[...bytes.slice(at,at+3)].join(',')==='36,104,172');
  const quick=await execute({command:'snip',rect:{x:100,y:180,width:300,height:200},output:'quick-save'});
  await wait(()=>notifications().some(n=>(n.body??'').includes(quick.path)),'quick save notification');
  check('quick save posts a visible native notification',notifications().some(n=>(n.body??'').includes(quick.path)));
  const saved=notifications().find(n=>(n.body??'').includes(quick.path));
  const icon=saved.source.icon;
  const iconBytes=icon instanceof Gio.FileIcon?icon.get_file().load_contents(null)[1]:null;
  const expectedIcon=GLib.file_get_contents(`${observer.path}/icons/clivoo.svg`)[1];
  check('native notification header uses launcher SVG with bundled fallback',iconBytes!==null&&GLib.compute_checksum_for_data(GLib.ChecksumType.SHA256,iconBytes)===GLib.compute_checksum_for_data(GLib.ChecksumType.SHA256,expectedIcon));
  check('saved screenshot preview is distinct from its source icon',saved.gicon instanceof GdkPixbuf.Pixbuf);
  check('ordinary capture and consent create no ScreenCast sharing handle',handles.length===0);
  await execute({command:'exit'});
 }catch(e){results.push({name:'cold consent exception',passed:false,error:e.message+'\n'+e.stack});}
 finally{Flashspot.prototype.fire=fire;if(remote)global.backend.get_remote_access_controller().disconnect(remote);child?.force_exit();fixture?.force_exit();GLib.file_set_contents(`${directory}/results.json`,JSON.stringify(results));}
}

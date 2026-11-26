// Actual GNOME notification source, image previews, system labels and FileManager1.
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import GdkPixbuf from 'gi://GdkPixbuf';
import Clutter from 'gi://Clutter';
import Shell from 'gi://Shell';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {pause,run} from './common/process.js';
import {authorizeScreenshot} from './screenshot-access.js';
export async function probe(observer){
 const directory=GLib.getenv('CLIVUU_TEST_DIR'),results=[],shown=[];let child,fixture,manager;
 const check=(name,passed)=>{results.push({name,passed:!!passed});GLib.file_set_contents(`${directory}/notification-progress.json`,JSON.stringify(results));if(!passed)throw new Error(name);};
 const wait=async(fn,label)=>{for(let n=0;n<250;n++){if(await fn())return;await pause(50);}throw new Error(`Notification timed out: ${label}`);};
 const execute=r=>new Promise((resolve,reject)=>Gio.DBus.session.call('io.github.naerelyth.Clivuu.Desktop','/io/github/naerelyth/Clivuu/Desktop','io.github.naerelyth.Clivuu.Desktop1','Execute',new GLib.Variant('(s)',[JSON.stringify({protocol:1,...r})]),null,0,130000,null,(c,res)=>{try{resolve(JSON.parse(c.call_finish(res).deep_unpack()[0]));}catch(e){reject(e);}}));
 const notifications=()=>Main.messageTray.getSources().filter(s=>s.title==='Clivuu').flatMap(s=>s.notifications??[]);
 try{
  for(const[locale,app,label]of[['zh_CN.UTF-8','en','在文件管理器中显示'],['en_US.UTF-8','zh-CN','Show in Files'],['de_DE.UTF-8','zh-CN','Show in Files']]){
   await run(['/tmp/clivuu-target/release/examples/notification_probe',label],null,null,{LC_ALL:locale,LANGUAGE:locale,CLIVUU_LANGUAGE:app});
  }
  check('system action labels ignore application language and fall back to English',true);
  await run(['dbus-run-session','--','/tmp/clivuu-target/release/examples/notification_probe'],null,null,{GDK_BACKEND:'x11',GIO_USE_VFS:'local',CLIVUU_TEST_NOTIFICATION_THEME:'1'});
  check('private notification protocol preserves themes, image aspect, actions and daemon restart',true);
  // This is deliberately a generic manager, not Nautilus.
  await new Promise((resolve,reject)=>Gio.DBus.session.call('org.freedesktop.DBus','/org/freedesktop/DBus','org.freedesktop.DBus','RequestName',new GLib.Variant('(su)',['org.freedesktop.FileManager1',0]),null,0,5000,null,(c,r)=>{try{resolve(c.call_finish(r));}catch(e){reject(e);}}));
  manager=Gio.DBusExportedObject.wrapJSObject('<node><interface name="org.freedesktop.FileManager1"><method name="ShowItems"><arg type="as" direction="in"/><arg type="s" direction="in"/></method></interface></node>',{ShowItems(uris){shown.push(...uris);}});
  manager.export(Gio.DBus.session,'/org/freedesktop/FileManager1');
  GLib.file_set_contents(`${directory}/fixture-color`,'#2468ac');fixture=Gio.Subprocess.new(['gjs','-m',`${directory}/fixture.js`],0);await pause(1500);Main.overview.hide();await pause(300);
  const cli=observer._workerPath(),packaged=GLib.getenv('CLIVUU_TEST_APPIMAGE')==='1';
  const launcher=new Gio.SubprocessLauncher({flags:0});launcher.setenv('GDK_BACKEND','x11',true);launcher.setenv('CLIVUU_LANGUAGE','zh-CN',true);launcher.setenv('LC_ALL','C.UTF-8',true);launcher.setenv('LANGUAGE','en',true);launcher.set_stderr_file_path(`${directory}/notification-service.log`);
  if(packaged){launcher.setenv('CLIVUU_DESKTOP_DIR','',true);child=launcher.spawnv([cli]);}else child=launcher.spawnv(['/tmp/clivuu-target/release/clivuu-desktop']);
  await wait(async()=>{try{return(await execute({command:'status'})).ready;}catch{return false;}},'startup');
  for(const[key,value]of[['image-auto-save','true'],['image-auto-directory',`${directory}/auto`],['image-save-notify','true'],['video-save-notify','true']])await run([cli,'config','set',key,value]);
  const saved=await authorizeScreenshot(()=>execute({command:'snip',rect:{x:200,y:200,width:360,height:180},output:'clipboard'}));
  await wait(()=>notifications().some(n=>n.body?.includes(saved.automatic_path)),'automatic image save');
  const n=notifications().find(n=>n.body?.includes(saved.automatic_path));
  check('automatic image save posts a native visible notification',!!n&&GLib.file_test(saved.automatic_path,GLib.FileTest.EXISTS));
  const expected=GLib.file_get_contents(`${observer.path}/icons/clivoo.svg`)[1],icon=n.source.icon;
  check('header uses the installed launcher SVG',icon instanceof Gio.FileIcon&&GLib.compute_checksum_for_data(GLib.ChecksumType.SHA256,icon.get_file().load_contents(null)[1])===GLib.compute_checksum_for_data(GLib.ChecksumType.SHA256,expected));
  check('header remains associated with the resolved renamed launcher',n.source.app?.get_id()==='renamed-clivuu.desktop');
  const preview=n.gicon;
  check('body contains an aspect-preserving screenshot thumbnail',preview instanceof GdkPixbuf.Pixbuf&&preview.width===160&&preview.height===80);
  const pixels=preview.get_pixels();check('thumbnail is the saved image, not the application logo',[...pixels.slice(0,4)].join(',')==='36,104,172,255');
  const label='Show in Files';
  check('file action label uses the desktop language independently of Clivuu Chinese',n.actions.length===1&&n.actions[0].label===label&&n.title==='图片已保存');
  for(const source of Main.messageTray.getSources())for(const item of [...source.notifications])if(item!==n)item.destroy();
  n.acknowledged=false;n.source.emit('notification-request-banner',n);
  GLib.file_set_contents(`${directory}/banner-state.json`,JSON.stringify({current:Main.messageTray._notification?.title,queue:Main.messageTray._notificationQueue?.map(n=>n.title),blocked:Main.messageTray._bannerBlocked,busy:Main.messageTray._busy,full:Main.layoutManager.primaryMonitor.inFullscreen,policy:n.source.policy.showBanners,state:Main.messageTray._notificationState}));
  await wait(()=>Main.messageTray._notification===n&&Main.messageTray._banner,'saved-image banner');
  await pause(400);
  const banner=Main.messageTray._banner,[bx,by]=banner.get_transformed_position();
  const pointer=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
  pointer.notify_absolute_motion(GLib.get_monotonic_time(),5,750);await pause(150);
  pointer.notify_absolute_motion(GLib.get_monotonic_time(),bx+banner.width/2,by+20);await pause(600);
  check('saved image is the banner shown for visual verification',Main.messageTray._notification===n);
  const stream=Gio.File.new_for_path(`${directory}/notification-image.png`).replace(null,false,Gio.FileCreateFlags.PRIVATE,null);await new Shell.Screenshot().screenshot_area(0,0,1280,800,stream);stream.close(null);
  n.actions[0].activate();await wait(()=>shown.includes(Gio.File.new_for_path(saved.automatic_path).get_uri()),'image reveal');
  check('Show in Files selects the exact saved file through generic FileManager1',true);
  await run([cli,'config','set','image-auto-save','false']);
  await execute({command:'record',rect:{x:200,y:200,width:360,height:180},output:`${directory}/record.webm`});await pause(1200);await execute({command:'stop-recording'});
  await wait(()=>notifications().some(n=>n.body?.includes('record.webm')),'recorded video');const video=notifications().find(n=>n.body?.includes('record.webm'));
  check('recording notification has no body icon or thumbnail',video.gicon===null);
  check('recording retains the system-language file action',video.actions.length===1&&video.actions[0].label===label);
  video.actions[0].activate();await wait(()=>shown.includes(Gio.File.new_for_path(`${directory}/record.webm`).get_uri()),'video reveal');
  await new Promise((resolve,reject)=>Gio.DBus.session.call('io.github.naerelyth.Clivuu.Desktop','/MenuBar','com.canonical.dbusmenu','Event',new GLib.Variant('(isvu)',[2,'clicked',new GLib.Variant('i',0),0]),null,0,5000,null,(c,r)=>{try{resolve(c.call_finish(r));}catch(e){reject(e);}}));
  await wait(()=>notifications().some(n=>n.title==='Clivuu'),'tray error');const error=notifications().find(n=>n.title==='Clivuu');
  check('other notifications have no content icon or file action',error.gicon===null&&error.actions.length===0);
  await execute({command:'exit'});
 }catch(e){results.push({name:'notification exception',passed:false,error:`${e.message}\n${e.stack}`});}
 finally{manager?.unexport();child?.force_exit();fixture?.force_exit();GLib.file_set_contents(`${directory}/results.json`,JSON.stringify(results));}
}

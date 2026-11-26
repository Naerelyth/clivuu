import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import St from 'gi://St';
import Clutter from 'gi://Clutter';
import GdkPixbuf from 'gi://GdkPixbuf';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {pause,run} from './common/process.js';
import {authorizeScreenshot} from './screenshot-access.js';
const BUS='io.github.naerelyth.Clivuu.Desktop',PATH='/io/github/naerelyth/Clivuu/Desktop',IFACE=`${BUS}1`;
export async function probe(observer){
 const directory=GLib.getenv('CLIVUU_TEST_DIR'),results=[],host='appindicatorsupport@rgcjonas.gmail.com';let child;
 const check=(name,passed)=>{results.push({name,passed:Boolean(passed)});GLib.file_set_contents(`${directory}/native-integration-progress.json`,JSON.stringify(results));if(!passed)throw new Error(name);};
 const wait=async predicate=>{for(let i=0;i<150;i++){if(await predicate())return;await pause(80);}throw new Error('integration condition timed out');};
 const execute=r=>new Promise((resolve,reject)=>Gio.DBus.session.call(BUS,PATH,IFACE,'Execute',new GLib.Variant('(s)',[JSON.stringify({protocol:1,...r})]),null,0,30000,null,(c,result)=>{try{resolve(JSON.parse(c.call_finish(result).deep_unpack()[0]));}catch(e){reject(e);}}));
 const find=()=>Object.values(Main.panel.statusArea).find(i=>i?._indicator?.id==='clivuu');
 const iconPath=i=>{let icon=i?.icon?.gicon;while(icon instanceof Gio.EmblemedIcon)icon=icon.get_icon();return icon instanceof Gio.FileIcon?icon.get_file().get_path():icon?.to_string();};
 const theme=St.ThemeContext.get_for_stage(global.stage),scale=theme.scale_factor;
 try{
  Main.overview.hide();await pause(300);
  await run([GLib.getenv('CLIVUU_TEST_CONFIG_PROBE')||'/tmp/clivuu-target/release/examples/config_probe'],null,null,{GDK_BACKEND:'x11',CLIVUU_CONFIG_PROBE_RESULT:`${directory}/native-configuration.json`});
  const configured=JSON.parse(new TextDecoder().decode(GLib.file_get_contents(`${directory}/native-configuration.json`)[1]));
  for(const field of ['responsive_busy_retry','live_language','coalesced_final_value','reset_removes_override','external_cli_latest'])check(`native configuration ${field}`,configured[field]);
  check('native configuration reports no background write errors',configured.errors.length===0);
  await run(['dbus-run-session','--',GLib.getenv('CLIVUU_TEST_NOTIFICATION_PROBE')||'/tmp/clivuu-target/release/examples/notification_probe'],null,null,{GDK_BACKEND:'x11',GIO_USE_VFS:'local',CLIVUU_TEST_NOTIFICATION_THEME:'1'});
  check('notifications use only the canonical icon name, live themes, inheritance and bundled fallback',true);
  const launcher=new Gio.SubprocessLauncher({flags:0});launcher.unsetenv('APPIMAGE');launcher.setenv('GDK_BACKEND','x11',true);launcher.setenv('CLIVUU_LANGUAGE','en',true);launcher.set_stderr_file_path(`${directory}/rust-integration.log`);const packaged=GLib.getenv('CLIVUU_TEST_APPIMAGE')==='1';if(packaged){launcher.setenv('CLIVUU_DESKTOP_DIR','',true);}let portable=observer._workerPath();if(packaged){portable=`${directory}/Portable Apps/Clivuu 100%.AppImage`;GLib.mkdir_with_parents(GLib.path_get_dirname(portable),0o700);Gio.File.new_for_path(observer._workerPath()).copy(Gio.File.new_for_path(portable),Gio.FileCopyFlags.OVERWRITE,null,null);Gio.File.new_for_path(portable).set_attribute_uint32('unix::mode',0o700,Gio.FileQueryInfoFlags.NONE,null);}child=launcher.spawnv(packaged?[portable]:[GLib.getenv('CLIVUU_TEST_DESKTOP_BINARY')||'/tmp/clivuu-target/release/clivuu-desktop']);await wait(async()=>{try{return(await execute({command:'status'})).ready;}catch{return false;}});
  check('native application startup does not start a capture',(await execute({command:'status'})).pins===0&&!global.get_window_actors().some(a=>a.meta_window.get_title()==='Clivuu capture overlay'));
  await run(['gnome-extensions','enable',host]);await wait(find);await wait(()=>iconPath(find())?.endsWith('clivoo-transparent.svg'));
  const artwork=GdkPixbuf.Pixbuf.new_from_file(iconPath(find()));
  check('late real SNI host loads the scalable transparent tray source',artwork.width===64&&artwork.height===64&&artwork.has_alpha);
  for(const panel of global.dashToPanel?.panels??[])panel.intellihide.revealAndHold(1,true);
  let indicator=find();indicator.menu.open();await wait(()=>indicator.menu._getMenuItems().some(i=>i.label?.text==='Quit'));
  GLib.file_set_contents(`${directory}/native-menu-items.json`,JSON.stringify(indicator.menu._getMenuItems().map(i=>({type:i.constructor.name,label:i.label?.text,reactive:i.reactive}))));
  check('native DBusMenu uses two compact logical separators',indicator.menu._getMenuItems().filter(i=>i.constructor.name.includes('Separator')).length===2);
  check('real host menu has no scrolling capture entry',!indicator.menu._getMenuItems().some(i=>i.label?.text==='Scrolling capture'));
  check('tray Pin groups header displays active group and idle controls stay hidden',indicator.menu._getMenuItems().some(i=>i.label?.text==='Pin groups: Default')&&!indicator.menu._getMenuItems().find(i=>i.label?.text==='Stop recording')?.visible&&!indicator.menu._getMenuItems().find(i=>i.label?.text==='Restore pointer interaction')?.visible);
  const recoveryMenu=indicator.menu._getMenuItems().find(i=>i.label?.text?.startsWith('Pin recovery & cleanup'));
  check('native closed-pin actions live in recovery submenu and start insensitive',recoveryMenu?.menu?._getMenuItems().find(i=>i.label?.text==='Reopen last closed pin')?.reactive===false);indicator.menu.close();
  theme.scale_factor=2;await pause(700);check('real host preserves SVG source after scaling',iconPath(find())?.endsWith('clivoo-transparent.svg'));theme.scale_factor=scale;
  await run(['gnome-extensions','disable',host]);await wait(()=>!find());await run(['gnome-extensions','enable',host]);await wait(find);await wait(()=>Boolean(find().icon?.gicon));await pause(600);indicator=find();indicator.menu.open();await wait(()=>indicator.menu._getMenuItems().some(i=>i.label?.text==='Preferences'));
  check('restarted host rebuilds one native tray icon and a complete menu',Object.values(Main.panel.statusArea).filter(i=>i?._indicator?.id==='clivuu').length===1);
  check('restarted host retains the updated menu without scrolling capture',!indicator.menu._getMenuItems().some(i=>i.label?.text==='Scrolling capture')); 
  await wait(()=>{
   const item=indicator.menu._getMenuItems().find(i=>i.label?.text==='Preferences');if(!indicator.menu.isOpen||!item?.mapped||!item.has_allocation()||indicator.menu._boxPointer?._muteInput)return false;
   const values=[...item.get_transformed_position(),...item.get_transformed_size()];return values.every(Number.isFinite)&&values[2]>0&&values[3]>0;
  });
  const entry=indicator.menu._getMenuItems().find(i=>i.label?.text==='Preferences');const[x,y]=entry.get_transformed_position(),[w,h]=entry.get_transformed_size();
  GLib.file_set_contents(`${directory}/native-menu-click.json`,JSON.stringify({x,y,w,h,mapped:entry.mapped,visible:entry.visible,open:indicator.menu.isOpen}));
  const pointer=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);pointer.notify_absolute_motion(GLib.get_monotonic_time(),x+w/2,y+h/2);await pause(100);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);
  await pause(300);GLib.file_set_contents(`${directory}/native-windows-after-click.json`,JSON.stringify(global.get_window_actors().map(a=>({title:a.meta_window.get_title(),rect:a.meta_window.get_frame_rect()}))));
  await wait(()=>global.get_window_actors().some(a=>a.meta_window.get_title()==='Clivuu Preferences'));check('real native tray Preferences action opens GTK preferences',true);
  const preferences=global.get_window_actors().map(a=>a.meta_window).find(w=>w.get_title()==='Clivuu Preferences');
  const startupFile=`${GLib.getenv('XDG_CONFIG_HOME')}/autostart/io.github.naerelyth.clivuu.desktop`,startupExists=()=>GLib.file_test(startupFile,GLib.FileTest.EXISTS);
  await pause(400);await observer.captureArea({x:0,y:0,width:1280,height:800},`${directory}/startup-before.png`);const pr=preferences.get_frame_rect();pointer.notify_absolute_motion(GLib.get_monotonic_time(),pr.x+840,pr.y+432);await pause(100);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(500);await observer.captureArea({x:0,y:0,width:1280,height:800},`${directory}/startup-after.png`);await wait(startupExists);
  const startupText=()=>new TextDecoder().decode(GLib.file_get_contents(startupFile)[1]);
  check('General startup switch writes a durable native or original portable launcher',startupText().includes(packaged?'Portable Apps/Clivuu 100%%.AppImage':'/clivuu')&&!startupText().includes('clivuu-appimage-')&&!startupText().includes('clivuu-desktop'));
  preferences.delete(global.get_current_time());await wait(()=>!global.get_window_actors().some(a=>a.meta_window===preferences));
  await authorizeScreenshot(()=>execute({command:'snip',rect:{x:5,y:5,width:1270,height:730},output:'silent'}));
  const keyboard=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
  const move=(x,y)=>pointer.notify_absolute_motion(GLib.get_monotonic_time(),x,y);
  const button=down=>pointer.notify_button(GLib.get_monotonic_time(),1,down?Clutter.ButtonState.PRESSED:Clutter.ButtonState.RELEASED);
  for(let n=0;n<2;n++){
   move(300,300);await pause(300);await observer.captureArea({x:0,y:0,width:1280,height:740},`${directory}/tray-baseline-${n}.png`);
   indicator=find();indicator.menu.open();await wait(()=>indicator.menu.isOpen&&indicator.menu._getMenuItems().find(i=>i.label?.text==='Capture')?.mapped);await pause(300);
   const capture=indicator.menu._getMenuItems().find(i=>i.label?.text==='Capture'),[cx,cy]=capture.get_transformed_position(),[cw,ch]=capture.get_transformed_size();
   move(cx+cw/2,cy+ch/2);await pause(100);button(true);await pause(40);button(false);
   await wait(()=>global.get_window_actors().some(a=>a.meta_window.get_title()==='Clivuu capture overlay'));
   check('real tray menu has finished hiding before selector '+n,!indicator.menu.isOpen&&!indicator.menu.actor.visible);
   move(5,5);await pause(100);button(true);for(let i=1;i<=15;i++){move(5+1270*i/15,5+730*i/15);await pause(20);}button(false);await pause(150);
   keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Return,Clutter.KeyState.PRESSED);keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Return,Clutter.KeyState.RELEASED);
   await wait(async()=>!(await execute({command:'status'})).busy);
   await run(['gjs','-m',`${directory}/clipboard-read.js`,`${directory}/tray-captured-${n}.png`],null,null,{GDK_BACKEND:'x11'});
   const before=GdkPixbuf.Pixbuf.new_from_file(`${directory}/tray-baseline-${n}.png`),after=GdkPixbuf.Pixbuf.new_from_file(`${directory}/tray-captured-${n}.png`);
   const b=before.get_pixels(),a=after.get_pixels();let error=0,count=0;
   for(let y=Math.max(10,Math.ceil(cy+4));y<Math.min(730,cy+ch-4);y+=2)for(let x=Math.max(10,Math.ceil(cx+10));x<Math.min(1270,cx+cw-10);x+=3)for(let c=0;c<3;c++){
    error+=Math.abs(b[y*before.rowstride+x*before.n_channels+c]-a[(y-5)*after.rowstride+(x-5)*after.n_channels+c]);count++;
   }
   GLib.file_set_contents(`${directory}/tray-pixel-${n}.json`,JSON.stringify({cx,cy,cw,ch,count,error,mean:error/count,width:after.width,height:after.height}));
   check('captured pixels contain the background rather than the tray menu '+n,after.width===1270&&after.height===730&&count>0&&error/count<1);
  }
  await execute({command:'paste',source:'plain',text:'RESTART_PERSISTENCE',position:[240,180]});
  const beforeRestart=await execute({command:'status'}),oldPid=beforeRestart.pid;
  indicator=find();indicator.menu.open();await wait(()=>indicator.menu._getMenuItems().find(i=>i.label?.text==='Restart')?.mapped);await pause(250);
  const restartItem=indicator.menu._getMenuItems().find(i=>i.label?.text==='Restart'),[rx,ry]=restartItem.get_transformed_position(),[rw,rh]=restartItem.get_transformed_size();
  move(rx+rw/2,ry+rh/2);await pause(100);button(true);await pause(40);button(false);
  await wait(async()=>{try{const s=await execute({command:'status'});return s.ready&&s.pid!==oldPid;}catch{return false;}});
  const afterRestart=await execute({command:'status'});
  check('real tray Restart replaces the desktop process and restores the persisted pin',afterRestart.pins===1&&JSON.stringify(afterRestart.images)===JSON.stringify(beforeRestart.images));
  await wait(find);await pause(400);
  check('restart re-registers exactly one tray icon',Object.values(Main.panel.statusArea).filter(i=>i?._indicator?.id==='clivuu').length===1);
  const pendingSelection=execute({command:'snip'}).then(()=>({ok:true}),e=>({error:e.message}));
  await wait(()=>global.get_window_actors().some(a=>a.meta_window.get_title()==='Clivuu capture overlay'));
  await execute({command:'restart'});
  await wait(async()=>{try{const s=await execute({command:'status'});return s.ready&&s.pid!==afterRestart.pid;}catch{return false;}});
  const cancelled=await pendingSelection;
  check('restart cancels an active selector without losing pins or replaying the snip command',Boolean(cancelled.error)&&(await execute({command:'status'})).pins===1&&!global.get_window_actors().some(a=>a.meta_window.get_title()==='Clivuu capture overlay'));
  const lastPid=(await execute({command:'status'})).pid;
  await execute({command:'exit'});await pause(500);
  // Simulate the standard XDG desktop launcher in a fresh login environment.
  await run(['gio','launch',startupFile],null,null,{CLIVUU_DESKTOP_DIR:'',GDK_BACKEND:'wayland'});
  await wait(async()=>{try{const s=await execute({command:'status'});return s.ready&&s.pid!==lastPid;}catch{return false;}});
  check('the actual XDG startup launcher starts the service and restores its persisted pin',(await execute({command:'status'})).pins===1);
  await execute({command:'open-preferences'});let startupPreferences;await wait(()=>startupPreferences=global.get_window_actors().map(a=>a.meta_window).find(w=>w.get_title()==='Clivuu Preferences'));await pause(300);
  const sr=startupPreferences.get_frame_rect();move(sr.x+840,sr.y+432);await pause(100);button(true);button(false);await wait(()=>!startupExists());
  check('General startup switch removes the user entry without exiting the current service',(await execute({command:'status'})).ready);
  await execute({command:'exit'});
 }catch(e){results.push({name:'native integration exception',passed:false,error:`${e.message}\n${e.stack}`});}
 finally{for(const panel of global.dashToPanel?.panels??[])panel.intellihide.release(1);theme.scale_factor=scale;child?.force_exit();GLib.file_set_contents(`${directory}/results.json`,JSON.stringify(results));}
}

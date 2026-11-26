// Native cursor pixels across live XSettings scale changes, on a private bus.
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Clutter from 'gi://Clutter';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {pause} from './common/process.js';
import {authorizeScreenshot} from './screenshot-access.js';
export async function probe(observer) {
 const directory=GLib.getenv('CLIVUU_TEST_DIR'),results=[],children=[];
 const check=(name,passed,details={})=>{results.push({name,passed:!!passed,...details});GLib.file_set_contents(`${directory}/cursor-scale-progress.json`,JSON.stringify(results));if(!passed)throw Error(name);};
 const wait=async f=>{for(let n=0;n<200;n++){const v=await f();if(v)return v;await pause(50);}throw Error('Cursor scale test timed out');};
 const execute=r=>new Promise((resolve,reject)=>Gio.DBus.session.call('io.github.naerelyth.Clivuu.Desktop','/io/github/naerelyth/Clivuu/Desktop','io.github.naerelyth.Clivuu.Desktop1','Execute',new GLib.Variant('(s)',[JSON.stringify({protocol:1,...r})]),null,0,30000,null,(c,r)=>{try{resolve(JSON.parse(c.call_finish(r).deep_unpack()[0]));}catch(e){reject(e);}}));
 const pointer=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
 const keyboard=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
 const move=async(x,y)=>{pointer.notify_absolute_motion(GLib.get_monotonic_time(),x,y);await pause(180);};
 const size=()=>{const s=global.backend.get_cursor_tracker().get_sprite();return s?[s.get_width(),s.get_height()]:[];};
 const click=async(r,button=1,scale=1)=>{await move((r.x+r.width/2)*scale,(r.y+r.height/2)*scale);pointer.notify_button(GLib.get_monotonic_time(),button,Clutter.ButtonState.PRESSED);await pause(60);pointer.notify_button(GLib.get_monotonic_time(),button,Clutter.ButtonState.RELEASED);await pause(160);};
 const key=async k=>{keyboard.notify_keyval(GLib.get_monotonic_time(),k,Clutter.KeyState.PRESSED);keyboard.notify_keyval(GLib.get_monotonic_time(),k,Clutter.KeyState.RELEASED);await pause(180);};
 const samplePin=async scale=>{const r=(await execute({command:'status'})).images[0].rect;await click(r,1,scale);return size();};
 try {
  Main.overview.hide();
  const settings=new Gio.Settings({schema_id:'org.gnome.desktop.interface'});settings.set_string('cursor-theme','Adwaita');settings.set_int('cursor-size',24);
  const xs=new Gio.Settings({schema_id:'org.gnome.settings-daemon.plugins.xsettings'});
  const setScale=async scale=>{xs.set_value('overrides',new GLib.Variant('a{sv}',{'Gdk/WindowScalingFactor':new GLib.Variant('i',scale),'Gtk/CursorThemeSize':new GLib.Variant('i',24*scale)}));await pause(700);};
  children.push(Gio.Subprocess.new(['/usr/libexec/gsd-xsettings'],0));await pause(900);await setScale(1);
  const control=`${directory}/cursor-scale-control.js`;
  GLib.file_set_contents(control,`import Gtk from 'gi://Gtk?version=4.0';import GLib from 'gi://GLib';Gtk.init();const loop=new GLib.MainLoop(null,false),w=new Gtk.Window({title:'Cursor scale control',decorated:false,default_width:120,default_height:120});w.set_cursor_from_name('default');w.present();loop.run();`);
  const launch=new Gio.SubprocessLauncher({flags:0});launch.setenv('GDK_BACKEND','x11',true);launch.setenv('CLIVUU_LANGUAGE','en',true);launch.set_stderr_file_path(`${directory}/cursor-scale-service.log`);
  children.push(launch.spawnv(['gjs','-m',control]));
  const sampleControl=async()=>{const w=await wait(()=>global.get_window_actors().map(a=>a.meta_window).find(w=>w.get_title()==='Cursor scale control'));Main.activateWindow(w);await pause(120);const r=w.get_frame_rect();await move(r.x+r.width/2,r.y+r.height/2);return size();};
  const baseline=await sampleControl();
  children.push(launch.spawnv([GLib.getenv('CLIVUU_TEST_DESKTOP_BINARY')||observer._workerPath()]));
  await wait(async()=>{try{return(await execute({command:'status'})).ready;}catch{return false;}});
  const pid=(await execute({command:'status'})).pid;
  await execute({command:'paste',text:'#4080c0',position:[30,60]});
  const initial=await samplePin(1);
  check('initial Clivuu and GTK control cursors use the same theme size',JSON.stringify(initial)===JSON.stringify(baseline),{initial,baseline});
  await authorizeScreenshot(()=>execute({command:'snip',rect:{x:0,y:0,width:32,height:32},output:'no-auto-save'}));
  for(const scale of [2,1,2,1]) {
   await setScale(scale);
   const actual=await samplePin(scale),expected=baseline.map(n=>n*scale),controlSize=await sampleControl();
   check(`Clivuu refreshes an existing cursor at ${scale}x without restarting`,JSON.stringify(actual)===JSON.stringify(expected),{actual,expected,control:controlSize});
   // Record the upstream behavior without requiring future GTK versions to
   // retain the cache bug. Clivuu's assertions always require the correct size.
   results.push({name:`unadapted GTK cursor reference at ${scale}x`,passed:true,size:controlSize});
   const r=(await execute({command:'status'})).images[0].rect;await click(r,3,scale);
   const menu=await wait(async()=>(await execute({command:'status'})).pinUi.menu);
   const b=menu.buttons.find(b=>b.label==='Show toolbar')||menu.buttons.find(b=>b.rect.width>0);
   await move((b.rect.x+b.rect.width/2)*scale,(b.rect.y+b.rect.height/2)*scale);
   const hover=size();pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(120);const held=size();
   await move(1200,750);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await key(Clutter.KEY_Escape);
   check(`menu hover and held cursor use the current ${scale}x size`,[hover,held].every(s=>JSON.stringify(s)===JSON.stringify(expected)),{hover,held,expected});
   const capture=execute({command:'snip'});capture.catch(()=>{});
   await wait(async()=>(await execute({command:'status'})).captureWindows.some(w=>w.mapped&&w.active));
   await move(450,260);const background=size();await key(Clutter.KEY_Escape);await capture;
   check(`capture background cursor uses the current ${scale}x size`,JSON.stringify(background)===JSON.stringify(expected),{background,expected});
  }
  check('scale changes retain the same desktop process',(await execute({command:'status'})).pid===pid);
  await execute({command:'exit'});
 } catch(e) {results.push({name:'cursor scale exception',passed:false,error:`${e.message}\n${e.stack}`});}
 finally {for(const child of children)child.force_exit();GLib.file_set_contents(`${directory}/results.json`,JSON.stringify(results));}
}

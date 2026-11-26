import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {PickPixel} from 'resource:///org/gnome/shell/ui/screenshot.js';
import {pause} from './common/process.js';

export async function probe() {
 const directory=GLib.getenv('CLIVUU_TEST_DIR'),results=[];let child,owner,portal,picker;
 const check=(name,passed,data={})=>{results.push({name,passed:!!passed,...data});GLib.file_set_contents(`${directory}/color-progress.json`,JSON.stringify(results));if(!passed)throw new Error(name);};
 const wait=async(predicate,label)=>{for(let n=0;n<200;n++){if(await predicate())return;await pause(50);}throw new Error(`Color probe timed out: ${label}`);};
 const read=()=>JSON.parse(new TextDecoder().decode(GLib.file_get_contents(`${directory}/color-probe-state.json`)[1]));
 let requests=0,completed=0;
 try {
  const xml=`<node><interface name="org.freedesktop.portal.Screenshot"><method name="PickColor"><arg type="s" direction="in"/><arg type="a{sv}" direction="in"/><arg type="o" direction="out"/></method></interface></node>`;
  portal=Gio.DBusExportedObject.wrapJSObject(xml,{
   async PickColorAsync([_parent,options],invocation) {
    const token=options.handle_token.deep_unpack(),sender=invocation.get_sender();
    const path=`/org/freedesktop/portal/desktop/request/${sender.slice(1).replaceAll('.','_')}/${token}`;
    invocation.return_value(new GLib.Variant('(o)',[path]));requests++;
    picker=new PickPixel(new Shell.Screenshot());
    let code=0,color;
    try{color=await picker.pickAsync();if(!color)code=1;}catch{code=1;}
    picker=null;completed++;
    Gio.DBus.session.emit_signal(sender,path,'org.freedesktop.portal.Request','Response',new GLib.Variant('(ua{sv})',[code,color?{color:new GLib.Variant('(ddd)',[color.red/255,color.green/255,color.blue/255])}:{}]));
   },
  });
  portal.export(Gio.DBus.session,'/org/freedesktop/portal/desktop');
  let owned=false;
  owner=Gio.bus_own_name_on_connection(Gio.DBus.session,'io.github.naerelyth.Clivuu.ColorTest',Gio.BusNameOwnerFlags.NONE,()=>owned=true,()=>{});
  await wait(()=>owned,'private portal name');
  const launch=new Gio.SubprocessLauncher({flags:0});launch.set_stderr_file_path(`${directory}/color-probe.log`);launch.setenv('GDK_BACKEND','wayland',true);launch.setenv('CLIVUU_LANGUAGE','en',true);
  child=launch.spawnv(['/tmp/clivuu-target/release/examples/color_probe']);
  await wait(()=>GLib.file_test(`${directory}/color-probe-state.json`,GLib.FileTest.EXISTS),'GTK window');
  Main.overview.hide();
  let settings;await wait(()=>settings=global.get_window_actors().map(a=>a.meta_window).find(w=>w.get_title()==='Clivuu Preferences'),'settings surface');
  settings.activate(global.get_current_time());await pause(400);
  const pointer=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
  const keyboard=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
  const move=(x,y)=>pointer.notify_absolute_motion(GLib.get_monotonic_time(),x,y);
  const click=async(x,y)=>{move(x,y);await pause(100);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(80);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(250);};
  const key=async k=>{keyboard.notify_keyval(GLib.get_monotonic_time(),k,Clutter.KeyState.PRESSED);keyboard.notify_keyval(GLib.get_monotonic_time(),k,Clutter.KeyState.RELEASED);await pause(250);};
  const visible=async()=>{
   const tracker=global.backend.get_cursor_tracker(),sprite=tracker.get_sprite();if(!sprite)return false;
   const stream=Gio.MemoryOutputStream.new_resizable();
   const image=await new Promise((resolve,reject)=>Shell.Screenshot.composite_to_stream(sprite,0,0,sprite.get_width(),sprite.get_height(),1,null,0,0,1,stream,(_,res)=>{try{resolve(Shell.Screenshot.composite_to_stream_finish(res));}catch(e){reject(e);}}));
   stream.close(null);const pixels=image.get_pixels();if(!image.has_alpha)return true;
   for(let y=0;y<image.height;y++)for(let x=0;x<image.width;x++)if(pixels[y*image.rowstride+x*image.n_channels+3]>0)return true;
   return false;
  };
  const r=settings.get_frame_rect();move(r.x+30,r.y+250);await pause(200);check('settings has a visible baseline cursor',await visible());
  const a=read().accent;await click(r.x+a.x+a.width/2,r.y+a.y+a.height/2);await wait(()=>read().dialog?.pick,'color panel');
  const dialog=()=>global.get_window_actors().map(a=>a.meta_window).find(w=>w.get_title()==='Accent color')||settings;
  await wait(dialog,'color dialog');
  check('settings color dialog shares its native Wayland surface',global.get_window_actors().filter(a=>['Clivuu Preferences','Accent color'].includes(a.meta_window.get_title())).length===1);
  const control=async name=>{const d=dialog().get_frame_rect(),b=read().dialog[name];await click(d.x+b.x+b.width/2,d.y+b.y+b.height/2);};
  const target=[r.x+30,r.y+250];
  move(...target);await pause(200);check('modal parent retains a visible cursor before screen picking',await visible());
  for(let n=0;n<3;n++){
   await control('pick');await wait(()=>requests===n+1&&picker,'real compositor picker');
   await click(...target);await wait(()=>completed===n+1,'pick completion');await pause(300);
   check(`screen pick ${n+1} restores a visible cursor over the settings parent`,await visible(),{html:read().dialog.html});
   move(target[0]+15,target[1]+20);await pause(200);check(`parent hover after pick ${n+1} stays visible`,await visible());
  }
  await control('pick');await wait(()=>requests===4&&picker,'cancel picker');move(...target);await pause(100);await key(Clutter.KEY_Escape);await wait(()=>completed===4,'cancel completion');await pause(200);
  check('cancelled screen picking restores the settings-parent cursor',await visible());
  await control('cancel');await wait(()=>!read().dialog,'cancel color panel');
  check('closing the color panel leaves settings usable with a visible cursor',await visible());
  await click(r.x+a.x+a.width/2,r.y+a.y+a.height/2);await wait(()=>read().dialog,'reopened dialog');await key(Clutter.KEY_Escape);await wait(()=>!read().dialog,'Escape dismisses color panel');
  check('Escape closes only the color dialog and retains settings',global.get_window_actors().some(a=>a.meta_window===settings));
  GLib.file_set_contents(`${directory}/color-probe-stop`,'stop');await pause(300);
 } catch(e) {
  results.push({name:'settings screen picker',passed:false,error:e.stack||String(e)});
 } finally {
  if(picker)picker._grabHelper.ungrab();
  GLib.file_set_contents(`${directory}/color-probe-stop`,'stop');
  if(child)child.force_exit();
  portal?.unexport();if(owner)Gio.bus_unown_name(owner);
  GLib.file_set_contents(`${directory}/results.json`,JSON.stringify(results,null,2));
 }
}

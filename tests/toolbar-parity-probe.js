// Both galleries execute in the same private desktop, theme and renderer.
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import Clutter from 'gi://Clutter';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {pause} from './common/process.js';
export async function probe(){
 const directory=GLib.getenv('CLIVUU_TEST_DIR'),results=[];let child,pointer;
 const wait=async file=>{for(let i=0;i<1000;i++){
  if(GLib.file_test(`${directory}/${file}`,GLib.FileTest.EXISTS))return;
  const request=`${directory}/gallery-capture-request.json`;
  if(GLib.file_test(request,GLib.FileTest.EXISTS)){
   const r=JSON.parse(new TextDecoder().decode(GLib.file_get_contents(request)[1])),done=`${directory}/${r.id}-done`;
   if(!GLib.file_test(done,GLib.FileTest.EXISTS)){
    pointer.notify_absolute_motion(GLib.get_monotonic_time(),30,40);await pause(80);
    const window=global.get_window_actors().map(a=>a.meta_window).find(w=>w.get_title()===r.title);if(!window)throw new Error(`missing popup parent ${r.title}`);
    const origin=window.get_buffer_rect(),stream=Gio.File.new_for_path(`${directory}/${r.id}.png`).replace(null,false,Gio.FileCreateFlags.PRIVATE,null);
    try{await new Shell.Screenshot().screenshot_area(origin.x+r.x,origin.y+r.y,r.width,r.height,stream);}finally{stream.close(null);}
    GLib.file_set_contents(`${directory}/${r.id}-location.json`,JSON.stringify({origin,request:r}));
    GLib.file_set_contents(done,'done');
   }
  }
  await pause(100);
 }throw new Error(`Toolbar gallery timeout: ${file}`);};
 try{
  Main.overview.hide();await pause(300);
  pointer=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
  pointer.notify_absolute_motion(GLib.get_monotonic_time(),30,40);
  const launcher=new Gio.SubprocessLauncher({flags:0});launcher.unsetenv('APPIMAGE');launcher.set_stderr_file_path(`${directory}/reference-toolbar.log`);
  launcher.setenv('GDK_SCALE',GLib.getenv('CLIVUU_GALLERY_SCALE')??'1',true);
  child=launcher.spawnv(['gjs','-m',`${GLib.getenv('CLIVUU_REFERENCE_GALLERY')}`]);await wait('reference-toolbar-gallery.json');child.wait(null);
  const reference=JSON.parse(new TextDecoder().decode(GLib.file_get_contents(`${directory}/reference-toolbar-gallery.json`)[1]));if(reference.error)throw new Error(reference.error);
  launcher.set_stderr_file_path(`${directory}/native-toolbar.log`);child=launcher.spawnv(['/tmp/clivuu-target/debug/examples/toolbar_gallery']);await wait('native-toolbar-gallery.json');child.wait(null);
  results.push({name:'both unmodified-style GJS and native GTK galleries rendered all cases',passed:true});
 }catch(e){results.push({name:'toolbar pixel comparison fixtures',passed:false,error:`${e.message}\n${e.stack}`});}
 finally{child?.force_exit();GLib.file_set_contents(`${directory}/results.json`,JSON.stringify(results));}
}

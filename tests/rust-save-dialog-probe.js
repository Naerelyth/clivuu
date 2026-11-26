import GLib from 'gi://GLib';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {pause,run} from './common/process.js';
export async function probe(){
 const directory=GLib.getenv('CLIVUU_TEST_DIR'),results=[];
 try{
  Main.overview.hide();await pause(300);
  for(const backend of ['x11','wayland']){
   await run(['/tmp/clivuu-target/release/examples/save_dialog_probe'],null,null,{GDK_BACKEND:backend,G_DEBUG:'fatal-warnings'});
   const state=JSON.parse(new TextDecoder().decode(GLib.file_get_contents(`${directory}/save-dialog.json`)[1]));
   for(const [name,passed] of Object.entries(state))results.push({name:`${backend} save chooser ${name}`,passed});
  }
 }catch(e){results.push({name:'save chooser exception',passed:false,error:e.message});}
 GLib.file_set_contents(`${directory}/results.json`,JSON.stringify(results));
}

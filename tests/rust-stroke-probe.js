import GLib from 'gi://GLib';
import {run} from './common/process.js';
export async function probe(){
 const directory=GLib.getenv('CLIVUU_TEST_DIR');let results=[];
 try{
  await run(['/tmp/clivuu-target/release/examples/stroke_probe'],null,null,{GDK_BACKEND:'wayland',G_DEBUG:'fatal-warnings'});
  results=JSON.parse(new TextDecoder().decode(GLib.file_get_contents(`${directory}/stroke-feedback.json`)[1]));
 }catch(e){results.push({name:'stroke feedback exception',passed:false,error:`${e.message}\n${e.stack}`});}
 GLib.file_set_contents(`${directory}/results.json`,JSON.stringify(results));
}

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {pause,run} from './common/process.js';
export async function probe(){
 const directory=GLib.getenv('CLIVUU_TEST_DIR'),results=[];
 try{
  Main.overview.hide();await pause(300);
  await run(['/tmp/clivuu-target/release/examples/preferences_sections_probe'],null,null,{GDK_BACKEND:'wayland',G_DEBUG:'fatal-warnings'});
  const frames=JSON.parse(new TextDecoder().decode(GLib.file_get_contents(`${directory}/section-frames.json`)[1]));
  const summaries={};
  for(const page of ['capture','save','control']){
   const rows=frames.filter(f=>(f.page??'capture')===page),heights=[...new Set(rows.map(f=>f.height))];
   summaries[page]={heights,frames:rows.length};
   results.push({name:`${page} section allocation trace`,passed:rows.length>5});
   if(GLib.getenv('CLIVUU_SECTION_BASELINE')!=='1')results.push({name:`${page} section switches keep the viewport height stable`,passed:heights.length===1&&heights[0]>0});
  }
  GLib.file_set_contents(`${directory}/section-summary.json`,JSON.stringify(summaries));
 }catch(e){results.push({name:'sections exception',passed:false,error:e.message});}
 GLib.file_set_contents(`${directory}/results.json`,JSON.stringify(results));
}

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Clutter from 'gi://Clutter';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {pause} from './common/process.js';
import {authorizeScreenshot} from './screenshot-access.js';
import {recordingPlayback} from './recording-playback.js';
export async function probe(observer) {
 const directory=GLib.getenv('CLIVUU_TEST_DIR'),results=[];let child,fixture;
 const check=(name,passed)=>{results.push({name,passed:!!passed});GLib.file_set_contents(`${directory}/audio-progress.json`,JSON.stringify(results));if(!passed)throw new Error(name);};
 const execute=r=>new Promise((resolve,reject)=>Gio.DBus.session.call('io.github.naerelyth.Clivuu.Desktop','/io/github/naerelyth/Clivuu/Desktop','io.github.naerelyth.Clivuu.Desktop1','Execute',new GLib.Variant('(s)',[JSON.stringify({protocol:1,...r})]),null,0,30000,null,(c,res)=>{try{resolve(JSON.parse(c.call_finish(res).deep_unpack()[0]));}catch(e){reject(e);}}));
 const current=async()=>(await execute({command:'status'})).recordingUi;
 const wait=async fn=>{for(let n=0;n<240;n++){if(await fn())return;await pause(50);}throw new Error('Recording audio timed out');};
 const pointer=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
 const action=async label=>{let b;await wait(async()=>{b=(await current())?.buttons.filter(b=>(b.label===label||b.label?.startsWith(label+' ('))&&b.enabled).at(-1);return b?.rect.width>0&&global.get_window_actors().some(a=>a.mapped&&['Clivuu recording','Clivuu playback'].includes(a.meta_window.get_title()));});pointer.notify_absolute_motion(GLib.get_monotonic_time(),b.rect.x+b.rect.width/2,b.rect.y+b.rect.height/2);await pause(100);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(60);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(120);};
 try {
  Main.overview.hide();
  fixture=Gio.Subprocess.new(['gjs','-m',`${directory}/fixture.js`],0);await pause(1200);
  const launcher=new Gio.SubprocessLauncher({flags:0});launcher.setenv('GDK_BACKEND','x11',true);launcher.setenv('CLIVUU_LANGUAGE','en',true);launcher.setenv('GST_DEBUG_DUMP_DOT_DIR',directory,true);launcher.set_stderr_file_path(`${directory}/audio-service.log`);
  child=launcher.spawnv([GLib.getenv('CLIVUU_TEST_DESKTOP_BINARY')||observer._workerPath()]);
  await wait(async()=>{try{return(await execute({command:'status'})).ready;}catch{return false;}});
  await authorizeScreenshot(()=>execute({command:'snip',rect:{x:220,y:180,width:640,height:320},output:'no-auto-save'}));
  await recordingPlayback({check,execute,current,action,wait,pointer,directory,observer});
  await execute({command:'exit'});
 }catch(e){results.push({name:'recording playback exception',passed:false,error:`${e.message}\n${e.stack}`});}
 finally{child?.force_exit();fixture?.force_exit();GLib.file_set_contents(`${directory}/results.json`,JSON.stringify(results));}
}

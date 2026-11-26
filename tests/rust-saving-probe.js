// Real portal, native chooser and encoder; all paths belong to the private test HOME.
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import GdkPixbuf from 'gi://GdkPixbuf';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {pause,run} from './common/process.js';
import {authorizeScreenshot} from './screenshot-access.js';
import {saveChooser} from './save-chooser.js';
export async function probe(observer){
 const directory=GLib.getenv('CLIVUU_TEST_DIR'),results=[];let child;
 const check=(name,passed)=>{results.push({name,passed:!!passed});GLib.file_set_contents(`${directory}/saving-progress.json`,JSON.stringify(results));if(!passed)throw new Error(name);};
 const execute=r=>new Promise((resolve,reject)=>Gio.DBus.session.call('io.github.naerelyth.Clivuu.Desktop','/io/github/naerelyth/Clivuu/Desktop','io.github.naerelyth.Clivuu.Desktop1','Execute',new GLib.Variant('(s)',[JSON.stringify({protocol:1,...r})]),null,0,120000,null,(c,res)=>{try{resolve(JSON.parse(c.call_finish(res).deep_unpack()[0]));}catch(e){reject(e);}}));
 const wait=async fn=>{for(let i=0;i<200;i++){if(await fn())return;await pause(60);}throw new Error('Saving timed out');};
 const status=()=>execute({command:'status'}),rect={x:100,y:160,width:400,height:250};
 const bytes=path=>GLib.file_get_contents(path)[1];
 try{
  Main.overview.hide();await pause(300);
  const packaged=GLib.getenv('CLIVUU_TEST_APPIMAGE')==='1',cli=packaged?observer._workerPath():'/tmp/clivuu-target/release/clivuu';
  const launcher=new Gio.SubprocessLauncher({flags:0});launcher.setenv('GDK_BACKEND','x11',true);launcher.setenv('CLIVUU_LANGUAGE','en',true);launcher.set_stderr_file_path(`${directory}/saving-service.log`);
  if(packaged){launcher.setenv('CLIVUU_DESKTOP_DIR','',true);child=launcher.spawnv([cli]);}else child=launcher.spawnv(['/tmp/clivuu-target/release/clivuu-desktop']);
  await wait(async()=>{try{return(await status()).ready;}catch{return false;}});
  const set=async(k,v)=>{await run([cli,'config','set',k,String(v)]);};
  await set('image-save-notify',false);await set('video-save-notify',false);
  const snip=output=>execute({command:'snip',rect,output});
  await authorizeScreenshot(()=>snip('no-auto-save'));
  const first=await snip('quick-save'),second=await snip('quick-save');
  check('default image quick save creates Pictures/Clivuu',first.path.startsWith(`${GLib.get_home_dir()}/Pictures/Clivuu/`)&&Gio.File.new_for_path(first.path).query_exists(null));
  check('quick save never overwrites an existing capture',first.path!==second.path);
  const same='$yyyy$/$MM$/media_%username%_$uuid$_$unix$_$unix_ms$';
  await set('image-quick-directory',`${directory}/images`);await set('image-quick-name',`${same}.jpg`);
  const image=await snip('quick-save'),data=bytes(image.path);
  check('shared naming renders dated folders, UUID, Unix seconds and milliseconds',/\/\d{4}\/\d{2}\/media_.*_[0-9a-f-]{36}_\d{10}_\d{13}\.jpg$/.test(image.path));
  check('JPEG export contains JPEG bytes and selected dimensions',data[0]===255&&data[1]===216&&GdkPixbuf.Pixbuf.new_from_file(image.path).width===400);
  await set('image-auto-save',true);await set('image-auto-directory',`${directory}/image-backups`);await set('image-auto-name','nested/auto_$uuid$.png');
  const copied=await snip('clipboard');check('completed clipboard capture also saves automatic copy',copied.automatic_path.includes('/image-backups/nested/')&&Gio.File.new_for_path(copied.automatic_path).query_exists(null));
  const suppressed=await snip('no-auto-save');check('no-auto-save explicitly suppresses automatic copy',!suppressed.automatic_path);
  let pending=snip('file-dialog');pending.catch(()=>{});await saveChooser();const cancelled=await pending;
  check('cancelled manual save does not generate an automatic copy',cancelled.cancelled&&!cancelled.automatic_path);
  await set('image-auto-save',false);await set('image-remember-extension',false);
  // Exercise an explicit extension through the real native Save action.
  const manual=`${directory}/manual-image.jpeg`;
  pending=snip('file-dialog');pending.catch(()=>{});await saveChooser(manual);const saved=await pending;
  GLib.file_set_contents(`${directory}/manual-save-reply.json`,JSON.stringify(saved));
  check('manual JPEG preserves .jpeg and encodes JPEG bytes',saved.path===manual&&bytes(manual)[0]===255&&bytes(manual)[1]===216);
  const defaults=await execute({command:'record',rect});await pause(600);const defaultSaved=await execute({command:'stop-recording'});
  const defaultMeta=JSON.parse(await run(['ffprobe','-v','error','-show_entries','stream=codec_name','-of','json',defaultSaved.path]));
  check('default recording output is MP4',defaultSaved.path.endsWith('.mp4')&&['h264','mpeg4'].includes(defaultMeta.streams[0].codec_name)&&defaults.path===defaultSaved.path);
  await set('video-directory',`${directory}/videos`);await set('video-quick-name',`${same}.mp4`);
  const started=await execute({command:'record',rect});await pause(800);const stopped=await execute({command:'stop-recording'});
  check('video uses exactly the same naming grammar as image',/\/\d{4}\/\d{2}\/media_.*_[0-9a-f-]{36}_\d{10}_\d{13}\.mp4$/.test(stopped.path)&&started.path===stopped.path);
  const media=JSON.parse(await run(['ffprobe','-v','error','-show_entries','stream=width,height','-of','json',stopped.path]));
  check('video primary is complete with selected dimensions',media.streams[0].width===400&&media.streams[0].height===250);
  pending=snip('file-dialog');pending.catch(()=>{});
  let saveWindow;
  await wait(()=>saveWindow=global.get_window_actors().map(a=>a.meta_window).find(w=>w.get_title()==='Save screenshot'));
  const savePid=saveWindow.get_pid();
  await execute({command:'exit'});
  await pending.catch(()=>null);
  await wait(()=>!global.get_window_actors().some(a=>a.meta_window===saveWindow));
  await wait(()=>!Gio.File.new_for_path(`/proc/${savePid}`).query_exists(null));
  check('service shutdown closes and reaps the native save helper',true);
 }catch(e){results.push({name:'saving exception',passed:false,error:`${e.message}\n${e.stack}`});}
 finally{child?.force_exit();GLib.file_set_contents(`${directory}/results.json`,JSON.stringify(results));}
}

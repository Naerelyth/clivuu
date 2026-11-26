// Real recording → stopped acquisition → playback → non-destructive late export.
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Clutter from 'gi://Clutter';
import GdkPixbuf from 'gi://GdkPixbuf';
import {pause,run} from './common/process.js';
import {saveChooser} from './save-chooser.js';
export async function recordingPlayback({check,execute,current,action,wait,pointer,observer,directory}) {
 const keyboard=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
 const click=async(x,y)=>{pointer.notify_absolute_motion(GLib.get_monotonic_time(),x,y);await pause(70);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(50);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(150);};
 const drag=async(x,y,dx,dy)=>{pointer.notify_absolute_motion(GLib.get_monotonic_time(),x,y);await pause(70);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);for(let n=1;n<=8;n++){pointer.notify_absolute_motion(GLib.get_monotonic_time(),x+dx*n/8,y+dy*n/8);await pause(35);}pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(200);};
 const pending=execute({command:'record'});pending.catch(()=>{});
 await wait(()=>global.get_window_actors().some(a=>a.mapped&&a.meta_window.get_title()==='Clivuu capture overlay'));
 await drag(220,180,640,320);await wait(async()=>(await current())?.phase==='ready');
 check('normal recording is the default and ready controls have no output-format choice',(await current()).mode==='normal'&&!(await current()).buttons.some(b=>b.label==='Recording format'));
 await action('Start recording');await pending;await pause(1000);await execute({command:'pause-recording'});await pause(650);await execute({command:'pause-recording'});await pause(1100);
 const stopped=await execute({command:'stop-recording'});await wait(async()=>(await current())?.phase==='playback');
 const original=await current();
 check('normal Stop hands native-pixel material to playback without publishing',stopped.path===null&&original.material.width===640&&original.material.height===320&&original.material.duration_ns>1e9);
 check('playback releases the compositor recording window',!global.get_window_actors().some(a=>a.mapped&&a.meta_window.get_title()==='Clivuu recording')&&original.visible);
 check('recorded material excludes pause wall time',original.material.duration_ns<3.6e9);
 pointer.notify_absolute_motion(GLib.get_monotonic_time(),100,100);
 await pause(200);await observer.captureArea({x:0,y:0,width:1280,height:800},`${directory}/recording-playback.png`);
 const geometry=await current();
 check('playback adds a compact progress row above an ordinary-height action row',geometry.timeline.height>0&&geometry.timeline.height<32&&geometry.toolbar.height-geometry.timeline.height-1===32);
 const sampleRgb=async(x,y,name)=>{const p=`${directory}/sample-pb-${name}.png`;await observer.captureArea({x:Math.round(x),y:Math.round(y),width:1,height:1},p);return [...GdkPixbuf.Pixbuf.new_from_file(p).get_pixels()].slice(0,3);};
 await run(['gsettings','set','org.gnome.desktop.interface','color-scheme','prefer-dark']);
 let pbDark=false;for(let n=0;n<60;n++){const tb=(await current()).toolbar,rgb=await sampleRgb(tb.x+2,tb.y+tb.height-8,`dark-${n}`);if(rgb[0]<70&&rgb[1]<70&&rgb[2]<80){pbDark=true;break;}await pause(50);}
 await run(['gsettings','set','org.gnome.desktop.interface','color-scheme','default']);
 let pbLight=false;for(let n=0;n<60;n++){const tb=(await current()).toolbar,rgb=await sampleRgb(tb.x+2,tb.y+tb.height-8,`light-${n}`);if(rgb[0]>210&&rgb[1]>210&&rgb[2]>210){pbLight=true;break;}await pause(50);}
 check('playback toolbar follows live system theme transitions',pbDark&&pbLight);
 await run([observer._workerPath(),'config','set','accent-color','#ff0000']);
 let pbRed=false;for(let n=0;n<60;n++){const rc=(await current()).rect,rgb=await sampleRgb(rc.x+60,rc.y-2,`red-${n}`);if(rgb[0]>200&&rgb[1]<60&&rgb[2]<60){pbRed=true;break;}await pause(50);}
 await run([observer._workerPath(),'config','set','accent-color','#00ff00']);
 let pbGreen=false;for(let n=0;n<60;n++){const rc=(await current()).rect,rgb=await sampleRgb(rc.x+60,rc.y-2,`green-${n}`);if(rgb[1]>200&&rgb[0]<60&&rgb[2]<60){pbGreen=true;break;}await pause(50);}
 const pbRect=geometry.rect;const pbSmall=await sampleRgb(pbRect.x+pbRect.width/2,pbRect.y-11,'bw2');
 await run([observer._workerPath(),'config','set','capture-border-width','8']);
 let pbLarge=false;for(let n=0;n<60;n++){const rgb=await sampleRgb(pbRect.x+pbRect.width/2,pbRect.y-11,`bw8-${n}`);if(rgb[1]>200&&rgb[0]<60&&rgb[2]<60){pbLarge=true;break;}await pause(50);}
 check('playback crop border and anchor size follow live accent-color and capture-border-width changes',pbRed&&pbGreen&&!(pbSmall[1]>200&&pbSmall[0]<60)&&pbLarge);
 await run([observer._workerPath(),'config','set','capture-border-width','2']);
 await run([observer._workerPath(),'config','set','accent-color','#bd93f9']);await pause(200);
 await action('Play / pause');await pause(250);await action('Play / pause');
 const timeline=(await current()).timeline;await click(timeline.x+timeline.width*.6,timeline.y+timeline.height/2);
 await drag(timeline.x+8,timeline.y+timeline.height/2,50,0);
 await wait(async()=>(await current()).edit.start_ns>0);
 await action('Playback speed');await action('2.0×');
 check('playback timeline trim and speed modify the shared edit snapshot',(await current()).edit.start_ns>0&&(await current()).edit.rate===2);
 const grip=(await current()).buttons.find(b=>b.label==='Move toolbar').rect;
 const origGripY=grip.y+grip.height/2;
 await drag(grip.x+grip.width/2,origGripY,0,750-origGripY);
 await action('Playback speed');
 await wait(async()=>(await current()).popupBounds?.height>0);
 const upState=await current(),speedAnchor=upState.buttons.find(b=>b.label==='Playback speed').rect,upPop=upState.popupBounds;
 const upGap=speedAnchor.y-(upPop.y+upPop.height);
 check('upward-flipped popup hugs its anchor button without excess vertical gap',upGap>=2&&upGap<=10);
 await action('Playback speed');
 const movedGrip=(await current()).buttons.find(b=>b.label==='Move toolbar').rect;
 await drag(movedGrip.x+movedGrip.width/2,movedGrip.y+movedGrip.height/2,0,origGripY-(movedGrip.y+movedGrip.height/2));
 const r=(await current()).rect;await drag(r.x+r.width,r.y+r.height/2,-40,0);
 await wait(async()=>(await current()).edit.crop.width<640);
 check('playback frame edges crop native source pixels',(await current()).edit.crop.width===600&&(await current()).edit.crop.height===320);
 if(original.material.tracks.length===2) {
  await action('Recorded audio');
  await wait(async()=>(await current()).popupBounds?.width>0);
  const audioPopup=(await current()).popupBounds;
  await observer.captureArea({x:Math.round(audioPopup.x),y:Math.round(audioPopup.y),width:Math.round(audioPopup.width),height:Math.round(audioPopup.height)},`${directory}/audio-popup-compact.png`);
  check('playback recorded audio popup uses compact bounds',audioPopup.width<=245&&audioPopup.height<=56);
  const controls=(await current()).buttons;
  const volume=controls.find(b=>b.name==='playback-system-volume').rect;
  await click(volume.x+volume.width*.25,volume.y+volume.height/2);
  const mute=(await current()).buttons.find(b=>b.name==='playback-microphone-mute').rect;
  await click(mute.x+mute.width/2,mute.y+mute.height/2);
  await observer.captureArea({x:Math.round(audioPopup.x),y:Math.round(audioPopup.y),width:Math.round(audioPopup.width),height:Math.round(audioPopup.height)},`${directory}/audio-popup-muted.png`);
  check('playback changes system gain and microphone mute independently',(await current()).edit.audio[0].volume<.6&&(await current()).edit.audio[0].volume>.1&&(await current()).edit.audio[1].muted);
  const resetGain=(await current()).buttons.find(b=>b.label==='Reset volume').rect;
  await click(resetGain.x+resetGain.width/2,resetGain.y+resetGain.height/2);
  check('clicking percentage readout resets track volume to 100% and restores test gain',Math.abs((await current()).edit.audio[0].volume-1)<.01);
  await click(volume.x+volume.width*.25,volume.y+volume.height/2);
  await click(r.x+r.width/2,r.y+r.height/2);
 }
 if((await current()).playing)await action('Play / pause');
 await action('Play / pause');await wait(async()=>!(await current()).playing);
 check('playback completion resets the play state after the trimmed end',!(await current()).playing);
 await action('Save recording');await wait(()=>global.get_window_actors().some(a=>a.mapped&&a.meta_window.get_title()==='Save recording'));
 check('late native save chooser owns focus with playback decorations unmapped',!(await current()).visible&&global.display.focus_window?.get_title()==='Save recording'&&!global.get_window_actors().some(a=>a.mapped&&a.meta_window.get_title()==='Clivuu playback'));
 await saveChooser();await wait(async()=>(await current())?.phase==='playback'&&(await current()).visible);
 check('cancelling late save keeps playback material and edits',(await current()).edit.crop.width===600&&(await current()).edit.rate===2);
 for(const format of ['GIF','MP4','WebM']){
  await action('Export format');await action(format);
  const afterFormat=await current();
  check(`${format} keeps all playback controls inside the toolbar bounds`,Math.round(afterFormat.toolbar.width)===Math.round(original.toolbar.width)&&afterFormat.buttons.every(b=>b.rect.x>=afterFormat.toolbar.x&&b.rect.x+b.rect.width<=afterFormat.toolbar.x+afterFormat.toolbar.width));
  const path=`${directory}/playback.${format.toLowerCase()}`;
  if(format==='GIF') {
   keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Control_L,Clutter.KeyState.PRESSED);keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_s,Clutter.KeyState.PRESSED);keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_s,Clutter.KeyState.RELEASED);keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Control_L,Clutter.KeyState.RELEASED);
  }else await action('Save recording');
  await saveChooser(path);
  await wait(async()=>Gio.File.new_for_path(path).query_exists(null)&&(await current()).phase==='playback');
  const meta=JSON.parse(await run(['ffprobe','-v','error','-show_entries','stream=codec_name,width,height:format=duration','-of','json',path]));
  const state=await current();
  check(`late ${format} export preserves material, crop and speed`,meta.streams[0].width===600&&meta.streams[0].height===320&&state.edit.rate===2&&state.material.duration_ns===original.material.duration_ns&&Number(meta.format.duration)<original.material.duration_ns/1e9*.6);
 }
 await action('Quick save');await wait(async()=>(await current()).phase==='playback'&&(await current()).message.startsWith('Recording saved: '));
 const quickPath=(await current()).message.slice('Recording saved: '.length);
 check('playback quick save publishes the selected format to the configured directory',quickPath.endsWith('.webm')&&Gio.File.new_for_path(quickPath).query_exists(null));
 await action('Copy file');await wait(async()=>(await current()).phase==='playback'&&(await current()).message==='Recording file copied');
 const uri=(await run(['gjs','-m',`${directory}/clipboard-read.js`,'--mime','text/uri-list'],null,null,{GDK_BACKEND:'x11'})).trim();
 check('playback copy exports a real file and offers its standard URI',uri.startsWith('file:')&&Gio.File.new_for_uri(uri).query_exists(null));
 if(original.material.tracks.length===2){
  await action('Restart recording');await action('Discard recording');await wait(async()=>(await current())?.phase==='ready');
  check('confirmed playback rerecord returns to setup with the original region',(await current()).rect.width===640&&(await current()).rect.height===320);
  await action('Recording mode');await wait(async()=>(await current())?.mode==='quick');await action('Start recording');await wait(async()=>(await current())?.phase==='recording');await pause(600);
  const quick=await execute({command:'stop-recording'});
  check('quick mode stops directly into MP4 without playback',quick.path?.endsWith('.mp4')&&Gio.File.new_for_path(quick.path).query_exists(null)&&!(await current()));
 }else{
 await action('Close playback');await action('Discard recording');await wait(async()=>!(await current()));
 }
 check('playback close destroys all owned surfaces and releases the session',!(await execute({command:'status'})).recording&&!global.get_window_actors().some(a=>a.meta_window.get_title()==='Clivuu playback'));
}

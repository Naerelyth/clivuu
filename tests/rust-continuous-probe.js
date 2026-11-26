import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Clutter from 'gi://Clutter';
import GdkPixbuf from 'gi://GdkPixbuf';
import {authorizeScreenshot} from './screenshot-access.js';
import {saveChooser} from './save-chooser.js';
import {recordingNative} from './recording-native.js';
import {recordingPlayback} from './recording-playback.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {pause,run} from './common/process.js';
export async function probe(observer){
 const directory=GLib.getenv('CLIVUU_TEST_DIR'),results=[];let fixture,child;
 const notifications=()=>Main.messageTray.getSources().filter(s=>s.title==='Clivuu').flatMap(s=>s.notifications??[]);
 const clearNotifications=async()=>{await pause(300);for(const n of notifications())n.destroy();};
 const silent=async()=>{await pause(250);return notifications().length===0;};
 const check=(name,passed)=>{results.push({name,passed:!!passed});GLib.file_set_contents(`${directory}/continuous-progress.json`,JSON.stringify(results));if(!passed)throw new Error(name);};
 try{
  GLib.file_set_contents(`${directory}/fixture-color`,'#2468ac');
  fixture=Gio.Subprocess.new(['gjs','-m',`${directory}/fixture.js`],Gio.SubprocessFlags.NONE);await pause(1600);Main.overview.hide();await pause(500);
  const launcher=new Gio.SubprocessLauncher({flags:0});launcher.unsetenv('APPIMAGE');launcher.setenv('GDK_BACKEND','x11',true);launcher.set_stderr_file_path(`${directory}/continuous.log`);
  const desktop=GLib.getenv('CLIVUU_TEST_DESKTOP_BINARY')||'/tmp/clivuu-target/debug/clivuu-desktop',build=GLib.path_get_dirname(desktop);
  child=launcher.spawnv([`${build}/examples/continuous_probe`,directory]);
  await new Promise((resolve,reject)=>child.wait_check_async(null,(p,r)=>{try{p.wait_check_finish(r);resolve();}catch(e){reject(e);}}));
  const result=JSON.parse(new TextDecoder().decode(GLib.file_get_contents(`${directory}/continuous-result.json`)[1]));
  if(result.error)throw new Error(result.error);
  check('native persistent PipeWire appsink returns the selected logical region',result.dimensions.join(',')==='640,320'&&result.secondDimensions.join(',')==='640,320');
  check('native frame pixels preserve the Wayland fixture without PNG intermediates',result.pixel.slice(0,3).join(',')==='36,104,172');
  check('native encoder finalizes and exports a completed WebM',result.size>0&&result.path===`${directory}/native-video.webm`);
  check('completed WebM decodes with the selected region dimensions',result.decoded?.join(',')==='640,320');
  check('recording stop releases active state and repeated Stop returns the same result',!result.active&&result.repeatedStop===result.path);
  const BUS='io.github.naerelyth.Clivuu.Desktop',PATH='/io/github/naerelyth/Clivuu/Desktop',IFACE=`${BUS}1`;
  const execute=request=>new Promise((resolve,reject)=>Gio.DBus.session.call(BUS,PATH,IFACE,'Execute',new GLib.Variant('(s)',[JSON.stringify({protocol:1,...request})]),null,Gio.DBusCallFlags.NO_AUTO_START,30000,null,(c,r)=>{try{resolve(JSON.parse(c.call_finish(r).deep_unpack()[0]));}catch(e){reject(e);}}));
  const applications=`${GLib.getenv('XDG_DATA_HOME')}/applications`;GLib.mkdir_with_parents(applications,0o700);
  GLib.file_set_contents(`${applications}/io.github.naerelyth.clivuu.desktop`,`[Desktop Entry]\nType=Application\nName=Clivuu\nExec=${observer._workerPath()}\nStartupWMClass=io.github.naerelyth.clivuu\n`);
  child=launcher.spawnv([desktop]);
  for(let n=0;n<180;n++){try{if((await execute({command:'status'})).ready)break;}catch{}await pause(50);}
  const recordStarted=GLib.get_monotonic_time();
  const recording=await execute({command:'record',rect:{x:220,y:180,width:640,height:320},output:`${directory}/service-video.webm`});
  await pause(1000);
  const pointer=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
  const keyboard=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
  const current=async()=>(await execute({command:'status'})).recordingUi;
  const wait=async predicate=>{for(let n=0;n<240;n++){if(await predicate())return;await pause(50);}throw new Error('Recording controls timed out');};
  const click=async r=>{pointer.notify_absolute_motion(GLib.get_monotonic_time(),r.x+r.width/2,r.y+r.height/2);await pause(100);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(60);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(120);};
  const action=async label=>{let b;await wait(async()=>{b=(await current())?.buttons.filter(b=>(b.label===label||b.label?.startsWith(label+' ('))&&b.enabled).at(-1);return b?.rect.width>0&&global.get_window_actors().some(a=>a.mapped&&['Clivuu recording','Clivuu playback'].includes(a.meta_window.get_title()));});GLib.file_set_contents(`${directory}/last-recording-action.json`,JSON.stringify({label,button:b,state:await current(),actors:global.get_window_actors().map(a=>({title:a.meta_window.get_title(),mapped:a.mapped}))}));await click(b.rect);};
  await wait(async()=> (await current())?.phase==='recording');
  check('recording retains a 32px live toolbar outside its original region',(await current()).visible&&(await current()).toolbar.height===32&&(await current()).rect.width===640);
  await observer.captureArea({x:0,y:0,width:1280,height:800},`${directory}/recording-active.png`);
  await action('Pause recording');await wait(async()=> (await current()).phase==='paused');
  const stoppedClock=(await current()).elapsed;await pause(1600);
  check('pause freezes the active-time label and keeps controls available',(await current()).elapsed===stoppedClock&&(await current()).visible);
  await action('Resume recording');await wait(async()=> (await current()).phase==='recording');await pause(900);
  check('Desktop1 recording retains its capture session after Start returns',recording.recording&&(await execute({command:'status'})).recording&&!GLib.file_test(recording.path,GLib.FileTest.EXISTS));
  const stops=await Promise.all([execute({command:'stop-recording'}),execute({command:'stop-recording'})]);
  check('concurrent Desktop1 Stop requests return the same finalized file',stops.every(s=>s.path===recording.path)&&!(await execute({command:'status'})).recording);
  const media=JSON.parse(await run(['ffprobe','-v','error','-show_entries','stream=width,height','-show_entries','format=duration','-of','json',recording.path]));
  check('Desktop1 exported recording is decodable with a real duration',media.streams[0].width===640&&media.streams[0].height===320&&Number(media.format.duration)>.5&&Number(media.format.duration)<(GLib.get_monotonic_time()-recordStarted)/1e6-1.2);

  check('finalization releases every live recording surface',!(await current())&&!global.get_window_actors().some(a=>a.meta_window.get_title()==='Clivuu recording'));
  for(const ext of ['mp4','gif']){
   const path=`${directory}/format-${ext}.${ext}`;
   const result=await execute({command:'record',rect:{x:220,y:180,width:640,height:320},output:path});await pause(850);
   await execute({command:'stop-recording'});
   const metadata=JSON.parse(await run(['ffprobe','-v','error','-show_entries','stream=codec_name,width,height','-of','json',path]));
   check(`${ext} recording exports real decodable format with correct dimensions`,result.path===path&&metadata.streams[0].width===640&&metadata.streams[0].height===320&&(ext==='gif'?metadata.streams[0].codec_name==='gif':['h264','mpeg4'].includes(metadata.streams[0].codec_name)));
   const decoded=`${directory}/decoded-${ext}.png`;await run(['ffmpeg','-nostdin','-y','-loglevel','error','-i',path,'-frames:v','1',decoded]);
   const pix=GdkPixbuf.Pixbuf.new_from_file(decoded),bytes=pix.get_pixels(),channels=pix.get_n_channels(),stride=pix.get_rowstride();
   check(`${ext} output keeps toolbar and recording frame outside sampled pixels`,[[0,0],[639,0],[0,319],[639,319]].every(([x,y])=>[36,104,172].every((v,c)=>Math.abs(bytes[y*stride+x*channels+c]-v)<7)));

  }
  const full=await execute({command:'record',rect:{x:0,y:0,width:1280,height:800},output:`${directory}/full.webm`});await pause(500);
  check('overlapping full-screen controls hide during capture',!(await current()).visible);
  await execute({command:'pause-recording'});check('paused full-screen controls reappear for interaction',(await current()).visible);
  await execute({command:'pause-recording'});check('resuming hides overlapping controls before sampling',!(await current()).visible);
  await execute({command:'stop-recording'});
  const retry=await execute({command:'record',rect:{x:220,y:180,width:640,height:320},output:`${directory}/restarted.webm`});await pause(500);
  await action('Restart recording');await wait(async()=> (await current()).popup);
  await action('Restart recording');await wait(async()=> (await current()).phase==='recording');await pause(650);
  await execute({command:'stop-recording'});
  check('confirmed restart publishes only the replacement recording',GLib.file_test(retry.path,GLib.FileTest.EXISTS));
  await clearNotifications();
  await execute({command:'record',rect:{x:220,y:180,width:640,height:320},output:`${directory}/discard.webm`});await pause(450);
  await action('Discard recording');await wait(async()=> (await current()).popup);await action('Discard recording');await wait(async()=>!(await execute({command:'status'})).recording);
  check('confirmed discard releases the session without publishing a file',!GLib.file_test(`${directory}/discard.webm`,GLib.FileTest.EXISTS)&&!(await current()));
  check('discarding a running recording posts no cancellation notification',await silent());
  const selecting=authorizeScreenshot(()=>execute({command:'record',output:`${directory}/selection-cancel.webm`}));selecting.catch(()=>{});
  await wait(()=>global.get_window_actors().some(a=>a.meta_window.get_title()==='Clivuu capture overlay'));
  await clearNotifications();await execute({command:'stop-recording'});const cancelledSelection=await selecting;
  check('stopping during initial selection returns cancellation without any notification',cancelledSelection.cancelled&&await silent()&&!global.get_window_actors().some(a=>a.meta_window.get_title()==='Clivuu capture overlay'));
  // Both direct area and interactive/tray entry use the same controls.
  const pending=authorizeScreenshot(()=>execute({command:'record',output:`${directory}/ready.webm`}));pending.catch(()=>{});
  await wait(()=>global.get_window_actors().some(a=>a.meta_window.get_title()==='Clivuu capture overlay'));
  pointer.notify_absolute_motion(GLib.get_monotonic_time(),220,180);await pause(100);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(60);pointer.notify_absolute_motion(GLib.get_monotonic_time(),860,500);await pause(100);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(120);
  await wait(async()=> (await current())?.phase==='ready');
  check('interactive selection enters persistent ready controls rather than immediate encoding',(await current()).visible);
  const drag=async(label,dx,dy)=>{let b;await wait(async()=>{const s=await current();b=s.buttons.find(b=>b.label===label);return global.get_window_actors().some(a=>a.mapped&&a.meta_window.get_title()==='Clivuu recording')&&b?.rect.width>0&&b.rect.x>=s.toolbar.x&&b.rect.y>=s.toolbar.y&&b.rect.x+b.rect.width<=s.toolbar.x+s.toolbar.width+1&&b.rect.y+b.rect.height<=s.toolbar.y+s.toolbar.height+1;});const x=b.rect.x+b.rect.width/2,y=b.rect.y+b.rect.height/2;pointer.notify_absolute_motion(GLib.get_monotonic_time(),x,y);await pause(80);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(60);for(let n=1;n<=8;n++){pointer.notify_absolute_motion(GLib.get_monotonic_time(),x+dx*n/8,y+dy*n/8);await pause(25);}pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(200);};
  GLib.file_set_contents(`${directory}/ready-desktop-context.json`,JSON.stringify({focus:global.display.focus_window?.get_title(),pointer:global.get_pointer(),modal:Main.modalCount,windows:global.get_window_actors().map(a=>({title:a.meta_window.get_title(),type:a.meta_window.get_window_type(),rect:a.meta_window.get_frame_rect(),mapped:a.mapped}))}));
  const original=await current();await drag('Move toolbar',-60,20);const moved=await current();GLib.file_set_contents(`${directory}/ready-toolbar-drag.json`,JSON.stringify({original,moved}));
  check('ready toolbar uses the common grip without moving the source region',Math.abs(moved.toolbar.x-original.toolbar.x+60)<2&&Math.abs(moved.toolbar.y-original.toolbar.y-20)<2&&moved.rect.x===original.rect.x&&moved.rect.y===original.rect.y);
  for(let n=0;n<2;n++){await drag('Move toolbar',-30,-10);await drag('Move toolbar',30,10);}
  const roundtrip=await current();check('repeated grip drags coexist with ready frame resize input',Math.abs(roundtrip.toolbar.x-moved.toolbar.x)<2&&Math.abs(roundtrip.toolbar.y-moved.toolbar.y)<2&&roundtrip.rect.x===original.rect.x);

  await drag('Move recording region',20,10);const translated=await current();
  check('ready region movement preserves its dimensions and custom toolbar offset',translated.rect.x===original.rect.x+20&&translated.rect.y===original.rect.y+10&&translated.rect.width===original.rect.width&&Math.abs(translated.toolbar.x-moved.toolbar.x-20)<2&&Math.abs(translated.toolbar.y-moved.toolbar.y-10)<2);

  await observer.captureArea({x:0,y:0,width:1280,height:800},`${directory}/recording-ready.png`);
  const sampleRgb=async(x,y,name)=>{const p=`${directory}/sample-${name}.png`;await observer.captureArea({x:Math.round(x),y:Math.round(y),width:1,height:1},p);return [...GdkPixbuf.Pixbuf.new_from_file(p).get_pixels()].slice(0,3);};
  pointer.notify_absolute_motion(GLib.get_monotonic_time(),5,750);await pause(100);
  await run(['gsettings','set','org.gnome.desktop.interface','color-scheme','prefer-light']);
  await wait(async()=>{const tb=(await current()).toolbar,rgb=await sampleRgb(tb.x+2,tb.y+16,'rec-light');return rgb[0]>210&&rgb[1]>210&&rgb[2]>210;});
  await run(['gsettings','set','org.gnome.desktop.interface','color-scheme','prefer-dark']);
  let darkToolbar=false;for(let n=0;n<60;n++){const tb=(await current()).toolbar,rgb=await sampleRgb(tb.x+2,tb.y+16,`rec-dark-${n}`);if(rgb[0]<70&&rgb[1]<70&&rgb[2]<80){darkToolbar=true;break;}await pause(50);}
  check('ready recording toolbar follows live system dark scheme switch',darkToolbar);
  await run(['gsettings','set','org.gnome.desktop.interface','color-scheme','default']);
  let lightToolbar=false;for(let n=0;n<60;n++){const tb=(await current()).toolbar,rgb=await sampleRgb(tb.x+2,tb.y+16,`rec-light2-${n}`);if(rgb[0]>210&&rgb[1]>210&&rgb[2]>210){lightToolbar=true;break;}await pause(50);}
  check('ready recording toolbar follows live system light scheme restore',lightToolbar);
  await run([observer._workerPath(),'config','set','accent-color','#ff0000']);
  let redBorder=false;for(let n=0;n<60;n++){const rc=(await current()).rect,rgb=await sampleRgb(rc.x+60,rc.y-2,`rec-red-${n}`);if(rgb[0]>200&&rgb[1]<60&&rgb[2]<60){redBorder=true;break;}await pause(50);}
  await run([observer._workerPath(),'config','set','accent-color','#00ff00']);
  let greenBorder=false;for(let n=0;n<60;n++){const rc=(await current()).rect,rgb=await sampleRgb(rc.x+60,rc.y-2,`rec-green-${n}`);if(rgb[1]>200&&rgb[0]<60&&rgb[2]<60){greenBorder=true;break;}await pause(50);}
  check('ready recording border follows live accent-color changes',redBorder&&greenBorder);
  const rcBefore=(await current()).rect;
  const smallAnchorOuter=await sampleRgb(rcBefore.x+rcBefore.width/2,rcBefore.y-11,'anchor-bw2');
  await run([observer._workerPath(),'config','set','capture-border-width','8']);
  let largeAnchorOuter=false;for(let n=0;n<60;n++){const rgb=await sampleRgb(rcBefore.x+rcBefore.width/2,rcBefore.y-11,`anchor-bw8-${n}`);if(rgb[1]>200&&rgb[0]<60&&rgb[2]<60){largeAnchorOuter=true;break;}await pause(50);}
  check('anchor radius expands outward from border centerline with capture-border-width (bw=2 vs bw=8)',smallAnchorOuter[1]<140&&largeAnchorOuter);
  const grabX=rcBefore.x-13,grabY=rcBefore.y-4;
  pointer.notify_absolute_motion(GLib.get_monotonic_time(),grabX,grabY);await pause(80);
  const hoverCursor=(await current()).pointerCursor;
  pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(60);
  const dragCursors=[(await current()).pointerCursor];
  for(const stepX of [grabX-4,grabX-28,grabX+36,grabX-20]){pointer.notify_absolute_motion(GLib.get_monotonic_time(),stepX,grabY);await pause(30);dragCursors.push((await current()).pointerCursor);}
  pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(150);
  const resizedAnchor=(await current()).rect;
  check('enlarged anchor outer edge acts as resize hit target',resizedAnchor.x<rcBefore.x-10&&resizedAnchor.width>rcBefore.width+10);
  check('corner resize drag retains nwse-resize cursor throughout fast multi-step drag',hoverCursor==='nwse-resize'&&dragCursors.every(c=>c==='nwse-resize'));
  const bottomGrabX=resizedAnchor.x+resizedAnchor.width/2,bottomGrabY=resizedAnchor.y+resizedAnchor.height+2;
  pointer.notify_absolute_motion(GLib.get_monotonic_time(),bottomGrabX,bottomGrabY);await pause(80);
  const bottomHoverCursor=(await current()).pointerCursor;
  pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(60);
  const bottomDragCursors=[(await current()).pointerCursor];
  const tbNow=(await current()).toolbar;
  for(const targetY of [tbNow.y+tbNow.height/2,bottomGrabY+48,bottomGrabY]){pointer.notify_absolute_motion(GLib.get_monotonic_time(),bottomGrabX,targetY);await pause(35);bottomDragCursors.push((await current()).pointerCursor);}
  pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(150);
  check('bottom border resize drag across toolbar retains ns-resize cursor without resetting to default',bottomHoverCursor==='ns-resize'&&bottomDragCursors.every(c=>c==='ns-resize'));
  await run([observer._workerPath(),'config','set','capture-border-width','2']);
  await run([observer._workerPath(),'config','set','accent-color','#bd93f9']);
  await pause(200);
  await clearNotifications();await execute({command:'stop-recording'});const cancelledReady=await pending;
  check('ready cancellation is a normal result and posts no notification',cancelledReady.cancelled&&await silent());
  check('Stop cancels ready startup and destroys its controls',!(await current())&&!GLib.file_test(`${directory}/ready.webm`,GLib.FileTest.EXISTS));
  const snipLive=execute({command:'snip',output:'no-auto-save'});snipLive.catch(()=>{});
  await wait(()=>global.get_window_actors().some(a=>a.mapped&&a.meta_window.get_title()==='Clivuu capture overlay'));
  pointer.notify_absolute_motion(GLib.get_monotonic_time(),220,180);await pause(80);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(50);pointer.notify_absolute_motion(GLib.get_monotonic_time(),860,500);await pause(80);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(120);
  pointer.notify_absolute_motion(GLib.get_monotonic_time(),5,750);await pause(100);
  await run(['gsettings','set','org.gnome.desktop.interface','color-scheme','prefer-dark']);
  let snipDark=false;for(let n=0;n<60;n++){const rgb=await sampleRgb(856,524,`snip-dark-${n}`);if(rgb[0]<70&&rgb[1]<70&&rgb[2]<80){snipDark=true;break;}await pause(50);}
  await run(['gsettings','set','org.gnome.desktop.interface','color-scheme','default']);
  let snipLight=false;for(let n=0;n<60;n++){const rgb=await sampleRgb(856,524,`snip-light-${n}`);if(rgb[0]>210&&rgb[1]>210&&rgb[2]>210){snipLight=true;break;}await pause(50);}
  await run([observer._workerPath(),'config','set','accent-color','#ff0000']);
  let snipRed=false;for(let n=0;n<60;n++){const rgb=await sampleRgb(280,179,`snip-red-${n}`);if(rgb[0]>200&&rgb[1]<60&&rgb[2]<60){snipRed=true;break;}await pause(50);}
  await run([observer._workerPath(),'config','set','accent-color','#00ff00']);
  let snipGreen=false;for(let n=0;n<60;n++){const rgb=await sampleRgb(280,179,`snip-green-${n}`);if(rgb[1]>200&&rgb[0]<60&&rgb[2]<60){snipGreen=true;break;}await pause(50);}
  const snipSmall=await sampleRgb(540,169,'snip-bw2');
  await run([observer._workerPath(),'config','set','capture-border-width','8']);
  let snipLarge=false;for(let n=0;n<60;n++){const rgb=await sampleRgb(540,169,`snip-bw8-${n}`);if(rgb[1]>200&&rgb[0]<60&&rgb[2]<60){snipLarge=true;break;}await pause(50);}
  pointer.notify_absolute_motion(GLib.get_monotonic_time(),207,176);await pause(80);
  pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(60);
  pointer.notify_absolute_motion(GLib.get_monotonic_time(),180,176);await pause(80);
  pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(150);
  const snipResized=await sampleRgb(190,179,'snip-resized');
  check('capture overlay follows live system theme, accent-color and capture-border-width anchor expansion',snipDark&&snipLight&&snipRed&&snipGreen&&snipSmall[1]<140&&snipLarge&&snipResized[1]>200&&snipResized[0]<60);
  await run([observer._workerPath(),'config','set','capture-border-width','2']);
  await run([observer._workerPath(),'config','set','accent-color','#bd93f9']);await pause(150);
  keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Escape,Clutter.KeyState.PRESSED);keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Escape,Clutter.KeyState.RELEASED);
  await wait(()=>!global.get_window_actors().some(a=>a.mapped&&a.meta_window.get_title()==='Clivuu capture overlay'));await snipLive;
  const manual=execute({command:'record',output:'file-dialog'});manual.catch(()=>{});
  await wait(()=>global.get_window_actors().some(a=>a.meta_window.get_title()==='Clivuu capture overlay'));
  pointer.notify_absolute_motion(GLib.get_monotonic_time(),220,180);await pause(80);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(50);pointer.notify_absolute_motion(GLib.get_monotonic_time(),860,500);await pause(80);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(100);
  await wait(async()=> (await current())?.phase==='ready');
  await action('Frame rate');await action('16 FPS');await action('Recording mode');await wait(async()=>(await current())?.mode==='quick');await action('Recording mode');await action('Recording delay');await action('5 s');
  check('ready controls select FPS mode and countdown without an export format',(await current()).fps===16&&(await current()).mode==='normal'&&(await current()).delay===5&&!(await current()).buttons.some(b=>b.label==='Recording format'));
  keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Return,Clutter.KeyState.PRESSED);keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Return,Clutter.KeyState.RELEASED);
  await wait(async()=>(await current())?.phase==='countdown');
  check('recording starts without an early save chooser',!global.get_window_actors().some(a=>a.mapped&&a.meta_window.get_title()==='Save recording'));
  await clearNotifications();await action('Discard recording');await wait(async()=> (await current()).popup);await action('Discard recording');const cancelledCountdown=await manual;
  check('countdown cancellation is a normal result and posts no notification',cancelledCountdown.cancelled&&await silent());
  check('confirmed countdown discard cancels startup and leaves no published video',!(await current())&&!GLib.file_test(`${directory}/countdown.mp4`,GLib.FileTest.EXISTS));
  await recordingPlayback({check,execute,current,action,wait,pointer,observer,directory});
  await execute({command:'exit'});
  await recordingNative({check,observer});

 }catch(e){results.push({name:'native continuous exception',passed:false,error:`${e.message}\n${e.stack}`});}
 finally{child?.force_exit();fixture?.force_exit();GLib.file_set_contents(`${directory}/results.json`,JSON.stringify(results));}
}

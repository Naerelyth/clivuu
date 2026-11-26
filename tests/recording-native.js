// Native layer-shell input and presentation fixture, no capture backend substitution.
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Clutter from 'gi://Clutter';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {pause,run} from './common/process.js';
export async function recordingNative({check,observer}) {
 const directory=GLib.getenv('CLIVUU_TEST_DIR'),prefix=GLib.getenv('CLIVUU_TEST_NIRI_PREFIX')||'/tmp/clivuu-niri';
 if(!GLib.file_test(`${prefix}/usr/bin/niri`,GLib.FileTest.IS_EXECUTABLE))throw new Error('Set CLIVUU_TEST_NIRI_PREFIX to the private niri runtime for native recording checks');
 const binary=`${GLib.path_get_dirname(GLib.getenv('CLIVUU_TEST_DESKTOP_BINARY'))}/examples/recording_controls_probe`;
 const config=`${directory}/recording-niri.kdl`;GLib.file_set_contents(config,'hotkey-overlay { skip-at-startup; }\n');
 const launcher=new Gio.SubprocessLauncher({flags:0});launcher.set_stderr_file_path(`${directory}/recording-niri.log`);launcher.setenv('LD_LIBRARY_PATH',`${prefix}/usr/lib64`,true);launcher.setenv('XDG_CURRENT_DESKTOP','niri',true);launcher.setenv('GDK_BACKEND','wayland',true);launcher.unsetenv('APPIMAGE');
 const script=`${directory}/recording-native.sh`;GLib.file_set_contents(script,`LD_PRELOAD=libgtk4-layer-shell.so.0 exec ${binary} > '${directory}/recording-native.log' 2>&1\n`);
 const child=launcher.spawnv([`${prefix}/usr/bin/niri`,'-c',config,'--','sh',script]);
 const state=()=>JSON.parse(new TextDecoder().decode(GLib.file_get_contents(`${directory}/native-record-status.json`)[1]));
 const wait=async predicate=>{for(let n=0;n<240;n++){if(await predicate())return;await pause(50);}throw new Error('Native recording fixture timed out');};
 try{
  let window;await wait(()=>window=global.get_window_actors().map(a=>a.meta_window).find(w=>/niri/i.test(w.get_title())));Main.overview.hide();window.maximize();window.activate(global.get_current_time());await pause(600);
  await wait(()=>GLib.file_test(`${directory}/native-record-status.json`,GLib.FileTest.EXISTS));
  check('niri recording controls use native Wayland layers',state().backend==='GdkWaylandDisplay'&&state().controls.visible);
  await wait(()=>state().paint!==null);
  GLib.file_set_contents(`${directory}/recording-paint-bounds.json`,JSON.stringify(state().paint));
  const region=state().controls.rect,ready=state().controls.toolbar,paint=state().paint;
  check('niri recording uses the shared 8px below-selection gap and right alignment',ready.y-region.y-region.height===8&&ready.x+ready.width===region.x+region.width);
  check('recording shadow matches the measured logical GSK envelope',paint.x===ready.x-11&&paint.y===ready.y-8&&paint.width===ready.width+22&&paint.height===ready.height+22);
  const frame=window.get_buffer_rect(),offsetX=Math.max(0,(frame.width-state().width)/2),offsetY=Math.max(0,frame.height-state().height-offsetX);
  const pointer=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
  const click=async(x,y)=>{pointer.notify_absolute_motion(GLib.get_monotonic_time(),frame.x+offsetX+x,frame.y+offsetY+y);await pause(120);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(80);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(180);};
  const maps=state().maps,unmaps=state().unmaps;
  GLib.file_set_contents(`${directory}/native-record-command`,'preparing');await wait(()=>state().controls.phase==='preparing');
  const preparingClicks=state().clicks;await click(region.x+region.width/2,region.y+region.height/2);await wait(()=>state().clicks>preparingClicks);
  check('niri startup keeps native layers mapped and preserves the outside toolbar',state().controls.visible&&state().maps===maps&&state().unmaps===unmaps);
  const restoredMaps=state().maps,restoredUnmaps=state().unmaps;
  GLib.file_set_contents(`${directory}/native-record-command`,'starting');await wait(()=>state().controls.phase==='starting');
  GLib.file_set_contents(`${directory}/native-record-command`,'recording');await wait(()=>state().controls.phase==='recording');
  check('niri Starting Recording transitions never remap the toolbar or frame hosts',state().maps===restoredMaps&&state().unmaps===restoredUnmaps&&state().controls.toolbar.y===ready.y);
  const count=state().clicks;await click(region.x+region.width/2,region.y+region.height/2);await wait(()=>state().clicks>count);
  check('niri recording interior passes input to the real native application',state().clicks>count&&state().controls.visible);
  await observer.captureArea({x:0,y:0,width:1280,height:800},`${directory}/recording-native-wayland.png`);
  GLib.file_set_contents(`${directory}/native-record-command`,'full');await wait(()=>state().controls.phase==='ready'&&state().controls.rect.width===state().width);
  GLib.file_set_contents(`${directory}/native-record-command`,'recording');await wait(()=>state().controls.phase==='recording');
  check('niri hides overlapping full-screen recording controls',!state().controls.visible);
  GLib.file_set_contents(`${directory}/native-record-command`,'paused');await wait(()=>state().controls.phase==='paused');
  check('niri paused full-screen recording controls remain operable',state().controls.visible);
  for(const side of ['left','right']){
   GLib.file_set_contents(`${directory}/native-record-command`,`side-${side}`);
   await wait(()=>state().controls.phase==='ready'&&Math.abs(state().controls.rect.width-state().width/3)<1&&((state().controls.rect.x===0)===(side==='left'))&&state().paint!==null);
   const s=state(),r=s.controls.rect,b=s.controls.toolbar;
   check(`niri lateral toolbar beside the ${side} region is vertically centered with its shadow on screen`,Math.abs(b.y+b.height/2-r.y-r.height/2)<1&&((b.x>r.x)===(side==='left'))&&s.paint.x>=0&&s.paint.y>=0&&s.paint.x+s.paint.width<=s.width&&s.paint.y+s.paint.height<=s.height);
   GLib.file_set_contents(`${directory}/native-record-command`,'recording');await wait(()=>state().controls.phase==='recording');
   check(`niri ${side} region keeps the centered lateral toolbar visible during recording`,state().controls.visible&&state().controls.toolbar.y===b.y);
   await observer.captureArea({x:0,y:0,width:1280,height:800},`${directory}/recording-native-side-${side}.png`);
  }
  GLib.file_set_contents(`${directory}/native-record-command`,'dispose');await wait(()=>!state().controls);
  const previous=state().clicks;await click(400,400);await wait(()=>state().clicks>previous);
  const env={NIRI_SOCKET:state().socket,LD_LIBRARY_PATH:`${prefix}/usr/lib64`};
  const layers=await run([`${prefix}/usr/bin/niri`,'msg','--json','layers'],null,null,env);
  check('niri recording teardown releases layers and input regions',state().clicks>previous&&!layers.includes('clivuu-pins'));
  GLib.file_set_contents(`${directory}/native-record-command`,'playback');await wait(()=>state().playback?.phase==='playback');await pause(250);
  check('niri playback uses native layer surfaces and keeps its timeline visible',state().backend==='GdkWaylandDisplay'&&state().playback.visible&&state().playback.timeline.width>0);
  check('niri playback keeps an ordinary-height action row below compact progress',state().playback.timeline.height<32&&state().playback.toolbar.height-state().playback.timeline.height-1===32);
  GLib.file_set_contents(`${directory}/native-record-command`,'suspend');await wait(()=>!state().playback.visible);
  const hiddenClicks=state().clicks;await click(400,400);await wait(()=>state().clicks>hiddenClicks);
  check('niri playback save suspension releases its layers and input',state().clicks>hiddenClicks&&!state().playback.visible);
  GLib.file_set_contents(`${directory}/native-record-command`,'restore');await wait(()=>state().playback.visible);
  const playback=state().playback,edge=playback.rect;
  pointer.notify_absolute_motion(GLib.get_monotonic_time(),frame.x+offsetX+edge.x+edge.width,frame.y+offsetY+edge.y+edge.height/2);await pause(100);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(80);pointer.notify_absolute_motion(GLib.get_monotonic_time(),frame.x+offsetX+edge.x+edge.width-40,frame.y+offsetY+edge.y+edge.height/2);await pause(100);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await wait(()=>state().playback.edit.crop.width===600);
  check('niri playback border resizing updates the shared native-pixel crop',state().playback.edit.crop.width===600&&state().playback.edit.crop.height===320);
  await observer.captureArea({x:0,y:0,width:1280,height:800},`${directory}/playback-native-wayland.png`);
  GLib.file_set_contents(`${directory}/native-record-command`,'dispose');await wait(()=>!state().playback);
  const after=state().clicks;await click(400,400);await wait(()=>state().clicks>after);
  check('niri playback teardown releases input to the source application',state().clicks>after);
  GLib.file_set_contents(`${directory}/native-record-command`,'exit');await pause(200);
 }finally{child.force_exit();}
}

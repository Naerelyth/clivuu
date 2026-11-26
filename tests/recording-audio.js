// All audio runs through a private PulseAudio-compatible server and null sinks.
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Clutter from 'gi://Clutter';
import GdkPixbuf from 'gi://GdkPixbuf';
import {pause,run} from './common/process.js';
import {recordingPlayback} from './recording-playback.js';

export async function recordingAudio({check,execute,current,action,wait,pointer,directory,observer}) {
 const runtime=GLib.getenv('XDG_RUNTIME_DIR');
 if(runtime!==`${directory}/runtime`)throw new Error('Audio fixture requires a private runtime');
 const server=`unix:${runtime}/pulse/native`,modules=[],tones=[];let pulse;
 const pactl=args=>run(['pactl',`--server=${server}`,...args]);
 const ready=async(name)=>{
  const pending=execute({command:'record',output:`${directory}/${name}`});pending.catch(()=>{});
  await wait(()=>global.get_window_actors().some(a=>a.meta_window.get_title()==='Clivuu capture overlay'));
  pointer.notify_absolute_motion(GLib.get_monotonic_time(),220,180);await pause(60);
  pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(60);
  pointer.notify_absolute_motion(GLib.get_monotonic_time(),860,500);await pause(80);
  pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);
  await wait(async()=>(await current())?.phase==='ready');
  check('record selection release opens recording controls without an extra confirmation',(await current()).visible&&!global.get_window_actors().some(a=>a.mapped&&a.meta_window.get_title()==='Clivuu capture overlay'));
  return {pending};
 };
 const metadata=async(path)=>JSON.parse(await run(['ffprobe','-v','error','-show_entries','stream=codec_type,codec_name,width,height,start_time,duration:format=duration','-of','json',path]));
 const spectrum=async(path)=>{
  const raw=`${path}.f32`;
  await run(['ffmpeg','-nostdin','-y','-v','error','-i',path,'-vn','-ac','1','-ar','48000','-f','f32le',raw]);
  const code=`import array,json,math,sys\na=array.array('f');a.frombytes(open(sys.argv[1],'rb').read());a=a[12000:60000]\ndef amplitude(f):\n n=len(a);c=2*math.cos(2*math.pi*f/48000);u=v=0.0\n for x in a:u,v=x+c*u-v,u\n return 2*math.sqrt(max(0,u*u+v*v-c*u*v))/n\nprint(json.dumps([amplitude(440),amplitude(880)]))`;
  try{return JSON.parse(await run(['python3','-c',code,raw]));}finally{GLib.unlink(raw);}
 };
 try {
  GLib.file_set_contents(`${directory}/fixture-color`,'#2468ac');
  const launcher=new Gio.SubprocessLauncher({flags:0});
  for(const name of ['PIPEWIRE_CONFIG_DIR','PIPEWIRE_CONFIG_NAME','PIPEWIRE_CONFIG_PREFIX'])launcher.unsetenv(name);
  launcher.set_stderr_file_path(`${directory}/pulse.log`);
  pulse=launcher.spawnv(['pipewire-pulse']);
  await wait(async()=>{try{await pactl(['info']);return true;}catch{return false;}});
  const empty=await ready('audio-no-devices.webm'),state=await current();
  GLib.file_set_contents(`${directory}/audio-empty-state.json`,JSON.stringify(state));
  GLib.file_set_contents(`${directory}/audio-empty-sinks.json`,await pactl(['-f','json','list','sinks']));
  GLib.file_set_contents(`${directory}/audio-empty-sources.json`,await pactl(['-f','json','list','sources']));
  check('absent audio devices disable both inputs with specific tooltips',!state.systemAudio&&!state.microphone&&!state.systemAudioAvailable&&!state.microphoneAvailable&&['No system audio device detected','No microphone device detected'].every(label=>state.buttons.some(b=>b.label===label&&!b.enabled)));
  await execute({command:'stop-recording'});try{await empty.pending;}catch{}
  for(const name of ['clivuu_system','clivuu_mic'])modules.push((await pactl(['load-module','module-null-sink',`sink_name=${name}`,'sink_properties=device.class=sound'])).trim());
  modules.push((await pactl(['load-module','module-remap-source','master=clivuu_mic.monitor','source_name=clivuu_mic_input','source_properties=device.class=sound'])).trim());
  await pactl(['set-default-sink','clivuu_system']);await pactl(['set-default-source','clivuu_mic_input']);
  await pactl(['set-sink-volume','clivuu_system','100%']);await pactl(['set-sink-volume','clivuu_mic','100%']);
  await pactl(['set-source-volume','clivuu_mic_input','100%']);
  for(const [device,freq] of [['clivuu_system',440],['clivuu_mic',880]]) {
   const l=new Gio.SubprocessLauncher({flags:0});l.setenv('PULSE_SERVER',server,true);l.set_stderr_file_path(`${directory}/tone-${freq}.log`);
   tones.push(l.spawnv(['gst-launch-1.0','-q','audiotestsrc','is-live=true',`freq=${freq}`,'volume=0.12','!','audio/x-raw,rate=48000,channels=2','!','pulsesink',`device=${device}`]));
  }
  await pause(600);
  const registry=JSON.parse(await run(['pw-dump']));
  check('audio fixtures expose only virtual devices',!registry.some(o=>o.type==='PipeWire:Interface:Device'||['device.api','api','factory.name'].some(k=>/^(alsa|bluez|v4l2|libcamera|api\.(alsa|bluez|v4l2|libcamera))/.test(String(o.info?.props?.[k]||'')))));
  const details=[];
  for(const [name,off,expected] of [['audio-both.webm',null,[true,true]],['audio-system.webm','Record microphone audio',[true,false]],['audio-microphone.mp4','Record system audio',[false,true]],['audio-muted.webm','both',[false,false]]]) {
   const selected=await ready(name),before=await current();
   check('available system and microphone sources start enabled',before.systemAudio&&before.microphone);
   if(off==='both'){await action('Record system audio');await action('Record microphone audio');}
   else if(off)await action(off);
   if(off)await action('Show mouse cursor');
   check('record cursor selection is independent from screenshot settings',(await current()).cursor===!off);
   await action('Start recording');const started=await selected.pending;
   pointer.notify_absolute_motion(GLib.get_monotonic_time(),500,300);
   await pause(900);
   const active=await current();
   check('recording locks audio and cursor choices',active.systemAudio===expected[0]&&active.microphone===expected[1]&&active.buttons.filter(b=>['Stop recording to change audio settings','Stop recording to change cursor capture'].includes(b.label)).every(b=>!b.enabled));
   if(!off){GLib.file_set_contents(`${directory}/audio-source-outputs.json`,await pactl(['-f','json','list','source-outputs']));GLib.file_set_contents(`${directory}/audio-sink-inputs.json`,await pactl(['-f','json','list','sink-inputs']));await execute({command:'pause-recording'});await pause(1200);await execute({command:'pause-recording'});}
   await pause(900);await execute({command:'stop-recording'});
   const meta=await metadata(started.path),audio=meta.streams.filter(s=>s.codec_type==='audio');
   const imagePath=`${started.path}.png`;
   await run(['ffmpeg','-v','error','-y','-ss','0.5','-i',started.path,'-frames:v','1',imagePath]);
   const image=GdkPixbuf.Pixbuf.new_from_file(imagePath),pixels=image.get_pixels();let cursorPixels=0;
   for(let y=116;y<166;y++)for(let x=276;x<326;x++)if([36,104,172].some((v,c)=>Math.abs(pixels[y*image.rowstride+x*image.n_channels+c]-v)>10))cursorPixels++;
   check('encoded pixels follow the cursor toggle',off?cursorPixels===0:cursorPixels>10);
   if(audio.length){const video=meta.streams.find(s=>s.codec_type==='video');check('audio and video retain aligned timestamps',Math.abs(Number(video.start_time)-Number(audio[0].start_time))<0.1&&(!video.duration||!audio[0].duration||Math.abs(Number(video.duration)-Number(audio[0].duration))<0.2));}
   check('audio-enabled formats publish one playable mixed track',audio.length===(expected.some(Boolean)?1:0)&&(!audio.length||audio[0].codec_name===(name.endsWith('.mp4')?'aac':'opus')));
   if(expected.some(Boolean)) {
    const peaks=await spectrum(started.path);details.push({name,peaks,meta});
    check('audio spectrum contains exactly the selected sources',peaks.every((v,n)=>expected[n]?v>0.02:v<0.005));
   }
   if(!off)check('audio/video duration excludes the paused interval',Number(meta.format.duration)<3.1);
  }
  const deviceVolumes=async()=>JSON.stringify(JSON.parse(await pactl(['-f','json','list','sources'])).map(s=>({name:s.name,volume:s.volume,mute:s.mute})));
  const originalVolumes=await deviceVolumes();
  await recordingPlayback({check,execute,current,action,wait,pointer,directory,observer});
  check('playback gain and mute leave all source device volumes unchanged',await deviceVolumes()===originalVolumes);
  const playbackPeaks=await spectrum(`${directory}/playback.mp4`);
  check('late export applies independent gain and mute to recorded tracks',playbackPeaks[0]>.005&&playbackPeaks[1]<.003);
  for(const tone of tones.splice(0))tone.force_exit();await pause(400);
  const quiet=await ready('audio-quiet.webm');await action('Start recording');const quietStarted=await quiet.pending;await pause(1100);await execute({command:'stop-recording'});
  const quietMeta=await metadata(quietStarted.path),quietPeaks=await spectrum(quietStarted.path);
  check('silent but available audio inputs keep recording and finalization live',quietMeta.streams.some(s=>s.codec_type==='audio')&&quietPeaks.every(v=>v<0.001));
  const gif=await ready('audio-format.gif');const stateGif=await current();
  check('export format does not disable capture audio choices',stateGif.systemAudio&&stateGif.microphone&&!stateGif.buttons.some(b=>b.label==='Recording format'));
  await execute({command:'stop-recording'});try{await gif.pending;}catch{}
  GLib.file_set_contents(`${directory}/audio-results.json`,JSON.stringify(details));
 } finally {
  try{await execute({command:'stop-recording'});}catch{}
  for(const tone of tones)tone.force_exit();
  for(const module of modules.reverse())try{await pactl(['unload-module',module]);}catch{}
  pulse?.force_exit();await pause(900);
 }
}

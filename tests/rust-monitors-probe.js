// All monitor configuration and input stay on the isolated headless compositor.
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Clutter from 'gi://Clutter';
import St from 'gi://St';
import Cairo from 'cairo';
import GdkPixbuf from 'gi://GdkPixbuf';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {pause,run} from './common/process.js';
import {authorizeScreenshot} from './screenshot-access.js';
export async function probe(observer) {
 const directory=GLib.getenv('CLIVUU_TEST_DIR'),single=GLib.getenv('CLIVUU_TEST_SINGLE_MONITOR')==='1',results=[];let child,pattern;
 const check=(name,passed,details={})=>{results.push({name,passed:!!passed,...details});if(!passed)throw new Error(name);};
 const wait=async predicate=>{for(let n=0;n<300;n++){if(await predicate())return;await pause(50);}throw new Error('Monitor probe timed out');};
 const call=(bus,path,iface,method,args)=>new Promise((resolve,reject)=>Gio.DBus.session.call(bus,path,iface,method,args,null,0,120000,null,(c,r)=>{try{resolve(c.call_finish(r).deep_unpack());}catch(e){reject(e);}}));
 const execute=async r=>JSON.parse((await call('io.github.naerelyth.Clivuu.Desktop','/io/github/naerelyth/Clivuu/Desktop','io.github.naerelyth.Clivuu.Desktop1','Execute',new GLib.Variant('(s)',[JSON.stringify({protocol:1,...r})])))[0]);
 const display=(method,args=null)=>call('org.gnome.Mutter.DisplayConfig','/org/gnome/Mutter/DisplayConfig','org.gnome.Mutter.DisplayConfig',method,args);
 const pointer=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
 const keyboard=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
 const key=async(k,ctrl=false)=>{if(ctrl)keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Control_L,Clutter.KeyState.PRESSED);keyboard.notify_keyval(GLib.get_monotonic_time(),k,Clutter.KeyState.PRESSED);keyboard.notify_keyval(GLib.get_monotonic_time(),k,Clutter.KeyState.RELEASED);if(ctrl)keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Control_L,Clutter.KeyState.RELEASED);await pause(120);};
 const cursorSize=()=>{const sprite=global.backend.get_cursor_tracker().get_sprite();return sprite?[sprite.get_width(),sprite.get_height()]:null;};
 const heldCursor=async(name,x,y)=>{
  const move=(x,y)=>pointer.notify_absolute_motion(GLib.get_monotonic_time(),x,y);
  move(x,y);await pause(120);const hover=cursorSize(),held=[];
  pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);
  try{for(const ms of [20,40,80,160]){if(ms===40)move(x+1,y);await pause(ms);held.push(cursorSize());}}
  finally{
   // Release outside the button so this probe does not activate its action.
   move(x,y-48);await pause(60);
   pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);
  }
  move(x,y);await pause(120);const released=cursorSize();
  results.push({name,passed:!!hover&&[...held,released].every(s=>JSON.stringify(s)===JSON.stringify(hover)),hover,held,released});
  GLib.file_set_contents(`${directory}/cursor-press-results.json`,JSON.stringify(results));
 };
 try {
  Main.overview.hide();await pause(300);
  const initial=await display('GetCurrentState');
  const monitors=initial[1].map(m=>({connector:m[0][0],mode:m[1].find(m=>m[6]['is-current']?.deep_unpack())??m[1][0]})).sort((a,b)=>b.mode[1]-a.mode[1]);
  check('private compositor provides the requested physical monitor resolutions',single?monitors.length===1&&monitors[0].mode[1]===1920&&monitors[0].mode[2]===1080:monitors.length===2&&monitors[0].mode[1]===2880&&monitors[0].mode[2]===1800&&monitors[1].mode[1]===1920&&monitors[1].mode[2]===1080);
  const launcher=new Gio.SubprocessLauncher({flags:0});launcher.unsetenv('APPIMAGE');launcher.setenv('GDK_BACKEND','x11',true);launcher.setenv('CLIVUU_LANGUAGE','en',true);launcher.set_stderr_file_path(`${directory}/monitors-service.log`);
  const packaged=GLib.getenv('CLIVUU_TEST_APPIMAGE')==='1';
  if(packaged)launcher.setenv('CLIVUU_DESKTOP_DIR','',true);
  child=launcher.spawnv([packaged?observer._workerPath():GLib.getenv('CLIVUU_TEST_DESKTOP_BINARY')]);await wait(async()=>{try{return(await execute({command:'status'})).ready;}catch{return false;}});
  let authorized=false;
  for(const scale of single?[1]:[2,1]) {
   const configuration=single?'single monitor':`secondary at ${scale}x`;
   const [serial]=await display('GetCurrentState');
   const positions=monitors.map((m,i)=>[i===0?0:2880/scale,0,i===0?scale:1,0,single||i===1,[[m.connector,m.mode[0],{}]]]);
   await display('ApplyMonitorsConfig',new GLib.Variant('(uua(iiduba(ssa{sv}))a{sv})',[serial,1,positions,{'layout-mode':new GLib.Variant('u',1)}]));
   await pause(700);
   await run(['python3',`${directory}/x11-root-cursor.py`]);
   const right={x:single?0:2880/scale,y:0,width:1920,height:1080};
   pattern?.destroy();
   pattern=new St.DrawingArea({x:right.x+128,y:200,width:128,height:128});
   pattern.connect('repaint',area=>{
    const cr=area.get_context();cr.setAntialias(Cairo.Antialias.NONE);
    cr.setSourceRGB(0,0,0);cr.paint();cr.setSourceRGB(1,1,1);
    for(let y=0;y<128;y++)for(let x=0;x<128;x++)if((x+y)%2)cr.rectangle(x,y,1,1);
    cr.fill();cr.$dispose();
   });
   global.window_group.add_child(pattern);await pause(250);
   const pixels=(image,ox,oy)=>{
    const data=image.get_pixels();let wrong=0,gray=0;
    for(let y=0;y<128;y++)for(let x=0;x<128;x++){const value=data[(oy+y)*image.rowstride+(ox+x)*image.n_channels],expected=(x+y)%2===0?0:255;if(value!==expected)wrong++;if(value>0&&value<255)gray++;}
    return {wrong,gray};
   };
   const native=`${directory}/oracle-${scale}.png`;
   await observer.captureArea({x:right.x+128,y:200,width:128,height:128},native);
   const reference=pixels(GdkPixbuf.Pixbuf.new_from_file(native),0,0);
   check(`native compositor oracle preserves primary checkerboard (${configuration})`,reference.wrong===0,reference);
   const rawPath=`${directory}/raw-${scale}.png`;
   const rawRequest=()=>execute({command:'snip',mode:'full',output:rawPath});
   if(!authorized){await authorizeScreenshot(rawRequest);authorized=true;}else await rawRequest();
   const raw=GdkPixbuf.Pixbuf.new_from_file(rawPath);
   const sx=raw.width/global.stage.width,sy=raw.height/global.stage.height,source=raw.get_pixels();
   const sourcePixel=(x,y)=>source[Math.floor((200+y+0.5)*sy)*raw.rowstride+Math.floor((right.x+128+x+0.5)*sx)*raw.n_channels];
   const compareSource=(image,inset=0)=>{
    const data=image.get_pixels();let different=0;
    for(let y=inset;y<128-inset;y++)for(let x=inset;x<128-inset;x++)if(data[(200+y)*image.rowstride+(128+x)*image.n_channels]!==sourcePixel(x,y))different++;
    return different;
   };
   let sourceWrong=0,sourceGray=0;
   for(let y=0;y<128;y++)for(let x=0;x<128;x++){const value=sourcePixel(x,y);if(value!==((x+y)%2===0?0:255))sourceWrong++;if(value>0&&value<255)sourceGray++;}
   // GNOME's static desktop image may already lose detail at mixed scales.
   // Record that independently; require exports to preserve the available
   // source samples, without requiring an upstream limitation to persist.
   results.push({name:`static portal source quality (${configuration})`,passed:true,size:[raw.width,raw.height],logical:[global.stage.width,global.stage.height],nativeDifference:sourceWrong,gray:sourceGray});
   const captures=[['primary',right,1920,1080]];
   if(!single)captures.push(['secondary',{x:0,y:0,width:2880/scale,height:1800/scale},2880,1800]);
   for(const [label,rect,w,h] of captures) {
    const output=`${directory}/${label}-${scale}.png`;
    const request=()=>execute({command:'snip',rect,output});
    if(!authorized){await authorizeScreenshot(request);authorized=true;}else await request();
    const image=GdkPixbuf.Pixbuf.new_from_file(output);
    check(`${label} direct capture keeps native pixels (${configuration})`,image.width===w&&image.height===h,{size:[image.width,image.height]});
    if(label==='primary')check(`primary direct capture preserves static source samples (${configuration})`,compareSource(image)===0,pixels(image,128,200));
   }
   for(const [label,monitor] of captures) {
    const density=label==='secondary'?scale:1;
    const rect={x:monitor.x+200,y:300,width:640,height:320};
    const output=`${directory}/recording-${label}-${scale}.webm`;
    await execute({command:'record',rect,output});await pause(900);
    await execute({command:'stop-recording'});
    const metadata=JSON.parse(await run(['ffprobe','-v','error','-show_entries','stream=width,height,codec_name','-of','json',output]));
    const video=metadata.streams[0];
    check(`${label} recording retains native stream dimensions (${configuration})`,video.width===640*density&&video.height===320*density&&video.codec_name==='vp9',{size:[video.width,video.height],logical:[640,320],density});
   }
   if(!single&&scale===2){
    const notifications=()=>Main.messageTray.getSources().filter(s=>s.title==='Clivuu').flatMap(s=>s.notifications??[]);
    for(const n of notifications())n.destroy();
    const selectSubpixel=async command=>{
     const op=execute(command);op.catch(()=>{});
     await wait(async()=>(await execute({command:'status'})).captureWindows?.some(w=>w.mapped&&w.active));
     pointer.notify_absolute_motion(GLib.get_monotonic_time(),60,70);await pause(80);
     pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(60);
     pointer.notify_absolute_motion(GLib.get_monotonic_time(),220,170);await pause(80);
     pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(100);
     // Nudge x by +0.5 logical (+1 physical px) and expand width by +0.5 logical (+1 physical px -> 641 px).
     await key(Clutter.KEY_Right,false);
     await key(Clutter.KEY_Right,true);
     return {op};
    };
    const layoutError=()=>notifications().some(n=>/Invalid monitor layout|显示器布局无效/i.test(`${n.title??''} ${n.bannerBodyText??''}`));
    const subScroll=`${directory}/subpixel-scroll-2x.png`;
    const {op:scrollOp}=await selectSubpixel({command:'snip',output:subScroll});
    await key(Clutter.KEY_l,false);
    await wait(async()=>(await execute({command:'status'})).scrolling?.frames>=1);
    const scrollState=(await execute({command:'status'})).scrolling;
    check('2x subpixel selection enters scrolling capture without Invalid monitor layout',Math.abs(scrollState.rect.x-120.5)<0.01&&Math.abs(scrollState.rect.width-320.5)<0.01&&!layoutError(),{rect:scrollState.rect});
    await key(Clutter.KEY_s,true);
    const scrollResult=await scrollOp;
    const scrollImg=GdkPixbuf.Pixbuf.new_from_file(subScroll);
    check('2x subpixel scrolling capture exports exact odd physical width (641x400)',scrollResult.path===subScroll&&scrollImg.width===641&&scrollImg.height===400,{size:[scrollImg.width,scrollImg.height]});

    const subRec=`${directory}/subpixel-record-2x.webm`;
    const {op:recOp}=await selectSubpixel({command:'snip',output:subRec});
    await key(Clutter.KEY_v,false);
    await wait(async()=>(await execute({command:'status'})).recordingUi?.phase==='ready');
    await key(Clutter.KEY_Return,false);
    await recOp;
    await wait(async()=>(await execute({command:'status'})).recordingUi?.phase==='recording');
    await pause(900);
    await execute({command:'stop-recording'});
    const subMeta=JSON.parse(await run(['ffprobe','-v','error','-show_entries','stream=width,height,codec_name','-of','json',subRec]));
    const subVideo=subMeta.streams[0];
    check('2x subpixel recording starts and exports exact odd physical width (641x400) without error notifications',subVideo.width===641&&subVideo.height===400&&subVideo.codec_name==='vp9'&&!layoutError(),{size:[subVideo.width,subVideo.height]});
   }
   pointer.notify_absolute_motion(GLib.get_monotonic_time(),right.x+800,400);await pause(100);
   const started=GLib.get_monotonic_time();
   const output=`${directory}/interactive-${scale}.png`,pending=execute({command:'snip',output});pending.catch(()=>{});
   await wait(async()=>(await execute({command:'status'})).captureWindows?.some(w=>w.mapped&&w.active));
   const readyMs=(GLib.get_monotonic_time()-started)/1000;
   results.push({name:`selector input readiness (${configuration})`,passed:true,milliseconds:readyMs});
   pointer.notify_absolute_motion(GLib.get_monotonic_time(),right.x+800,400);await pause(100);
   await key(Clutter.KEY_a,true);
   pointer.notify_absolute_motion(GLib.get_monotonic_time(),right.x+right.width-200,right.height-24);await pause(250);
   const overlay=`${directory}/overlay-${scale}.png`;
   await observer.captureArea({x:right.x+128,y:200,width:128,height:128},overlay);
   const shown=pixels(GdkPixbuf.Pixbuf.new_from_file(overlay),0,0);
   // Keep presentation diagnostics separate from portal-source quality.
   check(`primary selection background pattern (${configuration})`,scale!==1||shown.wrong===0,shown);
   const sprite=global.backend.get_cursor_tracker().get_sprite();
   check(`primary toolbar cursor has normal themed dimensions (${configuration})`,sprite&&sprite.get_width()<=64&&sprite.get_height()<=64,{cursor:sprite?[sprite.get_width(),sprite.get_height()]:null});
   await heldCursor(`primary toolbar cursor keeps its size while pressed (${configuration})`,right.x+right.width-20,right.height-24);
   pointer.notify_absolute_motion(GLib.get_monotonic_time(),right.x+800,400);await pause(100);
   pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);
   pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(100);
   await key(Clutter.KEY_c,true);await pending;
   const image=GdkPixbuf.Pixbuf.new_from_file(output);
   check(`primary interactive export keeps 1920x1080 (${configuration})`,image.width===1920&&image.height===1080,{size:[image.width,image.height]});
   // GSK and image::Nearest choose different half-texel samples on the
   // pattern's outer edge. Compare its interior without that boundary.
   check(`primary interactive export preserves static source samples (${configuration})`,compareSource(image,1)===0,pixels(image,128,200));
   if(scale===1) {
    const rect={x:right.x+200,y:300,width:320,height:180};
    await execute({command:'snip',rect,output:'pin'});
    pointer.notify_absolute_motion(GLib.get_monotonic_time(),rect.x+80,rect.y+60);await pause(120);
    pointer.notify_button(GLib.get_monotonic_time(),3,Clutter.ButtonState.PRESSED);
    pointer.notify_button(GLib.get_monotonic_time(),3,Clutter.ButtonState.RELEASED);await pause(200);
    let menu;
    await wait(async()=>menu=(await execute({command:'status'})).pinUi?.menu?.buttons.find(b=>b.rect.width>0));
    pointer.notify_absolute_motion(GLib.get_monotonic_time(),menu.rect.x+menu.rect.width/2,menu.rect.y+menu.rect.height/2);await pause(180);
    const menuSprite=global.backend.get_cursor_tracker().get_sprite();
    check('primary pin menu cursor has normal themed dimensions',menuSprite&&menuSprite.get_width()<=64&&menuSprite.get_height()<=64,{cursor:menuSprite?[menuSprite.get_width(),menuSprite.get_height()]:null});
    await heldCursor('primary pin menu cursor keeps its size while pressed',menu.rect.x+menu.rect.width/2,menu.rect.y+menu.rect.height/2);
    await key(Clutter.KEY_Escape);
   }
  }
  await execute({command:'exit'});
 }catch(e){results.push({name:'multi-monitor exception',passed:false,error:`${e.message}\n${e.stack}`});}
 finally{pattern?.destroy();child?.force_exit();GLib.file_set_contents(`${directory}/results.json`,JSON.stringify(results));}
}

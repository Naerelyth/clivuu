import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Clutter from 'gi://Clutter';
import Meta from 'gi://Meta';
import GdkPixbuf from 'gi://GdkPixbuf';
import Shell from 'gi://Shell';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {pause,run} from './common/process.js';
import {verifyAutomatic} from './scrolling-auto.js';
import {saveChooser} from './save-chooser.js';
export async function probe(observer){
 const directory=GLib.getenv('CLIVUU_TEST_DIR'),results=[];let child,fixture,signal,handles=0;
 const check=(name,passed)=>{results.push({name,passed:!!passed});GLib.file_set_contents(`${directory}/scrolling-progress.json`,JSON.stringify(results));if(!passed)throw new Error(name);};
 const wait=async predicate=>{for(let n=0;n<200;n++){if(await predicate())return;await pause(50);}throw new Error('Native scrolling timed out');};
 const BUS='io.github.naerelyth.Clivuu.Desktop',PATH='/io/github/naerelyth/Clivuu/Desktop',IFACE=`${BUS}1`;
 const execute=request=>new Promise((resolve,reject)=>Gio.DBus.session.call(BUS,PATH,IFACE,'Execute',new GLib.Variant('(s)',[JSON.stringify({protocol:1,...request})]),null,Gio.DBusCallFlags.NO_AUTO_START,120000,null,(c,r)=>{try{resolve(JSON.parse(c.call_finish(r).deep_unpack()[0]));}catch(e){reject(e);}}));
 const pointer=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
 const keyboard=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
 const move=(x,y)=>pointer.notify_absolute_motion(GLib.get_monotonic_time(),x,y);
 const click=async rect=>{move(rect.x+rect.width/2,rect.y+rect.height/2);await pause(120);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(60);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(100);};
 const action=async label=>{let state;await wait(async()=>{state=(await execute({command:'status'})).scrolling;return state?.controlsVisible&&state.buttons.some(b=>b.label===label&&b.rect.width>0);});await click(state.buttons.find(b=>b.label===label).rect);};
  const key=async k=>{keyboard.notify_keyval(GLib.get_monotonic_time(),k,Clutter.KeyState.PRESSED);keyboard.notify_keyval(GLib.get_monotonic_time(),k,Clutter.KeyState.RELEASED);await pause(100);};
  const current=async()=>(await execute({command:'status'})).scrolling;
 try{
  signal=global.backend.get_remote_access_controller().connect('new-handle',()=>handles++);
  fixture=Gio.Subprocess.new(['gjs','-m',`${directory}/fixture.js`],Gio.SubprocessFlags.NONE);await pause(1600);Main.overview.hide();await pause(400);
  await run([observer._workerPath(),'config','set','language','en']);
  const launcher=new Gio.SubprocessLauncher({flags:0});launcher.unsetenv('APPIMAGE');launcher.setenv('GDK_BACKEND','x11',true);launcher.set_stderr_file_path(`${directory}/scrolling.log`);const packaged=GLib.getenv('CLIVUU_TEST_APPIMAGE')==='1';if(packaged){launcher.setenv('CLIVUU_DESKTOP_DIR','',true);}child=launcher.spawnv(packaged?[observer._workerPath()]:[GLib.getenv('CLIVUU_TEST_DESKTOP_BINARY')||'/tmp/clivuu-target/release/clivuu-desktop']);
  await wait(async()=>{try{return(await execute({command:'status'})).ready;}catch{return false;}});
  if(GLib.getenv('CLIVUU_TEST_SCROLL_AUTO_ONLY')==='1'){await verifyAutomatic({execute,current,action,move,key,click,wait,check});await execute({command:'exit'});return;}
  const early=execute({command:'scroll',rect:{x:70,y:150,width:850,height:470}});early.catch(()=>{});
  await wait(()=>global.display.focus_window?.get_title()==='Clivuu scrolling capture');
  const cancelStarted=GLib.get_monotonic_time();await key(Clutter.KEY_Escape);
  const earlyResult=await early;
  check('Escape during startup cancels without waiting for first-frame acquisition',earlyResult.cancelled&&!(await execute({command:'status'})).busy&&(GLib.get_monotonic_time()-cancelStarted)/1e6<2);
  const output=`${directory}/native-long.png`,operation=execute({command:'scroll',rect:{x:70,y:150,width:850,height:470},output});let failure;operation.catch(e=>failure=e);
  await wait(async()=>{if(failure)throw failure;return (await execute({command:'status'})).scrolling?.frames===1;});
  const visible = async name => {
   move(500,400);
   // after-paint is submission, not compositor presentation. Establish the
   // first actually visible toolbar before checking subsequent sample cycles.
   await wait(async()=>{
    const s=await current(),path=`${directory}/first-presented-toolbar.png`;
    await observer.captureArea({x:Math.round(s.toolbar.x+s.toolbar.width/2),y:Math.round(s.toolbar.y+29),width:4,height:1},path);
    const rgb=Array.from(GdkPixbuf.Pixbuf.new_from_file(path).get_pixels()).slice(0,3).join(',');
    return ['230,230,230','40,42,54'].includes(rgb);
   });
   let stable=true,paint=null;const started=handles;
   for(let n=0;n<90;n++){const s=await current();stable&&=!!s&&!s.sampling&&s.toolbarOpacity===1&&s.previewOpacity===1&&s.mapped;if(n%15===0){
    const path=`${directory}/visibility-${name.split(' ')[0]}-${n}.png`;
    await observer.captureArea({x:Math.round(s.toolbar.x+s.toolbar.width/2),y:Math.round(s.toolbar.y+29),width:4,height:1},path);
    const pixels=Array.from(GdkPixbuf.Pixbuf.new_from_file(path).get_pixels()).join(',');
    if(paint===null)paint=pixels;else stable&&=paint===pixels;
   }
   await pause(20);}
   check(name,stable);
   check(`${name}: sampling reuses one continuous stream`,started>0&&handles===started);
  };
  await visible('manual long capture keeps its toolbar, frame and preview continuously visible');
  const state=(await execute({command:'status'})).scrolling;
  check('scrolling keeps the original live selection and places a 32px toolbar below it',state.rect.x===70&&state.rect.y===150&&state.toolbar.height===32&&state.toolbar.y===628&&state.toolbar.x+state.toolbar.width===920);
  check('scrolling uses special desktop surfaces without a separate panel window',global.get_window_actors().filter(a=>a.meta_window.get_title()==='Clivuu scrolling capture').every(a=>a.meta_window.get_window_type()===Meta.WindowType.OVERRIDE_OTHER));
  const drag=async(label,dx,dy,held)=>{
   const s=await current(),b=s.buttons.find(b=>b.label===label).rect,x=b.x+b.width/2,y=b.y+b.height/2;
   move(x,y);await pause(120);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(80);
   for(let n=1;n<=6;n++){move(x+dx*n/6,y+dy*n/6);await pause(35);}
   await pause(300);if(held)await held(await current());
   pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(250);
  };
  const verifyShadow=async()=>{
  const shadowPath=`${directory}/toolbar-shadow.png`,shadowOperation=execute({command:'scroll',rect:{x:70,y:150,width:850,height:470},output:shadowPath});
  await wait(async()=> (await current())?.frames>=1);
  check('toolbar exposes the shared movement grip',(await current()).buttons.some(b=>b.label==='Move toolbar'));
  const streamCount=handles;await drag('Move toolbar',-40,-120);move(500,400);await pause(900);check('toolbar movement rebuilds its reader within the same authorized session',handles===streamCount);
  await action('Capture direction');await wait(async()=> (await current()).axis==='horizontal');
  move(500,400);await wait(async()=> (await current()).frames>=1);await action('Save image');await wait(async()=>!(await execute({command:'status'})).busy);await shadowOperation;
  const shadowImage=GdkPixbuf.Pixbuf.new_from_file(shadowPath),shadowPixels=shadowImage.get_pixels();let unexpected=0;
  for(let y=0;y<shadowImage.height;y++)for(let x=0;x<shadowImage.width;x++){const n=y*shadowImage.rowstride+x*shadowImage.n_channels;if(Math.abs(shadowPixels[n]-36)>1||Math.abs(shadowPixels[n+1]-104)>1||Math.abs(shadowPixels[n+2]-172)>1)unexpected++;}
  check('capture excludes toolbar shadows that extend beyond the input rectangle',unexpected===0);
  };
  const verifyFullscreen=async()=>{
  GLib.file_set_contents(`${directory}/fixture-fullscreen`,'1');
  await wait(()=>global.get_window_actors().some(a=>a.meta_window.get_title()==='Clivuu focus fixture'&&a.meta_window.is_fullscreen()));await pause(400);
  const fullPath=`${directory}/fullscreen-export.png`,fullOperation=execute({command:'scroll',rect:{x:0,y:0,width:1280,height:800},output:fullPath});
  await wait(async()=> (await current())?.frames>=1);await visible('full-screen sampling never fades its controls');
  check('full-screen long capture has distinct toolbar and region movement controls',(await current()).buttons.some(b=>b.label==='Move capture region')&&(await current()).buttons.some(b=>b.label==='Move toolbar'));
  await action('Capture direction');await wait(async()=> (await current()).axis==='horizontal');
  move(500,400);await wait(async()=> (await current()).frames>=1);await action('Save image');await wait(async()=>!(await execute({command:'status'})).busy);await fullOperation;
  const full=GdkPixbuf.Pixbuf.new_from_file(fullPath),fp=full.get_pixels();let unexpected=0;
  // GNOME itself displays a sharing indicator/panel for continuous capture.
  // Check the stable fixture body separately from legitimate system feedback
  // at the screen edges. The fixed toolbar uses normal inside placement.
  for(let y=80;y<710;y++)for(let x=10;x<1270;x++){const a=y*full.rowstride+x*full.n_channels;if(Math.abs(fp[a]-36)>1||Math.abs(fp[a+1]-104)>1||Math.abs(fp[a+2]-172)>1)unexpected++;}
  check('full-screen capture exports source pixels without resizing',full.width===1280&&full.height===800&&unexpected===0);
  GLib.unlink(`${directory}/fixture-fullscreen`);await wait(()=>global.get_window_actors().some(a=>a.meta_window.get_title()==='Clivuu focus fixture'&&!a.meta_window.is_fullscreen()));await pause(250);
  };
  if(GLib.getenv('CLIVUU_TEST_SCROLL_FULLSCREEN_ONLY')==='1'){
   await action('Cancel');await operation;GLib.file_set_contents(`${directory}/fixture-color`,'#2468ac');await pause(400);
   await verifyFullscreen();await execute({command:'exit'});return;
  }
  if(GLib.getenv('CLIVUU_TEST_SCROLL_SHADOW_ONLY')==='1'){
   await action('Cancel');await operation;GLib.file_set_contents(`${directory}/fixture-color`,'#2468ac');await pause(400);
   await verifyShadow();await execute({command:'exit'});return;
  }
  if(GLib.getenv('CLIVUU_TEST_SCROLL_DEEP')==='1'){
   await action('Cancel');await operation;
   GLib.file_set_contents(`${directory}/fixture-header`,'1');await pause(500);
   const fixturePosition=()=>JSON.parse(new TextDecoder().decode(GLib.file_get_contents(`${directory}/fixture-scroll.json`)[1])).value;
   for(const reverse of [false,true])for(const label of ['Pin image','Copy image','Save image']){
    GLib.file_set_contents(`${directory}/fixture-scroll-request`,reverse?'bottom':'0');await pause(1000);
    const start=fixturePosition(),pending=execute({command:'scroll',rect:{x:0,y:90,width:1200,height:470}});pending.catch(()=>{});
    await wait(async()=> {const s=await current();return s?.active&&s.frames===1;});move(500,450);
    const stable=await current(),states=[];
    if(GLib.getenv('CLIVUU_TEST_SCROLL_DUMP')==='1')await observer.captureArea({x:0,y:90,width:1200,height:470},`${directory}/source-00.png`);
    for(let n=0;n<48;n++){
     pointer.notify_discrete_scroll(GLib.get_monotonic_time(),reverse?Clutter.ScrollDirection.UP:Clutter.ScrollDirection.DOWN,Clutter.ScrollSource.WHEEL);await pause(380);
     if(GLib.getenv('CLIVUU_TEST_SCROLL_DUMP')==='1')await observer.captureArea({x:0,y:90,width:1200,height:470},`${directory}/source-${String(n+1).padStart(2,'0')}.png`);
     if(n%8===7){
      await pause(300);const sample=`${directory}/body-${reverse}-${label.split(' ')[0]}-${n}.png`;
      await observer.captureArea({x:0,y:90,width:1200,height:470},sample);
      states.push({source:fixturePosition(),capture:await current(),sample});
     }
    }
    await pause(1200);const end=fixturePosition(),last=await current(),expected=470+Math.round(Math.abs(end-start));
    const prefix=`${directory}/deep-${reverse?'up':'down'}-${label.split(' ')[0]}`;
    GLib.file_set_contents(`${prefix}-state.json`,JSON.stringify({start,end,expected,last,states}));
    check(`deep ${reverse}/${label} tracks the actual GTK scroll distance`,expected>2350&&Math.abs(last.height-expected)<=2&&!last.message);
    check(`deep ${reverse}/${label} keeps toolbar and preview allocations stable`,states.every(s=>JSON.stringify(s.capture.toolbar)===JSON.stringify(stable.toolbar)&&JSON.stringify(s.capture.preview)===JSON.stringify(stable.preview)));
    await action(label);if(label==='Save image')await saveChooser(`${prefix}.png`);
    await wait(async()=>!(await execute({command:'status'})).busy);await pending;
    if(label==='Pin image'){
     const pins=(await execute({command:'status'})).images;check(`deep ${reverse} actually creates a full-sized pin`,pins.length===1&&pins[0].rect.height===expected);
     const store=Gio.File.new_for_path(`${GLib.getenv('XDG_DATA_HOME')}/clivuu/pins`);let asset;
     await wait(()=>{try{
      const manifest=JSON.parse(new TextDecoder().decode(GLib.file_get_contents(`${store.get_path()}/pins.json`)[1]));
      const pin=manifest.pins.find(p=>!p.closed&&p.image);if(!pin)return false;
      asset=store.get_child(pin.image);return asset.query_exists(null);
     }catch{return false;}});
     asset.copy(Gio.File.new_for_path(`${prefix}.png`),Gio.FileCopyFlags.OVERWRITE,null,null);
    }else if(label==='Copy image')await run(['gjs','-m',`${directory}/clipboard-read.js`,`${prefix}.png`],null,null,{GDK_BACKEND:'x11'});
    const image=GdkPixbuf.Pixbuf.new_from_file(`${prefix}.png`),bytes=image.get_pixels();let repeats=0;
    for(let y=300;y<image.height;y++){const i=y*image.rowstride+10*image.n_channels;if(bytes[i]===81&&bytes[i+1]===43&&bytes[i+2]===115)repeats++;}
    check(`deep ${reverse}/${label} exports one title and every scroll pixel`,repeats===0&&Math.abs(image.height-expected)<=2);
    let wrong=0,compared=0;
    for(const sample of states){
     const source=GdkPixbuf.Pixbuf.new_from_file(sample.sample),pixels=source.get_pixels();
     const offset=Math.round(sample.source)-Math.round(reverse?end:start);
     for(let y=300;y<440;y++)for(let x=20;x<1180;x++){
      const a=y*source.rowstride+x*source.n_channels,b=(offset+y)*image.rowstride+x*image.n_channels;
      if([0,1,2].some(c=>Math.abs(pixels[a+c]-bytes[b+c])>2))wrong++;
      compared++;
     }
    }
    GLib.file_set_contents(`${prefix}-pixels.json`,JSON.stringify({wrong,compared}));
    check(`deep ${reverse}/${label} retains every sampled body span`,wrong/compared<0.003);
    if(label==='Pin image')await execute({command:'empty-group'});
   }
   await execute({command:'exit'});return;
  }
  if(GLib.getenv('CLIVUU_TEST_SCROLL_HEADER_ONLY')==='1'){
   await action('Cancel');await operation;
   GLib.file_set_contents(`${directory}/fixture-header`,'1');await pause(500);
   const path=`${directory}/fixed-title.png`,pending=execute({command:'scroll',rect:{x:70,y:90,width:850,height:470},output:path});pending.catch(()=>{});
   await wait(async()=> {const s=await current();return s?.active&&s.frames===1;});move(500,450);
   for(let n=0;n<12;n++){pointer.notify_discrete_scroll(GLib.get_monotonic_time(),Clutter.ScrollDirection.DOWN,Clutter.ScrollSource.WHEEL);await pause(380);}
   await wait(async()=> (await current()).frames>=4);
   await pause(1200);const beforeMove=await current();
   GLib.file_set_contents(`${directory}/fixed-title-before.json`,JSON.stringify(beforeMove));
   await drag('Move capture region',0,16);move(500,450);await pause(1200);
   await drag('Move capture region',0,-16);move(500,450);await pause(1200);
   const afterMove=await current();GLib.file_set_contents(`${directory}/fixed-title-after.json`,JSON.stringify(afterMove));check('fixed-title capture keeps its scroll position after region movement',Math.abs(beforeMove.position*beforeMove.height-afterMove.position*afterMove.height)<1&&afterMove.height===beforeMove.height+16);
   GLib.file_set_contents(`${directory}/fixed-title-status.json`,JSON.stringify(afterMove));
   await action('Save image');await wait(async()=>!(await execute({command:'status'})).busy);await pending;
   const im=GdkPixbuf.Pixbuf.new_from_file(path),bytes=im.get_pixels();let repeated=0,first=0;
   for(let y=0;y<im.height;y++){const i=y*im.rowstride+10*im.n_channels;
    if(bytes[i]===81&&bytes[i+1]===43&&bytes[i+2]===115){if(y>=300)repeated++;else first++;}
   }
   check('a tall real GTK title is retained once across scrolling frames',first>150&&repeated===0&&im.height>700);
   await execute({command:'exit'});return;
  }
  if(GLib.getenv('CLIVUU_TEST_SCROLL_ROUNDTRIP_ONLY')==='1'){
   await action('Cancel');await operation;
   for(const dy of [28,-28])for(const label of ['Pin image','Copy image','Save image']){
    const pending=execute({command:'scroll',rect:{x:12,y:60,width:1256,height:650}});pending.catch(()=>{});
    await wait(async()=> (await current())?.frames>=1);
    await drag('Move capture region',0,dy);move(500,400);await pause(1700);
    GLib.file_set_contents(`${directory}/roundtrip-${dy}-out.json`,JSON.stringify(await current()));
    await drag('Move capture region',0,-dy);move(500,400);await pause(1700);
    const back=await current();GLib.file_set_contents(`${directory}/roundtrip-${dy}-back.json`,JSON.stringify(back));
    check(`${label}/${dy}: moving back restores the stitch position`,Math.abs(back.position-(dy<0?28/678:0))<0.00001&&back.height===678);
    await action(label);
    const output=`${directory}/roundtrip-${dy}-${label.split(' ')[0]}.png`;
    if(label==='Save image')await saveChooser(output);
    await wait(async()=>!(await execute({command:'status'})).busy);
    const result=await pending;check(`region roundtrip/${dy} remains exportable through ${label}`,!result.cancelled);
    if(label==='Pin image'){
     const pins=(await execute({command:'status'})).images;check(`roundtrip/${dy} actually creates a pin`,pins.length===1&&pins[0].rect.height===678);
     await execute({command:'empty-group'});
    }else{
     if(label==='Copy image')await run(['gjs','-m',`${directory}/clipboard-read.js`,output],null,null,{GDK_BACKEND:'x11'});
     const im=GdkPixbuf.Pixbuf.new_from_file(output);check(`roundtrip/${dy} ${label} exports exact dimensions`,im.width===1256&&im.height===678);
    }
   }
   await execute({command:'exit'});return;
  }
  const initialToolbar=state.toolbar,initialFrames=state.frames;
  check('long-capture has a shared toolbar grip and no pause button',state.buttons.some(b=>b.label==='Move toolbar')&&!state.buttons.some(b=>/Pause|Resume/.test(b.label)));
  const streamsBefore=handles;
  await drag('Move toolbar',-70,-80,s=>check('toolbar drag moves controls without changing the source region or appending frames',s.interacting&&s.rect.x===70&&s.rect.y===150&&s.frames===initialFrames&&Math.abs(s.toolbar.x-initialToolbar.x+70)<2&&Math.abs(s.toolbar.y-initialToolbar.y+80)<2));
  await wait(async()=>!(await current()).interacting);await pause(600);
  check('toolbar drag keeps the same authorization session',handles===streamsBefore);
  const movedToolbar=(await current()).toolbar;
  check('region movement uses the normal inherited pointer', (await current()).buttons.find(b=>b.label==='Move capture region').cursor===null);
  await drag('Move capture region',40,24,s=>{
   check('separate region control moves only along the active capture axis',s.rect.x===70&&s.rect.y===174);
   check('region movement carries the manually placed toolbar by the actual selection delta',Math.abs(s.toolbar.x-movedToolbar.x)<2&&Math.abs(s.toolbar.y-movedToolbar.y-24)<2);
   check('dragging the region keeps the normal pointer',s.buttons.find(b=>b.label==='Move capture region').cursor===null);
   check('holding the region control never appends drag frames',s.interacting&&s.frames===initialFrames);
  });
  await drag('Move capture region',-40,-24);
  await wait(async()=>{const s=await current();return !s.interacting&&s.rect.y===150;});
  check('region roundtrip restores the manually placed toolbar',Math.abs((await current()).toolbar.x-movedToolbar.x)<2&&Math.abs((await current()).toolbar.y-movedToolbar.y)<2);
  // Keep its shadow outside the source too: a newly reached tail must be
  // captured completely before reversing away from it after a committed cut.
  await drag('Move toolbar',70,100);move(500,400);await pause(600);
  const stream=Gio.File.new_for_path(`${directory}/inline-scrolling.png`).replace(null,false,Gio.FileCreateFlags.PRIVATE,null);await new Shell.Screenshot().screenshot_area(0,0,1280,800,stream);stream.close(null);
  move(500,400);await pause(120);for(let n=0;n<16;n++){pointer.notify_discrete_scroll(GLib.get_monotonic_time(),Clutter.ScrollDirection.DOWN,Clutter.ScrollSource.WHEEL);await pause(220);}await pause(350);
  await wait(async()=>{const s=(await execute({command:'status'})).scrolling;return s?.frames>=2;});
  check('wheel input inside the frame reaches the underlying page and appends a changed frame',true);
  await action('Trim scrolling capture');await wait(async()=>{const s=(await execute({command:'status'})).scrolling;return s.capturing&&s.popupOpen;});
  await wait(async()=> (await current()).buttons.filter(b=>b.label.startsWith('Trim ')&&b.label!=='Trim scrolling capture').length===2);
  const trimButtons=(await current()).buttons.filter(b=>b.label.startsWith('Trim ')&&b.label!=='Trim scrolling capture');
  const trimAbove=trimButtons.find(b=>b.label==='Trim above current view');
  const trimBelow=trimButtons.find(b=>b.label==='Trim below current view');
  check('directional crop subbar uses compact square icon buttons',trimButtons.every(b=>b.rect.width===b.rect.height&&b.rect.height<=32));
  check('at the bottom boundary only Trim above is enabled while Trim below is disabled',trimAbove?.sensitive===true&&trimBelow?.sensitive===false);
  await observer.captureArea({x:0,y:0,width:1280,height:800},`${directory}/crop-subbar.png`);
  const beforeTrim=(await execute({command:'status'})).scrolling.height;
  await wait(()=>global.display.focus_window?.get_title()==='Clivuu scrolling capture');
  await key(Clutter.KEY_Return);
  await wait(async()=>{const s=(await execute({command:'status'})).scrolling;return s&&!s.popupOpen&&s.height<beforeTrim;});
  check('Enter in the inline trim menu activates cropping without exporting or closing the session',true);
  check('directional cut commits the current viewport and keeps capture active',(await current()).capturing&&(await current()).height===470);
  await action('Trim scrolling capture');await wait(async()=> (await current()).popupOpen&&(await current()).buttons.filter(b=>b.label.startsWith('Trim ')&&b.label!=='Trim scrolling capture').length===2);
  const singleTrimButtons=(await current()).buttons.filter(b=>b.label.startsWith('Trim ')&&b.label!=='Trim scrolling capture');
  check('single-viewport capture disables both directional trim buttons',singleTrimButtons.every(b=>b.sensitive===false));
  await key(Clutter.KEY_Escape);await wait(async()=>!(await current()).popupOpen);
  move(500,400);for(let n=0;n<4;n++){pointer.notify_discrete_scroll(GLib.get_monotonic_time(),Clutter.ScrollDirection.UP,Clutter.ScrollSource.WHEEL);await pause(300);}
  await wait(async()=>{const s=await current();return s.height>470&&s.position<1e-6;});
  check('scrolling continues after a committed crop',true);
  await action('Trim scrolling capture');await wait(async()=> (await current()).popupOpen&&(await current()).buttons.filter(b=>b.label.startsWith('Trim ')&&b.label!=='Trim scrolling capture').length===2);
  const topTrimButtons=(await current()).buttons.filter(b=>b.label.startsWith('Trim ')&&b.label!=='Trim scrolling capture');
  check('at the top boundary only Trim below is enabled while Trim above is disabled',topTrimButtons.find(b=>b.label==='Trim above current view')?.sensitive===false&&topTrimButtons.find(b=>b.label==='Trim below current view')?.sensitive===true);
  await wait(()=>global.display.focus_window?.get_title()==='Clivuu scrolling capture');
  await key(Clutter.KEY_Return);
  await wait(async()=>{const s=await current();return s&&!s.popupOpen&&s.height===470;});
  check('Trim below at the top boundary keeps the current viewport and keeps capture active',(await current()).capturing&&(await current()).height===470);
  move(500,400);for(let n=0;n<3;n++){pointer.notify_discrete_scroll(GLib.get_monotonic_time(),Clutter.ScrollDirection.UP,Clutter.ScrollSource.WHEEL);await pause(300);}
  await wait(async()=> (await current()).height>470);
  await action('Trim scrolling capture');await key(Clutter.KEY_Escape);await wait(async()=>!(await current()).popupOpen);
  check('Escape closes the trim subbar while keeping capture active',(await current()).capturing);
  await action('Save image');await wait(async()=>!(await execute({command:'status'})).busy);const result=await operation;
  const image=GdkPixbuf.Pixbuf.new_from_file(output);
  check('native long capture exports the cropped and newly extended document',result.path===output&&image.width===850&&image.height>470&&beforeTrim>940);
  let accent=0;const bytes=image.get_pixels(),width=image.width,height=image.height,stride=image.rowstride,channels=image.n_channels;for(let y=0;y<height;y++)for(let x=0;x<width;x++){const i=y*stride+x*channels;if(bytes[i]===189&&bytes[i+1]===147&&bytes[i+2]===249)accent++;}
  check('export contains no selection border or inline toolbar pixels',accent===0);
  check('finishing releases scrolling UI and keeps Desktop1 responsive',!(await execute({command:'status'})).busy&&!(await execute({command:'status'})).scrolling);
  {
   const hudTrimPath=`${directory}/hud-trim-output.png`,hudOp=execute({command:'scroll',rect:{x:70,y:150,width:850,height:470},output:hudTrimPath});
   await wait(async()=>{const s=await current();return s?.controlsVisible&&s.frames>=1;});
   move(500,400);await pause(120);for(let n=0;n<10;n++){pointer.notify_discrete_scroll(GLib.get_monotonic_time(),Clutter.ScrollDirection.DOWN,Clutter.ScrollSource.WHEEL);await pause(220);}await pause(300);
   await wait(async()=>{const s=await current();return s.frames>=2&&s.height>470;});
   await action('Trim scrolling capture');await wait(async()=> (await current()).popupOpen);
   await action('Trim above current view');await wait(async()=>{const s=await current();return s&&!s.popupOpen&&s.height===470;});
   move(500,400);for(let n=0;n<4;n++){pointer.notify_discrete_scroll(GLib.get_monotonic_time(),Clutter.ScrollDirection.UP,Clutter.ScrollSource.WHEEL);await pause(300);}
   await wait(async()=>{const s=await current();return s.height>470&&s.position<1e-6;});
   await action('Save image');await wait(async()=>!(await execute({command:'status'})).busy);const hudRes=await hudOp;
   const hudImg=GdkPixbuf.Pixbuf.new_from_file(hudTrimPath);
   check('default HUD-overlapping trim above current view + reverse scroll exports cleanly without holes',!hudRes.cancelled&&hudRes.path===hudTrimPath&&hudImg.width===850&&hudImg.height>470);
  }
  {
   const popupPath=`${directory}/popup-scroll-output.png`,popupOp=execute({command:'scroll',rect:{x:70,y:150,width:850,height:470},output:popupPath});
   await wait(async()=>{const s=await current();return s?.controlsVisible&&s.frames>=1;});
   move(500,400);await pause(120);
   for(let n=0;n<10;n++){pointer.notify_discrete_scroll(GLib.get_monotonic_time(),Clutter.ScrollDirection.DOWN,Clutter.ScrollSource.WHEEL);await pause(180);}
   await wait(async()=>{const s=await current();return s.frames>=2&&s.height>700;});
   await action('Trim scrolling capture');await wait(async()=> (await current()).popupOpen);
   // Scroll all the way from bottom to top while Trim popup is open so focused "Trim above" disables
   // and "Trim below" enables without triggering a RefCell panic or spurious focus-leave dismissal.
   move(500,400);
   for(let n=0;n<12;n++){pointer.notify_discrete_scroll(GLib.get_monotonic_time(),Clutter.ScrollDirection.UP,Clutter.ScrollSource.WHEEL);await pause(180);}
   await wait(async()=>{const s=await current();const top=s?.buttons.find(b=>b.label==='Trim above current view');const bot=s?.buttons.find(b=>b.label==='Trim below current view');return s?.popupOpen&&s.tracking&&!s.message&&s.position<1e-6&&top?.sensitive===false&&bot?.sensitive===true;});
   check('scrolling across top boundary with Trim popup open flips button sensitivity without crash or dismissal',true);
   // Keep the same Trim popup open and scroll all the way back to bottom so "Trim below" disables too.
   move(500,400);
   for(let n=0;n<14;n++){pointer.notify_discrete_scroll(GLib.get_monotonic_time(),Clutter.ScrollDirection.DOWN,Clutter.ScrollSource.WHEEL);await pause(180);}
   await wait(async()=>{const s=await current();const top=s?.buttons.find(b=>b.label==='Trim above current view');const bot=s?.buttons.find(b=>b.label==='Trim below current view');return s?.popupOpen&&s.tracking&&!s.message&&s.position+s.viewport>1-1e-6&&top?.sensitive===true&&bot?.sensitive===false;});
   check('scrolling across bottom boundary with Trim popup open updates preview and sensitivity safely',true);
   const trimHeightBeforeClose=(await current()).height;
   await key(Clutter.KEY_Escape);await wait(async()=>{const s=await current();return s&&!s.popupOpen;});
   check('closing Trim popup after scrolling keeps live tracking intact',(await current()).tracking&&!(await current()).message&&(await current()).height>=trimHeightBeforeClose);
   await action('Reset scrolling capture');await wait(async()=> (await current()).popupOpen);
   move(500,400);
   for(let n=0;n<18;n++){pointer.notify_discrete_scroll(GLib.get_monotonic_time(),Clutter.ScrollDirection.UP,Clutter.ScrollSource.WHEEL);await pause(180);}
   await wait(async()=>{const s=await current();return s?.popupOpen&&s.tracking&&!s.message&&s.position<1e-6;});
   const resetHeightBeforeDown=(await current()).height;
   move(500,400);
   for(let n=0;n<18;n++){pointer.notify_discrete_scroll(GLib.get_monotonic_time(),Clutter.ScrollDirection.DOWN,Clutter.ScrollSource.WHEEL);await pause(180);}
   await wait(async()=>{const s=await current();return s?.popupOpen&&s.tracking&&!s.message&&s.position+s.viewport>1-1e-6&&s.height>=resetHeightBeforeDown;});
   check('scrolling across boundaries with Reset popup open updates preview without overlap errors or dismissal',true);
   await key(Clutter.KEY_Escape);await wait(async()=>{const s=await current();return s&&!s.popupOpen;});
   check('closing Reset popup after scrolling keeps live tracking intact',(await current()).tracking&&!(await current()).message);
   // Drag toolbar clear below the region so Trim above current view completes cleanly without HUD refresh or flicker.
   await drag('Move toolbar',0,100);
   move(500,400);
   for(let n=0;n<6;n++){pointer.notify_discrete_scroll(GLib.get_monotonic_time(),Clutter.ScrollDirection.DOWN,Clutter.ScrollSource.WHEEL);await pause(180);}
   await wait(async()=> (await current()).height>470&&(await current()).position>1e-6);
   await action('Trim scrolling capture');await wait(async()=> (await current()).popupOpen);
   let watching=true,noFlicker=true;
   const watch=(async()=>{while(watching){const s=await current();if(s)noFlicker&&=!s.sampling&&s.previewOpacity===1&&s.frames>=1;await pause(15);}})();
   await action('Trim above current view');await wait(async()=>{const s=await current();return s&&!s.popupOpen&&s.height===470;});
   move(500,400);
   for(let n=0;n<4;n++){pointer.notify_discrete_scroll(GLib.get_monotonic_time(),Clutter.ScrollDirection.DOWN,Clutter.ScrollSource.WHEEL);await pause(180);}
   await wait(async()=> (await current()).height>470);
   await action('Reset scrolling capture');await wait(async()=> (await current()).popupOpen);
   await action('Restart in current region');await wait(async()=>{const s=await current();return s&&!s.popupOpen&&s.height===470&&s.frames===1;});
   watching=false;await watch;
   check('trim and reset keep selection frame and preview visible without flickering',noFlicker);
   await action('Cancel');await wait(async()=>!(await execute({command:'status'})).busy);await popupOp;
  }
  GLib.file_set_contents(`${directory}/fixture-color`,'#2468ac');await pause(450);
  const largePath=`${directory}/large-region.png`,largeOperation=execute({command:'scroll',rect:{x:12,y:60,width:1256,height:690},output:largePath});
  await wait(async()=>{const s=await current();return s?.frames>=1;});
  await visible('large-region sampling keeps its overlapping toolbar continuously visible');
  check('large live capture suppresses a preview that cannot fit outside the region',(await current()).preview===null);
  await pause(900);await action('Save image');await largeOperation;
  const largeImage=GdkPixbuf.Pixbuf.new_from_file(largePath),largePixels=largeImage.get_pixels();let unexpected=0;
  // GTK's identical foreground/background glyph AA can differ by one LSB.
  for(let y=0;y<largeImage.height;y++)for(let x=0;x<largeImage.width;x++){const n=y*largeImage.rowstride+x*largeImage.n_channels;if(Math.abs(largePixels[n]-36)>1||Math.abs(largePixels[n+1]-104)>1||Math.abs(largePixels[n+2]-172)>1)unexpected++;}
  check('large-region export contains only source pixels',unexpected===0);
  await verifyShadow();
  await verifyFullscreen();
  GLib.file_set_contents(`${directory}/fixture-color`,'');await pause(250);
  for(let cycle=0;cycle<8;cycle++){
   const cancelled=execute({command:'scroll',rect:cycle===1?{x:0,y:0,width:1280,height:800}:{x:70,y:150,width:850,height:470}});
   await wait(async()=>{const s=(await execute({command:'status'})).scrolling;return s?.controlsVisible&&s.frames>=1;});
   if(cycle===0){
    const initial=await current();await drag('Move capture region',30,22,s=>check('vertical region control always locks horizontal movement',s.rect.x===initial.rect.x&&s.rect.y===initial.rect.y+22&&s.frames===initial.frames));
    await drag('Move capture region',-30,-22);
    await action('Capture direction');await wait(async()=> (await current()).axis==='horizontal'&&(await current()).frames>=1);
    check('direction switch retains the fixed-size region',(await current()).rect.width===850&&(await current()).rect.height===470);
    const horizontal=await current();await drag('Move capture region',35,24,s=>{
     check('horizontal region control always locks vertical movement',s.rect.x===horizontal.rect.x+35&&s.rect.y===horizontal.rect.y&&s.frames===horizontal.frames);
     check('horizontal region dragging also uses the normal pointer',s.buttons.find(b=>b.label==='Move capture region').cursor===null);
    });
    await drag('Move capture region',-35,-24);
   }
   if(cycle===1)check('full-screen scrolling keeps its overlapping toolbar clickable',true);
   await action('Cancel');check(`Cancel cycle ${cycle+1} releases all surfaces without creating a pin`,(await cancelled).cancelled&&(await execute({command:'status'})).pins===0);
  }
  check('manual capture uses visible system sharing feedback',handles>0);
  await verifyAutomatic({execute,current,action,move,key,click,wait,check});
  await execute({command:'exit'});
 }catch(e){try{GLib.file_set_contents(`${directory}/scrolling-xshape.json`,await run(['python3',`${directory}/x11-shape.py`]));}catch{}try{const stream=Gio.File.new_for_path(`${directory}/inline-scrolling-failure.png`).replace(null,false,Gio.FileCreateFlags.PRIVATE,null);await new Shell.Screenshot().screenshot_area(0,0,1280,800,stream);stream.close(null);}catch{}let state;try{state=await execute({command:'status'});}catch{}GLib.file_set_contents(`${directory}/scrolling-failure.json`,JSON.stringify({state:state??null,focus:global.display.focus_window?.get_title()}));results.push({name:'native scrolling exception',passed:false,error:`${e.message}\n${e.stack}`});}
 finally{if(signal)global.backend.get_remote_access_controller().disconnect(signal);child?.force_exit();fixture?.force_exit();GLib.file_set_contents(`${directory}/results.json`,JSON.stringify(results));}
}

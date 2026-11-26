import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Clutter from 'gi://Clutter';
import Meta from 'gi://Meta';
import GdkPixbuf from 'gi://GdkPixbuf';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {pause,run} from './common/process.js';
import {authorizeScreenshot} from './screenshot-access.js';
const BUS='io.github.naerelyth.Clivuu.Desktop',PATH='/io/github/naerelyth/Clivuu/Desktop',IFACE=`${BUS}1`;
export async function probe(observer){
 const directory=GLib.getenv('CLIVUU_TEST_DIR'),results=[];let child,fixture;
 const check=(name,passed)=>{results.push({name,passed:Boolean(passed)});GLib.file_set_contents(`${directory}/native-pins-progress.json`,JSON.stringify(results));if(!passed)throw new Error(name);};
 const wait=async p=>{for(let i=0;i<160;i++){if(await p())return;await pause(50);}throw new Error('Native pins timed out');};
 const execute=r=>new Promise((resolve,reject)=>Gio.DBus.session.call(BUS,PATH,IFACE,'Execute',new GLib.Variant('(s)',[JSON.stringify({protocol:1,...r})]),null,0,30000,null,(c,result)=>{try{resolve(JSON.parse(c.call_finish(result).deep_unpack()[0]));}catch(e){reject(e);}}));
 const status=()=>execute({command:'status'});
 const pointer=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE),keyboard=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
 const move=(x,y)=>pointer.notify_absolute_motion(GLib.get_monotonic_time(),x,y);
 const click=async(x,y,b=1)=>{move(x,y);await pause(100);pointer.notify_button(GLib.get_monotonic_time(),b,Clutter.ButtonState.PRESSED);await pause(60);pointer.notify_button(GLib.get_monotonic_time(),b,Clutter.ButtonState.RELEASED);await pause(200);};
 const drag=async(a,b)=>{move(...a);await pause(120);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(80);for(let i=1;i<=16;i++){move(a[0]+(b[0]-a[0])*i/16,a[1]+(b[1]-a[1])*i/16);await pause(16);}pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(250);};
 const key=async(k,ctrl=false)=>{if(ctrl)keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Control_L,Clutter.KeyState.PRESSED);keyboard.notify_keyval(GLib.get_monotonic_time(),k,Clutter.KeyState.PRESSED);keyboard.notify_keyval(GLib.get_monotonic_time(),k,Clutter.KeyState.RELEASED);if(ctrl)keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Control_L,Clutter.KeyState.RELEASED);await pause(150);};
 const panel=()=>global.dashToPanel?.panels.map(p=>p.panelBox.visible)||[];
 const menus=()=>global.get_window_actors().map(a=>a.meta_window).filter(w=>w.get_title()==='Clivuu pin menu');
 const pinWindows=()=>global.get_window_actors().map(a=>a.meta_window).filter(w=>w.get_title()==='Clivuu pins');
 const toolbarAction=async label=>{let button;await wait(async()=>button=(await status()).pinUi.toolbar?.buttons.find(b=>(b.label===label||b.label.startsWith(label+' ('))&&b.rect.width>0));const r=button.rect;await click(r.x+r.width/2,r.y+r.height/2);};
 const menuAction=async label=>{let button;await wait(async()=>button=(await status()).pinUi.menu?.buttons.find(b=>b.label===label&&b.rect.width>0));const r=button.rect;await click(r.x+r.width/2,r.y+r.height/2);};
 try{
  GLib.file_set_contents(`${directory}/fixture-color`,'#2468ac');fixture=Gio.Subprocess.new(['gjs','-m',`${directory}/fixture.js`],0);await pause(1800);Main.overview.hide();await pause(700);
  const window=global.get_window_actors().map(a=>a.meta_window).find(w=>w.get_title()==='Clivuu focus fixture');window.activate(global.get_current_time());await pause(1000);const baseline=panel();
  const packaged=GLib.getenv('CLIVUU_TEST_APPIMAGE')==='1',cli=observer._workerPath(),desktop=GLib.getenv('CLIVUU_TEST_DESKTOP_BINARY')||GLib.build_filenamev([GLib.path_get_dirname(cli),'clivuu-desktop']);
  const launcher=new Gio.SubprocessLauncher({flags:0});if(packaged){launcher.setenv('CLIVUU_DESKTOP_DIR','',true);}launcher.unsetenv('APPIMAGE');launcher.setenv('GDK_BACKEND','x11',true);launcher.setenv('CLIVUU_LANGUAGE','en',true);launcher.set_stderr_file_path(`${directory}/rust-pins.log`);child=launcher.spawnv(packaged?[cli]:[desktop]);await wait(async()=>{try{return(await status()).backend==='rust';}catch{return false;}});
  await run([cli,'config','set','pin-gnome-focus','false']);await pause(200);
  await authorizeScreenshot(()=>execute({command:'snip',rect:{x:200,y:180,width:340,height:200},output:'pin'}));await wait(()=>pinWindows().length>0);await pause(300);
  const first=(await status()).images[0].rect;
  check('native pin preserves the original selection position and dimensions',first.x===200&&first.y===180&&first.width===340&&first.height===200);
  check('native pins use only override-redirect surfaces with an outside gutter',pinWindows().every(w=>w.get_window_type()===Meta.WindowType.OVERRIDE_OTHER&&!w.is_monitor_sized()));
  check('pin creation preserves native Wayland focus and the natural panel state',global.display.focus_window===window&&JSON.stringify(panel())===JSON.stringify(baseline));
  await click(270,240);check('a logical pin click does not change desktop keyboard focus or panel visibility',global.display.focus_window===window&&JSON.stringify(panel())===JSON.stringify(baseline));
  await pause(650);
  const fc1=(await status()).pinUi.frameClock;await pause(200);const fc2=(await status()).pinUi.frameClock;
  check(`XWayland pin surface paces GdkFrameClock above the 60 Hz fallback without idle repaints (${JSON.stringify({fc1,fc2})})`,fc1?.refreshIntervalUs>0&&fc1.refreshIntervalUs<=4000&&fc2?.frameCounter===fc1.frameCounter);
  await drag([270,240],[350,290]);let rect=(await status()).images[0].rect;
  GLib.file_set_contents(`${directory}/drag-geometry.json`,JSON.stringify({rect,focus:global.display.focus_window?.get_title()}));
  check('native pin moves by the pointer delta in stationary host coordinates',rect.x===280&&rect.y===230&&global.display.focus_window===window);
  await drag([rect.x+rect.width-2,rect.y+rect.height/2],[rect.x+rect.width+58,rect.y+rect.height/2]);rect=(await status()).images[0].rect;
  check('native pin edge resize preserves the image aspect ratio',rect.width>340&&Math.abs(rect.width/rect.height-1.7)<0.02);
  const serial=()=>{try{return JSON.parse(new TextDecoder().decode(GLib.file_get_contents(`${directory}/fixture-last-click`)[1])).serial;}catch{return 0;}};
  for(let cycle=0;cycle<3;cycle++){await click(rect.x+40,rect.y+40,3);await wait(()=>menus().length===pinWindows().length);await click(900,650);await wait(()=>menus().length===0);}
  const before=serial();await click(900,600);await wait(()=>serial()>before);check('three menu outside-dismiss cycles leave the native desktop clickable',serial()>before);
  await click(rect.x+40,rect.y+40,3);await wait(()=>menus().length>0);await click(rect.x+45,rect.y+45,3);await wait(()=>menus().length===0);check('second right click closes the menu without moving or reopening it',menus().length===0);
  await run([cli,'config','set','pin-gnome-focus','true']);await pause(200);
  {
   await run([cli,'config','set','pin-zoom-at-cursor','true']);await pause(200);
   await click(rect.x+40,rect.y+40,3);await menuAction('Reset size');await wait(async()=>Math.abs((await status()).images[0].zoom-1)<1e-6);
   rect=(await status()).images[0].rect;
   const countPinGhosts=(shotPath,w,h,keepBox)=>{const pb=GdkPixbuf.Pixbuf.new_from_file(shotPath),stride=pb.get_rowstride(),n=pb.get_n_channels(),px=pb.get_pixels();let ghosts=0;for(let y=16;y<Math.floor(h)-16;y++){for(let x=16;x<Math.floor(w)-16;x++){if(x>=keepBox[0]&&x<=keepBox[2]&&y>=keepBox[1]&&y<=keepBox[3])continue;const o=y*stride+x*n;if(Math.abs(px[o]-36)>5||Math.abs(px[o+1]-104)>5||Math.abs(px[o+2]-172)>5)ghosts++;}}return ghosts;};
   await click(rect.x+10,rect.y+45);await wait(()=>pinWindows().includes(global.display.focus_window));
   await key(Clutter.KEY_equal);
   await wait(async()=>(await status()).images[0].zoom>1.05);
   const frac=(await status()).images[0].rect;
   check(`zoom-at-cursor positions real pin at integer X and 0.5 fractional Y (${JSON.stringify(frac)})`,Math.abs(frac.x-Math.round(frac.x))<1e-6&&Math.abs((((frac.y%1)+1)%1)-0.5)<1e-6);
   await click(frac.x+40,frac.y+40,3);await menuAction('Show toolbar');await wait(async()=>Boolean((await status()).pinUi.toolbar));
   await key(Clutter.KEY_2,true);await wait(async()=>(await status()).pinUi.toolbar?.tool==='rect');
   await key(Clutter.KEY_3,true);await wait(async()=>(await status()).pinUi.toolbar?.tool==='arrow');
   await key(Clutter.KEY_5,true);await wait(async()=>(await status()).pinUi.toolbar?.tool==='highlight');
   await key(Clutter.KEY_2,true);await wait(async()=>(await status()).pinUi.toolbar?.tool==='rect');
   await key(Clutter.KEY_3,true);await wait(async()=>(await status()).pinUi.toolbar?.tool==='arrow');
   move(900,650);await pause(220);
   const tbUi=(await status()).pinUi.toolbar;
   const shapesBtn=tbUi.buttons.find(b=>b.label.startsWith('Shapes'));
   const hlBtn=tbUi.buttons.find(b=>b.label.startsWith('Highlight'));
   const tbShot=`${directory}/toolbar-switch.png`;
   const tbX=Math.floor(shapesBtn.rect.x),tbY=Math.floor(shapesBtn.rect.y);
   await observer.captureArea({x:tbX,y:tbY,width:Math.ceil(hlBtn.rect.x+hlBtn.rect.width-tbX),height:Math.ceil(shapesBtn.rect.height)},tbShot);
   const tbPb=GdkPixbuf.Pixbuf.new_from_file(tbShot),tbS=tbPb.get_rowstride(),tbN=tbPb.get_n_channels(),tbP=tbPb.get_pixels();
   let tbLeaked=0;
   for(const btn of [shapesBtn,hlBtn]){const bx0=Math.max(0,Math.floor(btn.rect.x-tbX)+1),bx1=Math.min(tbPb.get_width(),Math.ceil(btn.rect.x+btn.rect.width-tbX)-1),by0=Math.max(0,Math.floor(btn.rect.y-tbY)+1),by1=Math.min(tbPb.get_height(),Math.ceil(btn.rect.y+btn.rect.height-tbY));for(let y=by0;y<by1;y++){for(let x=bx0;x<bx1;x++){const o=y*tbS+x*tbN,r=tbP[o],g=tbP[o+1],b=tbP[o+2];if(Math.abs(r-b)>24&&Math.max(r-g,b-g)>20)tbLeaked++;}}}
   check(`switching annotation tools on a fractional-Y pin leaves zero bottom highlight leak (${tbLeaked})`,tbLeaked===0);

   await key(Clutter.KEY_5,true);await wait(async()=>(await status()).pinUi.toolbar?.tool==='highlight');
   const hlRectOpt=(await status()).pinUi.toolbar.options.find(b=>b.label==='Rectangle');
   await click(hlRectOpt.rect.x+hlRectOpt.rect.width/2,hlRectOpt.rect.y+hlRectOpt.rect.height/2);
   await drag([frac.x+30,frac.y+30],[frac.x+310,frac.y+185]);await wait(async()=>(await status()).pinUi.toolbar?.marks.length===1);
   await drag([frac.x+310,frac.y+185],[frac.x+220,frac.y+140]);
   await drag([frac.x+220,frac.y+140],[frac.x+130,frac.y+95]);
   move(900,650);await pause(250);
   await observer.captureArea({x:Math.floor(frac.x),y:Math.floor(frac.y),width:Math.ceil(frac.width),height:Math.ceil(frac.height)},`${directory}/ghost-hl-rect.png`);
   const hlRectGhosts=countPinGhosts(`${directory}/ghost-hl-rect.png`,frac.width,frac.height,[16,16,146,111]);
   check(`resizing a highlight rectangle on an integer-X fractional-Y pin leaves zero edge or handle ghosts (${hlRectGhosts})`,hlRectGhosts===0);

   while((await status()).pinUi.toolbar?.marks.length>0)await key(Clutter.KEY_z,true);
   const hlEllipseOpt=(await status()).pinUi.toolbar.options.find(b=>b.label==='Ellipse');
   await click(hlEllipseOpt.rect.x+hlEllipseOpt.rect.width/2,hlEllipseOpt.rect.y+hlEllipseOpt.rect.height/2);
   await drag([frac.x+30,frac.y+30],[frac.x+310,frac.y+185]);await wait(async()=>(await status()).pinUi.toolbar?.marks.length===1);
   await drag([frac.x+310,frac.y+185],[frac.x+220,frac.y+140]);
   await drag([frac.x+220,frac.y+140],[frac.x+130,frac.y+95]);
   move(900,650);await pause(250);
   await observer.captureArea({x:Math.floor(frac.x),y:Math.floor(frac.y),width:Math.ceil(frac.width),height:Math.ceil(frac.height)},`${directory}/ghost-hl-ellipse.png`);
   const hlEllipseGhosts=countPinGhosts(`${directory}/ghost-hl-ellipse.png`,frac.width,frac.height,[16,16,146,111]);
   check(`resizing a highlight ellipse on an integer-X fractional-Y pin leaves zero edge or handle ghosts (${hlEllipseGhosts})`,hlEllipseGhosts===0);

   while((await status()).pinUi.toolbar?.marks.length>0)await key(Clutter.KEY_z,true);
   await key(Clutter.KEY_2,true);await wait(async()=>(await status()).pinUi.toolbar?.tool==='rect');
   await drag([frac.x+30,frac.y+30],[frac.x+310,frac.y+185]);await wait(async()=>(await status()).pinUi.toolbar?.marks.length===1);
   await drag([frac.x+310,frac.y+185],[frac.x+220,frac.y+140]);
   await drag([frac.x+220,frac.y+140],[frac.x+130,frac.y+95]);
   move(900,650);await pause(250);
   await observer.captureArea({x:Math.floor(frac.x),y:Math.floor(frac.y),width:Math.ceil(frac.width),height:Math.ceil(frac.height)},`${directory}/ghost-rect.png`);
   const rectGhosts=countPinGhosts(`${directory}/ghost-rect.png`,frac.width,frac.height,[16,16,146,111]);
   check(`resizing a rectangle shape on an integer-X fractional-Y pin leaves zero anchor handle ghosts (${rectGhosts})`,rectGhosts===0);

   await toolbarAction('Cancel');await wait(async()=>!(await status()).pinUi.toolbar);
   await click(frac.x+40,frac.y+40,3);await menuAction('Reset size');await wait(async()=>Math.abs((await status()).images[0].zoom-1)<1e-6);
   await run([cli,'config','set','pin-zoom-at-cursor','false']);await pause(200);
   rect=(await status()).images[0].rect;
  }
  await click(rect.x+40,rect.y+40,3);await menuAction('Show toolbar');await wait(async()=>Boolean((await status()).pinUi.toolbar));await wait(()=>pinWindows().includes(global.display.focus_window));
  await key(Clutter.KEY_2,true);await drag([rect.x+50,rect.y+50],[rect.x+100,rect.y+100]);await drag([rect.x+150,rect.y+50],[rect.x+210,rect.y+110]);await wait(async()=>(await status()).pinUi.toolbar?.marks.length===2);
  await key(Clutter.KEY_z,true);let ui=(await status()).pinUi.toolbar;check('native pin Ctrl+Z removes exactly one annotation using the common editor',ui.marks.length===1&&ui.canUndo&&ui.canRedo);
  await key(Clutter.KEY_3,true);keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Shift_L,Clutter.KeyState.PRESSED);await drag([rect.x+70,rect.y+130],[rect.x+210,rect.y+180]);keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Shift_L,Clutter.KeyState.RELEASED);await pause(200);
  ui=(await status()).pinUi.toolbar;const arrow=ui.marks.at(-1),[a,b]=arrow.points,dx=Math.abs(a[0]-b[0]),dy=Math.abs(a[1]-b[1]);check('real pin editor shares the Shift 45-degree arrow constraint',arrow.kind==='arrow'&&Math.min(dx,dy,Math.abs(dx-dy))<1e-6&&ui.marks.length===2);
  await key(Clutter.KEY_z,true);check('one Ctrl+Z removes the constrained pin arrow and preserves the previous shape',(await status()).pinUi.toolbar.marks.length===1);
  await key(Clutter.KEY_2,true);
  for(const kind of ['rect','ellipse']){
   if(kind==='ellipse'){const opt=(await status()).pinUi.toolbar.options.find(b=>b.label==='Ellipse');await click(opt.rect.x+opt.rect.width/2,opt.rect.y+opt.rect.height/2);}
   keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Shift_L,Clutter.KeyState.PRESSED);await drag([rect.x+240,rect.y+40],[rect.x+310,rect.y+80]);keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Shift_L,Clutter.KeyState.RELEASED);await pause(150);
   const m=(await status()).pinUi.toolbar.marks.at(-1),p=m.points;check(`real pin editor shares the Shift ${kind} constraint`,m.kind===kind&&Math.abs(Math.abs(p[1][0]-p[0][0])-Math.abs(p[1][1]-p[0][1]))<1e-6);
   await key(Clutter.KEY_z,true);check(`one pin Undo removes only the constrained ${kind}`,(await status()).pinUi.toolbar.marks.length===1);
  }
  await toolbarAction('Crop');await drag([rect.x+40,rect.y+30],[rect.x+190,rect.y+130]);await key(Clutter.KEY_3,true);let cropUi=(await status()).pinUi.toolbar;
  check('choosing a shared annotation tool cancels an unapplied pin crop',cropUi.tool==='arrow'&&cropUi.crop===null);
  const beforeCrop={...rect};
  await toolbarAction('Crop');await drag([rect.x+40,rect.y+30],[rect.x+170,rect.y+110]);
  const c0=(await status()).pinUi.toolbar.crop;
  await drag([rect.x+105,rect.y+70],[rect.x+125,rect.y+85]);
  const c1=(await status()).pinUi.toolbar.crop;
  check('dragging inside an active pin crop moves the crop region without changing its size',c1.x>c0.x&&c1.y>c0.y&&c1.width===c0.width&&c1.height===c0.height);
  const seX=rect.x+(c1.x+c1.width)/340*rect.width,seY=rect.y+(c1.y+c1.height)/200*rect.height;
  await drag([seX,seY],[seX+20,seY+20]);
  const c2=(await status()).pinUi.toolbar.crop;
  check('dragging a pin crop corner resizes the crop region while preserving its top-left origin',c2.x===c1.x&&c2.y===c1.y&&c2.width>c1.width&&c2.height>c1.height);
  await toolbarAction('Apply changes');GLib.file_set_contents(`${directory}/after-crop-apply.json`,JSON.stringify(await status()));await wait(async()=>!(await status()).pinUi.toolbar);rect=(await status()).images[0].rect;
  check('native pin cropping applies source pixels, logical dimensions, and framed desktop position',Math.abs(rect.width-150)<=2&&Math.abs(rect.height-100)<=2&&Math.abs(rect.x-(beforeCrop.x+60))<=2&&Math.abs(rect.y-(beforeCrop.y+45))<=2);
  await click(rect.x+30,rect.y+25,3);await menuAction('Show toolbar');await toolbarAction('Rotate right');await wait(async()=>Math.abs((await status()).images[0].rect.width-100)<=2);
  check('native pin rotation keeps the shared toolbar usable',Boolean((await status()).pinUi.toolbar));await toolbarAction('Rotate left');await wait(async()=>Math.abs((await status()).images[0].rect.width-150)<=2);rect=(await status()).images[0].rect;
  await key(Clutter.KEY_2,true);await drag([rect.x+25,rect.y+20],[rect.x+70,rect.y+55]);await wait(async()=>(await status()).pinUi.toolbar?.marks.length===1);await toolbarAction('Cancel');await wait(async()=>!(await status()).pinUi.toolbar);
  await click(rect.x+20,rect.y+20,3);await menuAction('Show toolbar');
  check('Cancel restores the prior image without retaining newly completed annotations',(await status()).pinUi.toolbar.marks.length===0);
  await key(Clutter.KEY_Escape);await wait(async()=>!(await status()).pinUi.toolbar);check('hiding the native pin toolbar preserves its keyboard context',!(await status()).pinUi.toolbar);
  await click(rect.x+40,rect.y+40,3);await menuAction('Close pin');await wait(async()=>(await status()).closedPins===1);check('Close archives the pin rather than destroying it',(await status()).pins===0);
  await execute({command:'reopen-pin'});await wait(async()=>(await status()).pins===1);check('last closed pin can be restored at its original geometry',(await status()).images[0].rect.x===rect.x);
  await run([cli,'config','set','pin-gnome-focus','false']);await pause(200);
  window.minimize();move(900,500);await pause(1000);const desktopPanel=panel();check('pins do not inhibit a panel that should be visible on the desktop',desktopPanel.every(Boolean));
  await click(rect.x+40,rect.y+40,3);await wait(()=>menus().length>0);const entry=(await status()).pinUi.menu.buttons[0].rect;move(entry.x+20,entry.y+entry.height/2);await pause(300);check('hovering a native pin menu preserves the visible desktop panel',JSON.stringify(panel())===JSON.stringify(desktopPanel));await click(900,650);
  const manifest=`${directory}/data/clivuu/pins/pins.json`;
  await pause(1600);const readBoard=()=>JSON.parse(new TextDecoder().decode(GLib.file_get_contents(manifest)[1]));
  const saved=GLib.file_get_contents(manifest)[1];await pause(1400);
  check('idle edited pin does not repeatedly write its manifest or image revisions',new TextDecoder().decode(saved)===new TextDecoder().decode(GLib.file_get_contents(manifest)[1]));
  await execute({command:'exit'});await new Promise((resolve,reject)=>child.wait_async(null,(p,r)=>{try{p.wait_finish(r);resolve();}catch(e){reject(e);}}));
  await wait(()=>pinWindows().length===0);
  child=launcher.spawnv(packaged?[cli]:[desktop]);await wait(async()=>{try{return(await status()).ready;}catch{return false;}});
  // D-Bus readiness precedes restored XWayland surface presentation. Match the
  // initial pin's mapped-window/frame wait before injecting a real click.
  await wait(()=>pinWindows().length>0);await pause(300);
  check('native pin board and original geometry survive service restart',(await status()).pins===1&&(await status()).images[0].rect.x===rect.x&&!readBoard().pins[0].closed);
  await click(rect.x+40,rect.y+40,3);await menuAction('Close pin');await wait(async()=>(await status()).closedPins===1);await pause(1300);
  check('closed state is published before reopen',readBoard().pins[0].closed);
  await wait(async()=> (await status()).pinUi.closedResidentImages===0);check('durable closed pins release decoded pixels, textures and editor views',(await status()).pinUi.residentImages===0);
  await execute({command:'exit'});await new Promise((resolve,reject)=>child.wait_async(null,(p,r)=>{try{p.wait_finish(r);resolve();}catch(e){reject(e);}}));child=launcher.spawnv(packaged?[cli]:[desktop]);await wait(async()=>{try{return (await status()).ready;}catch{return false;}});
  check('startup reads closed metadata without decoding closed images',(await status()).closedPins===1&&(await status()).pinUi.residentImages===0);
  await execute({command:'reopen-pin'});await pause(1300);check('reopening a persisted pin publishes the new board state',!readBoard().pins[0].closed);
  await execute({command:'paste',text:'Native text pin',position:[1500,220]});let text=(await status()).images[1].rect;
  check('native text pin honors explicit placement on the second output',text.x===1500&&text.y===220);
  await run([cli,'config','set','pin-gnome-focus','true']);await pause(200);
  await click(text.x+25,text.y+20,3);await menuAction('Show toolbar');await wait(()=>global.display.focus_window?.get_title()==='Clivuu pins');
  await key(Clutter.KEY_2,true);await drag([text.x+15,text.y+15],[text.x+85,text.y+35]);await wait(async()=>(await status()).pinUi.toolbar?.marks.length===1);await key(Clutter.KEY_z,true);
  check('second-output pin uses the same editor keyboard and undo model',(await status()).pinUi.toolbar.marks.length===0);await key(Clutter.KEY_Escape);
  await execute({command:'open-preferences'});await wait(()=>global.get_window_actors().some(a=>a.meta_window.get_title()==='Clivuu Preferences'));
  check('native preferences open through the desktop command',true);
  await execute({command:'exit'});
  const logText=GLib.file_test(`${directory}/rust-pins.log`,GLib.FileTest.EXISTS)?new TextDecoder().decode(GLib.file_get_contents(`${directory}/rust-pins.log`)[1]):'';
  check('pin desktop session emits no Gsk or Gtk warnings',!/(Gsk|Gtk)-(WARNING|CRITICAL)/i.test(logText));
 }catch(e){try{GLib.file_set_contents(`${directory}/pin-failure-state.json`,JSON.stringify(await status()));}catch{}results.push({name:'native pin exception',passed:false,error:`${e.message}\n${e.stack}`});}
 finally{child?.force_exit();fixture?.force_exit();GLib.file_set_contents(`${directory}/results.json`,JSON.stringify(results));}
}

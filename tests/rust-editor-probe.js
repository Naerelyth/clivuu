import Clutter from 'gi://Clutter';
import GdkPixbuf from 'gi://GdkPixbuf';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import Shell from 'gi://Shell';
import {pause} from './common/process.js';
export async function probe(){
 const directory=GLib.getenv('CLIVUU_TEST_DIR'),results=[];let child;
 const check=(name,passed)=>{results.push({name,passed:Boolean(passed)});if(!passed)throw new Error(name);};
 const wait=async predicate=>{for(let i=0;i<100;i++){if(await predicate())return;await pause(50);}throw new Error('Rust GTK probe timed out');};
 const read=name=>JSON.parse(new TextDecoder().decode(GLib.file_get_contents(`${directory}/${name}`)[1]));
 try{
  const launcher=new Gio.SubprocessLauncher({flags:0});launcher.set_stderr_file_path(`${directory}/rust-editor.log`);launcher.unsetenv('APPIMAGE');
  child=launcher.spawnv([GLib.getenv('CLIVUU_RUST_EDITOR_PROBE')]);await wait(()=>GLib.file_test(`${directory}/rust-editor-ready`,GLib.FileTest.EXISTS));
  results.push(...read('rust-render-results.json'));if(results.some(r=>!r.passed))throw new Error('native render pixels');
  Main.overview.hide();const window=global.get_window_actors().map(a=>a.meta_window).find(w=>w.get_title()==='Clivuu Rust editor probe');window.activate(global.get_current_time());await pause(500);
  const pointer=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
  const keyboard=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
  const move=(x,y)=>pointer.notify_absolute_motion(GLib.get_monotonic_time(),x,y);
  const drag=async(a,b)=>{move(...a);await pause(150);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(80);
   for(let i=1;i<=8;i++){move(a[0]+(b[0]-a[0])*i/8,a[1]+(b[1]-a[1])*i/8);await pause(16);}
   pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(180);};
  const key=async(k,shift=false)=>{keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Control_L,Clutter.KeyState.PRESSED);if(shift)keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Shift_L,Clutter.KeyState.PRESSED);
   keyboard.notify_keyval(GLib.get_monotonic_time(),k,Clutter.KeyState.PRESSED);keyboard.notify_keyval(GLib.get_monotonic_time(),k,Clutter.KeyState.RELEASED);
   if(shift)keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Shift_L,Clutter.KeyState.RELEASED);keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Control_L,Clutter.KeyState.RELEASED);await pause(200);};
  const state=()=>read('rust-editor-state.json');
  check('real Rust toolbar starts with undo and redo disabled',!state().toolbar.undo.sensitive&&!state().toolbar.redo.sensitive);
  check('Rust toolbar retains the 32px height',state().toolbar.height===32);
  const bar=state().layout.bar;
  check('shared editor view right-aligns the toolbar below the original image with an 8px gap',bar[0]+bar[2]===740&&bar[1]===528);
  move(1000,600);await pause(150);
  await drag([140,160],[240,240]);await wait(()=>state().marks.length===1);
  await drag([360,180],[480,260]);await wait(()=>state().marks.length===2);check('Rust scene accepts two real GTK annotation drags',state().marks.length===2);
  const button=async name=>{const r=state().toolbar[name];move(r.x+r.width/2,r.y+r.height/2);await pause(100);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(80);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(200);};
  await button('undo');check('one real toolbar Undo click removes only one element',state().marks.length===1&&state().toolbar.redo.sensitive);
  await button('redo');check('real toolbar Redo restores the same element',state().marks.length===2&&!state().toolbar.redo.sensitive);
  const original=state().layout.bar,grip=state().layout.grip;
  await drag([grip.x+grip.width/2,grip.y+grip.height/2],[grip.x+grip.width/2+180,grip.y+grip.height/2-80]);
  await wait(()=>Math.abs(state().layout.bar[0]-original[0]-180)<1);
  check('shared toolbar grip follows the native pointer displacement exactly',Math.abs(state().layout.bar[1]-original[1]+80)<1);
  await key(Clutter.KEY_z);check('real Ctrl+Z removes only the last Rust annotation',state().marks.length===1&&state().canRedo);
  await key(Clutter.KEY_z);check('second Ctrl+Z removes the first element and disables undo',state().marks.length===0&&!state().canUndo);
  await key(Clutter.KEY_y);check('Ctrl+Y restores one element',state().marks.length===1);
  await key(Clutter.KEY_y);check('Ctrl+Y restores the second element and disables redo',state().marks.length===2&&!state().canRedo);
  const click=async r=>{move(r.x+r.width/2,r.y+r.height/2);await pause(100);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(60);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(200);};
  const option=label=>click(state().options.controls.find(c=>c.label===label));
  const tool=label=>click(state().tools.find(c=>c.label===label));
  const plain=async k=>{keyboard.notify_keyval(GLib.get_monotonic_time(),k,Clutter.KeyState.PRESSED);keyboard.notify_keyval(GLib.get_monotonic_time(),k,Clutter.KeyState.RELEASED);await pause(50);};
  let captureState=null;
  const legacyTool=async code=>{
   const labels=['Cursor (Ctrl+1)','Shapes (Ctrl+2)','Shapes (Ctrl+2)','Arrow (Ctrl+3)','Pen (Ctrl+4)','Highlight (Ctrl+5)','Mosaic (Ctrl+7)','Text (Ctrl+8)','Eraser (E)'];
   const get=captureState??state,index=code-Clutter.KEY_1;
   await click(get().tools.find(b=>b.label===labels[index]));
   if(index===1||index===2)await click(get().options.controls.find(b=>b.label===(index===1?'Rectangle':'Ellipse')));
  };
  await tool('Sequence (Ctrl+6)');
  await click({x:580,y:300,width:1,height:1});await wait(()=>state().marks.length===3);
  check('sequence click creates a numbered badge in the shared editor',state().current.kind==='sequence'&&state().current.sequence.index===1);
  await drag([630,300],[690,345]);await wait(()=>state().marks.length===4);
  check('sequence drag creates the next badge and a leader',state().current.sequence.index===2&&state().current.points.length===2&&state().handles.some(h=>h.kind==='endpoint'));
  await key(Clutter.KEY_z);await wait(()=>state().marks.length===3);
  await click({x:630,y:380,width:1,height:1});await wait(()=>state().marks.length===4);
  check('undo followed by a new badge reuses the uncommitted next number',state().current.sequence.index===2&&!state().canRedo);
  await plain(Clutter.KEY_z);await plain(Clutter.KEY_z);await plain(Clutter.KEY_x);
  await wait(()=>state().sequenceNext===4);
  check('Z/X change only the future sequence number while a placed badge is selected',state().current.sequence.index===2);
  const placedControl=async kind=>{const h=state().handles.find(h=>h.kind===kind);await click({x:h.point[0]+100,y:h.point[1]+120,width:1,height:1});};
  await placedControl('increase');await wait(()=>state().current.sequence.index===3);
  check('hover increment edits the placed sequence without changing its future counter',state().sequenceNext===4&&state().marks.length===4);
  await key(Clutter.KEY_z);await wait(()=>state().current.sequence.index===2);
  check('placed sequence increment is a single reversible edit',state().sequenceNext===4);
  const preview=state().options.controls.find(c=>c.label==='Next sequence number');
  check('sequence subbar matches the shared 32px subbar height and fits swatches',state().options.height===32&&state().options.swatchesFit);
  move(preview.x+preview.width/2,preview.y+preview.height/2);await pause(250);
  check('hovering the next-number preview does not open an accidental shape menu',!state().options.popup);
  await option('Badge shape');await wait(()=>state().options.popup);
  check('badge shape selector offers five compact visual choices',state().optionMenu.filter(v=>['Filled circle','Bordered circle','Outline circle','Number only','Rounded square'].includes(v.label)).length===5);
  await click(state().optionMenu.find(v=>v.label==='Outline circle'));await wait(()=>!state().options.popup);
  check('sequence shape changes the selected badge without replacing earlier marks',state().current.sequence.shape==='outline-circle'&&state().marks.length===4);
  check('sequence uses one fixed PixPin leader without an arrow-style selector',!state().options.controls.some(v=>v.label==='Sequence leader'));
  await placedControl('toggleleader');await wait(()=>state().current.points.length===2);
  await placedControl('toggleleader');await wait(()=>state().current.points.length===1);
  check('hover arrow control adds and removes the placed leader in separate reversible edits',state().sequenceNext===4);
  await option('Next sequence number');await wait(()=>state().options.popup);
  await key(Clutter.KEY_a);await plain(Clutter.KEY_4);await plain(Clutter.KEY_2);await plain(Clutter.KEY_z);
  check('the next-number input owns typing rather than invoking canvas Z',state().sequenceNext===4&&state().current.sequence.index===2);
  await plain(Clutter.KEY_Return);await wait(()=>!state().options.popup&&state().sequenceNext===42);
  check('Enter confirms the future index without editing a placed number',state().current.sequence.index===2);
  await option('Next sequence number');await wait(()=>state().options.popup);await plain(Clutter.KEY_Escape);await wait(()=>!state().options.popup);
  check('Escape dismisses the future-index picker without changing the counter',state().sequenceNext===42);
  const sequenceStream=Gio.File.new_for_path(`${directory}/sequence-editor.png`).replace(null,false,Gio.FileCreateFlags.PRIVATE,null);await new Shell.Screenshot().screenshot_area(0,0,1280,800,sequenceStream);sequenceStream.close(null);
  await plain(Clutter.KEY_Delete);await wait(()=>state().marks.length===3);
  await key(Clutter.KEY_1);check('closing the index picker restores editor tool shortcuts',state().tool==='select');await click({x:580,y:300,width:1,height:1});await plain(Clutter.KEY_Delete);await wait(()=>state().marks.length===2);
  const bakedBefore=JSON.stringify(state().marks);
  await click({x:360,y:220,width:1,height:1});await drag([360,220],[400,250]);
  check('baked non-text shapes stay locked and cannot be reselected or moved in Cursor mode',state().current===null&&JSON.stringify(state().marks)===bakedBefore);
  await tool('Shapes (Ctrl+2)');await drag([520,180],[620,250]);await wait(()=>state().marks.length===3&&state().current!==null);
  check('content-sized native subbar is separate, remains 32px high, and fits both swatch rows without clipping',state().options.height===32&&state().options.swatchesFit);
  await option('Ellipse');check('native shape subbar changes the selected mark rather than duplicating it',state().marks.length===3&&state().marks[2].kind==='ellipse');
  await option('Filled');check('one native fill toggle enables fill',state().marks[2].fill==='solid');
  await button('undo');check('undo fill preserves the earlier ellipse change',state().marks[2].fill==='stroke'&&state().marks[2].kind==='ellipse');
  await option('Choose color');await wait(()=>state().options.popup);
  check('native color panel opens without a managed editor window',state().options.popup&&global.get_window_actors().filter(a=>a.meta_window.get_title()==='Clivuu Rust editor probe').length===1);
  await wait(()=>state().colorPanel);
  const colorLayout=state().colorPanel;
  check('color panel and its inputs inherit the toolbar font',colorLayout.font===state().toolbarFont&&colorLayout.alpha.inheritsFont&&colorLayout.html.inheritsFont);
  check('all 64 basic and custom colors are square controls',colorLayout.swatches.length===64&&colorLayout.swatches.every(([w,h])=>w===h&&w>=20));
  check('HSV, RGB, HTML, and Alpha align in a uniform 3-column flush grid',colorLayout.gridAligned&&colorLayout.channelsUniform);
  check('channel spin steppers stay hidden with preview, alpha slider, and hex copy button visible',colorLayout.steppersHidden&&colorLayout.hasPreview&&colorLayout.hasAlphaSlider&&colorLayout.hasCopyButton);
  GLib.file_set_contents(`${directory}/toolbar-color-layout.json`,JSON.stringify(colorLayout));
  const colorStream=Gio.File.new_for_path(`${directory}/toolbar-color-panel.png`).replace(null,false,Gio.FileCreateFlags.PRIVATE,null);await new Shell.Screenshot().screenshot_area(0,0,1280,800,colorStream);colorStream.close(null);
  await plain(Clutter.KEY_Escape);await wait(()=>!state().options.popup);check('Escape closes the native color panel',!state().options.popup);
  await plain(Clutter.KEY_Delete);await wait(()=>state().marks.length===2);
  await tool('Arrow (Ctrl+3)');
  const labels=state().options.controls.map(c=>c.label);
  check('arrow subbar has exactly the two PixPin selectors and no endpoint controls',labels.includes('Arrow type')&&labels.includes('Line segment type')&&!labels.includes('Start marker')&&!labels.includes('End marker'));
  move(140,320);await pause(100);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(80);move(270,360);await pause(200);
  const modifier=async(k,on)=>{keyboard.notify_keyval(GLib.get_monotonic_time(),k,on?Clutter.KeyState.PRESSED:Clutter.KeyState.RELEASED);await pause(200);};
  const snapped=()=>{const s=state(),p=(s.drawing||s.current).points;const dx=Math.abs(p[1][0]-p[0][0]),dy=Math.abs(p[1][1]-p[0][1]);return Math.min(dx,dy,Math.abs(dx-dy))<1e-6;};
  await modifier(Clutter.KEY_Shift_L,true);check('pressing Shift snaps a stationary drawing immediately to 45 degrees',snapped());
  await modifier(Clutter.KEY_Shift_R,true);await modifier(Clutter.KEY_Shift_L,false);check('releasing one Shift retains the other held Shift constraint',snapped());
  await modifier(Clutter.KEY_Shift_R,false);check('releasing Shift restores the raw drawing endpoint without pointer movement',!snapped());
  await modifier(Clutter.KEY_Shift_L,true);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(200);await modifier(Clutter.KEY_Shift_L,false);await wait(()=>state().marks.length===3);
  check('a committed Shift arrow has exactly two endpoint handles',snapped()&&state().handles.length===2&&state().handles.every(h=>h.kind==='endpoint'));
  const p=state().current.points[1];move(p[0]+100,p[1]+120);await pause(120);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(100);move(p[0]+135,p[1]+165);await pause(150);await modifier(Clutter.KEY_Shift_L,true);
  check('Shift applies the same angle constraint while adjusting an existing endpoint',snapped());pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(150);await modifier(Clutter.KEY_Shift_L,false);
  await option('Arrow type');await wait(()=>state().options.popup);
  check('native arrow popup exposes all eleven visual types',state().optionMenu.filter(v=>['Straight arrow','Straight bilateral arrow','Line arrow','Line bilateral arrow','Arrow outline','Arrow fill','Dimension line','Solid arrow','Solid bilateral arrow','Dimension arrow','Plain line'].includes(v.label)).length===11);
  await plain(Clutter.KEY_Escape);await wait(()=>!state().options.popup);
  const chooseArrow=async(label,value)=>{await option('Arrow type');await wait(()=>state().options.popup);await click(state().optionMenu.find(v=>v.label===label));await wait(()=>!state().options.popup&&state().current.arrowType===value);};
  for(const [label,value] of [['Straight arrow','straight'],['Straight bilateral arrow','straight-bilateral'],['Line arrow','line'],['Line bilateral arrow','line-bilateral'],['Arrow outline','outline'],['Arrow fill','filled'],['Dimension line','dimension-line'],['Solid arrow','solid'],['Solid bilateral arrow','solid-bilateral'],['Dimension arrow','dimension-arrow'],['Plain line','plain-line']]){
   await chooseArrow(label,value);check(`native selection edits the existing ${value} arrow`,state().marks.length===3&&state().current.arrowType===value);
  }
  for(const [label,value] of [['Solid line','solid'],['Dashed line','dash'],['Dotted line','dot'],['Dash-dot line','dash-dot'],['Dash-dot-dot line','dash-dot-dot']]){
   await option('Line segment type');await wait(()=>state().options.popup);await click(state().optionMenu.find(v=>v.label===label));await wait(()=>state().current.lineStyle===value);
   check(`plain line supports ${value} without changing its type`,state().current.arrowType==='plain-line');
  }
  await option('Line segment type');await wait(()=>state().options.popup);await click(state().optionMenu.find(v=>v.label==='Dashed line'));await wait(()=>state().current.lineStyle==='dash');
  await chooseArrow('Arrow fill','filled');check('filled tapered arrow disables its line selector',!state().options.controls.find(v=>v.label==='Line segment type').sensitive);
  await chooseArrow('Arrow outline','outline');check('switching back restores the remembered line type',state().options.controls.find(v=>v.label==='Line segment type').sensitive&&state().current.lineStyle==='dash');
  await button('undo');check('one Undo restores exactly the prior arrow type',state().current.arrowType==='filled'&&state().marks.length===3);await button('redo');
  await plain(Clutter.KEY_Delete);await wait(()=>state().marks.length===2);
  await click({x:520,y:290,width:1,height:1});
  check('first arrow click starts unpressed polyline rubber-band preview without committing yet',state().marks.length===2&&state().drawing?.kind==='arrow');
  await click({x:610,y:290,width:1,height:1});
  await click({x:610,y:360,width:1,height:1});
  move(630,370);await pause(80);pointer.notify_button(GLib.get_monotonic_time(),3,Clutter.ButtonState.PRESSED);await pause(60);pointer.notify_button(GLib.get_monotonic_time(),3,Clutter.ButtonState.RELEASED);
  await wait(()=>state().marks.length===3&&!state().drawing);
  check('right-click finishes a 3-vertex polyline arrow with per-vertex active handles',state().current?.kind==='arrow'&&state().current.points.length===3&&state().handles.length===3&&state().handles.every(h=>h.kind==='endpoint'));
  await key(Clutter.KEY_z);await wait(()=>state().marks.length===2);
  check('one Undo removes the entire multi-segment polyline arrow',state().marks.length===2&&state().canRedo);
  for(const [shapeLabel,fillName,start,end] of [['Rectangle','rect',[540,300],[650,350]],['Ellipse','ellipse',[650,410],[540,360]]]){
   await tool('Highlight (Ctrl+5)');await option(shapeLabel);
   move(...start);await pause(100);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(80);move(...end);await pause(150);
   const hSquare=()=>{const p=state().drawing.points;return Math.abs(Math.abs(p[1][0]-p[0][0])-Math.abs(p[1][1]-p[0][1]))<1e-6;};
   await modifier(Clutter.KEY_Shift_L,true);check(`stationary Shift constrains highlight ${fillName} to equal sides`,hSquare());
   await modifier(Clutter.KEY_Shift_L,false);check(`releasing Shift restores free highlight ${fillName} proportions`,!hSquare());
   await modifier(Clutter.KEY_Shift_L,true);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(150);await modifier(Clutter.KEY_Shift_L,false);await wait(()=>state().marks.length===3);
   {const b=state().current;check(`Shift commits one equal-sided highlight ${fillName}`,b.kind==='highlight'&&b.fill===fillName&&Math.abs(Math.abs(b.points[1][0]-b.points[0][0])-Math.abs(b.points[1][1]-b.points[0][1]))<1e-6);}
   await key(Clutter.KEY_z);await wait(()=>state().marks.length===2);
  }
  await option('Freehand highlight');
  for(const [keycode,kind,start,end] of [[Clutter.KEY_2,'rect',[540,300],[650,350]],[Clutter.KEY_3,'ellipse',[650,410],[540,360]]]){
   await legacyTool(keycode);move(...start);await pause(100);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(80);move(...end);await pause(150);
   const square=()=>{const p=state().drawing.points;return Math.abs(Math.abs(p[1][0]-p[0][0])-Math.abs(p[1][1]-p[0][1]))<1e-6;};
   await modifier(Clutter.KEY_Shift_L,true);check(`stationary Shift constrains the native ${kind} draft`,square());
   await modifier(Clutter.KEY_Shift_L,false);check(`releasing Shift restores free ${kind} proportions immediately`,!square());
   await modifier(Clutter.KEY_Shift_L,true);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(150);await modifier(Clutter.KEY_Shift_L,false);await wait(()=>state().marks.length===3);
   {const b=state().current;check(`Shift commits one equal-sided ${kind}`,b.kind===kind&&Math.abs(Math.abs(b.points[1][0]-b.points[0][0])-Math.abs(b.points[1][1]-b.points[0][1]))<1e-6);}
   await key(Clutter.KEY_z);await wait(()=>state().marks.length===2);check(`one Undo removes only the constrained ${kind}`,state().canRedo);
  }
  await legacyTool(Clutter.KEY_7);await pause(150);
  {const controls=state().options.controls;check('mosaic subbar starts with the common stroke width control followed by a separate brush',controls[0].label==='Stroke width (image pixels)'&&controls[1].label==='Freehand mosaic'&&!controls.some(c=>c.label==='Ellipse')&&state().options.height===32);}
  await option('Stroke width (image pixels)');await wait(()=>state().options.popup);check('mosaic stroke width opens its own popup without creating an annotation',state().marks.length===2);
  await plain(Clutter.KEY_Escape);await wait(()=>!state().options.popup);
  await option('Freehand mosaic');await drag([540,300],[650,350]);await wait(()=>state().marks.length===3);
  check('the independent mosaic brush creates freehand strokes with no resize handles',state().current.kind==='mosaic'&&state().current.fill==='stroke'&&state().current.width===12&&state().current.points.length>2&&state().handles.length===0);
  const wheel=async()=>{for(let i=0;i<3;i++){pointer.notify_discrete_scroll(GLib.get_monotonic_time(),Clutter.ScrollDirection.UP,Clutter.ScrollSource.WHEEL);await pause(80);}await pause(250);};
  const ink=JSON.stringify(state().current);
  move(600,325);await wheel();
  check('native freehand mosaic wheel preserves all committed ink properties',JSON.stringify(state().current)===ink);
  move(700,400);await pause(150);check('native freehand wheel updates the next stroke cursor width',state().toolCursor.radius===7.5);
  await key(Clutter.KEY_z);check('future width changes add no Undo entries',state().marks.length===2);
  await key(Clutter.KEY_y);check('Redo restores the original ink footprint',state().marks.length===3&&JSON.stringify(state().current)===ink);
  const strengthControl=()=>state().options.controls.find(c=>c.label==='Effect strength');
  const setStrength=async end=>{const r=strengthControl();await click({x:r.x+20,y:r.y+4,width:0,height:r.height-8});await plain(end?Clutter.KEY_End:Clutter.KEY_Home);await pause(200);};
  await setStrength(true);check('freehand pixelation slider remains enabled and reaches its unified maximum without changing width',state().current.effectLevel===24&&state().current.width===12&&strengthControl().value===24&&strengthControl().range[0]===3&&strengthControl().range[1]===24);
  await option('Mosaic effect');
  check('mosaic effect switches directly on click without opening a dropdown popup while keeping unified strength',!state().options.popup&&state().current.effect==='blur'&&state().current.effectLevel===24&&strengthControl().range[0]===3&&strengthControl().range[1]===24&&state().current.width===12);
  await setStrength(false);check('freehand blur accepts unified minimum strength while retaining its brush width',state().current.effectLevel===3&&state().current.width===12);
  await setStrength(true);check('freehand blur accepts unified maximum strength while retaining its brush width',state().current.effectLevel===24&&state().current.width===12);
  move(700,400);await pause(150);check('freehand mosaic uses a centered next-stroke dot instead of the system crosshair',state().cursor==='none'&&state().toolCursor.radius===7.5&&!state().toolCursor.crosshair);
  await setStrength(false);
  await option('Mosaic effect');
  check('returning to pixelation preserves strength and unified range without changing brush geometry',!state().options.popup&&state().current.effect==='pixelate'&&state().current.effectLevel===3&&state().current.fill==='stroke'&&strengthControl().range[0]===3&&strengthControl().range[1]===24);
  const freehand=JSON.stringify(state().marks[2]);
  await option('Rectangle');
  check('switching to rectangle mosaic never converts an existing brush stroke',state().current===null&&JSON.stringify(state().marks[2])===freehand);
  await drag([540,360],[650,410]);await wait(()=>state().marks.length===4);
  const rectangle=state().current,initialLevel=rectangle.effectLevel;
  move(600,385);await wheel();
  check('native rectangular mosaic wheel changes effect strength and synchronizes its slider',state().current.effectLevel===initialLevel+3&&state().current.width===rectangle.width&&state().options.controls.find(c=>c.label==='Effect strength').value===initialLevel+3);
  await key(Clutter.KEY_z);check('one Undo restores rectangular effect and slider together',state().marks.length===4&&state().current.effectLevel===initialLevel&&state().options.controls.find(c=>c.label==='Effect strength').value===initialLevel);
  await key(Clutter.KEY_y);check('Redo restores the final rectangular wheel strength',state().current.effectLevel===initialLevel+3);
  await key(Clutter.KEY_z);
  {const r=strengthControl();move(r.x+r.width/2,r.y+r.height/2);await pause(120);await wheel();}
  check('scrolling mouse wheel over the Effect strength slider steps by 1 per notch rather than 10',state().current.effectLevel===initialLevel+3&&strengthControl().value===initialLevel+3);
  await key(Clutter.KEY_z);check('slider wheel steps coalesce into one Undo entry',state().marks.length===4&&state().current.effectLevel===initialLevel&&strengthControl().value===initialLevel);
  const marks=JSON.stringify(state().marks);
  await option('Freehand mosaic');
  check('switching back to brush never converts an existing rectangular mosaic',state().current===null&&JSON.stringify(state().marks)===marks);
  await legacyTool(Clutter.KEY_1);await drag([600,325],[620,335]);
  check('completed mosaic brush strokes cannot be repositioned with the selection tool',JSON.stringify(state().marks)===marks&&state().current===null);
  for(let i=0;i<20&&state().marks.length>2;i++)await key(Clutter.KEY_z);
  await wait(()=>state().marks.length===2);
  for(const [code,label] of [[Clutter.KEY_5,'pen'],[Clutter.KEY_6,'highlight'],[Clutter.KEY_9,'eraser'],[Clutter.KEY_4,'arrow']]){
   await legacyTool(code);move(700,400);await pause(150);
   check(`${label} drawing has the shared dot cursor and correct crosshair mode`,state().cursor==='none'&&state().toolCursor!==null&&state().toolCursor.crosshair===(label==='arrow'));
  }
  for(const [code,freehandLabel,shapeLabels,toolName] of [
   [Clutter.KEY_6,'Freehand highlight',['Line highlight','Rectangle','Ellipse'],'highlight'],
   [Clutter.KEY_9,'Freehand eraser',['Area eraser'],'eraser'],
  ]){
   await legacyTool(code);await option(freehandLabel);
   await drag([520,280],[630,335]);await wait(()=>state().marks.length===3);
   const strokeJson=JSON.stringify(state().marks[2]);
   for(const shapeLabel of shapeLabels){
    await option(shapeLabel);
    check(`${toolName} switching to ${shapeLabel} never converts an existing freehand stroke`,state().current===null&&JSON.stringify(state().marks[2])===strokeJson);
   }
   await drag([520,345],[630,395]);await wait(()=>state().marks.length===4);
   const shapeJson=JSON.stringify(state().marks[3]);
   await option(freehandLabel);
   check(`${toolName} switching back to freehand never converts an existing shape`,state().current===null&&JSON.stringify(state().marks[3])===shapeJson);
   await key(Clutter.KEY_z);await key(Clutter.KEY_z);await wait(()=>state().marks.length===2);
  }
  await legacyTool(Clutter.KEY_5);
  await drag([520,280],[620,330]);await wait(()=>state().marks.length===3);
  await legacyTool(Clutter.KEY_6);await option('Freehand highlight');
  await drag([520,340],[620,390]);await wait(()=>state().marks.length===4);
  const fixedFreehandMarks=JSON.stringify(state().marks);
  await legacyTool(Clutter.KEY_1);
  await drag([570,305],[610,325]);
  check('committed pen strokes cannot be dragged with the selection tool',JSON.stringify(state().marks)===fixedFreehandMarks&&state().current===null);
  await drag([570,365],[610,385]);
  check('committed freehand highlight strokes cannot be dragged with the selection tool',JSON.stringify(state().marks)===fixedFreehandMarks&&state().current===null);
  await key(Clutter.KEY_z);await key(Clutter.KEY_z);await wait(()=>state().marks.length===2);
  await tool('Text (Ctrl+8)');move(700,400);await pause(150);
  check('activating Text gives insertion areas the native text cursor',state().cursor==='text'&&state().toolCursor===null);
  await click({x:210,y:350,width:0,height:0});await wait(()=>state().textEditing);
  for(const ch of 'Clivuu')await plain(ch.codePointAt(0));await wait(()=>state().current?.text==='Clivuu');
  check('real native TextView inputs into the shared text draft',state().inline.focused&&state().current.textSize[0]>0);
  await option('Font family');await wait(()=>state().options.popup);
  await modifier(Clutter.KEY_Control_L,true);await modifier(Clutter.KEY_Shift_L,true);await plain(Clutter.KEY_u);await modifier(Clutter.KEY_Shift_L,false);await modifier(Clutter.KEY_Control_L,false);
  await plain(Clutter.KEY_4);await plain(Clutter.KEY_e);await pause(120);
  await plain(Clutter.KEY_Escape);await pause(180);
  check('Escape during uncommitted IME preedit clears preedit while keeping font search open',state().options.popup&&state().focusWidget==='GtkText');
  for(let tab=0;tab<5&&state().focusWidget!=='GtkListBoxRow';tab++){await plain(Clutter.KEY_Tab);await pause(150);}
  check('font popup allows native Tab navigation into its list',state().focusWidget==='GtkListBoxRow');
  const popupBefore=JSON.stringify({marks:state().marks,current:state().current});
  await plain(Clutter.KEY_5);await key(Clutter.KEY_z);await key(Clutter.KEY_s);
  check('font list focus does not leak editor tool, Undo or output shortcuts',state().options.popup&&JSON.stringify({marks:state().marks,current:state().current})===popupBefore);
  await plain(Clutter.KEY_Escape);await wait(()=>!state().options.popup&&state().inline.focused);
  check('Escape dismisses font search and returns input to the same text document',!state().options.popup&&state().inline.focused);
  await plain(Clutter.KEY_Escape);await wait(()=>!state().textEditing);check('Escape commits native text once without losing annotations',!state().textEditing&&state().marks.length===3&&state().marks[2].text==='Clivuu');
  await button('undo');check('undo of placed native text preserves both earlier shapes',state().marks.length===2);

  child.force_exit();
  const captureBinary=GLib.build_filenamev([GLib.path_get_dirname(GLib.getenv('CLIVUU_RUST_EDITOR_PROBE')),'capture_probe']);
  launcher.set_stderr_file_path(`${directory}/rust-capture.log`);child=launcher.spawnv([captureBinary]);
  await wait(()=>GLib.file_test(`${directory}/rust-capture-ready`,GLib.FileTest.EXISTS));
  let captureWindow;await wait(()=>captureWindow=global.get_window_actors().map(a=>a.meta_window).find(w=>w.get_title()==='Clivuu Rust capture probe'));captureWindow.activate(global.get_current_time());await pause(300);
  const capture=()=>read('rust-capture-state.json');captureState=capture;
  move(750,650);await pause(120);move(300,200);await wait(()=>capture().pointer?.[0]===300);
  check('native capture starts with a magnifier and detects the hovered window in place',capture().lens&&capture().rect.x===100&&capture().rect.width===600&&!capture().selected);
  await drag([920,480],[520,140]);await wait(()=>capture().selected);
  check('capture drag selects the original desktop coordinates and keeps the lens inside the selection',capture().rect.x===520&&capture().rect.y===140&&capture().rect.width===400&&capture().rect.height===340&&capture().lens&&capture().toolbar);
  move(720,310);await wait(()=>capture().pointer?.[0]===720&&capture().lens);
  check('stationary pointer inside selected region shows the magnifier in Select mode',capture().lens);
  move(720,310);await pause(80);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(80);move(740,325);await wait(()=>capture().drag?.kind==='move');
  check('moving the selection hides both magnifier and stationary key tips during drag',!capture().lens&&!capture().keyTips.visible);
  move(720,310);await pause(80);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await wait(()=>capture().drag===null&&capture().lens&&capture().keyTips.visible);
  move(200,150);await wait(()=>capture().pointer?.[0]===200&&!capture().lens);
  await modifier(Clutter.KEY_Alt_L,true);await wait(()=>capture().lens);await plain(Clutter.KEY_c);
  check('Alt+C is consumed by the temporary capture magnifier',capture().lens);
  await modifier(Clutter.KEY_Alt_L,false);await wait(()=>!capture().lens);
  check('Alt toggles selected edge and center guides persistently after the magnifier closes',capture().guides&&!capture().crosshair);
  const tipsBox=capture().keyTips.rect;
  check('capture key help is a separate lower-left panel with stationary history tips and chord/alternative connectors',capture().keyTips.visible&&tipsBox.x===12&&tipsBox.y+tipsBox.height<=800&&capture().keyTips.labels.includes('Move selection by one pixel')&&capture().keyTips.labels.includes('Use last successful capture region')&&capture().keyTips.labels.includes('Previous / next region')&&capture().keyTips.labels.includes('+')&&capture().keyTips.labels.includes('/'));
  const feedbackStream=Gio.File.new_for_path(`${directory}/capture-guides-tips.png`).replace(null,false,Gio.FileCreateFlags.PRIVATE,null);
  await new Shell.Screenshot().screenshot_area(0,0,1280,800,feedbackStream);feedbackStream.close(null);
  await plain(Clutter.KEY_space);await wait(()=>!capture().toolbar);
  await plain(Clutter.KEY_space);await wait(()=>capture().toolbar);
  check('Space hides and restores capture controls without changing the selection',capture().rect.width===400&&capture().rect.height===340);
  for(const [x,y,name] of [[720,70,'ns-resize'],[720,550,'ns-resize'],[440,310,'ew-resize'],[1000,310,'ew-resize'],[440,70,'nwse-resize'],[1000,70,'nesw-resize'],[440,550,'nesw-resize'],[1000,550,'nwse-resize']]){
   move(x,y);await wait(()=>capture().pointer?.[0]===x&&capture().pointer?.[1]===y);
   check(`native capture outside (${x},${y}) uses ${name} without changing the crop`,capture().cursor===name&&capture().rect.x===520&&capture().rect.y===140&&capture().rect.width===400&&capture().rect.height===340&&!capture().lens);
  }
  move(920,480);await pause(100);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(80);move(1100,700);await wait(()=>capture().drag?.kind==='resize'&&capture().lens&&capture().pointer?.[0]===1100);
  check('native border resize activates the magnifier while hiding stationary key tips and preserving its included-edge pixel',!capture().keyTips.visible&&capture().pixel[0]===1099&&capture().pixel[1]===699);
  move(1279,799);await pause(150);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await wait(()=>capture().drag===null&&capture().lens&&capture().toolbar&&capture().keyTips.visible);
  check('native capture resize reaches the exclusive screen boundary and keeps the in-selection lens',capture().rect.x+capture().rect.width===1280&&capture().rect.y+capture().rect.height===800&&capture().lens);
  const tb=capture().panels.toolbar;move(tb.x+16,tb.y+16);await wait(()=>!capture().lens);
  check('hovering the floating toolbar inside a selection hides the magnifier',!capture().lens);
  for(const toolLabel of ['Shapes (Ctrl+2)','Arrow (Ctrl+3)','Pen (Ctrl+4)','Highlight (Ctrl+5)','Sequence (Ctrl+6)','Mosaic (Ctrl+7)','Text (Ctrl+8)','Eraser (E)','Shapes (Ctrl+2)','Eraser (E)','Text (Ctrl+8)','Eraser (E)','Sequence (Ctrl+6)']){
   await click(capture().tools.find(b=>b.label===toolLabel));
   const opt=capture().options;
   check(`clicking ${toolLabel} immediately allocates full subbar width without hover or horizontal scrollbar`,opt.visible&&opt.bounds[3]===32&&opt.allFit&&!opt.overflow&&opt.hscrollPolicy==='Never');
  }
  for(const [toolLabel,startPt,midPt] of [
   ['Shapes (Ctrl+2)',[560,305],[620,340]],
   ['Pen (Ctrl+4)',[570,315],[630,350]],
   ['Mosaic (Ctrl+7)',[580,325],[640,360]],
   ['Eraser (E)',[590,335],[650,370]],
  ]){
   await click(capture().tools.find(b=>b.label===toolLabel));
   move(...startPt);await pause(80);
   pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(120);
   move(...midPt);await pause(120);
   check(`pressing and dragging ${toolLabel} keeps both main toolbar and subbar visible`,capture().toolbar&&capture().options.visible);
   pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(150);
  }
  while(capture().marks.length>0){await key(Clutter.KEY_z);}
  await legacyTool(Clutter.KEY_2);await wait(()=>capture().tool==='rect');
  await drag([540,300],[620,350]);await wait(()=>capture().marks.length===1);
  await drag([650,350],[750,430]);await wait(()=>capture().marks.length===2);
  check('capture uses the same native annotation controller for two independent shapes',capture().marks.length===2&&capture().undo.sensitive);
  await key(Clutter.KEY_z);await wait(()=>capture().marks.length===1);
  check('capture Ctrl+Z removes one annotation and enables the common Redo action',capture().marks.length===1&&capture().redo.sensitive);
  const r=capture().undo;await click(r);await wait(()=>capture().marks.length===0);
  check('one common toolbar Undo click removes the remaining capture annotation',!capture().undo.sensitive&&capture().redo.sensitive);
  await legacyTool(Clutter.KEY_4);await wait(()=>capture().tool==='arrow');
  await modifier(Clutter.KEY_Shift_L,true);await drag([540,320],[670,380]);await modifier(Clutter.KEY_Shift_L,false);await wait(()=>capture().marks.length===1);
  {const p=capture().marks[0].points,dx=Math.abs(p[1][0]-p[0][0]),dy=Math.abs(p[1][1]-p[0][1]);check('capture uses the same Shift 45-degree arrow drawing rule',Math.min(dx,dy,Math.abs(dx-dy))<1e-6);}
  await key(Clutter.KEY_z);await wait(()=>capture().marks.length===0);check('one Undo removes the entire constrained capture arrow',!capture().undo.sensitive);
  for(const [keycode,kind] of [[Clutter.KEY_2,'rect'],[Clutter.KEY_3,'ellipse']]){
   await legacyTool(keycode);await modifier(Clutter.KEY_Shift_L,true);await drag([750,350],[820,390]);await modifier(Clutter.KEY_Shift_L,false);await wait(()=>capture().marks.length===1);
   {const m=capture().marks[0],p=m.points;check(`capture shares the Shift ${kind} constraint`,m.kind===kind&&Math.abs(Math.abs(p[1][0]-p[0][0])-Math.abs(p[1][1]-p[0][1]))<1e-6);}
   await key(Clutter.KEY_z);await wait(()=>capture().marks.length===0);
  }
  const initialArea=JSON.stringify(capture().rect);
  for(const [code,kind] of [[Clutter.KEY_2,'rect'],[Clutter.KEY_3,'ellipse'],[Clutter.KEY_4,'arrow'],[Clutter.KEY_5,'pen'],[Clutter.KEY_6,'highlight'],[Clutter.KEY_7,'mosaic'],[Clutter.KEY_9,'eraser']]){
   await legacyTool(code);await wait(()=>capture().tool===kind);await drag([100,260],[200,320]);await wait(()=>capture().marks.length===1);
   const mark=capture().marks[0];
   check(`capture ${kind} draws outside the crop without clamping or moving it`,mark.points[0][0]===100&&mark.points[0][1]===260&&mark.points.at(-1)[0]===200&&mark.points.at(-1)[1]===320&&JSON.stringify(capture().rect)===initialArea&&!capture().lens);
   await key(Clutter.KEY_z);await wait(()=>capture().marks.length===0);
  }
  await legacyTool(Clutter.KEY_8);await click({x:150,y:280,width:0,height:0});
  for(const ch of 'Outside')await plain(ch.codePointAt(0));
  await plain(Clutter.KEY_Escape);await wait(()=>capture().marks.length===1);
  check('native capture text can be placed outside the crop',capture().marks[0].kind==='text'&&capture().marks[0].points[0][0]===150&&capture().marks[0].text==='Outside'&&JSON.stringify(capture().rect)===initialArea);
  await key(Clutter.KEY_z);await wait(()=>capture().marks.length===0);
  await legacyTool(Clutter.KEY_2);await drag([100,260],[200,320]);await wait(()=>capture().marks.length===1);
  const outsideInk=JSON.stringify(capture().marks);
  await legacyTool(Clutter.KEY_1);await wait(()=>capture().tool==='select');
  move(200,500);await pause(100);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);
  await wait(()=>capture().drag?.kind==='expand');
  check('outside press immediately expands the facing edge without a magnifier',capture().rect.x===200&&!capture().lens);
  move(150,500);await wait(()=>capture().rect.x===150);move(250,500);await wait(()=>capture().rect.x===250);
  pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await wait(()=>capture().drag===null);
  await click({x:50,y:100,width:0,height:0});await wait(()=>capture().rect.x===50&&capture().rect.y===100);
  check('outside corner click includes existing external ink without modifying it',JSON.stringify(capture().marks)===outsideInk&&capture().lens&&capture().rect.x+capture().rect.width===1280&&capture().rect.y+capture().rect.height===800&&!capture().keyTips.visible);
  await key(Clutter.KEY_z);await wait(()=>capture().marks.length===0);
  check('expanding the crop adds no annotation Undo step',capture().rect.x===50&&capture().rect.y===100);
  await drag([50,400],[520,400]);await wait(()=>capture().rect.x>=515&&capture().keyTips.visible);
  await legacyTool(Clutter.KEY_1);await wait(()=>capture().tool==='select');
  const help=capture().keyTips.rect,helpX=Math.round(help.x+8),helpY=Math.round(help.y+8);
  await click({x:helpX,y:helpY,width:0,height:0});await wait(()=>capture().rect.x===helpX&&!capture().keyTips.visible);
  check('capture shortcut help passes mouse input through to the selection beneath it',capture().selected&&capture().rect.x===helpX&&!capture().keyTips.visible);
  // Active annotation tool hovering the top-left size indicator hides it so strokes are unobstructed.
  await drag([helpX,400],[360,400]);await wait(()=>capture().rect.x>=355&&capture().hintVisible);
  await legacyTool(Clutter.KEY_2);await wait(()=>capture().tool==='rect');
  const hintBox=capture().panels.hint;
  move(Math.round(hintBox.x+hintBox.width/2),Math.round(hintBox.y+hintBox.height/2));
  await wait(()=>!capture().hintVisible);
  move(Math.round(hintBox.x+hintBox.width+40),Math.round(hintBox.y+hintBox.height+40));
  await wait(()=>capture().hintVisible);
  check('hovering the top-left pixel indicator while annotating hides and restores it',capture().hintVisible);
  // When bottom space only fits the main toolbar, opening a subbar moves both Above instead of overlapping the selection.
  await legacyTool(Clutter.KEY_1);await wait(()=>capture().tool==='select');
  await drag([600,799],[600,745]);await wait(()=>capture().rect.y+capture().rect.height<=750);
  await drag([600,100],[600,280]);await wait(()=>capture().rect.y>=275);
  await legacyTool(Clutter.KEY_2);await wait(()=>capture().tool==='rect'&&capture().options.visible);
  const tb2=capture().panels.toolbar,opt2=capture().options.bounds,sel2=capture().rect;
  check('expanding subbar near bottom moves both toolbar and subbar above selection without overlap',capture().panels.placement==='above'&&tb2.y+tb2.height<=sel2.y&&opt2[1]+opt2[3]<=tb2.y&&opt2[1]>=1);
  await legacyTool(Clutter.KEY_1);await wait(()=>capture().tool==='select');
  await drag([600,280],[600,100]);await wait(()=>capture().rect.y<=105);
  await drag([600,745],[600,799]);await wait(()=>capture().rect.y+capture().rect.height>=795);
  await pause(600);
  await click({x:750,y:500,width:0,height:0});await click({x:750,y:500,width:0,height:0});await wait(()=>capture().copies===1);
  check('Select double-click uses the capture completion command',capture().copies===1);
  // Regression: after using Eraser, dragging the selection border across other
  // annotations must keep them visible both outside and back inside the crop.
  {
   await legacyTool(Clutter.KEY_2);await wait(()=>capture().tool==='rect');
   await drag([360,200],[460,300]);await wait(()=>capture().marks.length===1);
   await legacyTool(Clutter.KEY_5);await wait(()=>capture().tool==='pen');
   await drag([520,200],[620,300]);await wait(()=>capture().marks.length===2);
   await legacyTool(Clutter.KEY_2);await wait(()=>capture().tool==='rect');
   await drag([680,200],[780,300]);await wait(()=>capture().marks.length===3);
   await legacyTool(Clutter.KEY_9);await wait(()=>capture().tool==='eraser');
   await drag([540,240],[600,260]);await wait(()=>capture().marks.length===4);
   await legacyTool(Clutter.KEY_1);await wait(()=>capture().tool==='select');
   const countRectInk=(shot,x0,y0,x1,y1,outside)=>{
    const px=shot.get_pixels(),rs=shot.rowstride,nc=shot.n_channels,f=outside?0.56:1.0;
    let n=0;
    for(let y=y0;y<y1;y++)for(let x=x0;x<x1;x++){
     const o=y*rs+x*nc;
     if(Math.abs(px[o]-Math.floor(x/5)*f)+Math.abs(px[o+1]-Math.floor(y/4)*f)+Math.abs(px[o+2]-72*f)>35)n++;
    }
    return n;
   };
   const grabShot=async name=>{
    const p=`${directory}/${name}.png`,s=Gio.File.new_for_path(p).replace(null,false,Gio.FileCreateFlags.PRIVATE,null);
    try{await new Shell.Screenshot().screenshot_area(0,0,1280,800,s);}finally{s.close(null);}
    return GdkPixbuf.Pixbuf.new_from_file(p);
   };
   await drag([600,100],[600,400]);await wait(()=>capture().rect.y>=395);
   const outShot=await grabShot('eraser-drag-outside');
   const outLeft=countRectInk(outShot,350,190,470,310,true),outRight=countRectInk(outShot,670,190,790,310,true);
   await drag([600,capture().rect.y],[600,100]);await wait(()=>capture().rect.y<=105);
   const inShot=await grabShot('eraser-drag-inside');
   const inLeft=countRectInk(inShot,350,190,470,310,false),inRight=countRectInk(inShot,670,190,790,310,false);
   check('other annotations remain visible outside and inside the selection when dragged after using Eraser',outLeft>250&&outRight>250&&inLeft>250&&inRight>250);
   while(capture().marks.length>0){await key(Clutter.KEY_z);}
   await wait(()=>capture().marks.length===0);
   await legacyTool(Clutter.KEY_5);await wait(()=>capture().tool==='pen');
   await click(capture().options.controls.find(c=>c.label==='#ff0000'));
   await drag([480,250],[660,250]);await wait(()=>capture().marks.length===1);
   await legacyTool(Clutter.KEY_6);await wait(()=>capture().tool==='highlight');
   await click(capture().options.controls.find(c=>c.label==='Freehand highlight'));
   await click(capture().options.controls.find(c=>c.label==='#4caf50'));
   await drag([570,190],[570,310]);await wait(()=>capture().marks.length===2);
   await legacyTool(Clutter.KEY_9);await wait(()=>capture().tool==='eraser');
   await drag([505,235],[525,265]);await wait(()=>capture().marks.length===3);
   await legacyTool(Clutter.KEY_1);await wait(()=>capture().tool==='select');
   const sample=(shot,x,y)=>{const p=shot.get_pixels(),o=y*shot.rowstride+x*shot.n_channels;return [p[o],p[o+1],p[o+2]];};
   const hasBlend=(cross,penOnly)=>Math.hypot(cross[0]-penOnly[0],cross[1]-penOnly[1],cross[2]-penOnly[2])>12;
   const inPh=await grabShot('pen-hl-inside');
   await drag([600,100],[600,360]);await wait(()=>capture().rect.y>=355);
   const outPh=await grabShot('pen-hl-outside');
   await drag([600,capture().rect.y],[600,100]);await wait(()=>capture().rect.y<=105);
   const backPh=await grabShot('pen-hl-back');
   check('pen + highlight + eraser preserves relative z-order across dragged selection boundaries',hasBlend(sample(inPh,570,250),sample(inPh,620,250))&&hasBlend(sample(outPh,570,250),sample(outPh,620,250))&&hasBlend(sample(backPh,570,250),sample(backPh,620,250)));
   while(capture().marks.length>0){await key(Clutter.KEY_z);}
   await wait(()=>capture().marks.length===0);
  }
  await legacyTool(Clutter.KEY_2);await wait(()=>capture().tool==='rect');await drag([400,300],[500,350]);await wait(()=>capture().marks.length===1);
  const right=async()=>{move(750,500);await pause(100);pointer.notify_button(GLib.get_monotonic_time(),3,Clutter.ButtonState.PRESSED);await pause(60);pointer.notify_button(GLib.get_monotonic_time(),3,Clutter.ButtonState.RELEASED);await pause(200);};
  await right();await wait(()=>capture().current===null);check('first capture right-click deselects the annotation without clearing it',capture().marks.length===1&&capture().selected);
  check('key tips update the next right-click behavior after deselecting an annotation',capture().keyTips.labels.includes('Reset region / cancel capture')&&!capture().keyTips.labels.includes('Deselect annotation'));
  await legacyTool(Clutter.KEY_8);await click({x:900,y:400,width:0,height:0});await wait(()=>capture().current?.kind==='text');
  check('empty text draft does not advertise annotation deselection after its pending commit',capture().marks.length===1&&capture().keyTips.labels.includes('Reset region / cancel capture')&&!capture().keyTips.labels.includes('Deselect annotation'));
  await legacyTool(Clutter.KEY_2);await wait(()=>capture().tool==='rect'&&capture().current===null);
  await right();await wait(()=>!capture().selected);check('second capture right-click clears the selection and annotations',capture().marks.length===0&&!capture().selected);
  let exited=false;child.wait_async(null,(process,result)=>{process.wait_finish(result);exited=true;});
  await right();await wait(()=>exited);check('third capture right-click exits after clearing the selection',exited);
  GLib.file_set_contents(`${directory}/rust-capture-stop`,'stop');

 }catch(e){
  GLib.file_set_contents(`${directory}/rust-shell-geometry.json`,JSON.stringify(global.get_window_actors().map(a=>({title:a.meta_window.get_title(),frame:a.meta_window.get_frame_rect()}))));
  const stream=Gio.File.new_for_path(`${directory}/rust-failure.png`).replace(null,false,Gio.FileCreateFlags.PRIVATE,null);await new Shell.Screenshot().screenshot_area(0,0,1280,800,stream);stream.close(null);
  results.push({name:'Rust editor probe',passed:false,error:`${e.message}\n${e.stack}`});}
 finally{GLib.file_set_contents(`${directory}/rust-editor-stop`,'stop');child?.force_exit();GLib.file_set_contents(`${directory}/results.json`,JSON.stringify(results));}
}

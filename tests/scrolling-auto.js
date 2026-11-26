// Real GNOME portal authorization and input regression, isolated by gnome.py.
import GLib from 'gi://GLib';
import Clutter from 'gi://Clutter';
import {pause} from './common/process.js';
export async function verifyAutomatic({execute,current,action,move,key,click,wait,check}) {
   // Prior exclusion checks use a solid-color oracle. Automatic stitching needs
   // actual moving text, rather than mistaking scrollbar animation for content.
   const directory=GLib.getenv('CLIVUU_TEST_DIR'),color=`${directory}/fixture-color`;
   if(GLib.file_test(color,GLib.FileTest.EXISTS))GLib.unlink(color);
   GLib.file_set_contents(`${directory}/fixture-scroll-request`,'0');
   await pause(300);
   const automaticCapture=execute({command:'scroll',rect:{x:70,y:150,width:850,height:470}});
   await wait(async()=> (await current())?.frames>=1);await action('Auto-scroll');
   let dialog;await wait(()=>dialog=global.get_window_actors().map(a=>a.meta_window).find(w=>w.get_title()==='Remote Desktop'));
   await wait(()=>global.display.focus_window===dialog);await pause(350);
   // The isolated GNOME portal initially focuses its remote-interaction switch.
   await key(Clutter.KEY_space);const d=dialog.get_frame_rect();
   await click({x:d.x+d.width-90,y:d.y+4,width:80,height:36});
   await wait(async()=> (await current())?.automatic);
   move(500,400);await pause(800);
   GLib.file_set_contents(`${GLib.getenv('CLIVUU_TEST_DIR')}/auto-position.json`,JSON.stringify({pointer:global.get_pointer(),state:await current()}));
   await wait(async()=>{const s=await current();return !s.autoWaiting&&s.frames>=2;});
   check('authorized automatic scrolling appends real scrolled page content',true);
   move(560,430);await pause(700);
   check('automatic scrolling never warps a freely moved pointer',global.get_pointer().slice(0,2).join(',')==='560,430');
   const beforeDrag=await current(),grip=beforeDrag.buttons.find(b=>b.label==='Move toolbar').rect;
   const pointer=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
   const x=grip.x+grip.width/2,y=grip.y+grip.height/2;
   move(x,y);await pause(150);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(100);
   for(let n=1;n<=8;n++){move(x-40*n/8,y-35*n/8);await pause(35);}
   pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(350);
   check('toolbar dragging keeps automatic capture enabled and follows the pointer', (await current()).automatic&&Math.abs((await current()).toolbar.x-beforeDrag.toolbar.x+40)<2&&Math.abs((await current()).toolbar.y-beforeDrag.toolbar.y+35)<2);
   const afterDrag=(await current()).height;move(560,430);
   await wait(async()=>{const s=await current();return s.automatic&&!s.autoWaiting&&s.height>afterDrag;});
   check('automatic capture resumes real scrolling after toolbar reader refresh',true);
   move(30,130);await wait(async()=> (await current()).autoWaiting);
   // A scroll issued before leaving may still commit its frame; only new
   // injection must stop immediately, not the pending image acquisition.
   await pause(700);const stopped=await current();await pause(700);
   check('moving outside the capture pauses injection without reclaiming the pointer',(await current()).frames===stopped.frames&&global.get_pointer().slice(0,2).join(',')==='30,130');
   await action('Auto-scroll');await wait(async()=>!(await current()).automatic);
   move(500,400);await pause(750);
   check('manually stopping auto-scroll keeps tracking cleanly without mismatch error',!(await current()).message&&(await current()).tracking);
   await action('Trim scrolling capture');await wait(async()=>(await current()).popupOpen);
   await action('Auto-crop on reverse scroll');await wait(async()=>(await current()).autoCrop===true);
   await key(Clutter.KEY_Escape);await wait(async()=>!(await current()).popupOpen);
   const stitchedBeforeJump=(await current()).height;
   GLib.file_set_contents(`${directory}/fixture-scroll-request`,'1600');
   await wait(async()=>!!(await current()).message);
   await action('Reset scrolling capture');await wait(async()=>(await current()).popupOpen);
   await action('Continue from current view');
   await wait(async()=>!(await current()).popupOpen&&!(await current()).message&&(await current()).tracking);
   check('continue from current view clears mismatch while keeping committed height',(await current()).height>=stitchedBeforeJump);
   await action('Reset scrolling capture');await wait(async()=>(await current()).popupOpen);
   await action('Restart in current region');
   await wait(async()=>!(await current()).popupOpen&&(await current()).frames===1&&(await current()).height===470&&!(await current()).message);
   check('restart in current region resets to one clean initial frame without leaving capture',true);
   await action('Trim scrolling capture');await wait(async()=>(await current()).popupOpen);
   await action('Auto-crop on reverse scroll');await wait(async()=>(await current()).autoCrop===false);
   await key(Clutter.KEY_Escape);await wait(async()=>!(await current()).popupOpen);
   await action('Cancel');await automaticCapture;

   // Small selection height + reaching the bottom of the scroll container:
   // auto-scroll must advance proportionally without overshooting and exit
   // automatic mode cleanly at the bottom without triggering overlap errors.
   const totalScroll=JSON.parse(new TextDecoder().decode(GLib.file_get_contents(`${directory}/fixture-scroll.json`)[1]));
   void totalScroll;
   GLib.file_set_contents(`${directory}/fixture-scroll-request`,'bottom');await pause(300);
   const bottomVal=JSON.parse(new TextDecoder().decode(GLib.file_get_contents(`${directory}/fixture-scroll.json`)[1])).value;
   GLib.file_set_contents(`${directory}/fixture-scroll-request`,String(Math.max(0,Math.round(bottomVal-52))));
   await pause(350);
   const smallAuto=execute({command:'scroll',rect:{x:70,y:150,width:850,height:84}});
   await wait(async()=>(await current())?.frames>=1);await action('Auto-scroll');
   let dialog2;await wait(()=>dialog2=global.get_window_actors().map(a=>a.meta_window).find(w=>w.get_title()==='Remote Desktop'));
   await wait(()=>global.display.focus_window===dialog2);await pause(350);
   await key(Clutter.KEY_space);const d2=dialog2.get_frame_rect();
   await click({x:d2.x+d2.width-90,y:d2.y+4,width:80,height:36});
   await wait(async()=>(await current())?.automatic);
   move(500,192);
   await wait(async()=>{const s=await current();return s&&s.height>84;});
   await wait(async()=>{const s=await current();return s&&!s.automatic;});
   await pause(600);
   const settledBottom=await current();
   check('small-height auto-scroll reaches bottom and exits cleanly without mismatch error',settledBottom.height>=130&&!settledBottom.message&&settledBottom.tracking);
   await action('Cancel');await smallAuto;
}

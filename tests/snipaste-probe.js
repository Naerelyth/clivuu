import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {pause,run} from './common/process.js';

export async function probe(extension) {
    const directory = GLib.getenv('CLIVUU_TEST_DIR');
    const results = [];
    let reference, fixture;
    try {
        fixture = Gio.Subprocess.new(['gjs', '-m', `${directory}/fixture.js`], Gio.SubprocessFlags.NONE);
        await pause(2000);
        Main.overview.hide();
        await pause(1000);
        const display = GLib.getenv('DISPLAY');
        const authority = GLib.getenv('XAUTHORITY');
        GLib.file_set_contents(`${directory}/display.json`, JSON.stringify({display, authority}));
        const prefix = ['bwrap', '--die-with-parent', '--unshare-pid', '--unshare-ipc', '--unshare-net',
            '--ro-bind', '/', '/', '--proc', '/proc', '--dev-bind', '/dev', '/dev',
            '--bind', `${directory}/private-tmp`, '/tmp',
            '--bind', directory, directory,
            '--ro-bind', '/tmp/clivuu-reference/squashfs-root', '/tmp/clivuu-reference/squashfs-root',
            '--setenv', 'CLIVUU_REFERENCE_APP', '/tmp/clivuu-reference/squashfs-root'];
        if (display) {
            const socket = `/tmp/.X11-unix/X${display.replace(':', '').split('.')[0]}`;
            prefix.push('--bind', socket, socket);
        }
        if (authority)
            prefix.push('--ro-bind', authority, authority);
        const bus = GLib.getenv('DBUS_SESSION_BUS_ADDRESS');
        const busPath = bus.match(/path=([^,;]+)/)?.[1];
        if (busPath)
            prefix.push('--bind', busPath, busPath);
        reference = Gio.Subprocess.new([...prefix, '/bin/sh', `${directory}/reference-run.sh`], Gio.SubprocessFlags.NONE);
        for (let i = 0; i < 300 && !GLib.file_test(`${directory}/reference-ready`, GLib.FileTest.EXISTS); i++)
            await pause(100);
        await pause(1500);
        const windows = global.get_window_actors().map(a => a.meta_window);
        GLib.file_set_contents(`${directory}/reference-windows.json`, JSON.stringify(windows.map(w => ({title: w.get_title(),
            wmclass: w.get_wm_class(), type: w.get_window_type(), rect: w.get_frame_rect()}))));
        const candidate = windows.find(w => w.get_wm_class()?.toLowerCase().includes('snipaste') && w.get_frame_rect().width < 600);
        const pointer = Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
        const click = async (x, y) => {
            pointer.notify_absolute_motion(GLib.get_monotonic_time(), x, y);
            await pause(100);
            pointer.notify_button(GLib.get_monotonic_time(), 1, Clutter.ButtonState.PRESSED);
            pointer.notify_button(GLib.get_monotonic_time(), 1, Clutter.ButtonState.RELEASED);
            await pause(350);
        };
        // Snipaste uses X11 override-redirect surfaces; they may be painted
        // without appearing in Mutter's normal window actor list.
        if (candidate)
            candidate.activate(global.get_current_time());
        const rect = candidate?.get_frame_rect();
        await click(rect ? rect.x + rect.width / 2 : 640, rect ? rect.y + rect.height / 2 : 400);
        await pause(500);
        await extension.captureArea({x: 0, y: 0, width: 1280, height: 800}, `${directory}/snipaste-active.png`);
        GLib.file_set_contents(`${directory}/reference-focus.json`, JSON.stringify({activeTitle: global.display.focus_window?.get_title(),
            pinRect: rect ? {x: rect.x, y: rect.y, width: rect.width, height: rect.height} : null}));
        await click(100, 150);
        await pause(700);
        await extension.captureArea({x: 0, y: 0, width: 1280, height: 800}, `${directory}/snipaste-idle.png`);
        GLib.file_set_contents(`${directory}/reference-dark`, 'dark');
        await pause(500);
        await click(rect ? rect.x + rect.width / 2 : 640, rect ? rect.y + rect.height / 2 : 400);
        await extension.captureArea({x: 0, y: 0, width: 1280, height: 800}, `${directory}/snipaste-active-dark.png`);
        await click(100, 150);
        await extension.captureArea({x: 0, y: 0, width: 1280, height: 800}, `${directory}/snipaste-idle-dark.png`);
        results.push({name: 'reference active/idle captures', passed: true});
        GLib.file_set_contents(`${directory}/reference-do-snip`, 'capture');
        for (let i = 0; i < 50 && !GLib.file_test(`${directory}/reference-snip-dispatched`, GLib.FileTest.EXISTS); i++) await pause(100);
        await pause(1600);
        GLib.file_set_contents(`${directory}/reference-capture-windows.json`, JSON.stringify(global.get_window_actors().map(a => ({
            title: a.meta_window.get_title(), type: a.meta_window.get_window_type(), rect: a.meta_window.get_frame_rect()}))));
        await extension.captureArea({x: 0, y: 0, width: 1280, height: 800}, `${directory}/snipaste-capture-ui.png`);
        const permission = global.get_window_actors().map(a => a.meta_window)
            .find(w => w.get_title() === 'Allow Apps to Take Screenshots?');
        if (permission) {
            const r = permission.get_frame_rect();
            await click(r.x + r.width * 0.73, r.y + r.height - 46);
            await pause(1500);
        }
        const keyboard = Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
        pointer.notify_button(GLib.get_monotonic_time(), 3, Clutter.ButtonState.PRESSED);
        pointer.notify_button(GLib.get_monotonic_time(), 3, Clutter.ButtonState.RELEASED); await pause(200);
        pointer.notify_absolute_motion(GLib.get_monotonic_time(), 180, 170); await pause(200);
        const phase=async name=>{GLib.file_set_contents(`${directory}/reference-phase`,name);await extension.captureArea({x:0,y:0,width:1280,height:800},`${directory}/magnifier-${name}.png`);};
        await phase('initial-hover');
        pointer.notify_button(GLib.get_monotonic_time(), 1, Clutter.ButtonState.PRESSED); await pause(100);
        pointer.notify_absolute_motion(GLib.get_monotonic_time(), 680, 450); await pause(100);
        await phase('initial-drag');
        pointer.notify_button(GLib.get_monotonic_time(), 1, Clutter.ButtonState.RELEASED); await pause(300);
        await phase('selected-hover');
        pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(100);
        pointer.notify_absolute_motion(GLib.get_monotonic_time(),700,470);await pause(100);await phase('resize');
        pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(150);await phase('resize-release');
        await extension.captureArea({x: 0, y: 0, width: 1280, height: 800}, `${directory}/snipaste-toolbar.png`);
        const layouts = [['below', 500, 200, 1000, 450], ['left', 10, 200, 200, 450],
            ['above', 500, 500, 1000, 785], ['inside-top', 30, 25, 1250, 795], ['inside-bottom', 30, 5, 1250, 775],
            ['equal-space', 30, 12, 1250, 787], ['full-screen', 0, 0, 1279, 799]];
        for (const [name, x, y, ex, ey] of layouts) {
            pointer.notify_button(GLib.get_monotonic_time(), 3, Clutter.ButtonState.PRESSED);
            pointer.notify_button(GLib.get_monotonic_time(), 3, Clutter.ButtonState.RELEASED); await pause(200);
            pointer.notify_absolute_motion(GLib.get_monotonic_time(), x, y); await pause(300);
            pointer.notify_button(GLib.get_monotonic_time(), 1, Clutter.ButtonState.PRESSED); await pause(80);
            pointer.notify_absolute_motion(GLib.get_monotonic_time(), ex, ey); await pause(100);
            pointer.notify_button(GLib.get_monotonic_time(), 1, Clutter.ButtonState.RELEASED); await pause(400);
            await extension.captureArea({x: 0, y: 0, width: 1280, height: 800}, `${directory}/layout-${name}.png`);
            GLib.file_set_contents(`${directory}/layout-${name}-windows.json`, JSON.stringify(global.get_window_actors().map(a => {
                const w = a.meta_window, r = w.get_frame_rect();
                return {title: w.get_title(), type: w.get_window_type(), x: r.x, y: r.y, width: r.width, height: r.height};
            })));
        }
        GLib.file_set_contents(`${directory}/layout-cases.json`, JSON.stringify(layouts));
        // Leave room below the subbar so the entire Qt marker list is visible.
        pointer.notify_button(GLib.get_monotonic_time(),3,Clutter.ButtonState.PRESSED);
        pointer.notify_button(GLib.get_monotonic_time(),3,Clutter.ButtonState.RELEASED);await pause(200);
        pointer.notify_absolute_motion(GLib.get_monotonic_time(),500,200);await pause(150);
        pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(80);
        pointer.notify_absolute_motion(GLib.get_monotonic_time(),1000,450);await pause(100);
        pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(300);
        if(GLib.getenv('CLIVUU_REFERENCE_CURSORS')==='1'){
            const sample=async(name,x,y)=>{pointer.notify_absolute_motion(GLib.get_monotonic_time(),x,y);await pause(200);await run(['python3',`${directory}/reference-cursor.py`,`${directory}/cursor-${name}`]);};
            for(const [name,x,y] of [['n',750,100],['s',750,650],['w',350,300],['e',1100,300],['nw',350,100],['ne',1100,100],['sw',350,650],['se',1100,650]])await sample(name,x,y);
            for(const [name,x] of [['mosaic',719],['eraser',783]]){
                await click(x,475);await extension.captureArea({x:0,y:0,width:1280,height:800},`${directory}/cursor-${name}-options.png`);
                await click(591,509);
                await sample(name,900,300);
                for(let n=0;n<10;n++)pointer.notify_discrete_scroll(GLib.get_monotonic_time(),Clutter.ScrollDirection.UP,Clutter.ScrollSource.WHEEL);
                await sample(`${name}-large`,900,300);
            }
            results.push({name:'Snipaste real freehand and exterior cursors sampled',passed:true});
            GLib.file_set_contents(`${directory}/reference-done`,'done');return;
        }
        await click(623,475);await pause(300);
        await extension.captureArea({x:0,y:0,width:1280,height:800},`${directory}/snipaste-arrow-options.png`);
        await click(635,509);await pause(300);
        await extension.captureArea({x:0,y:0,width:1280,height:800},`${directory}/snipaste-arrow-markers.png`);
        keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Escape,Clutter.KeyState.PRESSED);
        keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Escape,Clutter.KeyState.RELEASED);await pause(200);

        await click(719,475);await pause(300);
        await extension.captureArea({x:0,y:0,width:1280,height:800},`${directory}/snipaste-mosaic-options.png`);
        // Observe actual width limits rather than interpreting a slot/radius
        // limit as the visible stroke width. The reference remains isolated.
        for(const [kind,x] of [['shape',591],['arrow',623],['pen',655],['highlight',687],['mosaic',719]]){
            await click(x,475);pointer.notify_absolute_motion(GLib.get_monotonic_time(),900,300);await pause(100);
            for(let n=0;n<160;n++){pointer.notify_discrete_scroll(GLib.get_monotonic_time(),Clutter.ScrollDirection.UP,Clutter.ScrollSource.WHEEL);await pause(8);}
            await pause(180);await extension.captureArea({x:0,y:0,width:1280,height:800},`${directory}/snipaste-width-max-${kind}.png`);
        }
        await click(751,475);await click(650,300);
        for(const c of 'Size'){
            keyboard.notify_keyval(GLib.get_monotonic_time(),c.codePointAt(0),Clutter.KeyState.PRESSED);
            keyboard.notify_keyval(GLib.get_monotonic_time(),c.codePointAt(0),Clutter.KeyState.RELEASED);await pause(80);
        }
        await extension.captureArea({x:0,y:0,width:1280,height:800},`${directory}/snipaste-text-size-before.png`);
        pointer.notify_absolute_motion(GLib.get_monotonic_time(),950,260);await pause(180);
        await extension.captureArea({x:0,y:0,width:1280,height:800},`${directory}/snipaste-text-focused-away.png`);
        pointer.notify_absolute_motion(GLib.get_monotonic_time(),665,310);await pause(180);
        await extension.captureArea({x:0,y:0,width:1280,height:800},`${directory}/snipaste-text-focused-hover.png`);
        pointer.notify_absolute_motion(GLib.get_monotonic_time(),685,318);await pause(120);
        pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(100);
        for(let i=1;i<=12;i++){pointer.notify_absolute_motion(GLib.get_monotonic_time(),685+45*i/12,318+27*i/12);await pause(25);}
        pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(250);
        await extension.captureArea({x:0,y:0,width:1280,height:800},`${directory}/snipaste-text-size-drag.png`);
        // Observe committed text hover/re-edit/delete and both eraser modes.
        await click(850,370);
        for(const c of 'Other'){
            keyboard.notify_keyval(GLib.get_monotonic_time(),c.codePointAt(0),Clutter.KeyState.PRESSED);
            keyboard.notify_keyval(GLib.get_monotonic_time(),c.codePointAt(0),Clutter.KeyState.RELEASED);
        }
        await click(950,420);
        pointer.notify_absolute_motion(GLib.get_monotonic_time(),665,300);await pause(200);
        await extension.captureArea({x:0,y:0,width:1280,height:800},`${directory}/snipaste-text-hover.png`);
        await click(665,300);
        await extension.captureArea({x:0,y:0,width:1280,height:800},`${directory}/snipaste-text-reselect.png`);
        await click(783,475);
        await extension.captureArea({x:0,y:0,width:1280,height:800},`${directory}/snipaste-eraser-options.png`);
        const stroke=async(x,y,ex,ey)=>{
            pointer.notify_absolute_motion(GLib.get_monotonic_time(),x,y);await pause(80);
            pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(70);
            for(let n=1;n<=10;n++){pointer.notify_absolute_motion(GLib.get_monotonic_time(),x+(ex-x)*n/10,y+(ey-y)*n/10);await pause(25);}
            pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(200);
        };
        await stroke(610,300,720,300);
        await extension.captureArea({x:0,y:0,width:1280,height:800},`${directory}/snipaste-eraser-stroke.png`);
        await click(623,509);await stroke(640,280,700,330);
        await extension.captureArea({x:0,y:0,width:1280,height:800},`${directory}/snipaste-eraser-filled.png`);
        await stroke(780,250,960,420);
        await extension.captureArea({x:0,y:0,width:1280,height:800},`${directory}/snipaste-eraser-original.png`);
        GLib.file_set_contents(`${directory}/reference-controls-ready`,'ready');
        // Optional isolated observation actions. Only the private test owns this file.
        for(let i=0;i<50;i++){
            const path=`${directory}/reference-actions.json`;
            if(GLib.file_test(path,GLib.FileTest.EXISTS)){
                const [,raw]=GLib.file_get_contents(path);GLib.unlink(path);
                for(const command of JSON.parse(new TextDecoder().decode(raw))){
                    if(command.click)await click(...command.click);
                    if(command.scroll){const [x,y,steps]=command.scroll;pointer.notify_absolute_motion(GLib.get_monotonic_time(),x,y);await pause(60);for(let n=0;n<Math.abs(steps);n++){pointer.notify_discrete_scroll(GLib.get_monotonic_time(),steps>0?Clutter.ScrollDirection.UP:Clutter.ScrollDirection.DOWN,Clutter.ScrollSource.WHEEL);await pause(8);}await pause(100);}
                    if(command.double){const [x,y]=command.double;pointer.notify_absolute_motion(GLib.get_monotonic_time(),x,y);await pause(80);for(let n=0;n<2;n++){pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(70);}await pause(200);}
                    if(command.key){const keys=Array.isArray(command.key)?command.key:[command.key];for(const key of keys)keyboard.notify_keyval(GLib.get_monotonic_time(),key,Clutter.KeyState.PRESSED);for(const key of [...keys].reverse())keyboard.notify_keyval(GLib.get_monotonic_time(),key,Clutter.KeyState.RELEASED);await pause(200);}
                    if(command.text){for(const c of command.text){const cp=c.codePointAt(0),key=cp<256?cp:0x1000000|cp;keyboard.notify_keyval(GLib.get_monotonic_time(),key,Clutter.KeyState.PRESSED);keyboard.notify_keyval(GLib.get_monotonic_time(),key,Clutter.KeyState.RELEASED);await pause(25);}await pause(150);}
                    if(command.drag){const [x,y,ex,ey]=command.drag;pointer.notify_absolute_motion(GLib.get_monotonic_time(),x,y);await pause(100);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(60);pointer.notify_absolute_motion(GLib.get_monotonic_time(),ex,ey);await pause(100);pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(200);}
                    if(command.capture)await extension.captureArea({x:0,y:0,width:1280,height:800},`${directory}/${command.capture}.png`);
                }
            }
            await pause(500);
        }

        keyboard.notify_keyval(GLib.get_monotonic_time(), Clutter.KEY_Escape, Clutter.KeyState.PRESSED);
        keyboard.notify_keyval(GLib.get_monotonic_time(), Clutter.KEY_Escape, Clutter.KeyState.RELEASED);
        results.push({name: 'actual Snipaste capture IPC and UI observed in isolation', passed: true});
        GLib.file_set_contents(`${directory}/reference-done`, 'done');
    } catch (e) {
        results.push({name: 'reference', passed: false, error: `${e.message}\n${e.stack}`});
    } finally {
        reference?.force_exit();
        fixture?.force_exit();
        GLib.file_set_contents(`${directory}/results.json`, JSON.stringify(results, null, 2));
    }
}

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
        const press=async(code,ctrl=false)=>{
            if(ctrl)keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Control_L,Clutter.KeyState.PRESSED);
            keyboard.notify_keyval(GLib.get_monotonic_time(),code,Clutter.KeyState.PRESSED);
            keyboard.notify_keyval(GLib.get_monotonic_time(),code,Clutter.KeyState.RELEASED);
            if(ctrl)keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Control_L,Clutter.KeyState.RELEASED);
            await pause(300);
        };
        pointer.notify_button(GLib.get_monotonic_time(),3,Clutter.ButtonState.PRESSED);
        pointer.notify_button(GLib.get_monotonic_time(),3,Clutter.ButtonState.RELEASED);await pause(200);
        pointer.notify_absolute_motion(GLib.get_monotonic_time(),250,150);await pause(100);
        pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);
        pointer.notify_absolute_motion(GLib.get_monotonic_time(),900,450);await pause(100);
        pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(250);
        for(const [name,key,ctrl] of [['ctrl1',Clutter.KEY_1,true],['ctrl2',Clutter.KEY_2,true],['ctrl3',Clutter.KEY_3,true],['ctrl4',Clutter.KEY_4,true],['ctrl5',Clutter.KEY_5,true],['ctrl6',Clutter.KEY_6,true],['eraser',Clutter.KEY_e,false]]){
            await press(key,ctrl);await extension.captureArea({x:0,y:0,width:1280,height:800},`${directory}/shortcut-${name}.png`);
        }
        await press(Clutter.KEY_Escape);
        results.push({name:'Snipaste Ctrl+number and E shortcut images',passed:true});
    } catch(e){results.push({name:'Snipaste shortcuts',passed:false,message:String(e.stack??e)});}
    finally{GLib.file_set_contents(`${directory}/reference-done`,'done');fixture?.force_exit();reference?.force_exit();GLib.file_set_contents(`${directory}/results.json`,JSON.stringify(results));}
}

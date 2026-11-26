import Gio from 'gi://Gio';
import Shell from 'gi://Shell';
import Meta from 'gi://Meta';
import GLib from 'gi://GLib';
import Clutter from 'gi://Clutter';
import St from 'gi://St';
import {pause} from './common/process.js';

export async function saveChooser(destination = null) {
    let dialog;
    for (let i = 0; i < 80; i++) {
        dialog = global.get_window_actors().map(a => a.meta_window)
            .find(w => /Save screenshot|保存截图|Save recording|保存录像/.test(w.get_title()));
        if (dialog) break;
        await pause(100);
    }
    if (!dialog) throw new Error('System save chooser did not appear');
    if (dialog.get_client_type() !== Meta.WindowClientType.WAYLAND)
        throw new Error('Save chooser must use native Wayland, including XWayland pin service');
    dialog.activate(global.get_current_time()); await pause(400);
    const keyboard = Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
    await pause(400);
    // Native Wayland routes input after a seat enter, not merely an X11-style
    // window activation. Establish pointer focus before keyboard actions.
    const pointer=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
    const r=dialog.get_frame_rect();
    pointer.notify_absolute_motion(GLib.get_monotonic_time(),5,5);await pause(150);
    pointer.notify_absolute_motion(GLib.get_monotonic_time(),r.x+r.width/2,r.y+24);await pause(150);
    pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(80);
    pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(150);
    const key = async (symbol, modifier = null) => {
        if (modifier) { keyboard.notify_keyval(GLib.get_monotonic_time(), modifier, Clutter.KeyState.PRESSED); await pause(100); }
        keyboard.notify_keyval(GLib.get_monotonic_time(), symbol, Clutter.KeyState.PRESSED); await pause(80);
        keyboard.notify_keyval(GLib.get_monotonic_time(), symbol, Clutter.KeyState.RELEASED);
        await pause(80);
        if (modifier) keyboard.notify_keyval(GLib.get_monotonic_time(), modifier, Clutter.KeyState.RELEASED);
        await pause(150);
    };
    const debug = async name => {
        const path = `${GLib.getenv('CLIVUU_TEST_DIR')}/chooser-${name}.png`;
        const stream = Gio.File.new_for_path(path).replace(null,false,Gio.FileCreateFlags.PRIVATE,null);
        await new Shell.Screenshot().screenshot_area(0,0,1280,800,stream);stream.close(null);
    };
    GLib.file_set_contents(`${GLib.getenv('CLIVUU_TEST_DIR')}/chooser-focus.json`, JSON.stringify({focus:global.display.focus_window?.get_title(),keyFocus:global.stage.get_key_focus()?.toString()}));
    await debug(destination ? 'save-before' : 'cancel-before');
    if (destination) {
        await key(Clutter.KEY_l, Clutter.KEY_Control_L);
        await key(Clutter.KEY_a, Clutter.KEY_Control_L);
        St.Clipboard.get_default().set_text(St.ClipboardType.CLIPBOARD, destination);
        await key(Clutter.KEY_v, Clutter.KEY_Control_L);
        await debug('named');
        await key(Clutter.KEY_Return);
    } else {
        // Exercise the standard GTK cancellation action.
        await key(Clutter.KEY_Escape);
    }
    for (let i = 0; i < 80; i++) {
        if (!global.get_window_actors().some(a => a.meta_window === dialog)) return;
        await pause(100);
    }
    await debug('failed');
    throw new Error(`System save chooser did not close; focus=${global.display.focus_window?.get_title()}, target=${dialog.get_title()}`);
}

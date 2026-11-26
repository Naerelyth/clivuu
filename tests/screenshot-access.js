// Test-only input in the isolated GNOME session. Never changes permission data.
import Clutter from 'gi://Clutter';
import St from 'gi://St';
import GLib from 'gi://GLib';
import {pause} from './common/process.js';
export async function authorizeScreenshot(start) {
    const pending=start();let settled=false;pending.then(()=>{settled=true;},()=>{settled=true;});
    const pointer=Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
    const walk=function*(actor){yield actor;for(const child of actor.get_children())yield*walk(child);};
    for(let i=0;i<1100&&!settled;i++){
        const allow=[...walk(global.stage)].find(a=>a instanceof St.Button&&a.mapped&&/^(Allow|Allow Screenshots)$/.test(a.label));
        if(allow){const [x,y]=allow.get_transformed_position(),[w,h]=allow.get_transformed_size();
            pointer.notify_absolute_motion(GLib.get_monotonic_time(),x+w/2,y+h/2);await pause(100);
            pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.PRESSED);await pause(70);
            pointer.notify_button(GLib.get_monotonic_time(),1,Clutter.ButtonState.RELEASED);await pause(300);
        }
        await pause(100);
    }
    return pending;
}

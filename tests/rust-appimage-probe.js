// Exercise the shipped AppRun/CLI/service without installing an extension.
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import GdkPixbuf from 'gi://GdkPixbuf';
import Clutter from 'gi://Clutter';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {pause, run} from './common/process.js';
import {authorizeScreenshot} from './screenshot-access.js';

export async function probe(observer) {
    const directory = GLib.getenv('CLIVUU_TEST_DIR'), results = [];
    const bus = 'io.github.naerelyth.Clivuu.Desktop';
    let fixture;
    const check = (name, passed) => {
        results.push({name, passed: Boolean(passed)});
        GLib.file_set_contents(`${directory}/appimage-progress.json`, JSON.stringify(results));
        if (!passed) throw new Error(name);
    };
    const wait = async predicate => {
        for (let i = 0; i < 200; i++) {
            if (await predicate()) return;
            await pause(50);
        }
        throw new Error('AppImage validation timed out');
    };
    const cli = async (...args) => JSON.parse(await run([observer._workerPath(), ...args], null, null, {
        APPIMAGE: '', CLIVUU_DESKTOP_DIR: '', CLIVUU_LANGUAGE: 'en',
        CLIVUU_DESKTOP_LOG: `${directory}/appimage-native.log`,
    }));
    const status = () => cli('status');
    const pointer = Clutter.get_default_backend().get_default_seat()
        .create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
    const click = async (x, y, button = 1) => {
        pointer.notify_absolute_motion(GLib.get_monotonic_time(), x, y);
        await pause(100);
        pointer.notify_button(GLib.get_monotonic_time(), button, Clutter.ButtonState.PRESSED);
        await pause(60);
        pointer.notify_button(GLib.get_monotonic_time(), button, Clutter.ButtonState.RELEASED);
        await pause(150);
    };
    try {
        check('validation uses an actual AppImage', GLib.getenv('CLIVUU_TEST_APPIMAGE') === '1');
        GLib.file_set_contents(`${directory}/fixture-color`, '#2468ac');
        fixture = Gio.Subprocess.new(['gjs', '-m', `${directory}/fixture.js`], 0);
        await pause(1800);
        Main.overview.hide();
        await pause(400);
        check('packaged status does not start the desktop', !(await status()).running);
        check('launching the AppImage only starts the desktop', (await cli()).running);
        await wait(async () => (await status()).ready);
        const initial = await status();
        check('packaged native service works without the GNOME product extension',
            initial.backend === 'rust' && !initial.capabilities.smart_windows && !initial.busy && initial.pins === 0);
        const pid = await new Promise((resolve, reject) => Gio.DBus.session.call(
            'org.freedesktop.DBus', '/org/freedesktop/DBus', 'org.freedesktop.DBus',
            'GetConnectionUnixProcessID', new GLib.Variant('(s)', [bus]), null, 0, 5000, null,
            (connection, result) => {
                try { resolve(connection.call_finish(result).deep_unpack()[0]); } catch (e) { reject(e); }
            }));
        const executable = GLib.file_read_link(`/proc/${pid}/exe`);
        const sha = path => GLib.compute_checksum_for_data(GLib.ChecksumType.SHA256, GLib.file_get_contents(path)[1]);
        check('AppRun retains the exact release service after its mount exits',
            executable.includes('/clivuu-appimage-') && executable.endsWith('/libexec/clivuu-desktop') &&
            sha(executable) === sha('/tmp/clivuu-target/release/clivuu-desktop'));
        const output = `${directory}/appimage-capture.png`;
        const result = await authorizeScreenshot(() => cli('snip', '--area', '100', '160', '320', '180', '-o', output));
        const image = GdkPixbuf.Pixbuf.new_from_file(output);
        const pixels = image.get_pixels(), index = image.rowstride * 90 + image.n_channels * 160;
        check('packaged static Screenshot exports exact source pixels', result.path === output &&
            image.width === 320 && image.height === 180 && pixels.slice(index, index + 3).join(',') === '36,104,172');
        await cli('snip', '--area', '100', '160', '320', '180', '-o', 'pin');
        const pinned = await status(), rect = pinned.images[0].rect;
        check('packaged pins preserve selection placement and dimensions',
            pinned.pins === 1 && rect.x === 100 && rect.y === 160 && rect.width === 320 && rect.height === 180);
        for (let i = 0; i < 3; i++) {
            await click(140, 200, 3);
            await wait(async () => Boolean((await status()).pinUi.menu));
            await click(900, 650);
            await wait(async () => !(await status()).pinUi.menu);
        }
        const serial = () => GLib.file_test(`${directory}/fixture-last-click`, GLib.FileTest.EXISTS)
            ? JSON.parse(new TextDecoder().decode(GLib.file_get_contents(`${directory}/fixture-last-click`)[1])).serial : 0;
        const before = serial();
        await click(900, 600);
        await wait(() => serial() > before);
        check('packaged menu dismissal leaves the underlying desktop clickable', true);
        const longPath=`${directory}/appimage-long.png`;
        const pending=new Promise((resolve,reject)=>Gio.DBus.session.call(
            bus,'/io/github/naerelyth/Clivuu/Desktop',`${bus}1`,'Execute',
            new GLib.Variant('(s)',[JSON.stringify({protocol:1,command:'scroll',rect:{x:12,y:60,width:1256,height:690},output:longPath})]),
            null,0,120000,null,(connection,result)=>{try{resolve(JSON.parse(connection.call_finish(result).deep_unpack()[0]));}catch(e){reject(e);}}));
        pending.catch(()=>{});
        await wait(async()=> (await status()).scrolling?.frames>=1);
        pointer.notify_absolute_motion(GLib.get_monotonic_time(),500,400);
        let stable=true;
        for(let i=0;i<8;i++){
            const s=(await status()).scrolling;
            stable&&=!!s&&s.controlsVisible&&s.mapped&&!s.sampling&&s.toolbarOpacity===1&&s.previewOpacity===1;
            await pause(150);
        }
        check('packaged long capture keeps overlapping controls continuously visible',stable);
        const longState=(await status()).scrolling,save=longState.buttons.find(b=>b.label==='Save image').rect;
        await click(save.x+save.width/2,save.y+save.height/2);
        await wait(async()=>!(await status()).busy);await pending;
        const longImage=GdkPixbuf.Pixbuf.new_from_file(longPath),source=longImage.get_pixels();let bad=0;
        for(let y=0;y<longImage.height;y++)for(let x=0;x<longImage.width;x++){
            const a=y*longImage.rowstride+x*longImage.n_channels;
            if(Math.abs(source[a]-36)>1||Math.abs(source[a+1]-104)>1||Math.abs(source[a+2]-172)>1)bad++;
        }
        check('packaged long export contains source pixels without its toolbar or preview',longImage.width===1256&&longImage.height===690&&bad===0);
        await cli('open-preferences');
        await wait(() => global.get_window_actors().some(actor => actor.meta_window.get_title() === 'Clivuu Preferences'));
        check('packaged preferences open with native GTK', true);
        await cli('exit');
        await wait(async () => !(await status()).running);
        check('packaged exit releases the native service', true);
    } catch (error) {
        try { GLib.file_set_contents(`${directory}/appimage-failure.json`, JSON.stringify({state: await status(), focus: global.display.focus_window?.get_title(), overview: Main.overview.visible, windows: global.get_window_actors().map(a => ({title:a.meta_window.get_title(),rect:a.meta_window.get_frame_rect()}))})); } catch {}
        results.push({name: 'AppImage exception', passed: false, error: `${error.message}\n${error.stack}`});
        try { await cli('exit'); } catch {}
    } finally {
        fixture?.force_exit();
        GLib.file_set_contents(`${directory}/results.json`, JSON.stringify(results));
    }
}

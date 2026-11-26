// A real private D-Bus name deliberately precedes its exported interface.
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';

const loop = new GLib.MainLoop(null, false);
const name = 'io.github.naerelyth.Clivuu.Desktop';
const path = '/io/github/naerelyth/Clivuu/Desktop';
const xml = `<node><interface name="${name}1"><method name="Execute"><arg type="s" direction="in"/><arg type="s" direction="out"/></method></interface></node>`;
const started = GLib.get_monotonic_time();
let registration, status = 0, starts = 0, captures = 0, failure;
const run = (args, success) => new Promise((resolve, reject) => {
    const process = Gio.Subprocess.new([ARGV[0], ...args], Gio.SubprocessFlags.STDOUT_PIPE | Gio.SubprocessFlags.STDERR_PIPE);
    process.communicate_utf8_async(null, null, (child, result) => {
        try {
            const [, out, error] = child.communicate_utf8_finish(result);
            if (child.get_successful() !== success) throw new Error(error || out);
            resolve(out);
        } catch (e) { reject(e); }
    });
});
const owner = Gio.bus_own_name(Gio.BusType.SESSION, name, Gio.BusNameOwnerFlags.NONE,
    connection => {
        GLib.timeout_add(GLib.PRIORITY_DEFAULT, 250, () => {
            const info = Gio.DBusNodeInfo.new_for_xml(xml).interfaces[0];
            registration = connection.register_object(path, info, (_c, _s, _p, _i, _m, args, invocation) => {
                const request = JSON.parse(args.deep_unpack()[0]);
                if (request.command === 'status') status++;
                if (request.command === 'start') starts++;
                if (request.command === 'snip') {
                    captures++;
                    invocation.return_dbus_error('io.github.naerelyth.Clivuu.Error.Test', 'deliberate capture failure');
                    return;
                }
                invocation.return_value(new GLib.Variant('(s)', [JSON.stringify({protocol: 1, running: true, ready: true, backend: 'rust'})]));
            }, null, null);
            return GLib.SOURCE_REMOVE;
        });
    },
    async () => {
        try {
            const reply = JSON.parse(await run([], true));
            if (!reply.running || starts !== 1 || status < 1 || GLib.get_monotonic_time() - started < 240000)
                throw new Error('CLI did not await the exported interface and send start exactly once');
            await run(['snip', '--full', '-o', 'silent'], false);
            if (captures !== 1) throw new Error('CLI retried a mutating capture');
            const state = JSON.parse(await run(['status'], true));
            if (!state.running || starts !== 1 || captures !== 1) throw new Error('Status changed the operation counts');
            print('CLI readiness: delayed object export, single start/capture dispatch and read-only status passed');
        } catch (e) { failure = e; }
        finally { loop.quit(); }
    }, null);
loop.run();
Gio.bus_unown_name(owner);
if (failure) throw failure;

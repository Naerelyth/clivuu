import {t} from './i18n.js';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';

export function run(argv, cancellable = null, input = null, environment = {}, timeoutMs = 120000) {
    const launcher = new Gio.SubprocessLauncher({flags:
        Gio.SubprocessFlags.STDOUT_PIPE | Gio.SubprocessFlags.STDERR_PIPE |
        (input === null ? Gio.SubprocessFlags.NONE : Gio.SubprocessFlags.STDIN_PIPE)});
    for (const [name, value] of Object.entries(environment)) launcher.setenv(name, value, true);
    const process = launcher.spawnv(argv);
    return new Promise((resolve, reject) => {
        let timedOut = false;
        const timeout = timeoutMs > 0 ? GLib.timeout_add(GLib.PRIORITY_DEFAULT, timeoutMs, () => {
            timedOut = true;
            process.force_exit();
            return GLib.SOURCE_REMOVE;
        }) : 0;
        const cancelId = cancellable?.connect(() => process.force_exit());
        process.communicate_utf8_async(input, cancellable, (p, result) => {
            if (!timedOut && timeout)
                GLib.source_remove(timeout);
            if (cancelId)
                cancellable.disconnect(cancelId);
            try {
                const [, stdout, stderr] = p.communicate_utf8_finish(result);
                if (timedOut)
                    throw new Error(t("The operation exceeded 120 seconds; the worker was stopped"));
                if (!p.get_successful())
                    throw new Error(stderr.trim() || t("Process exited: {0}" ,argv[0]));
                resolve(stdout.trim());
            } catch (e) {
                reject(e);
            }
        });
    });
}

export function pause(ms, cancellable = null) {
    return new Promise((resolve, reject) => {
        let cancelId = 0;
        const id = GLib.timeout_add(GLib.PRIORITY_DEFAULT, ms, () => {
            if (cancelId)
                cancellable.disconnect(cancelId);
            resolve();
            return GLib.SOURCE_REMOVE;
        });
        cancelId = cancellable?.connect(() => {
            GLib.source_remove(id);
            // Avoid disconnecting from inside the cancellation callback.
            reject(new Error(t("Cancelled")));
        });
    });
}

import {t} from '../common/i18n.js';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';

export const DESTINATION = 'org.freedesktop.portal.Desktop';
export const PATH = '/org/freedesktop/portal/desktop';
export const variant = (type, value) => new GLib.Variant(type, value);
export const unpack = value => value instanceof GLib.Variant ? unpack(value.deep_unpack())
    : Array.isArray(value) ? value.map(unpack)
        : value && typeof value === 'object' ? Object.fromEntries(Object.entries(value).map(([k, v]) => [k, unpack(v)])) : value;

export function call(iface, method, parameters, path = PATH, cancellable = null) {
    return new Promise((resolve, reject) => Gio.DBus.session.call(DESTINATION, path,
        `org.freedesktop.portal.${iface}`, method, parameters, null,
        method === 'Close' ? Gio.DBusCallFlags.NO_AUTO_START : Gio.DBusCallFlags.NONE,
        method === 'Close' ? 5000 : 130000, cancellable, (connection, result) => {
            try { resolve(unpack(connection.call_finish(result))); } catch (e) { reject(e); }
        }));
}

// Subscribe to the predictable request path BEFORE invoking the method. The
// signal can otherwise beat the method reply on fast or cached-permission paths.
export function request(iface, method, signature, args, options = {}, cancellable = null) {
    const token = `clivuu_${GLib.uuid_string_random().replaceAll('-', '_')}`;
    const sender = Gio.DBus.session.get_unique_name().slice(1).replaceAll('.', '_');
    const path = `${PATH}/request/${sender}/${token}`;
    return new Promise((resolve, reject) => {
        let signal = 0, timer = 0, cancel = 0, settled = false;
        const finish = (error, result) => {
            if (settled) return;
            settled = true;
            if (signal) Gio.DBus.session.signal_unsubscribe(signal);
            if (timer) GLib.source_remove(timer);
            // Disconnecting from inside the cancelled callback deadlocks GIO.
            if (cancel) {
                const id = cancel;
                GLib.idle_add(GLib.PRIORITY_DEFAULT, () => { cancellable.disconnect(id); return GLib.SOURCE_REMOVE; });
            }
            if (error) {
                call('Request', 'Close', null, path).catch(() => {});
                reject(error);
            } else resolve(result);
        };
        signal = Gio.DBus.session.signal_subscribe(DESTINATION, 'org.freedesktop.portal.Request',
            'Response', path, null, Gio.DBusSignalFlags.NONE, (_c, _s, _p, _i, _m, data) => {
                const [code, result] = unpack(data);
                finish(code ? new Error(code === 1 ? t("System authorization or selection was cancelled") : t("The system denied the capture or recording request")) : null, result);
            });
        timer = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 120000, () => {
            timer = 0; finish(new Error(t("The system authorization request timed out"))); return GLib.SOURCE_REMOVE;
        });
        if (cancellable) {
            if (cancellable.is_cancelled()) { finish(new Error(t("Operation cancelled"))); return; }
            cancel = cancellable.connect(() => finish(new Error(t("Operation cancelled"))));
        }
        call(iface, method, variant(signature, [...args,
            {...options, handle_token: variant('s', token)}]), PATH, cancellable).catch(e => finish(e));
    });
}

export async function openPipeWire(session) {
    return new Promise((resolve, reject) => Gio.DBus.session.call_with_unix_fd_list(DESTINATION,
        PATH, 'org.freedesktop.portal.ScreenCast', 'OpenPipeWireRemote', variant('(oa{sv})', [session, {}]),
        null, Gio.DBusCallFlags.NONE, 10000, null, null, (connection, result) => {
            try {
                const [reply, fds] = connection.call_with_unix_fd_list_finish(result);
                resolve(fds.get(reply.deep_unpack()[0]));
            } catch (e) { reject(e); }
        }));
}

#!/usr/bin/env python3
"""Private IBus fixture: observe real GTK caret reports without host IM state."""
import json
import os
import time
from pathlib import Path
import gi
gi.require_version('IBus', '1.0')
from gi.repository import GLib, IBus

directory = Path(os.environ['CLIVUU_TEST_DIR'])
for attempt in range(100):
    files = sorted((directory / 'config/ibus/bus').glob('*'), key=lambda path: path.stat().st_mtime, reverse=True)
    if files:
        address = next(line.split('=', 1)[1] for line in files[0].read_text().splitlines() if line.startswith('IBUS_ADDRESS='))
        os.environ['IBUS_ADDRESS'] = address
        (directory / 'ime-address').write_text(address)
        break
    time.sleep(.1)
else:
    raise RuntimeError('Private GNOME IBus did not start')
IBus.init()
loop = GLib.MainLoop()
events = []


class Engine(IBus.Engine):
    def do_focus_in_id(self, path, client):
        print('focus in', path, client, flush=True)

    def do_focus_in(self):
        print('focus in', flush=True)

    def do_enable(self):
        print('enabled', flush=True)

    def do_set_cursor_location(self, x, y, width, height):
        events.append({'rect': [x, y, width, height]})
        (directory / 'ime-cursor.json').write_text(json.dumps(events[-100:]))

    def do_process_key_event(self, keyval, keycode, state):
        print('key', keyval, flush=True)
        if state & IBus.ModifierType.RELEASE_MASK:
            return False
        if keyval == IBus.KEY_F8:
            self.update_preedit_text(IBus.Text.new_from_string('中文'), 2, True)
            return True
        if keyval == IBus.KEY_F9:
            self.hide_preedit_text()
            self.commit_text(IBus.Text.new_from_string('中文'))
            return True
        return False


bus = IBus.Bus()
bus.set_watch_ibus_signal(True)
bus.connect('global-engine-changed', lambda _bus, name: print('global engine', name, flush=True))
factory = IBus.Factory.new(bus.get_connection())
factory.add_engine('clivuu-test', Engine.__gtype__)
component = IBus.Component.new('org.freedesktop.IBus.ClivuuTest', 'Clivuu test', '1', 'MIT', 'Clivuu', '', '', '')
component.add_engine(IBus.EngineDesc.new('clivuu-test', 'Clivuu test', 'Private test engine', 'zh', 'MIT', 'Clivuu', '', 'us'))
bus.register_component(component)
bus.request_name('org.freedesktop.IBus.ClivuuTest', 0)


def activate():
    def done(source, result):
        try:
            if not source.set_global_engine_async_finish(result):
                raise RuntimeError('Cannot activate private IBus fixture')
            (directory / 'ime-ready').write_text('ready')
        except Exception as error:
            (directory / 'ime-error').write_text(str(error))
    bus.set_global_engine_async('clivuu-test', -1, None, done)
    return GLib.SOURCE_REMOVE


GLib.timeout_add(300, activate)
def requested():
    path = directory / 'ime-activate'
    if path.exists():
        path.unlink()
        activate()
    if (directory / 'ime-stop').exists():
        def stopped(source, result):
            source.set_global_engine_async_finish(result)
            (directory / 'ime-stopped').write_text('stopped')
            loop.quit()
        bus.set_global_engine_async('xkb:us::eng', -1, None, stopped)
        return GLib.SOURCE_REMOVE
    return GLib.SOURCE_CONTINUE
GLib.timeout_add(100, requested)
loop.run()

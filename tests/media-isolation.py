#!/usr/bin/env python3
"""Fail a private test session if it discovers hardware or audio nodes."""
import json
import os
from pathlib import Path
import subprocess
import sys
import time


def verify(directory, pid, pipewire_pid):
    runtime = Path(os.environ['XDG_RUNTIME_DIR']).resolve()
    if runtime != directory.resolve() / 'runtime' or runtime.parent.parent != Path('/tmp'):
        raise RuntimeError('Media audit must run on a private /tmp test socket')
    deadline = time.monotonic() + 10
    while True:
        result = subprocess.run(['pw-dump'], capture_output=True, text=True, timeout=3)
        objects = json.loads(result.stdout) if result.returncode == 0 else []
        if any(str(o.get('info', {}).get('props', {}).get('application.process.id')) == str(pid) for o in objects):
            break
        if time.monotonic() >= deadline:
            (directory / 'media-registry.json').write_text(json.dumps(objects, indent=2))
            raise RuntimeError('Private WirePlumber did not become ready')
        time.sleep(0.1)
    # Let discovery complete before checking; recording is verified separately.
    time.sleep(0.5)
    objects = json.loads(subprocess.check_output(['pw-dump'], text=True, timeout=3))
    hardware = []
    for obj in objects:
        props = obj.get('info', {}).get('props', {})
        if (obj.get('type') == 'PipeWire:Interface:Device'
                or props.get('media.class', '').startswith('Audio/')
                or any(str(props.get(k, '')).startswith(('alsa', 'bluez', 'v4l2', 'libcamera', 'api.alsa', 'api.bluez', 'api.v4l2', 'api.libcamera'))
                       for k in ('device.api', 'api', 'factory.name'))):
            hardware.append({'id': obj['id'], 'props': props})
    descriptors = []
    libraries = set()
    for media_pid in (pid, pipewire_pid):
        process = Path('/proc') / str(media_pid)
        for fd in (process / 'fd').iterdir():
            try:
                target = os.readlink(fd)
            except FileNotFoundError:
                continue
            if target.startswith(('/dev/snd/', '/dev/video', '/dev/media')):
                descriptors.append({'pid': media_pid, 'target': target})
        libraries.update(line.split()[-1] for line in (process / 'maps').read_text().splitlines()
                         if any(name in line for name in ('libspa-alsa', 'libspa-bluez', 'libspa-v4l2', 'libspa-libcamera')))
    report = {'passed': not (hardware or descriptors or libraries), 'hardware_objects': hardware,
              'hardware_descriptors': descriptors, 'hardware_libraries': sorted(libraries),
              'profile': 'clivuu-test', 'runtime': str(runtime)}
    (directory / 'media-isolation.json').write_text(json.dumps(report, indent=2))
    if not report['passed']:
        raise RuntimeError('Test media session unexpectedly opened hardware; see media-isolation.json')
    print('Private media isolation: no hardware devices, audio nodes, hardware handles or monitor libraries', flush=True)


if __name__ == '__main__':
    verify(Path(sys.argv[1]), int(sys.argv[2]), int(sys.argv[3]))

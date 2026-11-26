#!/usr/bin/env python3
"""CLI configuration, no GUI, bus, schema or dconf; artifacts only in /tmp."""
import concurrent.futures
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

root = Path(__file__).resolve().parents[1]
directory = Path(tempfile.mkdtemp(prefix='clivuu-config-', dir='/tmp'))
binary = os.environ.get('CLIVUU_TEST_BINARY', '/tmp/clivuu-target/debug/clivuu')
env = dict(os.environ, HOME=str(directory/'home'), XDG_CONFIG_HOME=str(directory/'config'),
           XDG_DATA_HOME=str(directory/'data'), CLIVUU_TEST_DIR=str(directory), CLIVUU_BINARY=binary,
           DBUS_SESSION_BUS_ADDRESS='unix:path=/tmp/no-clivuu-test-bus')
def cli(*args, success=True, environ=env):
    p = subprocess.run([binary,'config',*args],env=environ,text=True,capture_output=True)
    assert (p.returncode == 0) == success, (args,p.stderr)
    return p.stdout.strip()
try:
    assert cli('path') == str(directory/'config/clivuu/config.ini')
    assert json.loads(cli('get','language')) == 'system'
    assert json.loads(cli('get','theme')) == 'system'
    cli('set','theme','dark')
    assert json.loads(cli('get','theme')) == 'dark'
    cli('set','theme','invalid',success=False)
    cli('set','theme','system')
    assert 'theme' not in (directory/'config/clivuu/config.ini').read_text()
    with concurrent.futures.ThreadPoolExecutor() as pool:
        list(pool.map(lambda args: cli('set', *args), [('language','en'),('video-directory',str(directory/'Video #1')),('scroll-interval','800')]))
    assert json.loads(cli('get','scroll-interval')) == 800
    assert json.loads(cli('get','language')) == 'en'
    assert json.loads(cli('get','video-directory')) == str(directory/'Video #1')
    assert (directory/'config/clivuu/config.ini').stat().st_mode & 0o777 == 0o600
    config = directory/'config/clivuu/config.ini'
    contents = config.read_text()
    assert '[general]' in contents and '[save.video]' in contents and '[scrolling]' in contents
    assert 'capture-border-width' not in contents and '[clivuu]' not in contents
    cli('set','language','system')
    contents = config.read_text()
    assert 'language' not in contents and '[general]' not in contents
    assert json.loads(cli('get','language')) == 'system'
    # Old flat files retain their values and migrate without saving defaults.
    config.write_text('[clivuu]\nlanguage="en"\ncapture-border-width=2\n')
    cli('set','capture-mask-color','"#00000099"')
    contents = config.read_text()
    assert '[capture.display]' in contents and '[general]' in contents
    assert '[clivuu]' not in contents and 'capture-border-width' not in contents
    cli('set','capture-mask-color',json.dumps(json.loads((root/'core/config-defaults.json').read_text())['capture-mask-color']['default']))
    assert '[capture.display]' not in config.read_text()
    before=config.read_bytes()
    cli('apply',json.dumps({'language':'zh-CN','capture-mask-color':'#000000gg'}),success=False)
    assert config.read_bytes()==before
    cli('apply',json.dumps({'language':'en','capture-mask-color':'#0000009c','capture-magnifier-border':False}))
    assert json.loads(cli('get','capture-mask-color'))=='#0000009c'
    unchanged=config.stat().st_mtime_ns
    cli('apply',json.dumps({'capture-mask-color':'#0000009c'}))
    assert config.stat().st_mtime_ns==unchanged
    for key,value in [('save-directory','/tmp'),('pin-storage','disk'),('video-directory','relative'),('scroll-interval','1')]: cli('set',key,value,success=False)
    relative = dict(env,XDG_CONFIG_HOME='relative')
    assert cli('path',environ=relative) == str(directory/'home/.config/clivuu/config.ini')
    print('config: CLI concurrency, validation, privacy, XDG defaults/overrides passed')
finally:
    shutil.rmtree(directory)

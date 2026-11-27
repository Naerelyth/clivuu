#!/usr/bin/env python3
"""Native desktop validation in private HOME/XDG/bus and headless GNOME."""
import argparse
import atexit
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
from reference_stage import stage
ROOT=Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser()
modes=['rust-cursor-scale','rust-playback','rust-i18n','rust-color','rust-recording-audio','rust-mouse','rust-monitors','rust-shortcuts','rust-pin-preferences','rust-paste','rust-notification','rust-stroke','rust-cursor','rust-capture-consent','rust-save-dialog','rust-saving','rust-preferences-sections','rust-preferences','rust-workflows','rust-service','rust-editor','rust-ui-parity','rust-integration','rust-pins','rust-scrolling','rust-scrolling-grid','rust-continuous','rust-layer','rust-appimage','reference']
select=parser.add_mutually_exclusive_group()
for mode in modes:select.add_argument('--'+mode,action='store_true')
parser.add_argument('--reference-probe',type=Path)
parser.add_argument('--two-monitors',action='store_true')
parser.add_argument('--single-monitor',action='store_true',help='Run --rust-monitors on one 1920x1080 output')
parser.add_argument('--renderer',choices=['cairo','gl','vulkan'],default='gl')
parser.add_argument('--keep-session',action='store_true')
parser.add_argument('--binary',type=Path,default=Path('/tmp/clivuu-target/debug/clivuu'))
args=parser.parse_args()
if args.single_monitor and not args.rust_monitors:
    parser.error('--single-monitor requires --rust-monitors')
mode=next((mode for mode in modes if getattr(args,mode.replace('-','_'))),'rust-service')
directory=Path(tempfile.mkdtemp(prefix='clivuu-gnome-',dir='/tmp'))
def clean_session():
    if not args.keep_session:
        for name in ['data','config','cache','runtime','private-tmp','home']:shutil.rmtree(directory/name,ignore_errors=True)
atexit.register(clean_session)
for name in ['data','config','cache','runtime','home']:(directory/name).mkdir(mode=0o700)
# Only distribution media configs are read. Test WirePlumber has no physical
# audio, Bluetooth or camera discovery, even though the system bus is visible.
for service in ['pipewire','wireplumber']:
    config=directory/'media'/service;config.mkdir(parents=True,mode=0o700)
    shutil.copy2(Path('/usr/share')/service/(service+'.conf'),config)
shutil.copy2('/usr/share/pipewire/client.conf',directory/'media/pipewire/client.conf')
fragment=directory/'media/wireplumber/wireplumber.conf.d';fragment.mkdir(mode=0o700)
shutil.copy2(ROOT/'tests/wireplumber-test.conf',fragment/'99-clivuu-test.conf')
extension=stage(args.binary,directory/'data/gnome-shell/extensions')
panel_source=Path.home()/'.local/share/gnome-shell/extensions/dash-to-panel@jderose9.github.com'
if panel_source.is_dir() and not args.rust_scrolling_grid:
    target=extension.parent/panel_source.name;shutil.copytree(panel_source,target)
    subprocess.run(['glib-compile-schemas',str(target/'schemas')],check=True)
if args.rust_integration:
    host=Path.home()/'.local/share/gnome-shell/extensions/appindicatorsupport@rgcjonas.gmail.com'
    if not host.is_dir():parser.error('--rust-integration needs the installed AppIndicator test host')
    target=extension.parent/host.name;shutil.copytree(host,target)
    subprocess.run(['glib-compile-schemas',str(target/'schemas')],check=True)
if args.rust_scrolling_grid:
    grid=directory/'nautilus-grid';grid.mkdir()
    words=['amber','birch','cedar','delta','elm','fern','grove','hazel']
    for n in range(240):(grid/f'folder-{n:03d}-{words[n%len(words)]}').mkdir()
for name in ['fixture.js','x11-remap.py','x11-shape.py','gnome-session.sh','media-isolation.py','clipboard-read.js','clipboard-owner.js']:
    shutil.copy2(ROOT/'tests'/name,directory/name)
if args.rust_monitors or args.rust_mouse:
    shutil.copy2(ROOT/'tests/x11-root-cursor.py',directory)
for name in ['save-chooser.js','screenshot-access.js','scrolling-auto.js','pin-mouse.js','recording-native.js','recording-audio.js','recording-playback.js']:shutil.copy2(ROOT/'tests'/name,extension/name)
if args.rust_ui_parity:
    shutil.copy2(ROOT/'tests/toolbar-reference.js',extension/'portable/toolbar-reference.js')
    for name in ['toolbar-cases.json','toolbar-panel-cases.json']:shutil.copy2(ROOT/'tests'/name,directory/name)
    legacy=(extension/'portable/selection.js').read_text()
    marker='try { this.overlay = this.embedded ?'
    if legacy.count(marker)!=1:raise ValueError('Reference constructor changed; refuse an invalid pixel fixture')
    (extension/'portable/selection-reference.js').write_text(legacy.replace(marker,'this.overlay={views:[]}; return; '+marker,1))
shutil.copy2((args.reference_probe if args.reference and args.reference_probe else ROOT/'tests'/('snipaste-probe.js' if args.reference else mode+'-probe.js' if mode!='rust-ui-parity' else 'toolbar-parity-probe.js')),extension/'gnome-probe.js')
if args.reference:
    import cairo
    (directory / 'private-tmp').mkdir(mode=0o700)
    shutil.copy2(ROOT / 'tests/reference-run.sh', directory)
    shutil.copy2(ROOT / 'tests/reference-cursor.py', directory)
    subprocess.run(['gcc', '-shared', '-fPIC', str(ROOT / 'tests/reference-hook.c'), '-ldl', '-o',
                    str(directory / 'reference-hook.so')], check=True)
    surface = cairo.ImageSurface(cairo.FORMAT_ARGB32, 320, 180)
    ctx = cairo.Context(surface)
    ctx.set_source_rgb(0.9, 0.9, 0.9)
    ctx.paint()
    ctx.set_source_rgb(0.2, 0.4, 0.7)
    ctx.rectangle(40, 40, 240, 100)
    ctx.fill()
    surface.write_to_png(str(directory / 'palette.png'))
    (directory / 'snipaste.ini').write_text('[General]\nfirst_run=false\n[Misc]\ncheck_update=false\n')
(extension/'extension.js').write_text("""import GLib from 'gi://GLib';
import Gio from 'gi://Gio';
import Shell from 'gi://Shell';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import {probe} from './gnome-probe.js';
export default class Observer extends Extension {
    _workerPath(){return `${this.path}/bin/clivuu`;}
    async captureArea(rect,path){
        const stream=Gio.File.new_for_path(path).replace(null,false,Gio.FileCreateFlags.PRIVATE,null);
        try{await new Shell.Screenshot().screenshot_area(rect.x,rect.y,rect.width,rect.height,stream);}finally{stream.close(null);}
    }
    enable(){ GLib.timeout_add(GLib.PRIORITY_DEFAULT,3000,()=>{probe(this).catch(console.error);return GLib.SOURCE_REMOVE;});}
    disable(){}
}
""")
if (args.rust_integration or args.rust_capture_consent or args.rust_notification or args.rust_saving or args.rust_recording_audio or args.rust_playback or args.rust_cursor or args.rust_cursor_scale or args.rust_pins or args.rust_monitors or args.rust_shortcuts or args.rust_mouse) and os.environ.get('CLIVUU_TEST_ANONYMOUS') != '1':
    applications=directory/'data/applications';applications.mkdir()
    (applications/'renamed-clivuu.desktop').write_text('[Desktop Entry]\nType=Application\nName=Clivuu\nStartupWMClass=io.github.naerelyth.clivuu\nExec='+str(args.binary.resolve())+'\nIcon='+str(extension/'icons/clivoo.svg')+'\n')
env=os.environ.copy()
for name in ['WAYLAND_DISPLAY','DISPLAY','DBUS_SESSION_BUS_ADDRESS','GJS_DEBUG_OUTPUT','GJS_DEBUG_TOPICS','APPIMAGE','CLIVUU_DESKTOP_BINARY']:env.pop(name,None)
env.update(HOME=str(directory/'home'),XDG_DATA_HOME=str(directory/'data'),XDG_CONFIG_HOME=str(directory/'config'),XDG_CACHE_HOME=str(directory/'cache'),XDG_RUNTIME_DIR=str(directory/'runtime'),XDG_STATE_HOME=str(directory/'home/.local/state'),GSETTINGS_BACKEND='keyfile',LIBGL_ALWAYS_SOFTWARE='1',CLIVUU_TEST_DIR=str(directory),XDG_CURRENT_DESKTOP='GNOME',XDG_SESSION_TYPE='wayland',TMPDIR='/tmp',WAYLAND_DISPLAY='wayland-clivuu-test',GDK_BACKEND='wayland',GTK_A11Y='none',GSK_RENDERER=args.renderer,CLIVUU_DESKTOP_DIR=str(extension),CLIVUU_RUST_EDITOR_PROBE=os.environ.get('CLIVUU_RUST_EDITOR_PROBE',str(args.binary.parent/'examples/editor_probe')))
if args.rust_ui_parity:env['CLIVUU_REFERENCE_GALLERY']=str(extension/'portable/toolbar-reference.js')
if args.rust_i18n:env['CLIVUU_RUST_I18N_PROBE']=str(args.binary.resolve().parent/'examples/i18n_probe')
if args.rust_layer:env['CLIVUU_RUST_SURFACE_PROBE']=os.environ.get('CLIVUU_RUST_SURFACE_PROBE',str(args.binary.parent/'examples/surface_probe'))
if args.two_monitors:env['CLIVUU_TEST_TWO_MONITORS']='1'
if args.rust_monitors:env.update(CLIVUU_TEST_TWO_MONITORS='0' if args.single_monitor else '1',CLIVUU_TEST_MONITOR_SIZE='1920x1080' if args.single_monitor else '2880x1800',CLIVUU_TEST_SECOND_MONITOR_SIZE='1920x1080',CLIVUU_TEST_MIXED_MONITORS='1',CLIVUU_TEST_SINGLE_MONITOR='1' if args.single_monitor else '0')
env['CLIVUU_TEST_EXTENSION_UUID']=extension.name
with args.binary.open('rb')as stream:appimage=stream.read(11)[8:11]==b'AI\x02'
if appimage:env.update(APPIMAGE_EXTRACT_AND_RUN='1',CLIVUU_TEST_APPIMAGE='1')
else:env.setdefault('CLIVUU_TEST_DESKTOP_BINARY',str(args.binary.resolve().parent/'clivuu-desktop'))
if panel_source.is_dir() and not args.rust_scrolling_grid:env['CLIVUU_TEST_PANEL']='1'
if args.reference:env.update(CLIVUU_REFERENCE='1',CLIVUU_REFERENCE_LOG=str(directory/'reference-abi.log'))
print(f"Isolated GNOME test: {directory}", flush=True)
with (directory / 'session.log').open('w') as log:
    process = subprocess.Popen(["dbus-run-session", "--", "sh", str(directory / "gnome-session.sh")], env=env,
                               stdout=subprocess.PIPE, stderr=subprocess.STDOUT, start_new_session=True)
    def drain_log():
        remaining = 2 * 1024 * 1024
        while chunk := process.stdout.read(8192):
            if remaining:
                text = chunk[:remaining].decode(errors='replace')
                log.write(text)
                log.flush()
                remaining -= min(remaining, len(chunk))
    logger = threading.Thread(target=drain_log, daemon=True)
    logger.start()
    try:
        code = process.wait(timeout=320)
    finally:
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        logger.join(timeout=2)
if code:
    print((directory / 'session.log').read_text()[-10000:])
    sys.exit(code)
results = json.loads((directory / "results.json").read_text())
if not results:
    results.append({"name": "probe produced functional assertions", "passed": False})
if args.rust_ui_parity and all(r['passed'] for r in results):
    sys.path.insert(0,str(ROOT/'tests'))
    from toolbar_parity import compare
    results.extend(compare(directory))
if args.rust_scrolling_grid and all(r['passed'] for r in results):
    from scrolling_grid import verify
    results.extend(verify(directory))
media = json.loads((directory / 'media-isolation.json').read_text())
results.append({'name': 'private media policy discovers no sound, Bluetooth or camera hardware', 'passed': media['passed']})
(directory / 'results.json').write_text(json.dumps(results, ensure_ascii=False, indent=2))
print(json.dumps(results, ensure_ascii=False, indent=2))
sys.exit(0 if all(r["passed"] for r in results) else 1)

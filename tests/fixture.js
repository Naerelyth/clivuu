import Gtk from 'gi://Gtk?version=4.0';
import GLib from 'gi://GLib';
import Gdk from 'gi://Gdk?version=4.0';
const app = new Gtk.Application({application_id: 'io.github.naerelyth.ClivuuFixture', flags: imports.gi.Gio.ApplicationFlags.NON_UNIQUE});
app.connect('activate', () => {
    const window = new Gtk.ApplicationWindow({application: app, title: GLib.getenv('CLIVUU_FIXTURE_TITLE') || 'Clivuu focus fixture',
        default_width: 1000, default_height: 700});
    const keys = new Gtk.EventControllerKey({propagation_phase: Gtk.PropagationPhase.CAPTURE});
    keys.connect('key-pressed', (_c, key) => {
        GLib.file_set_contents(`${GLib.getenv('CLIVUU_TEST_DIR')}/fixture-last-key`, String(key)); return false;
    }); window.add_controller(keys);
    let clickSerial=0;
    const clicks=new Gtk.EventControllerLegacy({propagation_phase:Gtk.PropagationPhase.CAPTURE});
    clicks.connect('event',(_controller,event)=>{
        if(event.get_event_type()===Gdk.EventType.BUTTON_PRESS){
            const [,x,y]=event.get_position();
            GLib.file_set_contents(`${GLib.getenv('CLIVUU_TEST_DIR')}/fixture-last-click`,JSON.stringify({serial:++clickSerial,button:event.get_button(),x,y}));
        }return false;
    });window.add_controller(clicks);
    const scroller = new Gtk.ScrolledWindow();
    const view = new Gtk.TextView({editable: false, monospace: true, left_margin: 60,
        right_margin: 30, top_margin: 30, bottom_margin: 30});
    view.get_buffer().set_text(Array.from({length: 500}, (_, i) =>
        `${String(i + 1).padStart(3, '0')}  Clivuu 截图测试 / Scroll capture test   ${((i * 7919) % 104729).toString(16)}\n` +
        `     GNOME Wayland • Focus stays in this maximized application.  [${i * 31}]\n`).join('\n'), -1);
    scroller.set_child(view);
    const content = new Gtk.Box({orientation: Gtk.Orientation.VERTICAL});
    const header = new Gtk.Box({orientation: Gtk.Orientation.VERTICAL, height_request: 300,
        visible: false, spacing: 36});
    header.add_css_class('scrolling-fixed-title');
    header.append(new Gtk.Label({label: 'Home — Clivuu fixed title regression'}));
    header.append(new Gtk.Label({label: 'Documents       Pictures       Downloads       Home'}));
    content.append(header);content.append(scroller);scroller.set_vexpand(true);
    const headerStyle = new Gtk.CssProvider();
    headerStyle.load_from_string('.scrolling-fixed-title {background-color:#512b73;color:#ffffff;}');
    Gtk.StyleContext.add_provider_for_display(window.get_display(),headerStyle,Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION);
    window.set_child(content);
    window.present();
    window.maximize();
    const visual = new Gtk.CssProvider();
    Gtk.StyleContext.add_provider_for_display(window.get_display(), visual, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION);
    let lastColor = '';
    let full = false;
    GLib.timeout_add(GLib.PRIORITY_DEFAULT, 100, () => {
        const path = `${GLib.getenv('CLIVUU_TEST_DIR')}/fixture-color`;
        const color = GLib.file_test(path, GLib.FileTest.EXISTS)
            ? new TextDecoder().decode(GLib.file_get_contents(path)[1]).trim() : '';
        if (color !== lastColor) {
            visual.load_from_string(/^#[0-9a-f]{6}$/i.test(color)
                ? `textview, textview text { background-color: ${color}; color: ${color}; } scrollbar { opacity: 0; }` : '');
            // Hide only the solid-color oracle's animated scrollbar pixels;
            // retain scrolling and allocation when returning to real text.
            lastColor = color;
        }
        const requestPath = `${GLib.getenv('CLIVUU_TEST_DIR')}/fixture-scroll-request`;
        if (GLib.file_test(requestPath, GLib.FileTest.EXISTS)) {
            const request = new TextDecoder().decode(GLib.file_get_contents(requestPath)[1]).trim();
            GLib.unlink(requestPath);
            const adjustment = scroller.get_vadjustment();
            adjustment.set_value(request==='bottom' ? adjustment.upper-adjustment.page_size : Number(request)||0);
        }
        GLib.file_set_contents(`${GLib.getenv('CLIVUU_TEST_DIR')}/fixture-scroll.json`,JSON.stringify({value:scroller.get_vadjustment().value}));
        header.set_visible(GLib.file_test(`${GLib.getenv('CLIVUU_TEST_DIR')}/fixture-header`, GLib.FileTest.EXISTS));
        const nextFull = GLib.file_test(`${GLib.getenv('CLIVUU_TEST_DIR')}/fixture-fullscreen`, GLib.FileTest.EXISTS);
        if (nextFull !== full) { full = nextFull; if (full) window.fullscreen(); else window.unfullscreen(); }
        return GLib.SOURCE_CONTINUE;
    });
    if (GLib.getenv('CLIVUU_REFERENCE')) {
        const provider = new Gtk.CssProvider();
        Gtk.StyleContext.add_provider_for_display(window.get_display(), provider, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION);
        GLib.timeout_add(GLib.PRIORITY_DEFAULT, 150, () => {
            if (GLib.file_test(`${GLib.getenv('CLIVUU_TEST_DIR')}/reference-dark`, GLib.FileTest.EXISTS)) {
                provider.load_from_string('textview, textview text { background-color: #000000; color: #000000; }');
                return GLib.SOURCE_REMOVE;
            }
            return GLib.SOURCE_CONTINUE;
        });
    }
});
app.run([]);

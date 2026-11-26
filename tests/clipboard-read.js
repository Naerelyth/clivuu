// Read through the X11 clipboard without creating a Wayland focus surface.
import Gtk from 'gi://Gtk?version=4.0';
import Gdk from 'gi://Gdk?version=4.0';
import GLib from 'gi://GLib';
Gtk.init();
const loop = new GLib.MainLoop(null, false);
const clipboard = Gdk.Display.get_default().get_clipboard();
if (ARGV[0] === '--mime') clipboard.read_async([ARGV[1]], GLib.PRIORITY_DEFAULT, null, (object,result) => {
    const [stream] = object.read_finish(result);
    stream.read_bytes_async(65536,GLib.PRIORITY_DEFAULT,null,(input,result)=>{
        try { print(new TextDecoder().decode(input.read_bytes_finish(result).get_data())); } finally { loop.quit(); }
    });
});
else if (ARGV[0]) clipboard.read_texture_async(null, (object, result) => {
    try { object.read_texture_finish(result).save_to_png(ARGV[0]); } finally { loop.quit(); }
});
else clipboard.read_text_async(null, (object, result) => {
    try { print(object.read_text_finish(result)); } finally { loop.quit(); }
});
loop.run();

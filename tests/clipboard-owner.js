// Persistent owner for real MIME negotiation, on the selected GDK backend.
import Gtk from 'gi://Gtk?version=4.0';
import Gdk from 'gi://Gdk?version=4.0';
import GLib from 'gi://GLib';
Gtk.init();
const directory=ARGV[0]||GLib.getenv('CLIVUU_TEST_DIR');
const clipboard=Gdk.Display.get_default().get_clipboard();
const loop=new GLib.MainLoop(null,false);
let serial=-1;
let window;
if(Gdk.Display.get_default().constructor.name.includes('Wayland')){
 window=new Gtk.Window({title:'Clivuu clipboard owner',default_width:320,default_height:180});
 window.set_child(new Gtk.Entry({placeholder_text:'Native clipboard fixture'}));window.present();
}
GLib.timeout_add(GLib.PRIORITY_DEFAULT,40,()=>{
 try{
  const request=JSON.parse(new TextDecoder().decode(GLib.file_get_contents(`${directory}/clipboard-request.json`)[1]));
  if(request.serial===serial)return GLib.SOURCE_CONTINUE;
  const providers=(request.formats||[]).map(f=>Gdk.ContentProvider.new_for_bytes(f.mime,new GLib.Bytes(f.file?GLib.file_get_contents(f.file)[1]:new TextEncoder().encode(f.text||''))));
  clipboard.set_content(providers.length?Gdk.ContentProvider.new_union(providers):null);
  serial=request.serial;GLib.file_set_contents(`${directory}/clipboard-ready`,String(serial));
 }catch(e){if(!e.message.includes('No such file'))printerr(e.message);}
 return GLib.SOURCE_CONTINUE;
});
loop.run();

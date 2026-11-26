// Test-only pre-migration toolbar fixture. selection-reference.js is a temporary
// copy whose platform overlay setup returns after installing its original CSS.
import Gtk from 'gi://Gtk?version=4.0';
import Adw from 'gi://Adw';
import Gdk from 'gi://Gdk?version=4.0';
import GLib from 'gi://GLib';
import Graphene from 'gi://Graphene';
import Cairo from 'cairo';
import {Selection} from './selection-reference.js';
import {pinEditActions} from '../common/pin-edit.js';
import {initI18n} from '../common/locale.js';
import {t} from '../common/i18n.js';
import {toolbarTheme,toolbarControlCss} from '../common/toolbar-theme.js';
Gtk.init();Adw.init();initI18n();
Gtk.Settings.get_default().gtk_cursor_blink=false;
Gtk.Settings.get_default().gtk_enable_animations=false;
const directory=GLib.getenv('CLIVUU_TEST_DIR'),loop=new GLib.MainLoop(null,false),window=new Gtk.Window({title:'Clivuu reference toolbar gallery',decorated:false,default_width:1280,default_height:800}),root=new Gtk.Fixed();window.set_child(root);window.present();window.maximize();
window.add_css_class('clivuu-gallery-surface');const background=new Gtk.CssProvider();background.load_from_string('.clivuu-gallery-surface {background:#181818;}');Gtk.StyleContext.add_provider_for_display(Gdk.Display.get_default(),background,Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION+20);
const image=new Cairo.ImageSurface(Cairo.Format.ARGB32,1280,800),ctx=new Cairo.Context(image);ctx.setSourceRGB(1,1,1);ctx.paint();ctx.$dispose();image.writeToPNG(`${directory}/toolbar-source.png`);image.finish();
let cases=[];for(const dark of [false,true])for(const pin of [false,true]){
 const prefix=`${dark?'dark':'light'}-${pin?'pin':'capture'}`;
 for(const tool of ['select','rect','ellipse','arrow','pen','highlight','mosaic','text','eraser'])cases.push({dark,pin,tool,name:`${prefix}-${tool}`});
 for(const [variable,file] of [['CLIVUU_GALLERY_VARIANTS','toolbar-cases.json'],['CLIVUU_GALLERY_PANELS','toolbar-panel-cases.json']])if(dark===pin&&GLib.getenv(variable))for(const v of JSON.parse(new TextDecoder().decode(GLib.file_get_contents(`${directory}/${file}`)[1])))cases.push({...v,dark,pin,name:`${prefix}-${v.tool}-${v.id}`});
}
const filter=GLib.getenv('CLIVUU_GALLERY_FILTER');if(filter)cases=cases.filter(c=>filter.split(',').includes(c.name));
const snapshot=(widget,path)=>{const paint=Gtk.WidgetPaintable.new(widget),w=paint.get_intrinsic_width(),h=paint.get_intrinsic_height(),scale=widget.get_scale_factor(),width=w*scale,height=h*scale,s=new Gtk.Snapshot();s.scale(scale,scale);paint.snapshot(s,w,h);const node=s.to_node();if(!node)throw new Error(`empty snapshot ${widget.constructor.name} ${width}x${height}, mapped=${widget.get_mapped()}`);const texture=window.get_renderer().render_texture(node,new Graphene.Rect({origin:new Graphene.Point({x:0,y:0}),size:new Graphene.Size({width,height})}));texture.save_to_png(path);return {width,height};};
const control=(parent,label)=>{if(parent.get_tooltip_text()===t(label))return parent;for(let w=parent.get_first_child();w;w=w.get_next_sibling()){const found=control(w,label);if(found)return found;}return null;};
const details=parent=>{const data=[];const walk=w=>{if(w instanceof Gtk.SpinButton)data.push({value:w.value,text:w.get_text(),width:w.get_width(),digits:w.digits,tip:w.get_tooltip_text()});for(let c=w.get_first_child();c;c=c.get_next_sibling())walk(c);};walk(parent);return data;};
let index=0,phase=0,selection,provider,panel;const records=[];
GLib.timeout_add(GLib.PRIORITY_DEFAULT,400,()=>{
 try{
 if(index===cases.length){GLib.file_set_contents(`${directory}/reference-toolbar-gallery.json`,JSON.stringify(records));loop.quit();return GLib.SOURCE_REMOVE;}
  const {dark,pin,tool,name,style,accent,popup,hover,pressed,history}=cases[index];
 if(phase===1&&(popup||hover||pressed)){
  if(popup){const b=control(selection.options.widget,popup);if(!b)throw new Error(`missing ${popup}`);if(b instanceof Gtk.MenuButton){b.popup();panel=b.get_popover();}else{b.active=true;panel=selection.options.controls.fontPopup;}}
  for(const [target,flag] of [[hover,Gtk.StateFlags.PRELIGHT],[pressed,Gtk.StateFlags.ACTIVE]])if(target){const [part,label]=target.split(':');control(part==='toolbar'?selection.toolbar:selection.options.widget,label).set_state_flags(flag,false);}
  phase=2;
 }
 else if(phase>0){
  if(history){const undo=history!=='redo',redo=['redo','both'].includes(history);if(selection.controls.get('undo').sensitive!==undo||selection.controls.get('redo').sensitive!==redo)throw new Error(`incorrect history fixture: ${name}`);if(history==='selected'&&selection.editor.current?.kind!=='rect')throw new Error(`missing selected mark: ${name}`);}
  if(panel)GLib.file_set_contents(`${directory}/reference-${name}-controls.json`,JSON.stringify(details(panel)));
  let nativePanel=null;
  if(panel instanceof Gtk.Popover){
   const s=panel.get_surface(),scale=panel.get_scale_factor(),id=`reference-${name}-panel`,width=s.get_width()*scale,height=s.get_height()*scale;nativePanel={width,height};if(!s.get_mapped())throw new Error(`popup not mapped: ${name}`);
   if(phase!==3){GLib.file_set_contents(`${directory}/gallery-capture-request.json`,JSON.stringify({id,title:'Clivuu reference toolbar gallery',x:s.get_position_x()*scale,y:s.get_position_y()*scale,width,height}));phase=3;return GLib.SOURCE_CONTINUE;}
   if(!GLib.file_test(`${directory}/${id}-done`,GLib.FileTest.EXISTS))return GLib.SOURCE_CONTINUE;
  }
  const content=panel instanceof Gtk.Popover?snapshot(panel.get_child(),`${directory}/reference-${name}-panel-content.png`):null;
  records.push({case:name,toolbar:snapshot(selection.toolbar,`${directory}/reference-${name}-toolbar.png`),options:selection.options.widget.visible?snapshot(selection.options.widget,`${directory}/reference-${name}-options.png`):null,panel:nativePanel??(panel?snapshot(panel,`${directory}/reference-${name}-panel.png`):null),'panel-content':content});index++;phase=0;
 }
 else{
  if(panel){panel.popdown();panel=null;}
  if(selection){selection.options.destroy();root.remove(selection.toolbar);root.remove(selection.options.widget);selection.image.finish();Gtk.StyleContext.remove_provider_for_display(Gdk.Display.get_default(),selection.provider);}
  if(provider)Gtk.StyleContext.remove_provider_for_display(Gdk.Display.get_default(),provider);
  Adw.StyleManager.get_default().set_color_scheme(dark?Adw.ColorScheme.FORCE_DARK:Adw.ColorScheme.FORCE_LIGHT);
  const full={x:0,y:0,width:1280,height:800},embedded=pin?{pin:{},pins:{_rect:()=>full,geometry:()=>[full],_inputRegion(){}}}:null;
  selection=new Selection(`${directory}/toolbar-source.png`,[full],[],full,{...(pin?pinEditActions():{}),embedded});
  const theme=toolbarTheme(dark?'dark':'light',false,accent??'#bd93f9');Object.assign(selection,{toolbarInk:theme.ink,toolbarSelectedInk:theme.selectedInk,toolbarAppearance:theme,toolbarThemeName:theme.name});
  for(const w of [selection.toolbar,selection.options.widget]){if(dark)w.add_css_class('clivuu-theme-dark');else w.remove_css_class('clivuu-theme-dark');}
  provider=new Gtk.CssProvider();provider.load_from_string(toolbarControlCss(theme));Gtk.StyleContext.add_provider_for_display(Gdk.Display.get_default(),provider,Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION);
  selection.overlay={views:[{rect:full,fixed:root,window,drawing:new Gtk.DrawingArea()}]};selection.window=window;
  if(history){for(const x of history==='both'?[300,400]:[300])selection.editor.add({...selection.editor.create('rect',[x,300]),points:[[x,300],[x+40,340]]});if(history==='redo'||history==='both')selection.editor.undo();}
  selection._tool(tool);
  if(history==='selected'){selection.editor.undo();selection.editor.redo();}
  if(style)selection.editor.change(history==='selected'?'rect':tool,style);
  selection._update();
  root.move(selection.toolbar,100,100);root.move(selection.options.widget,100,150);phase=1;
 }
 return GLib.SOURCE_CONTINUE;
 }catch(e){printerr(e.stack);GLib.file_set_contents(`${directory}/reference-toolbar-gallery.json`,JSON.stringify({error:e.message}));loop.quit();return GLib.SOURCE_REMOVE;}
});loop.run();window.destroy();

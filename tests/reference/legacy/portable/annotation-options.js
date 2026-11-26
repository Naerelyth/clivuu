import Gtk from 'gi://Gtk?version=4.0';
import {InlinePopup} from './inline-popup.js';
import Gdk from 'gi://Gdk?version=4.0';
import Pango from 'gi://Pango';
import PangoCairo from 'gi://PangoCairo';
import {t} from '../common/i18n.js';
import {DRAW_TOOLS,PALETTE} from '../common/annotation-model.js';
import {drawToolIcon} from './tool-icon.js';
import {toolOptions,END_STYLES,LINE_STYLES,endStyle,optionLabel,MOSAIC_EFFECTS,MOSAIC_LEVEL,mosaicLevel,fillIcon,fillLabel} from '../common/annotation-options-model.js';
import {drawLineOption,drawEffectIcon,drawStrokeIndicator,drawEraserOption} from '../common/annotation-render.js';
import {colorEditor} from '../common/color-editor.js';
import {colorWidgets} from './color-widgets.js';
import {TEXT_OUTLINES,TEXT_SIZE,toggleTextOutline,textSizeValue,textSizeLabel} from '../common/text-style.js';
import {TEXT_OPTION_ICON_SIZE,fillOptionInk} from '../common/toolbar-theme.js';

let fontNames;
// Compact secondary strip: width sample, pictographic modes, current color,
// two rows of swatches. Numeric controls live in child popovers, never windows.
// Capture and pin editing instantiate exactly this same adapter.
export function annotationOptions(owner){
    const row=new Gtk.Box({spacing:3,valign:Gtk.Align.CENTER});row.add_css_class('clivuu-annotation-options');let refreshing=false;
    const tool=()=>owner.editor.current?.kind??owner.tool;
    const style=()=>owner.editor.current??owner.editor.style(tool());
    const change=patch=>{if(!refreshing)owner._changeStyle(patch);};
    const separator=()=>{const widget=new Gtk.Separator({orientation:Gtk.Orientation.VERTICAL,margin_start:3,margin_end:3,margin_top:5,margin_bottom:5});row.append(widget);return widget;};
    const picture=(draw,w=18,h=18)=>{const image=new Gtk.DrawingArea({content_width:w,content_height:h});image.set_draw_func((_a,ctx,width,height)=>draw(ctx,width,height));return image;};
    const toggle=(tip,draw,action)=>{const button=new Gtk.ToggleButton({focusable:false,tooltip_text:tip});button.add_css_class('clivuu-option');button.set_child(picture((ctx,w,h)=>draw(ctx,w,h,button.active?(owner.toolbarSelectedInk??'#282a36'):(owner.toolbarInk??'#404040'))));button.connect('toggled',()=>{if(!refreshing)action(button);});row.append(button);return button;};
    const icon=(name,filled=false)=>(ctx,w,h,ink=owner.toolbarInk??'#404040')=>{ctx.save();drawToolIcon(ctx,w,h,name,ink,filled);ctx.restore();};
    const popupControl=(child,tip,inline=false)=>{
        const button=inline?new Gtk.ToggleButton({focusable:false,tooltip_text:tip}):new Gtk.MenuButton({focusable:false,tooltip_text:tip});button.add_css_class('clivuu-option');
        const popover=inline?new InlinePopup(owner,button,child):new Gtk.Popover({autohide:true,has_arrow:true,child});popover.add_css_class('clivuu-toolbar-popover');if(!inline)button.set_popover(popover);row.append(button);
        popover.connect('closed',()=>owner.inlineText?.focus?.grab_focus());
        const setContent=(content,arrow=true)=>{
            if(!arrow){button.set_child(content);return;}
            const trigger=new Gtk.Box({spacing:2,valign:Gtk.Align.CENTER});trigger.append(content);
            trigger.append(new Gtk.Image({icon_name:'pan-down-symbolic',pixel_size:8,opacity:.8,valign:Gtk.Align.CENTER}));button.set_child(trigger);
        };
        return {button,popover,setContent};
    };
    const width=Gtk.SpinButton.new_with_range(1,16,1);width.tooltip_text=t('Stroke width (image pixels)');width.focusable=false;
    width.connect('value-changed',()=>change({width:width.value}));
    const widthPopup=popupControl(width,t('Stroke width (image pixels)'));
    const widthSample=picture((ctx,w,h)=>drawStrokeIndicator(ctx,w,h,style().width??3,owner.toolbarInk??'#404040'),22,22);widthPopup.setContent(widthSample);
    const fills=new Map();
    for(const value of ['stroke','solid','line','rect','ellipse']){
        const button=toggle(fillLabel(tool(),value),
            (ctx,w,h,ink)=>{ink=fillOptionInk(owner.toolbarAppearance,style().fill===value);if(tool()==='eraser'){drawEraserOption(ctx,w,h,value,style().width,ink);return;}const glyph=fillIcon(tool(),value);icon(glyph.kind,glyph.filled)(ctx,w,h,ink);},button=>change({fill:toolOptions(tool()).fillToggle&&value==='solid'&&!button.active?'stroke':value}));fills.set(value,button);
    }
    // Keep the semantic selected property for accessibility and integration tests.
    let fillValues=['stroke','solid'];
    const fill={get selected(){return fillValues.indexOf(style().fill);},set selected(value){change({fill:fillValues[value]});}};
    const shapeSeparator=separator();
    const shapes=new Map();
    for(const value of ['rect','ellipse'])shapes.set(value,toggle(value==='rect'?t('Rectangle'):t('Ellipse'),icon(value),()=>owner._shape(value)));
    const lineMenu=(key,values,label,current)=>{
        const content=new Gtk.Box({orientation:Gtk.Orientation.VERTICAL,spacing:2}),popup=popupControl(content,label);
        const draw=value=>(ctx,w,h)=>drawLineOption(ctx,w,h,key,value,owner.toolbarInk??'#404040');
        const sample=picture(draw(current()),40,24);popup.setContent(sample,false);
        sample.set_draw_func((_a,ctx,w,h)=>draw(current())(ctx,w,h));
        for(const value of values){const b=new Gtk.Button({focusable:false,tooltip_text:optionLabel(value)});b.set_child(picture(draw(value),74,28));b.connect('clicked',()=>{change({[key]:value});popup.popover.popdown();});content.append(b);}
        return {...popup,sample,get selected(){return values.indexOf(current());},set selected(i){if(values[i])change({[key]:values[i]});}};
    };
    const startMenu=lineMenu('startStyle',END_STYLES,t('Start marker'),()=>endStyle(style(),true));
    const lineStyle=lineMenu('lineStyle',LINE_STYLES,t('Line style'),()=>style().lineStyle??'solid');
    const endMenu=lineMenu('endStyle',END_STYLES,t('End marker'),()=>endStyle(style()));
    const start={get active(){return endStyle(style(),true)!=='none';},set active(v){change({startStyle:v?'open':'none'});}},end={get active(){return endStyle(style())!=='none';},set active(v){change({endStyle:v?'open':'none'});}};
    const effects=new Map(),effectChoices=new Gtk.Box({orientation:Gtk.Orientation.VERTICAL,spacing:2});let effectPopup;
    for(const value of MOSAIC_EFFECTS){
        const label=value==='blur'?t('Blur'):t('Pixelate');
        const b=toggle(label,(ctx,w,h,ink)=>drawEffectIcon(ctx,w,h,value,ink??owner.toolbarInk??'#404040'),()=>{change({effect:value});effectPopup?.popover.popdown();});row.remove(b);
        const child=new Gtk.Box({spacing:6});child.append(picture((ctx,w,h,ink)=>drawEffectIcon(ctx,w,h,value,ink??owner.toolbarInk??'#404040')));child.append(new Gtk.Label({label}));b.set_child(child);effectChoices.append(b);effects.set(value,b);
    }
    effectPopup=popupControl(effectChoices,t('Mosaic effect'));
    const effectSample=picture((ctx,w,h)=>drawEffectIcon(ctx,w,h,style().effect,owner.toolbarInk??'#404040'),22,22);effectPopup.setContent(effectSample);
    const strength=Gtk.Scale.new_with_range(Gtk.Orientation.HORIZONTAL,MOSAIC_LEVEL.min,MOSAIC_LEVEL.max,1);strength.set_size_request(128,-1);strength.set_draw_value(true);strength.set_digits(0);strength.set_value_pos(Gtk.PositionType.RIGHT);strength.tooltip_text=t('Effect strength');
    let strengthGroup=null;
    const strengthInput=new Gtk.EventControllerLegacy({propagation_phase:Gtk.PropagationPhase.CAPTURE});
    strengthInput.connect('event',(_controller,event)=>{
        const type=event.get_event_type();
        if([Gdk.EventType.BUTTON_PRESS,Gdk.EventType.TOUCH_BEGIN].includes(type))strengthGroup={};
        if([Gdk.EventType.BUTTON_RELEASE,Gdk.EventType.TOUCH_END,Gdk.EventType.TOUCH_CANCEL].includes(type))strengthGroup=null;
        return false;
    });strength.add_controller(strengthInput);
    strength.connect('value-changed',()=>{const level=mosaicLevel(strength.get_value());if(!refreshing&&level!==mosaicLevel(style().effectLevel))owner._changeStyle({effectLevel:level},strengthGroup);});row.append(strength);
    fontNames??=[...new Set(['Sans','Serif','Monospace',...PangoCairo.FontMap.get_default().list_families().map(f=>f.get_name()).sort()])];
    const fontSearch=new Gtk.SearchEntry({placeholder_text:t('Font family')}),fontList=new Gtk.ListBox({selection_mode:Gtk.SelectionMode.SINGLE,activate_on_single_click:true}),fontRows=[];
    const fontScroll=new Gtk.ScrolledWindow({child:fontList,hscrollbar_policy:Gtk.PolicyType.NEVER,vscrollbar_policy:Gtk.PolicyType.AUTOMATIC,min_content_width:240,max_content_height:240,propagate_natural_height:true});
    const fontBox=new Gtk.Box({orientation:Gtk.Orientation.VERTICAL,spacing:4});fontBox.append(fontSearch);fontBox.append(fontScroll);
    const fontPopup=popupControl(fontBox,t('Font family'),true),fontLabel=new Gtk.Label({width_chars:10,max_width_chars:10,ellipsize:Pango.EllipsizeMode.END});fontPopup.setContent(fontLabel);
    const font={get selected(){return Math.max(0,fontNames.indexOf(style().fontFamily??'Sans'));},set selected(index){if(!refreshing&&fontNames[index])change({fontFamily:fontNames[index]});}};
    const chooseFont=item=>{if(!item)return;font.selected=item.fontIndex;fontPopup.popover.popdown();};
    fontList.connect('row-activated',(_list,item)=>chooseFont(item));
    fontList.set_filter_func(item=>fontNames[item.fontIndex].toLocaleLowerCase().includes(fontSearch.text.toLocaleLowerCase()));
    fontSearch.connect('stop-search',()=>fontPopup.popover.popdown());
    fontSearch.connect('search-changed',()=>fontList.invalidate_filter());fontSearch.connect('activate',()=>{fontList.invalidate_filter();chooseFont(fontRows.find(item=>item.get_child_visible()));});
    fontPopup.popover.connect('notify::visible',()=>{
        if(!fontPopup.popover.visible)return;
        if(!fontRows.length)for(const [index,name] of fontNames.entries()){
            const item=new Gtk.ListBoxRow({activatable:true,selectable:true,child:new Gtk.Label({label:name,xalign:0,ellipsize:Pango.EllipsizeMode.END,max_width_chars:28,margin_top:3,margin_bottom:3})});
            item.fontIndex=index;fontList.append(item);fontRows.push(item);
        }
        fontSearch.text='';fontList.invalidate_filter();fontList.select_row(fontRows[font.selected]);
    });
    fontPopup.popover.connect('map',()=>fontSearch.grab_focus());
    const size=Gtk.SpinButton.new_with_range(TEXT_SIZE.min,TEXT_SIZE.max,1);size.set_digits(2);size.focusable=false;size.tooltip_text=t('Font size');
    const sizePopup=popupControl(size,t('Font size')),sizeLabel=new Gtk.Label({width_chars:2,valign:Gtk.Align.CENTER,halign:Gtk.Align.CENTER,yalign:.5});sizePopup.setContent(sizeLabel);sizePopup.button.valign=Gtk.Align.CENTER;sizePopup.button.add_css_class('clivuu-font-size');size.connect('value-changed',()=>change({fontSizeValue:size.value}));
    const textStyleSeparator=separator();
    const bold=toggle(t('Bold'),icon('text-bold'),b=>change({bold:b.active})),italic=toggle(t('Italic'),icon('text-italic'),b=>change({italic:b.active}));
    const textOutlineSeparator=separator();
    const outline=toggle(t('Text outline'),icon('text-outline'),()=>change({outlineMode:toggleTextOutline(style())}));
    for(const b of [bold,italic,outline]){const image=b.get_child();image.set_content_width(TEXT_OPTION_ICON_SIZE);image.set_content_height(TEXT_OPTION_ICON_SIZE);image.halign=Gtk.Align.CENTER;image.valign=Gtk.Align.CENTER;}
    const outlineBox=new Gtk.Box({orientation:Gtk.Orientation.VERTICAL,spacing:6,margin_start:6,margin_end:6,margin_top:6,margin_bottom:6}),outlineModes=new Map();
    const outlineWidth=Gtk.SpinButton.new_with_range(0,8,.5);outlineWidth.tooltip_text=t('Outline width');outlineWidth.connect('value-changed',()=>change({outlineWidth:outlineWidth.value}));outlineBox.append(outlineWidth);
    for(const mode of TEXT_OUTLINES){
        const label={auto:t('Automatic'),white:t('White'),black:t('Black'),custom:t('Custom color')};
        const b=new Gtk.CheckButton({label:label[mode]});b.connect('toggled',()=>{if(!refreshing&&b.active)change({outlineMode:mode});});outlineBox.append(b);outlineModes.set(mode,b);
    }
    const outlineMenu=popupControl(outlineBox,t('Outline options'));outlineMenu.setContent(new Gtk.Image({icon_name:'pan-down-symbolic',pixel_size:12}),false);
    const outlineChooser=colorEditor(colorWidgets,{alphaEnabled:false,style:()=>({color:style().outlineColor??'#000000',alpha:255}),settings:owner.appearanceSettings,apply:value=>change({outlineMode:'custom',outlineColor:value.color}),close:()=>outlineMenu.popover.popdown()});outlineBox.append(outlineChooser.widget);
    outlineMenu.popover.connect('notify::visible',()=>{if(outlineMenu.popover.visible)outlineChooser.open();else outlineChooser.dismiss();});
    const colorSeparator=separator();let colorPopup;
    const chooser=colorEditor(colorWidgets,{style,settings:owner.appearanceSettings,apply:change,close:()=>colorPopup.popover.popdown()});
    const custom=chooser.html;colorPopup=popupControl(chooser.widget,t('Choose color'));
    colorPopup.popover.connect('notify::visible',()=>{if(colorPopup.popover.visible)chooser.open();else chooser.dismiss();});
    const currentColor=picture((ctx,w,h)=>{ctx.setSourceRGB(...[1,3,5].map(i=>parseInt((style().color??owner.color).slice(i,i+2),16)/255));ctx.rectangle(2.5,2.5,w-5,h-5);ctx.fillPreserve();ctx.setSourceRGB(...[1,3,5].map(i=>parseInt((owner.toolbarInk??'#404040').slice(i,i+2),16)/255));ctx.setLineWidth(1);ctx.stroke();
        for(let i=0;i<4;i++){ctx.setSourceRGB(...[[1,0,0],[1,1,0],[0,1,0],[0,.4,1]][i]);ctx.moveTo(w-3,h-3);ctx.lineTo(w-3-i*2,h-11+i*2);ctx.lineTo(w-5-i*2,h-9+i*2);ctx.closePath();ctx.fill();}},22,22);
    colorPopup.setContent(currentColor,false);colorPopup.button.add_css_class('clivuu-current-color');
    colorPopup.button.set_size_request(24,24);colorPopup.button.valign=Gtk.Align.CENTER;
    const colors=new Gtk.Grid({row_spacing:0,column_spacing:2}),swatches=[];row.append(colors);
    for(const [index,color] of PALETTE.entries()){
        const button=new Gtk.Button({focusable:false,tooltip_text:color});button.add_css_class('clivuu-swatch');
        button.set_child(picture((ctx,w,h)=>{ctx.setSourceRGB(...[1,3,5].map(i=>parseInt(color.slice(i,i+2),16)/255));ctx.rectangle(0,0,w,h);ctx.fill();},10,10));
        button.connect('clicked',()=>change({color}));colors.attach(button,index%10,Math.floor(index/10),1,1);swatches.push([button,color]);
    }
    const scroll=new Gtk.ScrolledWindow({hscrollbar_policy:Gtk.PolicyType.AUTOMATIC,vscrollbar_policy:Gtk.PolicyType.NEVER,child:row,propagate_natural_width:true,max_content_width:1000});
    return {widget:scroll,active:()=>DRAW_TOOLS.includes(tool()),destroy:()=>{fontPopup.popover.dispose();chooser.destroy();outlineChooser.destroy();},popupWidgets:()=>fontPopup.popover.visible?[fontPopup.popover]:[],popupAt:(x,y)=>fontPopup.popover.containsPoint(x,y),reposition:()=>fontPopup.popover.reposition(),closePopups:()=>fontPopup.popover.popdown(),handleKey:key=>{if(key===Gdk.KEY_Escape&&fontPopup.popover.visible){fontPopup.popover.popdown();return true;}return false;},handlesKey:()=>[widthPopup,fontPopup,sizePopup,colorPopup,startMenu,lineStyle,endMenu,effectPopup,outlineMenu].some(p=>p.popover.visible),controls:{fontButton:fontPopup.button,fontPopup:fontPopup.popover,fontSearch,fontList,width,fill,fillButtons:fills,shapeButtons:shapes,colorButton:colorPopup.button,start,end,startMenu,endMenu,lineStyle,strength,colorEditor:chooser,effectButtons:effects,effectPopup:effectPopup.popover,font,size,bold,italic,outline,outlineWidth,outlineModes,outlineMenu,custom,colorPopup:colorPopup.popover,widthPopup:widthPopup.popover},update(){
        refreshing=true;
        const kind=tool(),value=style(),spec=toolOptions(kind,value);if(kind!=='text')fontPopup.popover.popdown();scroll.visible=DRAW_TOOLS.includes(kind);
        const range=owner.editor.strokeRange(kind);width.set_range(range.min,range.max);width.set_increments(range.step,range.step*5);
        widthPopup.button.visible=!['text','mosaic','eraser'].includes(kind);width.value=value.width??3;widthSample.queue_draw();
        fillValues=spec.fills;
        shapeSeparator.visible=spec.shapes.length>0;
        for(const [name,button] of fills){button.visible=fillValues.includes(name)&&!(spec.fillToggle&&name==='stroke');button.active=name===value.fill;button.tooltip_text=fillLabel(kind,name);button.get_child().queue_draw();}
        for(const [name,button] of shapes){button.visible=spec.shapes.includes(name);button.active=name===kind;button.get_child().queue_draw();}
        for(const menu of [startMenu,lineStyle,endMenu]){menu.button.visible=spec.arrow;menu.sample.queue_draw();}
        effectPopup.button.visible=spec.effects.length>0;
        for(const [name,button] of effects){button.visible=spec.effects.includes(name);button.active=name===value.effect;button.get_child().get_first_child().queue_draw();}effectSample.queue_draw();
        strength.visible=spec.strength;strength.set_value(mosaicLevel(value.effectLevel));
        colorSeparator.visible=colorPopup.button.visible=colors.visible=spec.color;
        for(const widget of [fontPopup.button,sizePopup.button,bold,italic,outline,outlineMenu.button,textStyleSeparator,textOutlineSeparator])widget.visible=kind==='text';
        outline.active=Boolean(value.outlineMode&&value.outlineMode!=='none');outline.get_child().queue_draw();outlineWidth.value=value.outlineWidth??1;
        for(const [mode,b] of outlineModes)b.active=value.outlineMode===mode;
        outlineChooser.widget.visible=value.outlineMode==='custom';bold.get_child().queue_draw();italic.get_child().queue_draw();
        if(fontRows.length)fontList.select_row(fontRows[Math.max(0,fontNames.indexOf(value.fontFamily??'Sans'))]);fontLabel.label=value.fontFamily??'Sans';size.value=textSizeValue(value,owner.editor.strokeScale);sizeLabel.label=textSizeLabel(size.value);bold.active=Boolean(value.bold);italic.active=Boolean(value.italic);
        if(!colorPopup.popover.visible)custom.text=value.color??owner.color;
        for(const [button,color] of swatches)color===value.color?button.add_css_class('clivuu-swatch-selected'):button.remove_css_class('clivuu-swatch-selected');
        currentColor.queue_draw();refreshing=false;
    }};
}

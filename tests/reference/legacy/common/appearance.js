import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import {toolbarTheme,accentColor} from './toolbar-theme.js';

const namespace='org.freedesktop.appearance';
const unpack=value=>{while(value instanceof GLib.Variant)value=value.deep_unpack();return value;};
// The portal is read-only desktop integration. Clivuu preferences remain in INI.
// Cache one asynchronous subscription per process, including name-owner changes.
class SystemAppearance {
    constructor(){
        this.dark=false;this.listeners=new Set();this.serial=0;
        Gio.DBusProxy.new_for_bus(Gio.BusType.SESSION,Gio.DBusProxyFlags.DO_NOT_AUTO_START,null,
            'org.freedesktop.portal.Desktop','/org/freedesktop/portal/desktop','org.freedesktop.portal.Settings',null,(_source,result)=>{
                try{
                    this.proxy=Gio.DBusProxy.new_for_bus_finish(result);
                    this.proxy.connect('g-signal',(_p,_sender,name,args)=>{
                        if(name!=='SettingChanged')return;
                        const [ns,key,value]=args.deep_unpack();
                        if(ns===namespace&&key==='color-scheme'){this.serial++;this.publish(unpack(value));}
                    });
                    this.proxy.connect('notify::g-name-owner',()=>this.read());this.read();
                }catch(_error){/* A desktop without an appearance portal uses light. */}
            });
    }
    publish(value){const dark=value===1;if(dark===this.dark)return;this.dark=dark;this.listeners.forEach(callback=>callback());}
    read(){
        const serial=++this.serial;
        if(!this.proxy.g_name_owner){this.publish(0);return;}
        this.proxy.call('Read',new GLib.Variant('(ss)',[namespace,'color-scheme']),Gio.DBusCallFlags.NONE,2000,null,(proxy,result)=>{
            try{const [value]=proxy.call_finish(result).deep_unpack();if(serial===this.serial)this.publish(unpack(value));}catch(_error){if(serial===this.serial)this.publish(0);}
        });
    }
}
let system;
export function observeToolbarTheme(settings,apply){
    system??=new SystemAppearance();
    let previousKey;
    const update=()=>{
        const preference=settings?.get_string('theme')??'system',theme=toolbarTheme(preference,system.dark,accentColor(settings)),key=`${preference}:${theme.name}:${theme.selected}`;
        if(key===previousKey)return;
        previousKey=key;apply(theme);
    };
    system.listeners.add(update);
    const signal=settings?.connect('changed',(_s,key)=>{if(['theme','accent-color'].includes(key))update();});
    update();
    return ()=>{system.listeners.delete(update);if(signal)settings.disconnect(signal);};
}

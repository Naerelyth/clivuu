import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import {setLocale} from './i18n.js';

const directory = Gio.File.new_for_uri(import.meta.url).get_parent().get_path();
let catalog;

export function initI18n(settings = null) {
    catalog ??= JSON.parse(new TextDecoder().decode(GLib.file_get_contents(`${directory}/locales/zh-CN.json`)[1]));
    const preference = GLib.getenv('CLIVUU_LANGUAGE') || settings?.get_string('language') || 'system';
    const language = preference === 'system' ? GLib.get_language_names()[0] : preference;
    setLocale(language, catalog);
}

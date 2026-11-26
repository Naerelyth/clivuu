import Adw from 'gi://Adw';
export function applyGtkTheme(settings){
    const scheme=({light:Adw.ColorScheme.FORCE_LIGHT,dark:Adw.ColorScheme.FORCE_DARK})[settings?.get_string('theme')]??Adw.ColorScheme.DEFAULT;
    Adw.StyleManager.get_default().set_color_scheme(scheme);
}

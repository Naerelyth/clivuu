// Shared capture/pin toolbar tokens; geometry does not change with the theme.
export const DEFAULT_ACCENT='#bd93f9';
export const accentColor=settings=>{const value=settings?.values?.['accent-color']??settings?.get_string?.('accent-color');return /^#[0-9a-f]{6}$/i.test(value)?value.toLowerCase():DEFAULT_ACCENT;};
export const accentInk=color=>{const rgb=[1,3,5].map(i=>parseInt(color.slice(i,i+2),16)/255).map(v=>v<=.04045?v/12.92:((v+.055)/1.055)**2.4);return rgb[0]*.2126+rgb[1]*.7152+rgb[2]*.0722>.179?'#282a36':'#f8f8f2';};
export const TOOLBAR_HEIGHT=32;
export const TEXT_OPTION_ICON_SIZE=14;
// Filled mode glyphs retain their light ink when selected in dark appearance.
export const fillOptionInk=(theme,active=false)=>active&&theme?.name!=='dark'?(theme?.selectedInk??'#282a36'):(theme?.ink??'#404040');
export const TOOLBAR_THEMES = {
    light: {name:'light',background:'#e6e6e6',ink:'#404040',anchor:'#000000',border:'#999999',hover:'#d1d1d1',selected:DEFAULT_ACCENT,selectedInk:accentInk(DEFAULT_ACCENT),field:'#ffffff',divider:'#9b9b9b'},
    dark: {name:'dark',background:'#282a36',ink:'#f8f8f2',anchor:'#ffffff',border:'#625578',hover:'#44475a',selected:DEFAULT_ACCENT,selectedInk:accentInk(DEFAULT_ACCENT),field:'#21222c',divider:'#625578'},
};
export function toolbarTheme(preference,systemDark=false,accent=DEFAULT_ACCENT){
    const base=TOOLBAR_THEMES[preference==='dark'||(preference!=='light'&&systemDark)?'dark':'light'];
    return accent===DEFAULT_ACCENT?base:{...base,selected:accent,selectedInk:accentInk(accent)};
}

// Scoped GTK popovers keep capture/pin controls independent of the host theme.
export function toolbarControlCss(theme){
    const {background,ink,border,hover,selected,selectedInk,field,divider}=theme;
    const bar='.clivuu-capture-toolbar',pop='popover.clivuu-toolbar-popover';
    const css=`
        ${bar} button.suggested-action:not(.clivuu-color-action), ${bar} button:checked,
        ${bar}.clivuu-theme-dark button.suggested-action:not(.clivuu-color-action), ${bar}.clivuu-theme-dark button:checked { background:${selected};color:${selectedInk}; }
        ${bar} scale highlight { background:${selected}; }
        ${pop} { color:${ink};font-size:12px; }
        ${pop} > contents, ${pop} popover > contents { background:${background};color:${ink};border:1px solid ${border};border-radius:6px;padding:6px;box-shadow:0 3px 9px #0005; }
        ${pop} arrow { background:${background};border-color:${border}; }
        ${pop} label { color:inherit; }
        ${pop} button:not(.clivuu-color-action) { background:transparent;color:${ink};border:0;border-radius:3px;box-shadow:none;min-height:22px;padding:3px 6px; }
        ${pop} button:not(.clivuu-color-action):hover { background:${hover}; }
        ${pop} button:checked, ${pop} button.suggested-action { background:${selected};color:${selectedInk}; }
        ${pop} entry, ${pop} spinbutton, ${pop} dropdown > button { background:${field};color:${ink};border:1px solid ${border};border-radius:3px;box-shadow:none;min-height:24px; }
        ${pop} entry:focus-within, ${pop} spinbutton:focus-within { border-color:${selected}; }
        ${pop} spinbutton text { color:${ink};background:transparent;padding:0 4px; }
        ${pop} spinbutton button { min-width:14px;padding:0 3px; }
        ${pop} listview, ${pop} list { background:${field};color:${ink}; }
        ${pop} row { color:${ink};padding:3px 6px;border-radius:2px; }
        ${pop} row:hover { background:${hover}; }
        ${pop} row:selected { background:${selected};color:${selectedInk}; }
        ${pop} separator { background:${divider};min-height:1px; }
        ${pop} checkbutton check:checked, ${pop} scale highlight { background:${selected};color:${selectedInk};border-color:${selected}; }
        ${pop} .clivuu-color-action { background:${hover};color:${ink};border:1px solid ${border};border-radius:5px;min-height:24px;padding:4px 12px;font-weight:600; }
        ${pop} .clivuu-color-action.suggested-action { background:${selected};color:${selectedInk};border-color:${selected}; }
    `;
    return css+css.slice(css.indexOf(`${pop} {`)).replaceAll(pop,'.clivuu-inline-popover')+`
        .clivuu-inline-popover { background:${background};color:${ink};border:1px solid ${border};border-radius:6px;padding:6px;box-shadow:0 3px 9px #0005; }
    `;
}

// Toolkit-independent translation and interpolation, also exercised under Node.
let locale = 'en';
let catalog = {};

export function setLocale(language, translations = {}) {
    locale = /^zh(?:[-_]|$)/i.test(language) ? 'zh-CN' : 'en';
    catalog = translations;
}

export function t(message, ...values) {
    const translated = locale === 'zh-CN' ? catalog[message] ?? message : message;
    return translated.replace(/\{(\d+)\}/g, (match, index) => index < values.length ? String(values[index]) : match);
}

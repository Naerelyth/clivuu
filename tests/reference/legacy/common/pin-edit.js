import GdkPixbuf from 'gi://GdkPixbuf';
import GLib from 'gi://GLib';
import {t} from './i18n.js';

export function pinEditContinues(action) {
    return ['clipboard','quick-save','ocr_clipboard','rotate-right','rotate-left','flip-horizontal','flip-vertical'].includes(action);
}

export function pinEditActions() {
    return {defaultAction: 'replace',
        extraActions: [['rotate-right', t("Rotate right")], ['rotate-left', t("Rotate left")],
            ['flip-horizontal', t("Flip horizontally")], ['flip-vertical', t("Flip vertically")]],
        actions: [['replace', t("Apply changes")], ['clipboard', t("Copy image")],
            ['quick-save', t("Save image")], ['ocr_clipboard', t("Recognize text")], [null, t("Cancel")]]};
}


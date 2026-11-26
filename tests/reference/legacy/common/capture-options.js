import {accentColor} from './toolbar-theme.js';
// One snapshot per capture; CLI and preferences share core's validated schema.
import {t} from './i18n.js';

// Reference behavior: hover before selecting, initial rectangle drag and edge
// resizing have a lens. A completed selection, movement and annotation do not.
export function showCaptureMagnifier(selected, drag) {
    return !selected || drag?.kind==='new' || drag?.kind==='resize';
}

export function captureOptions(settings) {
    const value = (key, fallback) => settings?.values?.[`capture-${key}`] ?? fallback;
    return {
        borderWidth: value('border-width', 2), borderColor: accentColor(settings),
        maskColor: value('mask-color', '#000000'), maskOpacity: value('mask-opacity', 44) / 100,
        anchors: value('show-anchors', true), anchorColor: value('anchor-color', '#ffffff'),
        magnifierMask: value('magnifier-mask', true), magnifierBorder: value('magnifier-border', true),
        magnifierAnchors: value('magnifier-anchors', false), crosshair: value('crosshair', false),
        disableGuides: value('disable-guides', false), tips: value('show-tips', true),
        detectWindows: value('detect-windows', true), hide1x1: value('hide-1x1', true),
        cursor: value('cursor', false), hidePins: value('hide-pins', true),
        abortOnFocusOut: value('abort-on-focus-out', false), historyLoop: value('history-loop', false),
        historyAreas: value('history-areas', 8), historyRecords: value('history-records', 20),
    };
}

// Appearance follows the shared configuration even while a selection is open.
// Acquisition settings (cursor, hidden pins, etc.) belong to the frozen image.
export function observeCaptureAppearance(settings, apply) {
    if (!settings) return () => {};
    const fields = ['borderWidth','borderColor','maskColor','maskOpacity','anchors','anchorColor',
        'magnifierMask','magnifierBorder','magnifierAnchors','crosshair','disableGuides','tips','hide1x1'];
    const refresh = () => {
        const options = captureOptions(settings);
        apply(Object.fromEntries(fields.map(key => [key,options[key]])));
    };
    const id = settings.connect('changed', (_settings,key) => { if(key.startsWith('capture-')||key==='accent-color')refresh(); });
    refresh();
    return () => settings.disconnect(id);
}

export function rgb(hex) { return [1, 3, 5].map(i => parseInt(hex.slice(i, i + 2), 16) / 255); }

export const anchorPoints = r => [[0, 0], [0.5, 0], [1, 0], [1, 0.5], [1, 1], [0.5, 1], [0, 1], [0, 0.5]]
    .map(([x, y]) => [r.x + r.width * x, r.y + r.height * y]);

// Draw the entire stroke OUTSIDE the selected image. A full-screen selection
// has no inset border; the output clip naturally retains the edge anchors.
export function drawCaptureFrame(ctx, r, options, {anchors = true, scale = 1} = {}) {
    const width = options.borderWidth / scale;
    ctx.save(); ctx.setSourceRGB(...rgb(options.borderColor));
    if (width > 0) {
        ctx.setLineWidth(width);
        ctx.rectangle(r.x - width / 2, r.y - width / 2, r.width + width, r.height + width); ctx.stroke();
    }
    if (anchors && options.anchors) for (const [x, y] of anchorPoints(r)) {
        ctx.arc(x, y, 3 / scale, 0, Math.PI * 2);
        ctx.setSourceRGB(...rgb(options.borderColor)); ctx.fillPreserve();
        ctx.setLineWidth(1 / scale); ctx.setSourceRGB(...rgb(options.anchorColor)); ctx.stroke();
    }
    ctx.restore();
}

export function drawCaptureMask(ctx, r, limit, options) {
    const left = Math.max(limit.x, Math.min(limit.x + limit.width, r.x));
    const right = Math.max(left, Math.min(limit.x + limit.width, r.x + r.width));
    const top = Math.max(limit.y, Math.min(limit.y + limit.height, r.y));
    const bottom = Math.max(top, Math.min(limit.y + limit.height, r.y + r.height));
    ctx.setSourceRGBA(...rgb(options.maskColor), options.maskOpacity);
    for (const box of [[limit.x, limit.y, limit.width, top - limit.y],
        [limit.x, bottom, limit.width, limit.y + limit.height - bottom],
        [limit.x, top, left - limit.x, bottom - top],
        [right, top, limit.x + limit.width - right, bottom - top]]) ctx.rectangle(...box);
    ctx.fill();
}

// Lens coordinates are source pixels: the centre cell is [8,9) × [6,7).
// Leave it untouched so the colour swatch agrees with the sampled pixel.
export function drawMagnifierCrosshair(ctx, options) {
    ctx.save(); ctx.setSourceRGBA(...rgb(options.borderColor), 0.35);
    for (const box of [[0, 6, 8, 1], [9, 6, 8, 1], [8, 0, 1, 6], [8, 7, 1, 6]]) ctx.rectangle(...box);
    ctx.fill(); ctx.restore();
}

export function captureLabel(label, tips) {
    return tips ? label : label.replace(/[¹²³⁴⁵⁶⁷⁸]/g, '').replace(/\s(?:[PSOLV]|Esc|↵)$/, '');
}

export function drawCaptureGuides(ctx, point, limit, monitors, options) {
    if (options.disableGuides) return;
    ctx.save(); ctx.setLineWidth(1); ctx.setSourceRGBA(...rgb(options.borderColor), 0.5);
    if (options.crosshair && point) {
        ctx.moveTo(limit.x, point[1] + 0.5); ctx.lineTo(limit.x + limit.width, point[1] + 0.5);
        ctx.moveTo(point[0] + 0.5, limit.y); ctx.lineTo(point[0] + 0.5, limit.y + limit.height); ctx.stroke();
    }
    if (monitors.length > 1) {
        ctx.setSourceRGBA(0.65, 0.8, 1, 0.65);
        for (const m of monitors) ctx.rectangle(m.x, m.y, m.width, m.height);
        ctx.stroke();
    }
    ctx.restore();
}

export function rememberArea(areas, rect, maximum) {
    const different = areas.filter(r => !['x', 'y', 'width', 'height'].every(k => r[k] === rect[k]));
    return maximum ? [{...rect}, ...different].slice(0, maximum) : [];
}

export function historyArea(areas, index, delta, loop) {
    if (!areas.length) return null;
    const next = index + delta;
    return loop ? (next + areas.length) % areas.length : Math.max(0, Math.min(areas.length - 1, next));
}

export function historyImage(history, argument) {
    const index = argument === undefined ? 0 : Number(argument);
    if (!Number.isInteger(index) || index < 0 || index >= history.length || argument === '')
        throw new Error(t('No capture at this history index'));
    return history[history.length - 1 - index];
}

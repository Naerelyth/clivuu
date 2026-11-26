import {t} from './i18n.js';
// All interaction uses compositor logical coordinates. Texture conversion is
// performed once, at export; never apply the pointer monitor's scale globally.
export function clamp(v, lo, hi) {
    return Math.min(Math.max(v, lo), Math.max(lo, hi));
}

export function contains(r, x, y) {
    return x >= r.x && y >= r.y && x < r.x + r.width && y < r.y + r.height;
}

// Hit-test inside the image, leaving the measured outer shadow click-through.
export function pinResizeEdge(r, x, y) {
    if (!contains(r, x, y)) return '';
    const margin = Math.min(7, r.width / 3, r.height / 3);
    const horizontal = x < r.x + margin ? 'w' : x >= r.x + r.width - margin ? 'e' : '';
    const vertical = y < r.y + margin ? 'n' : y >= r.y + r.height - margin ? 's' : '';
    return vertical + horizontal;
}

export function pinResizeCursor(edge) {
    return ({n: 'ns-resize', s: 'ns-resize', w: 'ew-resize', e: 'ew-resize',
        nw: 'nwse-resize', se: 'nwse-resize', ne: 'nesw-resize', sw: 'nesw-resize'})[edge] ?? 'default';
}

// Selection borders are draggable from either side of the stroke. Keep the
// cursor and press hit tests identical, including at negative desktop origins.
export function selectionResizeEdge(r, x, y, margin = 8) {
    if (x < r.x - margin || y < r.y - margin ||
        x > r.x + r.width + margin || y > r.y + r.height + margin) return '';
    const west = Math.abs(x - r.x), east = Math.abs(x - r.x - r.width);
    const north = Math.abs(y - r.y), south = Math.abs(y - r.y - r.height);
    const horizontal = Math.min(west, east) <= margin ? west < east ? 'w' : 'e' : '';
    const vertical = Math.min(north, south) <= margin ? north < south ? 'n' : 's' : '';
    return vertical + horizontal;
}

export function pinSize(baseWidth, baseHeight, zoom) {
    // Bound the long side, not each axis independently: thin images must keep
    // their aspect ratio at creation, wheel zoom and edge resize alike.
    const minimum = Math.max(0.1, 24 / Math.max(baseWidth, baseHeight));
    zoom = clamp(zoom, minimum, Math.max(5, minimum));
    return {zoom, width: Math.max(1, Math.round(baseWidth * zoom)),
        height: Math.max(1, Math.round(baseHeight * zoom))};
}

// Use the drag's initial rectangle throughout. Opposite sides/corners stay
// anchored; a single edge keeps the perpendicular centre fixed. Project corner
// movement onto the image aspect ratio instead of stretching either dimension.
export function resizePin(r, edge, dx, dy, baseWidth, baseHeight,
    initialZoom = baseWidth >= baseHeight ? r.width / baseWidth : r.height / baseHeight) {
    const horizontal = edge.includes('w') || edge.includes('e');
    const vertical = edge.includes('n') || edge.includes('s');
    const dw = edge.includes('w') ? -dx : dx, dh = edge.includes('n') ? -dy : dy;
    const desired = initialZoom + (horizontal && vertical
        ? (baseWidth * dw + baseHeight * dh) / (baseWidth ** 2 + baseHeight ** 2)
        : horizontal ? dw / baseWidth : dh / baseHeight);
    const {zoom, width, height} = pinSize(baseWidth, baseHeight, desired);
    return {zoom, width, height,
        x: Math.round(edge.includes('w') ? r.x + r.width - width : horizontal ? r.x : r.x + (r.width - width) / 2),
        y: Math.round(edge.includes('n') ? r.y + r.height - height : vertical ? r.y : r.y + (r.height - height) / 2)};
}

export function intersect(a, b) {
    const x = Math.max(a.x, b.x), y = Math.max(a.y, b.y);
    return {x, y, width: Math.max(0, Math.min(a.x + a.width, b.x + b.width) - x),
        height: Math.max(0, Math.min(a.y + a.height, b.y + b.height) - y)};
}

export function bounds(rects) {
    const x = Math.min(...rects.map(r => r.x));
    const y = Math.min(...rects.map(r => r.y));
    return {x, y, width: Math.max(...rects.map(r => r.x + r.width)) - x,
        height: Math.max(...rects.map(r => r.y + r.height)) - y};
}

// X11 hosts need transparent space outside the output viewport: Mutter treats
// an exactly monitor/desktop-sized override-redirect window as fullscreen.
// Expand the canvas, never shrink the image or change its coordinate origin.
export function x11PinHostRect(monitor, outputs) {
    const desktop=bounds(outputs),sizes=[...outputs,{x:0,y:0,width:desktop.width,height:desktop.height}];
    let gutter=1;
    while(sizes.some(r=>r.x===monitor.x&&r.y===monitor.y&&r.width===monitor.width+gutter&&r.height===monitor.height+gutter))gutter++;
    return {x:monitor.x,y:monitor.y,width:monitor.width+gutter,height:monitor.height+gutter};
}

export function fromPoints(a, b, limit) {
    return intersect({x: Math.round(Math.min(a[0], b[0])), y: Math.round(Math.min(a[1], b[1])),
        width: Math.max(1, Math.round(Math.abs(a[0] - b[0]))),
        height: Math.max(1, Math.round(Math.abs(a[1] - b[1])))}, limit);
}

// Rectangles use exclusive right/bottom edges, whereas a pointer can only
// reach the last pixel. Snap only at exposed monitor edges, never at a seam
// to another output. Keep the original grab offset everywhere else.
function selectionBoundary(point, monitors) {
    const m = monitorAt(monitors, ...point), step = 1 / (m.scale || 1);
    const [x, y] = point, right = m.x + m.width, bottom = m.y + m.height;
    let edgeX = null, edgeY = null;
    const exposed = (x, y) => !monitors.some(other => other !== m && contains(other, x, y));
    if (x <= m.x + step && exposed(m.x - step, y)) edgeX = m.x;
    else if (x >= right - step && exposed(right, y)) edgeX = right;
    if (y <= m.y + step && exposed(x, m.y - step)) edgeY = m.y;
    else if (y >= bottom - step && exposed(x, bottom)) edgeY = bottom;
    return [edgeX, edgeY];
}

export function selectionPoint(point, monitors) {
    return selectionBoundary(point, monitors).map((edge, i) => edge ?? point[i]);
}

export function resizeSelection(drag, point, monitors, limit) {
    const r = drag.rect, [w, n, e, s] = drag.edge;
    const [edgeX, edgeY] = selectionBoundary(point, monitors);
    const dx = point[0] - drag.point[0], dy = point[1] - drag.point[1];
    return fromPoints([w ? edgeX ?? r.x + dx : r.x, n ? edgeY ?? r.y + dy : r.y],
        [e ? edgeX ?? r.x + r.width + dx : r.x + r.width,
            s ? edgeY ?? r.y + r.height + dy : r.y + r.height], limit);
}

export function textureRect(r, scale, origin = {x: 0, y: 0}) {
    const x = Math.round((r.x - origin.x) * scale);
    const y = Math.round((r.y - origin.y) * scale);
    return [x, y, Math.round((r.x + r.width - origin.x) * scale) - x,
        Math.round((r.y + r.height - origin.y) * scale) - y];
}

// Preserve fractional logical pointer positions until this single pixel mapping
// so 2× outputs can address odd pixel columns/rows as well as even ones.
export function imagePixel(point, rect, width, height) {
    return [clamp(Math.floor((point[0] - rect.x) * width / rect.width), 0, width - 1),
        clamp(Math.floor((point[1] - rect.y) * height / rect.height), 0, height - 1)];
}

// During a crop drag the lens identifies the pixel at the moving edge, not
// the arbitrary spot within the resize handle at which the press happened.
// Use the same rounded, exclusive image bounds as export. East/south select
// the last INCLUDED pixel; west/north select the first included pixel.
export function magnifierPixel(point, selection, limit, width, height, drag = null) {
    const pixel = imagePixel(point, limit, width, height);
    if (!drag || !['new', 'resize'].includes(drag.kind)) return pixel;
    const sx = width / limit.width, sy = height / limit.height;
    const start = drag.point, r = drag.rect;
    const sides = drag.kind === 'new' ? [true, true] :
        [drag.edge[0] || drag.edge[2], drag.edge[1] || drag.edge[3]];
    const opposite = drag.kind === 'new' ? start :
        [drag.edge[0] ? r.x + r.width : r.x, drag.edge[1] ? r.y + r.height : r.y];
    const moving = drag.kind === 'new' ? point :
        [ (drag.edge[0] ? r.x : r.x + r.width) + point[0] - start[0],
            (drag.edge[1] ? r.y : r.y + r.height) + point[1] - start[1]];
    for (let axis = 0; axis < 2; axis++) {
        if (!sides[axis]) continue;
        const origin = axis === 0 ? limit.x : limit.y, scale = axis === 0 ? sx : sy;
        const first = axis === 0 ? selection.x : selection.y;
        const length = axis === 0 ? selection.width : selection.height;
        const far = moving[axis] >= opposite[axis];
        pixel[axis] = clamp(Math.round((first + (far ? length : 0) - origin) * scale) - (far ? 1 : 0),
            0, (axis === 0 ? width : height) - 1);
    }
    return pixel;
}

export function magnifierSelectionRect(selection, limit, width, height, px, py) {
    const x = Math.round((selection.x - limit.x) * width / limit.width);
    const y = Math.round((selection.y - limit.y) * height / limit.height);
    return {x: x - px + 8, y: y - py + 6,
        width: Math.round((selection.x + selection.width - limit.x) * width / limit.width) - x,
        height: Math.round((selection.y + selection.height - limit.y) * height / limit.height) - y};
}

// X root coordinates use X pixels and may have a different origin from
// Mutter's desktop. Map through the complete X/GDK layout, not the primary
// monitor or a guessed zero desktop origin.
export function x11DesktopPoint(point, rootRect, rootScale, desktopRect) {
    return [desktopRect.x + (point[0] / rootScale - rootRect.x) * desktopRect.width / rootRect.width,
        desktopRect.y + (point[1] / rootScale - rootRect.y) * desktopRect.height / rootRect.height];
}

export function cursorImageRect(cursor, rootRect, rootScale, imageWidth, imageHeight) {
    const sx = imageWidth / rootRect.width / rootScale, sy = imageHeight / rootRect.height / rootScale;
    return {x: Math.round((cursor.x - cursor.xhot - rootRect.x * rootScale) * sx),
        y: Math.round((cursor.y - cursor.yhot - rootRect.y * rootScale) * sy),
        width: Math.max(1, Math.round(cursor.width * sx)), height: Math.max(1, Math.round(cursor.height * sy))};
}

export function monitorAt(monitors, x, y) {
    return monitors.find(m => contains(m, x, y)) ?? monitors.reduce((best, m) => {
        const distance = r => (clamp(x, r.x, r.x + r.width) - x) ** 2 +
            (clamp(y, r.y, r.y + r.height) - y) ** 2;
        return distance(m) < distance(best) ? m : best;
    });
}

export function selectionPanelLayout(r, viewport, toolbarSize, hintSize) {
    const edge = 1, gap = 8, hintGap = 5, inside = 8;
    const right = viewport.x + viewport.width, bottom = viewport.y + viewport.height;
    const width = Math.min(toolbarSize.width, Math.max(1, viewport.width - 2 * edge));
    const height = Math.min(toolbarSize.height, Math.max(1, viewport.height - 2 * edge));
    const hint = {width: Math.min(hintSize.width, Math.max(1, viewport.width - 2 * edge)),
        height: Math.min(hintSize.height, Math.max(1, viewport.height - 2 * edge))};
    hint.x = clamp(r.x, viewport.x + edge, right - hint.width - edge);
    const hintAbove = r.y - hintGap - hint.height;
    hint.y = hintAbove >= viewport.y + edge ? hintAbove
        : clamp(r.y + hintGap, viewport.y + edge, bottom - hint.height - edge);
    const x = clamp(r.x + r.width - width, viewport.x + edge, right - width - edge);
    const below = r.y + r.height + gap;
    const above = Math.min(r.y, hint.y) - gap - height;
    let y, placement;
    if (below + height <= bottom - edge) { y = below; placement = 'below'; }
    else if (above >= viewport.y + edge) { y = above; placement = 'above'; }
    else {
        // Match the reference's strict comparison: prefer the top only when
        // it has more free space. Equal space, including full-screen, is bottom.
        const topSpace = Math.max(0, r.y - viewport.y);
        const bottomSpace = Math.max(0, bottom - r.y - r.height);
        placement = topSpace > bottomSpace ? 'inside-top' : 'inside-bottom';
        y = placement === 'inside-top' ? r.y + inside : r.y + r.height - height - inside;
        y = clamp(y, viewport.y + edge, bottom - height - edge);
    }
    const toolbar = {x, y, width, height};
    const overlap = intersect(toolbar, hint);
    if (overlap.width > 0 && overlap.height > 0) {
        const after = toolbar.y + toolbar.height + hintGap;
        hint.y = after + hint.height <= bottom - edge ? after
            : clamp(toolbar.y - hint.height - hintGap, viewport.y + edge, bottom - hint.height - edge);
    }
    return {toolbar, hint, placement};
}

export function toolbarPosition(r, monitor, width, height) {
    const {toolbar} = selectionPanelLayout(r, monitor, {width, height}, {width: 0, height: 0});
    return [toolbar.x, toolbar.y];
}

// A content-sized sibling of the primary strip, with no separate surface.
export function annotationPanelPosition(toolbar,viewport,size,gap=4){
    const width=Math.min(size.width,Math.max(1,viewport.width-2)),height=Math.min(size.height,Math.max(1,viewport.height-2));
    const below=toolbar.y+toolbar.height+gap,above=toolbar.y-gap-height,bottom=viewport.y+viewport.height-1;
    const y=below+height<=bottom?below:above>=viewport.y+1?above:
        below+height-bottom<viewport.y+1-above?below:above;
    return {x:clamp(toolbar.x,viewport.x+1,viewport.x+viewport.width-width-1),
        y:clamp(y,viewport.y+1,bottom-height),width,height};
}

export function validateRect(rect, monitors) {
    if (!rect || !['x', 'y', 'width', 'height'].every(k => Number.isSafeInteger(rect[k])) ||
        rect.width < 2 || rect.height < 2)
        throw new Error(t("Selection coordinates must be integers, with width and height of at least 2 pixels"));
    const clipped = intersect(rect, bounds(monitors));
    if (clipped.width !== rect.width || clipped.height !== rect.height ||
        !monitors.some(m => intersect(m, rect).width > 0 && intersect(m, rect).height > 0))
        throw new Error(t("The selection is outside the monitor layout"));
    return {...rect};
}

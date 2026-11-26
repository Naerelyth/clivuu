// Measured from paired black/white Snipaste samples. Shared by both renderers.
const PROFILE = [97, 70, 49, 33, 22, 14, 9, 5, 3, 2, 1, 1].map(a => a / 255);
export const RADIUS = PROFILE.length;
export function coverage(position, length) {
    if (position < 0) return PROFILE[-position - 1] ?? 0;
    if (position >= length) return PROFILE[position - length] ?? 0;
    return 1 - (PROFILE[Math.min(position, length - 1 - position)] ?? 0);
}
export function shadowLayout(width, height) {
    const ix = Math.min(RADIUS, Math.floor(width / 2));
    const iy = Math.min(RADIUS, Math.floor(height / 2));
    const cw = RADIUS + ix, ch = RADIUS + iy;
    return [
        [-RADIUS, -RADIUS, cw, ch, 1, 1],
        [width - ix, -RADIUS, cw, ch, 1, 1],
        [-RADIUS, height - iy, cw, ch, 1, 1],
        [width - ix, height - iy, cw, ch, 1, 1],
        [ix, -RADIUS, 1, RADIUS, width - ix * 2, 1],
        [ix, height, 1, RADIUS, width - ix * 2, 1],
        [-RADIUS, iy, RADIUS, 1, 1, height - iy * 2],
        [width, iy, RADIUS, 1, 1, height - iy * 2],
    ];
}

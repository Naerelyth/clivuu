"""Independent area/Gaussian pixel reference for the headless effect backend.

The old GJS/Cairo parity enforced its alpha loss and large-reduction aliasing.
Reference arithmetic here uses floating point and one rounding per filter pass;
Rust uses a unit-sum fixed-point kernel. PNG unpremultiplication adds at most
one channel unit, so the premultiplied comparison allows two units.
"""
import argparse
import json
import math
import os
from pathlib import Path
import subprocess
import tempfile
from PIL import Image


def pixels(image):
    raw = image.tobytes()
    return zip(raw[0::4], raw[1::4], raw[2::4], raw[3::4])


def reference(image, effect, level, scale, roi):
    width, height = image.size
    size = max(1, math.floor(level * scale + .5))
    factor = max(1, size / 3) if effect == 'blur' else size
    columns, rows = math.ceil(width / factor), math.ceil(height / factor)
    sx, sy = (width / columns, height / rows) if effect == 'blur' else (factor, factor)
    halo = math.ceil(size / factor) + 2 if effect == 'blur' else 2
    left = max(0, math.floor(max(0, roi['x']) / sx) - halo)
    top = max(0, math.floor(max(0, roi['y']) / sy) - halo)
    right = min(columns, math.ceil(min(width, roi['x'] + roi['width']) / sx) + halo)
    bottom = min(rows, math.ceil(min(height, roi['y'] + roi['height']) / sy) + halo)
    w, h = right - left, bottom - top
    source = [(tuple((value * a + 127) // 255 for value in (r, g, b)) + (a,))
              for r, g, b, a in pixels(image)]
    result = []
    for y in range(top, bottom):
        ya, yb = y * sy, (y + 1) * sy
        for x in range(left, right):
            xa, xb = x * sx, (x + 1) * sx
            values = [0.] * 4
            for iy in range(min(height - 1, math.floor(ya)), min(height, math.ceil(yb))):
                dy = (yb if iy == height - 1 else min(yb, iy + 1)) - max(ya, iy)
                for ix in range(min(width - 1, math.floor(xa)), min(width, math.ceil(xb))):
                    dx = (xb if ix == width - 1 else min(xb, ix + 1)) - max(xa, ix)
                    for c, value in enumerate(source[iy * width + ix]):
                        values[c] += value * dx * dy
            result.append(tuple(math.floor(value / (sx * sy) + .5) for value in values))
    if effect == 'blur':
        radius = math.ceil(size / factor)
        sigma = max(.5, size / factor / 2.2)
        kernel = [math.exp(-i * i / (2 * sigma * sigma)) for i in range(-radius, radius + 1)]
        total = sum(kernel)
        for vertical in (False, True):
            output = []
            for y in range(h):
                for x in range(w):
                    values = [0.] * 4
                    for i, weight in zip(range(-radius, radius + 1), kernel):
                        xx = x if vertical else min(w - 1, max(0, x + i))
                        yy = min(h - 1, max(0, y + i)) if vertical else y
                        for c, value in enumerate(result[yy * w + xx]):
                            values[c] += value * weight
                    output.append(tuple(math.floor(value / total + .5) for value in values))
            result = output
    return (w, h), result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', default='/tmp/clivuu-target/release/examples/effects_probe')
    args = parser.parse_args()
    env = os.environ.copy()
    env.pop('APPIMAGE', None)
    with tempfile.TemporaryDirectory(prefix='clivuu-effects-parity-', dir='/tmp') as directory:
        work = Path(directory)
        cases, sources = [], {}
        for width, height in [(101, 73), (5, 3), (24001, 1), (1, 24001)]:
            for alpha in (False, True):
                image = Image.new('RGBA', (width, height))
                image.putdata([((x * 37 + y * 17) % 256, (x * 13 + y * 41) % 256,
                                (x * 53 + y * 3) % 256, (x * 19 + y * 7) % 256 if alpha else 255)
                               for y in range(height) for x in range(width)])
                source = str(work / f'source-{width}-{height}-{alpha}.png')
                image.save(source)
                sources[source] = image
                for effect, levels in [('pixelate', [3, 8, 24]), ('blur', [3, 8, 24])]:
                    for level in levels:
                        for scale in (1, 2):
                            for region in (False, True):
                                cases.append(dict(source=source, output=str(work / f'{len(cases)}.png'),
                                    effect=effect, level=level, scale=scale,
                                    roi=dict(x=width * .31 if region else 0, y=height * .27 if region else 0,
                                             width=width * .19 if region else width, height=height * .23 if region else height)))
        subprocess.run([args.binary], input=''.join(json.dumps(c) + '\n' for c in cases),
                       text=True, env=env, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=60)
        maximum = 0
        for case in cases:
            size, expected = reference(sources[case['source']], case['effect'], case['level'], case['scale'], case['roi'])
            actual = Image.open(case['output']).convert('RGBa')
            assert actual.size == size, (case, actual.size, size)
            error = max(abs(a - b) for p, q in zip(pixels(actual), expected) for a, b in zip(p, q))
            assert error <= 2, (case, error)
            maximum = max(maximum, error)
        print(f'PASS {len(cases)} independent area/Gaussian cases; max premultiplied error {maximum}/255')


if __name__ == '__main__':
    main()

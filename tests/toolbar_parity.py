"""Compare actual GJS/native GTK widget captures, including geometry and alpha."""
import json
from pathlib import Path
from PIL import Image


def compare(directory):
    directory = Path(directory)
    reference = json.loads((directory / 'reference-toolbar-gallery.json').read_text())
    native_records = json.loads((directory / 'native-toolbar-gallery.json').read_text())
    names = [item['case'] for item in reference]
    native_names = [item['case'] for item in native_records]
    if not names or len(set(names)) != len(names) or len(set(native_names)) != len(native_names):
        raise ValueError('Toolbar galleries must contain nonempty, unique cases')
    if set(names) != set(native_names):
        raise ValueError('Reference and native toolbar case sets differ')
    native = {item['case']: item for item in native_records}
    results = []
    for item in reference:
        name = item['case']
        if not item.get('toolbar') or not native[name].get('toolbar'):
            raise ValueError(f'Toolbar snapshot missing for {name}')
        for part in ('toolbar', 'options', 'panel', 'panel-content'):
            if part.startswith('panel') and not item.get(part):
                if native[name].get(part) is not None:
                    results.append({'name': f'pixel parity {name} {part}', 'passed': False,
                                    'reference': None, 'native': native[name][part]})
                continue
            expected, actual = item[part], native.get(name, {}).get(part)
            result = {'name': f'pixel parity {name} {part}', 'passed': False}
            if expected is None or actual is None:
                result['passed'] = expected == actual
            elif expected != actual:
                result.update(reference=expected, native=actual)
            else:
                a = Image.open(directory / f'reference-{name}-{part}.png').convert('RGBA')
                b = Image.open(directory / f'native-{name}-{part}.png').convert('RGBA')
                size = (expected['width'], expected['height'])
                if a.size != size or b.size != size:
                    result.update(reference_image_size=a.size, native_image_size=b.size, expected_size=size)
                    results.append(result)
                    continue
                different, maximum, heat = 0, 0, []
                for p, q in zip(a.get_flattened_data(), b.get_flattened_data()):
                    # Hidden RGB in fully transparent pixels has no visual value.
                    premultiplied = lambda v: tuple((c * v[3] + 127) // 255 for c in v[:3]) + (v[3],)
                    error = max(abs(x - y) for x, y in zip(premultiplied(p), premultiplied(q)))
                    different += error != 0
                    maximum = max(maximum, error)
                    heat.append((error, 0, 0, 255))
                result.update(passed=different == 0, different_pixels=different, maximum_channel_error=maximum)
                if different:
                    image = Image.new('RGBA', a.size)
                    image.putdata(heat)
                    image.save(directory / f'diff-{name}-{part}.png')
            results.append(result)
    (directory / 'toolbar-pixel-comparison.json').write_text(json.dumps(results, indent=2))
    return results

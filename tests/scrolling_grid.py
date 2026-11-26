"""Independent full-row identity oracle for real Nautilus long captures.

This deliberately does not invoke the application's matcher. Unique source rows
establish integer movement; known body pixels then verify the exported image.
"""
from collections import Counter
import hashlib
import json
from pathlib import Path
from PIL import Image


def signatures(image):
    width, height = image.size
    data = image.tobytes()
    rows = {y: data[(y * width + 8) * 3:(y * width + width - 20) * 3]
            for y in range(160, height - 40)}
    counts = Counter(rows.values())
    return {hashlib.sha256(row).digest(): y for y, row in rows.items() if counts[row] == 1}


def verify(directory):
    directory = Path(directory)
    trace = json.loads((directory / 'grid-trace.json').read_text())
    results, evidence = [], []
    for case in trace:
        name, samples = case['name'], case['samples']
        frames = [Image.open(s['path']).convert('RGB') for s in samples]
        positions, transitions = [0], []
        previous = signatures(frames[0])
        for frame in frames[1:]:
            current = signatures(frame)
            votes = Counter(previous[key] - current[key] for key in previous.keys() & current.keys())
            ranked = votes.most_common()
            delta, support = ranked[0] if ranked else (0, 0)
            reliable = support >= 8 and all(count < support / 2 for shift, count in ranked[1:] if abs(shift - delta) > 2)
            transitions.append({'delta': delta, 'support': support, 'reliable': reliable})
            positions.append(positions[-1] + delta)
            previous = current
        output = Image.open(directory / f'grid-{name}-output.png').convert('RGB')
        minimum, maximum = min(positions), max(positions)
        expected = frames[0].height + maximum - minimum
        wrong, compared, max_position_error = 0, 0, 0.
        for index, (source, position, sample) in enumerate(zip(frames, positions, samples)):
            state = sample['state']
            # Navigation is compared in source pixels, without an assumed wheel
            # distance or the production matching result as the oracle.
            max_position_error = max(max_position_error, abs(
                state['position'] * state['height'] + min(positions[:index+1]) - position))
            offset = position - minimum
            for y in range(160, source.height - 40, 5):
                for x in range(12, source.width - 24, 7):
                    if 0 <= y + offset < output.height:
                        actual, wanted = output.getpixel((x, y + offset)), source.getpixel((x, y))
                        wrong += any(abs(a - b) > 2 for a, b in zip(actual, wanted))
                    else:
                        wrong += 1
                    compared += 1
        evidence.append({'case': name, 'transitions': transitions, 'positions': positions,
                         'expectedHeight': expected, 'actualHeight': output.height,
                         'wrongPixels': wrong, 'comparedPixels': compared,
                         'maxPositionError': max_position_error})
        results.extend([
            {'name': f'{name}: independent unique-row oracle resolves every wheel transition',
             'passed': all(t['reliable'] for t in transitions)},
            {'name': f'{name}: export dimensions match the actual Nautilus movement',
             'passed': output.size == (frames[0].width, expected)},
            {'name': f'{name}: all sampled body pixels and labels survive export',
             'passed': wrong == 0 and compared > 100000},
            {'name': f'{name}: preview follows the verified source position',
             'passed': max_position_error <= 2},
        ])
    (directory / 'grid-pixels.json').write_text(json.dumps(evidence, indent=2))
    return results

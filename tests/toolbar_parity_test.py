"""Keep the visual migration gate strict, including incomplete fixtures."""
import json
from pathlib import Path
import tempfile
import unittest

from PIL import Image
from toolbar_parity import compare


class ToolbarParityTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='clivuu-pixel-gate-', dir='/tmp')
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.record = {'case': 'dark-pin-rect', 'toolbar': {'width': 2, 'height': 2},
                       'options': None}
        self.gallery('reference', [self.record])
        self.gallery('native', [self.record])
        self.image('reference', (2, 2), (20, 30, 40, 255))
        self.image('native', (2, 2), (20, 30, 40, 255))

    def gallery(self, side, records):
        (self.directory / f'{side}-toolbar-gallery.json').write_text(json.dumps(records))

    def image(self, side, size, color):
        Image.new('RGBA', size, color).save(self.directory / f'{side}-dark-pin-rect-toolbar.png')

    def test_single_channel_single_step_difference_fails(self):
        self.assertTrue(all(r['passed'] for r in compare(self.directory)))
        self.image('native', (2, 2), (21, 30, 40, 255))
        result = compare(self.directory)[0]
        self.assertFalse(result['passed'])
        self.assertEqual(result['maximum_channel_error'], 1)

    def test_invisible_rgb_is_ignored_but_alpha_is_not(self):
        self.image('reference', (2, 2), (20, 30, 40, 0))
        self.image('native', (2, 2), (200, 0, 0, 0))
        self.assertTrue(compare(self.directory)[0]['passed'])
        self.image('native', (2, 2), (200, 0, 0, 1))
        self.assertFalse(compare(self.directory)[0]['passed'])

    def test_png_dimensions_must_match_metadata(self):
        self.image('native', (1, 2), (20, 30, 40, 255))
        self.assertFalse(compare(self.directory)[0]['passed'])
        self.image('reference', (1, 2), (20, 30, 40, 255))
        self.assertFalse(compare(self.directory)[0]['passed'])

    def test_unexpected_panel_and_missing_toolbar_fail(self):
        self.gallery('native', [{**self.record, 'panel': {'width': 2, 'height': 2}}])
        self.assertFalse(all(r['passed'] for r in compare(self.directory)))
        for side in ('reference', 'native'):
            self.gallery(side, [{**self.record, 'toolbar': None}])
        with self.assertRaises(ValueError):
            compare(self.directory)

    def test_empty_missing_extra_and_duplicate_cases_are_rejected(self):
        for reference, native in [([], []), ([self.record], []),
                                  ([], [self.record]),
                                  ([self.record, self.record], [self.record]),
                                  ([self.record], [self.record, self.record]),
                                  ([self.record], [{**self.record, 'case': 'extra'}])]:
            with self.subTest(reference=reference, native=native):
                self.gallery('reference', reference)
                self.gallery('native', native)
                with self.assertRaises(ValueError):
                    compare(self.directory)


if __name__ == '__main__':
    unittest.main()

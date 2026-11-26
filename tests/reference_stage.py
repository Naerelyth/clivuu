"""Stage the test observer and frozen behavioral/visual references.
No production GNOME extension is required or staged.
"""
from pathlib import Path
import json
import shutil
ROOT=Path(__file__).resolve().parents[1]

def stage(binary,destination):
    destination=Path(destination).resolve()
    if not destination.is_relative_to(Path('/tmp')):raise ValueError("Tests must stage under /tmp")
    metadata=ROOT/'tests/gnome-observer-metadata.json'
    extension=destination/json.loads(metadata.read_text())['uuid'];extension.mkdir(parents=True,exist_ok=False)
    shutil.copy2(metadata,extension/'metadata.json')
    reference=ROOT/'tests/reference/legacy'
    for name in ['common','portable']:shutil.copytree(reference/name,extension/name)
    shutil.copytree(ROOT/'core/locales',extension/'common/locales')
    (extension/'bin').mkdir();shutil.copy2(binary,extension/'bin/clivuu')
    icons=extension/'icons';icons.mkdir()
    shutil.copy2(ROOT/'assets/icons/clivoo.svg',icons/'clivoo.svg')
    shutil.copy2(ROOT/'assets/icons/clivoo-transparent.svg',icons/'clivoo-transparent.svg')
    shutil.copy2(ROOT/'assets/icons/github-symbolic.svg',icons/'github-symbolic.svg')
    return extension

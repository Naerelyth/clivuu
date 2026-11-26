#!/usr/bin/env python3
"""Check public Markdown navigation, language parity and configuration examples."""
import json
from pathlib import Path
import re
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parents[1]


def prose(text):
    return re.sub(r'^```[^\n]*\n.*?^```\s*$', '', text, flags=re.M | re.S)


def anchors(text):
    found, counts = set(), {}
    for heading in re.findall(r'^#{1,6}\s+(.+)$', prose(text), re.M):
        slug = re.sub(r'[^\w\-\s]', '', heading.lower()).replace(' ', '-')
        count = counts.get(slug, 0)
        found.add(slug if not count else f'{slug}-{count}')
        counts[slug] = count + 1
    return found


def check_links(path):
    text = prose(path.read_text())
    targets = re.findall(r'\[[^\]\n]*\]\(([^)]+)\)', text)
    targets += re.findall(r'<img\b[^>]*\bsrc="([^"]+)"', text)
    for target in targets:
        if re.match(r'^[a-zA-Z][\w+.-]*:', target):
            continue
        filename, _, fragment = unquote(target.strip('<>')).partition('#')
        linked = (path.parent / filename).resolve() if filename else path
        assert linked.is_relative_to(ROOT), (path, 'link escapes repository', target)
        assert linked.exists(), (path.relative_to(ROOT), 'missing link', target)
        if fragment and linked.suffix == '.md':
            assert fragment in anchors(linked.read_text()), (path.relative_to(ROOT), 'missing anchor', target)


def main():
    en = {p.relative_to(ROOT / 'docs/en') for p in (ROOT / 'docs/en').rglob('*.md')}
    zh = {p.relative_to(ROOT / 'docs/zh-CN') for p in (ROOT / 'docs/zh-CN').rglob('*.md')}
    assert en == zh, (en - zh, zh - en)
    paths = [ROOT / 'README.md', ROOT / 'README.zh-CN.md', ROOT / 'AGENTS.md',
             ROOT / 'agent-docs/feature-contracts.md', *(ROOT / 'docs').rglob('*.md')]
    for path in paths:
        check_links(path)
    for rel in en:
        assert '简体中文' in (ROOT / 'docs/en' / rel).read_text(), rel
        assert '[English]' in (ROOT / 'docs/zh-CN' / rel).read_text(), rel
    schema = json.loads((ROOT / 'core/config-defaults.json').read_text())
    for language in ('en', 'zh-CN'):
        reference = (ROOT / f'docs/{language}/development/configuration.md').read_text()
        for key, entry in schema.items():
            default = json.dumps(entry['default'], ensure_ascii=False, separators=(',', ':'))
            assert f'| `{key}` | `{default}` |' in reference, (language, key)
    instructions = (ROOT / 'AGENTS.md').read_bytes()
    assert len(instructions) <= 8192 and len(instructions.splitlines()) < 200
    print(f'PASS {len(paths)} documentation files, {len(en)} bilingual page pairs, {len(schema)} configuration defaults and agent budget')


if __name__ == '__main__':
    main()

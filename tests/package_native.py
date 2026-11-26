#!/usr/bin/env python3
"""Exercise CMake installation and real package containers with harmless ELF fixtures."""
import argparse
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def run(*args, **kwargs):
    result = subprocess.run(args, text=True, capture_output=True, timeout=90, **kwargs)
    if result.returncode:
        raise AssertionError(f'{args}:\n{result.stdout}\n{result.stderr}')
    return result.stdout


def digest(path):
    return hashlib.sha256(path.read_bytes()).digest()


def verify_tree(prefix, pair):
    assert digest(prefix / 'bin/clivuu') == digest(pair / 'clivuu')
    assert digest(prefix / 'libexec/clivuu-desktop') == digest(pair / 'clivuu-desktop')
    entry = prefix / 'share/applications/io.github.naerelyth.clivuu.desktop'
    text = entry.read_text()
    assert 'Exec=clivuu\n' in text
    assert 'StartupWMClass=io.github.naerelyth.clivuu\n' in text
    assert 'Icon=io.github.naerelyth.clivuu\n' in text
    for name in ('clivoo.svg', 'clivoo-transparent.svg', 'github-symbolic.svg'):
        assert digest(prefix / 'share/clivuu/icons' / name) == digest(ROOT / 'assets/icons' / name)
    assert digest(prefix / 'share/icons/hicolor/scalable/apps/io.github.naerelyth.clivuu.svg') == digest(ROOT / 'assets/icons/clivoo.svg')
    assert digest(prefix / 'share/licenses/clivuu/Octicons-LICENSE.txt') == digest(ROOT / 'assets/icons/github-LICENSE.txt')
    docs = prefix / 'share/doc/clivuu'
    for name in ('README.md', 'README.zh-CN.md', 'LICENSE'):
        assert (docs / name).read_bytes() == (ROOT / name).read_bytes()
    assert (docs / 'assets/icons/clivoo.svg').is_file()
    for path in (ROOT / 'docs').rglob('*.md'):
        assert (docs / 'docs' / path.relative_to(ROOT / 'docs')).read_bytes() == path.read_bytes()
    assert not list(prefix.rglob('*.js'))
    assert not list(prefix.rglob('*.py'))
    assert not list(prefix.rglob('AGENTS.md'))
    assert not list(prefix.rglob('feature-contracts.md'))
    assert not (prefix / 'share/gnome-shell').exists()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--appimage-runtime', type=Path, default=os.environ.get('CLIVUU_TEST_APPIMAGE_RUNTIME'))
    parser.add_argument('--appimagetool', default='appimagetool')
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='clivuu-packaging-', dir='/tmp') as directory:
        work = Path(directory)
        pair = work / 'pair'
        pair.mkdir()
        shutil.copy2('/bin/true', pair / 'clivuu')
        shutil.copy2('/bin/false', pair / 'clivuu-desktop')
        build, output = work / 'build', work / 'packages'
        configure = ['cmake', '-S', str(ROOT), '-B', str(build),
                     f'-DCLIVUU_PREBUILT_DIR={pair}', f'-DCLIVUU_PACKAGE_OUTPUT={output}',
                     '-DCLIVUU_PACKAGE_CONTACT=Packaging Test <packaging@example.invalid>']
        if args.appimage_runtime:
            configure += [f'-DCLIVUU_APPIMAGE_RUNTIME={args.appimage_runtime.resolve()}',
                          f'-DCLIVUU_APPIMAGETOOL={args.appimagetool}']
        run(*configure)
        prefix = work / 'installed'
        run('cmake', '--install', str(build), '--prefix', str(prefix))
        verify_tree(prefix, pair)
        run(str(prefix / 'bin/clivuu'))

        config = str(build / 'CPackConfig.cmake')
        run('cpack', '--config', config, '-G', 'TGZ')
        archive, = output.glob('*.tar.gz')
        unpacked = work / 'tar'
        with tarfile.open(archive) as stream:
            stream.extractall(unpacked, filter='data')
        payload, = unpacked.iterdir()
        verify_tree(payload / 'usr', pair)

        if shutil.which('dpkg-deb') and shutil.which('dpkg-shlibdeps'):
            run('cpack', '--config', config, '-G', 'DEB')
            deb, = output.glob('*.deb')
            requirements = run('dpkg-deb', '-f', str(deb), 'Depends')
            assert 'libgtk-4-1 (>= 4.14)' in requirements
            assert 'gstreamer1.0-pipewire' in requirements and 'libgtk4-layer-shell0' in requirements
            if list(Path('/var/lib/dpkg/info').glob('libc6*.shlibs')):
                assert 'libc6' in requirements
            else:
                print('NOTE DEB: host has no Debian libc6 metadata; auto-dependency resolution needs a Debian build host')
            run('dpkg-deb', '-x', str(deb), str(work / 'deb'))
            verify_tree(work / 'deb/usr', pair)
        else:
            print('SKIP DEB container: dpkg tools unavailable')

        if shutil.which('rpmbuild') and shutil.which('rpm'):
            run('cpack', '--config', config, '-G', 'RPM')
            rpm, = output.glob('*.rpm')
            requirements = run('rpm', '-qp', '--requires', str(rpm))
            assert 'libc.so.6' in requirements and 'gtk4 >= 4.14' in requirements
            assert 'pipewire-gstreamer' in requirements and 'gtk4-layer-shell' in requirements
            listing = run('rpm', '-qlp', str(rpm))
            assert '/usr/bin/clivuu\n' in listing and '/usr/libexec/clivuu-desktop\n' in listing
            assert '/usr/share/doc/clivuu/README.zh-CN.md\n' in listing
            assert '.js\n' not in listing and '.py\n' not in listing
            if shutil.which('rpm2cpio') and shutil.which('cpio'):
                extracted = work / 'rpm'
                extracted.mkdir()
                with tempfile.TemporaryFile() as stream:
                    subprocess.run(['rpm2cpio', str(rpm)], stdout=stream, check=True, timeout=30)
                    stream.seek(0)
                    subprocess.run(['cpio', '-id', '--quiet', '--no-absolute-filenames'],
                                   stdin=stream, cwd=extracted, check=True, timeout=30)
                verify_tree(extracted / 'usr', pair)
            else:
                print('SKIP RPM payload identity: rpm2cpio/cpio unavailable')
        else:
            print('SKIP RPM container: rpm tools unavailable')

        if args.appimage_runtime:
            run('cpack', '--config', config, '-G', 'External')
            image, = output.glob('*.AppImage')
            env = os.environ.copy()
            runtime = work / 'runtime'
            runtime.mkdir(mode=0o700)
            env.update(XDG_RUNTIME_DIR=str(runtime), APPIMAGE_EXTRACT_AND_RUN='1')
            for key in ('APPIMAGE', 'APPDIR', 'CLIVUU_BINARY', 'CLIVUU_DESKTOP_DIR', 'CLIVUU_DESKTOP_BINARY'):
                env.pop(key, None)
            run(str(image), env=env)
            ready, = runtime.rglob('.ready')
            verify_tree(ready.parent, pair)
            assert ready.parent.stat().st_mode & 0o777 == 0o700
            assert len(ready.parent.name) == 64
        else:
            print('SKIP AppImage container: pass --appimage-runtime for a real builder run')

        # Packaging must re-read the ELF at install time, not trust a configure-time cache.
        corrupt = bytearray((pair / 'clivuu-desktop').read_bytes())
        corrupt[18:20] = b'\xff\xff'
        (pair / 'clivuu-desktop').write_bytes(corrupt)
        rejected = work / 'rejected'
        result = subprocess.run(['cmake', '--install', str(build), '--prefix', str(rejected)],
                                capture_output=True, text=True, timeout=30)
        assert result.returncode and 'package architecture' in result.stderr
        assert not (rejected / 'bin/clivuu').exists()
    print('PASS CMake layout, bilingual payload, native containers and ELF rejection')


if __name__ == '__main__':
    main()

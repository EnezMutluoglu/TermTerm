#!/usr/bin/env python3
"""Run inside the isolated Team jammy root, under Xvfb and dbus-run-session."""
from pathlib import Path
import json
import os
import re
import shutil
import subprocess
import tempfile
import time

root = Path('/work')
assert Path('/.team-build-root').is_file(), 'Use the isolated Team build root'
version = json.loads((root/'package.json').read_text())['version']
bundle = root/'src-tauri/target/release/bundle'
development = '-dev.' in version
product = 'TermTerm Team Dev' if development else 'TermTerm'
binary = 'termterm-team-dev' if development else 'termterm'
debs = list((bundle/'deb').glob(f'{product}_{version}_amd64.deb'))
images = list((bundle/'appimage').glob(f'{product}_{version}_amd64.AppImage'))
assert len(debs) == len(images) == 1
assert shutil.which('xwininfo'), 'x11-utils is required for actual window validation'
tree = subprocess.check_output(['cargo', 'tree', '--locked', '--manifest-path', '/work/src-tauri/Cargo.toml', '--edges', 'normal', '--prefix', 'none'], text=True)
assert 'tauri-plugin-wdio' not in tree
for js in (root/'dist/assets').glob('*.js'):
    assert not re.search('__nativeEvents|__E2E__', js.read_text())
contents = subprocess.check_output(['dpkg-deb', '-c', str(debs[0])], text=True)
assert 'usr/bin/'+binary in contents
assert not re.search(r'/(?:\.lab|\.tools|node_modules|tests)/|\.ttvault|\.ttbackup|service\.json', contents)
if development:
    assert not re.search(r'\busr/bin/termterm\s', contents), 'Must not overwrite stable binary'
subprocess.run(['dpkg', '-i', str(debs[0])], check=True, stdout=subprocess.DEVNULL)


def window(exe, directory, label):
    with tempfile.TemporaryDirectory(prefix='team-package-', dir='/tmp') as home:
        env = {**os.environ, 'HOME':home, 'XDG_CONFIG_HOME':home+'/config', 'XDG_DATA_HOME':home+'/data',
               'WEBKIT_DISABLE_COMPOSITING_MODE':'1', 'WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS':'1'}
        process = subprocess.Popen([str(exe)], cwd=directory, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            for _ in range(80):
                assert process.poll() is None, label + ' exited before opening a window'
                windows = subprocess.check_output(['xwininfo', '-root', '-tree'], text=True)
                if ('TermTerm Team' if development else 'TermTerm') in windows:
                    break
                time.sleep(.25)
            else:
                raise AssertionError(label + ' did not create a real X11 window')
            inodes = set()
            for fd in Path(f'/proc/{process.pid}/fd').iterdir():
                try:
                    link = os.readlink(fd)
                    if link.startswith('socket:['): inodes.add(link[8:-1])
                except OSError: pass
            for file in ['/proc/net/tcp', '/proc/net/tcp6']:
                if not Path(file).exists(): continue
                for line in Path(file).read_text().splitlines()[1:]:
                    columns = line.split()
                    assert not (columns[3] == '0A' and int(columns[1].rsplit(':',1)[1],16) == 4445 and columns[9] in inodes)
        finally:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill();process.wait()
                raise AssertionError(label + ' did not stop')
        print('PASS: '+label+' actual isolated desktop window, no native test listener')


window(Path('/usr/bin')/binary, '/tmp', 'installed DEB')
with tempfile.TemporaryDirectory(prefix='team-appimage-', dir='/tmp') as unpacked:
    subprocess.run([str(images[0]), '--appimage-extract'], cwd=unpacked, check=True, stdout=subprocess.DEVNULL)
    window(Path(unpacked)/'squashfs-root/AppRun', str(Path(unpacked)/'squashfs-root'), 'extracted AppImage AppRun')
name = subprocess.check_output(['dpkg-deb', '-f', str(debs[0]), 'Package'], text=True).strip()
subprocess.run(['dpkg', '-r', name], check=True, stdout=subprocess.DEVNULL)
print('PASS: isolated Team Linux packages, correct binary path, install/window/uninstall. '+version)

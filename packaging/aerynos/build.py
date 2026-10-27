#!/usr/bin/env python3
"""Package the verified desktop release using AerynOS's official stone writer."""
import argparse
import hashlib
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
TOOLS_URL = 'https://codeload.github.com/AerynOS/os-tools/tar.gz/refs/tags/v0.26.1'
TOOLS_SHA = '23ccca7bac8e8196b11ebb59bd0e255f067a97be72ec7dacc5a758753fceaa91'
DESKTOP_URL = 'https://github.com/belalhushelmy2-cmyk/SNI-Tunnel/releases/download/v2.1.5/sni-tunnel-v2_2.1.5_amd64.tar.gz'
DESKTOP_SHA = '8ac14af8127139578ca6e9197edcba6a1db158bf31b6ebfcdd3ff25594661a3f'

def verified(path, url, digest):
    if not path.exists():
        path.parent.mkdir(parents=True, exist_ok=True)
        with urllib.request.urlopen(url, timeout=180) as stream:
            path.write_bytes(stream.read())
    if hashlib.sha256(path.read_bytes()).hexdigest() != digest:
        raise RuntimeError('SHA-256 mismatch: ' + str(path))

def extract(archive, destination, prefix):
    with tarfile.open(archive) as source:
        for entry in source.getmembers():
            parts = Path(entry.name).parts
            if not parts or parts[0] != prefix or '..' in parts or entry.name.startswith('/') or not (entry.isfile() or entry.isdir()):
                raise RuntimeError('Unsafe source entry: ' + entry.name)
        source.extractall(destination, filter='data')

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--archive', type=Path, default=ROOT / 'target/aerynos-cache/desktop.tar.gz')
    p.add_argument('--tools-archive', type=Path, default=ROOT / 'target/aerynos-cache/os-tools.tar.gz')
    p.add_argument('--output', type=Path, default=ROOT / 'releases/v2/aerynos')
    p.add_argument('--moss', default='moss')
    a = p.parse_args()
    verified(a.archive, DESKTOP_URL, DESKTOP_SHA)
    verified(a.tools_archive, TOOLS_URL, TOOLS_SHA)
    tools = ROOT / 'target/aerynos-os-tools'
    if tools.exists():
        shutil.rmtree(tools)
    with tempfile.TemporaryDirectory(prefix='sni-stone-') as temporary:
        stage = Path(temporary)
        extract(a.tools_archive, stage, 'os-tools-0.26.1')
        shutil.move(stage / 'os-tools-0.26.1', tools)
        extract(a.archive, stage, 'sni-tunnel')
        payload = stage / 'payload'
        app = payload / 'usr/lib/sni-tunnel-v2'
        app.parent.mkdir(parents=True)
        shutil.move(stage / 'sni-tunnel', app)
        launcher = payload / 'usr/bin/sni-tunnel-v2'
        launcher.parent.mkdir(parents=True)
        launcher.write_text('#!/bin/sh\nexec /usr/lib/sni-tunnel-v2/start.sh "$@"\n')
        launcher.chmod(0o755)
        desktop = payload / 'usr/share/applications/sni-tunnel-v2.desktop'
        desktop.parent.mkdir(parents=True)
        shutil.copyfile(ROOT / 'packaging/linux/sni-tunnel.desktop', desktop)
        icon = payload / 'usr/share/icons/hicolor/256x256/apps/sni-tunnel-v2.png'
        icon.parent.mkdir(parents=True)
        shutil.copyfile(app / 'icon.png', icon)
        subprocess.run(['cargo', 'build', '--release', '--locked', '--manifest-path', str(ROOT / 'packaging/aerynos/stone-pack/Cargo.toml')], check=True)
        a.output.mkdir(parents=True, exist_ok=True)
        package = a.output / 'sni-tunnel-v2-2.1.5-1-1-x86_64.stone'
        subprocess.run([str(ROOT / 'packaging/aerynos/stone-pack/target/release/sni-stone-pack'), str(payload), str(package), str(Path(__file__).with_name('dependencies.txt'))], check=True)
        subprocess.run([a.moss, 'inspect', str(package)], check=True)
        subprocess.run([a.moss, 'index', str(a.output)], check=True)
        print(hashlib.sha256(package.read_bytes()).hexdigest(), package)

if __name__ == '__main__':
    main()

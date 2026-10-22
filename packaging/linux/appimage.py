#!/usr/bin/env python3
"""Build a V2 AppImage from the portable release and pinned official tools."""
import argparse
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
TOOLS = {
    'appimagetool.AppImage': (
        'https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage',
        'ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0'),
    'runtime-x86_64': (
        'https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-x86_64',
        '2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d'),
}


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/x86_64-unknown-linux-gnu/release/sni-tunnel')
    parser.add_argument('--tool-dir', type=Path, default=ROOT / 'target/appimage-tools')
    parser.add_argument('--output', type=Path, default=ROOT / 'releases/v2/linux/SNI-Tunnel-V2-2.0.2-x86_64.AppImage')
    args = parser.parse_args()
    if not args.binary.is_file():
        parser.error('Build the portable Linux release first (packaging/linux/build-portable.sh)')
    args.tool_dir.mkdir(parents=True, exist_ok=True)
    for name, (url, digest) in TOOLS.items():
        tool = args.tool_dir / name
        if not tool.exists():
            part = tool.with_suffix('.download')
            urllib.request.urlretrieve(url, part)
            if sha(part) != digest:
                part.unlink()
                raise RuntimeError(f'{name} download checksum mismatch')
            part.rename(tool)
        if sha(tool) != digest:
            raise RuntimeError(f'{name} checksum mismatch')
        tool.chmod(0o755)
    appdir = ROOT / 'target/appimage/SNI-Tunnel-V2.AppDir'
    if appdir.exists():
        shutil.rmtree(appdir)
    payload = appdir / 'usr/lib/sni-tunnel-v2'
    payload.mkdir(parents=True)
    for source, name, mode in [
        (args.binary, 'sni-tunnel', 0o755),
        (ROOT/'packaging/linux/network.py', 'network.py', 0o755),
        (ROOT/'assets/icon-256.png', 'icon.png', 0o644),
        (ROOT/'packaging/README-INSTALL.txt', 'README-INSTALL.txt', 0o644),
        (ROOT/'THIRD-PARTY-NOTICES.md', 'THIRD-PARTY-NOTICES.md', 0o644),
    ]:
        destination = payload / name
        shutil.copy2(source, destination)
        destination.chmod(mode)
    (payload/'core').mkdir()
    for name in ['xray', 'badvpn-tun2socks']:
        destination = payload/'core'/name
        shutil.copy2(ROOT/'packaging/staging/linux/core'/name, destination)
        destination.chmod(0o755)
    shutil.copytree(ROOT/'packaging/licenses', payload/'licenses')
    shutil.copy2(ROOT/'packaging/linux/AppRun', appdir/'AppRun')
    (appdir/'AppRun').chmod(0o755)
    shutil.copy2(ROOT/'assets/icon-256.png', appdir/'sni-tunnel-v2.png')
    (appdir/'.DirIcon').symlink_to('sni-tunnel-v2.png')
    (appdir/'sni-tunnel-v2.desktop').write_text(
        '[Desktop Entry]\nType=Application\nName=SNI Tunnel V2\n'
        'Comment=Native Slint VMess client with independent TLS SNI\n'
        'Exec=AppRun\nIcon=sni-tunnel-v2\nTerminal=false\nCategories=Network;\n')
    files = sorted(p for p in payload.rglob('*') if p.is_file())
    assert not any(p.name in ('app.py', 'config.json', 'profile.json', 'secret.key') for p in files)
    (appdir/'payload.sha256').write_text(''.join(f'{sha(p)}  {p.relative_to(payload).as_posix()}\n' for p in files))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env.update(ARCH='x86_64', VERSION='2.0.2', SOURCE_DATE_EPOCH='1792627200')
    subprocess.run([
        str(args.tool_dir.resolve()/'appimagetool.AppImage'), '--appimage-extract-and-run',
        '--runtime-file', str(args.tool_dir.resolve()/'runtime-x86_64'),
        '--comp', 'zstd', '--mksquashfs-opt', '-processors', '--mksquashfs-opt', '4',
        '--no-appstream', str(appdir), str(args.output.resolve()),
    ], env=env, check=True)
    args.output.chmod(0o755)
    print(f'AppImage written to {args.output}')
    print(f'SHA256 {sha(args.output)}')


if __name__ == '__main__':
    main()

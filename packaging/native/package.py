#!/usr/bin/env python3
"""Build on the destination OS; never relabel a Linux executable as native."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import plistlib
import shutil
import subprocess
import tempfile
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[2]
VERSION = '2.1.6'

def run(*args, **kwargs):
    return subprocess.run(list(map(str, args)), check=True, **kwargs)

def copy(source, destination, mode=0o644):
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, destination)
    destination.chmod(mode)

def core(dest):
    system = platform.system().lower()
    machine = platform.machine().lower()
    lock = json.loads((Path(__file__).with_name('core-lock.json')).read_text())
    name, digest = lock['artifacts'][f'{system}-{machine}']
    cache = ROOT / 'target/native-core' / lock['version'] / name
    cache.parent.mkdir(parents=True, exist_ok=True)
    if not cache.exists():
        with urllib.request.urlopen(f'https://github.com/XTLS/Xray-core/releases/download/v{lock["version"]}/{name}', timeout=180) as response:
            cache.write_bytes(response.read())
    if hashlib.sha256(cache.read_bytes()).hexdigest() != digest:
        raise RuntimeError(f'Core SHA-256 mismatch: {name}')
    dest.mkdir(parents=True)
    with zipfile.ZipFile(cache) as archive:
        for name in ['xray', 'LICENSE']:
            target = dest / name
            target.write_bytes(archive.read(name))
            target.chmod(0o755 if name == 'xray' else 0o644)
    run(dest / 'xray', 'version')

def documentation(dest):
    copy(ROOT / 'packaging/native/README.md', dest / 'README.md')
    copy(ROOT / 'THIRD-PARTY-NOTICES.md', dest / 'THIRD-PARTY-NOTICES.md')
    for source in (ROOT / 'packaging/licenses').iterdir():
        if source.is_file():
            copy(source, dest / 'licenses' / source.name)

def macos(binary, stage, out):
    app = stage / 'SNI Tunnel.app'
    contents = app / 'Contents'
    exe = contents / 'MacOS/sni-tunnel'
    copy(binary, exe, 0o755)
    core(contents / 'Helpers')
    license_bytes = (contents / 'Helpers/LICENSE').read_bytes()
    (contents / 'Helpers/LICENSE').unlink()
    resources = contents / 'Resources'
    documentation(resources)
    (resources / 'licenses/Xray-LICENSE.txt').write_bytes(license_bytes)
    iconset = stage / 'SNI.iconset'
    iconset.mkdir()
    for size in [16, 32, 128, 256, 512]:
        for scale in [1, 2]:
            name = f'icon_{size}x{size}' + ('@2x' if scale == 2 else '') + '.png'
            run('sips', '-z', size * scale, size * scale, ROOT / 'assets/icon-256.png', '--out', iconset / name, stdout=subprocess.DEVNULL)
    run('iconutil', '-c', 'icns', iconset, '-o', resources / 'SNI.icns')
    plist = dict(CFBundleExecutable='sni-tunnel', CFBundleIdentifier='com.snitunnel.v2',
                 CFBundleName='SNI Tunnel', CFBundleDisplayName='SNI Tunnel',
                 CFBundlePackageType='APPL', CFBundleShortVersionString=VERSION,
                 CFBundleVersion=VERSION, CFBundleIconFile='SNI.icns',
                 LSMinimumSystemVersion='12.0', NSHighResolutionCapable=True)
    (contents / 'Info.plist').write_bytes(plistlib.dumps(plist))
    dependencies = {}
    for target in [exe, contents / 'Helpers/xray']:
        output = subprocess.check_output(['otool', '-L', str(target)], text=True)
        libraries = [line.strip().split(' (', 1)[0] for line in output.splitlines()[1:] if line.strip()]
        if not libraries or not all(lib.startswith(('/usr/lib/', '/System/Library/')) for lib in libraries):
            raise RuntimeError(f'Unbundled runtime library in {target}: {libraries}')
        dependencies[str(target.relative_to(app))] = libraries
    (out / f'dependency-audit-{platform.machine()}.json').write_text(json.dumps(dependencies, indent=2) + '\n')
    # Ad-hoc signing provides arm64 executable integrity, not Developer ID trust.
    for target in [contents / 'Helpers/xray', exe, app]:
        run('codesign', '--force', '--sign', '-', target)
    run('codesign', '--verify', '--deep', '--strict', app)
    (stage / 'Applications').symlink_to('/Applications')
    shutil.rmtree(iconset)
    dmg = out / f'SNI-Tunnel-V2-{VERSION}-macos-{platform.machine()}.dmg'
    run('hdiutil', 'create', '-volname', 'SNI Tunnel', '-srcfolder', stage, '-ov', '-format', 'UDZO', dmg)
    run('hdiutil', 'verify', dmg)
    return dmg

def freebsd(binary, stage, out):
    prefix = stage / 'usr/local'
    app = prefix / 'lib/sni-tunnel-v2'
    copy(binary, app / 'sni-tunnel', 0o755)
    core(app / 'core')
    documentation(app)
    launcher = prefix / 'bin/sni-tunnel-v2'
    launcher.parent.mkdir(parents=True)
    launcher.write_text('#!/bin/sh\nexec /usr/local/lib/sni-tunnel-v2/sni-tunnel "$@"\n')
    launcher.chmod(0o755)
    desktop = prefix / 'share/applications/sni-tunnel-v2.desktop'
    desktop.parent.mkdir(parents=True)
    desktop.write_text('[Desktop Entry]\nType=Application\nName=SNI Tunnel V2\nExec=/usr/local/bin/sni-tunnel-v2 gui\nIcon=sni-tunnel-v2\nTerminal=false\nCategories=Network;\n')
    copy(ROOT / 'assets/icon-256.png', prefix / 'share/icons/hicolor/256x256/apps/sni-tunnel-v2.png')
    # Resolve installed native package versions/origins; let pkg install deps.
    deps = {}
    for name in ['fontconfig', 'libxkbcommon', 'libX11', 'libXcursor', 'libXrandr', 'libXi', 'libxcb', 'mesa-libs', 'wayland', 'glib', 'dbus', 'xclip', 'ca_root_nss', 'dejavu']:
        value = subprocess.check_output(['pkg', 'query', '%n|%v|%o', name], text=True).strip().split('|')
        deps[value[0]] = dict(version=value[1], origin=value[2])
    manifest = dict(name='sni-tunnel-v2', version=VERSION, origin='net/sni-tunnel-v2',
                    comment='Native Slint VMess desktop with custom TLS SNI',
                    desc='Local HTTP/SOCKS proxy client with bundled Xray. Full-device TUN is unavailable on this platform.',
                    maintainer='local@localhost', www='https://github.com/belalhushelmy2-cmyk/SNI-Tunnel',
                    prefix='/usr/local', deps=deps,
                    files={('/' + str(path.relative_to(stage))): hashlib.sha256(path.read_bytes()).hexdigest()
                           for path in stage.rglob('*') if path.is_file()})
    path = stage.parent / 'manifest.json'
    path.write_text(json.dumps(manifest))
    run('pkg', 'create', '-M', path, '-r', stage, '-o', out)
    return out / f'sni-tunnel-v2-{VERSION}.pkg'

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/sni-tunnel')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if platform.system() not in ['Darwin', 'FreeBSD']:
        parser.error('Run on macOS or FreeBSD with a native executable')
    args.output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='sni-native-') as temporary:
        stage = Path(temporary) / 'stage'
        stage.mkdir()
        artifact = (macos if platform.system() == 'Darwin' else freebsd)(args.binary.resolve(), stage, args.output.resolve())
        print(artifact)
        print(hashlib.sha256(artifact.read_bytes()).hexdigest())

if __name__ == '__main__':
    main()

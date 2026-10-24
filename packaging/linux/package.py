#!/usr/bin/env python3
"""Package an already built Linux backend and the Linux desktop launcher."""
import argparse, io, json, os, shutil, subprocess, tarfile, re
from pathlib import Path
ROOT = Path(__file__).resolve().parents[2]

def archive(path, files):
    with tarfile.open(path, 'w:gz', format=tarfile.PAX_FORMAT) as tar:
        for name, source, mode in files:
            info = tarfile.TarInfo(name)
            data = source.read_bytes() if isinstance(source, Path) else source
            info.size, info.mode, info.uid, info.gid = len(data), mode, 0, 0
            info.uname = info.gname = 'root'
            tar.addfile(info, io.BytesIO(data))

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/sni-tunnel')
    parser.add_argument('--output', type=Path, default=ROOT / 'releases/v2/linux')
    parser.add_argument('--rpm', action='store_true')
    args = parser.parse_args()
    out = args.output.resolve(); out.mkdir(parents=True, exist_ok=True)
    stage = ROOT / 'target/package-linux'; stage.mkdir(parents=True, exist_ok=True)
    files = [('sni-tunnel', args.binary.resolve(), 0o755), ('icon.png', ROOT / 'assets/icon-256.png', 0o644)]
    for name in ['network.py', 'start.sh']:
        files.append((name, ROOT / 'packaging/linux' / name, 0o755))
    for name in ['xray', 'badvpn-tun2socks']:
        files.append(('core/' + name, ROOT / 'packaging/staging/linux/core' / name, 0o755))
    files += [('README-INSTALL.txt', ROOT / 'packaging/README-INSTALL.txt', 0o644),
              ('THIRD-PARTY-NOTICES.md', ROOT / 'THIRD-PARTY-NOTICES.md', 0o644)]
    for license in (ROOT / 'packaging/licenses').glob('*'):
        if license.is_file(): files.append(('licenses/' + license.name, license, 0o644))
    archive(out / 'sni-tunnel-v2_2.1.5_amd64.tar.gz', [('sni-tunnel/' + n, p, m) for n,p,m in files])
    installed = [('usr/lib/sni-tunnel-v2/' + n, p, m) for n,p,m in files]
    installed += [('usr/bin/sni-tunnel-v2', b'#!/bin/sh\nexec /usr/lib/sni-tunnel-v2/start.sh "$@"\n', 0o755),
                  ('usr/share/applications/sni-tunnel-v2.desktop', ROOT / 'packaging/linux/sni-tunnel.desktop', 0o644),
                  ('usr/share/icons/hicolor/256x256/apps/sni-tunnel-v2.png', ROOT / 'assets/icon-256.png', 0o644)]
    archive(stage / 'data.tar.gz', installed)
    size = sum(p.stat().st_size if isinstance(p,Path) else len(p) for n,p,m in installed) // 1024 + 1
    versions = re.findall(r'GLIBC_([0-9]+\.[0-9]+)', '\n'.join(subprocess.check_output(['readelf', '--version-info', str(binary)], text=True) for binary in [args.binary, ROOT / 'packaging/staging/linux/core/badvpn-tun2socks']))
    glibc_required = max(versions, key=lambda value: tuple(map(int,value.split('.'))))
    control = f"""Package: sni-tunnel-v2
Version: 2.1.5
Section: net
Priority: optional
Architecture: amd64
Maintainer: SNI Tunnel local build <local@localhost>
Depends: libc6 (>= {glibc_required}), libgcc-s1, libfontconfig1, libglib2.0-bin, libxkbcommon0, libxkbcommon-x11-0, libxcb-xkb1, libxcursor1, libxrandr2, libxi6, libx11-6, libxcb1, libwayland-client0, python3 (>= 3.8), iproute2, nftables, libnotify-bin, pkexec | policykit-1, ca-certificates, kmod
Installed-Size: {size}
Description: VMess desktop tunnel with independent TLS SNI
 Native Slint desktop with Xray, BadVPN, Linux TUN and DNS recovery.
 Existing user profiles are retained in their original directories.
""".encode()
    archive(stage / 'control.tar.gz', [('control', control, 0o644)])
    with (out / 'sni-tunnel-v2_2.1.5_amd64.deb').open('wb') as ar:
        ar.write(b'!<arch>\n')
        for name, data in [('debian-binary', b'2.0\n'), ('control.tar.gz', (stage/'control.tar.gz').read_bytes()), ('data.tar.gz',(stage/'data.tar.gz').read_bytes())]:
            header = f'{name + "/":<16}{0:<12}{0:<6}{0:<6}{"100644":<8}{len(data):<10}`\n'.encode('ascii')
            assert len(header) == 60
            ar.write(header); ar.write(data)
            if len(data) % 2: ar.write(b'\n')
    if args.rpm:
        rpmstage = stage / 'rpm'; rpmstage.mkdir(exist_ok=True)
        assets = []
        for i,(name, source, mode) in enumerate(installed):
            target = rpmstage / f'asset-{i}'
            target.write_bytes(source.read_bytes() if isinstance(source,Path) else source)
            assets.append('{source=' + json.dumps(target.as_posix()) + ',dest=' + json.dumps('/' + name) + ',mode=' + json.dumps(oct(mode)[2:]) + '}')
        manifest = """[package]
name="sni-tunnel-v2"
version="2.1.5"
edition="2021"
description="VMess desktop tunnel with independent TLS SNI"
license="LicenseRef-SNI-Tunnel"
[package.metadata.generate-rpm]
release="1"
auto-req="disabled"
""" + 'assets=[\n' + ',\n'.join(assets) + '\n]\n' + """[package.metadata.generate-rpm.requires]
python3=">= 3.8"

iproute="*"
polkit="*"
ca-certificates="*"
kmod="*"
glibc=">= 2.30"
"""
        (rpmstage/'Cargo.toml').write_text(manifest)
        (rpmstage/'src').mkdir(exist_ok=True)
        (rpmstage/'src/main.rs').write_text('fn main() {}')
        subprocess.run(['cargo-generate-rpm', '--auto-req', 'disabled', '-a', 'x86_64', '--payload-compress', 'gzip', '-o', str(out/'sni-tunnel-v2_2.1.5_amd64.rpm')], cwd=rpmstage, check=True)
    print('Packages written to', out)
if __name__ == '__main__': main()

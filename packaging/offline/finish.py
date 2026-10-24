#!/usr/bin/env python3
"""Finalize an offline directory and produce a checksum-verifying .bin installer."""
import argparse, hashlib, shutil, subprocess
from pathlib import Path
p=argparse.ArgumentParser(); p.add_argument('directory',type=Path); p.add_argument('platform'); p.add_argument('output',type=Path)
a=p.parse_args(); root=a.directory.resolve(); a.output.mkdir(parents=True,exist_ok=True)
shutil.copy2(Path(__file__).with_name('installer.sh'),root/'install.sh')
(root/'platform').write_text(a.platform+'\n')
(root/'README.txt').write_text('SNI Tunnel V2 2.1.3 offline dependencies\nRun ./install.sh as your desktop user. It requests administrator permission for package installation.\nUse the bundle matching your distribution and x86_64 architecture.\nArch: existing system libraries are never selectively upgraded; update the whole system first if the installer reports a snapshot mismatch.\nThis bundle includes the AppImage and portable payload when supplied; installing the package prepares their host dependencies too.\nA working desktop, kernel TUN support, sudo, and the native package manager are prerequisites.\nNo Android APK is included: Android requires a separate VpnService integration.\n')
files=sorted(x for x in root.rglob('*') if x.is_file() and x.name!='SHA256SUMS')
(root/'SHA256SUMS').write_text(''.join(hashlib.sha256(x.read_bytes()).hexdigest()+'  '+str(x.relative_to(root))+'\n' for x in files))
name=f'SNI-Tunnel-V2-2.1.3-{a.platform}-x86_64-offline'
archive=a.output/(name+'.tar.gz')
subprocess.run(['tar','-czf',str(archive.resolve()),'-C',str(root),'.'],check=True)
digest=hashlib.sha256(archive.read_bytes()).hexdigest()
header='''#!/bin/sh
set -eu
line=$(awk '/^__SNI_ARCHIVE_BELOW__$/ {print NR + 1; exit}' "$0")
tmp=$(mktemp -d)
trap 'rm -rf -- "$tmp"' EXIT HUP INT TERM
tail -n +"$line" "$0" > "$tmp/bundle.tar.gz"
printf '%s  %s\\n' 'DIGEST' "$tmp/bundle.tar.gz" | sha256sum -c -
if [ "${1:-}" = --check ]; then exit 0; fi
if [ "${1:-}" = --extract ]; then
  mkdir -p -- "$2"
  tar -xzf "$tmp/bundle.tar.gz" -C "$2"
  exit 0
fi
tar -xzf "$tmp/bundle.tar.gz" -C "$tmp"
bash "$tmp/install.sh"
exit 0
__SNI_ARCHIVE_BELOW__
'''.replace('DIGEST',digest)
binary=a.output/(name+'.bin')
with binary.open('wb') as f:
 f.write(header.encode()); f.write(archive.read_bytes())
binary.chmod(0o755)
print(binary)

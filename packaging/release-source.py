#!/usr/bin/env python3
"""Create a source asset from tracked files plus verified vendor archives only."""
import argparse
from pathlib import Path
import subprocess
import zipfile

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument('output', type=Path)
args = parser.parse_args()
subprocess.run(['python3', 'packaging/linux/fetch-vendor.py'], cwd=ROOT, check=True)
tracked = {Path(p) for p in subprocess.check_output(['git', 'ls-files', '-z'], cwd=ROOT).decode().split('\0') if p}
tracked.update(Path('packaging/vendor')/name for name in ['Xray-linux-64.zip', 'badvpn-source.tar.gz', 'Xray-1.6.1-source.tar.gz'])
args.output.parent.mkdir(parents=True, exist_ok=True)
with zipfile.ZipFile(args.output, 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
    for relative in sorted(tracked):
        assert not any(part in ('private', 'work', 'target', 'target-windows', 'releases') for part in relative.parts)
        archive.write(ROOT/relative, 'SNI-Tunnel-V2/'+relative.as_posix())
print('Source archive:', args.output)

#!/bin/sh
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$ROOT"
python3 packaging/linux/fetch-vendor.py
[ "$(uname -m)" = x86_64 ] || { echo 'This package targets amd64/x86_64.'; exit 1; }
# Native build: Rust stable, a C compiler, CMake, Python 3 and unzip required.
cargo build --release --locked
mkdir -p target/badvpn-source packaging/staging/linux/core
python3 - <<'PY'
from pathlib import Path
import hashlib, zipfile
p=Path('packaging/vendor/Xray-linux-64.zip')
assert hashlib.sha256(p.read_bytes()).hexdigest() == 'fa6517eea07f10f5f0a7e8768bb4b411266a217161b0a9d2794b48769f1ad541'
with zipfile.ZipFile(p) as z:
    for n in ['xray', 'LICENSE']:
        Path('packaging/staging/linux/core', n).write_bytes(z.read(n))
PY
tar -xzf packaging/vendor/badvpn-source.tar.gz -C target/badvpn-source --strip-components=1
cmake -S target/badvpn-source -B target/badvpn-native -DCMAKE_POLICY_VERSION_MINIMUM=3.5 -DBUILD_NOTHING_BY_DEFAULT=ON -DBUILD_TUN2SOCKS=ON -DCMAKE_BUILD_TYPE=Release
cmake --build target/badvpn-native -j 2
cp target/badvpn-native/tun2socks/badvpn-tun2socks packaging/staging/linux/core/
chmod +x packaging/staging/linux/core/xray packaging/staging/linux/core/badvpn-tun2socks
python3 -m unittest discover -s packaging/linux -p 'test_*.py'
python3 packaging/linux/package.py

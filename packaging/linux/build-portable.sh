#!/bin/sh
# Build the GUI and forwarding helper against the same glibc 2.30 ABI floor.
# Build dependencies: cargo-zigbuild, zig, CMake, Python 3, tar, readelf.
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$ROOT"
python3 packaging/linux/fetch-vendor.py
cargo zigbuild --release --locked --target x86_64-unknown-linux-gnu.2.30
mkdir -p target/badvpn-source packaging/staging/linux/core
python3 - <<'PY'
from pathlib import Path
import hashlib, zipfile
archive=Path('packaging/vendor/Xray-linux-64.zip')
assert hashlib.sha256(archive.read_bytes()).hexdigest()=='fa6517eea07f10f5f0a7e8768bb4b411266a217161b0a9d2794b48769f1ad541'
with zipfile.ZipFile(archive) as source:
    Path('packaging/staging/linux/core/xray').write_bytes(source.read('xray'))
PY
tar -xzf packaging/vendor/badvpn-source.tar.gz -C target/badvpn-source --strip-components=1
cat > target/zig-cc <<'EOF'
#!/bin/sh
exec zig cc -target x86_64-linux-gnu.2.30 "$@"
EOF
chmod +x target/zig-cc
cmake --fresh -S target/badvpn-source -B target/badvpn-portable -DCMAKE_POLICY_VERSION_MINIMUM=3.5 -DBUILD_NOTHING_BY_DEFAULT=ON -DBUILD_TUN2SOCKS=ON -DCMAKE_BUILD_TYPE=Release -DCMAKE_C_COMPILER="$ROOT/target/zig-cc"
cmake --build target/badvpn-portable -j 4
cp target/badvpn-portable/tun2socks/badvpn-tun2socks packaging/staging/linux/core/
chmod +x packaging/staging/linux/core/xray packaging/staging/linux/core/badvpn-tun2socks
python3 -m unittest discover -s packaging/linux -p test_network.py
python3 packaging/linux/package.py --binary target/x86_64-unknown-linux-gnu/release/sni-tunnel

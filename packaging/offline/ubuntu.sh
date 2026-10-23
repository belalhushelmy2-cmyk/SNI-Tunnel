#!/usr/bin/env bash
# Build only on Ubuntu 24.04; uses an empty dpkg status to include installed deps too.
set -euo pipefail
source /etc/os-release
[[ $ID == ubuntu && $VERSION_ID == 24.04 ]]
assets=$(realpath "${1:-releases/v2/linux}")
root=$(realpath -m target/offline-ubuntu/2.1.2/bundle)
mkdir -p "$root/packages/partial"
cp "$assets/sni-tunnel-v2_2.1.2_amd64.deb" "$root/packages/"
cp "$assets/SNI-Tunnel-V2-2.1.2-x86_64.AppImage" "$assets/sni-tunnel-v2_2.1.2_amd64.tar.gz" "$root/"
touch target/offline-ubuntu/empty-status
sudo apt-get update
sudo apt-get -y --download-only -o "Dir::State::status=$PWD/target/offline-ubuntu/empty-status" -o "Dir::Cache::archives=$root/packages" install "$assets/sni-tunnel-v2_2.1.2_amd64.deb" policykit-1-gnome wl-clipboard xclip fonts-dejavu-core
sudo chown -R "$(id -u):$(id -g)" "$root"
rm -rf "$root/packages/partial" "$root/packages/lock"
(cd "$root/packages" && dpkg-scanpackages . /dev/null > Packages)
python3 packaging/offline/finish.py "$root" ubuntu-24.04 "$assets"

#!/usr/bin/env bash
# Online bootstrap: run as your desktop user, without sudo.
set -euo pipefail
if [[ ${1:-} == --help || ${1:-} == -h ]]; then
    echo 'Usage: bash install.sh [--check | --no-deps | --uninstall]'
    echo 'Installs or updates the latest Linux x86_64 release for the current user.'
    echo 'Requires glibc >= 2.30 and Python >= 3.8. See INSTALL.md.'
    exit 0
fi
[[ $(uname -s) == Linux && $(uname -m) == x86_64 ]] || { echo 'Requires Linux x86_64; no compatible ARM build is published.' >&2; exit 1; }
[[ $EUID != 0 ]] || { echo 'Run as your desktop user without sudo; dependency installation requests sudo separately.' >&2; exit 1; }
command -v python3 >/dev/null || { echo 'Install Python 3.8+ with your distribution package manager, then rerun this command.' >&2; exit 1; }
python3 -c 'import sys; sys.exit(0 if sys.version_info >= (3, 8) else 1)' || { echo 'Python 3.8 or newer is required.' >&2; exit 1; }
command -v curl >/dev/null || { echo 'Install curl and ca-certificates with your distribution package manager, then rerun.' >&2; exit 1; }
installer_tmp=$(mktemp -d)
trap 'rm -rf -- "$installer_tmp"' EXIT
curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 --retry 3 \
    https://raw.githubusercontent.com/belalhushelmy2-cmyk/SNI-Tunnel/main/scripts/install.py \
    -o "$installer_tmp/install.py"
python3 "$installer_tmp/install.py" "$@"

#!/bin/sh
set -eu
export SLINT_SCALE_FACTOR="${SLINT_SCALE_FACTOR:-1}"
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
exec "$HERE/sni-tunnel" gui "$@"

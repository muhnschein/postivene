#!/bin/sh
# Sailfish's cargo 1.75.0 cannot read a v4 lockfile (v4 arrived in 1.78),
# and `cargo update` on a modern host rewrites it silently.
set -eu

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
lock="$root/rust/Cargo.lock"

version=$(grep -E '^version = [0-9]+$' "$lock" | head -1 | tr -cd '0-9')
if [ "${version:-}" != "3" ]; then
    echo "check-lockfile: FAIL rust/Cargo.lock is v${version:-unknown}, must stay v3 for Sailfish's cargo 1.75 (see docs/BUILDING.md)" >&2
    exit 1
fi
echo "check-lockfile: ok (v3)"

#!/bin/sh
# Sailfish ships cargo 1.75.0, which cannot read a v4 Cargo.lock (that
# format arrived in 1.78). A `cargo update` on a modern host silently
# rewrites the lockfile to v4, and the failure then surfaces only inside
# the SDK, days later, as an unreadable lockfile.
set -eu

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
lock="$root/rust/Cargo.lock"

version=$(grep -E '^version = [0-9]+$' "$lock" | head -1 | tr -cd '0-9')
if [ "${version:-}" != "3" ]; then
    echo "check-lockfile: FAIL rust/Cargo.lock is v${version:-unknown}, must stay v3 for Sailfish's cargo 1.75 (see docs/ENGINEERING.md)" >&2
    exit 1
fi
echo "check-lockfile: ok (v3)"

#!/bin/sh
# Parse every .qml file in the repository.
#
# `qmllint` is a syntax check, not a semantic one: it cannot resolve
# `Sailfish.Silica`, which does not exist outside the Sailfish SDK, so it
# will not tell us whether a `Page` has the properties we think it has.
# What it does catch is the whole class of typo that would otherwise first
# show up as a blank screen on a phone.
#
# The Qt-5.6 dialect rules that qmllint cannot express (host Qt accepts
# newer syntax happily) are enforced separately, as a Rust test:
# rust/postivene-shim/tests/qml_syntax.rs.
set -eu

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)

if ! command -v qmllint >/dev/null 2>&1; then
    echo "qml-lint: FAIL (qmllint not found; install qtdeclarative5-dev-tools)" >&2
    exit 1
fi

count=0
status=0
for file in $(find "$root/qml" -name '*.qml' | sort); do
    count=$((count + 1))
    if ! qmllint "$file"; then
        status=1
    fi
done

if [ "$count" -eq 0 ]; then
    echo "qml-lint: FAIL (no .qml files found -- did the tree move?)" >&2
    exit 1
fi

[ "$status" -eq 0 ] && echo "qml-lint: ok ($count files)"
exit "$status"

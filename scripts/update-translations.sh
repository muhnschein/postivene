#!/bin/sh
# Regenerate translations/postivene.ts from the qsTr() calls in qml/.
#
# `-locations none`: with locations in, the catalog changes every time a
# line moves, so it would conflict on every unrelated edit and the check
# in ci/ would fail for no reason. Without them it changes only when the
# strings themselves do, which is the thing worth reviewing.
#
# `-no-obsolete`: a string that no longer exists in the source is dropped
# rather than kept as a tombstone. The catalog is regenerated from source,
# not maintained by hand.
set -eu

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)

lupdate=$(command -v lupdate || command -v lupdate-qt5 || true)
if [ -z "$lupdate" ]; then
    echo "update-translations: lupdate not found (install qttools5-dev-tools)" >&2
    exit 1
fi

"$lupdate" -recursive "$root/qml" \
    -ts "${1:-$root/translations/postivene.ts}" \
    -no-obsolete -locations none

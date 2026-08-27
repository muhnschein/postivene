#!/bin/sh
# Static checks on what decides whether the RPM installs and the launcher
# works. Resolving BuildRequires or running mb2 needs the SDK
# (docs/SDK-BUILD.md); this checks what is checkable anywhere.
set -eu

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
status=0
ran=0

if command -v rpmspec >/dev/null 2>&1; then
    ran=$((ran + 1))
    # -P expands and parses only; BuildRequires are the SDK's job.
    if rpmspec -P "$root/rpm/postivene.spec" >/dev/null; then
        echo "packaging-lint: rpm/postivene.spec parses"
    else
        echo "packaging-lint: FAIL rpm/postivene.spec does not parse" >&2
        status=1
    fi
else
    echo "packaging-lint: SKIP rpmspec (install rpm)"
fi

if command -v desktop-file-validate >/dev/null 2>&1; then
    ran=$((ran + 1))
    # Sailfish's own keys are not in the freedesktop spec; each expected
    # warning is named, and anything else still fails.
    out=$(desktop-file-validate "$root/postivene.desktop" 2>&1 |
        grep -v 'value "silica-qt5" for key "X-Nemo-Application-Type"' |
        grep -v 'key "X-Nemo-Application-Type" .* is not known' || true)
    if [ -z "$out" ]; then
        echo "packaging-lint: postivene.desktop valid"
    else
        echo "$out" >&2
        echo "packaging-lint: FAIL postivene.desktop" >&2
        status=1
    fi
else
    echo "packaging-lint: SKIP desktop-file-validate (install desktop-file-utils)"
fi

if command -v shellcheck >/dev/null 2>&1; then
    ran=$((ran + 1))
    if shellcheck "$root"/ci/*.sh "$root"/scripts/*.sh; then
        echo "packaging-lint: shell scripts clean"
    else
        echo "packaging-lint: FAIL shellcheck" >&2
        status=1
    fi
else
    echo "packaging-lint: SKIP shellcheck (install shellcheck)"
fi

if [ "$ran" -eq 0 ]; then
    echo "packaging-lint: FAIL (no checker was available; this job proves nothing)" >&2
    exit 1
fi

exit "$status"

#!/bin/sh
# Static checks on the things that decide whether an RPM installs and the
# launcher works -- the parts of the build no `cargo test` ever touches.
#
# What this cannot do is resolve Sailfish's BuildRequires or run mb2; that
# needs the SDK (docs/SDK-BUILD.md). It checks what is checkable anywhere:
# the spec parses, the desktop entry is valid, and the shell scripts are
# clean.
set -eu

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
status=0
ran=0

if command -v rpmspec >/dev/null 2>&1; then
    ran=$((ran + 1))
    # -P only expands and parses the spec; it does not try to resolve the
    # Sailfish-specific BuildRequires, which exist in no non-Sailfish
    # package database and are the SDK's job to satisfy.
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
    # Sailfish's own keys are not in the freedesktop spec, so they are
    # warnings we expect rather than errors we accept blindly: the filter
    # names them one by one, and anything else still fails.
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

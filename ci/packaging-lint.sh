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

# rpm expands macros inside comments, and the SDK's rpm still does even
# where a newer host rpm has stopped. `%build` there expands to the whole
# build preamble, whose first line is `LANG=C`, which rpm then reads as a
# tag: "error: line 91: Unknown tag: LANG=C". A host rpmspec parses the
# same file happily, so only a direct check catches it.
ran=$((ran + 1))
bare=$(awk '/^[[:space:]]*#/ {
        stripped = $0
        gsub(/%%/, "", stripped)
        if (stripped ~ /%/) printf "%s:%d: %s\n", FILENAME, FNR, $0
    }' "$root/rpm/postivene.spec")
if [ -z "$bare" ]; then
    echo "packaging-lint: spec comments escape their macros"
else
    echo "$bare" >&2
    echo "packaging-lint: FAIL a spec comment has an unescaped % (write %%)" >&2
    status=1
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

# Every build has to be a distinguishable package.
#
# The spec pins Version and Release, and mb2 runs with -X so nothing
# derives them from git -- so without a stamp in the workflow every build
# is postivene-0.1.0-1 and `rpm -U` refuses it as already installed. A
# phone then keeps the build it has while the file claims to be new,
# which is exactly what happened.
if grep -q '^Release:' "$root/rpm/postivene.spec" &&
    ! grep -q 'sed -i "s/\^Release:' "$root/.github/workflows/rpm.yml"; then
    echo "packaging-lint: FAIL the rpm workflow no longer stamps Release, so" \
         "every build would be the same NEVRA and refuse to install over" \
         "the last" >&2
    exit 1
fi
echo "packaging-lint: the rpm workflow stamps a unique Release"
    else
        echo "$out" >&2
        echo "packaging-lint: FAIL postivene.desktop" >&2
        status=1
    fi
else
    echo "packaging-lint: SKIP desktop-file-validate (install desktop-file-utils)"
fi

# The catalog is generated from source, so it is only right if it matches
# what the source currently says. Checked by regenerating into a temporary
# file rather than by eye: a qsTr() added without a catalog entry is
# invisible until someone tries to translate the app.
if command -v lupdate >/dev/null 2>&1 || command -v lupdate-qt5 >/dev/null 2>&1; then
    ran=$((ran + 1))
    # The suffix matters: lupdate picks its format from the extension.
    fresh=$(mktemp --suffix=.ts)
    cp "$root/translations/postivene.ts" "$fresh"
    if "$root/scripts/update-translations.sh" "$fresh" >/dev/null 2>&1 &&
        diff -q "$root/translations/postivene.ts" "$fresh" >/dev/null; then
        echo "packaging-lint: translations/postivene.ts is up to date"
    else
        echo "packaging-lint: FAIL translations/postivene.ts is stale; run scripts/update-translations.sh" >&2
        status=1
    fi
    rm -f "$fresh"
else
    echo "packaging-lint: SKIP lupdate (install qttools5-dev-tools)"
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

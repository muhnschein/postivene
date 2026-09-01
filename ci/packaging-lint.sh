#!/bin/sh
# Static checks on what decides whether the RPM installs and the launcher
# works. Resolving BuildRequires or running mb2 needs the SDK
# (docs/BUILDING.md); this checks what is checkable anywhere.
set -eu

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
status=0
ran=0

# PACKAGING_LINT_STRICT=1 turns "the tool for this check is missing" into
# a failure. CI sets it, so the job cannot quietly stop checking when the
# apt line that installs the tools changes.
strict=${PACKAGING_LINT_STRICT:-0}
skip() {
    if [ "$strict" = 1 ]; then
        echo "packaging-lint: FAIL $1 is not installed ($2) (strict mode)" >&2
        status=1
    else
        echo "packaging-lint: SKIP $1 ($2)"
    fi
}

if command -v rpmspec >/dev/null 2>&1; then
    ran=$((ran + 1))
    # -P expands and parses only; BuildRequires are the SDK's job.
    if rpmspec -P "$root/rpm/harbour-postivene.spec" >/dev/null; then
        echo "packaging-lint: rpm/harbour-postivene.spec parses"
    else
        echo "packaging-lint: FAIL rpm/harbour-postivene.spec does not parse" >&2
        status=1
    fi
else
    skip rpmspec "install rpm"
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
    }' "$root/rpm/harbour-postivene.spec")
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
    out=$(desktop-file-validate "$root/harbour-postivene.desktop" 2>&1 |
        grep -v 'value "silica-qt5" for key "X-Nemo-Application-Type"' |
        grep -v 'key "X-Nemo-Application-Type" .* is not known' || true)
    if [ -z "$out" ]; then
        echo "packaging-lint: harbour-postivene.desktop valid"
    else
        echo "$out" >&2
        echo "packaging-lint: FAIL harbour-postivene.desktop" >&2
        status=1
    fi
else
    skip desktop-file-validate "install desktop-file-utils"
fi

# Every build has to be a distinguishable package.
#
# The spec pins Version and Release, and mb2 runs with -X so nothing
# derives them from git -- so without a stamp in the workflow every build
# is harbour-postivene-0.1.0-1 and `rpm -U` refuses it as already
# installed. A phone then keeps the build it has while the file claims to
# be new, which is exactly what happened.
ran=$((ran + 1))
if grep -q '^Release:' "$root/rpm/harbour-postivene.spec" &&
    ! grep -q 'sed -i "s/\^Release:' "$root/.github/workflows/rpm.yml"; then
    echo "packaging-lint: FAIL the rpm workflow no longer stamps Release, so" \
         "every build would be the same NEVRA and refuse to install over" \
         "the last" >&2
    status=1
else
    echo "packaging-lint: the rpm workflow stamps a unique Release"
fi

# mb2 derives the package it is building from the directory it is run in,
# and then looks for rpm/<that>.spec. The workflow mounts the checkout at a
# path it chooses, so that name and the spec's have to agree or the build
# stops before rpmbuild starts.
ran=$((ran + 1))
spec_base=$(basename "$(find "$root/rpm" -maxdepth 1 -name '*.spec' | sort | head -1)" .spec)
# shellcheck disable=SC2016 # $home is the workflow's text, not ours.
builddir=$(sed -n 's|.*BUILDDIR=\$home/\([^"]*\)".*|\1|p' \
    "$root/.github/workflows/rpm.yml" | head -1)
if [ "$spec_base" = "$builddir" ]; then
    echo "packaging-lint: the rpm workflow builds in a directory named for the spec"
else
    echo "packaging-lint: FAIL rpm.yml mounts the checkout as '$builddir' but the" \
         "spec is rpm/$spec_base.spec; mb2 would not find it" >&2
    status=1
fi

# The catalog is generated from source, so it is only right if it matches
# what the source currently says. Checked by regenerating into a temporary
# file rather than by eye: a qsTr() added without a catalog entry is
# invisible until someone tries to translate the app.
if command -v lupdate >/dev/null 2>&1 || command -v lupdate-qt5 >/dev/null 2>&1; then
    ran=$((ran + 1))
    # The suffix matters: lupdate picks its format from the extension.
    # A directory from mktemp and a name inside it, since `--suffix` is
    # GNU's alone and this is a /bin/sh script.
    fresh_dir=$(mktemp -d)
    fresh="$fresh_dir/postivene.ts"
    cp "$root/translations/postivene.ts" "$fresh"
    if "$root/scripts/update-translations.sh" "$fresh" >/dev/null 2>&1 &&
        diff -q "$root/translations/postivene.ts" "$fresh" >/dev/null; then
        echo "packaging-lint: translations/postivene.ts is up to date"
    else
        echo "packaging-lint: FAIL translations/postivene.ts is stale; run scripts/update-translations.sh" >&2
        status=1
    fi
    rm -rf "$fresh_dir"
else
    skip lupdate "install qttools5-dev-tools"
fi

# Every docs/<name>.md that a comment, a script or a document points at
# has to exist. Seven of them went in a tidy-up and left thirty-odd
# references behind, each sending a reader to a file that was not there.
ran=$((ran + 1))
missing=$(grep -rhoE 'docs/[A-Za-z0-9_-]+\.md' "$root" \
        --include='*.rs' --include='*.qml' --include='*.js' --include='*.sh' \
        --include='*.yml' --include='*.toml' --include='*.md' --include='*.spec' \
        --include='.gitignore' --include='Makefile' --include='*.conf' \
        --exclude-dir=.git --exclude-dir=target --exclude-dir=vendor \
        --exclude-dir=third_party |
    sort -u | while read -r ref; do
        [ -f "$root/$ref" ] || echo "  $ref"
    done)
if [ -z "$missing" ]; then
    echo "packaging-lint: every docs/*.md referenced exists"
else
    echo "$missing" >&2
    echo "packaging-lint: FAIL these documents are referenced but do not exist; repoint or restore them" >&2
    status=1
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
    skip shellcheck "install shellcheck"
fi

if [ "$ran" -eq 0 ]; then
    echo "packaging-lint: FAIL (no checker was available; this job proves nothing)" >&2
    exit 1
fi

exit "$status"

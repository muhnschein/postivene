#!/bin/bash
# Run Jolla's own validator against a built RPM, and judge the result
# against ci/harbour/waivers.conf.
#
# This is the authority `ci/harbour-check.sh` stands in for: only a built
# package shows the Requires and Provides rpm generated, the stripped
# binary's symbols, and the real file modes. It needs an RPM, so it runs in
# .github/workflows/rpm.yml rather than on every pull request.
#
# Known blockers do not fail it. They are already recorded in
# ci/harbour/waivers.conf, the source check already reports them, and a
# workflow that is red for a reason nobody intends to fix this week is a
# workflow people stop reading. Anything *not* waived fails.
#
#     ci/harbour-validate-rpm.sh <rpm>
#     ci/harbour-validate-rpm.sh --log <saved validation log>
#
# The validator is upstream's, cloned rather than vendored: ci/harbour/
# carries the rules it reads, and this is the code that reads them.
# $HARBOUR_VALIDATOR points at an existing clone.
set -u
shopt -s extglob

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
waivers="$root/ci/harbour/waivers.conf"
validator=${HARBOUR_VALIDATOR:-/tmp/harbour-validator}

usage() {
    echo "usage: $0 <rpm> | $0 --log <validation log>" >&2
    exit 2
}

log=""
rpm=""
case "${1:-}" in
    --log) [ $# -eq 2 ] || usage; log=$2 ;;
    "" | -*) usage ;;
    *) [ $# -eq 1 ] || usage; rpm=$1 ;;
esac

if [ -n "$rpm" ]; then
    [ -f "$rpm" ] || { echo "harbour-rpm: FAIL no such RPM: $rpm" >&2; exit 1; }
    if [ ! -x "$validator/rpmvalidation.sh" ]; then
        git clone --depth 1 \
            https://github.com/sailfishos/sdk-harbour-rpmvalidator.git \
            "$validator" >&2 || {
            echo "harbour-rpm: FAIL could not fetch the validator" >&2
            exit 1
        }
    fi

    # The vendored rules and the validator's own must be the same Harbour,
    # or the two checks disagree about what is allowed.
    for conf in "$root"/ci/harbour/*.conf; do
        name=$(basename "$conf")
        [ "$name" = waivers.conf ] && continue
        [ -f "$validator/$name" ] || continue
        if ! diff -q "$conf" "$validator/$name" >/dev/null; then
            echo "harbour-rpm: ci/harbour/$name is behind upstream;" \
                 "run scripts/update-harbour-rules.sh"
        fi
    done

    log=$(mktemp)
    # BATCHERBATCHERBATCHER makes it emit `KIND|subject|message` without
    # colour. It exits non-zero for warnings too, so the markers decide,
    # not the status.
    BATCHERBATCHERBATCHER=1 "$validator/rpmvalidation.sh" \
        -g "$validator" "$rpm" > "$log" 2>&1 || true
    cat "$log"
fi

[ -f "$log" ] || { echo "harbour-rpm: FAIL no validation log: $log" >&2; exit 1; }

if ! grep -q '^!END!' "$log"; then
    echo "harbour-rpm: FAIL the validator produced no verdict" >&2
    exit 1
fi

# An error is waived when a waiver's subject matches the validator's
# subject field, or appears anywhere in the line. Both forms are needed:
# a layout error names the offending path as its subject, while a linking
# error names the *binary* and puts the library in the message.
waived_line() {
    local line=$1 subject=$2 entry pat
    [ -f "$waivers" ] || return 1
    while IFS= read -r entry; do
        entry=${entry%%#*}
        # shellcheck disable=SC2086 # deliberate: id then pattern.
        set -- $entry
        [ $# -ge 2 ] || continue
        pat=$2
        # shellcheck disable=SC2053
        [[ $subject == $pat ]] && return 0
        # shellcheck disable=SC2053
        [[ $line == *$pat* ]] && return 0
    done < "$waivers"
    return 1
}

errors=0
waived=0
while IFS= read -r line; do
    subject=$(cut -d'|' -f2 <<< "$line")
    message=$(cut -d'|' -f3- <<< "$line")
    if waived_line "$line" "$subject"; then
        echo "harbour-rpm: WAIVED $subject -- $message"
        waived=$((waived + 1))
    else
        echo "harbour-rpm: FAIL $subject -- $message" >&2
        errors=$((errors + 1))
    fi
done < <(grep '^ERROR|' "$log" || true)

while IFS= read -r line; do
    echo "harbour-rpm: warning $(cut -d'|' -f2 <<< "$line") --" \
         "$(cut -d'|' -f3- <<< "$line")"
done < <(grep '^WARNING|' "$log" || true)

echo
if [ "$errors" -gt 0 ]; then
    echo "harbour-rpm: FAILED -- $errors finding(s) Harbour would reject" >&2
    echo "harbour-rpm: $waived other finding(s) are waived in ci/harbour/waivers.conf" >&2
    exit 1
fi

if [ "$waived" -gt 0 ]; then
    echo "harbour-rpm: ok -- nothing new; $waived waived finding(s) still block" \
         "submission (docs/HARBOUR.md)"
else
    echo "harbour-rpm: ok -- the validator accepts this package"
fi

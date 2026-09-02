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
        # The commit ci/harbour/UPSTREAM names, so the rules vendored here
        # and the code that reads them are the same Harbour -- and so CI
        # is not executing whatever that repository's HEAD is today.
        # scripts/update-harbour-rules.sh moves both together.
        commit=$(sed -n 's/^at commit \([0-9a-f]\{40\}\).*/\1/p' \
            "$root/ci/harbour/UPSTREAM" | head -1)
        if [ -z "$commit" ]; then
            echo "harbour-rpm: FAIL ci/harbour/UPSTREAM names no commit to pin the validator to" >&2
            exit 1
        fi
        if ! { git init -q "$validator" &&
               git -C "$validator" fetch -q --depth 1 \
                   https://github.com/sailfishos/sdk-harbour-rpmvalidator.git "$commit" &&
               git -C "$validator" checkout -q FETCH_HEAD; } >&2; then
            echo "harbour-rpm: FAIL could not fetch the validator at $commit" >&2
            exit 1
        fi
    fi

    # The vendored rules and the validator's own must be the same Harbour,
    # or the two checks disagree about what is allowed. Same commit, so a
    # difference is a vendored file edited by hand.
    for conf in "$root"/ci/harbour/*.conf; do
        name=$(basename "$conf")
        [ "$name" = waivers.conf ] && continue
        [ -f "$validator/$name" ] || continue
        if ! diff -q "$conf" "$validator/$name" >/dev/null; then
            echo "harbour-rpm: FAIL ci/harbour/$name is not the validator's own;" \
                 "run scripts/update-harbour-rules.sh" >&2
            exit 1
        fi
    done

    log=$(mktemp)
    trap 'rm -f "$log"' EXIT
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

# An error is waived when one waiver's subject pattern matches the
# validator's subject field *and* its message pattern matches the message.
# Both, deliberately. The subject alone waived every error the validator
# could ever raise about the bundled server -- a setuid bit, an RPATH, a
# dynamic link if it ever stopped being static -- when only the errors the
# file names are known and accepted. The patterns are bash globs, as
# upstream's own allow-list matching is.
waived_line() {
    local subject=$1 message=$2 entry wid wsubject wmessage
    [ -f "$waivers" ] || return 1
    while IFS= read -r entry; do
        entry=${entry%%#*}
        read -r wid wsubject wmessage <<< "$entry"
        { [ -n "${wid:-}" ] && [ -n "${wsubject:-}" ] && [ -n "${wmessage:-}" ]; } || continue
        # shellcheck disable=SC2053 # unquoted on purpose: they are globs.
        [[ $subject == $wsubject ]] && [[ $message == $wmessage ]] && return 0
    done < "$waivers"
    return 1
}

errors=0
waived=0
while IFS= read -r line; do
    subject=$(cut -d'|' -f2 <<< "$line")
    message=$(cut -d'|' -f3- <<< "$line")
    if waived_line "$subject" "$message"; then
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

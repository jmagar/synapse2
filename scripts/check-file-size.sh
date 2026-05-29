#!/usr/bin/env bash
# Prevent monolithic staged TypeScript source files from being committed.
#
# Checks staged .ts / .tsx files against an effective line limit.
# Rust (.rs) is handled by scripts/check-rust-module-size.sh (the NO-MONOLITHS
# gate) — do NOT also check .rs here, or the two gates contradict each other.
# Test files are exempt.
set -euo pipefail

MAX_TS="${MAX_TS:-300}"

is_test_file() {
    local f="$1"
    [[ "$f" =~ (\.(test|spec)\.(ts|tsx)$|/__tests__/) ]] && return 0
    return 1
}

count_effective_loc() {
    local f="$1"
    awk '
        BEGIN { count=0; in_block=0 }
        {
            line=$0
            sub(/^[[:space:]]+/, "", line)
            if (line == "") next

            if (in_block) {
                if (line ~ /\*\//) {
                    sub(/^.*\*\//, "", line)
                    sub(/^[[:space:]]+/, "", line)
                    in_block=0
                    if (line == "") next
                } else {
                    next
                }
            }

            if (line ~ /^\/\//) next

            if (line ~ /^\/\*/) {
                if (line ~ /\*\//) {
                    sub(/^\/\*.*\*\//, "", line)
                    sub(/^[[:space:]]+/, "", line)
                    if (line == "") next
                } else {
                    in_block=1
                    next
                }
            }

            count++
        }
        END { print count }
    ' "$f"
}

violations=()

while IFS= read -r file; do
    [[ -f "$file" ]] || continue
    is_test_file "$file" && continue

    case "$file" in
        *.ts|*.tsx)
            lines=$(count_effective_loc "$file")
            limit=$MAX_TS
            ;;
        *) continue ;;
    esac

    if (( lines > limit )); then
        violations+=("  ${file}: ${lines} effective lines (limit: ${limit})")
    fi
done < <(git diff --cached --name-only --diff-filter=ACM)

if (( ${#violations[@]} > 0 )); then
    echo "" >&2
    echo "Monolithic staged file(s) detected; split them into focused modules:" >&2
    printf '%s\n' "${violations[@]}" >&2
    echo "" >&2
    echo "Limit: .ts/.tsx=${MAX_TS} effective lines; test files exempt. (.rs -> check-rust-module-size.sh)" >&2
    exit 1
fi

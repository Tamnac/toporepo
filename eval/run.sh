#!/usr/bin/env bash
# Eval runner for toporepo.
# Runs queries from a test file, checks expected files/strings appear in output.
#
# Usage:
#   ./eval/run.sh                          # run all tests from default file
#   ./eval/run.sh test_id [test_id...]      # run specific tests
#   EVAL_TESTS=path.json ./eval/run.sh      # use a different test file
#
# Test file format (JSON array):
#   [{
#     "id":             "unique-id",
#     "repo":           "path/to/repo",
#     "query":          "natural language query",
#     "tokens":         1024,
#     "expect_files":   ["src/foo.rs"],       -- file paths that must appear
#     "expect_strings": ["some_function"],     -- strings that must appear anywhere
#     "note":           "what this tests"      -- shown on failure
#   }]
#
# Requires: jq, cargo (builds release if binary is stale)

set -euo pipefail
cd "$(dirname "$0")/.."

BIN=./target/release/toporepo.exe
[[ -f "$BIN" ]] || BIN=./target/release/toporepo

if [[ ! -f "$BIN" ]] || [[ $(find src -newer "$BIN" 2>/dev/null | head -1) ]]; then
    echo "Building toporepo..."
    cargo build --release --quiet
fi

TESTS=${EVAL_TESTS:-eval/tests.json}
if [[ ! -f "$TESTS" ]]; then
    echo "Test file not found: $TESTS"
    echo "Set EVAL_TESTS= or create eval/tests.json (see run.sh header for format)"
    exit 1
fi

FILTER=("$@")
count=$(jq length "$TESTS")
passed=0
failed=0
skipped=0
fail_ids=()

for ((i=0; i<count; i++)); do
    id=$(jq -r ".[$i].id" "$TESTS")

    # Filter if specific tests requested
    if [[ ${#FILTER[@]} -gt 0 ]]; then
        match=0
        for f in "${FILTER[@]}"; do [[ "$id" == "$f" ]] && match=1; done
        [[ $match -eq 0 ]] && continue
    fi

    repo=$(jq -r ".[$i].repo" "$TESTS")
    query=$(jq -r ".[$i].query" "$TESTS")
    tokens=$(jq -r ".[$i].tokens" "$TESTS")
    note=$(jq -r ".[$i].note" "$TESTS")

    # Skip if repo doesn't exist
    if [[ ! -d "$repo" ]]; then
        echo "  SKIP  $id  (repo not found: $repo)"
        skipped=$((skipped + 1))
        continue
    fi

    output=$($BIN map "$repo" -q "$query" -n "$tokens" 2>/dev/null) || true

    ok=1
    details=""

    n_files=$(jq ".[$i].expect_files | length" "$TESTS")
    for ((j=0; j<n_files; j++)); do
        ef=$(jq -r ".[$i].expect_files[$j]" "$TESTS")
        if ! echo "$output" | grep -qF "$ef"; then
            ok=0
            details+="  missing file: $ef\n"
        fi
    done

    n_str=$(jq ".[$i].expect_strings | length" "$TESTS")
    for ((j=0; j<n_str; j++)); do
        es=$(jq -r ".[$i].expect_strings[$j]" "$TESTS")
        if ! echo "$output" | grep -qF "$es"; then
            ok=0
            details+="  missing string: $es\n"
        fi
    done

    if [[ $ok -eq 1 ]]; then
        echo "  PASS  $id"
        passed=$((passed + 1))
    else
        echo "  FAIL  $id  ($note)"
        echo -e "$details"
        failed=$((failed + 1))
        fail_ids+=("$id")
    fi
done

total=$((passed + failed))
echo ""
[[ $skipped -gt 0 ]] && echo "($skipped skipped — missing repos)"
echo "$passed/$total passed"
if [[ $failed -gt 0 ]]; then
    echo "Failed: ${fail_ids[*]}"
    exit 1
fi

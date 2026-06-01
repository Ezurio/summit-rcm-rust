#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

hidden_parent=""

restore_tests() {
    if [[ -n "$hidden_parent" && -d "$hidden_parent/tests" && ! -e tests ]]; then
        mv "$hidden_parent/tests" tests
        rmdir "$hidden_parent"
    fi
}

if [[ -e tests ]]; then
    hidden_parent=$(mktemp -d .tests-hidden.XXXXXX)
    mv tests "$hidden_parent/tests"
    trap restore_tests EXIT INT TERM
fi

if [[ $# -gt 0 ]]; then
    cargo check "$@"
else
    cargo check --features "api-v2 api-legacy at-interface" --lib --bins
fi

cargo clippy --all-features -- -D warnings
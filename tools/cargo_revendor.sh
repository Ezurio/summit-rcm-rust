#!/usr/bin/env bash

set -euo pipefail

usage() {
    cat <<'EOF'
Usage: tools/cargo_revendor.sh [--latest]

Refresh the checked-in vendor tree.

Modes:
    default    Temporarily disable vendored source replacement, update Cargo.lock
               from the current Cargo.toml requirements, resolve the full workspace
               with all features enabled, then rebuild vendor/.
    --latest   Rewrite Cargo.toml dependency requirements to the newest available
               versions, resolve the full workspace with all features enabled, then
               rebuild vendor/.
EOF
}

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
config_path="$repo_root/.cargo/config.toml"
vendor_path="$repo_root/vendor"
config_backup=""
use_latest=false

case "${1-}" in
    "")
        ;;
    --latest)
        use_latest=true
        ;;
    -h|--help)
        usage
        exit 0
        ;;
    *)
        usage >&2
        exit 1
        ;;
esac

restore_config() {
    if [[ -n "$config_backup" && -f "$config_backup" ]]; then
        mv "$config_backup" "$config_path"
    fi
}

with_online_registry() {
    if [[ ! -f "$config_path" ]]; then
        "$@"
        return
    fi

    config_backup="$(mktemp "$repo_root/.cargo/config.toml.revendor.XXXXXX")"
    cp "$config_path" "$config_backup"
    trap restore_config EXIT

    awk '
        /^[[:space:]]*replace-with[[:space:]]*=[[:space:]]*"vendored-sources"[[:space:]]*$/ {
            print "# " $0
            next
        }
        { print }
    ' "$config_path" > "$config_path.tmp"
    mv "$config_path.tmp" "$config_path"

    "$@"

    restore_config
    trap - EXIT
    config_backup=""
}

refresh_lockfile() {
    if $use_latest; then
        cargo upgrade --workspace
    else
        cargo update --workspace
    fi

    cargo metadata --all-features --format-version=1 >/dev/null
}

(
    cd "$repo_root"
    with_online_registry refresh_lockfile
    rm -rf "$vendor_path"
    cargo vendor --locked
)

#!/usr/bin/env bash

set -euo pipefail

usage() {
    cat <<'EOF'
Usage: tools/cargo_revendor.sh [--resolve]

Refresh the checked-in VENDOR tree.

Modes:
  default    Re-vendor from the current Cargo.lock without changing it.
  --resolve  Temporarily disable vendored source replacement, resolve Cargo.lock
             for the full workspace with all features enabled, then re-vendor.
EOF
}

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
config_path="$repo_root/.cargo/config.toml"
vendor_path="$repo_root/VENDOR"
config_backup=""
resolve_lockfile=false

case "${1-}" in
    "")
        ;;
    --resolve)
        resolve_lockfile=true
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

if $resolve_lockfile; then
    if [[ -f "$config_path" ]]; then
        config_backup="$(mktemp "$repo_root/.cargo/config.toml.revendor.XXXXXX")"
        mv "$config_path" "$config_backup"
        trap restore_config EXIT
    fi

    (
        cd "$repo_root"
        cargo metadata --all-features --format-version=1 >/dev/null
    )

    restore_config
    trap - EXIT
    config_backup=""
fi

(
    cd "$repo_root"
    rm -rf "$vendor_path"
    cargo vendor --locked VENDOR
)
#!/usr/bin/env bash

set -euo pipefail

usage() {
    cat <<'EOF'
Usage: tools/cargo_revendor.sh [--resolve | --full-upgrade]

Refresh the checked-in VENDOR tree.

Modes:
    default    Temporarily disable vendored source replacement, run cargo update,
                         then re-vendor.
  --resolve  Temporarily disable vendored source replacement, resolve Cargo.lock
                         for the full workspace with all features enabled, then re-vendor.
    --full-upgrade
                         Temporarily disable vendored source replacement, run cargo upgrade,
                         resolve for the full workspace with all features enabled, then
                         re-vendor.
EOF
}

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
config_path="$repo_root/.cargo/config.toml"
vendor_path="$repo_root/VENDOR"
config_backup=""
resolve_lockfile=false
full_upgrade=false
run_update=true

case "${1-}" in
    "")
        ;;
    --resolve)
        resolve_lockfile=true
        ;;
    --full-upgrade)
        full_upgrade=true
        resolve_lockfile=true
        run_update=false
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

disable_vendored_source_replace() {
    if [[ ! -f "$config_path" ]]; then
        return
    fi

    awk '
        /^[[:space:]]*replace-with[[:space:]]*=[[:space:]]*"vendored-sources"[[:space:]]*$/ {
            print "# " $0
            next
        }
        { print }
    ' "$config_path" > "$config_path.tmp"
    mv "$config_path.tmp" "$config_path"
}

if $resolve_lockfile || $full_upgrade || $run_update; then
    if [[ -f "$config_path" ]]; then
        config_backup="$(mktemp "$repo_root/.cargo/config.toml.revendor.XXXXXX")"
        cp "$config_path" "$config_backup"
        trap restore_config EXIT
        disable_vendored_source_replace
    fi

    if $run_update; then
        (
            cd "$repo_root"
            cargo update --workspace
        )
    fi

    if $full_upgrade; then
        (
            cd "$repo_root"
            cargo upgrade
        )
    fi

    if $resolve_lockfile; then
        (
            cd "$repo_root"
            cargo metadata --all-features --format-version=1 >/dev/null
        )
    fi

    restore_config
    trap - EXIT
    config_backup=""
fi

(
    cd "$repo_root"
    rm -rf "$vendor_path"
    cargo vendor --locked VENDOR
)

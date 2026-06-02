#!/usr/bin/env sh

set -eu

repo_root=$(cd "$(dirname "$0")/.." && pwd)
cd "$repo_root"

cargo_home=${CARGO_HOME:-$HOME/.cargo}
export PATH="$cargo_home/bin:$PATH"

if ! cargo sbom --help >/dev/null 2>&1; then
    echo "cargo-sbom is required; install it with: cargo install cargo-sbom --locked" >&2
    exit 1
fi

mkdir -p target/release

cargo sbom --project-directory "$repo_root" > target/release/summit-rcm.spdx.json
cargo sbom --project-directory "$repo_root" --output-format cyclone_dx_json_1_5 > target/release/summit-rcm.cdx.json

spdx_bom="$repo_root/target/release/summit-rcm.spdx.json"
release_bom="$repo_root/target/release/summit-rcm.cdx.json"

if [ ! -f "$spdx_bom" ]; then
    echo "cargo sbom did not produce $spdx_bom" >&2
    exit 1
fi

if [ ! -f "$release_bom" ]; then
    echo "cargo sbom did not produce $release_bom" >&2
    exit 1
fi

echo "SPDX: $spdx_bom"
echo "CycloneDX: $release_bom"

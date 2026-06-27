# Summit RCM - split Rust crate workspace

This repository is the split-crate variant of the Summit RCM Rust service.
It keeps the same feature-gated runtime surfaces as the main `summit-rcm-rust`
tree while factoring the implementation into reusable crates:

- REST API v2 routes
- legacy REST API routes
- the serial AT command interface
- inventory-registered Rust plugin crates
- generated OpenAPI output and optional Swagger UI

## Architecture

The workspace is composed around one embeddable root library crate plus shared
library crates under `crates/`:

- `src/lib.rs` is the reusable composition root for Summit RCM. It exposes the
  default `run()` entrypoint used by the packaged daemon binary and can also be
  called from an external binary crate.
- `src/main.rs` is the thin default binary wrapper around `src/lib.rs`.
- `build.rs` emits `plugin_links.rs`, which retains each enabled plugin crate so
  its `inventory::submit!` registrations survive linking.
- `crates/summit-rcm-core/` contains shared infrastructure such as config,
  D-Bus helpers, certificates, publication macros, shutdown handling, and
  startup hooks.
- `crates/summit-rcm-web/` contains the Axum web runtime, auth/security
  middleware, HTTP response helpers, and OpenAPI integration.
- `crates/summit-rcm-at/` contains the AT parser, FSM, HTTP bridge, and serial
  runtime.
- `crates/summit-rcm-*` plugin crates each own one feature-gated plugin and its
  routes, services, AT commands, and OpenAPI publications.

Current top-level workspace layout:

```text
.
├── Cargo.toml                 # workspace root + summit-rcm lib/bin package
├── build.rs                   # generate link-retention shim for enabled plugins
├── src/
│   ├── lib.rs                 # embeddable composition root
│   ├── main.rs                # default binary wrapper
│   └── bin/generate_openapi.rs
├── crates/
│   ├── summit-rcm-core/
│   ├── summit-rcm-web/
│   ├── summit-rcm-at/
│   ├── summit-rcm-awm/
│   ├── summit-rcm-bluetooth/
│   ├── summit-rcm-bluetooth-hid/
│   ├── summit-rcm-bluetooth-vsp/
│   ├── ...
│   └── summit-rcm-update/
└── tests/
  └── app.rs                 # assembled-application integration tests
```

## Feature flags

Default features enable both web API surfaces:

- `api-v2` enables the v2 Axum routes.
- `api-legacy` enables the legacy Axum routes.

Additional runtime surfaces and capabilities are controlled independently:

- `at-interface` enables the serial AT interface.
- Plugin features include `awm`, `bluetooth`, `bluetooth-hid`,
  `bluetooth-vsp`, `chrony`, `cww`, `date-time`, `files`, `fips`,
  `firewall`, `log-forwarding`, `login`, `logs`, `network`,
  `network-manager`, `provisioning`, `radio-siso-mode`, `stunnel`, `system`,
  `unauthenticated`, and `update`.
- `all-plugins` enables the full built-in plugin set.
- `api-docs` enables generated OpenAPI output and automatically enables `swagger-ui` so hosted docs use the in-memory OpenAPI source.
- `swagger-ui` enables hosted API documentation in the web runtime.
- `notifications` is the shared websocket notification transport used by
  Bluetooth websocket support.
- `test-support` enables cross-crate testing seams used by the assembled-app
  integration tests.

Plugin features do not expose an interface by themselves. A plugin only becomes
reachable when the matching runtime surface is also enabled:

- plugin + `api-v2` publishes v2 routes
- plugin + `api-legacy` publishes legacy routes
- plugin + `at-interface` publishes AT commands

## Plugin model

This workspace currently supports one plugin model:

- feature-gated Rust plugin crates linked into the final binary and registered
  through `inventory`

The root `Cargo.toml` dependency list is the source of truth for which plugin
crates exist. `build.rs` reads the enabled plugin features and generates a tiny
link-retention shim so the linker keeps each enabled plugin crate and its
`declare_plugin!` publications.

The Bluetooth family is split across three crates:

- `summit-rcm-bluetooth` for the shared controller/device API and command dispatcher
- `summit-rcm-bluetooth-hid` for HID barcode/socket bridging
- `summit-rcm-bluetooth-vsp` for BLE serial/GATT socket bridging

## Building

The workspace is configured for offline-friendly Cargo builds:

- Cargo uses the default `target/` output directory.
- `.cargo/config.toml` sets `rustflags = ["-Dwarnings"]`, so warnings fail the build.
- Cargo resolves third-party crates from the checked-in `VENDOR/` directory.

Standard release build:

```bash
cargo build --release
```

Focused validation across supported interface and parity feature sets:

```bash
cargo check --features "api-v2 api-legacy at-interface" --all-targets
cargo check --bin summit-rcm --no-default-features --features "api-v2,api-legacy,all-plugins"
cargo check --bin summit-rcm --no-default-features --features "api-v2,api-legacy,all-plugins,swagger-ui"
cargo check --bin generate_openapi --no-default-features --features "api-v2,api-legacy,all-plugins,api-docs"
cargo check -p generate-nl80211
```

Example production-style web build with only v2 routes:

```bash
cargo build --release --no-default-features --features api-v2
```

Refresh the vendored tree from the current lockfile with:

```bash
./tools/cargo_revendor.sh
```

If dependency declarations change and `Cargo.lock` must be re-resolved first, use:

```bash
./tools/cargo_revendor.sh --resolve
```

`--resolve` temporarily disables the repo's vendored source override, resolves `Cargo.lock`
for the workspace with all features enabled, restores the vendored source config, and then
rebuilds `VENDOR/` from the updated lockfile.

To verify offline dependency resolution explicitly:

```bash
cargo metadata --locked --offline --format-version=1
```

Generate SBOMs with the standard generator; the helper writes both formats under `target/release/`:

- SPDX JSON at `target/release/summit-rcm.spdx.json`
- CycloneDX JSON at `target/release/summit-rcm.cdx.json`

Install the generator first:

```bash
cargo install cargo-sbom --locked
```

Then run:

```bash
./tools/generate_sboms.sh
```

## Host code generators

### nl80211 bindings

`crates/generate-nl80211` regenerates the nl80211 bindings used by the split
network plugin crates from a
`linux/nl80211.h` header using bindgen with the `prettyplease` formatter.

Regenerate against the host system headers:

```bash
cargo run -p generate-nl80211
```

Regenerate against a cross sysroot (Buildroot `STAGING_DIR`, Yocto `SDKTARGETSYSROOT`, etc.):

```bash
STAGING_DIR=/path/to/sysroot cargo run -p generate-nl80211
# or any of: NL80211_INCLUDE_DIR  BR2_SYSROOT  SDKTARGETSYSROOT  OECORE_TARGET_SYSROOT
```

An explicit output path can be passed as a positional argument:

```bash
cargo run -p generate-nl80211 -- /tmp/nl80211_generated.rs
```

The generator searches for `linux/nl80211.h` under the sysroot in this order:
`usr/include/linux-backports/include`, `usr/include/uapi`, `usr/include`.

## OpenAPI and Swagger UI

With `swagger-ui` enabled, the running service serves `/api/openapi.json` and `/api/docs`.
When `api-docs` is enabled, it also enables `swagger-ui`, and `/api/openapi.json` is generated from memory.
Without `api-docs`, `/api/openapi.json` is read from `/etc/summit-rcm-openapi.json` or `SUMMIT_RCM_OPENAPI_PATH`.

Generate the document without starting the server (pass the same `--features` as the target build,
plus `api-docs`):

```bash
SUMMIT_RCM_OPENAPI_OUTPUT=./openapi.json \
  cargo run --bin generate_openapi --no-default-features \
  --features api-v2,api-legacy,all-plugins,api-docs
```

The `generate_openapi` binary is built only when `api-docs` is enabled.

## Running

The web runtime starts when either `api-v2` or `api-legacy` is compiled in.
It binds to `0.0.0.0:8080` by default and reads the bind address from `SUMMIT_RCM_BIND` when present.

```bash
SUMMIT_RCM_BIND=0.0.0.0:8080 ./target/release/summit-rcm
RUST_LOG=summit_rcm=debug,tower_http=info ./target/release/summit-rcm
```

The AT interface starts when `at-interface` is enabled and the server config provides both `serial_port` and `baud_rate` under `[summit-rcm]`:

- in web + AT builds it runs in the background alongside the web server
- in AT-only builds it becomes the main long-running task

## Configuration

The service reads the same two primary configuration files as the main `summit-rcm` implementation:

- `/etc/summit-rcm.ini` for server and startup configuration
- `/etc/summit-rcm/summit-rcm-settings.ini` for persisted runtime settings

The main config file path can be overridden with `SUMMIT_RCM_SERVER_CONF_FILE`.
The settings file path can be overridden with `SUMMIT_RCM_SETTINGS_FILE`.

Common runtime knobs include:

- `SUMMIT_RCM_BIND` to override the web bind address
- `RUST_LOG` to control tracing verbosity


## Tests and parity

The workspace includes Rust tests plus parity harnesses under `tests/parity/`.

Key Rust test entry points:

- `cargo test`
- `tests/plugin_links.rs` verifies that the generated link-retention shim is
  available to binaries and integration tests.
- `tests/plugins.rs` is the assembled integration-test entrypoint that includes
  submodules under `tests/plugins/`.
- `tests/app.rs` contains full-application integration tests that run against
  the composed router when the needed features are enabled.

Common local commands:

```bash
# Fast default Rust test run
cargo test

# Link-retention sanity check only
cargo test --test plugin_links

# One integration test target
cargo test --test plugins

# Feature-gated integration tests
cargo test --test plugins --features "unauthenticated radio-siso-mode"

# Build-graph and parity feature-matrix validation used during route/plugin migrations
cargo check --features "api-v2 api-legacy at-interface" --all-targets
cargo check --bin summit-rcm --no-default-features --features "api-v2,api-legacy,all-plugins"
cargo check --bin summit-rcm --no-default-features --features "api-v2,api-legacy,all-plugins,swagger-ui"
cargo check --bin generate_openapi --no-default-features --features "api-v2,api-legacy,all-plugins,api-docs"
```

Common targeted filters:

```bash
# Provisioning-only host response parity against the sibling Python summit-rcm baseline
../summit-rcm/.venv/bin/python tests/parity/api_parity.py responses \
  --cases tests/parity/provisioning_response_cases.json \
  --python-repo ../summit-rcm \
  --legacy-python-repo ../summit-rcm \
  --plugins provisioning

# One integration test by name inside tests/plugins.rs
cargo test --test plugins v2_unauthenticated_get_does_not_return_sdcerr -- --exact

# One websocket-specific Bluetooth test
cargo test --test plugins notification_payload_matches_python_connect_serialization \
  --features "bluetooth bluetooth-websocket" -- --exact
```

Examples:

```bash
python3 tests/parity/api_parity.py openapi --python-repo ../summit-rcm
python3 tests/parity/api_parity.py responses --python-repo ../summit-rcm
python3 tests/parity/api_parity.py weblcm --python-repo ../../cp_linux-12/som-external/externals/weblcm-python --python-runtime weblcm
python3 tests/parity/api_parity.py coverage --python-repo ../summit-rcm
python3 tests/parity/api_remote_parity.py responses --rust-ssh root@target --python-ssh root@target
python3 tests/parity/api_remote_parity.py responses --rust-ip <rust-target> --python-ip <python-target> --python-runtime weblcm
python3 tests/parity/api_remote_parity.py websockets --rust-ip <rust-target> --python-ip <python-target> --python-runtime weblcm
python3 tests/parity/at_parity.py --python-repo ../summit-rcm
```

Notes:

- Most plugin-specific Rust tests are feature-gated. If a test target appears
  to run zero tests, rerun it with the needed plugin feature enabled.
- The parity scripts expect the sibling Python repository at `../summit-rcm`
  unless `--python-repo` points elsewhere.
- The parity harnesses use temporary config and TLS assets so they do not
  depend on persistent `/etc` state.

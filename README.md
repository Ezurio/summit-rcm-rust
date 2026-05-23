# Summit RCM - Rust / Axum port

This repository contains the Rust rewrite of the original Python-based Summit RCM service.
It builds a single feature-gated binary that can expose any combination of:

- REST API v2 routes
- legacy REST API routes
- the serial AT command interface
- built-in Rust plugins
- dynamically loaded `.so` plugins for web builds
- generated OpenAPI output and optional Swagger UI

## Architecture

The runtime is assembled from a small set of top-level modules:

- `src/main.rs` is the composition root. It enables the web server, AT interface, OpenAPI docs, and plugin loading through Cargo features.
- `src/app/loader.rs` is the dynamic plugin loader for web builds. It scans the configured plugin directory, loads shared libraries, and wires their published routes into the Axum router.
- `src/app/publication.rs` defines the publication metadata used by built-in plugins and dynamic plugins.
- `src/plugins/` contains built-in feature-gated plugins. These own route registration, AT command publication, and plugin-specific services.
- `src/at_interface/` contains the AT parser, FSM, command registry, and serial runtime.
- `src/services/` holds shared helpers used across plugins and interfaces, including HTTP, SSL, connection, and AT file services.
- `src/support/` contains common infrastructure such as config loading, D-Bus helpers, certificate utilities, archive helpers, response shaping, and misc utilities.
- `src/web/` contains web-only middleware and guards such as session auth and security headers.
- `src/openapi/` contains the OpenAPI generator used by the runtime docs endpoint and the standalone `generate_openapi` binary.
- `crates/plugin-api/` defines the dynamic plugin ABI.
- `crates/plugin-example/` contains a sample plugin crate that targets that ABI.

Current top-level source layout:

```text
src/
├── main.rs
├── lib.rs
├── app/               # dynamic plugin loading and publication metadata
├── at_interface/      # AT command parser, FSM, and serial runtime
├── openapi/           # optional OpenAPI generation
├── plugins/           # built-in feature-gated plugins
├── services/          # shared service helpers
├── support/           # config, dbus, archive, certificates, responses, utils
├── systemd/           # systemd helpers
└── web/               # auth and security middleware
```

## Feature flags

Default features enable both web API surfaces:

- `api-v2` enables the v2 Axum routes.
- `api-legacy` enables the legacy Axum routes.

Additional runtime surfaces and capabilities are controlled independently:

- `at-interface` enables the serial AT interface.
- Core plugin features: `network`, `network-manager`, `date-time`, `login`, `system`, `logs`, `files`, `update`.
- Optional plugin features: `awm`, `bluetooth`, `chrony`, `fips`, `firewall`, `log-forwarding`, `provisioning`, `radio-siso-mode`, `stunnel`, `unauthenticated`.
- `all-plugins` enables all built-in plugin features.
- `api-docs` enables generated OpenAPI output.
- `swagger-ui` serves Swagger UI and implies `api-docs`.

Plugin features do not expose any interface on their own. A built-in plugin only becomes reachable when the matching runtime surface is also enabled:

- plugin + `api-v2` publishes v2 routes
- plugin + `api-legacy` publishes legacy routes
- plugin + `at-interface` publishes AT commands

## Plugin model

This repository supports two plugin models:

- Built-in Rust plugins under `src/plugins/`, compiled in through Cargo features.
- Dynamic shared libraries loaded at runtime through `src/app/loader.rs` using the ABI in `crates/plugin-api`.

Dynamic plugin discovery reads `plugin_dir` from the `[summit-rcm]` section of the server config. If it is not set, the loader defaults to `/usr/lib/summit-rcm/plugins`.

Each dynamic plugin must export:

```c
extern "C" PluginHandle *summit_rcm_plugin_create(void);
```

## Building

The workspace is configured for offline-friendly Cargo builds:

- `.cargo/config.toml` sets `target-dir = "build"`, so build outputs land under `build/` instead of `target/`.
- `.cargo/config.toml` also sets `rustflags = ["-Dwarnings"]`, so warnings fail the build.
- Cargo resolves third-party crates from the checked-in `VENDOR/` directory.

Standard release build:

```bash
cargo build --release
```

Focused validation across the supported interfaces:

```bash
cargo check --features "api-v2 api-legacy at-interface" --all-targets
```

Example production-style web build with only v2 routes:

```bash
cargo build --release --no-default-features --features api-v2
```

If dependency declarations change, refresh the vendored tree with:

```bash
cargo vendor --locked VENDOR
```

To verify offline dependency resolution explicitly:

```bash
cargo metadata --locked --offline --format-version=1
```

Generate SBOMs with the standard generator; the helper writes both formats under `build/release/`:

- SPDX JSON at `build/release/summit-rcm.spdx.json`
- CycloneDX JSON at `build/release/summit-rcm.cdx.json`

Install the generator first:

```bash
cargo install cargo-sbom --locked
```

Then run:

```bash
./tools/generate_sboms.sh
```

## NetworkManager connection profile generation

The NetworkManager connection-profile route model is generated from a pinned NetworkManager source tree,
not during Cargo builds.

The generator script lives at `tools/generate_nm_connection_profile.py` and reads the preserved
NetworkManager metadata and merged settings docs from the source tree you point it at.

Regenerate the Rust connection-profile module with:

```bash
python3 tools/generate_nm_connection_profile.py \
  --nm-source /path/to/lrd-network-manager
```

This updates `src/plugins/network_manager/routes/connection_profile.rs`.

Notes:

- The NetworkManager source path is environment-specific and must be provided manually.
- This generator is intentionally build-time independent so normal Cargo builds do not depend on a host
  libnm installation or on an embedded-target filesystem.
- The expected source tree must include the preserved NetworkManager metadata inputs at
  `src/libnm-core-impl/gen-metadata-nm-settings-libnm-core.xml.in` and
  `src/libnmc-setting/settings-docs.h.in`.

## OpenAPI and Swagger UI

With `api-docs` enabled, the running service exposes generated OpenAPI JSON at `/api-docs/openapi.json`.

With `swagger-ui` enabled, the running service also serves Swagger UI at `/swagger-ui/`.

You can also generate the OpenAPI document without starting the server:

```bash
SUMMIT_RCM_OPENAPI_OUTPUT=./openapi.json \
  cargo run --bin generate_openapi --no-default-features --features api-v2,api-docs
```

The `generate_openapi` helper binary is only built when `api-docs` is enabled.

## Running

The web runtime starts when either `api-v2` or `api-legacy` is compiled in.
It binds to `0.0.0.0:8080` by default and reads the bind address from `SUMMIT_RCM_BIND` when present.

```bash
SUMMIT_RCM_BIND=0.0.0.0:8080 ./build/release/summit-rcm
RUST_LOG=summit_rcm=debug,tower_http=info ./build/release/summit-rcm
```

The AT interface starts when `at-interface` is enabled and the server config provides both `serial_port` and `baud_rate` under `[summit-rcm]`:

- in web + AT builds it runs in the background alongside the web server
- in AT-only builds it becomes the main long-running task

## Configuration

The service reads the same two primary configuration files as the Python implementation:

- `/etc/summit-rcm.ini` for server and startup configuration
- `/etc/summit-rcm/summit-rcm-settings.ini` for persisted runtime settings

The main config file path can be overridden with `SUMMIT_RCM_SERVER_CONF_FILE`.
The settings file path can be overridden with `SUMMIT_RCM_SETTINGS_FILE`.

Common runtime knobs include:

- `SUMMIT_RCM_BIND` to override the web bind address
- `RUST_LOG` to control tracing verbosity
- `plugin_dir` in `[summit-rcm]` to change the dynamic plugin search directory

## Tests and parity

The repository includes Rust tests plus Python-based parity harnesses that compare the Rust implementation against the sibling Python implementation.

Key test entry points:

- `cargo test`
- `tests/plugin_registry.rs` verifies that the built-in plugin registry generated by `build.rs` matches the modules present under `src/plugins/`.
- `tests/parity/api_parity.py` checks OpenAPI parity, response parity, coverage, and response-case scaffolding.
- `tests/parity/api_remote_parity.py` runs response parity against remote Rust and Python targets over SSH.
- `tests/parity/at_parity.py` compares serial AT behavior.
- `tests/parity/plugin_response_logic_check.py` validates plugin response logic against system state.
- `tests/parity/provisioning_tls_check.py` validates provisioning TLS behavior.

Common local commands:

```bash
# Fast default Rust test run
cargo test

# Registry sanity check only
cargo test --test plugin_registry

# One integration test target
cargo test --test plugins

# Feature-gated integration tests in tests/plugins/parity_contract.rs
cargo test --test plugins --features "unauthenticated radio-siso-mode"

# Build-graph level validation used during route/plugin migrations
cargo check --features "api-v2 api-legacy at-interface" --all-targets
```

Common targeted filters:

```bash
# One Rust test by name
cargo test builtin_plugin_registry_matches_source_tree -- --exact

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
python3 tests/parity/api_parity.py coverage --python-repo ../summit-rcm
python3 tests/parity/api_remote_parity.py responses --rust-ssh root@target --python-ssh root@target
python3 tests/parity/at_parity.py --python-repo ../summit-rcm
```

Notes:

- `tests/plugins.rs` is only the Rust integration-test entrypoint that includes submodules under `tests/plugins/`; it is not part of plugin discovery or parity coverage.
- `tests/parity/api_parity.py` discovers Rust plugin names from `src/plugins/`, so the parity harness does not test itself.
- Most plugin-specific Rust tests are feature-gated. If a test target appears to run zero tests, rerun it with the plugin feature enabled.
- The parity scripts expect the sibling Python repository at `../summit-rcm` unless `--python-repo` points elsewhere.

The parity harnesses use temporary config and TLS assets so they do not depend on persistent `/etc` state.

## Embedded target testing

See [docs/embedded-testing.md](docs/embedded-testing.md) for the full device workflow, including Buildroot rebuilds, deployment, smoke checks, remote parity, and AT testing.

Quick links:

```bash
# Rebuild from Buildroot output
make -C /devel/cp_linux/output/som60sd_fips_11_rust summit-rcm-rust-rebuild

# Remote API parity against deployed Rust and Python targets
python3 tests/parity/api_remote_parity.py responses --rust-ssh root@target --python-ssh root@target
```

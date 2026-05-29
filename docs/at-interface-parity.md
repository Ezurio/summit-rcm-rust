# AT interface parity

This document defines how we validate AT behavior parity between the Rust runtime in this repository and the Python baseline in the sibling `summit-rcm` repository.

## Scope

- Script: `tests/parity/at_parity.py`
- Case manifest: `tests/parity/at_parity_cases.json`
- Python baseline: `summit-rcm` only

AT parity does not support a `weblcm` baseline. The harness exits with an error when `--python-runtime weblcm` is selected.

## What parity means

The harness compares both runtimes at the serial AT boundary:

- Startup parity: both runtimes emit equivalent `READY` startup banners.
- Command parity: each case command resolves to equivalent terminal responses (`OK` or `ERROR`) unless runtime-specific expectations are declared.
- Response framing: command completion waits for terminal status lines, not only an idle gap.

Cases can intentionally allow differences by defining runtime-specific expectations:

- `expected_rust_response` / `expected_python_response`
- `expected_rust_regex` / `expected_python_regex`

If none of those are present for a case, Rust and Python responses must match exactly.

## Running the harness

From the Rust repository root:

```bash
python3 tests/parity/at_parity.py --python-repo ../summit-rcm
```

Run a single case:

```bash
python3 tests/parity/at_parity.py \
  --python-repo ../summit-rcm \
  --case-id at_communication_check
```

Run with explicit plugin coverage for plugin-gated AT cases:

```bash
python3 tests/parity/at_parity.py \
  --python-repo ../summit-rcm \
  --plugins log-forwarding
```

### Live serial targets

Use this mode when Rust and Python services are already running on two targets connected to host serial adapters:

```bash
python3 tests/parity/at_parity.py \
  --python-repo ../summit-rcm \
  --python-serial-path /dev/ttyUSB4 \
  --rust-serial-path /dev/ttyUSB5 \
  --baud-rate 3000000 \
  --wait-for-ready-banner
```

Live mode rules:

- Both `--python-serial-path` and `--rust-serial-path` are required together.
- `--plugins` is not supported in live mode (runtimes are already configured/running).
- Use a positive `--read-timeout-seconds` value. The harness requires `> 0`.

## Case authoring guide

Update `tests/parity/at_parity_cases.json` when adding or changing AT behavior.

Case fields:

- `id`: unique stable identifier.
- `command`: AT command string.
- `steps`: optional multi-step command sequence.
- `plugins`: optional plugin list required for the case.
- `expected_*_response` or `expected_*_regex`: optional explicit expectations.
- `strip_command_echo`: defaults to `true`; set `false` when echo behavior is part of the assertion.
- `timeout_seconds`: optional per-case override for slow commands.
- `skip_live`: optional skip marker for live serial mode.

Guidelines:

- Prefer strict parity expectations by default.
- Add runtime-specific expectations only when behavior is intentionally different and documented.
- Use regex only where data is dynamic (timestamps, latency, environment-derived values).

## Troubleshooting

- `timed out waiting for ... READY`: service did not start AT interface, serial path mismatch, or startup stalled.
- `timed out waiting for terminal status (OK/ERROR)`: command did not reach terminal status within timeout; increase timeout or inspect runtime behavior.
- `response mismatch`: compare Rust/Python transcripts and confirm whether the case should be runtime-specific.
- `unsupported baud rate for termios`: host termios does not expose the selected baud constant.

Operational tips:

- Do not run AT parity concurrently with other Cargo commands; build-lock contention can delay startup and produce false parity failures.
- For embedded targets, verify deployed configs include correct `serial_port` and `baud_rate` values before running parity.
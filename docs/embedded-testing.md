# Embedded target testing

For device-level validation, the usual workflow is:

1. rebuild the Buildroot package,
2. copy the rebuilt `summit-rcm` binary to the target,
3. restart the service,
4. run quick smoke checks or the remote parity harness.

## Rebuild from Buildroot output

From the Buildroot output directory:

```bash
make summit-rcm-rust-rebuild
```

Example:

```bash
make -C /devel/cp_linux/output/som60sd_fips_11_rust summit-rcm-rust-rebuild
```

Common rebuilt artifact locations are:

```text
output/<board>/per-package/summit-rcm-rust/target/usr/bin/summit-rcm
output/<board>/build/summit-rcm-rust-local/target/
```

If you are checking the raw Cargo build products for the package, look under the package build tree. If you want the exact binary staged into the target rootfs, use the `per-package/.../target/usr/bin/summit-rcm` path.

## Deploy to a target

Copy the rebuilt binary to the target and restart the service:

```bash
scp -O output/<board>/per-package/summit-rcm-rust/target/usr/bin/summit-rcm \
  root@<rust-target>:/usr/bin/summit-rcm

ssh root@<rust-target> 'systemctl restart summit-rcm.service && systemctl --no-pager --full status summit-rcm.service | cat'
```

Notes:

- Prefer `scp` over redirecting binary data through `ssh ... > file`.
- Use `scp -O` so transfers work on targets that do not provide an SFTP subsystem.
- If your lab setup uses password wrappers or `sshpass`, make sure host keys are already accepted.
- If the service does not come back, check `journalctl -u summit-rcm.service` on the target.

## Fast embedded smoke checks

Useful direct checks after restart:

```bash
# Endpoint reachability (may return 401 when auth is enabled)
curl -ksS -D - https://<rust-target>/api/v2/system/version -o /tmp/version.json

# Auth behavior on a protected route
curl -ksS -D - https://<rust-target>/api/v2/login/users -o -
```

For authenticated checks:

```bash
python3 - <<'PY'
import json, ssl, urllib.request, http.cookiejar

base = 'https://<rust-target>'
ctx = ssl._create_unverified_context()
jar = http.cookiejar.CookieJar()
opener = urllib.request.build_opener(
    urllib.request.HTTPCookieProcessor(jar),
    urllib.request.HTTPSHandler(context=ctx),
)

login_req = urllib.request.Request(
    f'{base}/api/v2/login',
    data=json.dumps({'username': 'root', 'password': 'summit'}).encode(),
    headers={'Content-Type': 'application/json'},
    method='POST',
)
with opener.open(login_req, timeout=12) as resp:
    print('login', resp.status, resp.read().decode())

with opener.open(urllib.request.Request(f'{base}/api/v2/system/version'), timeout=12) as resp:
    print('version', resp.status)
    print(json.dumps(json.load(resp), indent=2, sort_keys=True))
PY
```

## Remote parity against Rust and Python targets

The remote parity harness can compare a deployed Rust image against a deployed Python image over SSH:

```bash
python3 tests/parity/api_remote_parity.py responses \
  --rust-ssh root@<rust-target> \
  --python-ssh root@<python-target>
```

The script:

- creates temporary runtime files on both targets,
- starts forwarded test instances on temporary local ports,
- compares responses without depending on permanent target `/etc` state.

For direct live target checks, use the deployed-service transport instead of SSH. Current WebLCM targets are legacy-only baselines, so always pass `--python-runtime weblcm` for them.

```bash
# Live legacy-only response parity against a WebLCM Python target
python3 tests/parity/api_remote_parity.py responses \
  --rust-ip <rust-target> \
  --python-ip <python-target> \
  --python-runtime weblcm

# Live legacy websocket smoke parity against a WebLCM Python target
python3 tests/parity/api_remote_parity.py websockets \
  --rust-ip <rust-target> \
  --python-ip <python-target> \
  --python-runtime weblcm
```

Notes:

- `--python-runtime weblcm` filters `/api/v2` response cases out of remote parity.
- In `websockets` mode with `--python-runtime weblcm`, the harness probes only `/bluetoothWebsocket/ws`.
- `websockets` mode currently supports direct deployed-service transport only.

## Serial / AT testing

For detailed AT parity scope, case design, and troubleshooting, see [at-interface-parity.md](at-interface-parity.md).

For AT parity on embedded hardware:

```bash
python3 tests/parity/at_parity.py --python-repo ../summit-rcm
```

For two already-running targets connected to host serial adapters:

```bash
python3 tests/parity/at_parity.py \
  --python-repo ../summit-rcm \
  --python-serial-path /dev/ttyUSB4 \
  --rust-serial-path /dev/ttyUSB5 \
  --read-timeout-seconds 8 \
  --wait-for-ready-banner
```

In live serial mode, the harness does not start services or rewrite target config. It compares command responses from the two connected targets as-is.
Use a positive `--read-timeout-seconds` value (`> 0`). Command completion waits for terminal AT status (`OK`/`ERROR`).

To exercise the direct firmware-upload AT path on a live target:

```bash
python3 tools/check_fw_update_at.py \
  --serial-path /dev/ttyUSB5 \
  --image-path /path/to/update.swu \
  --image complete \
  --wait-for-ready-banner
```

This script sends `AT+FWSENDDIRECT=<length>,<image>`, streams the image bytes over the serial prompt, and then polls `AT+FWSTATUS`. It performs a real firmware update on the target.

If the target image is Buildroot-generated, double-check that serial settings in the generated config are correct and not double-quoted.

## Troubleshooting checklist

- Confirm the target config says the expected `socket_port` and that the service is actually listening there.
- If provisioning is enabled, make sure missing provisioning TLS assets do not block normal startup.
- If a route mismatch appears, compare the live Rust and Python targets with the remote parity script before changing code.
- For version mismatches, prefer device-backed sources already exposed by plugins over ad hoc shell parsing when possible.

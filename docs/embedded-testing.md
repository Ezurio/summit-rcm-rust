# Embedded target testing

For device-level validation, the usual workflow is:

1. rebuild the Buildroot package,
2. copy the rebuilt `summit-rcm` binary to the target,
3. restart the service,
4. run quick smoke checks or direct parity against the deployed services.

For new chats or handoff work, assume the default validation path is:

1. rebuild in Buildroot,
2. deploy the staged `per-package/.../target/usr/bin/summit-rcm` binary,
3. restart `summit-rcm.service`,
4. run direct deployed-service parity with `tests/parity/api_remote_parity.py --rust-ip ... --python-ip ...`.

Do not default to SSH-managed runtime parity unless the task explicitly requires temporary isolated runtimes.

## Rebuild from Buildroot output

From the Buildroot output directory:

```bash
make summit-rcm-rust-rebuild
```

Example:

```bash
make -C /devel/cp_linux/output/som60sd_fips_11_rust summit-rcm-rust-rebuild
```

If Buildroot fails with a complaint that `PATH` contains spaces, sanitize `PATH` before invoking `make`. A known offender is the `Code - Insiders` path segment injected by editor tooling.

Example:

```bash
PATH=/home/boris/.local/bin:/home/boris/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:/usr/games:/usr/local/games:/snap/bin:/home/boris/.lmstudio/bin \
  make -C /devel/cp_linux/output/som60sd_fips_11_rust summit-rcm-rust-rebuild
```

Common rebuilt artifact locations are:

```text
output/<board>/per-package/summit-rcm-rust/target/usr/bin/summit-rcm
output/<board>/build/summit-rcm-rust-local/build/
```

If you are checking the raw Cargo build products for the package, look under the package build tree. If you want the exact binary staged into the target rootfs, use the `per-package/.../target/usr/bin/summit-rcm` path.

## Deploy to a target

Copy the rebuilt binary to the target and restart the service:

```bash
scp -O output/<board>/per-package/summit-rcm-rust/target/usr/bin/summit-rcm \
  root@<rust-target>:/tmp/summit-rcm.new

ssh root@<rust-target> '\
  chmod 755 /tmp/summit-rcm.new && \
  systemctl stop summit-rcm.service && \
  mv /tmp/summit-rcm.new /usr/bin/summit-rcm && \
  systemctl start summit-rcm.service && \
  systemctl --no-pager --full status summit-rcm.service | cat'
```

Notes:

- Prefer `scp` over redirecting binary data through `ssh ... > file`.
- Prefer a temp-file swap over writing directly to `/usr/bin/summit-rcm`.
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

These smoke checks are for quick confirmation only. For parity and regression work, prefer the Python parity harness below.

## Direct parity against deployed Rust and Python targets

The preferred parity path compares the deployed Rust and Python services directly by IP:

```bash
python3 tests/parity/api_remote_parity.py responses \
  --rust-ip <rust-target-ip> \
  --python-ip <python-target-ip> \
  --respect-skip-live
```

This direct mode:

- connects to the real deployed services on `:443`,
- uses the repo CA from `board/configs-common/keys/rest-server/ca.crt`,
- verifies the service certificate against `test.summit.com` while connecting to the specified target IPs,
- and does not require SSH.

Useful targeted examples:

```bash
# One network status case
python3 tests/parity/api_remote_parity.py responses \
  --rust-ip <rust-target-ip> \
  --python-ip <python-target-ip> \
  --respect-skip-live \
  --case-id v2_network_status

# Several Wi-Fi and interface cases
python3 tests/parity/api_remote_parity.py responses \
  --rust-ip <rust-target-ip> \
  --python-ip <python-target-ip> \
  --respect-skip-live \
  --case-id legacy_network_status \
  --case-id legacy_accesspoints \
  --case-id v2_network_status \
  --case-id v2_network_interfaces \
  --case-id v2_network_wifi \
  --case-id v2_interface_wlo1
```

Rules for direct parity:

- use `--rust-ip` and `--python-ip` for deployed-service work
- do not use SSH targets for normal live service validation
- do not use loopback or `127.0.0.1` transport for deployed-service checks
- do not generate replacement TLS materials for direct parity
- expect some body diffs to be live-state drift rather than implementation bugs

When a direct parity case fails, classify the mismatch before changing code:

- target-state drift: IPs, MACs, DHCP lease timers, UUIDs, scan timestamps, reg domain differences between live boxes
- response-shape bug: missing fields, wrong casing, wrong collection type, extra unmanaged devices
- auth bug: one side returns `401` while the other returns `200`
- fallback/source bug: one side pulls a field from the wrong runtime source

## Managed-runtime parity over SSH

Use the SSH transport only when you explicitly need temporary isolated runtimes with controlled config instead of the deployed services:

```bash
python3 tests/parity/api_remote_parity.py responses \
  --rust-ssh root@<rust-target> \
  --python-ssh root@<python-target>
```

That managed-runtime mode:

- writes temporary runtime config on both targets,
- starts temporary Rust and Python instances on test ports,
- and is intended for controlled parity experiments rather than normal deployed-service validation.

Use this mode only when you need one of these:

- temporary config overrides
- plugin selection independent of the deployed service
- isolated mutation testing without touching the running system service
- experiments against unreleased code paths before deployment

## Serial / AT testing

For AT parity on embedded hardware:

```bash
python3 tests/parity/at_parity.py --python-repo ../summit-rcm
```

If the target image is Buildroot-generated, double-check that serial settings in the generated config are correct and not double-quoted.

## Troubleshooting checklist

- Confirm the target config says the expected `socket_port` and that the service is actually listening there.
- If provisioning is enabled, make sure missing provisioning TLS assets do not block normal startup.
- If a route mismatch appears, compare the live Rust and Python targets with the direct parity command before changing code.
- For version mismatches, prefer device-backed sources already exposed by plugins over ad hoc shell parsing when possible.
- If network parity differs, first rule out live target drift before changing Rust code.
- If Buildroot rejects the environment, sanitize `PATH` before rebuilding.
- If direct parity transport fails, verify the target cert is still the expected deployed cert and that the repo CA matches it.

## Current known workflow notes

These are the main conclusions from the current network/Wi-Fi parity work and should be enough to resume in a new chat:

- `can0` filtering was fixed in `src/plugins/network_manager/service.rs` by checking both `status.state` and `status.State`.
- The fix was rebuilt, deployed, and verified live on the Rust target.
- Direct parity for deployed services now uses `tests/parity/api_remote_parity.py responses --rust-ip ... --python-ip ...`.
- Direct parity intentionally avoids SSH, loopback forwarding, and generated TLS material.
- Current observed live difference: Rust target reported `wlan0` reg domain `US` while the Python target reported `WW`; that looked like live target drift, not the same class of bug as the `can0` issue.

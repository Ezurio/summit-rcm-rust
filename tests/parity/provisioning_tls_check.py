#!/usr/bin/env python3

from __future__ import annotations

import argparse
import hashlib
import os
import signal
import socket
import ssl
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_STARTUP_TIMEOUT_SECONDS = 60.0
DEFAULT_FEATURES = "api-v2,api-legacy,provisioning"


class CheckError(RuntimeError):
    pass


@dataclass
class ManagedProcess:
    process: subprocess.Popen[str]

    def stop(self) -> None:
        if self.process.poll() is not None:
            return
        self.process.send_signal(signal.SIGTERM)
        try:
            self.process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait(timeout=5)

    def output(self) -> str:
        if self.process.stdout is None:
            return ""
        try:
            return self.process.stdout.read()
        except Exception:
            return ""


def run_checked(command: list[str]) -> None:
    completed = subprocess.run(command, check=False, capture_output=True, text=True)
    if completed.returncode != 0:
        raise CheckError(
            "command failed:\n"
            + " ".join(command)
            + "\n"
            + (completed.stdout or "")
            + (completed.stderr or "")
        )


def reserve_local_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def cert_fingerprint(cert_pem: str) -> str:
    cert_der = ssl.PEM_cert_to_DER_cert(cert_pem)
    return hashlib.sha256(cert_der).hexdigest()


def generate_self_signed_cert(
    cert_path: Path,
    key_path: Path,
    common_name: str,
    *,
    extended_key_usage: str,
    basic_constraints: str | None = None,
    key_usage: str | None = None,
) -> None:
    command = [
        "openssl",
        "req",
        "-x509",
        "-newkey",
        "rsa:2048",
        "-nodes",
        "-keyout",
        str(key_path),
        "-out",
        str(cert_path),
        "-days",
        "1",
        "-subj",
        f"/CN={common_name}",
        "-addext",
        "subjectAltName=DNS:localhost,IP:127.0.0.1",
    ]
    if basic_constraints is not None:
        command.extend(["-addext", f"basicConstraints={basic_constraints}"])
    if key_usage is not None:
        command.extend(["-addext", f"keyUsage={key_usage}"])
    command.extend(["-addext", f"extendedKeyUsage={extended_key_usage}"])
    run_checked(command)


def generate_signed_client_cert(
    temp_dir: Path,
    *,
    ca_cert_path: Path,
    ca_key_path: Path,
    client_cert_path: Path,
    client_key_path: Path,
    common_name: str,
) -> None:
    csr_path = temp_dir / f"{common_name}.csr"
    ext_path = temp_dir / f"{common_name}.ext"

    run_checked(
        [
            "openssl",
            "req",
            "-new",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-keyout",
            str(client_key_path),
            "-out",
            str(csr_path),
            "-subj",
            f"/CN={common_name}",
        ]
    )

    ext_path.write_text(
        "\n".join(
            [
                "basicConstraints=critical,CA:false",
                "keyUsage=critical,digitalSignature",
                "extendedKeyUsage=clientAuth",
                "subjectAltName=DNS:localhost,IP:127.0.0.1",
                "subjectKeyIdentifier=hash",
                "authorityKeyIdentifier=keyid,issuer",
            ]
        )
    )

    run_checked(
        [
            "openssl",
            "x509",
            "-req",
            "-in",
            str(csr_path),
            "-CA",
            str(ca_cert_path),
            "-CAkey",
            str(ca_key_path),
            "-CAcreateserial",
            "-out",
            str(client_cert_path),
            "-days",
            "1",
            "-extfile",
            str(ext_path),
        ]
    )


def write_settings(path: Path) -> None:
    path.write_text(
        "\n".join(
            [
                "[settings]",
                "session_timeout = 10",
                "login_retry_times = 5",
                "login_retry_window = 600",
                "",
            ]
        )
    )


def write_server_config(
    path: Path,
    *,
    port: int,
    cert_path: Path,
    key_path: Path,
    ca_path: Path,
    enable_client_auth: bool = False,
    enable_client_pairing: bool = False,
    rodata_ca_cert_path: Path | None = None,
    paired_client_cert_path: Path | None = None,
) -> None:
    summit_lines = [
        "[summit-rcm]",
        f"socket_port = {port}",
        "default_username = root",
        "default_password = summit",
        "allow_multiple_user_sessions = true",
        f"enable_client_auth = {'true' if enable_client_auth else 'false'}",
        f"enable_client_pairing = {'true' if enable_client_pairing else 'false'}",
        "disable_certificate_expiry_verification = true",
    ]
    if rodata_ca_cert_path is not None:
        summit_lines.append(f"rodata_ca_cert_path = {rodata_ca_cert_path}")
    if paired_client_cert_path is not None:
        summit_lines.append(f"paired_client_cert_path = {paired_client_cert_path}")

    path.write_text(
        "\n".join(
            [
                "[/]",
                "tools.sessions.on = false",
                "",
                *summit_lines,
                "",
                "[global]",
                f"server.ssl_private_key = {key_path}",
                f"server.ssl_certificate = {cert_path}",
                f"server.ssl_certificate_chain = {ca_path}",
                "",
            ]
        )
    )


def start_server(env: dict[str, str], features: str) -> ManagedProcess:
    process = subprocess.Popen(
        [
            "cargo",
            "run",
            "--quiet",
            "--bin",
            "summit-rcm",
            "--no-default-features",
            "--features",
            features,
        ],
        cwd=ROOT,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    return ManagedProcess(process)


def wait_for_port(port: int, process: ManagedProcess, timeout_seconds: float) -> None:
    deadline = time.time() + timeout_seconds
    last_error = ""
    while time.time() < deadline:
        if process.process.poll() is not None:
            raise CheckError(f"server exited early:\n{process.output()}")
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=1):
                return
        except OSError as error:
            last_error = str(error)
            time.sleep(0.25)
    raise CheckError(f"timed out waiting for TCP listener: {last_error}")


def wait_for_certificate(port: int, process: ManagedProcess, timeout_seconds: float) -> str:
    deadline = time.time() + timeout_seconds
    last_error = ""
    while time.time() < deadline:
        if process.process.poll() is not None:
            raise CheckError(f"server exited early:\n{process.output()}")
        try:
            return ssl.get_server_certificate(("127.0.0.1", port))
        except Exception as error:
            last_error = str(error)
            time.sleep(0.25)
    raise CheckError(f"timed out waiting for TLS server certificate: {last_error}")


def run_state_check(
    *,
    state: int,
    expected_cert_path: Path,
    server_cert_path: Path,
    server_key_path: Path,
    provisioning_cert_path: Path,
    provisioning_key_path: Path,
    settings_path: Path,
    startup_timeout_seconds: float,
    features: str,
) -> None:
    port = reserve_local_port()
    temp_dir = settings_path.parent
    server_config_path = temp_dir / f"summit-rcm-{state}.ini"
    state_path = temp_dir / f"provisioning-state-{state}"
    ca_path = temp_dir / f"ca-{state}.crt"
    state_path.write_text(f"{state}\n")
    write_server_config(
        server_config_path,
        port=port,
        cert_path=server_cert_path,
        key_path=server_key_path,
        ca_path=ca_path,
    )

    env = {
        **os.environ,
        "SUMMIT_RCM_BIND": f"127.0.0.1:{port}",
        "SUMMIT_RCM_SERVER_CONF_FILE": str(server_config_path),
        "SUMMIT_RCM_SETTINGS_FILE": str(settings_path),
        "SUMMIT_RCM_PROVISIONING_STATE_FILE": str(state_path),
        "SUMMIT_RCM_PROVISIONING_SERVER_CERT": str(provisioning_cert_path),
        "SUMMIT_RCM_PROVISIONING_SERVER_KEY": str(provisioning_key_path),
    }

    process = start_server(env, features)
    try:
        presented_cert = wait_for_certificate(port, process, startup_timeout_seconds)
    finally:
        process.stop()

    presented_fingerprint = cert_fingerprint(presented_cert)
    expected_fingerprint = cert_fingerprint(expected_cert_path.read_text())
    if presented_fingerprint != expected_fingerprint:
        raise CheckError(
            f"state {state} presented wrong certificate: expected {expected_cert_path.name}, got sha256={presented_fingerprint}"
        )

    print(f"PASS provisioning state {state}: presented {expected_cert_path.name}")


def tls_handshake(
    *,
    port: int,
    server_ca_cert_path: Path,
    client_cert_path: Path | None = None,
    client_key_path: Path | None = None,
) -> None:
    context = ssl.create_default_context(cafile=str(server_ca_cert_path))
    context.check_hostname = False
    if client_cert_path is not None and client_key_path is not None:
        context.load_cert_chain(str(client_cert_path), str(client_key_path))

    with socket.create_connection(("127.0.0.1", port), timeout=5) as sock:
        with context.wrap_socket(sock, server_hostname="127.0.0.1"):
            return


def run_client_cert_validation_check(
    *,
    server_cert_path: Path,
    server_key_path: Path,
    provisioning_cert_path: Path,
    provisioning_key_path: Path,
    rodata_ca_cert_path: Path,
    valid_client_cert_path: Path,
    valid_client_key_path: Path,
    invalid_client_cert_path: Path,
    invalid_client_key_path: Path,
    settings_path: Path,
    startup_timeout_seconds: float,
    features: str,
) -> None:
    port = reserve_local_port()
    temp_dir = settings_path.parent
    server_config_path = temp_dir / "summit-rcm-client-auth.ini"
    state_path = temp_dir / "provisioning-state-client-auth"
    trust_store_path = temp_dir / "client-auth-ca.crt"
    state_path.write_text("1\n")
    write_server_config(
        server_config_path,
        port=port,
        cert_path=server_cert_path,
        key_path=server_key_path,
        ca_path=trust_store_path,
        enable_client_pairing=True,
        rodata_ca_cert_path=rodata_ca_cert_path,
        paired_client_cert_path=valid_client_cert_path,
    )

    env = {
        **os.environ,
        "SUMMIT_RCM_BIND": f"127.0.0.1:{port}",
        "SUMMIT_RCM_SERVER_CONF_FILE": str(server_config_path),
        "SUMMIT_RCM_SETTINGS_FILE": str(settings_path),
        "SUMMIT_RCM_PROVISIONING_STATE_FILE": str(state_path),
        "SUMMIT_RCM_PROVISIONING_SERVER_CERT": str(provisioning_cert_path),
        "SUMMIT_RCM_PROVISIONING_SERVER_KEY": str(provisioning_key_path),
    }

    process = start_server(env, features)
    try:
        wait_for_port(port, process, startup_timeout_seconds)

        try:
            tls_handshake(port=port, server_ca_cert_path=server_cert_path)
        except Exception:
            print("PASS client auth: handshake without client cert rejected")
        else:
            raise CheckError("client-auth server unexpectedly accepted a connection without a client certificate")

        try:
            tls_handshake(
                port=port,
                server_ca_cert_path=server_cert_path,
                client_cert_path=invalid_client_cert_path,
                client_key_path=invalid_client_key_path,
            )
        except Exception:
            print("PASS client auth: handshake with invalid client cert rejected")
        else:
            raise CheckError("client-auth server unexpectedly accepted an invalid client certificate")

        tls_handshake(
            port=port,
            server_ca_cert_path=server_cert_path,
            client_cert_path=valid_client_cert_path,
            client_key_path=valid_client_key_path,
        )
        print("PASS client auth: handshake with valid client cert accepted")
    finally:
        process.stop()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Validate Rust provisioning TLS certificate swapping.")
    parser.add_argument(
        "--startup-timeout-seconds",
        type=float,
        default=DEFAULT_STARTUP_TIMEOUT_SECONDS,
        help="Server startup timeout for each provisioning-state check.",
    )
    parser.add_argument(
        "--features",
        default=DEFAULT_FEATURES,
        help="Cargo feature list used to launch summit-rcm.",
    )
    return parser.parse_args()


def run_checks(*, startup_timeout_seconds: float, features: str) -> None:
    with tempfile.TemporaryDirectory(prefix="provisioning-tls-check-") as temp_dir_raw:
        temp_dir = Path(temp_dir_raw)
        server_cert_path = temp_dir / "server.crt"
        server_key_path = temp_dir / "server.key"
        provisioning_cert_path = temp_dir / "provisioning.crt"
        provisioning_key_path = temp_dir / "provisioning.key"
        rodata_ca_cert_path = temp_dir / "rodata-ca.crt"
        rodata_ca_key_path = temp_dir / "rodata-ca.key"
        valid_client_cert_path = temp_dir / "paired-client.crt"
        valid_client_key_path = temp_dir / "paired-client.key"
        invalid_client_cert_path = temp_dir / "invalid-client.crt"
        invalid_client_key_path = temp_dir / "invalid-client.key"
        settings_path = temp_dir / "summit-rcm-settings.ini"

        generate_self_signed_cert(
            server_cert_path,
            server_key_path,
            "server.local",
            extended_key_usage="serverAuth",
        )
        generate_self_signed_cert(
            provisioning_cert_path,
            provisioning_key_path,
            "provisioning.local",
            extended_key_usage="serverAuth",
        )
        generate_self_signed_cert(
            rodata_ca_cert_path,
            rodata_ca_key_path,
            "rodata-ca.local",
            extended_key_usage="clientAuth",
            basic_constraints="critical,CA:true",
            key_usage="critical,keyCertSign,cRLSign",
        )
        generate_signed_client_cert(
            temp_dir,
            ca_cert_path=rodata_ca_cert_path,
            ca_key_path=rodata_ca_key_path,
            client_cert_path=valid_client_cert_path,
            client_key_path=valid_client_key_path,
            common_name="paired-client",
        )
        generate_self_signed_cert(
            invalid_client_cert_path,
            invalid_client_key_path,
            "invalid-client.local",
            extended_key_usage="clientAuth",
        )
        write_settings(settings_path)

        run_state_check(
            state=0,
            expected_cert_path=provisioning_cert_path,
            server_cert_path=server_cert_path,
            server_key_path=server_key_path,
            provisioning_cert_path=provisioning_cert_path,
            provisioning_key_path=provisioning_key_path,
            settings_path=settings_path,
            startup_timeout_seconds=startup_timeout_seconds,
            features=features,
        )
        run_state_check(
            state=1,
            expected_cert_path=server_cert_path,
            server_cert_path=server_cert_path,
            server_key_path=server_key_path,
            provisioning_cert_path=provisioning_cert_path,
            provisioning_key_path=provisioning_key_path,
            settings_path=settings_path,
            startup_timeout_seconds=startup_timeout_seconds,
            features=features,
        )
        run_state_check(
            state=2,
            expected_cert_path=server_cert_path,
            server_cert_path=server_cert_path,
            server_key_path=server_key_path,
            provisioning_cert_path=provisioning_cert_path,
            provisioning_key_path=provisioning_key_path,
            settings_path=settings_path,
            startup_timeout_seconds=startup_timeout_seconds,
            features=features,
        )
        run_client_cert_validation_check(
            server_cert_path=server_cert_path,
            server_key_path=server_key_path,
            provisioning_cert_path=provisioning_cert_path,
            provisioning_key_path=provisioning_key_path,
            rodata_ca_cert_path=rodata_ca_cert_path,
            valid_client_cert_path=valid_client_cert_path,
            valid_client_key_path=valid_client_key_path,
            invalid_client_cert_path=invalid_client_cert_path,
            invalid_client_key_path=invalid_client_key_path,
            settings_path=settings_path,
            startup_timeout_seconds=startup_timeout_seconds,
            features=features,
        )


def main() -> int:
    args = parse_args()
    try:
        run_checks(
            startup_timeout_seconds=args.startup_timeout_seconds,
            features=args.features,
        )
    except CheckError as error:
        print(str(error), file=sys.stderr)
        return 1

    print("Provisioning TLS certificate swap and client-auth checks passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
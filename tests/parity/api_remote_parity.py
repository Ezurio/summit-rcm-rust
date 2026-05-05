#!/usr/bin/env python3

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shlex
import signal
import socket
import subprocess
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from api_parity import (
    DEFAULT_CASES,
    DEFAULT_REQUEST_TIMEOUT_SECONDS,
    DEFAULT_STARTUP_TIMEOUT_SECONDS,
    USER_PERMISSIONS,
    ParityError,
    find_readback_case,
    parse_plugin_names,
    python_wrapper_command,
    request_or_error,
    response_cookie,
    response_pair_mismatch,
    run_checked,
    selected_plugins,
    should_read_body,
    should_verify_readback,
    wait_for_server,
)


DEFAULT_REMOTE_RUST_PORT = 18443
DEFAULT_REMOTE_PYTHON_PORT = 28443


@dataclass
class ManagedProcess:
    name: str
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


@dataclass
class RemoteManagedProcess:
    name: str
    target: str
    ssh_options: list[str]
    pid_file: str
    process: subprocess.Popen[str]

    def stop(self) -> None:
        if self.process.poll() is None:
            run_ssh_best_effort(
                self.target,
                self.ssh_options,
                f"test ! -f {shell_quote(self.pid_file)} || kill $(cat {shell_quote(self.pid_file)}) >/dev/null 2>&1 || true",
            )
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


@dataclass(frozen=True)
class RemoteRuntimeFiles:
    remote_dir: str
    plugin_dir: str
    cert_path: str
    key_path: str
    settings_path: str
    provisioning_state_path: str
    config_path: str
    pid_file: str


@dataclass(frozen=True)
class TerminalCaseOutcome:
    accepted: bool
    detail: str


def shell_quote(value: str) -> str:
    return shlex.quote(value)


def reserve_local_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def ssh_base_command(target: str, ssh_options: list[str]) -> list[str]:
    command = ["ssh"]
    for option in ssh_options:
        command.extend(["-o", option])
    command.append(target)
    return command


def ssh_shell_command(target: str, ssh_options: list[str], remote_command: str) -> list[str]:
    return [*ssh_base_command(target, ssh_options), "sh", "-lc", remote_command]


def run_ssh_checked(
    target: str,
    ssh_options: list[str],
    remote_command: str,
    *,
    input_text: str | None = None,
) -> subprocess.CompletedProcess[str]:
    completed = subprocess.run(
        ssh_shell_command(target, ssh_options, remote_command),
        input=input_text,
        text=True,
        capture_output=True,
        check=False,
    )
    if completed.returncode != 0:
        raise ParityError(
            f"remote command failed on {target}: {remote_command}\n"
            f"stdout:\n{completed.stdout}\n"
            f"stderr:\n{completed.stderr}"
        )
    return completed


def run_ssh_best_effort(target: str, ssh_options: list[str], remote_command: str) -> None:
    subprocess.run(
        ssh_shell_command(target, ssh_options, remote_command),
        text=True,
        capture_output=True,
        check=False,
    )


def remote_temp_dir(target: str, ssh_options: list[str]) -> str:
    completed = run_ssh_checked(target, ssh_options, "mktemp -d /tmp/api-remote-parity.XXXXXX")
    return completed.stdout.strip()


def upload_text(target: str, ssh_options: list[str], remote_path: str, content: str) -> None:
    run_ssh_checked(
        target,
        ssh_options,
        f"cat > {shell_quote(remote_path)}",
        input_text=content,
    )


def ensure_remote_directory(target: str, ssh_options: list[str], remote_path: str) -> None:
    run_ssh_checked(target, ssh_options, f"mkdir -p {shell_quote(remote_path)}")


def start_tunnel(
    name: str,
    target: str,
    ssh_options: list[str],
    *,
    local_port: int,
    remote_port: int,
) -> ManagedProcess:
    command = ["ssh"]
    for option in ssh_options:
        command.extend(["-o", option])
    command.extend(
        [
            "-o",
            "ExitOnForwardFailure=yes",
            "-N",
            "-L",
            f"{local_port}:127.0.0.1:{remote_port}",
            target,
        ]
    )
    process = subprocess.Popen(
        command,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    return ManagedProcess(name, process)


def start_remote_process(
    name: str,
    target: str,
    ssh_options: list[str],
    *,
    pid_file: str,
    command: str,
    env: dict[str, str],
    workdir: str | None,
) -> RemoteManagedProcess:
    env_prefix = " ".join(
        f"{key}={shell_quote(value)}"
        for key, value in sorted(env.items())
    )
    script_parts: list[str] = []
    if workdir:
        script_parts.append(f"cd {shell_quote(workdir)}")
    script_parts.append(f"printf '%s\\n' $$ > {shell_quote(pid_file)}")
    if env_prefix:
        script_parts.append(f"exec env {env_prefix} {command}")
    else:
        script_parts.append(f"exec {command}")
    process = subprocess.Popen(
        ssh_shell_command(target, ssh_options, " && ".join(script_parts)),
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    return RemoteManagedProcess(name, target, ssh_options, pid_file, process)


def parse_env_assignments(raw_items: list[str]) -> dict[str, str]:
    env: dict[str, str] = {}
    for item in raw_items:
        if "=" not in item:
            raise ParityError(f"invalid env assignment: {item!r}; expected KEY=VALUE")
        key, value = item.split("=", 1)
        key = key.strip()
        if not key:
            raise ParityError(f"invalid env assignment: {item!r}; missing key")
        env[key] = value
    return env


def render_settings_file(password: str) -> str:
    salt = "parity-salt"
    digest = hashlib.sha256(f"{salt}{password}".encode()).hexdigest()
    permission = " ".join(USER_PERMISSIONS)
    return "\n".join(
        [
            "[settings]",
            "session_timeout = 10",
            "login_retry_times = 5",
            "login_retry_window = 600",
            "",
            "[root]",
            f"salt = {salt}",
            f"password = {digest}",
            f"permission = {permission}",
            "",
        ]
    )


def render_server_config(
    *,
    port: int,
    sessions_on: bool,
    enabled_plugins: list[str],
    plugin_dir: str,
    cert_path: str,
    key_path: str,
) -> str:
    return "\n".join(
        [
            "[/]",
            f"tools.sessions.on = {'true' if sessions_on else 'false'}",
            "",
            "[plugins]",
            *[f"{plugin} = true" for plugin in enabled_plugins],
            "",
            "[summit-rcm]",
            f"socket_port = {port}",
            "default_username = root",
            "default_password = summit",
            "allow_multiple_user_sessions = true",
            "rest_api_docs = false",
            "rest_api_docs_root_redirect = false",
            "network_status_restricted = false",
            "log_routes_loaded = false",
            "enable_client_auth = false",
            f"plugin_dir = {plugin_dir}",
            "",
            "[global]",
            f"server.ssl_private_key = {key_path}",
            f"server.ssl_certificate = {cert_path}",
            f"server.ssl_certificate_chain = {cert_path}",
            "",
        ]
    )


def generate_tls_materials(temp_dir: Path) -> tuple[Path, Path]:
    cert_path = temp_dir / "server.crt"
    key_path = temp_dir / "server.key"
    run_checked(
        [
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
            "/CN=127.0.0.1",
            "-addext",
            "subjectAltName=DNS:localhost,IP:127.0.0.1",
            "-addext",
            "extendedKeyUsage=serverAuth",
        ]
    )
    return cert_path, key_path


def is_destructive_skip(case: dict[str, Any]) -> bool:
    reason = str(case.get("skip_reason", "")).lower()
    return "not safe to execute" in reason or "destructive" in reason


def is_terminal_destructive_case(case: dict[str, Any]) -> bool:
    path = str(case.get("path", "")).lower()
    return any(token in path for token in ("factoryreset", "poweroff", "reboot", "suspend"))


def is_reboot_like_case(case: dict[str, Any]) -> bool:
    path = str(case.get("path", "")).lower()
    return any(token in path for token in ("factoryreset", "reboot"))


def is_power_loss_case(case: dict[str, Any]) -> bool:
    path = str(case.get("path", "")).lower()
    return any(token in path for token in ("poweroff", "suspend"))


def filter_cases(cases: list[dict[str, Any]], requested_case_ids: list[str]) -> list[dict[str, Any]]:
    if not requested_case_ids:
        return cases

    requested = set(requested_case_ids)
    filtered = [case for case in cases if str(case.get("id")) in requested]
    found = {str(case.get("id")) for case in filtered}
    missing = sorted(requested - found)
    if missing:
        raise ParityError(f"unknown case id(s): {', '.join(missing)}")
    return filtered


def order_cases_for_remote_run(cases: list[dict[str, Any]]) -> list[dict[str, Any]]:
    return sorted(
        cases,
        key=lambda case: (
            is_terminal_destructive_case(case),
            str(case.get("id", "")),
        ),
    )


def skip_reason_for_remote_case(
    case: dict[str, Any],
    *,
    respect_skip_live: bool,
    allow_destructive: bool,
) -> str | None:
    if not case.get("skip_live"):
        return None
    reason = str(case.get("skip_reason", "skip_live"))
    if respect_skip_live:
        return reason
    if is_destructive_skip(case) and not allow_destructive:
        return reason
    return None


def is_expected_disconnect_error(error: str) -> bool:
    lowered = error.lower()
    return any(
        token in lowered
        for token in (
            "remotedisconnected",
            "connection reset",
            "connection aborted",
            "connection refused",
            "broken pipe",
            "unexpected eof",
            "server disconnected",
            "timed out",
            "sslerror",
        )
    )


def wait_for_ssh(target: str, ssh_options: list[str], timeout_seconds: float) -> None:
    deadline = time.time() + timeout_seconds
    last_error = ""
    while time.time() < deadline:
        completed = subprocess.run(
            ssh_shell_command(target, ssh_options, "true"),
            text=True,
            capture_output=True,
            check=False,
        )
        if completed.returncode == 0:
            return
        last_error = (completed.stderr or completed.stdout).strip()
        time.sleep(1.0)
    raise ParityError(f"timed out waiting for SSH on {target}: {last_error}")


def create_remote_runtime_files(
    target: str,
    ssh_options: list[str],
    *,
    cert_text: str,
    key_text: str,
    settings_content: str,
    provisioning_state_content: str,
    config_name: str,
    port: int,
    sessions_on: bool,
    enabled_plugins: list[str],
) -> RemoteRuntimeFiles:
    remote_dir = remote_temp_dir(target, ssh_options)
    plugin_dir = f"{remote_dir}/plugins"
    cert_path = f"{remote_dir}/server.crt"
    key_path = f"{remote_dir}/server.key"
    settings_path = f"{remote_dir}/summit-rcm-settings.ini"
    provisioning_state_path = f"{remote_dir}/provisioning-state"
    config_path = f"{remote_dir}/{config_name}"
    pid_file = f"{remote_dir}/runtime.pid"

    config_content = render_server_config(
        port=port,
        sessions_on=sessions_on,
        enabled_plugins=enabled_plugins,
        plugin_dir=plugin_dir,
        cert_path=cert_path,
        key_path=key_path,
    )

    ensure_remote_directory(target, ssh_options, plugin_dir)
    upload_text(target, ssh_options, cert_path, cert_text)
    upload_text(target, ssh_options, key_path, key_text)
    upload_text(target, ssh_options, settings_path, settings_content)
    upload_text(target, ssh_options, provisioning_state_path, provisioning_state_content)
    upload_text(target, ssh_options, config_path, config_content)

    return RemoteRuntimeFiles(
        remote_dir=remote_dir,
        plugin_dir=plugin_dir,
        cert_path=cert_path,
        key_path=key_path,
        settings_path=settings_path,
        provisioning_state_path=provisioning_state_path,
        config_path=config_path,
        pid_file=pid_file,
    )


def evaluate_terminal_case_outcome(
    case: dict[str, Any],
    response: Any,
    error: str | None,
) -> TerminalCaseOutcome:
    if error is not None:
        if is_expected_disconnect_error(error):
            return TerminalCaseOutcome(True, f"disconnect ({error})")
        return TerminalCaseOutcome(False, error)

    expected_status = case.get("expected_status")
    if expected_status is not None and response.status != expected_status:
        return TerminalCaseOutcome(
            False,
            f"status {response.status} expected {expected_status}",
        )
    return TerminalCaseOutcome(True, f"status {response.status}")


def run_remote_terminal_case_once(
    label: str,
    case: dict[str, Any],
    *,
    target: str,
    ssh_options: list[str],
    command: str,
    workdir: str | None,
    env_extra: dict[str, str],
    remote_port: int,
    config_name: str,
    startup_timeout_seconds: float,
    request_timeout_seconds: float,
    sessions_on: bool,
    enabled_plugins: list[str],
    python_runtime: bool,
) -> TerminalCaseOutcome:
    local_port = reserve_local_port()

    with tempfile.TemporaryDirectory(prefix=f"api-remote-terminal-{label}-") as temp_dir_raw:
        temp_dir = Path(temp_dir_raw)
        cert_path, key_path = generate_tls_materials(temp_dir)
        cert_text = cert_path.read_text()
        key_text = key_path.read_text()
        settings_content = render_settings_file("summit")
        provisioning_state_content = "2\n"

        remote_files = create_remote_runtime_files(
            target,
            ssh_options,
            cert_text=cert_text,
            key_text=key_text,
            settings_content=settings_content,
            provisioning_state_content=provisioning_state_content,
            config_name=config_name,
            port=remote_port,
            sessions_on=sessions_on,
            enabled_plugins=enabled_plugins,
        )

        tunnel: ManagedProcess | None = None
        runtime: RemoteManagedProcess | None = None
        try:
            tunnel = start_tunnel(
                f"{label}-tunnel",
                target,
                ssh_options,
                local_port=local_port,
                remote_port=remote_port,
            )

            runtime_env = {
                "SUMMIT_RCM_BIND": f"127.0.0.1:{remote_port}",
                "SUMMIT_RCM_SERVER_CONF_FILE": remote_files.config_path,
                "SUMMIT_RCM_SETTINGS_FILE": remote_files.settings_path,
                "SUMMIT_RCM_PROVISIONING_STATE_FILE": remote_files.provisioning_state_path,
                **env_extra,
            }
            if python_runtime:
                runtime_env.setdefault("DOCS_GENERATION", "True")

            runtime = start_remote_process(
                label,
                target,
                ssh_options,
                pid_file=remote_files.pid_file,
                command=command,
                env=runtime_env,
                workdir=workdir,
            )

            base_url = wait_for_server(
                label,
                f"https://127.0.0.1:{local_port}",
                runtime,
                ca_cert_path=cert_path,
                request_timeout_seconds=request_timeout_seconds,
                startup_timeout_seconds=startup_timeout_seconds,
            )
            response, error = request_or_error(
                f"{base_url}{case['path']}",
                case["method"],
                body=case.get("body"),
                ca_cert_path=cert_path,
                timeout_seconds=float(case.get("timeout_seconds", request_timeout_seconds)),
                read_body=should_read_body(case),
            )
            return evaluate_terminal_case_outcome(case, response, error)
        finally:
            if runtime is not None:
                runtime.stop()
            if tunnel is not None:
                tunnel.stop()
            run_ssh_best_effort(target, ssh_options, f"rm -rf {shell_quote(remote_files.remote_dir)}")


def run_remote_terminal_case_parity(
    case: dict[str, Any],
    *,
    rust_target: str,
    python_target: str,
    ssh_options: list[str],
    rust_command: str,
    python_command: str,
    rust_workdir: str | None,
    python_workdir: str | None,
    rust_env_extra: dict[str, str],
    python_env_extra: dict[str, str],
    remote_rust_port: int,
    remote_python_port: int,
    request_timeout_seconds: float,
    startup_timeout_seconds: float,
    sessions_on: bool,
    enabled_plugins: list[str],
) -> None:
    if rust_target == python_target and is_power_loss_case(case):
        raise ParityError(
            f"{case['id']}: poweroff/suspend cases need separate rust/python targets; a shared target cannot continue after the first request"
        )

    rust_outcome = run_remote_terminal_case_once(
        "rust",
        case,
        target=rust_target,
        ssh_options=ssh_options,
        command=rust_command,
        workdir=rust_workdir,
        env_extra=rust_env_extra,
        remote_port=remote_rust_port,
        config_name="summit-rcm-rust.ini",
        startup_timeout_seconds=startup_timeout_seconds,
        request_timeout_seconds=request_timeout_seconds,
        sessions_on=sessions_on,
        enabled_plugins=enabled_plugins,
        python_runtime=False,
    )

    if rust_target == python_target and is_reboot_like_case(case):
        wait_for_ssh(rust_target, ssh_options, startup_timeout_seconds)

    python_outcome = run_remote_terminal_case_once(
        "python",
        case,
        target=python_target,
        ssh_options=ssh_options,
        command=python_command,
        workdir=python_workdir,
        env_extra=python_env_extra,
        remote_port=remote_python_port,
        config_name="summit-rcm.ini",
        startup_timeout_seconds=startup_timeout_seconds,
        request_timeout_seconds=request_timeout_seconds,
        sessions_on=sessions_on,
        enabled_plugins=enabled_plugins,
        python_runtime=True,
    )

    if not rust_outcome.accepted or not python_outcome.accepted:
        raise ParityError(
            f"{case['id']}: destructive parity failed, rust={rust_outcome.detail}, python={python_outcome.detail}"
        )

    print(
        f"PASS {case['id']}: {case['method']} {case['path']} "
        f"(rust={rust_outcome.detail}; python={python_outcome.detail})"
    )


def run_remote_response_cases(
    cases: list[dict[str, Any]],
    *,
    rust_target: str,
    python_target: str,
    ssh_options: list[str],
    rust_command: str,
    python_command: str,
    rust_workdir: str | None,
    python_workdir: str | None,
    rust_env_extra: dict[str, str],
    python_env_extra: dict[str, str],
    remote_rust_port: int,
    remote_python_port: int,
    request_timeout_seconds: float,
    startup_timeout_seconds: float,
    sessions_on: bool,
    enabled_plugins: list[str],
    respect_skip_live: bool,
    allow_destructive: bool,
) -> None:
    cases = order_cases_for_remote_run(cases)
    shared_cases: list[dict[str, Any]] = []
    terminal_cases: list[dict[str, Any]] = []

    for case in cases:
        skip_reason = skip_reason_for_remote_case(
            case,
            respect_skip_live=respect_skip_live,
            allow_destructive=allow_destructive,
        )
        if skip_reason is not None:
            print(f"SKIP {case['id']}: {case['method']} {case['path']} ({skip_reason})")
            continue
        if is_terminal_destructive_case(case):
            terminal_cases.append(case)
        else:
            shared_cases.append(case)

    if not shared_cases and not terminal_cases:
        print("No remote response cases selected.")
        return

    if shared_cases:
        rust_local_port = reserve_local_port()
        python_local_port = reserve_local_port()

        with tempfile.TemporaryDirectory(prefix="api-remote-parity-") as temp_dir_raw:
            temp_dir = Path(temp_dir_raw)
            cert_path, key_path = generate_tls_materials(temp_dir)
            settings_content = render_settings_file("summit")
            provisioning_state_content = "2\n"

            rust_remote_dir = remote_temp_dir(rust_target, ssh_options)
            python_remote_dir = remote_temp_dir(python_target, ssh_options)

            rust_remote_plugin_dir = f"{rust_remote_dir}/plugins"
            python_remote_plugin_dir = f"{python_remote_dir}/plugins"
            rust_remote_cert = f"{rust_remote_dir}/server.crt"
            rust_remote_key = f"{rust_remote_dir}/server.key"
            rust_remote_settings = f"{rust_remote_dir}/summit-rcm-settings.ini"
            rust_remote_state = f"{rust_remote_dir}/provisioning-state"
            rust_remote_config = f"{rust_remote_dir}/summit-rcm-rust.ini"
            rust_remote_pid = f"{rust_remote_dir}/runtime.pid"

            python_remote_cert = f"{python_remote_dir}/server.crt"
            python_remote_key = f"{python_remote_dir}/server.key"
            python_remote_settings = f"{python_remote_dir}/summit-rcm-settings.ini"
            python_remote_state = f"{python_remote_dir}/provisioning-state"
            python_remote_config = f"{python_remote_dir}/summit-rcm.ini"
            python_remote_pid = f"{python_remote_dir}/runtime.pid"

            rust_config = render_server_config(
                port=remote_rust_port,
                sessions_on=sessions_on,
                enabled_plugins=enabled_plugins,
                plugin_dir=rust_remote_plugin_dir,
                cert_path=rust_remote_cert,
                key_path=rust_remote_key,
            )
            python_config = render_server_config(
                port=remote_python_port,
                sessions_on=sessions_on,
                enabled_plugins=enabled_plugins,
                plugin_dir=python_remote_plugin_dir,
                cert_path=python_remote_cert,
                key_path=python_remote_key,
            )

            rust: RemoteManagedProcess | None = None
            python: RemoteManagedProcess | None = None
            rust_tunnel: ManagedProcess | None = None
            python_tunnel: ManagedProcess | None = None

            try:
                ensure_remote_directory(rust_target, ssh_options, rust_remote_plugin_dir)
                ensure_remote_directory(python_target, ssh_options, python_remote_plugin_dir)

                rust_cert_text = cert_path.read_text()
                rust_key_text = key_path.read_text()
                upload_text(rust_target, ssh_options, rust_remote_cert, rust_cert_text)
                upload_text(rust_target, ssh_options, rust_remote_key, rust_key_text)
                upload_text(rust_target, ssh_options, rust_remote_settings, settings_content)
                upload_text(rust_target, ssh_options, rust_remote_state, provisioning_state_content)
                upload_text(rust_target, ssh_options, rust_remote_config, rust_config)

                python_cert_text = cert_path.read_text()
                python_key_text = key_path.read_text()
                upload_text(python_target, ssh_options, python_remote_cert, python_cert_text)
                upload_text(python_target, ssh_options, python_remote_key, python_key_text)
                upload_text(python_target, ssh_options, python_remote_settings, settings_content)
                upload_text(python_target, ssh_options, python_remote_state, provisioning_state_content)
                upload_text(python_target, ssh_options, python_remote_config, python_config)

                rust_tunnel = start_tunnel(
                    "rust-tunnel",
                    rust_target,
                    ssh_options,
                    local_port=rust_local_port,
                    remote_port=remote_rust_port,
                )
                python_tunnel = start_tunnel(
                    "python-tunnel",
                    python_target,
                    ssh_options,
                    local_port=python_local_port,
                    remote_port=remote_python_port,
                )

                rust_runtime_env = {
                    "SUMMIT_RCM_BIND": f"127.0.0.1:{remote_rust_port}",
                    "SUMMIT_RCM_SERVER_CONF_FILE": rust_remote_config,
                    "SUMMIT_RCM_SETTINGS_FILE": rust_remote_settings,
                    "SUMMIT_RCM_PROVISIONING_STATE_FILE": rust_remote_state,
                    **rust_env_extra,
                }
                python_runtime_env = {
                    "DOCS_GENERATION": "True",
                    "SUMMIT_RCM_SERVER_CONF_FILE": python_remote_config,
                    "SUMMIT_RCM_SETTINGS_FILE": python_remote_settings,
                    "SUMMIT_RCM_PROVISIONING_STATE_FILE": python_remote_state,
                    **python_env_extra,
                }

                rust = start_remote_process(
                    "rust",
                    rust_target,
                    ssh_options,
                    pid_file=rust_remote_pid,
                    command=rust_command,
                    env=rust_runtime_env,
                    workdir=rust_workdir,
                )
                python = start_remote_process(
                    "python",
                    python_target,
                    ssh_options,
                    pid_file=python_remote_pid,
                    command=python_command,
                    env=python_runtime_env,
                    workdir=python_workdir,
                )

                rust_base_url = wait_for_server(
                    "rust",
                    f"https://127.0.0.1:{rust_local_port}",
                    rust,
                    ca_cert_path=cert_path,
                    request_timeout_seconds=request_timeout_seconds,
                    startup_timeout_seconds=startup_timeout_seconds,
                )
                python_base_url = wait_for_server(
                    "python",
                    f"https://127.0.0.1:{python_local_port}",
                    python,
                    ca_cert_path=cert_path,
                    request_timeout_seconds=request_timeout_seconds,
                    startup_timeout_seconds=startup_timeout_seconds,
                )

                failures: list[str] = []
                rust_cookies: dict[str, str] = {}
                python_cookies: dict[str, str] = {}

                for case in shared_cases:
                    cookie_key = case.get("use_cookie_from")
                    read_body = should_read_body(case)
                    case_timeout_seconds = float(case.get("timeout_seconds", request_timeout_seconds))
                    readback_case = None
                    rust_readback_before = None
                    python_readback_before = None

                    if should_verify_readback(case):
                        readback_case = find_readback_case(case, shared_cases)
                        if readback_case is not None:
                            readback_cookie_key = readback_case.get("use_cookie_from") or cookie_key
                            readback_timeout_seconds = float(
                                readback_case.get("timeout_seconds", request_timeout_seconds)
                            )
                            readback_read_body = should_read_body(readback_case)
                            rust_readback_before, rust_readback_before_error = request_or_error(
                                f"{rust_base_url}{readback_case['path']}",
                                readback_case["method"],
                                body=readback_case.get("body"),
                                cookie=rust_cookies.get(readback_cookie_key)
                                if readback_cookie_key
                                else None,
                                ca_cert_path=cert_path,
                                timeout_seconds=readback_timeout_seconds,
                                read_body=readback_read_body,
                            )
                            python_readback_before, python_readback_before_error = request_or_error(
                                f"{python_base_url}{readback_case['path']}",
                                readback_case["method"],
                                body=readback_case.get("body"),
                                cookie=python_cookies.get(readback_cookie_key)
                                if readback_cookie_key
                                else None,
                                ca_cert_path=cert_path,
                                timeout_seconds=readback_timeout_seconds,
                                read_body=readback_read_body,
                            )
                            if rust_readback_before_error or python_readback_before_error:
                                failures.append(
                                    f"{case['id']}: pre-readback failed, rust={rust_readback_before_error or 'ok'}, python={python_readback_before_error or 'ok'}"
                                )
                                continue

                    rust_response, rust_error = request_or_error(
                        f"{rust_base_url}{case['path']}",
                        case["method"],
                        body=case.get("body"),
                        cookie=rust_cookies.get(cookie_key) if cookie_key else None,
                        ca_cert_path=cert_path,
                        timeout_seconds=case_timeout_seconds,
                        read_body=read_body,
                    )
                    python_response, python_error = request_or_error(
                        f"{python_base_url}{case['path']}",
                        case["method"],
                        body=case.get("body"),
                        cookie=python_cookies.get(cookie_key) if cookie_key else None,
                        ca_cert_path=cert_path,
                        timeout_seconds=case_timeout_seconds,
                        read_body=read_body,
                    )

                    if rust_error or python_error:
                        failures.append(
                            f"{case['id']}: request failed, rust={rust_error or 'ok'}, python={python_error or 'ok'}"
                        )
                        continue

                    store_cookie_key = case.get("store_cookie_as")
                    if store_cookie_key:
                        rust_cookie = response_cookie(rust_response)
                        python_cookie = response_cookie(python_response)
                        if not rust_cookie or not python_cookie:
                            failures.append(
                                f"{case['id']}: missing session cookie, rust={bool(rust_cookie)}, python={bool(python_cookie)}"
                            )
                            continue
                        rust_cookies[store_cookie_key] = rust_cookie
                        python_cookies[store_cookie_key] = python_cookie

                    mismatch = response_pair_mismatch(
                        case["id"],
                        "rust",
                        rust_response,
                        "python",
                        python_response,
                        case["compare"],
                        expected_status=case.get("expected_status"),
                        allow_framework_validation_mismatch=bool(
                            case.get("allow_framework_validation_mismatch")
                        ),
                    )
                    if mismatch is not None:
                        failures.append(mismatch)
                        continue

                    if readback_case is not None:
                        readback_cookie_key = readback_case.get("use_cookie_from") or cookie_key
                        readback_timeout_seconds = float(
                            readback_case.get("timeout_seconds", request_timeout_seconds)
                        )
                        readback_read_body = should_read_body(readback_case)
                        rust_readback_after, rust_readback_after_error = request_or_error(
                            f"{rust_base_url}{readback_case['path']}",
                            readback_case["method"],
                            body=readback_case.get("body"),
                            cookie=rust_cookies.get(readback_cookie_key)
                            if readback_cookie_key
                            else None,
                            ca_cert_path=cert_path,
                            timeout_seconds=readback_timeout_seconds,
                            read_body=readback_read_body,
                        )
                        python_readback_after, python_readback_after_error = request_or_error(
                            f"{python_base_url}{readback_case['path']}",
                            readback_case["method"],
                            body=readback_case.get("body"),
                            cookie=python_cookies.get(readback_cookie_key)
                            if readback_cookie_key
                            else None,
                            ca_cert_path=cert_path,
                            timeout_seconds=readback_timeout_seconds,
                            read_body=readback_read_body,
                        )
                        if rust_readback_after_error or python_readback_after_error:
                            failures.append(
                                f"{case['id']}: post-readback failed, rust={rust_readback_after_error or 'ok'}, python={python_readback_after_error or 'ok'}"
                            )
                            continue

                        mismatch = response_pair_mismatch(
                            f"{case['id']} readback",
                            "rust",
                            rust_readback_after,
                            "python",
                            python_readback_after,
                            readback_case["compare"],
                            expected_status=readback_case.get("expected_status"),
                        )
                        if mismatch is not None:
                            failures.append(mismatch)
                            continue

                        if (
                            rust_readback_before is not None
                            and python_readback_before is not None
                            and rust_response.status >= 400
                        ):
                            mismatch = response_pair_mismatch(
                                f"{case['id']} rust-state",
                                "before",
                                rust_readback_before,
                                "after",
                                rust_readback_after,
                                readback_case["compare"],
                            )
                            if mismatch is not None:
                                failures.append(mismatch)
                                continue

                            mismatch = response_pair_mismatch(
                                f"{case['id']} python-state",
                                "before",
                                python_readback_before,
                                "after",
                                python_readback_after,
                                readback_case["compare"],
                            )
                            if mismatch is not None:
                                failures.append(mismatch)
                                continue

                    print(f"PASS {case['id']}: {case['method']} {case['path']}")

                if failures:
                    raise ParityError("Remote response parity failed:\n" + "\n".join(failures))

                print("Remote shared response parity passed.")
            finally:
                if rust is not None:
                    rust.stop()
                if python is not None:
                    python.stop()
                if rust_tunnel is not None:
                    rust_tunnel.stop()
                if python_tunnel is not None:
                    python_tunnel.stop()
                run_ssh_best_effort(rust_target, ssh_options, f"rm -rf {shell_quote(rust_remote_dir)}")
                run_ssh_best_effort(python_target, ssh_options, f"rm -rf {shell_quote(python_remote_dir)}")

    for case in terminal_cases:
        run_remote_terminal_case_parity(
            case,
            rust_target=rust_target,
            python_target=python_target,
            ssh_options=ssh_options,
            rust_command=rust_command,
            python_command=python_command,
            rust_workdir=rust_workdir,
            python_workdir=python_workdir,
            rust_env_extra=rust_env_extra,
            python_env_extra=python_env_extra,
            remote_rust_port=remote_rust_port,
            remote_python_port=remote_python_port,
            request_timeout_seconds=request_timeout_seconds,
            startup_timeout_seconds=startup_timeout_seconds,
            sessions_on=sessions_on,
            enabled_plugins=enabled_plugins,
        )

    print("Remote response parity passed.")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run API response parity with both Summit RCM runtimes managed remotely over SSH."
    )
    parser.add_argument(
        "mode",
        choices=["responses"],
        help="The remote parity mode to run.",
    )
    parser.add_argument(
        "--cases",
        type=Path,
        default=DEFAULT_CASES,
        help="Path to the representative response case manifest.",
    )
    parser.add_argument(
        "--case-id",
        action="append",
        default=[],
        help="Run only the named case id. Repeat to select multiple cases.",
    )
    parser.add_argument(
        "--rust-ssh",
        required=True,
        help="SSH target for the remote Rust runtime, for example root@device.",
    )
    parser.add_argument(
        "--python-ssh",
        help="SSH target for the remote Python runtime. Defaults to --rust-ssh.",
    )
    parser.add_argument(
        "--ssh-option",
        action="append",
        default=["BatchMode=yes", "ConnectTimeout=5", "ConnectionAttempts=1"],
        help="Additional ssh -o option to apply to every SSH connection.",
    )
    parser.add_argument(
        "--rust-command",
        default="/usr/bin/summit-rcm",
        help="Shell command used to start the remote Rust runtime.",
    )
    parser.add_argument(
        "--python-command",
        default=f"python3 -c {shell_quote(python_wrapper_command())}",
        help="Shell command used to start the remote Python runtime.",
    )
    parser.add_argument(
        "--rust-workdir",
        help="Remote working directory for the Rust command.",
    )
    parser.add_argument(
        "--python-workdir",
        help="Remote working directory for the Python command.",
    )
    parser.add_argument(
        "--rust-env",
        action="append",
        default=[],
        help="Extra KEY=VALUE environment assignments for the Rust runtime.",
    )
    parser.add_argument(
        "--python-env",
        action="append",
        default=[],
        help="Extra KEY=VALUE environment assignments for the Python runtime.",
    )
    parser.add_argument(
        "--request-timeout-seconds",
        type=float,
        default=DEFAULT_REQUEST_TIMEOUT_SECONDS,
        help="Per-request timeout for live response parity checks.",
    )
    parser.add_argument(
        "--startup-timeout-seconds",
        type=float,
        default=DEFAULT_STARTUP_TIMEOUT_SECONDS,
        help="Server startup timeout for live response parity checks.",
    )
    parser.add_argument(
        "--sessions-on",
        action="store_true",
        help="Enable session middleware in the temporary parity config.",
    )
    parser.add_argument(
        "--plugins",
        action="append",
        default=[],
        help="Comma-separated plugin names to mark enabled in the temporary parity config.",
    )
    parser.add_argument(
        "--rust-port",
        type=int,
        default=DEFAULT_REMOTE_RUST_PORT,
        help="Remote loopback port for the Rust runtime.",
    )
    parser.add_argument(
        "--python-port",
        type=int,
        default=DEFAULT_REMOTE_PYTHON_PORT,
        help="Remote loopback port for the Python runtime.",
    )
    parser.add_argument(
        "--respect-skip-live",
        action="store_true",
        help="Honor every skip_live case exactly as the local parity harness does.",
    )
    parser.add_argument(
        "--allow-destructive",
        action="store_true",
        help="Run skip_live cases whose reason marks them unsafe or destructive.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    cases = filter_cases(json.loads(args.cases.read_text()), args.case_id)
    requested_plugins = parse_plugin_names(args.plugins)
    enabled_plugins = selected_plugins(cases, requested_plugins)
    python_target = args.python_ssh or args.rust_ssh

    selected_destructive = [case["id"] for case in cases if is_terminal_destructive_case(case)]
    if selected_destructive and not args.allow_destructive:
        raise SystemExit(
            "selected destructive case(s) require --allow-destructive: "
            + ", ".join(selected_destructive)
        )

    try:
        if args.mode == "responses":
            run_remote_response_cases(
                cases,
                rust_target=args.rust_ssh,
                python_target=python_target,
                ssh_options=args.ssh_option,
                rust_command=args.rust_command,
                python_command=args.python_command,
                rust_workdir=args.rust_workdir,
                python_workdir=args.python_workdir,
                rust_env_extra=parse_env_assignments(args.rust_env),
                python_env_extra=parse_env_assignments(args.python_env),
                remote_rust_port=args.rust_port,
                remote_python_port=args.python_port,
                request_timeout_seconds=args.request_timeout_seconds,
                startup_timeout_seconds=args.startup_timeout_seconds,
                sessions_on=args.sessions_on,
                enabled_plugins=enabled_plugins,
                respect_skip_live=args.respect_skip_live,
                allow_destructive=args.allow_destructive,
            )
    except ParityError as error:
        print(str(error), file=os.sys.stderr)
        return 1

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
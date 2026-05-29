#!/usr/bin/env python3

from __future__ import annotations

import argparse
import errno
import json
import os
import pty
import re
import select
import signal
import subprocess
import sys
import tempfile
import termios
import time
import tty
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from api_parity import (
    DEFAULT_PYTHON_REPO,
    PLUGIN_SPECS,
    PYTHON_RUNTIME_CHOICES,
    ParityError,
    ensure_rust_binary,
    parse_plugin_names,
    python_parity_path,
    python_runtime_executable,
    resolve_python_runtime,
)


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_CASES = Path(__file__).with_name("at_parity_cases.json")
DEFAULT_BAUD_RATE = 115200
DEFAULT_READ_TIMEOUT_SECONDS = 8.0
DEFAULT_STARTUP_TIMEOUT_SECONDS = 20.0
DEFAULT_IDLE_TIMEOUT_SECONDS = 0.25
TERMINAL_STATUS_LINES = {"OK", "ERROR"}


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
        chunks: list[bytes] = []
        try:
            while True:
                try:
                    chunk = os.read(self.process.stdout.fileno(), 4096)
                except BlockingIOError:
                    break
                if not chunk:
                    break
                chunks.append(chunk)
        except Exception:
            return ""
        return b"".join(chunks).decode("utf-8", "replace")


@dataclass
class SerialEndpoint:
    master_fd: int
    slave_path: str
    should_close: bool = True

    def close(self) -> None:
        if not self.should_close:
            return
        try:
            os.close(self.master_fd)
        except OSError:
            pass


def rust_at_features(plugins: list[str]) -> str:
    feature_names = ["api-v2", "api-legacy", "at-interface", *[PLUGIN_SPECS[plugin].rust_feature for plugin in plugins]]
    return ",".join(feature_names)


def plugin_names_from_case(case: dict[str, Any]) -> list[str]:
    raw_plugins = case.get("plugins", [])
    if isinstance(raw_plugins, str):
        return parse_plugin_names([raw_plugins])
    if isinstance(raw_plugins, list):
        return parse_plugin_names([str(plugin) for plugin in raw_plugins])
    raise ParityError(f"invalid plugins field for case {case.get('id', '<unknown>')}")


def selected_plugins(cases: list[dict[str, Any]], requested_plugins: list[str]) -> list[str]:
    plugins = set(requested_plugins)
    for case in cases:
        plugins.update(plugin_names_from_case(case))
    return sorted(plugins)


def write_test_config(
    temp_dir: Path,
    rust_serial_path: str,
    python_serial_path: str,
    *,
    baud_rate: int,
    enabled_plugins: list[str],
) -> tuple[Path, Path, Path]:
    cert_path = temp_dir / "server.crt"
    key_path = temp_dir / "server.key"
    settings_path = temp_dir / "summit-rcm-settings.ini"
    rust_config_path = temp_dir / "rust-summit-rcm.ini"
    python_config_path = temp_dir / "python-summit-rcm.ini"
    plugin_dir = temp_dir / "plugins"
    plugin_dir.mkdir(parents=True, exist_ok=True)

    subprocess.run(
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
        ],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )

    settings_path.write_text(
        "\n".join(
            [
                "[settings]",
                "session_timeout = 10",
                "login_retry_times = 5",
                "login_retry_window = 600",
                "",
                "[root]",
                "salt = parity-salt",
                "password = ignored",
                "permission = status_networking networking_connections networking_edit networking_activate networking_ap_activate networking_certificates system_user system_settings system_firmware system_logs",
                "",
            ]
        )
    )

    def write_server_config(path: Path, serial_path: str) -> None:
        path.write_text(
            "\n".join(
                [
                    "[/]",
                    "tools.sessions.on = false",
                    "",
                    "[plugins]",
                    *[f"{plugin} = true" for plugin in enabled_plugins],
                    "",
                    "[summit-rcm]",
                    f"serial_port = {serial_path}",
                    f"baud_rate = {baud_rate}",
                    "default_username = root",
                    "default_password = summit",
                    "allow_multiple_user_sessions = true",
                    "network_status_restricted = false",
                    "log_routes_loaded = false",
                    f"plugin_dir = {plugin_dir}",
                    "",
                    "[global]",
                    f"server.ssl_private_key = {key_path}",
                    f"server.ssl_certificate = {cert_path}",
                    f"server.ssl_certificate_chain = {cert_path}",
                    "",
                ]
            )
        )

    write_server_config(rust_config_path, rust_serial_path)
    write_server_config(python_config_path, python_serial_path)
    return rust_config_path, python_config_path, settings_path


def write_python_stub_modules(temp_dir: Path) -> Path:
    stub_dir = temp_dir / "python-stubs"
    stub_dir.mkdir(parents=True, exist_ok=True)
    (stub_dir / "swclient.py").write_text(
        "def _missing(*args, **kwargs):\n"
        "    raise RuntimeError('swclient is unavailable in AT parity environment')\n"
        "\n"
        "prepare_fw_update = _missing\n"
        "do_fw_update = _missing\n"
        "open_progress_ipc = _missing\n"
        "read_progress_ipc = _missing\n"
        "close_progress_ipc = _missing\n"
        "end_fw_update = _missing\n"
    )
    return stub_dir


def create_serial_endpoint() -> SerialEndpoint:
    master_fd, slave_fd = pty.openpty()
    slave_path = os.ttyname(slave_fd)
    tty.setraw(slave_fd)
    os.set_blocking(master_fd, False)
    os.close(slave_fd)
    return SerialEndpoint(master_fd=master_fd, slave_path=slave_path)


def _termios_baudrate(baud_rate: int) -> int:
    constant_name = f"B{baud_rate}"
    value = getattr(termios, constant_name, None)
    if value is None:
        raise ParityError(f"unsupported baud rate for termios: {baud_rate}")
    return value


def open_serial_device(serial_path: str, *, baud_rate: int) -> SerialEndpoint:
    try:
        fd = os.open(serial_path, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
    except OSError as error:
        raise ParityError(f"failed to open serial device {serial_path}: {error}") from error

    try:
        attrs = termios.tcgetattr(fd)
        baud = _termios_baudrate(baud_rate)
        attrs[0] = 0
        attrs[1] = 0
        attrs[2] = attrs[2] | termios.CLOCAL | termios.CREAD
        attrs[3] = 0
        attrs[4] = baud
        attrs[5] = baud
        attrs[6][termios.VMIN] = 0
        attrs[6][termios.VTIME] = 0
        termios.tcsetattr(fd, termios.TCSANOW, attrs)
        tty.setraw(fd)
        os.set_blocking(fd, False)
    except Exception as error:
        os.close(fd)
        raise ParityError(f"failed to configure serial device {serial_path}: {error}") from error

    return SerialEndpoint(master_fd=fd, slave_path=serial_path, should_close=True)


def flush_serial_input(endpoint: SerialEndpoint, *, drain_seconds: float = 0.25) -> None:
    end = time.time() + drain_seconds
    while time.time() < end:
        ready, _, _ = select.select([endpoint.master_fd], [], [], 0.05)
        if not ready:
            continue
        data = read_master_fd(endpoint)
        if not data:
            break


def read_master_fd(endpoint: SerialEndpoint) -> bytes:
    try:
        return os.read(endpoint.master_fd, 4096)
    except OSError as error:
        if error.errno in {errno.EAGAIN, errno.EWOULDBLOCK}:
            return b""
        if error.errno == errno.EIO:
            return b""
        raise


def start_process(name: str, command: list[str], *, cwd: Path, env: dict[str, str]) -> ManagedProcess:
    process = subprocess.Popen(
        command,
        cwd=cwd,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    if process.stdout is not None:
        os.set_blocking(process.stdout.fileno(), False)
    return ManagedProcess(name, process)


def python_wrapper_command() -> str:
    return "\n".join(
        [
            "import asyncio",
            "from summit_rcm.at_interface.at_interface import ATInterface",
            "",
            "async def main():",
            "    interface = ATInterface()",
            "    await interface.start()",
            "    await asyncio.Event().wait()",
            "",
            "asyncio.run(main())",
        ]
    )


def wait_for_ready(endpoint: SerialEndpoint, process: ManagedProcess, *, timeout_seconds: float) -> str:
    deadline = time.time() + timeout_seconds
    chunks: list[bytes] = []
    while time.time() < deadline:
        if process.process.poll() is not None:
            raise ParityError(f"{process.name} exited early:\n{process.output()}")
        ready, _, _ = select.select([endpoint.master_fd], [], [], 0.25)
        if not ready:
            continue
        data = read_master_fd(endpoint)
        if not data:
            continue
        chunks.append(data)
        transcript = normalize_serial_text(b"".join(chunks).decode("utf-8", "replace"))
        if "READY" in transcript.split("\n"):
            return transcript
    raise ParityError(
        f"timed out waiting for {process.name} READY\n"
        f"serial transcript: {normalize_serial_text(b''.join(chunks).decode('utf-8', 'replace'))!r}\n"
        f"process output:\n{process.output()}"
    )


def normalize_serial_text(text: str) -> str:
    normalized = text.replace("\r\n", "\n").replace("\r", "\n")
    lines = normalized.split("\n")
    while lines and lines[0] == "":
        lines.pop(0)
    while lines and lines[-1] == "":
        lines.pop()
    return "\n".join(lines)


def sanitize_serial_text(text: str) -> str:
    return "".join(ch for ch in text if ch == "\n" or ch == "\r" or (" " <= ch <= "~"))


def has_terminal_at_status(response_text: str) -> bool:
    lines = [
        line.strip()
        for line in normalize_serial_text(sanitize_serial_text(response_text)).split("\n")
        if line.strip()
    ]
    if not lines:
        return False
    return any(line in TERMINAL_STATUS_LINES for line in lines)


def read_serial_response(
    endpoint: SerialEndpoint,
    *,
    timeout_seconds: float,
    idle_timeout_seconds: float,
) -> str:
    if timeout_seconds <= 0:
        raise ParityError("read timeout must be > 0")

    deadline = time.time() + timeout_seconds
    chunks: list[bytes] = []
    saw_data = False
    while True:
        now = time.time()
        if now >= deadline:
            break
        wait_timeout = max(0.0, deadline - now)
        ready, _, _ = select.select([endpoint.master_fd], [], [], wait_timeout)
        if not ready:
            continue
        data = read_master_fd(endpoint)
        if not data:
            continue
        saw_data = True
        chunks.append(data)
        response_text = b"".join(chunks).decode("utf-8", "replace")
        if has_terminal_at_status(response_text):
            return normalize_serial_text(sanitize_serial_text(response_text))

    if not saw_data:
        raise ParityError(
            f"timed out waiting for serial response after {timeout_seconds:.2f}s"
        )

    raise ParityError(
        f"timed out waiting for terminal status (OK/ERROR) after {timeout_seconds:.2f}s"
    )


def send_command(
    endpoint: SerialEndpoint,
    command: str,
    *,
    timeout_seconds: float,
    idle_timeout_seconds: float,
) -> str:
    # Drop any late bytes from the previous command so each request starts at a clean boundary.
    flush_serial_input(
        endpoint,
        drain_seconds=max(0.05, min(0.25, idle_timeout_seconds)),
    )
    os.write(endpoint.master_fd, f"{command}\r".encode())
    return read_serial_response(
        endpoint,
        timeout_seconds=timeout_seconds,
        idle_timeout_seconds=idle_timeout_seconds,
    )


def normalize_case_response(command: str, response: str, *, strip_command_echo: bool) -> str:
    if not strip_command_echo:
        return response

    lines = response.split("\n")
    if lines and lines[0] == command:
        lines = lines[1:]
        while lines and lines[0] == "":
            lines = lines[1:]
    return "\n".join(lines)


def load_cases(cases_path: Path, case_ids: list[str]) -> list[dict[str, Any]]:
    cases = json.loads(cases_path.read_text())
    normalized_case_ids = [case_id.strip() for case_id in case_ids if case_id and case_id.strip()]
    if not normalized_case_ids:
        return cases
    requested = set(normalized_case_ids)
    selected = [case for case in cases if case["id"] in requested]
    missing = sorted(requested - {case["id"] for case in selected})
    if missing:
        raise ParityError(f"unknown case ids: {', '.join(missing)}")
    return selected


def validate_expected_response(
    case: dict[str, Any],
    *,
    runtime: str,
    actual_response: str,
) -> str | None:
    expected_regex = case.get(f"expected_{runtime}_regex")
    if expected_regex is not None:
        if re.fullmatch(expected_regex, actual_response) is not None:
            return None
        return "\n".join(
            [
                f"{case['id']}: unexpected {runtime} response for {case['command']}",
                f"  expected_regex={expected_regex!r}",
                f"  actual={actual_response!r}",
            ]
        )

    expected_response = case.get(f"expected_{runtime}_response")
    if expected_response is None:
        return None
    if actual_response == expected_response:
        return None

    # Keep compatibility with older case literals that omitted terminal status.
    if not expected_response.endswith("\nOK") and not expected_response.endswith("\nERROR"):
        normalized_expected = normalize_serial_text(expected_response)
        normalized_actual = normalize_serial_text(actual_response)
        if normalized_actual.startswith(f"{normalized_expected}\n") and re.search(
            r"(?:^|\n)(?:OK|ERROR)$",
            normalized_actual,
        ) is not None:
            return None

    if expected_response in {"OK", "ERROR"} and actual_response.startswith(expected_response):
        return None
    return "\n".join(
        [
            f"{case['id']}: unexpected {runtime} response for {case['command']}",
            f"  expected={expected_response!r}",
            f"  actual={actual_response!r}",
        ]
    )


def has_runtime_specific_expectation(case: dict[str, Any]) -> bool:
    return any(
        case.get(key) is not None
        for key in (
            "expected_rust_response",
            "expected_python_response",
            "expected_rust_regex",
            "expected_python_regex",
        )
    )


def execute_case_command(
    case: dict[str, Any],
    *,
    rust_endpoint: SerialEndpoint,
    python_endpoint: SerialEndpoint,
    read_timeout_seconds: float,
    idle_timeout_seconds: float,
) -> str | None:
    timeout_seconds = float(case.get("timeout_seconds", read_timeout_seconds))

    try:
        rust_response = send_command(
            rust_endpoint,
            case["command"],
            timeout_seconds=timeout_seconds,
            idle_timeout_seconds=idle_timeout_seconds,
        )
    except ParityError as error:
        return f"{case['id']}: rust timeout/error for {case['command']}: {error}"

    try:
        python_response = send_command(
            python_endpoint,
            case["command"],
            timeout_seconds=timeout_seconds,
            idle_timeout_seconds=idle_timeout_seconds,
        )
    except ParityError as error:
        return f"{case['id']}: python timeout/error for {case['command']}: {error}"

    strip_command_echo = bool(case.get("strip_command_echo", True))
    rust_response = normalize_case_response(
        case["command"],
        rust_response,
        strip_command_echo=strip_command_echo,
    )
    python_response = normalize_case_response(
        case["command"],
        python_response,
        strip_command_echo=strip_command_echo,
    )

    rust_failure = validate_expected_response(
        case,
        runtime="rust",
        actual_response=rust_response,
    )
    if rust_failure is not None:
        return rust_failure

    python_failure = validate_expected_response(
        case,
        runtime="python",
        actual_response=python_response,
    )
    if python_failure is not None:
        return python_failure

    if not has_runtime_specific_expectation(case) and rust_response != python_response:
        return "\n".join(
            [
                f"{case['id']}: response mismatch for {case['command']}",
                f"  rust={rust_response!r}",
                f"  python={python_response!r}",
            ]
        )

    return None


def compare_responses(
    python_repo: Path,
    cases: list[dict[str, Any]],
    *,
    python_runtime: str,
    baud_rate: int,
    startup_timeout_seconds: float,
    read_timeout_seconds: float,
    idle_timeout_seconds: float,
    requested_plugins: list[str],
    rust_serial_path: str | None,
    python_serial_path: str | None,
    wait_for_ready_banner: bool,
) -> None:
    if python_runtime != "summit-rcm":
        raise ParityError("AT parity is only supported with the summit-rcm Python baseline; weblcm does not support AT.")

    using_live_serial_targets = rust_serial_path is not None or python_serial_path is not None
    if using_live_serial_targets and (not rust_serial_path or not python_serial_path):
        raise ParityError("both --rust-serial-path and --python-serial-path are required when using live serial targets")

    if using_live_serial_targets and requested_plugins:
        raise ParityError("--plugins is not supported with live serial targets because runtimes are already running")

    enabled_plugins = selected_plugins(cases, requested_plugins)
    python_executable = python_runtime_executable(python_repo)
    features = rust_at_features(enabled_plugins)
    rust_binary = ensure_rust_binary(bin_name="summit-rcm", features=features)

    if using_live_serial_targets:
        rust_endpoint = open_serial_device(rust_serial_path, baud_rate=baud_rate)
        python_endpoint = open_serial_device(python_serial_path, baud_rate=baud_rate)
        try:
            flush_serial_input(rust_endpoint)
            flush_serial_input(python_endpoint)

            if wait_for_ready_banner:
                rust_ready = read_serial_response(
                    rust_endpoint,
                    timeout_seconds=startup_timeout_seconds,
                    idle_timeout_seconds=idle_timeout_seconds,
                )
                python_ready = read_serial_response(
                    python_endpoint,
                    timeout_seconds=startup_timeout_seconds,
                    idle_timeout_seconds=idle_timeout_seconds,
                )
                if rust_ready != python_ready:
                    raise ParityError(
                        "AT startup parity failed on live targets:\n"
                        f"  rust={rust_ready!r}\n"
                        f"  python={python_ready!r}"
                    )

            failures: list[str] = []
            total_cases = len(cases)
            passed_cases = 0
            failed_cases = 0
            skipped_cases = 0
            for case in cases:
                if case.get("skip_live"):
                    reason = case.get("skip_reason")
                    suffix = f" ({reason})" if reason else ""
                    label = case.get("command") or case.get("id")
                    print(f"SKIP {case['id']}: {label}{suffix}")
                    skipped_cases += 1
                    continue

                steps = case.get("steps")
                if isinstance(steps, list):
                    step_failed = False
                    for index, step in enumerate(steps, start=1):
                        step_case = dict(case)
                        step_case.pop("steps", None)
                        step_case.update(step)
                        step_case["id"] = f"{case['id']}[{index}]"
                        failure = execute_case_command(
                            step_case,
                            rust_endpoint=rust_endpoint,
                            python_endpoint=python_endpoint,
                            read_timeout_seconds=read_timeout_seconds,
                            idle_timeout_seconds=idle_timeout_seconds,
                        )
                        if failure is not None:
                            failures.append(failure)
                            failed_cases += 1
                            step_failed = True
                            break
                    if not step_failed:
                        print(f"PASS {case['id']}: {len(steps)} steps")
                        passed_cases += 1
                    continue

                try:
                    failure = execute_case_command(
                        case,
                        rust_endpoint=rust_endpoint,
                        python_endpoint=python_endpoint,
                        read_timeout_seconds=read_timeout_seconds,
                        idle_timeout_seconds=idle_timeout_seconds,
                    )
                except Exception as error:
                    failure = f"{case['id']}: harness runtime error for {case['command']}: {error}"
                if failure is not None:
                    failures.append(failure)
                    failed_cases += 1
                    continue

                print(f"PASS {case['id']}: {case['command']}")
                passed_cases += 1

            print(
                f"SUMMARY total={total_cases} passed={passed_cases} failed={failed_cases} skipped={skipped_cases}"
            )

            if failures:
                raise ParityError("AT command parity failed:\n" + "\n".join(failures))

            print("AT command parity passed.")
        finally:
            rust_endpoint.close()
            python_endpoint.close()
        return

    with tempfile.TemporaryDirectory(prefix="at-parity-") as temp_dir_raw:
        temp_dir = Path(temp_dir_raw)
        rust_endpoint = create_serial_endpoint()
        python_endpoint = create_serial_endpoint()
        python_stub_dir = write_python_stub_modules(temp_dir)
        rust_config_path, python_config_path, settings_path = write_test_config(
            temp_dir,
            rust_endpoint.slave_path,
            python_endpoint.slave_path,
            baud_rate=baud_rate,
            enabled_plugins=enabled_plugins,
        )

        rust_process: ManagedProcess | None = None
        python_process: ManagedProcess | None = None
        try:
            rust_process = start_process(
                "rust",
                [str(rust_binary)],
                cwd=ROOT,
                env={
                    **os.environ,
                    "SUMMIT_RCM_SERVER_CONF_FILE": str(rust_config_path),
                    "SUMMIT_RCM_SETTINGS_FILE": str(settings_path),
                },
            )
            python_process = start_process(
                "python",
                [python_executable, "-c", python_wrapper_command()],
                cwd=python_repo,
                env={
                    **os.environ,
                    "PYTHONPATH": os.pathsep.join(
                        [str(python_stub_dir), python_parity_path(python_repo, enabled_plugins, python_runtime)]
                    ),
                    "SUMMIT_RCM_SERVER_CONF_FILE": str(python_config_path),
                    "SUMMIT_RCM_SETTINGS_FILE": str(settings_path),
                },
            )

            rust_ready = wait_for_ready(rust_endpoint, rust_process, timeout_seconds=startup_timeout_seconds)
            python_ready = wait_for_ready(python_endpoint, python_process, timeout_seconds=startup_timeout_seconds)
            if rust_ready != python_ready:
                raise ParityError(
                    "AT startup parity failed:\n"
                    f"  rust={rust_ready!r}\n"
                    f"  python={python_ready!r}"
                )

            failures: list[str] = []
            total_cases = len(cases)
            passed_cases = 0
            failed_cases = 0
            skipped_cases = 0
            for case in cases:
                if case.get("skip_live"):
                    reason = case.get("skip_reason")
                    suffix = f" ({reason})" if reason else ""
                    label = case.get("command") or case.get("id")
                    print(f"SKIP {case['id']}: {label}{suffix}")
                    skipped_cases += 1
                    continue

                steps = case.get("steps")
                if isinstance(steps, list):
                    step_failed = False
                    for index, step in enumerate(steps, start=1):
                        step_case = dict(case)
                        step_case.pop("steps", None)
                        step_case.update(step)
                        step_case["id"] = f"{case['id']}[{index}]"
                        failure = execute_case_command(
                            step_case,
                            rust_endpoint=rust_endpoint,
                            python_endpoint=python_endpoint,
                            read_timeout_seconds=read_timeout_seconds,
                            idle_timeout_seconds=idle_timeout_seconds,
                        )
                        if failure is not None:
                            failures.append(failure)
                            failed_cases += 1
                            step_failed = True
                            break
                    if not step_failed:
                        print(f"PASS {case['id']}: {len(steps)} steps")
                        passed_cases += 1
                    continue

                try:
                    failure = execute_case_command(
                        case,
                        rust_endpoint=rust_endpoint,
                        python_endpoint=python_endpoint,
                        read_timeout_seconds=read_timeout_seconds,
                        idle_timeout_seconds=idle_timeout_seconds,
                    )
                except Exception as error:
                    failure = f"{case['id']}: harness runtime error for {case['command']}: {error}"
                if failure is not None:
                    failures.append(failure)
                    failed_cases += 1
                    continue

                print(f"PASS {case['id']}: {case['command']}")
                passed_cases += 1

            print(
                f"SUMMARY total={total_cases} passed={passed_cases} failed={failed_cases} skipped={skipped_cases}"
            )

            if failures:
                raise ParityError("AT command parity failed:\n" + "\n".join(failures))

            print("AT command parity passed.")
        finally:
            if rust_process is not None:
                rust_process.stop()
            if python_process is not None:
                python_process.stop()
            rust_endpoint.close()
            python_endpoint.close()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Compare Rust and Python Summit RCM AT command parity over serial.")
    parser.add_argument(
        "--python-repo",
        type=Path,
        default=DEFAULT_PYTHON_REPO,
        help="Path to the Python summit-rcm repo.",
    )
    parser.add_argument(
        "--python-runtime",
        choices=PYTHON_RUNTIME_CHOICES,
        default="auto",
        help="Python baseline to use. AT parity only supports summit-rcm.",
    )
    parser.add_argument(
        "--cases",
        type=Path,
        default=DEFAULT_CASES,
        help="Path to the AT parity case manifest.",
    )
    parser.add_argument(
        "--case-id",
        action="append",
        default=[],
        help="Run only the specified case id. Can be repeated.",
    )
    parser.add_argument(
        "--baud-rate",
        type=int,
        default=DEFAULT_BAUD_RATE,
        help="Baud rate to configure for the temporary pseudo-terminals.",
    )
    parser.add_argument(
        "--startup-timeout-seconds",
        type=float,
        default=DEFAULT_STARTUP_TIMEOUT_SECONDS,
        help="Startup timeout while waiting for READY from each runtime.",
    )
    parser.add_argument(
        "--read-timeout-seconds",
        type=float,
        default=DEFAULT_READ_TIMEOUT_SECONDS,
        help="Maximum time to wait for each AT command response while waiting for OK/ERROR terminal status.",
    )
    parser.add_argument(
        "--idle-timeout-seconds",
        type=float,
        default=DEFAULT_IDLE_TIMEOUT_SECONDS,
        help="How long the serial line must stay quiet before a response is considered complete.",
    )
    parser.add_argument(
        "--plugins",
        action="append",
        default=[],
        help="Comma-separated plugin names to enable for AT parity checks.",
    )
    parser.add_argument(
        "--rust-serial-path",
        help="Use an already-running Rust target connected via this host serial device (for example /dev/ttyUSB5).",
    )
    parser.add_argument(
        "--python-serial-path",
        help="Use an already-running Python target connected via this host serial device (for example /dev/ttyUSB4).",
    )
    parser.add_argument(
        "--wait-for-ready-banner",
        action="store_true",
        help="In live serial mode, read one startup banner chunk from each endpoint before running cases.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        if args.read_timeout_seconds <= 0:
            raise ParityError("--read-timeout-seconds must be > 0")
        python_repo = args.python_repo.resolve()
        python_runtime = resolve_python_runtime(python_repo, args.python_runtime)
        compare_responses(
            python_repo,
            load_cases(args.cases.resolve(), args.case_id),
            python_runtime=python_runtime,
            baud_rate=args.baud_rate,
            startup_timeout_seconds=args.startup_timeout_seconds,
            read_timeout_seconds=args.read_timeout_seconds,
            idle_timeout_seconds=args.idle_timeout_seconds,
            requested_plugins=parse_plugin_names(args.plugins),
            rust_serial_path=args.rust_serial_path,
            python_serial_path=args.python_serial_path,
            wait_for_ready_banner=args.wait_for_ready_banner,
        )
    except ParityError as error:
        print(str(error), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
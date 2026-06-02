#!/usr/bin/env python3

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import shlex
import signal
import socket
import ssl
import subprocess
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from api_parity import (
    DEFAULT_CASES,
    DEFAULT_REQUEST_TIMEOUT_SECONDS,
    DEFAULT_STARTUP_TIMEOUT_SECONDS,
    PYTHON_RUNTIME_CHOICES,
    USER_PERMISSIONS,
    ParityError,
    create_parity_ssl_context,
    detect_wireless_interface,
    effective_ignore_json_keys,
    find_readback_case,
    filter_cases_for_python_runtime,
    maybe_prime_datetime_for_case,
    path_uses_wireless_interface_placeholder,
    parse_plugin_names,
    python_wrapper_command,
    request_or_error,
    resolve_wireless_interface_path,
    response_cookie,
    response_pair_mismatch,
    run_checked,
    session_mode_skip_reason,
    selected_plugins,
    should_read_body,
    should_verify_readback,
    wait_for_server,
)


DEFAULT_REMOTE_RUST_PORT = 18443
DEFAULT_REMOTE_PYTHON_PORT = 28443
DEFAULT_POWER_LOSS_OFFLINE_TIMEOUT_SECONDS = 10.0
DIRECT_HTTPS_HOST = os.environ.get("SUMMIT_API_HOST", "test.summit.com")
DIRECT_HTTPS_CA = Path(
    os.environ.get(
        "SUMMIT_API_CA",
        str(Path(__file__).resolve().parents[2].parent.parent / "board/configs-common/keys/rest-server/ca.crt"),
    )
)
TARGET_SERVER_CERT_PATH = "/etc/summit-rcm/ssl/server.crt"
TARGET_SERVER_KEY_PATH = "/etc/summit-rcm/ssl/server.key"
TARGET_SERVER_CA_PATH = "/etc/summit-rcm/ssl/ca.crt"
DEFAULT_LOGIN_BODY = {"username": "root", "password": "summit"}
WEBSOCKET_ACCEPT_GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"


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


@dataclass(frozen=True)
class WebSocketProbeResult:
    status: int | None
    detail: str


def shell_quote(value: str) -> str:
    return shlex.quote(value)


def ssh_target_host(target: str) -> str:
    return target.rsplit("@", 1)[-1]


def https_base_url(port: int) -> str:
    return f"https://{DIRECT_HTTPS_HOST}:{port}"


def websocket_paths_for_python_runtime(python_runtime: str) -> list[str]:
    if python_runtime == "weblcm":
        return ["/bluetoothWebsocket/ws"]
    return ["/api/v2/bluetooth/ws", "/bluetoothWebsocket/ws"]


def websocket_enable_path(path: str) -> str | None:
    if path == "/api/v2/bluetooth/ws":
        return "/api/v2/bluetooth/controller0"
    if path == "/bluetoothWebsocket/ws":
        return "/bluetooth/controller0"
    return None


def enable_direct_websocket_notifications(
    *,
    base_url: str,
    connect_host: str,
    cookie: str | None,
    timeout_seconds: float,
    path: str,
) -> str | None:
    enable_path = websocket_enable_path(path)
    if enable_path is None:
        return None
    response, error = request_or_error(
        f"{base_url}{enable_path}",
        "PUT",
        body={"command": "bleEnableWebsockets"},
        cookie=cookie,
        ca_cert_path=DIRECT_HTTPS_CA,
        connect_host=connect_host,
        timeout_seconds=timeout_seconds,
    )
    if error is not None:
        return error
    if response is None:
        return "no response"
    if response.status >= 400:
        return f"status {response.status}"
    return None


def ssh_base_command(target: str, ssh_options: list[str]) -> list[str]:
    command = ["ssh"]
    for option in ssh_options:
        command.extend(["-o", option])
    command.append(target)
    return command


def ssh_shell_command(target: str, ssh_options: list[str], remote_command: str) -> list[str]:
    return [
        *ssh_base_command(target, ssh_options),
        f"sh -lc {shell_quote(remote_command)}",
    ]


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
            "",
        ]
    )


def is_destructive_skip(case: dict[str, Any]) -> bool:
    reason = str(case.get("skip_reason", "")).lower()
    return "not safe to execute" in reason or "destructive" in reason


def power_action_for_case(case: dict[str, Any]) -> str:
    path = str(case.get("path", "")).lower()
    if path == "/api/v2/system/power":
        body = case.get("body")
        if isinstance(body, dict):
            return str(body.get("state", "")).lower()
    return path


def is_terminal_destructive_case(case: dict[str, Any]) -> bool:
    action = power_action_for_case(case)
    return any(token in action for token in ("factoryreset", "poweroff", "reboot", "suspend"))


def is_reboot_like_case(case: dict[str, Any]) -> bool:
    return "reboot" in power_action_for_case(case)


def is_power_loss_case(case: dict[str, Any]) -> bool:
    action = power_action_for_case(case)
    return any(token in action for token in ("poweroff", "suspend"))


def cookie_scope_for_path(path: str) -> str:
    return "v2" if str(path).startswith("/api/v2/") else "legacy"


def is_login_request(case: dict[str, Any]) -> bool:
    path = str(case.get("path", ""))
    return case.get("method", "").upper() == "POST" and path in {"/login", "/api/v2/login"}


def is_logout_request(case: dict[str, Any]) -> bool:
    path = str(case.get("path", ""))
    return case.get("method", "").upper() == "DELETE" and path in {"/login", "/api/v2/login"}


def default_cookie_key_for_case(case: dict[str, Any]) -> str | None:
    return "__default__"


def default_store_cookie_key_for_case(case: dict[str, Any]) -> str | None:
    return "__default__"


def request_cookie_value(
    cookies: dict[str, str],
    case: dict[str, Any],
) -> str | None:
    cookie_key = case.get("use_cookie_from") or default_cookie_key_for_case(case)
    if not cookie_key:
        return None
    return cookies.get(cookie_key)


def store_cookie_value(
    cookies: dict[str, str],
    case: dict[str, Any],
    response: Any,
) -> bool:
    cookie_key = case.get("store_cookie_as") or default_store_cookie_key_for_case(case)
    if not cookie_key:
        return True
    cookie = response_cookie(response)
    if not cookie:
        if is_login_request(case) and cookie_key in cookies:
            return True
        return False
    cookies[cookie_key] = cookie
    return True


def clear_cookie_value(cookies: dict[str, str], case: dict[str, Any]) -> None:
    cookie_key = case.get("use_cookie_from") or default_cookie_key_for_case(case)
    if cookie_key:
        cookies.pop(cookie_key, None)


def clear_all_cookie_values(cookies: dict[str, str]) -> None:
    cookies.clear()


def logout_direct_sessions_best_effort(
    *,
    base_url: str,
    connect_host: str,
    cookies: dict[str, str],
    timeout_seconds: float,
) -> None:
    def logout_path_for_cookie_key(cookie_key: str) -> str:
        lowered = cookie_key.lower()
        if lowered in {"/api/v2/login", "__v2__"} or "v2" in lowered:
            return "/api/v2/login"
        return "/login"

    if not cookies:
        return

    attempted: set[tuple[str, str]] = set()
    for cookie_key, cookie in cookies.items():
        if not cookie:
            continue
        logout_path = logout_path_for_cookie_key(cookie_key)
        signature = (logout_path, cookie)
        if signature in attempted:
            continue
        attempted.add(signature)
        request_or_error(
            f"{base_url}{logout_path}",
            "DELETE",
            cookie=cookie,
            ca_cert_path=DIRECT_HTTPS_CA,
            connect_host=connect_host,
            timeout_seconds=timeout_seconds,
            read_body=False,
        )


def attempt_fresh_direct_login(
    *,
    rust_base_url: str,
    python_base_url: str,
    rust_ip: str,
    python_ip: str,
    timeout_seconds: float,
    cookie_key: str,
    rust_cookies: dict[str, str],
    python_cookies: dict[str, str],
    login_path: str,
) -> str | None:
    clear_all_cookie_values(rust_cookies)
    clear_all_cookie_values(python_cookies)
    return attempt_direct_login(
        rust_base_url=rust_base_url,
        python_base_url=python_base_url,
        rust_ip=rust_ip,
        python_ip=python_ip,
        timeout_seconds=timeout_seconds,
        cookie_key=cookie_key,
        rust_cookies=rust_cookies,
        python_cookies=python_cookies,
        login_path=login_path,
    )


def direct_login_path_for_case(case: dict[str, Any], python_runtime: str) -> str:
    if python_runtime == "weblcm":
        return "/login"
    return "/api/v2/login" if cookie_scope_for_path(case.get("path", "")) == "v2" else "/login"


def login_response_indicates_already_logged(response: Any) -> bool:
    try:
        payload = json.loads((response.body or b"").decode("utf-8", errors="replace"))
    except Exception:
        return False

    if not isinstance(payload, dict):
        return False

    if payload.get("SDCERR") == 2:
        return True

    info_msg = str(payload.get("InfoMsg", "")).lower()
    return "already logged" in info_msg


def login_cookie_with_recovery(
    *,
    base_url: str,
    connect_host: str,
    timeout_seconds: float,
    login_path: str,
    existing_cookie: str | None,
) -> tuple[str | None, str | None]:
    login_response, login_error = request_or_error(
        f"{base_url}{login_path}",
        "POST",
        body=DEFAULT_LOGIN_BODY,
        ca_cert_path=DIRECT_HTTPS_CA,
        connect_host=connect_host,
        timeout_seconds=timeout_seconds,
        read_body=True,
    )
    if login_error is not None:
        return None, login_error

    cookie = response_cookie(login_response)
    if cookie:
        return cookie, None

    if existing_cookie and login_response_indicates_already_logged(login_response):
        return existing_cookie, None

    if not login_response_indicates_already_logged(login_response):
        return None, "login response missing session cookie"

    # Some targets can return "already logged in" without Set-Cookie.
    # Force a logout+login cycle to mint a fresh cookie.
    request_or_error(
        f"{base_url}{login_path}",
        "DELETE",
        cookie=existing_cookie,
        ca_cert_path=DIRECT_HTTPS_CA,
        connect_host=connect_host,
        timeout_seconds=timeout_seconds,
        read_body=False,
    )

    fresh_login_response, fresh_login_error = request_or_error(
        f"{base_url}{login_path}",
        "POST",
        body=DEFAULT_LOGIN_BODY,
        ca_cert_path=DIRECT_HTTPS_CA,
        connect_host=connect_host,
        timeout_seconds=timeout_seconds,
        read_body=True,
    )
    if fresh_login_error is not None:
        return None, fresh_login_error

    fresh_cookie = response_cookie(fresh_login_response)
    if fresh_cookie:
        return fresh_cookie, None

    if existing_cookie and login_response_indicates_already_logged(fresh_login_response):
        return existing_cookie, None

    return None, "login response missing session cookie after recovery"


def attempt_direct_login(
    *,
    rust_base_url: str,
    python_base_url: str,
    rust_ip: str,
    python_ip: str,
    timeout_seconds: float,
    cookie_key: str,
    rust_cookies: dict[str, str],
    python_cookies: dict[str, str],
    login_path: str,
) -> str | None:
    rust_cookie, rust_error = login_cookie_with_recovery(
        base_url=rust_base_url,
        connect_host=rust_ip,
        timeout_seconds=timeout_seconds,
        login_path=login_path,
        existing_cookie=rust_cookies.get(cookie_key),
    )
    python_cookie, python_error = login_cookie_with_recovery(
        base_url=python_base_url,
        connect_host=python_ip,
        timeout_seconds=timeout_seconds,
        login_path=login_path,
        existing_cookie=python_cookies.get(cookie_key),
    )

    if rust_error is not None or python_error is not None:
        return f"auto-login failed, rust={rust_error or 'ok'}, python={python_error or 'ok'}"

    if not rust_cookie or not python_cookie:
        return f"auto-login missing session cookie, rust={bool(rust_cookie)}, python={bool(python_cookie)}"

    rust_cookies[cookie_key] = rust_cookie
    python_cookies[cookie_key] = python_cookie
    return None


def login_cookie_for_websocket(
    *,
    base_url: str,
    connect_host: str,
    timeout_seconds: float,
    login_path: str,
) -> str | None:
    response, error = request_or_error(
        f"{base_url}{login_path}",
        "POST",
        body=DEFAULT_LOGIN_BODY,
        ca_cert_path=DIRECT_HTTPS_CA,
        connect_host=connect_host,
        timeout_seconds=timeout_seconds,
        read_body=True,
    )
    if error is not None:
        raise ParityError(f"websocket login failed for {login_path}: {error}")
    return response_cookie(response)


def restart_direct_services(*, rust_ip: str, python_ip: str, python_runtime: str) -> None:
    python_service = "weblcm-python.service" if python_runtime == "weblcm" else "summit-rcm.service"
    for ip, service in ((rust_ip, "summit-rcm.service"), (python_ip, python_service)):
        completed = subprocess.run(
            ["ssh", f"root@{ip}", f"systemctl restart {service} && systemctl is-active {service}"],
            text=True,
            capture_output=True,
            check=False,
        )
        if completed.returncode != 0:
            raise ParityError(
                f"failed to restart {service} on {ip}\n"
                f"stdout:\n{completed.stdout}\n"
                f"stderr:\n{completed.stderr}"
            )


def attempt_direct_login_with_recovery(
    *,
    rust_base_url: str,
    python_base_url: str,
    rust_ip: str,
    python_ip: str,
    timeout_seconds: float,
    startup_timeout_seconds: float,
    cookie_key: str,
    rust_cookies: dict[str, str],
    python_cookies: dict[str, str],
    login_path: str,
    python_runtime: str,
    restarted_for_clean_session: bool,
    clear_cookies_first: bool = False,
) -> tuple[str | None, bool]:
    if clear_cookies_first:
        clear_all_cookie_values(rust_cookies)
        clear_all_cookie_values(python_cookies)

    login_error = attempt_direct_login(
        rust_base_url=rust_base_url,
        python_base_url=python_base_url,
        rust_ip=rust_ip,
        python_ip=python_ip,
        timeout_seconds=timeout_seconds,
        cookie_key=cookie_key,
        rust_cookies=rust_cookies,
        python_cookies=python_cookies,
        login_path=login_path,
    )
    if login_error is None:
        return None, restarted_for_clean_session

    if "missing session cookie" not in login_error or restarted_for_clean_session:
        return login_error, restarted_for_clean_session

    print("INFO response parity: restarting services to recover from stale login state")
    restart_direct_services(
        rust_ip=rust_ip,
        python_ip=python_ip,
        python_runtime=python_runtime,
    )
    wait_for_direct_recovery(
        rust_base_url=rust_base_url,
        python_base_url=python_base_url,
        rust_ip=rust_ip,
        python_ip=python_ip,
        request_timeout_seconds=timeout_seconds,
        startup_timeout_seconds=startup_timeout_seconds,
    )
    clear_all_cookie_values(rust_cookies)
    clear_all_cookie_values(python_cookies)
    login_error = attempt_direct_login(
        rust_base_url=rust_base_url,
        python_base_url=python_base_url,
        rust_ip=rust_ip,
        python_ip=python_ip,
        timeout_seconds=timeout_seconds,
        cookie_key=cookie_key,
        rust_cookies=rust_cookies,
        python_cookies=python_cookies,
        login_path=login_path,
    )
    return login_error, True


def websocket_probe(
    *,
    connect_host: str,
    port: int,
    path: str,
    cookie: str | None,
    timeout_seconds: float,
) -> WebSocketProbeResult:
    websocket_key = base64.b64encode(os.urandom(16)).decode("ascii")
    expected_accept = base64.b64encode(
        hashlib.sha1(f"{websocket_key}{WEBSOCKET_ACCEPT_GUID}".encode("ascii")).digest()
    ).decode("ascii")
    request_lines = [
        f"GET {path} HTTP/1.1",
        f"Host: {DIRECT_HTTPS_HOST}" if port == 443 else f"Host: {DIRECT_HTTPS_HOST}:{port}",
        "Connection: Upgrade",
        "Upgrade: websocket",
        f"Origin: https://{DIRECT_HTTPS_HOST}{path}",
        f"Sec-WebSocket-Key: {websocket_key}",
        "Sec-WebSocket-Version: 13",
        "",
        "",
    ]
    if cookie:
        request_lines.insert(7, f"Cookie: {cookie}")
    ssl_context = create_parity_ssl_context(DIRECT_HTTPS_CA)
    try:
        with socket.create_connection((connect_host, port), timeout=timeout_seconds) as tcp_socket:
            with ssl_context.wrap_socket(tcp_socket, server_hostname=DIRECT_HTTPS_HOST) as tls_socket:
                tls_socket.settimeout(timeout_seconds)
                tls_socket.sendall("\r\n".join(request_lines).encode("ascii"))
                response = bytearray()
                while b"\r\n\r\n" not in response:
                    chunk = tls_socket.recv(4096)
                    if not chunk:
                        break
                    response.extend(chunk)
                    if len(response) > 16384:
                        break
    except (OSError, ssl.SSLError) as error:
        return WebSocketProbeResult(None, str(error))

    if not response:
        return WebSocketProbeResult(None, "empty response")

    header_block = response.split(b"\r\n\r\n", 1)[0].decode("iso-8859-1", errors="replace")
    header_lines = header_block.split("\r\n")
    status_line = header_lines[0] if header_lines else ""
    status_parts = status_line.split(" ", 2)
    if len(status_parts) < 2 or not status_parts[1].isdigit():
        return WebSocketProbeResult(None, f"invalid response: {status_line or '<empty>'}")

    headers: dict[str, str] = {}
    for line in header_lines[1:]:
        if ":" not in line:
            continue
        name, value = line.split(":", 1)
        headers[name.strip().lower()] = value.strip()

    status = int(status_parts[1])
    if status == 101 and headers.get("sec-websocket-accept") != expected_accept:
        return WebSocketProbeResult(status, "invalid Sec-WebSocket-Accept header")

    return WebSocketProbeResult(status, status_line)


def wait_for_direct_service(
    name: str,
    base_url: str,
    *,
    connect_host: str,
    request_timeout_seconds: float,
    startup_timeout_seconds: float,
) -> None:
    deadline = time.time() + startup_timeout_seconds
    last_error = ""
    while time.time() < deadline:
        response, error = request_or_error(
            f"{base_url}/version",
            "GET",
            ca_cert_path=DIRECT_HTTPS_CA,
            connect_host=connect_host,
            timeout_seconds=request_timeout_seconds,
            read_body=False,
        )
        if error is None and response is not None and response.status in {200, 401, 403, 404, 500}:
            return
        last_error = error or f"status {response.status}"
        time.sleep(0.25)
    raise ParityError(f"timed out waiting for {name} at {base_url}: {last_error}")


def wait_for_direct_recovery(
    *,
    rust_base_url: str,
    python_base_url: str,
    rust_ip: str,
    python_ip: str,
    request_timeout_seconds: float,
    startup_timeout_seconds: float,
) -> None:
    wait_for_direct_service(
        "rust",
        rust_base_url,
        connect_host=rust_ip,
        request_timeout_seconds=request_timeout_seconds,
        startup_timeout_seconds=startup_timeout_seconds,
    )
    wait_for_direct_service(
        "python",
        python_base_url,
        connect_host=python_ip,
        request_timeout_seconds=request_timeout_seconds,
        startup_timeout_seconds=startup_timeout_seconds,
    )


def wait_for_direct_offline(
    name: str,
    base_url: str,
    *,
    connect_host: str,
    request_timeout_seconds: float,
    startup_timeout_seconds: float,
) -> None:
    deadline = time.time() + startup_timeout_seconds
    last_state = "still online"
    while time.time() < deadline:
        response, error = request_or_error(
            f"{base_url}/version",
            "GET",
            ca_cert_path=DIRECT_HTTPS_CA,
            connect_host=connect_host,
            timeout_seconds=request_timeout_seconds,
            read_body=False,
        )
        if error is not None and is_expected_disconnect_error(error):
            return
        if error is not None:
            last_state = error
        elif response is not None:
            last_state = f"status {response.status}"
        time.sleep(0.25)
    raise ParityError(f"timed out waiting for {name} to go offline at {base_url}: {last_state}")


def run_direct_websocket_cases(
    *,
    rust_ip: str,
    python_ip: str,
    port: int,
    request_timeout_seconds: float,
    startup_timeout_seconds: float,
    python_runtime: str,
) -> None:
    rust_base_url = https_base_url(port)
    python_base_url = https_base_url(port)
    wait_for_direct_recovery(
        rust_base_url=rust_base_url,
        python_base_url=python_base_url,
        rust_ip=rust_ip,
        python_ip=python_ip,
        request_timeout_seconds=request_timeout_seconds,
        startup_timeout_seconds=startup_timeout_seconds,
    )

    restarted_for_clean_session = False
    failures: list[str] = []
    rust_logout_cookies: dict[str, str] = {}
    python_logout_cookies: dict[str, str] = {}
    while True:
        failures = []
        retry_after_restart = False
        rust_cookies_by_login_path: dict[str, str | None] = {}
        python_cookies_by_login_path: dict[str, str | None] = {}
        for path in websocket_paths_for_python_runtime(python_runtime):
            login_path = "/api/v2/login" if path.startswith("/api/v2/") else "/login"
            if login_path not in rust_cookies_by_login_path:
                rust_cookies_by_login_path[login_path] = login_cookie_for_websocket(
                    base_url=rust_base_url,
                    connect_host=rust_ip,
                    timeout_seconds=request_timeout_seconds,
                    login_path=login_path,
                )
            if login_path not in python_cookies_by_login_path:
                python_cookies_by_login_path[login_path] = login_cookie_for_websocket(
                    base_url=python_base_url,
                    connect_host=python_ip,
                    timeout_seconds=request_timeout_seconds,
                    login_path=login_path,
                )

            rust_cookie_value = rust_cookies_by_login_path[login_path]
            python_cookie_value = python_cookies_by_login_path[login_path]
            if rust_cookie_value:
                rust_logout_cookies[login_path] = rust_cookie_value
            if python_cookie_value:
                python_logout_cookies[login_path] = python_cookie_value

            rust_cookie = rust_cookies_by_login_path[login_path]
            python_cookie = python_cookies_by_login_path[login_path]
            if (rust_cookie is None or python_cookie is None) and not restarted_for_clean_session:
                retry_after_restart = True
                break

            rust_enable_error = enable_direct_websocket_notifications(
                base_url=rust_base_url,
                connect_host=rust_ip,
                cookie=rust_cookie,
                timeout_seconds=request_timeout_seconds,
                path=path,
            )
            python_enable_error = enable_direct_websocket_notifications(
                base_url=python_base_url,
                connect_host=python_ip,
                cookie=python_cookie,
                timeout_seconds=request_timeout_seconds,
                path=path,
            )
            if (
                rust_enable_error == "status 401"
                and python_enable_error == "status 401"
                and not restarted_for_clean_session
            ):
                retry_after_restart = True
                break
            if rust_enable_error or python_enable_error:
                failures.append(
                    f"{path}: websocket enable failed, rust={rust_enable_error or 'ok'}, python={python_enable_error or 'ok'}"
                )
                continue
            rust_result = websocket_probe(
                connect_host=rust_ip,
                port=port,
                path=path,
                cookie=rust_cookie,
                timeout_seconds=request_timeout_seconds,
            )
            python_result = websocket_probe(
                connect_host=python_ip,
                port=port,
                path=path,
                cookie=python_cookie,
                timeout_seconds=request_timeout_seconds,
            )
            if rust_result.status != 101 or python_result.status != 101:
                failures.append(
                    f"{path}: websocket upgrade failed, rust={rust_result.status or 'error'} ({rust_result.detail}), "
                    f"python={python_result.status or 'error'} ({python_result.detail})"
                )
                continue
            print(f"PASS websocket: {path}")

        if retry_after_restart and not restarted_for_clean_session:
            print("INFO websocket parity: restarting services to recover from stale login state")
            restart_direct_services(
                rust_ip=rust_ip,
                python_ip=python_ip,
                python_runtime=python_runtime,
            )
            wait_for_direct_recovery(
                rust_base_url=rust_base_url,
                python_base_url=python_base_url,
                rust_ip=rust_ip,
                python_ip=python_ip,
                request_timeout_seconds=request_timeout_seconds,
                startup_timeout_seconds=startup_timeout_seconds,
            )
            restarted_for_clean_session = True
            continue
        break

    logout_direct_sessions_best_effort(
        base_url=rust_base_url,
        connect_host=rust_ip,
        cookies=rust_logout_cookies,
        timeout_seconds=request_timeout_seconds,
    )
    logout_direct_sessions_best_effort(
        base_url=python_base_url,
        connect_host=python_ip,
        cookies=python_logout_cookies,
        timeout_seconds=request_timeout_seconds,
    )

    if failures:
        raise ParityError("Direct websocket parity failed:\n" + "\n".join(failures))

    print("Direct websocket parity passed.")


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
    indexed_cases = list(enumerate(cases))
    ordered = sorted(
        indexed_cases,
        key=lambda item: (
            is_terminal_destructive_case(item[1]),
            is_reboot_like_case(item[1]),
            not is_login_request(item[1]),
            is_logout_request(item[1]),
            item[0],
        ),
    )
    return [case for _, case in ordered]


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
    settings_content: str,
    provisioning_state_content: str,
    config_name: str,
    port: int,
    sessions_on: bool,
    enabled_plugins: list[str],
) -> RemoteRuntimeFiles:
    remote_dir = remote_temp_dir(target, ssh_options)
    plugin_dir = f"{remote_dir}/plugins"
    cert_path = TARGET_SERVER_CERT_PATH
    key_path = TARGET_SERVER_KEY_PATH
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
    settings_content = render_settings_file("summit")
    provisioning_state_content = "2\n"

    remote_files = create_remote_runtime_files(
        target,
        ssh_options,
        settings_content=settings_content,
        provisioning_state_content=provisioning_state_content,
        config_name=config_name,
        port=remote_port,
        sessions_on=sessions_on,
        enabled_plugins=enabled_plugins,
    )

    runtime: RemoteManagedProcess | None = None
    try:
        runtime_env = {
            "SUMMIT_RCM_BIND": f"0.0.0.0:{remote_port}",
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
            https_base_url(remote_port),
            runtime,
            ca_cert_path=DIRECT_HTTPS_CA,
            connect_host=ssh_target_host(target),
            request_timeout_seconds=request_timeout_seconds,
            startup_timeout_seconds=startup_timeout_seconds,
        )
        response, error = request_or_error(
            f"{base_url}{case['path']}",
            case["method"],
            body=case.get("body"),
            ca_cert_path=DIRECT_HTTPS_CA,
            connect_host=ssh_target_host(target),
            timeout_seconds=float(case.get("timeout_seconds", request_timeout_seconds)),
            read_body=should_read_body(case),
        )
        return evaluate_terminal_case_outcome(case, response, error)
    finally:
        if runtime is not None:
            runtime.stop()
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
        config_name="summit-rcm.ini",
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
        session_skip_reason = session_mode_skip_reason(case, sessions_on=sessions_on)
        if session_skip_reason is not None:
            print(f"SKIP {case['id']}: {case['method']} {case['path']} ({session_skip_reason})")
            continue
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
        settings_content = render_settings_file("summit")
        provisioning_state_content = "2\n"

        rust_remote_dir = remote_temp_dir(rust_target, ssh_options)
        python_remote_dir = remote_temp_dir(python_target, ssh_options)

        rust_remote_plugin_dir = f"{rust_remote_dir}/plugins"
        python_remote_plugin_dir = f"{python_remote_dir}/plugins"
        rust_remote_settings = f"{rust_remote_dir}/summit-rcm-settings.ini"
        rust_remote_state = f"{rust_remote_dir}/provisioning-state"
        rust_remote_config = f"{rust_remote_dir}/summit-rcm.ini"
        rust_remote_pid = f"{rust_remote_dir}/runtime.pid"

        python_remote_settings = f"{python_remote_dir}/summit-rcm-settings.ini"
        python_remote_state = f"{python_remote_dir}/provisioning-state"
        python_remote_config = f"{python_remote_dir}/summit-rcm.ini"
        python_remote_pid = f"{python_remote_dir}/runtime.pid"

        rust_config = render_server_config(
            port=remote_rust_port,
            sessions_on=sessions_on,
            enabled_plugins=enabled_plugins,
            plugin_dir=rust_remote_plugin_dir,
            cert_path=TARGET_SERVER_CERT_PATH,
            key_path=TARGET_SERVER_KEY_PATH,
        )
        python_config = render_server_config(
            port=remote_python_port,
            sessions_on=sessions_on,
            enabled_plugins=enabled_plugins,
            plugin_dir=python_remote_plugin_dir,
            cert_path=TARGET_SERVER_CERT_PATH,
            key_path=TARGET_SERVER_KEY_PATH,
        )

        rust: RemoteManagedProcess | None = None
        python: RemoteManagedProcess | None = None

        try:
            ensure_remote_directory(rust_target, ssh_options, rust_remote_plugin_dir)
            ensure_remote_directory(python_target, ssh_options, python_remote_plugin_dir)

            upload_text(rust_target, ssh_options, rust_remote_settings, settings_content)
            upload_text(rust_target, ssh_options, rust_remote_state, provisioning_state_content)
            upload_text(rust_target, ssh_options, rust_remote_config, rust_config)

            upload_text(python_target, ssh_options, python_remote_settings, settings_content)
            upload_text(python_target, ssh_options, python_remote_state, provisioning_state_content)
            upload_text(python_target, ssh_options, python_remote_config, python_config)

            rust_runtime_env = {
                "SUMMIT_RCM_BIND": f"0.0.0.0:{remote_rust_port}",
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
                https_base_url(remote_rust_port),
                rust,
                ca_cert_path=DIRECT_HTTPS_CA,
                connect_host=ssh_target_host(rust_target),
                request_timeout_seconds=request_timeout_seconds,
                startup_timeout_seconds=startup_timeout_seconds,
            )
            python_base_url = wait_for_server(
                "python",
                https_base_url(remote_python_port),
                python,
                ca_cert_path=DIRECT_HTTPS_CA,
                connect_host=ssh_target_host(python_target),
                request_timeout_seconds=request_timeout_seconds,
                startup_timeout_seconds=startup_timeout_seconds,
            )

            failures: list[str] = []
            rust_cookies: dict[str, str] = {}
            python_cookies: dict[str, str] = {}
            rust_wireless_interface = detect_wireless_interface(
                rust_base_url,
                ca_cert_path=DIRECT_HTTPS_CA,
                connect_host=ssh_target_host(rust_target),
                timeout_seconds=request_timeout_seconds,
            )
            python_wireless_interface = detect_wireless_interface(
                python_base_url,
                ca_cert_path=DIRECT_HTTPS_CA,
                connect_host=ssh_target_host(python_target),
                timeout_seconds=request_timeout_seconds,
            )

            for case in shared_cases:
                rust_case_path = resolve_wireless_interface_path(case["path"], rust_wireless_interface)
                python_case_path = resolve_wireless_interface_path(case["path"], python_wireless_interface)
                cookie_key = case.get("use_cookie_from")
                read_body = should_read_body(case)
                case_timeout_seconds = float(case.get("timeout_seconds", request_timeout_seconds))
                readback_case = None
                rust_readback_before = None
                python_readback_before = None

                datetime_prime_error = maybe_prime_datetime_for_case(
                    case,
                    rust_base_url=rust_base_url,
                    python_base_url=python_base_url,
                    rust_case_path=rust_case_path,
                    python_case_path=python_case_path,
                    rust_cookie=rust_cookies.get(cookie_key) if cookie_key else None,
                    python_cookie=python_cookies.get(cookie_key) if cookie_key else None,
                    ca_cert_path=DIRECT_HTTPS_CA,
                    timeout_seconds=case_timeout_seconds,
                    rust_connect_host=ssh_target_host(rust_target),
                    python_connect_host=ssh_target_host(python_target),
                )
                if datetime_prime_error is not None:
                    failures.append(datetime_prime_error)
                    continue

                if should_verify_readback(case):
                    readback_case = find_readback_case(case, shared_cases)
                    if readback_case is not None:
                        rust_readback_path = resolve_wireless_interface_path(
                            readback_case["path"],
                            rust_wireless_interface,
                        )
                        python_readback_path = resolve_wireless_interface_path(
                            readback_case["path"],
                            python_wireless_interface,
                        )
                        readback_cookie_key = readback_case.get("use_cookie_from") or cookie_key
                        readback_timeout_seconds = float(
                            readback_case.get("timeout_seconds", request_timeout_seconds)
                        )
                        readback_read_body = should_read_body(readback_case)
                        rust_readback_before, rust_readback_before_error = request_or_error(
                            f"{rust_base_url}{rust_readback_path}",
                            readback_case["method"],
                            body=readback_case.get("body"),
                            cookie=rust_cookies.get(readback_cookie_key)
                            if readback_cookie_key
                            else None,
                            ca_cert_path=DIRECT_HTTPS_CA,
                            connect_host=ssh_target_host(rust_target),
                            timeout_seconds=readback_timeout_seconds,
                            read_body=readback_read_body,
                        )
                        python_readback_before, python_readback_before_error = request_or_error(
                            f"{python_base_url}{python_readback_path}",
                            readback_case["method"],
                            body=readback_case.get("body"),
                            cookie=python_cookies.get(readback_cookie_key)
                            if readback_cookie_key
                            else None,
                            ca_cert_path=DIRECT_HTTPS_CA,
                            connect_host=ssh_target_host(python_target),
                            timeout_seconds=readback_timeout_seconds,
                            read_body=readback_read_body,
                        )
                        if rust_readback_before_error or python_readback_before_error:
                            failures.append(
                                f"{case['id']}: pre-readback failed, rust={rust_readback_before_error or 'ok'}, python={python_readback_before_error or 'ok'}"
                            )
                            continue

                    rust_response, rust_error = request_or_error(
                        f"{rust_base_url}{rust_case_path}",
                        case["method"],
                        body=case.get("body"),
                        cookie=rust_cookies.get(cookie_key) if cookie_key else None,
                        ca_cert_path=DIRECT_HTTPS_CA,
                        connect_host=ssh_target_host(rust_target),
                        timeout_seconds=case_timeout_seconds,
                        read_body=read_body,
                    )
                    python_response, python_error = request_or_error(
                        f"{python_base_url}{python_case_path}",
                        case["method"],
                        body=case.get("body"),
                        cookie=python_cookies.get(cookie_key) if cookie_key else None,
                        ca_cert_path=DIRECT_HTTPS_CA,
                        connect_host=ssh_target_host(python_target),
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
                        ignore_json_keys=effective_ignore_json_keys(case),
                    )
                    if mismatch is not None:
                        failures.append(mismatch)
                        continue

                    if readback_case is not None:
                        rust_readback_path = resolve_wireless_interface_path(
                            readback_case["path"],
                            rust_wireless_interface,
                        )
                        python_readback_path = resolve_wireless_interface_path(
                            readback_case["path"],
                            python_wireless_interface,
                        )
                        readback_cookie_key = readback_case.get("use_cookie_from") or cookie_key
                        readback_timeout_seconds = float(
                            readback_case.get("timeout_seconds", request_timeout_seconds)
                        )
                        readback_read_body = should_read_body(readback_case)
                        rust_readback_after, rust_readback_after_error = request_or_error(
                            f"{rust_base_url}{rust_readback_path}",
                            readback_case["method"],
                            body=readback_case.get("body"),
                            cookie=rust_cookies.get(readback_cookie_key)
                            if readback_cookie_key
                            else None,
                            ca_cert_path=DIRECT_HTTPS_CA,
                            connect_host=ssh_target_host(rust_target),
                            timeout_seconds=readback_timeout_seconds,
                            read_body=readback_read_body,
                        )
                        python_readback_after, python_readback_after_error = request_or_error(
                            f"{python_base_url}{python_readback_path}",
                            readback_case["method"],
                            body=readback_case.get("body"),
                            cookie=python_cookies.get(readback_cookie_key)
                            if readback_cookie_key
                            else None,
                            ca_cert_path=DIRECT_HTTPS_CA,
                            connect_host=ssh_target_host(python_target),
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
                            ignore_json_keys=effective_ignore_json_keys(readback_case),
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
                                ignore_json_keys=effective_ignore_json_keys(readback_case),
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
                                ignore_json_keys=effective_ignore_json_keys(readback_case),
                            )
                            if mismatch is not None:
                                failures.append(mismatch)
                                continue

                    print(f"PASS {case['id']}: {case['method']} {rust_case_path}")

            if failures:
                raise ParityError("Remote response parity failed:\n" + "\n".join(failures))

            print("Remote shared response parity passed.")
        finally:
            if rust is not None:
                rust.stop()
            if python is not None:
                python.stop()
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


def run_direct_response_cases(
    cases: list[dict[str, Any]],
    *,
    rust_ip: str,
    python_ip: str,
    python_runtime: str,
    port: int,
    request_timeout_seconds: float,
    startup_timeout_seconds: float,
    respect_skip_live: bool,
    allow_destructive: bool,
) -> None:
    cases = order_cases_for_remote_run(cases)
    failures: list[str] = []
    rust_cookies: dict[str, str] = {}
    python_cookies: dict[str, str] = {}
    rust_base_url = https_base_url(port)
    python_base_url = https_base_url(port)
    rust_wireless_interface: str | None = None
    python_wireless_interface: str | None = None
    restarted_for_clean_session = False

    for case in cases:
        cookie_key = case.get("use_cookie_from") or default_cookie_key_for_case(case)
        session_skip_reason = session_mode_skip_reason(case, sessions_on=sessions_on)
        if session_skip_reason is not None:
            rust_case_path = resolve_wireless_interface_path(case["path"], rust_wireless_interface)
            print(f"SKIP {case['id']}: {case['method']} {rust_case_path} ({session_skip_reason})")
            continue
        if path_uses_wireless_interface_placeholder(case["path"]) and (
            rust_wireless_interface is None or python_wireless_interface is None
        ):
            if request_cookie_value(rust_cookies, case) is None or request_cookie_value(python_cookies, case) is None:
                login_error, restarted_for_clean_session = attempt_direct_login_with_recovery(
                    rust_base_url=rust_base_url,
                    python_base_url=python_base_url,
                    rust_ip=rust_ip,
                    python_ip=python_ip,
                    timeout_seconds=request_timeout_seconds,
                    startup_timeout_seconds=startup_timeout_seconds,
                    cookie_key=cookie_key,
                    rust_cookies=rust_cookies,
                    python_cookies=python_cookies,
                    login_path=direct_login_path_for_case(case, python_runtime),
                    python_runtime=python_runtime,
                    restarted_for_clean_session=restarted_for_clean_session,
                )
                if login_error is not None:
                    failures.append(f"{case['id']}: {login_error}")
                    continue
            if rust_wireless_interface is None:
                rust_wireless_interface = detect_wireless_interface(
                    rust_base_url,
                    ca_cert_path=DIRECT_HTTPS_CA,
                    connect_host=rust_ip,
                    timeout_seconds=request_timeout_seconds,
                    cookie=request_cookie_value(rust_cookies, case),
                )
            if python_wireless_interface is None:
                python_wireless_interface = detect_wireless_interface(
                    python_base_url,
                    ca_cert_path=DIRECT_HTTPS_CA,
                    connect_host=python_ip,
                    timeout_seconds=request_timeout_seconds,
                    cookie=request_cookie_value(python_cookies, case),
                )

        rust_case_path = resolve_wireless_interface_path(case["path"], rust_wireless_interface)
        python_case_path = resolve_wireless_interface_path(case["path"], python_wireless_interface)
        skip_reason = skip_reason_for_remote_case(
            case,
            respect_skip_live=respect_skip_live,
            allow_destructive=allow_destructive,
        )
        if skip_reason is not None:
            print(f"SKIP {case['id']}: {case['method']} {rust_case_path} ({skip_reason})")
            continue

        read_body = should_read_body(case)
        case_timeout_seconds = float(case.get("timeout_seconds", request_timeout_seconds))
        readback_case = None
        rust_readback_before = None
        python_readback_before = None

        datetime_prime_error = maybe_prime_datetime_for_case(
            case,
            rust_base_url=rust_base_url,
            python_base_url=python_base_url,
            rust_case_path=rust_case_path,
            python_case_path=python_case_path,
            rust_cookie=request_cookie_value(rust_cookies, case),
            python_cookie=request_cookie_value(python_cookies, case),
            ca_cert_path=DIRECT_HTTPS_CA,
            timeout_seconds=case_timeout_seconds,
            rust_connect_host=rust_ip,
            python_connect_host=python_ip,
        )
        if datetime_prime_error is not None:
            failures.append(datetime_prime_error)
            continue

        if should_verify_readback(case):
            readback_case = find_readback_case(case, cases)
            if readback_case is not None:
                rust_readback_path = resolve_wireless_interface_path(
                    readback_case["path"],
                    rust_wireless_interface,
                )
                python_readback_path = resolve_wireless_interface_path(
                    readback_case["path"],
                    python_wireless_interface,
                )
                readback_cookie_key = readback_case.get("use_cookie_from") or cookie_key
                readback_timeout_seconds = float(
                    readback_case.get("timeout_seconds", request_timeout_seconds)
                )
                readback_read_body = should_read_body(readback_case)
                rust_readback_before, rust_readback_before_error = request_or_error(
                    f"{rust_base_url}{rust_readback_path}",
                    readback_case["method"],
                    body=readback_case.get("body"),
                    multipart=readback_case.get("multipart"),
                    cookie=rust_cookies.get(readback_cookie_key) if readback_cookie_key else None,
                    ca_cert_path=DIRECT_HTTPS_CA,
                    connect_host=rust_ip,
                    timeout_seconds=readback_timeout_seconds,
                    read_body=readback_read_body,
                )
                python_readback_before, python_readback_before_error = request_or_error(
                    f"{python_base_url}{python_readback_path}",
                    readback_case["method"],
                    body=readback_case.get("body"),
                    multipart=readback_case.get("multipart"),
                    cookie=python_cookies.get(readback_cookie_key) if readback_cookie_key else None,
                    ca_cert_path=DIRECT_HTTPS_CA,
                    connect_host=python_ip,
                    timeout_seconds=readback_timeout_seconds,
                    read_body=readback_read_body,
                )
                if rust_readback_before_error or python_readback_before_error:
                    failures.append(
                        f"{case['id']}: pre-readback failed, rust={rust_readback_before_error or 'ok'}, python={python_readback_before_error or 'ok'}"
                    )
                    continue

        rust_response, rust_error = request_or_error(
            f"{rust_base_url}{rust_case_path}",
            case["method"],
            body=case.get("body"),
            multipart=case.get("multipart"),
            cookie=request_cookie_value(rust_cookies, case),
            ca_cert_path=DIRECT_HTTPS_CA,
            connect_host=rust_ip,
            timeout_seconds=case_timeout_seconds,
            read_body=read_body,
        )
        python_response, python_error = request_or_error(
            f"{python_base_url}{python_case_path}",
            case["method"],
            body=case.get("body"),
            multipart=case.get("multipart"),
            cookie=request_cookie_value(python_cookies, case),
            ca_cert_path=DIRECT_HTTPS_CA,
            connect_host=python_ip,
            timeout_seconds=case_timeout_seconds,
            read_body=read_body,
        )

        if (rust_error or python_error) and is_destructive_skip(case):
            login_error, restarted_for_clean_session = attempt_direct_login_with_recovery(
                rust_base_url=rust_base_url,
                python_base_url=python_base_url,
                rust_ip=rust_ip,
                python_ip=python_ip,
                timeout_seconds=case_timeout_seconds,
                startup_timeout_seconds=startup_timeout_seconds,
                cookie_key=cookie_key,
                rust_cookies=rust_cookies,
                python_cookies=python_cookies,
                login_path=direct_login_path_for_case(case, python_runtime),
                python_runtime=python_runtime,
                restarted_for_clean_session=restarted_for_clean_session,
                clear_cookies_first=True,
            )
            if login_error is None:
                rust_response, rust_error = request_or_error(
                    f"{rust_base_url}{rust_case_path}",
                    case["method"],
                    body=case.get("body"),
                    multipart=case.get("multipart"),
                    cookie=request_cookie_value(rust_cookies, case),
                    ca_cert_path=DIRECT_HTTPS_CA,
                    connect_host=rust_ip,
                    timeout_seconds=case_timeout_seconds,
                    read_body=read_body,
                )
                python_response, python_error = request_or_error(
                    f"{python_base_url}{python_case_path}",
                    case["method"],
                    body=case.get("body"),
                    multipart=case.get("multipart"),
                    cookie=request_cookie_value(python_cookies, case),
                    ca_cert_path=DIRECT_HTTPS_CA,
                    connect_host=python_ip,
                    timeout_seconds=case_timeout_seconds,
                    read_body=read_body,
                )

        unauthorized_pair = {
            rust_response.status if rust_response else None,
            python_response.status if python_response else None,
        }
        saw_unauthorized_status = any(status in {401, 403} for status in unauthorized_pair)
        if (
            not rust_error
            and not python_error
            and not is_login_request(case)
            and saw_unauthorized_status
        ):
            login_error, restarted_for_clean_session = attempt_direct_login_with_recovery(
                rust_base_url=rust_base_url,
                python_base_url=python_base_url,
                rust_ip=rust_ip,
                python_ip=python_ip,
                timeout_seconds=case_timeout_seconds,
                startup_timeout_seconds=startup_timeout_seconds,
                cookie_key=cookie_key,
                rust_cookies=rust_cookies,
                python_cookies=python_cookies,
                login_path=direct_login_path_for_case(case, python_runtime),
                python_runtime=python_runtime,
                restarted_for_clean_session=restarted_for_clean_session,
            )
            if login_error is None:
                rust_response, rust_error = request_or_error(
                    f"{rust_base_url}{rust_case_path}",
                    case["method"],
                    body=case.get("body"),
                    multipart=case.get("multipart"),
                    cookie=request_cookie_value(rust_cookies, case),
                    ca_cert_path=DIRECT_HTTPS_CA,
                    connect_host=rust_ip,
                    timeout_seconds=case_timeout_seconds,
                    read_body=read_body,
                )
                python_response, python_error = request_or_error(
                    f"{python_base_url}{python_case_path}",
                    case["method"],
                    body=case.get("body"),
                    multipart=case.get("multipart"),
                    cookie=request_cookie_value(python_cookies, case),
                    ca_cert_path=DIRECT_HTTPS_CA,
                    connect_host=python_ip,
                    timeout_seconds=case_timeout_seconds,
                    read_body=read_body,
                )
            else:
                failures.append(f"{case['id']}: {login_error}")
                continue

        if is_reboot_like_case(case):
            rust_outcome = evaluate_terminal_case_outcome(case, rust_response, rust_error)
            python_outcome = evaluate_terminal_case_outcome(case, python_response, python_error)
            if not rust_outcome.accepted or not python_outcome.accepted:
                failures.append(
                    f"{case['id']}: destructive parity failed, rust={rust_outcome.detail}, python={python_outcome.detail}"
                )
                continue
            try:
                wait_for_direct_recovery(
                    rust_base_url=rust_base_url,
                    python_base_url=python_base_url,
                    rust_ip=rust_ip,
                    python_ip=python_ip,
                    request_timeout_seconds=request_timeout_seconds,
                    startup_timeout_seconds=startup_timeout_seconds,
                )
            except ParityError as error:
                failures.append(f"{case['id']}: {error}")
                continue

            print(
                f"PASS {case['id']}: {case['method']} {rust_case_path} "
                f"(rust={rust_outcome.detail}; python={python_outcome.detail})"
            )
            continue

        if is_power_loss_case(case):
            rust_outcome = evaluate_terminal_case_outcome(case, rust_response, rust_error)
            python_outcome = evaluate_terminal_case_outcome(case, python_response, python_error)
            if not rust_outcome.accepted or not python_outcome.accepted:
                failures.append(
                    f"{case['id']}: destructive parity failed, rust={rust_outcome.detail}, python={python_outcome.detail}"
                )
                continue

            try:
                offline_timeout_seconds = max(
                    startup_timeout_seconds,
                    DEFAULT_POWER_LOSS_OFFLINE_TIMEOUT_SECONDS,
                )
                wait_for_direct_offline(
                    "rust",
                    rust_base_url,
                    connect_host=rust_ip,
                    request_timeout_seconds=request_timeout_seconds,
                    startup_timeout_seconds=offline_timeout_seconds,
                )
                wait_for_direct_offline(
                    "python",
                    python_base_url,
                    connect_host=python_ip,
                    request_timeout_seconds=request_timeout_seconds,
                    startup_timeout_seconds=offline_timeout_seconds,
                )
            except ParityError as error:
                failures.append(f"{case['id']}: {error}")
                continue

            print(
                f"PASS {case['id']}: {case['method']} {rust_case_path} "
                f"(rust={rust_outcome.detail}; python={python_outcome.detail}; offline=verified)"
            )
            logout_direct_sessions_best_effort(
                base_url=rust_base_url,
                connect_host=rust_ip,
                cookies=rust_cookies,
                timeout_seconds=request_timeout_seconds,
            )
            logout_direct_sessions_best_effort(
                base_url=python_base_url,
                connect_host=python_ip,
                cookies=python_cookies,
                timeout_seconds=request_timeout_seconds,
            )
            raise ParityError(
                f"{case['id']}: poweroff/suspend targets are offline as expected; "
                "perform manual recovery before continuing parity"
            )

        if rust_error or python_error:
            failures.append(
                f"{case['id']}: request failed, rust={rust_error or 'ok'}, python={python_error or 'ok'}"
            )
            continue

        should_store_cookie = bool(case.get("store_cookie_as")) or is_login_request(case)
        if should_store_cookie:
            if not store_cookie_value(rust_cookies, case, rust_response) or not store_cookie_value(
                python_cookies, case, python_response
            ):
                rust_cookie = response_cookie(rust_response)
                python_cookie = response_cookie(python_response)
                if is_login_request(case):
                    login_error, restarted_for_clean_session = attempt_direct_login_with_recovery(
                        rust_base_url=rust_base_url,
                        python_base_url=python_base_url,
                        rust_ip=rust_ip,
                        python_ip=python_ip,
                        timeout_seconds=case_timeout_seconds,
                        startup_timeout_seconds=startup_timeout_seconds,
                        cookie_key=cookie_key,
                        rust_cookies=rust_cookies,
                        python_cookies=python_cookies,
                        login_path=direct_login_path_for_case(case, python_runtime),
                        python_runtime=python_runtime,
                        restarted_for_clean_session=restarted_for_clean_session,
                        clear_cookies_first=True,
                    )
                    if login_error is None:
                        rust_response, rust_error = request_or_error(
                            f"{rust_base_url}{rust_case_path}",
                            case["method"],
                            body=case.get("body"),
                            multipart=case.get("multipart"),
                            cookie=None,
                            ca_cert_path=DIRECT_HTTPS_CA,
                            connect_host=rust_ip,
                            timeout_seconds=case_timeout_seconds,
                            read_body=read_body,
                        )
                        python_response, python_error = request_or_error(
                            f"{python_base_url}{python_case_path}",
                            case["method"],
                            body=case.get("body"),
                            multipart=case.get("multipart"),
                            cookie=None,
                            ca_cert_path=DIRECT_HTTPS_CA,
                            connect_host=python_ip,
                            timeout_seconds=case_timeout_seconds,
                            read_body=read_body,
                        )
                        if rust_error or python_error:
                            failures.append(
                                f"{case['id']}: request failed, rust={rust_error or 'ok'}, python={python_error or 'ok'}"
                            )
                            continue
                        if store_cookie_value(rust_cookies, case, rust_response) and store_cookie_value(
                            python_cookies, case, python_response
                        ):
                            pass
                        else:
                            rust_cookie = response_cookie(rust_response)
                            python_cookie = response_cookie(python_response)
                            failures.append(
                                f"{case['id']}: missing session cookie, rust={bool(rust_cookie)}, python={bool(python_cookie)}"
                            )
                            continue
                    else:
                        failures.append(f"{case['id']}: {login_error}")
                        continue
                else:
                    failures.append(
                        f"{case['id']}: missing session cookie, rust={bool(rust_cookie)}, python={bool(python_cookie)}"
                    )
                    continue

        mismatch = response_pair_mismatch(
            case["id"],
            "rust",
            rust_response,
            "python",
            python_response,
            case["compare"],
            expected_status=case.get("expected_status"),
            ignore_status=bool(case.get("ignore_status")),
            ignore_content_type=bool(case.get("ignore_content_type")),
            ignore_body=bool(case.get("ignore_body")),
            allow_framework_validation_mismatch=bool(case.get("allow_framework_validation_mismatch")),
            ignore_json_keys=effective_ignore_json_keys(case),
        )
        if mismatch is not None:
            failures.append(mismatch)
            continue

        if is_logout_request(case):
            clear_cookie_value(rust_cookies, case)
            clear_cookie_value(python_cookies, case)

        if readback_case is not None:
            rust_readback_path = resolve_wireless_interface_path(
                readback_case["path"],
                rust_wireless_interface,
            )
            python_readback_path = resolve_wireless_interface_path(
                readback_case["path"],
                python_wireless_interface,
            )
            readback_cookie_key = readback_case.get("use_cookie_from") or cookie_key
            readback_timeout_seconds = float(readback_case.get("timeout_seconds", request_timeout_seconds))
            readback_read_body = should_read_body(readback_case)
            rust_readback_after, rust_readback_after_error = request_or_error(
                f"{rust_base_url}{rust_readback_path}",
                readback_case["method"],
                body=readback_case.get("body"),
                multipart=readback_case.get("multipart"),
                cookie=rust_cookies.get(readback_cookie_key) if readback_cookie_key else None,
                ca_cert_path=DIRECT_HTTPS_CA,
                connect_host=rust_ip,
                timeout_seconds=readback_timeout_seconds,
                read_body=readback_read_body,
            )
            python_readback_after, python_readback_after_error = request_or_error(
                f"{python_base_url}{python_readback_path}",
                readback_case["method"],
                body=readback_case.get("body"),
                multipart=readback_case.get("multipart"),
                cookie=python_cookies.get(readback_cookie_key) if readback_cookie_key else None,
                ca_cert_path=DIRECT_HTTPS_CA,
                connect_host=python_ip,
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
                ignore_status=bool(readback_case.get("ignore_status") or case.get("ignore_status")),
                ignore_content_type=bool(
                    readback_case.get("ignore_content_type") or case.get("ignore_content_type")
                ),
                ignore_body=bool(readback_case.get("ignore_body") or case.get("ignore_body")),
                ignore_json_keys=effective_ignore_json_keys(readback_case),
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
                    ignore_json_keys=effective_ignore_json_keys(readback_case),
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
                    ignore_json_keys=effective_ignore_json_keys(readback_case),
                )
                if mismatch is not None:
                    failures.append(mismatch)
                    continue

        print(f"PASS {case['id']}: {case['method']} {rust_case_path}")

    logout_direct_sessions_best_effort(
        base_url=rust_base_url,
        connect_host=rust_ip,
        cookies=rust_cookies,
        timeout_seconds=request_timeout_seconds,
    )
    logout_direct_sessions_best_effort(
        base_url=python_base_url,
        connect_host=python_ip,
        cookies=python_cookies,
        timeout_seconds=request_timeout_seconds,
    )

    if failures:
        raise ParityError("Direct response parity failed:\n" + "\n".join(failures))

    print("Direct response parity passed.")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Run API response parity either directly against deployed Rust/Python targets over HTTPS "
            "or against temporary managed runtimes over SSH. Direct deployed-service parity is the default workflow."
        )
    )
    parser.add_argument(
        "mode",
        choices=["responses", "websockets"],
        help=(
            "Parity mode to run. 'responses' supports both direct deployed-service and managed-runtime transports. "
            "'websockets' performs direct deployed-service websocket smoke parity."
        ),
    )
    parser.add_argument(
        "--cases",
        type=Path,
        default=DEFAULT_CASES,
        help="Path to the representative response case manifest. With --python-runtime weblcm, /api/v2 cases are filtered out.",
    )
    parser.add_argument(
        "--case-id",
        action="append",
        default=[],
        help="Run only the named case id. Repeat to select multiple cases.",
    )
    parser.add_argument(
        "--python-runtime",
        choices=[runtime for runtime in PYTHON_RUNTIME_CHOICES if runtime != "auto"],
        default="summit-rcm",
        help="Python runtime under test. Use weblcm for legacy-only target parity; it skips /api/v2 response cases and probes only the legacy websocket route.",
    )
    parser.add_argument(
        "--rust-ssh",
        help="SSH target for a temporary managed Rust runtime, for example root@device. Do not use for normal deployed-service parity.",
    )
    parser.add_argument(
        "--python-ssh",
        help="SSH target for a temporary managed Python runtime. Defaults to --rust-ssh.",
    )
    parser.add_argument(
        "--rust-ip",
        help="Direct target IP for deployed Rust service parity over HTTPS. Preferred for normal live parity.",
    )
    parser.add_argument(
        "--python-ip",
        help="Direct target IP for deployed Python service parity over HTTPS. Defaults to --rust-ip.",
    )
    parser.add_argument(
        "--direct-port",
        type=int,
        default=443,
        help="Direct deployed-service HTTPS port to use with --rust-ip/--python-ip. Defaults to 443.",
    )
    parser.add_argument(
        "--ssh-option",
        action="append",
        default=["ConnectTimeout=5", "ConnectionAttempts=1"],
        help="Additional ssh -o option to apply to every managed-runtime SSH connection.",
    )
    parser.add_argument(
        "--rust-command",
        default="/usr/bin/summit-rcm",
        help="Shell command used to start the temporary managed Rust runtime.",
    )
    parser.add_argument(
        "--python-command",
        default=f"python3 -c {shell_quote(python_wrapper_command('summit-rcm'))}",
        help="Shell command used to start the temporary managed Python runtime.",
    )
    parser.add_argument(
        "--rust-workdir",
        help="Remote working directory for the managed Rust command.",
    )
    parser.add_argument(
        "--python-workdir",
        help="Remote working directory for the managed Python command.",
    )
    parser.add_argument(
        "--rust-env",
        action="append",
        default=[],
        help="Extra KEY=VALUE environment assignments for the managed Rust runtime.",
    )
    parser.add_argument(
        "--python-env",
        action="append",
        default=[],
        help="Extra KEY=VALUE environment assignments for the managed Python runtime.",
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
        help="Server startup timeout for parity checks. Mainly relevant to managed-runtime startup.",
    )
    parser.add_argument(
        "--sessions-on",
        action="store_true",
        help="Enable session middleware in the temporary managed-runtime config.",
    )
    parser.add_argument(
        "--plugins",
        action="append",
        default=[],
        help="Comma-separated plugin names to mark enabled in the temporary managed-runtime config.",
    )
    parser.add_argument(
        "--rust-port",
        type=int,
        default=DEFAULT_REMOTE_RUST_PORT,
        help="Managed-runtime HTTPS port for the temporary Rust instance.",
    )
    parser.add_argument(
        "--python-port",
        type=int,
        default=DEFAULT_REMOTE_PYTHON_PORT,
        help="Managed-runtime HTTPS port for the temporary Python instance.",
    )
    parser.add_argument(
        "--respect-skip-live",
        action="store_true",
        help="Honor every skip_live case exactly as the local parity harness does.",
    )
    parser.add_argument(
        "--allow-destructive",
        action="store_true",
        help=(
            "Run skip_live cases whose reason marks them unsafe or destructive. "
            "For direct deployed-service parity, poweroff/suspend execution stops the run and requires manual target recovery before continuing."
        ),
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    requested_plugins = parse_plugin_names(args.plugins)
    cases: list[dict[str, Any]] = []
    enabled_plugins = requested_plugins
    if args.mode == "responses":
        cases = filter_cases(json.loads(args.cases.read_text()), args.case_id)
        cases = filter_cases_for_python_runtime(cases, args.python_runtime)
        enabled_plugins = selected_plugins(cases, requested_plugins)

    if bool(args.rust_ip) == bool(args.rust_ssh):
        raise SystemExit("choose exactly one transport: direct (--rust-ip) or managed remote (--rust-ssh)")

    if args.mode == "websockets" and not args.rust_ip:
        raise SystemExit("websocket parity currently supports direct deployed-service transport only; use --rust-ip/--python-ip")

    python_target = args.python_ssh or args.rust_ssh

    selected_destructive = [case["id"] for case in cases if is_terminal_destructive_case(case)]
    if (
        args.mode == "responses"
        and not args.rust_ip
        and selected_destructive
        and not args.allow_destructive
    ):
        raise SystemExit(
            "selected destructive case(s) require --allow-destructive: "
            + ", ".join(selected_destructive)
        )

    try:
        if args.mode == "responses":
            if args.rust_ip:
                run_direct_response_cases(
                    cases,
                    rust_ip=args.rust_ip,
                    python_ip=args.python_ip or args.rust_ip,
                    python_runtime=args.python_runtime,
                    port=args.direct_port,
                    request_timeout_seconds=args.request_timeout_seconds,
                    startup_timeout_seconds=args.startup_timeout_seconds,
                    respect_skip_live=args.respect_skip_live,
                    allow_destructive=args.allow_destructive,
                )
            else:
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
        elif args.mode == "websockets":
            run_direct_websocket_cases(
                rust_ip=args.rust_ip,
                python_ip=args.python_ip or args.rust_ip,
                port=args.direct_port,
                request_timeout_seconds=args.request_timeout_seconds,
                startup_timeout_seconds=args.startup_timeout_seconds,
                python_runtime=args.python_runtime,
            )
    except ParityError as error:
        print(str(error), file=os.sys.stderr)
        return 1

    return 0


if __name__ == "__main__":
    raise SystemExit(main())

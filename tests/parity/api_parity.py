#!/usr/bin/env python3

from __future__ import annotations

import argparse
import hashlib
import http.client
import json
import os
import signal
import ssl
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import provisioning_tls_check


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_CASES = Path(__file__).with_name("api_response_cases.json")
DEFAULT_PYTHON_REPO = ROOT.parent / "summit-rcm"
PYTHON_RUNTIME_CHOICES = ("auto", "summit-rcm", "weblcm")
DEFAULT_REQUEST_TIMEOUT_SECONDS = 3.0
DEFAULT_STARTUP_TIMEOUT_SECONDS = 60.0
CORE_RUST_PARITY_FEATURES = ("api-v2", "api-legacy")
FRAMEWORK_VALIDATION_STATUSES = frozenset({400, 415, 422})
WIRELESS_INTERFACE_PLACEHOLDER = "wlo1"

USER_PERMISSIONS = [
    "status_networking",
    "networking_connections",
    "networking_edit",
    "networking_activate",
    "networking_ap_activate",
    "networking_certificates",
    "system_user",
    "system_settings",
    "system_firmware",
    "system_logs",
]


class ParityError(RuntimeError):
    pass


@dataclass
class HttpResponse:
    status: int
    headers: dict[str, str]
    body: bytes


class ResolvedHTTPSConnection(http.client.HTTPSConnection):
    def __init__(
        self,
        host: str,
        *,
        connect_host: str,
        server_hostname: str,
        port: int | None = None,
        timeout: float = socket._GLOBAL_DEFAULT_TIMEOUT,
        context: ssl.SSLContext | None = None,
    ) -> None:
        super().__init__(host, port=port, timeout=timeout, context=context)
        self._connect_host = connect_host
        self._server_hostname_override = server_hostname

    def connect(self) -> None:
        sock = socket.create_connection((self._connect_host, self.port), self.timeout, self.source_address)
        if self._tunnel_host:
            self.sock = sock
            self._tunnel()
            sock = self.sock
        self.sock = self._context.wrap_socket(sock, server_hostname=self._server_hostname_override)


def create_parity_ssl_context(ca_cert_path: Path | None) -> ssl.SSLContext:
    ssl_context = ssl.create_default_context(
        cafile=str(ca_cert_path) if ca_cert_path is not None else None
    )
    verify_x509_strict = getattr(ssl, "VERIFY_X509_STRICT", 0)
    if verify_x509_strict:
        ssl_context.verify_flags &= ~verify_x509_strict
    return ssl_context


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


@dataclass(frozen=True)
class PluginSpec:
    rust_feature: str
    python_dir: Path | None


PLUGIN_SPECS = {
    "awm": PluginSpec("awm", Path("summit_rcm/plugins/awm")),
    "bluetooth": PluginSpec("bluetooth", Path("summit_rcm/plugins/bluetooth")),
    "chrony": PluginSpec("chrony", Path("summit_rcm/plugins/chrony")),
    "fips": PluginSpec("fips", Path("summit_rcm")),
    "firewall": PluginSpec("firewall", Path("summit_rcm/plugins/firewall")),
    "log-forwarding": PluginSpec(
        "log-forwarding", Path("summit_rcm/plugins/log-forwarding")
    ),
    "provisioning": PluginSpec(
        "provisioning", Path("summit_rcm/plugins/provisioning")
    ),
    "radio-siso-mode": PluginSpec(
        "radio-siso-mode", Path("summit_rcm/plugins/radio-siso-mode")
    ),
    "stunnel": PluginSpec("stunnel", Path("summit_rcm/plugins/stunnel")),
    "unauthenticated": PluginSpec(
        "unauthenticated", Path("summit_rcm/plugins/unauthenticated")
    ),
}

RUST_PLUGIN_NAMES = {
    child.name.replace("_", "-")
    for child in (ROOT / "src/plugins").iterdir()
    if child.is_dir()
}


def parse_plugin_names(raw_names: list[str]) -> list[str]:
    plugins: set[str] = set()
    for raw_name in raw_names:
        for plugin in raw_name.split(","):
            name = plugin.strip().lower()
            if not name:
                continue
            if name not in RUST_PLUGIN_NAMES:
                raise ParityError(
                    f"unknown rust plugin: {name}. Expected one of: {', '.join(sorted(RUST_PLUGIN_NAMES))}"
                )
            plugins.add(name)
    return sorted(plugins)


def plugin_spec(name: str) -> PluginSpec:
    spec = PLUGIN_SPECS.get(name)
    if spec is not None:
        return spec
    return PluginSpec(name, None)


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


def rust_parity_features(plugins: list[str]) -> str:
    return ",".join([*CORE_RUST_PARITY_FEATURES, *[plugin_spec(plugin).rust_feature for plugin in plugins]])


def detect_python_runtime(python_repo: Path) -> str:
    if (python_repo / "summit_rcm").is_dir():
        return "summit-rcm"
    if (python_repo / "weblcm").is_dir():
        return "weblcm"
    raise ParityError(
        f"unable to detect Python runtime type for repo {python_repo}. "
        "Expected either a summit_rcm/ or weblcm/ package directory."
    )


def resolve_python_runtime(python_repo: Path, requested_runtime: str) -> str:
    detected = detect_python_runtime(python_repo)
    if requested_runtime == "auto":
        return detected
    if requested_runtime != detected:
        raise ParityError(
            f"Python runtime mismatch: requested {requested_runtime}, but repo {python_repo} looks like {detected}."
        )
    return requested_runtime


def python_runtime_executable(python_repo: Path) -> str:
    venv_python = python_repo / ".venv/bin/python"
    if venv_python.is_file() and os.access(venv_python, os.X_OK):
        return str(venv_python)
    return sys.executable


def ensure_mode_supported(mode: str, python_runtime: str) -> None:
    if python_runtime != "weblcm":
        return
    unsupported_modes = {"openapi", "coverage", "scaffold", "schema", "all", "auto"}
    if mode in unsupported_modes:
        raise ParityError(
            "The weblcm Python baseline does not support OpenAPI-driven parity modes "
            f"({mode}). Use 'responses' or 'auto' instead."
        )


def filter_cases_for_python_runtime(
    cases: list[dict[str, Any]], python_runtime: str
) -> list[dict[str, Any]]:
    if python_runtime != "weblcm":
        return cases

    ignored_case_ids = {
        "legacy_accesspoints",
        "legacy_accesspoints_scan_request",
        "legacy_definitions",
        "legacy_firewall_add_forward_port",
        "legacy_network_interface_wlo1_available_ap_channels",
        "legacy_network_interface_wlo1_station_dump",
        "legacy_network_interface_wlo1_summit_status",
        "legacy_ntp_override_sources",
    }

    return [
        {
            **case,
            "ignore_content_type": True,
        }
        for case in cases
        if case.get("id") not in ignored_case_ids
        and not normalize_path(case.get("path", "")).startswith("/api/v2/")
    ]


def is_v2_case(case: dict[str, Any]) -> bool:
    return normalize_path(case.get("path", "")).startswith("/api/v2/")


def split_cases_by_python_runtime(
    cases: list[dict[str, Any]],
    *,
    primary_runtime: str,
    legacy_runtime: str | None,
) -> list[tuple[list[dict[str, Any]], str]]:
    if legacy_runtime is None:
        return [(filter_cases_for_python_runtime(cases, primary_runtime), primary_runtime)]

    legacy_cases = [case for case in cases if not is_v2_case(case)]
    primary_cases = [case for case in cases if is_v2_case(case)]
    runs: list[tuple[list[dict[str, Any]], str]] = []
    if legacy_cases:
        runs.append((filter_cases_for_python_runtime(legacy_cases, legacy_runtime), legacy_runtime))
    if primary_cases:
        runs.append((filter_cases_for_python_runtime(primary_cases, primary_runtime), primary_runtime))
    return runs


def manifest_mixes_legacy_and_v2_cases(cases_path: Path) -> bool:
    cases = json.loads(cases_path.read_text())
    has_legacy = any(not is_v2_case(case) for case in cases)
    has_v2 = any(is_v2_case(case) for case in cases)
    return has_legacy and has_v2


def require_explicit_legacy_baseline(
    *,
    mode: str,
    cases_path: Path,
    python_runtime: str,
    legacy_python_repo: Path | None,
) -> None:
    if mode not in {"responses", "all"}:
        return
    if legacy_python_repo is not None:
        return
    if python_runtime != "summit-rcm":
        return
    if not manifest_mixes_legacy_and_v2_cases(cases_path):
        return
    raise ParityError(
        "response parity case manifest mixes legacy and /api/v2/ cases. "
        "Pass --legacy-python-repo to choose the legacy baseline explicitly; "
        "otherwise legacy cases would run against --python-repo."
    )


def python_parity_path(python_repo: Path, plugins: list[str], python_runtime: str) -> str:
    paths = [
        str(python_repo),
    ]
    if python_runtime == "summit-rcm":
        paths.extend(
            [
                str((python_repo / spec.python_dir).resolve())
                for plugin in plugins
                for spec in [plugin_spec(plugin)]
                if spec.python_dir is not None
            ]
        )
    if os.environ.get("PYTHONPATH"):
        paths.append(os.environ["PYTHONPATH"])
    return os.pathsep.join(paths)


def normalize_path(path: str) -> str:
    return path.split("?", 1)[0]


def path_segments(path: str) -> list[str]:
    return [segment for segment in normalize_path(path).split("/") if segment]


def case_covers_path(case_path: str, operation_path: str) -> bool:
    case_segments = path_segments(case_path)
    operation_segments = path_segments(operation_path)

    if len(case_segments) != len(operation_segments):
        return False

    for case_segment, operation_segment in zip(case_segments, operation_segments):
        if operation_segment.startswith("{") and operation_segment.endswith("}"):
            continue
        if case_segment != operation_segment:
            return False

    return True


def load_case_operations(cases_path: Path) -> list[tuple[str, str]]:
    cases = json.loads(cases_path.read_text())
    return [
        (case["method"].upper(), normalize_path(case["path"]))
        for case in cases
    ]


def generate_specs(python_repo: Path) -> tuple[dict[str, Any], dict[str, Any]]:
    python_executable = python_runtime_executable(python_repo)
    with tempfile.TemporaryDirectory(prefix="api-parity-openapi-") as tmp_dir:
        tmp = Path(tmp_dir)
        python_output = tmp / "python-openapi.json"
        rust_output = tmp / "rust-openapi.json"

        run_checked(
            [python_executable, "generate_docs.py"],
            cwd=python_repo,
            env={
                **os.environ,
                "DOCS_GENERATION": "True",
                "OPENAPI_JSON_PATH": str(python_output),
            },
        )
        run_checked(
            [
                "cargo",
                "run",
                "--quiet",
                "--bin",
                "generate_openapi",
                "--all-features",
            ],
            cwd=ROOT,
            env=rust_env(
                extra_env={"SUMMIT_RCM_OPENAPI_OUTPUT": str(rust_output)},
            ),
        )

        return json.loads(python_output.read_text()), json.loads(rust_output.read_text())


def v2_paths_only(spec: dict[str, Any]) -> dict[str, Any]:
    paths = spec.get("paths", {})
    if not isinstance(paths, dict):
        return spec
    return {
        **spec,
        "paths": {
            path: value
            for path, value in paths.items()
            if normalize_path(path).startswith("/api/v2/")
        },
    }


def shared_operations(python_spec: dict[str, Any], rust_spec: dict[str, Any]) -> list[tuple[str, str]]:
    python_paths = python_spec.get("paths", {})
    rust_paths = rust_spec.get("paths", {})

    operations: list[tuple[str, str]] = []
    for path in sorted(set(python_paths) & set(rust_paths)):
        python_methods = python_paths[path].keys()
        rust_methods = rust_paths[path].keys()
        for method in sorted(set(python_methods) & set(rust_methods)):
            operations.append((method.upper(), path))
    return operations


def shared_operation_specs(
    python_spec: dict[str, Any], rust_spec: dict[str, Any]
) -> list[dict[str, Any]]:
    python_paths = python_spec.get("paths", {})
    rust_paths = rust_spec.get("paths", {})

    operations: list[dict[str, Any]] = []
    for path in sorted(set(python_paths) & set(rust_paths)):
        python_methods = python_paths[path]
        rust_methods = rust_paths[path]
        for method in sorted(set(python_methods) & set(rust_methods)):
            operations.append(
                {
                    "method": method.upper(),
                    "path": path,
                    "python": python_methods[method],
                    "rust": rust_methods[method],
                }
            )
    return operations


def uncovered_shared_operations(
    shared_ops: list[dict[str, Any]], covered_ops: list[tuple[str, str]]
) -> list[dict[str, Any]]:
    return [
        operation
        for operation in shared_ops
        if not any(
            covered_method == operation["method"]
            and case_covers_path(covered_path, operation["path"])
            for covered_method, covered_path in covered_ops
        )
    ]


def operation_tag(operation: dict[str, Any]) -> str:
    for spec_key in ("rust", "python"):
        tags = operation.get(spec_key, {}).get("tags") or []
        if tags:
            return str(tags[0])
    return "untagged"


def coverage_rows_by_tag(
    shared_ops: list[dict[str, Any]], covered_ops: list[tuple[str, str]]
) -> list[tuple[str, int, int]]:
    rows: dict[str, dict[str, int]] = {}
    for operation in shared_ops:
        tag = operation_tag(operation)
        row = rows.setdefault(tag, {"total": 0, "covered": 0})
        row["total"] += 1
        if any(
            covered_method == operation["method"]
            and case_covers_path(covered_path, operation["path"])
            for covered_method, covered_path in covered_ops
        ):
            row["covered"] += 1
    return sorted(
        ((tag, counts["covered"], counts["total"]) for tag, counts in rows.items()),
        key=lambda row: (row[2] - row[1], row[0]),
        reverse=True,
    )


def resolve_ref(spec: dict[str, Any], value: Any) -> Any:
    resolved = value
    seen: set[str] = set()
    while isinstance(resolved, dict) and "$ref" in resolved:
        ref = resolved["$ref"]
        if not isinstance(ref, str) or not ref.startswith("#/") or ref in seen:
            break
        seen.add(ref)
        target: Any = spec
        for part in ref[2:].split("/"):
            if not isinstance(target, dict):
                return value
            target = target.get(part)
        resolved = target
    return resolved


def sample_string(name_hint: str, schema: dict[str, Any]) -> str:
    schema_format = schema.get("format")
    if schema_format == "date-time":
        return "2026-01-01T00:00:00Z"
    if schema_format == "date":
        return "2026-01-01"
    if schema_format == "password":
        return "summit"

    lowered = name_hint.lower()
    if "username" in lowered or lowered == "user":
        return "root"
    if "password" in lowered or "passphrase" in lowered:
        return "summit"
    if "controller" in lowered:
        return "controller0"
    if "device" in lowered:
        return "device0"
    if "interface" in lowered:
        return "lo"
    if lowered == "type":
        return "cert"
    if "file" in lowered:
        return "sample.txt"
    if lowered == "name":
        return "sample"
    return "sample"


def sample_value_from_schema(
    spec: dict[str, Any], schema: Any, *, name_hint: str = "value"
) -> Any:
    resolved = resolve_ref(spec, schema)
    if not isinstance(resolved, dict) or not resolved:
        return {}

    if "example" in resolved:
        return resolved["example"]
    if "default" in resolved:
        return resolved["default"]
    if "enum" in resolved and resolved["enum"]:
        return resolved["enum"][0]

    for key in ("oneOf", "anyOf"):
        options = resolved.get(key)
        if isinstance(options, list) and options:
            return sample_value_from_schema(spec, options[0], name_hint=name_hint)

    all_of = resolved.get("allOf")
    if isinstance(all_of, list) and all_of:
        merged: dict[str, Any] = {}
        for sub_schema in all_of:
            sample = sample_value_from_schema(spec, sub_schema, name_hint=name_hint)
            if isinstance(sample, dict):
                merged.update(sample)
        if merged:
            return merged

    schema_type = resolved.get("type")
    if schema_type == "object" or "properties" in resolved:
        properties = resolved.get("properties", {})
        if isinstance(properties, dict) and properties:
            return {
                property_name: sample_value_from_schema(
                    spec,
                    property_schema,
                    name_hint=property_name,
                )
                for property_name, property_schema in properties.items()
            }
        additional = resolved.get("additionalProperties")
        if isinstance(additional, dict):
            return {"example": sample_value_from_schema(spec, additional, name_hint="value")}
        return {}
    if schema_type == "array":
        return [sample_value_from_schema(spec, resolved.get("items", {}), name_hint=name_hint)]
    if schema_type == "boolean":
        return True
    if schema_type == "integer":
        return 1
    if schema_type == "number":
        return 1
    if schema_type == "string":
        return sample_string(name_hint, resolved)

    return {}


def best_success_status(operation: dict[str, Any]) -> int | None:
    responses = operation.get("responses", {})
    success_codes = sorted(
        int(code)
        for code in responses
        if isinstance(code, str) and code.isdigit() and code.startswith("2")
    )
    return success_codes[0] if success_codes else None


def build_scaffold_path(
    spec: dict[str, Any], path: str, parameters: list[dict[str, Any]]
) -> tuple[str, list[str]]:
    scaffold_path = path
    query_params: dict[str, str] = {}
    notes: list[str] = []

    for parameter in parameters:
        resolved = resolve_ref(spec, parameter)
        if not isinstance(resolved, dict):
            continue
        location = resolved.get("in")
        name = str(resolved.get("name", "value"))
        required = bool(resolved.get("required")) or location == "path"
        if not required:
            continue

        value = sample_value_from_schema(spec, resolved.get("schema", {}), name_hint=name)
        if value == {}:
            value = sample_string(name, {})

        if location == "path":
            scaffold_path = scaffold_path.replace(
                f"{{{name}}}",
                urllib.parse.quote(str(value), safe=""),
            )
        elif location == "query":
            query_params[name] = str(value).lower() if isinstance(value, bool) else str(value)
        else:
            notes.append(f"review required {location} parameter: {name}")

    if query_params:
        scaffold_path = f"{scaffold_path}?{urllib.parse.urlencode(query_params, doseq=True)}"

    return scaffold_path, notes


def request_body_example(spec: dict[str, Any], operation: dict[str, Any]) -> tuple[Any | None, list[str]]:
    request_body = resolve_ref(spec, operation.get("requestBody"))
    if not isinstance(request_body, dict):
        return None, []

    content = request_body.get("content", {})
    if not isinstance(content, dict) or not content:
        return None, []

    json_schema = None
    if "application/json" in content:
        json_schema = content["application/json"].get("schema", {})
    if json_schema is not None:
        return sample_value_from_schema(spec, json_schema, name_hint="body"), []

    content_types = ", ".join(sorted(content))
    return None, [f"manual request body required for content-type(s): {content_types}"]


def scaffold_case_id(method: str, path: str) -> str:
    slug = normalize_path(path).strip("/").replace("{", "").replace("}", "")
    slug = "_".join(segment for segment in slug.replace("-", "_").split("/") if segment)
    slug = "".join(ch if ch.isalnum() or ch == "_" else "_" for ch in slug)
    return f"todo_{method.lower()}_{slug or 'root'}"


def default_compare_mode(method: str) -> str:
    return compare_mode_for_path(method, "")


def compare_mode_for_path(method: str, path: str) -> str:
    if method == "GET" and (path.endswith("/stationDump") or path.endswith("/stats") or path == "/networkInterfaceStatistics"):
        return "json_shape"
    return "exact"


def scaffold_case(spec: dict[str, Any], operation: dict[str, Any]) -> dict[str, Any]:
    rust_operation = operation["rust"]
    scaffold_path, notes = build_scaffold_path(
        spec,
        operation["path"],
        rust_operation.get("parameters", []),
    )
    body, body_notes = request_body_example(spec, rust_operation)
    notes.extend(body_notes)

    case: dict[str, Any] = {
        "id": scaffold_case_id(operation["method"], operation["path"]),
        "method": operation["method"],
        "path": scaffold_path,
        "compare": compare_mode_for_path(operation["method"], operation["path"]),
        "_operation_id": rust_operation.get("operationId"),
        "_tag": operation_tag(operation),
    }

    success_status = best_success_status(rust_operation)
    if success_status is not None:
        case["expected_status"] = success_status
    if body is not None:
        case["body"] = body
    if operation["path"] != scaffold_path:
        case["_template_path"] = operation["path"]
    if case["_tag"] in PLUGIN_SPECS:
        case["plugins"] = [case["_tag"]]
    if notes:
        case["_notes"] = sorted(set(notes))

    return case


def scaffold_uncovered_cases(
    python_repo: Path,
    cases_path: Path,
    *,
    output_path: Path | None,
) -> None:
    python_spec, rust_spec = generate_specs(python_repo)
    shared_ops = shared_operation_specs(python_spec, rust_spec)
    covered_ops = load_case_operations(cases_path)
    uncovered = uncovered_shared_operations(shared_ops, covered_ops)
    scaffold = [scaffold_case(rust_spec, operation) for operation in uncovered]

    rendered = json.dumps(scaffold, indent=2) + "\n"
    if output_path is None:
        sys.stdout.write(rendered)
        return

    output_path.write_text(rendered)
    print(f"Wrote {len(scaffold)} scaffold cases to {output_path}")


def run_checked(command: list[str], *, cwd: Path | None = None, env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
    completed = subprocess.run(
        command,
        cwd=cwd,
        env=env,
        text=True,
        capture_output=True,
        check=False,
    )
    if completed.returncode != 0:
        raise ParityError(
            f"command failed: {' '.join(command)}\nstdout:\n{completed.stdout}\nstderr:\n{completed.stderr}"
        )
    return completed


def rust_env(*, extra_env: dict[str, str] | None = None) -> dict[str, str]:
    env = {**os.environ}
    if extra_env:
        env.update(extra_env)
    rustflags = env.get("RUSTFLAGS", "").strip()
    env["RUSTFLAGS"] = f"{rustflags} -A warnings".strip()
    return env


def rust_binary_path(bin_name: str) -> Path:
    suffix = ".exe" if os.name == "nt" else ""
    return ROOT / "build" / "debug" / f"{bin_name}{suffix}"


def ensure_rust_binary(*, bin_name: str, features: str) -> Path:
    run_checked(
        [
            "cargo",
            "build",
            "--quiet",
            "--bin",
            bin_name,
            "--no-default-features",
            "--features",
            features,
        ],
        cwd=ROOT,
        env=rust_env(),
    )
    binary_path = rust_binary_path(bin_name)
    if not binary_path.is_file():
        raise ParityError(f"expected built Rust binary at {binary_path}")
    return binary_path


def compare_openapi(python_repo: Path) -> None:
    python_spec, rust_spec = generate_specs(python_repo)
    python_spec = v2_paths_only(python_spec)
    rust_spec = v2_paths_only(rust_spec)

    python_paths = python_spec.get("paths", {})
    rust_paths = rust_spec.get("paths", {})

    python_only = sorted(set(python_paths) - set(rust_paths))
    rust_only = sorted(set(rust_paths) - set(python_paths))

    method_diffs: list[tuple[str, list[str], list[str]]] = []
    for path in sorted(set(python_paths) & set(rust_paths)):
        python_methods = sorted(python_paths[path].keys())
        rust_methods = sorted(rust_paths[path].keys())
        if python_methods != rust_methods:
            method_diffs.append((path, python_methods, rust_methods))

    if python_only or rust_only or method_diffs:
        lines = ["OpenAPI parity failed:"]
        if python_only:
            lines.append("Python-only paths:")
            lines.extend(f"  {path}" for path in python_only)
        if rust_only:
            lines.append("Rust-only paths:")
            lines.extend(f"  {path}" for path in rust_only)
        if method_diffs:
            lines.append("Method diffs:")
            lines.extend(
                f"  {path}: python={python_methods}, rust={rust_methods}"
                for path, python_methods, rust_methods in method_diffs
            )
        raise ParityError("\n".join(lines))

    print("OpenAPI parity passed: paths and methods match.")


def report_response_coverage(python_repo: Path, cases_path: Path) -> None:
    python_spec, rust_spec = generate_specs(python_repo)
    shared_ops = shared_operation_specs(python_spec, rust_spec)
    covered_ops = load_case_operations(cases_path)

    uncovered = uncovered_shared_operations(shared_ops, covered_ops)

    print(
        f"Response coverage: {len(shared_ops) - len(uncovered)}/{len(shared_ops)} shared operations covered by {cases_path.name}."
    )
    print("Coverage by tag:")
    for tag, covered, total in coverage_rows_by_tag(shared_ops, covered_ops):
        print(f"  {tag}: {covered}/{total} covered ({total - covered} uncovered)")
    if not uncovered:
        print("Response coverage passed: every shared operation has a parity case.")
        return

    lines = ["Response coverage failed: uncovered shared operations:"]
    lines.extend(f"  {operation['method']} {operation['path']}" for operation in uncovered)
    raise ParityError("\n".join(lines))


def _collect_schema_properties(
    spec: dict[str, Any], schema: Any, *, path: str = ""
) -> list[tuple[str, str]]:
    """Recursively collect (json_pointer, type_or_enum) for schema leaf nodes."""
    resolved = resolve_ref(spec, schema)
    if not isinstance(resolved, dict):
        return [(path, "unknown")]

    results: list[tuple[str, str]] = []
    schema_type = resolved.get("type", "")
    enum_vals = resolved.get("enum")

    # allOf / anyOf / oneOf – union all branches
    for combiner in ("allOf", "anyOf", "oneOf"):
        for sub in resolved.get(combiner) or []:
            results.extend(_collect_schema_properties(spec, sub, path=path))

    if "properties" in resolved:
        for prop_name, prop_schema in resolved["properties"].items():
            results.extend(
                _collect_schema_properties(
                    spec, prop_schema, path=f"{path}/{prop_name}"
                )
            )
    elif schema_type == "array" and "items" in resolved:
        results.extend(
            _collect_schema_properties(spec, resolved["items"], path=f"{path}[]")
        )
    elif schema_type or enum_vals:
        leaf = str(enum_vals) if enum_vals else schema_type
        results.append((path or "/", leaf))

    return results


def _required_properties(spec: dict[str, Any], schema: Any) -> set[str]:
    resolved = resolve_ref(spec, schema)
    if not isinstance(resolved, dict):
        return set()
    required: set[str] = set(resolved.get("required") or [])
    for combiner in ("allOf", "anyOf", "oneOf"):
        for sub in resolved.get(combiner) or []:
            required |= _required_properties(spec, sub)
    return required


def _schema_diff_lines(
    label: str,
    python_spec: dict[str, Any],
    python_schema: Any,
    rust_spec: dict[str, Any],
    rust_schema: Any,
) -> list[str]:
    python_props = dict(_collect_schema_properties(python_spec, python_schema))
    rust_props = dict(_collect_schema_properties(rust_spec, rust_schema))
    python_required = _required_properties(python_spec, python_schema)
    rust_required = _required_properties(rust_spec, rust_schema)

    lines: list[str] = []
    all_paths = sorted(set(python_props) | set(rust_props))
    for prop_path in all_paths:
        in_python = prop_path in python_props
        in_rust = prop_path in rust_props
        if not in_python:
            lines.append(f"    {label}{prop_path}: rust-only (type={rust_props[prop_path]})")
        elif not in_rust:
            lines.append(f"    {label}{prop_path}: python-only (type={python_props[prop_path]})")
        elif python_props[prop_path] != rust_props[prop_path]:
            lines.append(
                f"    {label}{prop_path}: type mismatch "
                f"python={python_props[prop_path]} rust={rust_props[prop_path]}"
            )

    common = set(python_props) & set(rust_props)
    req_only_python = (python_required & common) - rust_required
    req_only_rust = (rust_required & common) - python_required
    for name in sorted(req_only_python):
        lines.append(f"    {label}/{name}: required in python, optional in rust")
    for name in sorted(req_only_rust):
        lines.append(f"    {label}/{name}: required in rust, optional in python")
    return lines


def diff_schemas(python_repo: Path, *, output_path: Path | None) -> None:
    """
    Static schema diff for all shared operations.

    For every operation present in both the Python and Rust OpenAPI specs, compare:
      - required path/query parameter names and types
      - request body required fields and field types
      - documented response status codes
      - response content types and top-level response schema shape
    No servers are started; this is purely a spec-level review.
    """
    python_spec, rust_spec = generate_specs(python_repo)
    shared_ops = shared_operation_specs(python_spec, rust_spec)

    diffs: list[str] = []
    for operation in shared_ops:
        method = operation["method"]
        path = operation["path"]
        python_op = operation["python"]
        rust_op = operation["rust"]
        op_diffs: list[str] = []

        # --- parameters ---
        def params_by_location(op: dict[str, Any], spec: dict[str, Any]) -> dict[str, dict[str, Any]]:
            result: dict[str, dict[str, Any]] = {}
            for param in op.get("parameters") or []:
                resolved = resolve_ref(spec, param)
                if isinstance(resolved, dict):
                    key = f"{resolved.get('in','?')}:{resolved.get('name','?')}"
                    result[key] = resolved
            return result

        python_params = params_by_location(python_op, python_spec)
        rust_params = params_by_location(rust_op, rust_spec)
        all_param_keys = sorted(set(python_params) | set(rust_params))
        for pkey in all_param_keys:
            if pkey not in python_params:
                op_diffs.append(f"  param {pkey}: rust-only")
            elif pkey not in rust_params:
                op_diffs.append(f"  param {pkey}: python-only")
            else:
                p_req = bool(python_params[pkey].get("required"))
                r_req = bool(rust_params[pkey].get("required"))
                if p_req != r_req:
                    op_diffs.append(
                        f"  param {pkey}: required mismatch python={p_req} rust={r_req}"
                    )
                schema_diffs = _schema_diff_lines(
                    f"param {pkey} ",
                    python_spec, python_params[pkey].get("schema", {}),
                    rust_spec, rust_params[pkey].get("schema", {}),
                )
                op_diffs.extend(schema_diffs)

        # --- request body ---
        def request_body_schema(op: dict[str, Any], spec: dict[str, Any]) -> Any:
            rb = resolve_ref(spec, op.get("requestBody"))
            if not isinstance(rb, dict):
                return None
            content = rb.get("content", {})
            json_content = content.get("application/json", {})
            return json_content.get("schema") if json_content else None

        python_rb = request_body_schema(python_op, python_spec)
        rust_rb = request_body_schema(rust_op, rust_spec)
        if (python_rb is None) != (rust_rb is None):
            op_diffs.append(
                f"  requestBody: {'python' if python_rb else 'rust'} has it, "
                f"{'rust' if python_rb else 'python'} does not"
            )
        elif python_rb is not None and rust_rb is not None:
            body_diffs = _schema_diff_lines(
                "body:", python_spec, python_rb, rust_spec, rust_rb
            )
            op_diffs.extend(body_diffs)

        # --- response status codes ---
        python_codes = sorted(
            python_op.get("responses", {}).keys(), key=lambda c: str(c)
        )
        rust_codes = sorted(
            rust_op.get("responses", {}).keys(), key=lambda c: str(c)
        )
        python_code_set = set(str(c) for c in python_codes)
        rust_code_set = set(str(c) for c in rust_codes)
        for code in sorted(python_code_set - rust_code_set):
            op_diffs.append(f"  response {code}: python-only")
        for code in sorted(rust_code_set - python_code_set):
            op_diffs.append(f"  response {code}: rust-only")

        # --- response content types and schema for shared status codes ---
        for code in sorted(python_code_set & rust_code_set):
            python_resp = python_op.get("responses", {}).get(code, {})
            rust_resp = rust_op.get("responses", {}).get(code, {})

            python_ct = set((python_resp.get("content") or {}).keys())
            rust_ct = set((rust_resp.get("content") or {}).keys())
            for ct in sorted(python_ct - rust_ct):
                op_diffs.append(f"  response {code} content-type {ct}: python-only")
            for ct in sorted(rust_ct - python_ct):
                op_diffs.append(f"  response {code} content-type {ct}: rust-only")

            shared_ct = python_ct & rust_ct
            for ct in sorted(shared_ct):
                python_schema = (python_resp.get("content") or {}).get(ct, {}).get("schema")
                rust_schema = (rust_resp.get("content") or {}).get(ct, {}).get("schema")
                if python_schema is not None and rust_schema is not None:
                    resp_diffs = _schema_diff_lines(
                        f"response {code} {ct} ",
                        python_spec, python_schema,
                        rust_spec, rust_schema,
                    )
                    op_diffs.extend(resp_diffs)

        if op_diffs:
            diffs.append(f"{method} {path}")
            diffs.extend(op_diffs)

    total_ops = len(shared_ops)
    ops_with_diffs = sum(
        1 for operation in shared_ops
        if any(
            operation["method"] == line.split()[0] and operation["path"] == line.split()[1]
            for line in diffs
            if not line.startswith(" ")
        )
    )

    summary_lines = [
        f"Schema diff: {total_ops} shared operations, "
        f"{len([l for l in diffs if not l.startswith(' ')])} with differences.",
        "",
        *diffs,
    ]
    rendered = "\n".join(summary_lines) + ("\n" if summary_lines else "")

    if output_path is not None:
        output_path.write_text(rendered)
        print(
            f"Schema diff written to {output_path} "
            f"({len([l for l in diffs if not l.startswith(' ')])} operations with differences "
            f"out of {total_ops} shared)."
        )
    else:
        sys.stdout.write(rendered)


def write_test_config(
    temp_dir: Path,
    rust_port: int,
    python_port: int,
    *,
    sessions_on: bool,
    enabled_plugins: list[str],
    python_runtime: str,
) -> tuple[Path, Path, Path, Path, Path]:
    cert_path = temp_dir / "server.crt"
    key_path = temp_dir / "server.key"
    settings_path = temp_dir / "summit-rcm-settings.ini"
    rust_config_path = temp_dir / "rust-summit-rcm.ini"
    python_config_path = temp_dir / "python-summit-rcm.ini"
    provisioning_state_path = temp_dir / "provisioning-state"
    plugin_dir = temp_dir / "plugins"
    plugin_dir.mkdir(parents=True, exist_ok=True)

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
                    "basicConstraints=critical,CA:TRUE",
                    "-addext",
            "extendedKeyUsage=serverAuth",
        ]
    )

    password = "summit"
    salt = "parity-salt"
    digest = hashlib.sha256(f"{salt}{password}".encode()).hexdigest()
    permission = " ".join(USER_PERMISSIONS)
    settings_path.write_text(
        "\n".join(
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
    )

    def write_server_config(path: Path, port: int) -> None:
        plugin_lines = ["[plugins]", *[f"{plugin} = true" for plugin in enabled_plugins], ""]
        path.write_text(
            "\n".join(
                [
                    "[/]",
                    f"tools.sessions.on = {'true' if sessions_on else 'false'}",
                    "",
                    *plugin_lines,
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
        )

    def write_weblcm_server_config(path: Path, port: int) -> None:
        dispatch_paths = [
            "/networkStatus",
            "/connections",
            "/connection",
            "/accesspoints",
            "/version",
            "/networkInterfaces",
            "/networkInterface",
            "/networkInterfaceStatistics",
            "/networkInterfaceDriverInfo",
            "/firewall",
            "/certificates",
            "/certificateProvisioning",
            "/files",
            "/file",
            "/users",
            "/firmware",
            "/login",
            "/logData",
            "/logSetting",
            "/logForwarding",
            "/poweroff",
            "/suspend",
            "/reboot",
            "/factoryReset",
            "/allowUnauthenticatedResetReboot",
            "/datetime",
            "/ntp",
            "/awm",
            "/positioningSwitch",
            "/positioning",
            "/fips",
            "/bluetooth",
            "/modemFirmwareUpdate",
            "/wifiEnable",
            "/radioSISOMode",
            "/modemEnable",
            "/stunnel",
        ]
        dispatch_lines = [
            line
            for route in dispatch_paths
            for line in ("", f"[{route}]", 'request.dispatch: cherrypy.dispatch.MethodDispatcher()')
        ]
        path.write_text(
            "\n".join(
                [
                    "[global]",
                    'server.socket_host: "127.0.0.1"',
                    f"server.socket_port: {port}",
                    f'server.ssl_certificate: "{cert_path}"',
                    f'server.ssl_private_key: "{key_path}"',
                    f'server.ssl_certificate_chain: "{cert_path}"',
                    'server.ssl_module: "builtin"',
                    "server.thread_pool: 1",
                    "engine.autoreload.on: False",
                    "checker.on: False",
                    "tools.log_headers.on: False",
                    "tools.encode.on: True",
                    'tools.encode.encoding: "utf-8"',
                    "request.show_tracebacks: False",
                    "request.show_mismatched_params: False",
                    'log.screen: False',
                    'log.access_file: ""',
                    'log.error_file: ""',
                    "",
                    "[/]",
                    f"tools.sessions.on: {'True' if sessions_on else 'False'}",
                    "tools.sessions.secure: False",
                    "tools.sessions.httponly: True",
                    *dispatch_lines,
                    "",
                    "[plugins]",
                    "status: True",
                    "networking: True",
                    "system: True",
                    "logging: True",
                    "help: True",
                    "",
                    "[weblcm]",
                    'default_username: "root"',
                    'default_password: "summit"',
                    "allow_multiple_user_sessions: True",
                    "network_status_restricted: False",
                    "enable_client_auth: False",
                    "",
                ]
            )
        )

    write_server_config(rust_config_path, rust_port)
    if python_runtime == "weblcm":
        settings_path.write_text(
            "\n".join(
                [
                    "[settings]",
                    "session_timeout = 10",
                    "login_retry_times = 5",
                    "login_retry_window = 600",
                    "user_callback_timeout = 10",
                    "max_web_clients = 8",
                    "",
                    "[root]",
                    f"salt = {salt}",
                    f"password = {digest}",
                    f"permission = {permission}",
                    "",
                ]
            )
        )
        write_weblcm_server_config(python_config_path, python_port)
    else:
        write_server_config(python_config_path, python_port)
    provisioning_state_path.write_text("2\n")

    return rust_config_path, python_config_path, settings_path, provisioning_state_path, cert_path


def reserve_local_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def python_wrapper_command(python_runtime: str) -> str:
    if python_runtime == "weblcm":
        return (
            "import os; "
            "config_path = os.environ['WEBLCM_PYTHON_SERVER_CONF_FILE']; "
            "settings_path = os.environ['WEBLCM_PYTHON_SETTINGS_FILE']; "
            "import weblcm.definition as definition; "
            "definition.WEBLCM_PYTHON_SERVER_CONF_FILE = config_path; "
            "definition.WEBLCM_PYTHON_SETTINGS_FILE = settings_path; "
            "import weblcm.settings as settings_mod; "
            "settings_mod.definition.WEBLCM_PYTHON_SETTINGS_FILE = settings_path; "
            "settings_mod.WeblcmConfigManage._filename = settings_path; "
            "settings_mod.WeblcmConfigManage._parser.read(settings_path); "
            "import weblcm.utils as utils; "
            "utils.WEBLCM_PYTHON_SERVER_CONF_FILE = config_path; "
            "import weblcm; "
            "raise SystemExit(weblcm.main())"
        )

    return (
        "import os; "
        "import importlib.util; "
        "os.environ['DOCS_GENERATION'] = 'True'; "
        "state_path = os.environ.get('SUMMIT_RCM_PROVISIONING_STATE_FILE'); "
        "import summit_rcm; "
        "(state_path and importlib.util.find_spec('summit_rcm_provisioning') and setattr(__import__('summit_rcm_provisioning.services.provisioning_service', fromlist=['PROVISIONING_STATE_FILE_PATH']), 'PROVISIONING_STATE_FILE_PATH', state_path)); "
        "from summit_rcm.settings import ServerConfig; "
        "from summit_rcm.rest_api.services.spectree_service import SpectreeService; "
        "SpectreeService.validate = lambda self, *args, **kwargs: (lambda func: func); "
        "ServerConfig()._rest_api_docs_enabled = False; "
        "raise SystemExit(summit_rcm.main())"
    )


def write_python_stub_modules(temp_dir: Path, python_runtime: str) -> Path | None:
    if python_runtime != "weblcm":
        return None

    stub_dir = temp_dir / "python-stubs"
    stub_dir.mkdir(parents=True, exist_ok=True)
    (stub_dir / "swclient.py").write_text(
        "def _missing(*args, **kwargs):\n"
        "    raise RuntimeError('swclient is unavailable in parity environment')\n"
        "\n"
        "prepare_fw_update = _missing\n"
        "do_fw_update = _missing\n"
        "open_progress_ipc = _missing\n"
        "read_progress_ipc = _missing\n"
        "close_progress_ipc = _missing\n"
        "end_fw_update = _missing\n"
    )
    return stub_dir


def start_process(name: str, command: list[str], *, cwd: Path, env: dict[str, str]) -> ManagedProcess:
    process = subprocess.Popen(
        command,
        cwd=cwd,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    return ManagedProcess(name, process)


def request(
    url: str,
    method: str,
    *,
    body: Any = None,
    multipart: list[dict[str, Any]] | None = None,
    cookie: str | None = None,
    ca_cert_path: Path | None = None,
    connect_host: str | None = None,
    timeout_seconds: float = DEFAULT_REQUEST_TIMEOUT_SECONDS,
    read_body: bool = True,
) -> HttpResponse:
    data = None
    headers: dict[str, str] = {}
    if body is not None and multipart is not None:
        raise ValueError("request body and multipart are mutually exclusive")
    if multipart is not None:
        boundary = f"copilot-{uuid.uuid4().hex}"
        chunks: list[bytes] = []
        for part in multipart:
            name = str(part["name"])
            chunks.append(f"--{boundary}\r\n".encode())
            disposition = f"Content-Disposition: form-data; name=\"{name}\""
            filename = part.get("filename")
            if filename is not None:
                disposition += f"; filename=\"{filename}\""
            chunks.append(f"{disposition}\r\n".encode())
            content_type = part.get("content_type")
            if content_type:
                chunks.append(f"Content-Type: {content_type}\r\n".encode())
            chunks.append(b"\r\n")
            if "text" in part:
                part_bytes = str(part["text"]).encode()
            else:
                part_bytes = str(part.get("content", "")).encode()
            chunks.append(part_bytes)
            chunks.append(b"\r\n")
        chunks.append(f"--{boundary}--\r\n".encode())
        data = b"".join(chunks)
        headers["Content-Type"] = f"multipart/form-data; boundary={boundary}"
    elif body is not None:
        data = json.dumps(body).encode()
        headers["Content-Type"] = "application/json"
    if cookie:
        headers["Cookie"] = cookie

    parsed_url = urllib.parse.urlsplit(url)

    if parsed_url.scheme.lower() == "https" and connect_host is not None:
        ssl_context = create_parity_ssl_context(ca_cert_path)
        path = parsed_url.path or "/"
        if parsed_url.query:
            path = f"{path}?{parsed_url.query}"
        headers.setdefault("Host", parsed_url.netloc)
        connection = ResolvedHTTPSConnection(
            parsed_url.hostname or parsed_url.netloc,
            connect_host=connect_host,
            server_hostname=parsed_url.hostname or parsed_url.netloc,
            port=parsed_url.port or 443,
            timeout=timeout_seconds,
            context=ssl_context,
        )
        try:
            connection.request(method, path, body=data, headers=headers)
            response = connection.getresponse()
            response_body = response.read() if read_body else b""
            return HttpResponse(
                response.status,
                {k.lower(): v for k, v in response.headers.items()},
                response_body,
            )
        finally:
            connection.close()

    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    request_kwargs: dict[str, Any] = {"timeout": timeout_seconds}
    if url.lower().startswith("https://"):
        if parsed_url.hostname in {"127.0.0.1", "localhost"}:
            ssl_context = ssl._create_unverified_context()
        else:
            ssl_context = create_parity_ssl_context(ca_cert_path)
        request_kwargs["context"] = ssl_context

    try:
        with urllib.request.urlopen(req, **request_kwargs) as response:
            response_body = response.read() if read_body else b""
            return HttpResponse(response.status, {k.lower(): v for k, v in response.headers.items()}, response_body)
    except urllib.error.HTTPError as error:
        error_body = error.read() if read_body else b""
        return HttpResponse(error.code, {k.lower(): v for k, v in error.headers.items()}, error_body)


def request_or_error(
    url: str,
    method: str,
    *,
    body: Any = None,
    multipart: list[dict[str, Any]] | None = None,
    cookie: str | None = None,
    ca_cert_path: Path | None = None,
    connect_host: str | None = None,
    timeout_seconds: float = DEFAULT_REQUEST_TIMEOUT_SECONDS,
    read_body: bool = True,
) -> tuple[HttpResponse | None, str | None]:
    try:
        return (
            request(
                url,
                method,
                body=body,
                multipart=multipart,
                cookie=cookie,
                ca_cert_path=ca_cert_path,
                connect_host=connect_host,
                timeout_seconds=timeout_seconds,
                read_body=read_body,
            ),
            None,
        )
    except Exception as error:  # pragma: no cover - network/runtime failures are environment-specific
        return None, f"{type(error).__name__}: {error}"


def wait_for_server(
    name: str,
    base_url: str,
    process: ManagedProcess,
    *,
    ca_cert_path: Path,
    connect_host: str | None = None,
    request_timeout_seconds: float,
    startup_timeout_seconds: float,
) -> str:
    deadline = time.time() + startup_timeout_seconds
    last_error = ""
    while time.time() < deadline:
        if process.process.poll() is not None:
            raise ParityError(f"{name} exited early:\n{process.output()}")
        try:
            response = request(
                f"{base_url}/version",
                "GET",
                ca_cert_path=ca_cert_path,
                connect_host=connect_host,
                timeout_seconds=request_timeout_seconds,
            )
            if response.status in {200, 401, 403, 404, 500}:
                return base_url
        except Exception as error:  # pragma: no cover - probe loop
            last_error = f"{base_url}: {error}"
        time.sleep(0.25)
    raise ParityError(f"timed out waiting for {name} at {base_url}: {last_error}")


def normalize_json(value: Any) -> Any:
    if isinstance(value, dict):
        return {key: normalize_json(value[key]) for key in sorted(value)}
    if isinstance(value, list):
        return [normalize_json(item) for item in value]
    return value


def should_read_body(case: dict[str, Any]) -> bool:
    return bool(case.get("read_body", True))


def should_verify_readback(case: dict[str, Any]) -> bool:
    return case["method"] != "GET"


def is_json_content_type(content_type: str) -> bool:
    return content_type == "application/json" or content_type.endswith("+json")


def is_textual_content_type(content_type: str) -> bool:
    if not content_type:
        return True
    if content_type.startswith("text/"):
        return True
    return is_json_content_type(content_type)


def candidate_readback_paths(path: str) -> list[str]:
    segments = path_segments(path)
    candidates = [normalize_path(path)]
    while len(segments) > 1:
        segments = segments[:-1]
        candidates.append("/" + "/".join(segments))
    return candidates


def case_plugins_key(case: dict[str, Any]) -> tuple[str, ...]:
    return tuple(plugin_names_from_case(case))


def find_readback_case(
    case: dict[str, Any], cases: list[dict[str, Any]]
) -> dict[str, Any] | None:
    expected_plugins = case_plugins_key(case)
    get_cases = [
        candidate
        for candidate in cases
        if candidate["method"] == "GET"
        and not candidate.get("skip_live")
        and case_plugins_key(candidate) == expected_plugins
    ]

    for candidate_path in candidate_readback_paths(case["path"]):
        exact_matches = [
            candidate
            for candidate in get_cases
            if normalize_path(candidate["path"]) == candidate_path
        ]
        if exact_matches:
            exact_matches.sort(
                key=lambda candidate: (
                    candidate.get("compare") == "json_shape",
                    len(normalize_path(candidate["path"])),
                )
            )
            return exact_matches[0]

    return None


def render_compared_body(value: Any) -> str:
    if value is None:
        return "<ignored>"
    if isinstance(value, bytes):
        if not value:
            return "b''"
        try:
            return repr(value.decode("utf-8"))
        except UnicodeDecodeError:
            return f"<bytes len={len(value)} sha256={hashlib.sha256(value).hexdigest()}>"
    if isinstance(value, str):
        return repr(value)
    return json.dumps(value, sort_keys=True)


def body_preview(body: bytes, limit: int = 200) -> str:
    preview = body[:limit]
    try:
        return preview.decode("utf-8")
    except UnicodeDecodeError:
        return preview.hex()


def response_pair_mismatch(
    case_id: str,
    left_label: str,
    left_response: HttpResponse,
    right_label: str,
    right_response: HttpResponse,
    compare_mode: str,
    *,
    expected_status: int | None = None,
    ignore_status: bool = False,
    ignore_content_type: bool = False,
    ignore_body: bool = False,
    allow_framework_validation_mismatch: bool = False,
) -> str | None:
    if ignore_status:
        expected_status = None

    if allow_framework_validation_mismatch and framework_validation_mismatch_allowed(
        left_response.status,
        right_response.status,
        expected_status,
    ):
        return None

    if not ignore_status and left_response.status != right_response.status:
        return (
            f"{case_id}: status mismatch, "
            f"{left_label}={left_response.status}, {right_label}={right_response.status}"
        )

    if not ignore_status and expected_status is not None and left_response.status != expected_status:
        return (
            f"{case_id}: expected status {expected_status}, "
            f"{left_label}={left_response.status}, {right_label}={right_response.status}"
        )

    left_content_type = content_type_prefix(left_response)
    right_content_type = content_type_prefix(right_response)
    if not ignore_content_type and left_content_type != right_content_type:
        return (
            f"{case_id}: content-type mismatch, "
            f"{left_label}={left_content_type!r}, {right_label}={right_content_type!r}"
        )

    if ignore_body:
        return None

    left_body = comparable_body(compare_mode, left_response)
    right_body = comparable_body(compare_mode, right_response)
    if left_body != right_body:
        return "\n".join(
            [
                f"{case_id}: body mismatch",
                f"  {left_label}={render_compared_body(left_body)}",
                f"  {right_label}={render_compared_body(right_body)}",
            ]
        )

    return None


def framework_validation_mismatch_allowed(
    left_status: int,
    right_status: int,
    expected_status: int | None,
) -> bool:
    if (
        left_status in FRAMEWORK_VALIDATION_STATUSES
        and right_status in FRAMEWORK_VALIDATION_STATUSES
    ):
        return True

    if expected_status is None:
        return False

    return (
        left_status in FRAMEWORK_VALIDATION_STATUSES and right_status == expected_status
    ) or (
        right_status in FRAMEWORK_VALIDATION_STATUSES and left_status == expected_status
    )


def comparable_body(compare_mode: str, response: HttpResponse) -> Any:
    if response.status >= 400 and response.body == b"":
        if compare_mode == "json_exact":
            return None
        if compare_mode == "json_shape":
            return json_shape(None)
        if compare_mode == "json_schema":
            return json_schema(None)

    if compare_mode == "exact":
        if is_json_content_type(content_type_prefix(response)):
            try:
                return normalize_json(json.loads(response.body))
            except json.JSONDecodeError:
                return response.body
        return response.body
    if compare_mode == "text_exact":
        return response.body.decode("utf-8", "replace")
    if compare_mode == "json_exact":
        try:
            return normalize_json(json.loads(response.body))
        except json.JSONDecodeError as error:
            raise ParityError(f"expected JSON body, got: {body_preview(response.body)} ({error})")
    if compare_mode == "json_shape":
        try:
            return json_shape(json.loads(response.body))
        except json.JSONDecodeError as error:
            raise ParityError(f"expected JSON body, got: {body_preview(response.body)} ({error})")
    if compare_mode == "json_schema":
        try:
            return json_schema(json.loads(response.body))
        except json.JSONDecodeError as error:
            raise ParityError(f"expected JSON body, got: {body_preview(response.body)} ({error})")
    raise ParityError(f"unsupported compare mode: {compare_mode}")


def json_shape(value: Any) -> Any:
    if isinstance(value, dict):
        return {key: json_shape(value[key]) for key in sorted(value)}
    if isinstance(value, list):
        return [json_shape(item) for item in value]
    if value is None:
        return "null"
    if isinstance(value, bool):
        return "bool"
    if isinstance(value, int):
        return "int"
    if isinstance(value, float):
        return "float"
    if isinstance(value, str):
        return "str"
    return type(value).__name__


def json_schema(value: Any) -> Any:
    if isinstance(value, dict):
        return {key: json_schema(value[key]) for key in sorted(value)}
    if isinstance(value, list):
        unique_items: dict[str, Any] = {}
        for item in value:
            shape = json_schema(item)
            marker = json.dumps(shape, sort_keys=True)
            unique_items.setdefault(marker, shape)
        return [unique_items[key] for key in sorted(unique_items)]
    return json_shape(value)


def content_type_prefix(response: HttpResponse) -> str:
    return response.headers.get("content-type", "").split(";", 1)[0].strip().lower()


def response_cookie(response: HttpResponse) -> str | None:
    set_cookie = response.headers.get("set-cookie")
    if not set_cookie:
        return None
    cookie = set_cookie.split(";", 1)[0].strip()
    return cookie or None


def path_uses_wireless_interface_placeholder(path: str) -> bool:
    return f"/{WIRELESS_INTERFACE_PLACEHOLDER}" in path or f"={WIRELESS_INTERFACE_PLACEHOLDER}" in path


def resolve_wireless_interface_path(path: str, wireless_interface: str | None) -> str:
    if not wireless_interface or wireless_interface == WIRELESS_INTERFACE_PLACEHOLDER:
        return path

    split = urllib.parse.urlsplit(path)
    resolved_path = split.path.replace(
        f"/{WIRELESS_INTERFACE_PLACEHOLDER}/",
        f"/{wireless_interface}/",
    )
    if resolved_path.endswith(f"/{WIRELESS_INTERFACE_PLACEHOLDER}"):
        resolved_path = resolved_path[: -len(WIRELESS_INTERFACE_PLACEHOLDER)] + wireless_interface

    query = []
    for key, value in urllib.parse.parse_qsl(split.query, keep_blank_values=True):
        if key in {"name", "interface"} and value == WIRELESS_INTERFACE_PLACEHOLDER:
            query.append((key, wireless_interface))
        else:
            query.append((key, value))

    resolved_query = urllib.parse.urlencode(query)
    return urllib.parse.urlunsplit(("", "", resolved_path, resolved_query, split.fragment))


def detect_wireless_interface(
    base_url: str,
    *,
    ca_cert_path: Path | None,
    connect_host: str | None = None,
    timeout_seconds: float = DEFAULT_REQUEST_TIMEOUT_SECONDS,
    cookie: str | None = None,
) -> str | None:
    interfaces_response, interfaces_error = request_or_error(
        f"{base_url}/api/v2/network/interfaces",
        "GET",
        cookie=cookie,
        ca_cert_path=ca_cert_path,
        connect_host=connect_host,
        timeout_seconds=timeout_seconds,
        read_body=True,
    )
    if interfaces_error or interfaces_response is None or interfaces_response.status != 200:
        return None
    if not is_json_content_type(content_type_prefix(interfaces_response)):
        return None

    try:
        interface_names = json.loads(interfaces_response.body)
    except json.JSONDecodeError:
        return None
    if not isinstance(interface_names, list):
        return None

    names = [name for name in interface_names if isinstance(name, str)]
    if WIRELESS_INTERFACE_PLACEHOLDER in names:
        return WIRELESS_INTERFACE_PLACEHOLDER

    for name in names:
        detail_response, detail_error = request_or_error(
            f"{base_url}/api/v2/network/interfaces/{urllib.parse.quote(name, safe='')}",
            "GET",
            cookie=cookie,
            ca_cert_path=ca_cert_path,
            connect_host=connect_host,
            timeout_seconds=timeout_seconds,
            read_body=True,
        )
        if detail_error or detail_response is None or detail_response.status != 200:
            continue
        if not is_json_content_type(content_type_prefix(detail_response)):
            continue
        try:
            detail = json.loads(detail_response.body)
        except json.JSONDecodeError:
            continue
        if not isinstance(detail, dict):
            continue
        if isinstance(detail.get("wireless"), dict):
            return name
        status = detail.get("status")
        if isinstance(status, dict):
            device_type = status.get("deviceType")
            device_type_text = str(status.get("deviceTypeText", "")).lower()
            if device_type == 2 or "wireless" in device_type_text or "wifi" in device_type_text:
                return name

    for name in names:
        if name.startswith(("wl", "wlan", "wlp", "wlo")):
            return name

    return None


def compare_responses(
    python_repo: Path,
    cases_path: Path,
    *,
    python_runtime: str,
    legacy_python_repo: Path | None,
    legacy_python_runtime: str | None,
    request_timeout_seconds: float,
    startup_timeout_seconds: float,
    sessions_on: bool,
    requested_plugins: list[str],
) -> None:
    cases = json.loads(cases_path.read_text())
    response_runs = split_cases_by_python_runtime(
        cases,
        primary_runtime=python_runtime,
        legacy_runtime=legacy_python_runtime,
    )
    enabled_plugins = selected_plugins(cases, requested_plugins)

    for run_cases, run_runtime in response_runs:
        run_repo = legacy_python_repo if run_runtime == legacy_python_runtime else python_repo
        if run_repo is None:
            raise ParityError("legacy Python repo must be provided when legacy runtime is configured")
        run_response_cases(
            run_repo,
            run_cases,
            python_runtime=run_runtime,
            request_timeout_seconds=request_timeout_seconds,
            startup_timeout_seconds=startup_timeout_seconds,
            sessions_on=sessions_on,
            enabled_plugins=enabled_plugins,
        )

    if python_runtime != "weblcm":
        run_provisioning_tls_parity(
            enabled_plugins,
            startup_timeout_seconds=startup_timeout_seconds,
        )


def auto_compare_responses(
    python_repo: Path,
    *,
    python_runtime: str,
    request_timeout_seconds: float,
    startup_timeout_seconds: float,
    sessions_on: bool,
    requested_plugins: list[str],
) -> None:
    """Scaffold all shared operations in memory and run live parity on them."""
    python_spec, rust_spec = generate_specs(python_repo)
    shared_ops = shared_operation_specs(python_spec, rust_spec)
    cases = [scaffold_case(rust_spec, op) for op in shared_ops]
    enabled_plugins = selected_plugins(cases, requested_plugins)
    noted_cases = len([case for case in cases if case.get("_notes")])
    print(f"Auto-generated {len(cases)} cases from shared operations ({noted_cases} with scaffold notes)")
    run_response_cases(
        python_repo,
        cases,
        python_runtime=python_runtime,
        request_timeout_seconds=request_timeout_seconds,
        startup_timeout_seconds=startup_timeout_seconds,
        sessions_on=sessions_on,
        enabled_plugins=enabled_plugins,
    )
    run_provisioning_tls_parity(
        enabled_plugins,
        startup_timeout_seconds=startup_timeout_seconds,
    )


def run_response_cases(
    python_repo: Path,
    cases: list[dict[str, Any]],
    *,
    python_runtime: str,
    request_timeout_seconds: float,
    startup_timeout_seconds: float,
    sessions_on: bool,
    enabled_plugins: list[str],
) -> None:
    features = rust_parity_features(enabled_plugins)
    rust_binary = ensure_rust_binary(bin_name="summit-rcm", features=features)

    with tempfile.TemporaryDirectory(prefix="api-parity-live-") as temp_dir_raw:
        temp_dir = Path(temp_dir_raw)
        python_stub_dir = write_python_stub_modules(temp_dir, python_runtime)
        rust_port = reserve_local_port()
        python_port = reserve_local_port()
        rust_config_path, python_config_path, settings_path, provisioning_state_path, cert_path = write_test_config(
            temp_dir,
            rust_port,
            python_port,
            sessions_on=sessions_on,
            enabled_plugins=enabled_plugins,
            python_runtime=python_runtime,
        )
        rust_runtime_env = rust_env(
            extra_env={
                "SUMMIT_RCM_BIND": f"127.0.0.1:{rust_port}",
                "SUMMIT_RCM_SERVER_CONF_FILE": str(rust_config_path),
                "SUMMIT_RCM_SETTINGS_FILE": str(settings_path),
                "SUMMIT_RCM_PROVISIONING_STATE_FILE": str(provisioning_state_path),
            },
        )
        python_env = {
            **os.environ,
            "PYTHONPATH": python_parity_path(python_repo, enabled_plugins, python_runtime),
        }
        if python_stub_dir is not None:
            python_env["PYTHONPATH"] = os.pathsep.join([str(python_stub_dir), python_env["PYTHONPATH"]])
        if python_runtime == "weblcm":
            python_env.update(
                {
                    "WEBLCM_PYTHON_SERVER_CONF_FILE": str(python_config_path),
                    "WEBLCM_PYTHON_SETTINGS_FILE": str(settings_path),
                }
            )
        else:
            python_env.update(
                {
                    "DOCS_GENERATION": "True",
                    "SUMMIT_RCM_SERVER_CONF_FILE": str(python_config_path),
                    "SUMMIT_RCM_SETTINGS_FILE": str(settings_path),
                    "SUMMIT_RCM_PROVISIONING_STATE_FILE": str(provisioning_state_path),
                }
            )

        def start_servers() -> tuple[ManagedProcess, ManagedProcess, str, str]:
            rust_process = start_process(
                "rust",
                [str(rust_binary)],
                cwd=ROOT,
                env=rust_runtime_env,
            )
            python_process = start_process(
                "python",
                [python_runtime_executable(python_repo), "-c", python_wrapper_command(python_runtime)],
                cwd=python_repo,
                env=python_env,
            )
            rust_url = wait_for_server(
                "rust",
                f"https://127.0.0.1:{rust_port}",
                rust_process,
                ca_cert_path=cert_path,
                request_timeout_seconds=request_timeout_seconds,
                startup_timeout_seconds=startup_timeout_seconds,
            )
            python_url = wait_for_server(
                "python",
                f"https://127.0.0.1:{python_port}",
                python_process,
                ca_cert_path=cert_path,
                request_timeout_seconds=request_timeout_seconds,
                startup_timeout_seconds=startup_timeout_seconds,
            )
            return rust_process, python_process, rust_url, python_url

        rust: ManagedProcess | None = None
        python: ManagedProcess | None = None

        try:
            rust, python, rust_base_url, python_base_url = start_servers()

            failures: list[str] = []
            rust_cookies: dict[str, str] = {}
            python_cookies: dict[str, str] = {}
            rust_wireless_interface = detect_wireless_interface(
                rust_base_url,
                ca_cert_path=cert_path,
                timeout_seconds=request_timeout_seconds,
            )
            python_wireless_interface = detect_wireless_interface(
                python_base_url,
                ca_cert_path=cert_path,
                timeout_seconds=request_timeout_seconds,
            )

            for case in cases:
                rust_case_path = resolve_wireless_interface_path(case["path"], rust_wireless_interface)
                python_case_path = resolve_wireless_interface_path(case["path"], python_wireless_interface)
                if case.get("skip_live"):
                    reason = case.get("skip_reason")
                    suffix = f" ({reason})" if reason else ""
                    print(f"SKIP {case['id']}: {case['method']} {rust_case_path}{suffix}")
                    continue

                cookie_key = case.get("use_cookie_from")
                read_body = should_read_body(case)
                case_timeout_seconds = float(case.get("timeout_seconds", request_timeout_seconds))
                readback_case = None
                rust_readback_before: HttpResponse | None = None
                python_readback_before: HttpResponse | None = None

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
                            cookie=rust_cookies.get(readback_cookie_key)
                            if readback_cookie_key
                            else None,
                            ca_cert_path=cert_path,
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
                    f"{rust_base_url}{rust_case_path}",
                    case["method"],
                    body=case.get("body"),
                    multipart=case.get("multipart"),
                    cookie=rust_cookies.get(cookie_key) if cookie_key else None,
                    ca_cert_path=cert_path,
                    timeout_seconds=case_timeout_seconds,
                    read_body=read_body,
                )
                python_response, python_error = request_or_error(
                    f"{python_base_url}{python_case_path}",
                    case["method"],
                    body=case.get("body"),
                    multipart=case.get("multipart"),
                    cookie=python_cookies.get(cookie_key) if cookie_key else None,
                    ca_cert_path=cert_path,
                    timeout_seconds=case_timeout_seconds,
                    read_body=read_body,
                )

                if rust_error or python_error:
                    failures.append(
                        f"{case['id']}: request failed, rust={rust_error or 'ok'}, python={python_error or 'ok'}"
                    )
                    error_messages = [message.lower() for message in (rust_error, python_error) if message]
                    timeout_like_failure = any("timed out" in message for message in error_messages)
                    cookie_free_case = (
                        not rust_cookies
                        and not python_cookies
                        and not cookie_key
                        and not case.get("store_cookie_as")
                    )
                    if timeout_like_failure and cookie_free_case:
                        rust.stop()
                        python.stop()
                        rust, python, rust_base_url, python_base_url = start_servers()
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

                expected_status = case.get("expected_status")
                mismatch = response_pair_mismatch(
                    case["id"],
                    "rust",
                    rust_response,
                    "python",
                    python_response,
                    case["compare"],
                    expected_status=expected_status,
                    ignore_status=bool(case.get("ignore_status")),
                    ignore_content_type=bool(case.get("ignore_content_type")),
                    ignore_body=bool(case.get("ignore_body")),
                    allow_framework_validation_mismatch=bool(
                        case.get("allow_framework_validation_mismatch")
                    ),
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
                        multipart=readback_case.get("multipart"),
                        cookie=rust_cookies.get(readback_cookie_key)
                        if readback_cookie_key
                        else None,
                        ca_cert_path=cert_path,
                        timeout_seconds=readback_timeout_seconds,
                        read_body=readback_read_body,
                    )
                    python_readback_after, python_readback_after_error = request_or_error(
                        f"{python_base_url}{python_readback_path}",
                        readback_case["method"],
                        body=readback_case.get("body"),
                        multipart=readback_case.get("multipart"),
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
                        ignore_status=bool(
                            readback_case.get("ignore_status") or case.get("ignore_status")
                        ),
                        ignore_content_type=bool(
                            readback_case.get("ignore_content_type")
                            or case.get("ignore_content_type")
                        ),
                        ignore_body=bool(
                            readback_case.get("ignore_body") or case.get("ignore_body")
                        ),
                        allow_framework_validation_mismatch=bool(
                            readback_case.get("allow_framework_validation_mismatch")
                            or case.get("allow_framework_validation_mismatch")
                        ),
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
                    continue

                print(f"PASS {case['id']}: {case['method']} {rust_case_path}")

            if failures:
                raise ParityError("Representative response parity failed:\n" + "\n".join(failures))

            print("Representative response parity passed.")
        finally:
            if rust is not None:
                rust.stop()
            if python is not None:
                python.stop()


def run_provisioning_tls_parity(enabled_plugins: list[str], *, startup_timeout_seconds: float) -> None:
    tls_plugins = sorted({*enabled_plugins, "provisioning"})

    try:
        provisioning_tls_check.run_checks(
            startup_timeout_seconds=startup_timeout_seconds,
            features=rust_parity_features(tls_plugins),
        )
    except provisioning_tls_check.CheckError as error:
        raise ParityError(f"Provisioning TLS parity failed:\n{error}") from error

    print("Provisioning TLS parity passed.")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Compare Rust and Python Summit RCM API parity.")
    parser.add_argument(
        "mode",
        choices=["openapi", "responses", "coverage", "scaffold", "schema", "auto", "provisioning_tls", "all"],
        help="Which parity checks to run.",
    )
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
        help="Python baseline to use: auto-detect summit-rcm or weblcm. weblcm supports only live HTTP parity modes.",
    )
    parser.add_argument(
        "--legacy-python-repo",
        type=Path,
        help="Legacy-only Python baseline repo. For mixed legacy and /api/v2/ response manifests, pass this explicitly so legacy cases do not silently run against --python-repo.",
    )
    parser.add_argument(
        "--legacy-python-runtime",
        choices=PYTHON_RUNTIME_CHOICES,
        default="auto",
        help="Runtime for --legacy-python-repo. Use with responses mode to split legacy and v2 baselines.",
    )
    parser.add_argument(
        "--cases",
        type=Path,
        default=DEFAULT_CASES,
        help="Path to the representative response case manifest.",
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
        help="Enable session middleware in the temporary parity config for auth flow checks.",
    )
    parser.add_argument(
        "--plugins",
        action="append",
        default=[],
        help="Comma-separated plugin names to enable for live response parity checks.",
    )
    parser.add_argument(
        "--scaffold-output",
        type=Path,
        help="Write uncovered scaffold cases to this JSON file instead of stdout.",
    )
    parser.add_argument(
        "--schema-output",
        type=Path,
        help="Write schema diff to this file instead of stdout (used with 'schema' mode).",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    requested_plugins = parse_plugin_names(args.plugins)
    python_repo = args.python_repo.resolve()
    cases_path = args.cases.resolve()

    try:
        python_runtime: str | None = None
        legacy_python_repo: Path | None = None
        legacy_python_runtime: str | None = None
        if args.mode != "provisioning_tls":
            python_runtime = resolve_python_runtime(python_repo, args.python_runtime)
            ensure_mode_supported(args.mode, python_runtime)
            if args.legacy_python_repo is not None:
                if args.mode not in {"responses", "all"}:
                    raise ParityError("--legacy-python-repo is supported only for responses and all modes")
                legacy_python_repo = args.legacy_python_repo.resolve()
                legacy_python_runtime = resolve_python_runtime(
                    legacy_python_repo, args.legacy_python_runtime
                )
                ensure_mode_supported("responses", legacy_python_runtime)
            require_explicit_legacy_baseline(
                mode=args.mode,
                cases_path=cases_path,
                python_runtime=python_runtime,
                legacy_python_repo=legacy_python_repo,
            )

        if args.mode in {"openapi", "all"}:
            compare_openapi(python_repo)
        if args.mode in {"responses", "all"}:
            compare_responses(
                python_repo,
                cases_path,
                python_runtime=python_runtime or "summit-rcm",
                legacy_python_repo=legacy_python_repo,
                legacy_python_runtime=legacy_python_runtime,
                request_timeout_seconds=args.request_timeout_seconds,
                startup_timeout_seconds=args.startup_timeout_seconds,
                sessions_on=args.sessions_on,
                requested_plugins=requested_plugins,
            )
        if args.mode == "auto":
            auto_compare_responses(
                python_repo,
                python_runtime=python_runtime or "summit-rcm",
                request_timeout_seconds=args.request_timeout_seconds,
                startup_timeout_seconds=args.startup_timeout_seconds,
                sessions_on=args.sessions_on,
                requested_plugins=requested_plugins,
            )
        if args.mode == "provisioning_tls":
            run_provisioning_tls_parity(
                requested_plugins,
                startup_timeout_seconds=args.startup_timeout_seconds,
            )
        if args.mode in {"coverage", "all"}:
            report_response_coverage(python_repo, cases_path)
        if args.mode == "scaffold":
            scaffold_uncovered_cases(
                python_repo,
                cases_path,
                output_path=args.scaffold_output.resolve() if args.scaffold_output else None,
            )
        if args.mode == "schema":
            diff_schemas(
                python_repo,
                output_path=args.schema_output.resolve() if args.schema_output else None,
            )
    except ParityError as error:
        print(str(error), file=sys.stderr)
        return 1

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
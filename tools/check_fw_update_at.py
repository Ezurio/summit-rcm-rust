#!/usr/bin/env python3

# SPDX-License-Identifier: LicenseRef-Ezurio-Clause
# Copyright (C) 2026 Ezurio LLC.

from __future__ import annotations

import argparse
import errno
import os
import re
import select
import sys
import termios
import time
import tty
from pathlib import Path


DEFAULT_BAUD_RATE = 115200
DEFAULT_COMMAND_TIMEOUT_SECONDS = 30.0
DEFAULT_READY_TIMEOUT_SECONDS = 20.0
DEFAULT_STATUS_POLLS = 3
DEFAULT_STATUS_INTERVAL_SECONDS = 2.0
TERMINAL_STATUS_LINES = {"OK", "ERROR"}


class CheckError(RuntimeError):
    pass


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Send a firmware image over the live serial AT interface using "
            "AT+FWSENDDIRECT. This performs a real update on the target."
        )
    )
    parser.add_argument("--serial-path", required=True, help="Serial device path, for example /dev/ttyUSB5")
    parser.add_argument("--image-path", required=True, help="Path to the .swu image to upload")
    parser.add_argument(
        "--image",
        default="complete",
        help="Image selector passed to AT+FWSENDDIRECT, default: complete",
    )
    parser.add_argument(
        "--baud-rate",
        type=int,
        default=DEFAULT_BAUD_RATE,
        help=f"Serial baud rate, default: {DEFAULT_BAUD_RATE}",
    )
    parser.add_argument(
        "--command-timeout-seconds",
        type=float,
        default=DEFAULT_COMMAND_TIMEOUT_SECONDS,
        help=f"Timeout for command responses, default: {DEFAULT_COMMAND_TIMEOUT_SECONDS}",
    )
    parser.add_argument(
        "--ready-timeout-seconds",
        type=float,
        default=DEFAULT_READY_TIMEOUT_SECONDS,
        help=f"Timeout when waiting for READY, default: {DEFAULT_READY_TIMEOUT_SECONDS}",
    )
    parser.add_argument(
        "--wait-for-ready-banner",
        action="store_true",
        help="Wait for an initial READY banner before sending commands",
    )
    parser.add_argument(
        "--status-polls",
        type=int,
        default=DEFAULT_STATUS_POLLS,
        help=f"Number of AT+FWSTATUS polls after upload, default: {DEFAULT_STATUS_POLLS}",
    )
    parser.add_argument(
        "--status-interval-seconds",
        type=float,
        default=DEFAULT_STATUS_INTERVAL_SECONDS,
        help=(
            "Delay between AT+FWSTATUS polls, default: "
            f"{DEFAULT_STATUS_INTERVAL_SECONDS}"
        ),
    )
    return parser.parse_args()


def open_serial_device(serial_path: str, *, baud_rate: int) -> int:
    try:
        fd = os.open(serial_path, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
    except OSError as error:
        raise CheckError(f"failed to open serial device {serial_path}: {error}") from error

    try:
        attrs = termios.tcgetattr(fd)
        baud = getattr(termios, f"B{baud_rate}", None)
        if baud is None:
            raise CheckError(f"unsupported baud rate for termios: {baud_rate}")
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
    except Exception:
        os.close(fd)
        raise

    return fd


def read_serial(fd: int) -> bytes:
    try:
        return os.read(fd, 4096)
    except OSError as error:
        if error.errno in {errno.EAGAIN, errno.EWOULDBLOCK, errno.EIO}:
            return b""
        raise


def flush_serial_input(fd: int, *, drain_seconds: float = 0.25) -> None:
    deadline = time.time() + drain_seconds
    while time.time() < deadline:
        ready, _, _ = select.select([fd], [], [], 0.05)
        if not ready:
            continue
        data = read_serial(fd)
        if not data:
            break


def normalize_serial_text(text: str) -> str:
    normalized = text.replace("\r\n", "\n").replace("\r", "\n")
    lines = normalized.split("\n")
    while lines and lines[0] == "":
        lines.pop(0)
    while lines and lines[-1] == "":
        lines.pop()
    return "\n".join(lines)


def sanitize_serial_text(text: str) -> str:
    return "".join(ch for ch in text if ch in {"\n", "\r", "\t"} or (" " <= ch <= "~"))


def has_terminal_status(response_text: str) -> bool:
    lines = [
        line.strip()
        for line in normalize_serial_text(sanitize_serial_text(response_text)).split("\n")
        if line.strip()
    ]
    return any(line in TERMINAL_STATUS_LINES for line in lines)


def read_until(fd: int, *, timeout_seconds: float, done: callable) -> str:
    deadline = time.time() + timeout_seconds
    chunks: list[bytes] = []
    while time.time() < deadline:
        ready, _, _ = select.select([fd], [], [], max(0.0, deadline - time.time()))
        if not ready:
            continue
        data = read_serial(fd)
        if not data:
            continue
        chunks.append(data)
        response_text = b"".join(chunks).decode("utf-8", "replace")
        if done(response_text):
            return normalize_serial_text(sanitize_serial_text(response_text))
    transcript = normalize_serial_text(sanitize_serial_text(b"".join(chunks).decode("utf-8", "replace")))
    raise CheckError(f"timed out waiting for serial response after {timeout_seconds:.2f}s: {transcript!r}")


def wait_for_ready(fd: int, *, timeout_seconds: float) -> str:
    return read_until(
        fd,
        timeout_seconds=timeout_seconds,
        done=lambda response: "READY" in normalize_serial_text(sanitize_serial_text(response)).split("\n"),
    )


def send_command(fd: int, command: str, *, timeout_seconds: float) -> str:
    flush_serial_input(fd)
    os.write(fd, f"{command}\r".encode())
    return read_until(fd, timeout_seconds=timeout_seconds, done=has_terminal_status)


def send_command_for_prompt(fd: int, command: str, *, timeout_seconds: float) -> str:
    flush_serial_input(fd)
    os.write(fd, f"{command}\r".encode())
    return read_until(
        fd,
        timeout_seconds=timeout_seconds,
        done=lambda response: re.search(r"(?:\r\n|\n|^)>", sanitize_serial_text(response)) is not None,
    )


def write_all(fd: int, data: bytes) -> None:
    view = memoryview(data)
    while view:
        _, writable, _ = select.select([], [fd], [], 30.0)
        if not writable:
            raise CheckError("timed out waiting for serial port to accept upload data")
        try:
            written = os.write(fd, view)
        except OSError as error:
            if error.errno in {errno.EAGAIN, errno.EWOULDBLOCK}:
                continue
            raise
        if written <= 0:
            raise CheckError("serial port write returned no progress during upload")
        view = view[written:]


def send_payload(fd: int, image_path: Path, *, timeout_seconds: float) -> str:
    with image_path.open("rb") as handle:
        while True:
            chunk = handle.read(65536)
            if not chunk:
                break
            write_all(fd, chunk)
    return read_until(fd, timeout_seconds=timeout_seconds, done=has_terminal_status)


def print_transcript(label: str, transcript: str) -> None:
    print(f"[{label}]")
    print(transcript or "<empty>")


def main() -> int:
    args = parse_args()
    image_path = Path(args.image_path)
    if not image_path.is_file():
        raise CheckError(f"image path does not exist: {image_path}")

    fd = open_serial_device(args.serial_path, baud_rate=args.baud_rate)
    try:
        if args.wait_for_ready_banner:
            ready = wait_for_ready(fd, timeout_seconds=args.ready_timeout_seconds)
            print_transcript("READY", ready)

        ate0 = send_command(fd, "ATE0", timeout_seconds=args.command_timeout_seconds)
        print_transcript("ATE0", ate0)

        pre_status = send_command(fd, "AT+FWSTATUS", timeout_seconds=args.command_timeout_seconds)
        print_transcript("FWSTATUS before", pre_status)

        command = f"AT+FWSENDDIRECT={image_path.stat().st_size},{args.image}"
        prompt = send_command_for_prompt(fd, command, timeout_seconds=args.command_timeout_seconds)
        print_transcript("FWSENDDIRECT prompt", prompt)

        upload = send_payload(fd, image_path, timeout_seconds=args.command_timeout_seconds)
        print_transcript("FWSENDDIRECT result", upload)

        expected = f"+FWSENDDIRECT: {image_path.stat().st_size}"
        if expected not in upload or "OK" not in upload.split("\n"):
            raise CheckError(f"unexpected upload response, wanted {expected!r} and terminal OK")

        for index in range(args.status_polls):
            if index:
                time.sleep(args.status_interval_seconds)
            status = send_command(fd, "AT+FWSTATUS", timeout_seconds=args.command_timeout_seconds)
            print_transcript(f"FWSTATUS after #{index + 1}", status)

        return 0
    finally:
        os.close(fd)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except CheckError as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1)
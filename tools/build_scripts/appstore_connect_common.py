#!/usr/bin/env python3
"""Shared App Store Connect command helpers for local and automated release tooling."""
from __future__ import annotations

import os
from pathlib import Path

from common import REPO_ROOT, require_command, run

DEFAULT_ENV_PATH = REPO_ROOT / "tools" / "release" / "secrets" / "appstoreconnect" / "appstoreconnect.env"


def load_env(path: Path = DEFAULT_ENV_PATH) -> dict[str, str]:
    """Load the private App Store Connect dotenv file without printing secrets."""
    if not path.is_file():
        raise RuntimeError(f"App Store Connect env file not found: {path}")
    values: dict[str, str] = {}
    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        values[key.strip()] = value.strip().strip("\"'")
    return values


def resolve_value(values: dict[str, str], key: str, explicit: str | None = None) -> str:
    value = explicit or os.environ.get(key) or values.get(key)
    if not value:
        raise RuntimeError(f"Missing App Store Connect setting: {key}")
    return value


def resolve_api_key_path(values: dict[str, str], explicit: str | None = None) -> Path:
    raw_path = resolve_value(values, "APP_STORE_CONNECT_API_KEY_PATH", explicit)
    path = Path(raw_path)
    if not path.is_absolute():
        path = REPO_ROOT / path
    if not path.is_file():
        raise RuntimeError(f"App Store Connect API key file not found: {path}")
    return path


def altool_command(
    operation: str,
    package: Path,
    values: dict[str, str],
) -> list[str | Path]:
    altool = require_command("xcrun")
    key_id = resolve_value(values, "APP_STORE_CONNECT_API_KEY_ID")
    issuer_id = resolve_value(values, "APP_STORE_CONNECT_API_ISSUER_ID")
    key_path = resolve_api_key_path(values)
    return [
        altool,
        "altool",
        operation,
        str(package),
        "--api-key",
        key_id,
        "--api-issuer",
        issuer_id,
        "--p8-file-path",
        str(key_path),
    ]


def run_altool(operation: str, package: Path, values: dict[str, str], wait: bool = False) -> None:
    command = altool_command(operation, package, values)
    if operation == "--upload-package" and wait:
        command.append("--wait")
    run(command)

#!/usr/bin/env python3
"""Validate an iOS or macOS package with App Store Connect tooling."""
from __future__ import annotations

import argparse
from pathlib import Path

from appstore_connect_common import DEFAULT_ENV_PATH, load_env, run_altool


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate an Apple package for App Store Connect.")
    parser.add_argument("package", type=Path, help="Path to an .ipa or .pkg file")
    parser.add_argument("--env-file", type=Path, default=DEFAULT_ENV_PATH)
    args = parser.parse_args()
    package = args.package.resolve()
    if not package.is_file():
        raise RuntimeError(f"Package not found: {package}")
    run_altool("--validate-app", package, load_env(args.env_file))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

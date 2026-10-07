from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import zipfile
import urllib.parse
import urllib.request
from dataclasses import dataclass
from pathlib import Path

MANIFEST_FILENAMES = ("manifest.hjson", "manifest.json")
SYNCABLE_SUFFIXES = {".js", ".toolpkg"}
HOT_RELOAD_STATE_FILE = ".sync_hot_reload_state.json"
XCODE_CROSS_ENVIRONMENT_KEYS = {
    "ARCHS",
    "AR",
    "CC",
    "CFLAGS",
    "CPP",
    "CPPFLAGS",
    "CURRENT_ARCH",
    "CXX",
    "CXXFLAGS",
    "EFFECTIVE_PLATFORM_NAME",
    "IPHONEOS_DEPLOYMENT_TARGET",
    "LDFLAGS",
    "NATIVE_ARCH",
    "PLATFORM_NAME",
    "RANLIB",
    "SDKROOT",
    "TARGET_BUILD_DIR",
    "VALID_ARCHS",
}
@dataclass(frozen=True)
class SyncPlanItem:
    mode: str
    source: Path
    destination_name: str


def _repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def _plugins_root() -> Path:
    return Path(__file__).resolve().parents[1]


def _plugin_packages_root() -> Path:
    return _plugins_root() / "packages"


def _find_manifest_file(folder: Path) -> Path | None:
    for file_name in MANIFEST_FILENAMES:
        manifest = folder / file_name
        if manifest.is_file():
            return manifest
    return None


# Validates the package build contract for a script-managed ToolPkg.
def _is_script_packed_toolpkg(folder: Path) -> bool:
    package_json = folder / "package.json"
    if not package_json.is_file():
        return False
    package_data = json.loads(package_json.read_text(encoding="utf-8"))
    if not isinstance(package_data, dict):
        raise ValueError(f"package.json must contain a JSON object: {package_json}")
    scripts = package_data.get("scripts")
    if not isinstance(scripts, dict):
        raise ValueError(f"package.json must define scripts: {package_json}")
    pack_script = scripts.get("pack:toolpkg")
    if not isinstance(pack_script, str) or not pack_script.strip():
        raise ValueError(f"package.json must define scripts.pack:toolpkg: {package_json}")
    return True


# Collects the output operations required for one plugin source directory.
def _collect_sync_plan(source_dir: Path) -> list[SyncPlanItem]:
    plans: list[SyncPlanItem] = []
    if not source_dir.is_dir():
        return plans

    for child in sorted(source_dir.iterdir(), key=lambda path: path.name.lower()):
        if child.name in {"types", "node_modules"}:
            continue
        if child.is_file() and child.suffix.lower() in SYNCABLE_SUFFIXES:
            plans.append(
                SyncPlanItem(
                    mode="copy",
                    source=child,
                    destination_name=child.name,
                )
            )
            continue
        if child.is_file() and child.suffix.lower() == ".ts" and not child.name.endswith(".d.ts"):
            plans.append(
                SyncPlanItem(
                    mode="compile-ts",
                    source=child,
                    destination_name=f"{child.stem}.js",
                )
            )
            continue
        if child.is_dir() and _find_manifest_file(child):
            plans.append(
                SyncPlanItem(
                    mode="copy-script-toolpkg" if _is_script_packed_toolpkg(child) else "pack",
                    source=child,
                    destination_name=f"{child.name}.toolpkg",
                )
            )
    return plans


# Runs one command and reports the exact command line on failure. The child's
# stderr is captured and printed when the command fails, so failures carry the
# actual tool output (pnpm version checks, compile errors) instead of a bare
# exit code.
def _run_checked_command(
    command: list[str],
    cwd: Path,
    *,
    dry_run: bool,
    env: dict[str, str] | None = None,
) -> None:
    command_text = subprocess.list2cmdline(command)
    if dry_run:
        print(f"DRY-RUN-CMD: (cd {cwd}) {command_text}")
        return
    print(f"RUN-CMD: (cd {cwd}) {command_text}")
    completed = subprocess.run(
        command,
        cwd=str(cwd),
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        errors="replace",
    )
    stdout_tail = (completed.stdout or "").strip()
    stderr_tail = (completed.stderr or "").strip()
    if completed.returncode != 0:
        if stdout_tail:
            print(f"stdout: {stdout_tail[-4000:]}", flush=True)
        if stderr_tail:
            print(f"stderr: {stderr_tail[-4000:]}", flush=True)
        raise RuntimeError(f"Command failed with exit code {completed.returncode}: {command_text}")
    if stdout_tail or stderr_tail:
        print(f"output: {(stdout_tail + chr(10) + stderr_tail).strip()[-1500:]}", flush=True)


# Reports the rust related environment and pins it onto a real toolchain.
#
# IDE hosts, MSBuild and Xcode's build service hand us a polluted rust
# environment: PATH carries a stale rustup proxy `cargo` (sometimes symlinked
# into Homebrew's Cellar) while neither a `rustc` proxy nor a toolchain bin
# directory is reachable, and CARGO_HOME points at a Homebrew prefix. The rustup
# proxy then starts the real cargo, which runs `rustc -vV` and dies with
# `could not execute process 'rustc -vV' (never executed)` because there is no
# `rustc` to execute at all.
#
# _apply_rustup_proxy_environment only fills in *missing* variables, which can
# never repair that state. This function instead resolves the toolchain that
# actually owns a `cargo` + `rustc` pair and forces RUSTUP_HOME,
# RUSTUP_TOOLCHAIN, RUSTC and the toolchain bin directory (prepended to PATH),
# so cargo no longer depends on the caller's PATH to locate rustc.
def _repair_rust_environment(environment: dict[str, str]) -> None:
    def _log(message: str) -> None:
        print(f"[rust-env] {message}", file=sys.stderr, flush=True)

    path_value = environment.get("PATH", "")
    _log(f"platform={sys.platform}")
    _log(f"CARGO_HOME={environment.get('CARGO_HOME')!r}")
    _log(f"RUSTUP_HOME={environment.get('RUSTUP_HOME')!r}")
    _log(f"RUSTUP_TOOLCHAIN={environment.get('RUSTUP_TOOLCHAIN')!r}")
    _log(f"RUSTC={environment.get('RUSTC')!r}")
    _log(f"which(cargo)={shutil.which('cargo')!r}")
    _log(f"which(rustc)={shutil.which('rustc')!r}")
    for directory in _rust_bin_directories(environment):
        try:
            entries = sorted(os.listdir(directory))
        except OSError as error:
            _log(f"ls({directory})=<unreadable: {error}>")
            continue
        _log(f"ls({directory})={entries}")
    _log(f"PATH={path_value}")

    resolved = _resolve_rust_toolchain(environment)
    if resolved is None:
        _log("no rustup toolchain carrying cargo + rustc, environment unchanged")
        return

    rustup_home, toolchain, bin_directory = resolved
    environment["RUSTUP_HOME"] = rustup_home
    environment["RUSTUP_TOOLCHAIN"] = toolchain
    environment["RUSTC"] = os.path.join(bin_directory, "rustc")
    entries = [
        entry
        for entry in path_value.split(os.pathsep)
        if entry and entry != bin_directory
    ]
    environment["PATH"] = os.pathsep.join([bin_directory, *entries])
    _log(f"repair: RUSTUP_HOME={rustup_home}")
    _log(f"repair: RUSTUP_TOOLCHAIN={toolchain}")
    _log(f"repair: RUSTC={environment['RUSTC']}")
    _log(f"repair: PATH<-{bin_directory}")
    _probe_rust_tools(environment, bin_directory)


# Runs the resolved tools once so a broken toolchain shows up in the build log.
def _probe_rust_tools(environment: dict[str, str], bin_directory: str) -> None:
    def _log(message: str) -> None:
        print(f"[rust-env] {message}", file=sys.stderr, flush=True)

    for tool in ("rustc", "cargo"):
        executable = os.path.join(bin_directory, tool)
        try:
            completed = subprocess.run(
                [executable, "-vV" if tool == "rustc" else "-V"],
                env=environment,
                capture_output=True,
                text=True,
                timeout=180,
                check=False,
            )
        except (OSError, subprocess.SubprocessError) as error:
            _log(f"probe {tool} failed: {error}")
            continue
        stdout = completed.stdout.strip().splitlines()
        first_line = stdout[0] if stdout else ""
        _log(
            f"probe {tool}: rc={completed.returncode} "
            f"out={first_line!r} "
            f"err={completed.stderr.strip()[:200]!r}"
        )


# Lists the rust bin directories worth reporting while diagnosing.
def _rust_bin_directories(environment: dict[str, str]) -> list[str]:
    found: list[str] = []
    for location in (shutil.which("cargo"), environment.get("CARGO_HOME")):
        if not location:
            continue
        entry = Path(location)
        if entry.name in {"cargo", "cargo.exe", "cargo.cmd", "cargo.bat"}:
            directory = entry.parent
        else:
            directory = entry / "bin"
        if str(directory) not in found:
            found.append(str(directory))
    for rustup_home in _rustup_home_candidates(environment):
        toolchains = Path(rustup_home) / "toolchains"
        try:
            names = sorted(os.listdir(toolchains))
        except OSError:
            continue
        for name in names:
            directory = str(toolchains / name / "bin")
            if directory not in found:
                found.append(directory)
    return found


# Lists the rustup homes that could own a toolchain, most likely first.
def _rustup_home_candidates(environment: dict[str, str]) -> list[str]:
    locations: list[Path | None] = [
        _path_or_none(environment.get("RUSTUP_HOME")),
        Path.home() / ".rustup",
    ]
    cargo = shutil.which("cargo")
    if cargo:
        for parent in Path(cargo).resolve().parents:
            locations.append(parent / "rustup")
            locations.append(parent / ".rustup")
    candidates: list[str] = []
    for location in locations:
        if location is None:
            continue
        resolved = str(location)
        if resolved in candidates or not location.is_dir():
            continue
        candidates.append(resolved)
    return candidates


def _path_or_none(value: str | None) -> Path | None:
    return Path(value) if value else None


# Resolves (rustup_home, toolchain, bin_directory) for a usable toolchain.
def _resolve_rust_toolchain(
    environment: dict[str, str],
) -> tuple[str, str, str] | None:
    for rustup_home in _rustup_home_candidates(environment):
        toolchains = Path(rustup_home) / "toolchains"
        try:
            names = sorted(os.listdir(toolchains))
        except OSError:
            continue
        if not names:
            continue
        preferred = environment.get("RUSTUP_TOOLCHAIN") or _rustup_default_toolchain(
            Path(rustup_home)
        )
        ordered = [name for name in names if name == preferred]
        ordered.extend(name for name in names if name != preferred)
        for name in ordered:
            bin_directory = toolchains / name / "bin"
            if (bin_directory / "cargo").is_file() and (
                bin_directory / "rustc"
            ).is_file():
                return rustup_home, name, str(bin_directory)
    return None


# Builds the environment used for host-only Cargo code generators.
def _host_cargo_environment() -> dict[str, str]:
    environment = os.environ.copy()
    for key in XCODE_CROSS_ENVIRONMENT_KEYS:
        environment.pop(key, None)
    if sys.platform == "darwin":
        completed = subprocess.run(
            ["xcrun", "--sdk", "macosx", "--show-sdk-path"],
            check=True,
            stdout=subprocess.PIPE,
            text=True,
        )
        environment["SDKROOT"] = completed.stdout.strip()
    _apply_rustup_proxy_environment(environment)
    _repair_rust_environment(environment)
    return environment


# Fills rustup proxy variables when MSBuild or IDE builds omit the user env.
def _apply_rustup_proxy_environment(environment: dict[str, str]) -> None:
    if environment.get("RUSTUP_HOME") and environment.get("CARGO_HOME"):
        return
    derived: dict[str, str] = {}
    cargo = shutil.which("cargo")
    if cargo is not None:
        cargo_path = Path(cargo)
        derived.update(_environment_from_cargo_cmd(cargo_path))
        derived.update(_rustup_proxy_environment_from_cargo(cargo_path))
    if "RUSTUP_HOME" not in derived or "CARGO_HOME" not in derived:
        derived.update(_rustup_proxy_environment_from_path(environment.get("PATH", os.environ.get("PATH", ""))))
    for key, value in derived.items():
        if not environment.get(key):
            environment[key] = value


# Reads rustup variables assigned by a cargo.cmd wrapper.
def _environment_from_cargo_cmd(cargo: Path) -> dict[str, str]:
    if cargo.suffix.lower() not in {".cmd", ".bat"}:
        sibling = cargo.with_name("cargo.cmd")
        if not sibling.is_file():
            sibling = cargo.with_name("cargo.bat")
        cargo = sibling
    if not cargo.is_file():
        return {}
    derived: dict[str, str] = {}
    dp0 = str(cargo.parent) + os.sep
    try:
        lines = cargo.read_text(encoding="utf-8", errors="ignore").splitlines()
    except OSError:
        return {}
    for line in lines:
        stripped = line.strip()
        if not stripped.lower().startswith("set "):
            continue
        assignment = stripped[4:].strip().strip('"')
        if "=" not in assignment:
            continue
        key, value = assignment.split("=", 1)
        key = key.strip()
        if key not in {"CARGO_HOME", "RUSTUP_HOME", "RUSTUP_TOOLCHAIN"}:
            continue
        derived[key] = value.replace("%~dp0", dp0).replace("%~DP0", dp0)
    return derived


# Walks PATH for a rustup-proxy cargo.exe when the first cargo is a wrapper.
def _rustup_proxy_environment_from_path(path_value: str) -> dict[str, str]:
    names = ("cargo.exe", "cargo")
    for directory in path_value.split(os.pathsep):
        if not directory:
            continue
        for name in names:
            candidate = Path(directory) / name
            derived = _rustup_proxy_environment_from_cargo(candidate)
            if derived.get("RUSTUP_HOME") and derived.get("CARGO_HOME"):
                return derived
    return {}


# Derives CARGO_HOME/RUSTUP_HOME from a rustup-proxy cargo executable.
def _rustup_proxy_environment_from_cargo(cargo: Path) -> dict[str, str]:
    if not cargo.exists():
        return {}
    bin_dir = cargo.resolve().parent
    rustup_names = ("rustup.exe", "rustup")
    if not any((bin_dir / name).exists() for name in rustup_names):
        return {}
    cargo_home = bin_dir.parent
    root = cargo_home.parent
    derived = {"CARGO_HOME": str(cargo_home)}
    for rustup_home in (root / "rustup", root / ".rustup"):
        if rustup_home.is_dir():
            derived["RUSTUP_HOME"] = str(rustup_home)
            toolchain = _rustup_default_toolchain(rustup_home)
            if toolchain:
                derived["RUSTUP_TOOLCHAIN"] = toolchain
            break
    return derived


# Reads the default toolchain pinned by a rustup home.
def _rustup_default_toolchain(rustup_home: Path) -> str | None:
    settings = rustup_home / "settings.toml"
    if not settings.is_file():
        return None
    for line in settings.read_text(encoding="utf-8").splitlines():
        stripped = line.strip()
        if not stripped.startswith("default_toolchain"):
            continue
        _, _, value = stripped.partition("=")
        toolchain = value.strip().strip('"').strip("'")
        return toolchain or None
    return None


# Resolves host tools through the configured toolchain before handing them to subprocess.
def _platform_command(executable: str) -> str:
    candidates = [executable]
    if os.name == "nt":
        # The repository-managed Windows toolchain ships a cargo.cmd wrapper
        # which pins the rustup toolchain. Prefer it over cargo.exe, because
        # cargo.exe is a rustup proxy and otherwise depends on the caller's
        # default-toolchain state (which is often stale inside VS Code).
        cargo_home = os.environ.get("CARGO_HOME", "").strip()
        if not cargo_home:
            derived: dict[str, str] = {}
            _apply_rustup_proxy_environment(derived)
            cargo_home = derived.get("CARGO_HOME", "").strip()
        if cargo_home:
            configured_wrapper = Path(cargo_home) / "bin" / f"{executable}.cmd"
            if configured_wrapper.is_file():
                return str(configured_wrapper)
        candidates.insert(0, f"{executable}.cmd")

    for candidate in candidates:
        resolved = shutil.which(candidate)
        if resolved is not None:
            return resolved
    if executable in {"cargo", "rustc", "rustdoc"}:
        toolchain = _resolve_rust_toolchain(os.environ)
        if toolchain is not None:
            toolchain_executable = os.path.join(toolchain[2], executable)
            if os.path.isfile(toolchain_executable):
                return toolchain_executable
    raise FileNotFoundError(
        f"Required executable is not available on PATH: {', '.join(candidates)}"
    )


def _typescript_command(repo_root: Path, *, dry_run: bool) -> str:
    """Resolves the repository-managed TypeScript compiler before PATH."""
    executable = "tsc.cmd" if os.name == "nt" else "tsc"
    repository_executable = (
        repo_root / ".ci-tools" / "typescript" / "node_modules" / ".bin" / executable
    )
    if repository_executable.is_file():
        return str(repository_executable)

    resolved_executable = shutil.which(executable)
    if resolved_executable is not None:
        return resolved_executable

    if dry_run:
        return executable

    if _provision_typescript(repo_root) is not None:
        return str(repository_executable)

    raise FileNotFoundError(
        "TypeScript compiler not found. Expected the repository-managed compiler at "
        f"{repository_executable} or '{executable}' on PATH."
    )


# TypeScript major.minor line installed when the repository pins no version.
FALLBACK_TYPESCRIPT_LINE = "5.9"


# Writes one diagnostic line for the plugin sync tooling.
def _log_plugin_sync(message: str) -> None:
    print(f"[plugin-sync] {message}", file=sys.stderr, flush=True)


# Locates node together with npm's CLI entry point.
#
# CI runners that never ran actions/setup-node keep node in the runner tool
# cache, so neither 'node' nor 'npm' is on PATH even though both exist on disk.
def _node_toolchain() -> tuple[str, str, str] | None:
    candidates: list[Path] = []
    for executable in ("node", "npm"):
        resolved = shutil.which(executable)
        if resolved:
            location = Path(resolved)
            candidates.append(location)
            candidates.append(location.resolve())
    for candidate in ("/opt/homebrew/bin/node", "/usr/local/bin/node", "/usr/bin/node"):
        candidates.append(Path(candidate))
    home = Path.home()
    for pattern in (
        "hostedtoolcache/node/*/arm64/bin",
        "hostedtoolcache/node/*/x64/bin",
        "hostedtoolcache/node/*/bin",
        ".nvm/versions/node/*/bin",
        ".fnm/node-versions/*/installation/bin",
        ".local/share/fnm/node-versions/*/installation/bin",
        "Library/Application Support/fnm/node-versions/*/installation/bin",
    ):
        for discovered in sorted(home.glob(pattern), reverse=True):
            candidates.append(discovered / "node")

    searched: list[str] = []
    for candidate in candidates:
        directory = candidate.parent
        if str(directory) in searched:
            continue
        searched.append(str(directory))
        if not candidate.is_file():
            continue
        npm_script = _npm_script_path(directory)
        if npm_script is None:
            continue
        return str(candidate), npm_script, str(directory)
    _log_plugin_sync(
        "node toolchain not found; searched dirs: " + ", ".join(searched)
    )
    _log_node_search_state()
    return None


# Reports where a node installation could have been looked for.
def _log_node_search_state() -> None:
    home = Path.home()
    directories = (
        home / "hostedtoolcache",
        home / "hostedtoolcache" / "node",
        home / ".nvm" / "versions" / "node",
        home / ".fnm" / "node-versions",
        Path("/opt/homebrew/bin"),
        Path("/usr/local/bin"),
    )
    for directory in directories:
        try:
            entries = sorted(os.listdir(directory))
        except OSError as error:
            _log_plugin_sync(f"ls({directory}) failed: {error}")
            continue
        relevant = [
            entry
            for entry in entries
            if any(word in entry for word in ("node", "npm", "corepack", "pnpm"))
        ]
        _log_plugin_sync(f"ls({directory})={relevant[:40]}")


# Finds npm's JavaScript entry point inside a node installation.
def _npm_script_path(directory: Path) -> str | None:
    candidates = (
        directory.parent / "lib" / "node_modules" / "npm" / "bin" / "npm-cli.js",
        directory / "npm-cli.js",
        directory / "npm",
        directory / "npm.cmd",
    )
    for candidate in candidates:
        if candidate.is_file():
            return str(candidate)
    return None


# Prepends the discovered node directory to PATH so every child process
# (tsc, corepack, pnpm, node) becomes resolvable for the rest of the build.
def _ensure_node_on_path() -> Path | None:
    toolchain = _node_toolchain()
    if toolchain is None:
        return None
    directory = Path(toolchain[2])
    entries = os.environ.get("PATH", "").split(os.pathsep)
    if str(directory) not in entries:
        os.environ["PATH"] = os.pathsep.join([str(directory), *entries])
        _log_plugin_sync(f"PATH <- {directory}")
    return directory


# Reads the TypeScript line pinned by the plugin SDK client when present.
#
# Only the major.minor pair is kept: exact pins such as '5.8.0' are not always
# published (TypeScript shipped 5.8.2 as the first 5.8 release) while a caret
# range would happily jump to the next minor version.
def _typescript_version_line(repo_root: Path) -> str:
    manifests = (
        repo_root / "plugins" / "sdk" / "clients" / "typescript" / "package.json",
        repo_root / "package.json",
    )
    for manifest in manifests:
        try:
            payload = json.loads(manifest.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            continue
        if not isinstance(payload, dict):
            continue
        for section in ("devDependencies", "dependencies"):
            entries = payload.get(section)
            if not isinstance(entries, dict):
                continue
            line = _major_minor(str(entries.get("typescript", "")))
            if line:
                return line
    return FALLBACK_TYPESCRIPT_LINE


# Extracts the leading 'major.minor' pair from a semver-ish spec.
def _major_minor(version: str) -> str:
    digits: list[str] = []
    current = ""
    for character in version:
        if character.isdigit():
            current += character
        elif current:
            digits.append(current)
            current = ""
    if current:
        digits.append(current)
    if len(digits) >= 2:
        return f"{digits[0]}.{digits[1]}"
    return digits[0] if digits else ""


# Exports the rust toolchain into the process environment.
#
# Xcode's build service hands the hook a PATH without a usable cargo/rustc pair
# and Flutter re-runs the hook in that same state, so the toolchain has to be
# exported once per invocation instead of being pinned for a single command.
def _expose_rust_toolchain() -> None:
    resolved = _resolve_rust_toolchain(os.environ)
    if resolved is None:
        _log_plugin_sync(
            "no rustup toolchain carrying cargo + rustc found in the environment"
        )
        return
    rustup_home, toolchain, bin_directory = resolved
    os.environ["RUSTUP_HOME"] = rustup_home
    os.environ["RUSTUP_TOOLCHAIN"] = toolchain
    os.environ["RUSTC"] = os.path.join(bin_directory, "rustc")
    _prepend_path(Path(bin_directory))
    _log_plugin_sync(f"rust toolchain: {toolchain} ({bin_directory})")


# Prepends a directory to PATH when it is missing from it.
def _prepend_path(directory: Path) -> None:
    entries = os.environ.get("PATH", "").split(os.pathsep)
    if str(directory) in entries:
        return
    os.environ["PATH"] = os.pathsep.join([str(directory), *entries])
    _log_plugin_sync(f"PATH <- {directory}")


# Locates corepack next to the node installation.
def _corepack_command(node_directory: Path) -> str | None:
    for name in ("corepack", "corepack.cmd"):
        candidate = node_directory / name
        if candidate.is_file():
            return str(candidate)
    return shutil.which("corepack")


# Puts corepack's pnpm shim on PATH so `pnpm run ...` works inside pack scripts.
def _ensure_package_manager(corepack: str, repo_root: Path) -> None:
    directory = repo_root / ".ci-tools" / "bin"
    shim = directory / ("pnpm.cmd" if os.name == "nt" else "pnpm")
    if not shim.exists():
        try:
            directory.mkdir(parents=True, exist_ok=True)
        except OSError as error:
            _log_plugin_sync(f"cannot create {directory}: {error}")
            return
        completed = subprocess.run(
            [corepack, "enable", "--install-directory", str(directory), "pnpm"],
            capture_output=True,
            text=True,
            check=False,
        )
        _log_plugin_sync(f"corepack enable pnpm -> rc={completed.returncode}")
        for stream, content in (
            ("stdout", completed.stdout),
            ("stderr", completed.stderr),
        ):
            trimmed = content.strip()
            if trimmed:
                _log_plugin_sync(f"{stream}: {trimmed[-600:]}")
    if shim.exists():
        _prepend_path(directory)
    else:
        _log_plugin_sync(f"corepack did not create {shim}")


# Installs the dependencies declared by a script-packed ToolPkg.
def _install_toolpkg_dependencies(corepack: str, child_dir: Path) -> None:
    _log_plugin_sync(f"installing dependencies for {child_dir.name}")
    completed = subprocess.run(
        [corepack, "pnpm", "install"],
        cwd=str(child_dir),
        capture_output=True,
        text=True,
        check=False,
    )
    _log_plugin_sync(f"pnpm install -> rc={completed.returncode}")
    for stream, content in (
        ("stdout", completed.stdout),
        ("stderr", completed.stderr),
    ):
        trimmed = content.strip()
        if trimmed:
            _log_plugin_sync(f"{stream}: {trimmed[-1500:]}")
    if completed.returncode != 0:
        raise RuntimeError(f"Failed to install dependencies for {child_dir.name}")


# Makes a script-packed ToolPkg buildable inside the host build environment.
#
# Its pack script calls bare `tsc`, `pnpm` and `node`, and it bundles the web
# experience with esbuild, so the compiler, the package manager shim and the
# declared dependencies must all exist before the script runs.
def _prepare_script_toolpkg(
    repo_root: Path,
    child_dir: Path,
    *,
    dry_run: bool,
) -> str:
    if dry_run:
        return shutil.which("corepack") or "corepack"
    node_directory = _ensure_node_on_path()
    if node_directory is None:
        raise FileNotFoundError("Node.js is required to build script-packed ToolPkgs")
    _prepend_path(Path(_typescript_command(repo_root, dry_run=False)).parent)
    corepack = _corepack_command(node_directory)
    if corepack is None:
        raise FileNotFoundError("Corepack is required to build script-packed ToolPkgs")
    _ensure_package_manager(corepack, repo_root)
    os.environ.setdefault("COREPACK_ENABLE_DOWNLOAD_PROMPT", "0")
    os.environ.setdefault("PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD", "1")
    if not (child_dir / "node_modules").is_dir():
        _install_toolpkg_dependencies(corepack, child_dir)
    return corepack


# Installs the repository-managed TypeScript compiler into .ci-tools.
def _provision_typescript(repo_root: Path) -> Path | None:
    root = repo_root / ".ci-tools" / "typescript"
    executable = "tsc.cmd" if os.name == "nt" else "tsc"
    repository_executable = root / "node_modules" / ".bin" / executable
    if repository_executable.is_file():
        return repository_executable
    if _ensure_node_on_path() is None:
        return None
    toolchain = _node_toolchain()
    if toolchain is None:
        return None
    node, npm_cli, _ = toolchain
    version = _typescript_version_line(repo_root)
    _log_plugin_sync(f"provisioning typescript@{version} into {root}")
    try:
        root.mkdir(parents=True, exist_ok=True)
    except OSError as error:
        _log_plugin_sync(f"cannot create {root}: {error}")
        return None
    completed = subprocess.run(
        [
            node,
            npm_cli,
            "install",
            "--prefix",
            str(root),
            "--no-audit",
            "--no-fund",
            "--loglevel",
            "error",
            f"typescript@{version}",
        ],
        cwd=str(root),
        capture_output=True,
        text=True,
        check=False,
    )
    _log_plugin_sync(f"npm install typescript@{version} -> rc={completed.returncode}")
    for stream, content in (
        ("stdout", completed.stdout),
        ("stderr", completed.stderr),
    ):
        trimmed = content.strip()
        if trimmed:
            _log_plugin_sync(f"{stream}: {trimmed[-600:]}")
    return repository_executable if repository_executable.is_file() else None


def _generate_plugin_sdk_types(repo_root: Path, *, dry_run: bool) -> None:
    core_root = repo_root / "core"
    sdk_source_root = core_root / "crates" / "plugin" / "sdk" / "src"
    declaration_root = repo_root / "plugins" / "types"
    _run_checked_command(
        [
            _platform_command("cargo"),
            "run",
            "-p",
            "operit-plugin-sdk-codegen",
            "--",
            "generate",
            str(sdk_source_root),
            str(declaration_root),
        ],
        core_root,
        dry_run=dry_run,
        env=_host_cargo_environment(),
    )


def _iter_signature_files(paths: list[Path]) -> list[Path]:
    seen: set[Path] = set()
    files: list[Path] = []
    for path in paths:
        if not path.is_file() or path in seen:
            continue
        seen.add(path)
        files.append(path)
    files.sort(key=lambda path: path.as_posix().lower())
    return files


def _compute_paths_signature(base_dir: Path, paths: list[Path]) -> str:
    digest = hashlib.sha256()
    for file_path in _iter_signature_files(paths):
        digest.update(file_path.relative_to(base_dir).as_posix().encode("utf-8"))
        digest.update(b"\0")
        with file_path.open("rb") as handle:
            for chunk in iter(lambda: handle.read(1024 * 1024), b""):
                digest.update(chunk)
        digest.update(b"\0")
    return digest.hexdigest()


def _collect_prebuild_inputs(source_dir: Path, child_dir: Path) -> list[Path]:
    paths: list[Path] = []
    tsconfig = child_dir / "tsconfig.json"
    if tsconfig.is_file():
        paths.append(tsconfig)
    for file_path in child_dir.rglob("*"):
        if "node_modules" in file_path.parts:
            continue
        if file_path.is_file() and file_path.suffix.lower() in {".ts", ".d.ts"}:
            paths.append(file_path)
    types_dir = _plugins_root() / "types"
    if types_dir.is_dir():
        for file_path in types_dir.rglob("*"):
            if file_path.is_file() and file_path.suffix.lower() in {".ts", ".d.ts"}:
                paths.append(file_path)
    package_json = child_dir / "package.json"
    if package_json.is_file():
        paths.append(package_json)
        build_script = child_dir / "build.js"
        if build_script.is_file():
            paths.append(build_script)
    return paths


def _collect_root_prebuild_inputs(source_dir: Path) -> list[Path]:
    paths: list[Path] = []
    tsconfig = source_dir / "tsconfig.json"
    if tsconfig.is_file():
        paths.append(tsconfig)
    for file_path in source_dir.iterdir():
        if file_path.is_file() and file_path.suffix.lower() in {".ts", ".d.ts"}:
            paths.append(file_path)
    types_dir = _plugins_root() / "types"
    if types_dir.is_dir():
        for file_path in types_dir.rglob("*"):
            if file_path.is_file() and file_path.suffix.lower() in {".ts", ".d.ts"}:
                paths.append(file_path)
    return paths


def _load_state(path: Path) -> dict[str, str]:
    if not path.is_file():
        return {}
    data = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict):
        raise ValueError(f"State file must contain a JSON object: {path}")
    return {str(key): str(value) for key, value in data.items()}


def _save_state(path: Path, state: dict[str, str]) -> None:
    path.write_text(json.dumps(state, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")



def _collect_hot_reload_outputs(output_dir: Path) -> list[Path]:
    if not output_dir.is_dir():
        return []
    return [
        file_path
        for file_path in output_dir.iterdir()
        if file_path.is_file() and file_path.suffix.lower() in SYNCABLE_SUFFIXES
    ]


def _compute_hot_reload_signature(output_dir: Path) -> str:
    digest = hashlib.sha256()
    if output_dir.is_dir():
        for file_path in _iter_signature_files(_collect_hot_reload_outputs(output_dir)):
            digest.update(file_path.name.encode("utf-8"))
            digest.update(b"\0")
            with file_path.open("rb") as handle:
                for chunk in iter(lambda: handle.read(1024 * 1024), b""):
                    digest.update(chunk)
            digest.update(b"\0")
    return digest.hexdigest()


# Lists command lines of currently running processes for VM service discovery.
def _running_process_command_lines() -> list[str]:
    if os.name == "nt":
        command = [
            "powershell.exe",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-CimInstance Win32_Process | Select-Object -ExpandProperty CommandLine",
        ]
    else:
        command = ["ps", "-Ao", "args="]
    completed = subprocess.run(
        command,
        capture_output=True,
        check=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    return [line.strip() for line in completed.stdout.splitlines() if line.strip()]


# Finds the authenticated VM service URI created by Flutter's development service.
def _discover_vm_service() -> str:
    uri_pattern = re.compile(
        r"--vm-service-uri=(?:\"([^\"]+)\"|'([^']+)'|([^\s]+))"
    )
    candidates: set[str] = set()
    for command_line in _running_process_command_lines():
        if not re.search(r"\bdevelopment-service\b", command_line):
            continue
        match = uri_pattern.search(command_line)
        if match is None:
            continue
        candidate = next(value for value in match.groups() if value)
        parsed = urllib.parse.urlsplit(candidate)
        if parsed.scheme in {"http", "https", "ws", "wss"} and parsed.netloc:
            candidates.add(candidate)
    if len(candidates) != 1:
        raise RuntimeError(
            "Expected exactly one running Flutter development service with an authenticated VM Service URI; "
            f"found {len(candidates)}"
        )
    discovered = next(iter(candidates))
    print(f"AUTO-DISCOVERED-VM-SERVICE: {discovered}")
    return discovered


# Uploads packages and waits for the running application's reload acknowledgement.
def _maybe_hot_reload_output(
    source_dir: Path,
    output_dir: Path,
    *,
    state_key: str,
    label: str,
    dry_run: bool,
    disabled: bool,
    timeout_seconds: float,
    vm_service: str | None,
) -> None:
    if dry_run or disabled:
        return
    if vm_service is None:
        vm_service = _discover_vm_service()
    parsed = urllib.parse.urlsplit(vm_service)
    if parsed.scheme not in {"http", "https", "ws", "wss"} or not parsed.netloc:
        raise ValueError("--vm-service must be a complete authenticated VM Service URL")
    scheme = {"ws": "http", "wss": "https"}.get(parsed.scheme, parsed.scheme)
    path = parsed.path.removesuffix("/ws").rstrip("/") + "/"
    endpoint = urllib.parse.urlunsplit((scheme, parsed.netloc, path, "", ""))

    # Calls the VM service HTTP interface and rejects protocol errors.
    def call(method: str, **params: str) -> dict:
        url = endpoint + method + "?" + urllib.parse.urlencode(params)
        with urllib.request.urlopen(url, timeout=timeout_seconds) as response:
            payload = json.load(response)
        if "error" in payload:
            raise RuntimeError(f"Plugin hot reload failed: {payload['error']}")
        return payload["result"]

    isolates = []
    for isolate in call("getVM")["isolates"]:
        info = call("getIsolate", isolateId=isolate["id"])
        if "ext.operit.reloadPlugins" in info.get("extensionRPCs", []):
            isolates.append(isolate["id"])
    if len(isolates) != 1:
        raise RuntimeError("Expected exactly one app isolate with ext.operit.reloadPlugins; restart the debug app with the new endpoint")
    isolate_id = isolates[0]
    call("ext.operit.reloadPlugins", isolateId=isolate_id, action="begin")
    for artifact in _iter_signature_files(_collect_hot_reload_outputs(output_dir)):
        content = base64.b64encode(artifact.read_bytes()).decode("ascii")
        for offset in range(0, len(content), 24000):
            call("ext.operit.reloadPlugins", isolateId=isolate_id, action="chunk",
                 name=artifact.name, content=content[offset:offset + 24000])
    call("ext.operit.reloadPlugins", isolateId=isolate_id, action="commit")
    signature = _compute_hot_reload_signature(output_dir)
    state_file = source_dir / HOT_RELOAD_STATE_FILE
    state = _load_state(state_file)
    state[state_key] = signature
    _save_state(state_file, state)
    print(f"HOT-RELOAD-DONE: {label} application acknowledged reload")


# Builds ToolPkg sources before their synchronization operations run.
def _prebuild_plans(repo_root: Path, source_dir: Path, plans: list[SyncPlanItem], *, dry_run: bool) -> None:
    state_file = source_dir / ".sync_state.json"
    state = _load_state(state_file)
    changed = False
    typescript_command: str | None = None

    def run_typescript(tsconfig: Path) -> None:
        nonlocal typescript_command
        if typescript_command is None:
            typescript_command = _typescript_command(repo_root, dry_run=dry_run)
        _run_checked_command(
            [typescript_command, "-p", str(tsconfig)],
            repo_root,
            dry_run=dry_run,
        )

    if any(plan.mode == "compile-ts" for plan in plans):
        tsconfig = source_dir / "tsconfig.json"
        if not tsconfig.is_file():
            raise ValueError(f"Missing tsconfig.json for TypeScript plugins: {source_dir}")
        signature = _compute_paths_signature(repo_root, _collect_root_prebuild_inputs(source_dir))
        key = "prebuild:."
        # A saved input signature does not guarantee that generated files survived cleanup.
        compiled_outputs_exist = all(
            (_plugins_root() / ".out" / source_dir.name / f"{plan.source.stem}.js").is_file()
            for plan in plans
            if plan.mode == "compile-ts"
        )
        if state.get(key) == signature and compiled_outputs_exist:
            print(f"SKIP-PREBUILD: {source_dir}")
        else:
            run_typescript(tsconfig)
            state[key] = signature
            changed = True

    child_dirs = sorted(
        {plan.source for plan in plans if plan.source.is_dir()},
        key=lambda path: path.name.lower(),
    )
    for child_dir in child_dirs:
        if _is_script_packed_toolpkg(child_dir):
            corepack_command = _prepare_script_toolpkg(
                repo_root, child_dir, dry_run=dry_run
            )
            _run_checked_command(
                [corepack_command, "pnpm", "run", "pack:toolpkg"],
                child_dir,
                dry_run=dry_run,
            )
            continue

        tsconfig = child_dir / "tsconfig.json"
        if not tsconfig.is_file():
            continue
        signature = _compute_paths_signature(repo_root, _collect_prebuild_inputs(source_dir, child_dir))
        key = f"prebuild:{child_dir.relative_to(source_dir).as_posix()}"
        if state.get(key) == signature:
            print(f"SKIP-PREBUILD: {child_dir}")
        else:
            run_typescript(tsconfig)
            state[key] = signature
            changed = True

    if changed and not dry_run:
        _save_state(state_file, state)


def _iter_files_for_pack(repo_root: Path, folder: Path) -> list[Path]:
    folder_rel = folder.relative_to(repo_root).as_posix()
    completed = subprocess.run(
        ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard", "--", folder_rel],
        cwd=str(repo_root),
        capture_output=True,
        check=False,
    )
    if completed.returncode != 0:
        raise RuntimeError(f"git ls-files failed for: {folder_rel}")

    files: list[Path] = []
    seen: set[Path] = set()
    for raw_path in completed.stdout.split(b"\x00"):
        if not raw_path:
            continue
        file_path = repo_root / Path(raw_path.decode("utf-8"))
        if file_path.is_file() and file_path not in seen:
            seen.add(file_path)
            files.append(file_path)
    files.sort(key=lambda path: path.relative_to(folder).as_posix())
    return files


def _pack_toolpkg_folder(repo_root: Path, source_folder: Path, destination_file: Path) -> None:
    if _find_manifest_file(source_folder) is None:
        raise ValueError(f"Missing manifest.hjson or manifest.json: {source_folder}")
    destination_file.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(destination_file, mode="w", compression=zipfile.ZIP_DEFLATED) as archive:
        for file_path in _iter_files_for_pack(repo_root, source_folder):
            archive.write(file_path, file_path.relative_to(source_folder).as_posix())


def _delete_unplanned_outputs(output_dir: Path, planned_names: set[str], *, dry_run: bool) -> int:
    if not output_dir.is_dir():
        return 0
    deleted = 0
    for file_path in sorted(output_dir.iterdir(), key=lambda path: path.name.lower()):
        if not file_path.is_file() or file_path.suffix.lower() not in SYNCABLE_SUFFIXES:
            continue
        if file_path.name in planned_names:
            continue
        print(f"{'DRY-DELETE' if dry_run else 'DELETE'}: {file_path}")
        if not dry_run:
            file_path.unlink()
        deleted += 1
    return deleted


# Synchronizes one plugin source directory into its runtime output directory.
def _sync(source_dir: Path, output_dir: Path, *, dry_run: bool) -> tuple[int, int, int]:
    repo_root = _repo_root()
    plans = _collect_sync_plan(source_dir)
    _prebuild_plans(repo_root, source_dir, plans, dry_run=dry_run)

    if not dry_run:
        output_dir.mkdir(parents=True, exist_ok=True)

    planned_names = {plan.destination_name for plan in plans}
    deleted = _delete_unplanned_outputs(output_dir, planned_names, dry_run=dry_run)
    copied = 0
    packed = 0
    for plan in plans:
        destination = output_dir / plan.destination_name
        if plan.mode == "copy":
            print(f"{'DRY-COPY' if dry_run else 'COPY'}: {plan.source} -> {destination}")
            if not dry_run:
                shutil.copy2(plan.source, destination)
            copied += 1
        elif plan.mode == "compile-ts":
            compiled = _plugins_root() / ".out" / source_dir.name / f"{plan.source.stem}.js"
            print(f"{'DRY-COPY' if dry_run else 'COPY'}: {compiled} -> {destination}")
            if not dry_run:
                if not compiled.is_file():
                    raise FileNotFoundError(f"Compiled JavaScript not found: {compiled}")
                shutil.copy2(compiled, destination)
            copied += 1
        elif plan.mode == "copy-script-toolpkg":
            archive = plan.source / "dist" / plan.destination_name
            print(f"{'DRY-COPY' if dry_run else 'COPY'}: {archive} -> {destination}")
            if not dry_run:
                if not archive.is_file():
                    raise FileNotFoundError(f"Script-packed ToolPkg not found: {archive}")
                shutil.copy2(archive, destination)
            packed += 1
        else:
            print(f"{'DRY-PACK' if dry_run else 'PACK'}: {plan.source} -> {destination}")
            if not dry_run:
                _pack_toolpkg_folder(repo_root, plan.source, destination)
            packed += 1
    return copied, packed, deleted


def main() -> int:
    plugins_root = _plugins_root()
    repo_root = _repo_root()
    parser = argparse.ArgumentParser(description="Sync Operit2 plugin package sources.")
    parser.add_argument(
        "--source",
        choices=("buildin", "external", "runtime", "examples", "all"),
        default="all",
    )
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument(
        "--buildin-output",
        default=str(repo_root / "core" / "crates" / "runtime" / "application" / "assets" / "plugins" / "buildin"),
    )
    parser.add_argument(
        "--external-output",
        default=str(repo_root / "core" / "crates" / "runtime" / "application" / "assets" / "plugins" / "external"),
    )
    parser.add_argument(
        "--examples-output",
        default=str(plugins_root / ".out" / "examples"),
    )
    parser.add_argument("--no-hot-reload", action="store_true")
    parser.add_argument("--vm-service", help="Authenticated VM Service URL printed by fvm flutter run")
    parser.add_argument("--hot-reload-timeout", type=float, default=5.0)
    args = parser.parse_args()

    _generate_plugin_sdk_types(repo_root, dry_run=bool(args.dry_run))

    if not args.dry_run:
        _expose_rust_toolchain()
        _ensure_node_on_path()

    total_copied = 0
    total_packed = 0
    total_deleted = 0
    jobs: list[tuple[Path, Path]] = []
    if args.source in {"buildin", "runtime", "all"}:
        jobs.append((_plugin_packages_root() / "buildin", Path(args.buildin_output)))
    if args.source in {"external", "runtime", "all"}:
        jobs.append((_plugin_packages_root() / "external", Path(args.external_output)))
    if args.source in {"examples", "all"}:
        jobs.append((_plugin_packages_root() / "examples", Path(args.examples_output)))

    for source_dir, output_dir in jobs:
        copied, packed, deleted = _sync(source_dir, output_dir, dry_run=args.dry_run)
        total_copied += copied
        total_packed += packed
        total_deleted += deleted

    if args.source in {"buildin", "runtime", "all"}:
        _maybe_hot_reload_output(
            _plugin_packages_root() / "buildin",
            Path(args.buildin_output),
            state_key="buildin-output",
            label="buildin",
            dry_run=args.dry_run,
            disabled=bool(args.no_hot_reload),
            timeout_seconds=float(args.hot_reload_timeout),
            vm_service=args.vm_service,
        )
    if args.source in {"external", "runtime", "all"}:
        _maybe_hot_reload_output(
            _plugin_packages_root() / "external",
            Path(args.external_output),
            state_key="external-output",
            label="external",
            dry_run=args.dry_run,
            disabled=bool(args.no_hot_reload),
            timeout_seconds=float(args.hot_reload_timeout),
            vm_service=args.vm_service,
        )

    print(
        "Done. "
        f"source={args.source}, copied={total_copied}, packed={total_packed}, "
        f"deleted={total_deleted}, dry_run={bool(args.dry_run)}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

"""Exercise iSH cache decisions without downloading sources or invoking Xcode."""

import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


REPO_ROOT = Path(__file__).resolve().parents[2]
LIBRARIES = (
    "liblinux.a", "libiSHLinux.a", "libiSHLinuxUser.a", "libfakefs.a",
    "libish_emu.a", "libarchive.a", "libiSHFakefs.a", "libiSHFchdir.a",
)
HEADERS = ("app/LinuxInterop.h", "kernel/errno.h", "tools/fakefs.h")


class IshBuildCacheTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="operit ish cache ")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.script_dir = self.root / "tools/ios-runtime/ish"
        self.script_dir.mkdir(parents=True)
        original = REPO_ROOT / "tools/ios-runtime/ish"
        for name in ("build_ish_ios.sh", "fetch_sources.py"):
            shutil.copy2(original / name, self.script_dir / name)
        shutil.copytree(original / "patches", self.script_dir / "patches")
        inputs = [self.script_dir / "build_ish_ios.sh", self.script_dir / "fetch_sources.py"]
        inputs += sorted((self.script_dir / "patches").glob("*.patch"))
        digests = "".join(hashlib.sha256(path.read_bytes()).hexdigest() + "\n" for path in inputs)
        self.signature = hashlib.sha256(digests.encode()).hexdigest()
        self.source_dir = self.script_dir / "sources/ish"
        for header in HEADERS:
            path = self.source_dir / header
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("test header\n", encoding="utf-8")
        self.rootfs = self.root / "apps/flutter/app/ios/Runner/ish-root.tar.gz"
        self.rootfs.parent.mkdir(parents=True)
        self.rootfs.write_bytes(b"test rootfs")
        self.commands = self.root / "commands"
        self.commands.mkdir()
        self.log = self.root / "calls.log"
        self.stub("xcrun", '''
shift 2
case "$1" in
    --show-sdk-version) printf '27.0\\n' ;;
    nm)
        if [ "${OPERIT_TEST_MISSING_SYMBOL:-0}" != 1 ]; then
            printf '00000000 T _linux_mount_app_directory\\n'
        fi
        ;;
    *) exit 80 ;;
esac
''')
        self.stub("lipo", '''
test "$1" = -verify_arch
grep -qw "$2" "$3"
''')
        self.stub("python3", '''
printf 'fetch %s\\n' "$*" >> "$OPERIT_TEST_CALL_LOG"
exit 73
''')
        for name in ("xcodebuild", "patch"):
            self.stub(name, f'printf "unexpected {name}\\n" >> "$OPERIT_TEST_CALL_LOG"\nexit 80\n')
        self.make_cache()

    def stub(self, name, body):
        path = self.commands / name
        path.write_text("#!/bin/sh\nset -eu\n" + body, encoding="utf-8")
        path.chmod(0o755)

    def make_cache(self, configuration="Debug", *, signature=None, sdk="27.0", platform="iphonesimulator"):
        self.products = self.root / f"apps/flutter/app/apple/ish-build/{configuration}-iphonesimulator"
        self.products.mkdir(parents=True, exist_ok=True)
        for name in LIBRARIES:
            (self.products / name).write_text("arm64 x86_64\n", encoding="utf-8")
        self.marker = self.products / ".operit-ish-build-complete"
        self.marker.write_text(
            f"{configuration}:{sdk}:{platform}:arm64 x86_64:{self.signature if signature is None else signature}\n",
            encoding="utf-8",
        )

    def run_build(self, configuration="Debug", architectures="arm64", **environment):
        env = {**os.environ, "PATH": f"{self.commands}{os.pathsep}{os.environ['PATH']}",
               "OPERIT_TEST_CALL_LOG": str(self.log), "OPERIT_ISH_FORCE_REBUILD": "0",
               "OPERIT_TEST_MISSING_SYMBOL": "0", **environment}
        result = subprocess.run(
            ["bash", str(self.script_dir / "build_ish_ios.sh"), configuration,
             "iphonesimulator", "iphonesimulator", architectures],
            env=env, capture_output=True, text=True, check=False, timeout=15,
        )
        calls = self.log.read_text(encoding="utf-8") if self.log.exists() else ""
        return result, calls

    def assert_cache_hit(self, configuration="Debug", architectures="arm64"):
        result, calls = self.run_build(configuration, architectures)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Reusing", result.stdout)
        self.assertEqual(calls, "", "A warm launch must not fetch, patch, or compile")

    def assert_cache_miss(self, configuration="Debug", architectures="arm64", **environment):
        result, calls = self.run_build(configuration, architectures, **environment)
        self.assertEqual(result.returncode, 73, result.stderr)
        self.assertTrue(calls.startswith("fetch "), calls)
        self.assertNotIn("unexpected", calls)

    def test_warm_debug_cache_skips_fetch_patch_and_compilation(self):
        self.assert_cache_hit(architectures="arm64 x86_64")

    def test_warm_release_cache_skips_fetch_patch_and_compilation(self):
        self.make_cache("Release")
        self.assert_cache_hit("Release", "arm64 x86_64")

    def test_debug_allows_compatible_source_signature_change(self):
        self.make_cache(signature="previous-source-signature")
        result, calls = self.run_build()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("OPERIT_ISH_FORCE_REBUILD=1", result.stdout)
        self.assertEqual(calls, "")

    def test_release_rejects_source_signature_change(self):
        self.make_cache("Release", signature="previous-source-signature")
        self.assert_cache_miss("Release")

    def test_force_rebuild_bypasses_cache(self):
        self.assert_cache_miss(OPERIT_ISH_FORCE_REBUILD="1")

    def test_missing_header_restores_sources(self):
        (self.source_dir / HEADERS[0]).unlink()
        self.assert_cache_miss()

    def test_missing_rootfs_stops_before_fetch(self):
        self.rootfs.unlink()
        result, calls = self.run_build()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(calls, "")

    def test_missing_library_rejects_cache(self):
        (self.products / LIBRARIES[-1]).unlink()
        self.assert_cache_miss()

    def test_missing_marker_rejects_cache(self):
        self.marker.unlink()
        self.assert_cache_miss()

    def test_empty_source_signature_rejects_cache(self):
        self.make_cache(signature="")
        self.assert_cache_miss()

    def test_changed_sdk_rejects_cache(self):
        self.make_cache(sdk="26.0")
        self.assert_cache_miss()

    def test_changed_platform_rejects_cache(self):
        self.make_cache(platform="iphoneos")
        self.assert_cache_miss()

    def test_changed_configuration_rejects_cache(self):
        self.marker.write_text(f"Release:27.0:iphonesimulator:arm64:{self.signature}\n", encoding="utf-8")
        self.assert_cache_miss()

    def test_missing_required_architecture_rejects_cache(self):
        (self.products / LIBRARIES[0]).write_text("arm64\n", encoding="utf-8")
        self.assert_cache_miss(architectures="arm64 x86_64")

    def test_missing_mount_symbol_rejects_cache(self):
        self.assert_cache_miss(OPERIT_TEST_MISSING_SYMBOL="1")


if __name__ == "__main__":
    unittest.main()

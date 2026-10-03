"""Checks ABI-specific Android APK validation without building an application."""

from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch
import zipfile


sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "build_scripts"))
import build_flutter_android as android_build


class AndroidRuntimeAssetsTests(unittest.TestCase):
    """Exercises the actual release APK guard with small archive fixtures."""

    # Creates an APK fixture containing exactly the requested asset entries.
    def make_apk(self, directory, names):
        apk = Path(directory) / "app-arm64-v8a-release.apk"
        with zipfile.ZipFile(apk, "w") as archive:
            for name in names:
                archive.writestr(name, b"fixture")
        return apk

    # Returns the two mandatory rootfs artifacts for one ABI.
    def runtime_assets(self, abi="arm64-v8a"):
        return [
            f"assets/android-runtime/{abi}/rootfs.tar.gz.bin",
            f"assets/android-runtime/{abi}/rootfs.tar.gz.bin.sha256",
        ]

    # Accepts one matching rootfs while preserving unrelated Flutter assets.
    def test_accepts_selected_abi_with_flutter_assets(self):
        with tempfile.TemporaryDirectory() as directory:
            apk = self.make_apk(directory, self.runtime_assets() + [
                "assets/flutter_assets/AssetManifest.bin",
                "assets/android-runtime/",
                "assets/android-runtime/arm64-v8a/",
            ])
            android_build.verify_android_runtime_assets(apk, "arm64-v8a")

    # Rejects the three-architecture packaging error reported in production.
    def test_rejects_three_packaged_abis(self):
        with tempfile.TemporaryDirectory() as directory:
            names = sum((self.runtime_assets(abi) for abi in (
                "arm64-v8a", "armeabi-v7a", "x86_64",
            )), [])
            apk = self.make_apk(directory, names)
            with self.assertRaisesRegex(RuntimeError, "ABIs must be exactly arm64-v8a"):
                android_build.verify_android_runtime_assets(apk, "arm64-v8a")

    # Rejects even one stray artifact from an unselected architecture.
    def test_rejects_stray_foreign_asset(self):
        with tempfile.TemporaryDirectory() as directory:
            apk = self.make_apk(directory, self.runtime_assets() + [
                "assets/android-runtime/x86_64/rootfs.tar.gz.bin.sha256",
            ])
            with self.assertRaisesRegex(RuntimeError, "ABIs must be exactly arm64-v8a"):
                android_build.verify_android_runtime_assets(apk, "arm64-v8a")

    # Rejects an APK containing only a different architecture.
    def test_rejects_wrong_abi(self):
        with tempfile.TemporaryDirectory() as directory:
            apk = self.make_apk(directory, self.runtime_assets("x86_64"))
            with self.assertRaisesRegex(RuntimeError, "ABIs must be exactly arm64-v8a"):
                android_build.verify_android_runtime_assets(apk, "arm64-v8a")

    # Requires actual rootfs files rather than empty ABI directory entries.
    def test_rejects_missing_runtime(self):
        with tempfile.TemporaryDirectory() as directory:
            apk = self.make_apk(directory, ["assets/android-runtime/arm64-v8a/"])
            with self.assertRaisesRegex(RuntimeError, "ABIs must be exactly arm64-v8a"):
                android_build.verify_android_runtime_assets(apk, "arm64-v8a")

    # Requires both the compressed rootfs and its checksum sidecar.
    def test_rejects_missing_required_artifact(self):
        with tempfile.TemporaryDirectory() as directory:
            for missing_index in range(2):
                with self.subTest(missing_index=missing_index):
                    names = self.runtime_assets()
                    del names[missing_index]
                    apk = self.make_apk(directory, names)
                    with self.assertRaisesRegex(RuntimeError, "missing required runtime assets"):
                        android_build.verify_android_runtime_assets(apk, "arm64-v8a")

    # Verifies the real release entry point rejects mixed assets before publication.
    def test_release_does_not_publish_mixed_assets(self):
        with tempfile.TemporaryDirectory() as directory:
            app = Path(directory)
            output = app / "build/app/outputs/flutter-apk"
            output.mkdir(parents=True)
            self.make_apk(output, self.runtime_assets() + self.runtime_assets("x86_64"))
            with (
                patch.object(sys, "argv", ["build_flutter_android.py", "--skip-signing"]),
                patch.object(android_build, "FLUTTER_APP_DIR", app),
                patch.object(android_build, "flutter_command", return_value="flutter-test"),
                patch.object(android_build, "configure_android_flutter_sdk"),
                patch.object(android_build, "flutter_pub_get"),
                patch.object(android_build, "run") as run,
                patch.object(android_build, "copy_required_file") as publish,
            ):
                with self.assertRaisesRegex(RuntimeError, "ABIs must be exactly arm64-v8a"):
                    android_build.main()
                publish.assert_not_called()
                command = run.call_args.args[0]
                self.assertEqual(command[command.index("--target-platform") + 1], "android-arm64")

    # Allows the release entry point to publish a validated ABI-specific APK.
    def test_release_publishes_matching_assets(self):
        with tempfile.TemporaryDirectory() as directory:
            app = Path(directory)
            output = app / "build/app/outputs/flutter-apk"
            output.mkdir(parents=True)
            apk = self.make_apk(output, self.runtime_assets())
            with (
                patch.object(sys, "argv", ["build_flutter_android.py", "--skip-signing"]),
                patch.object(android_build, "FLUTTER_APP_DIR", app),
                patch.object(android_build, "flutter_command", return_value="flutter-test"),
                patch.object(android_build, "configure_android_flutter_sdk"),
                patch.object(android_build, "flutter_pub_get"),
                patch.object(android_build, "run"),
                patch.object(android_build, "copy_required_file") as publish,
            ):
                self.assertEqual(android_build.main(), 0)
                self.assertEqual(publish.call_args.args[0], apk)


if __name__ == "__main__":
    unittest.main()

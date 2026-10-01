"""Regression checks for the iOS iSH runtime patches."""

from pathlib import Path
import os
import shutil
import subprocess
import tempfile
import zipfile
import unittest


REPO_ROOT = Path(__file__).resolve().parents[2]
PATCH_DIR = REPO_ROOT / "tools/ios-runtime/ish/patches"


class IshRuntimePatchTests(unittest.TestCase):
    def test_mount_data_is_a_bounded_writable_page(self):
        patch = (PATCH_DIR / "0001-operit-managed-runtime-mount.patch").read_text()

        self.assertIn("#include <linux/mm.h>", patch)
        self.assertRegex(patch, r"strnlen\(directory, PAGE_SIZE\)")
        self.assertRegex(patch, r"kzalloc\(PAGE_SIZE, GFP_KERNEL\)")
        self.assertIn("memcpy(mount_data, directory, directory_length + 1)", patch)
        self.assertIn(
            'do_mount(directory, mount_point, "hostfs", MS_SILENT, mount_data)',
            patch,
        )
        self.assertIn("kfree(mount_data)", patch)
        self.assertNotIn(
            'do_mount(directory, mount_point, "hostfs", MS_SILENT, (void *) directory)',
            patch,
        )

    def test_mount_path_length_is_checked_before_copy(self):
        patch = (PATCH_DIR / "0001-operit-managed-runtime-mount.patch").read_text()
        length_check = patch.index("strnlen(directory, PAGE_SIZE)")
        allocation = patch.index("kzalloc(PAGE_SIZE, GFP_KERNEL)")
        copy = patch.index("memcpy(mount_data, directory, directory_length + 1)")
        self.assertLess(length_check, allocation)
        self.assertLess(allocation, copy)
        self.assertIn("return -ENAMETOOLONG", patch)

    def test_tlb_migration_fix_remains_in_the_build_inputs(self):
        patch = (PATCH_DIR / "0004-emulator-tlb-task-migration.patch").read_text()
        build_script = (REPO_ROOT / "tools/ios-runtime/ish/build_ish_ios.sh").read_text()
        project = (
            REPO_ROOT
            / "apps/flutter/app/ios/Runner.xcodeproj/project.pbxproj"
        ).read_text()

        self.assertIn("static struct tlb cpu_tlbs[NR_CPUS];", patch)
        self.assertIn("-static __thread struct tlb the_tlb;", patch)
        self.assertNotIn("+static __thread struct tlb the_tlb;", patch)
        self.assertIn("0004-emulator-tlb-task-migration.patch", project)
        self.assertIn('"$script_dir"/patches/*.patch', build_script)


class IshMountExecutionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        compiler = shutil.which("clang")
        if compiler is None:
            raise unittest.SkipTest("clang is required for the native mount regression")
        temporary = tempfile.TemporaryDirectory(prefix="operit ish mount ")
        cls.addClassCleanup(temporary.cleanup)
        cls.root = Path(temporary.name)
        patch = (PATCH_DIR / "0001-operit-managed-runtime-mount.patch").read_text()
        # Compile the actual implementation, not a duplicate of the fix.
        implementation = "\n".join(
            line[1:] for line in patch.splitlines()
            if line.startswith("+") and not line.startswith("+++")
        )
        implementation = implementation[implementation.index("struct operit_runtime_mount {"):]
        template = (REPO_ROOT / "tools/tests/fixtures/ish_runtime_mount_harness.c").read_text()
        source = cls.root / "mount.c"
        source.write_text(template.replace("/* OPERIT_MOUNT_IMPLEMENTATION */", implementation))
        cls.executable = cls.root / "mount"
        subprocess.run(
            [compiler, "-std=gnu11", "-Wall", "-Wextra", "-Werror", "-O1", "-g",
             "-fsanitize=address,undefined", "-fno-omit-frame-pointer",
             str(source), "-o", str(cls.executable)],
            capture_output=True, text=True, check=True, timeout=60,
        )

    def run_scenario(self, scenario):
        result = subprocess.run(
            [str(self.executable), scenario], capture_output=True, text=True,
            env={**os.environ, "ASAN_OPTIONS": "detect_leaks=0:halt_on_error=1",
                 "UBSAN_OPTIONS": "halt_on_error=1"}, timeout=15,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("mount regression passed", result.stdout)

    def test_readonly_path_mount_and_repeated_mounts(self):
        self.run_scenario("success")

    def test_invalid_paths_never_allocate_or_mount(self):
        self.run_scenario("invalid")

    def test_page_boundary_and_overlong_paths(self):
        self.run_scenario("boundary")

    def test_every_allocation_failure_is_cleaned_up_and_retryable(self):
        self.run_scenario("allocation-failure")

    def test_mount_failure_is_cleaned_up_and_retryable(self):
        self.run_scenario("mount-failure")


class IshPatchApplicationTests(unittest.TestCase):
    def test_all_runtime_patches_apply_to_pinned_upstream_sources(self):
        archives = {
            "ish": ("ish-7864dd601e615d0fc09b888a93d327f458b25a1d.zip",
                    "ish-7864dd601e615d0fc09b888a93d327f458b25a1d/"),
            "linux": ("linux-8ec9bf17f89c6dba818f3ed2427de4223e78644a.zip",
                      "linux-8ec9bf17f89c6dba818f3ed2427de4223e78644a/"),
        }
        patches = sorted(PATCH_DIR.glob("*.patch"))
        downloads = PATCH_DIR.parent / "downloads"
        if any(not (downloads / name).is_file() for name, _ in archives.values()):
            self.skipTest("pinned archives are not cached; no network download in tests")
        with tempfile.TemporaryDirectory(prefix="operit ish patch ") as temporary:
            root = Path(temporary)
            for patch in patches:
                for line in patch.read_text().splitlines():
                    if not line.startswith("--- a/"):
                        continue
                    name = line[len("--- a/"):]
                    key = "linux" if name.startswith("deps/linux/") else "ish"
                    archive, prefix = archives[key]
                    member = name[len("deps/linux/"):] if key == "linux" else name
                    target = root / name
                    target.parent.mkdir(parents=True, exist_ok=True)
                    with zipfile.ZipFile(downloads / archive) as source:
                        target.write_bytes(source.read(prefix + member))
            for patch in patches:
                result = subprocess.run(
                    ["patch", "--batch", "-p1", "-d", str(root), "-i", str(patch)],
                    capture_output=True, text=True, timeout=15,
                )
                self.assertEqual(result.returncode, 0,
                                 patch.name + "\n" + result.stdout + result.stderr)
            self.assertIn("kzalloc(PAGE_SIZE, GFP_KERNEL)",
                          (root / "app/LinuxInterop.c").read_text())
            emulator = (root / "linux/emu_asbestos.c").read_text()
            self.assertIn("static struct tlb cpu_tlbs[NR_CPUS];", emulator)
            self.assertNotIn("static __thread struct tlb the_tlb;", emulator)


if __name__ == "__main__":
    unittest.main()

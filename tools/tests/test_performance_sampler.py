import ctypes
import importlib.util
from pathlib import Path
import unittest

_spec = importlib.util.spec_from_file_location(
    'sample_macos', Path(__file__).parents[1] / 'performance' / 'sample_macos.py',
)
sampler = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(sampler)


class CpuTimeConversionTests(unittest.TestCase):
    def test_nanosecond_timebase(self):
        self.assertAlmostEqual(sampler.cpu_percent(1_000_000_000, 1, 1, 1), 100)

    def test_apple_silicon_timebase(self):
        # 24 million Mach ticks at 125/3 ns is one CPU-second, not 0.024 s.
        self.assertAlmostEqual(sampler.cpu_percent(24_000_000, 1, 125, 3), 100)
        self.assertAlmostEqual(sampler.cpu_percent(12_000_000, 2, 125, 3), 25)

    def test_multiple_cores_and_idle(self):
        self.assertAlmostEqual(sampler.cpu_percent(48_000_000, 1, 125, 3), 200)
        self.assertEqual(sampler.cpu_percent(0, 1, 125, 3), 0)


class TaskInfoTests(unittest.TestCase):
    def test_matches_macos_sdk_layout(self):
        self.assertEqual(ctypes.sizeof(sampler.ProcTaskInfo), 96)
        self.assertEqual(sampler.ProcTaskInfo.thread_count.offset, 84)
        self.assertEqual(sampler.ProcTaskInfo.running_threads.offset, 88)

    def test_reads_native_thread_counts(self):
        class FakeLib:
            def proc_pidinfo(self, pid, flavor, arg, out, size):
                self.args = pid, flavor, arg, size
                result = sampler.ProcTaskInfo()
                result.thread_count = 74
                result.running_threads = 2
                result.context_switches = 1234
                ctypes.memmove(out, ctypes.byref(result), size)
                return size

        lib = FakeLib()
        info = sampler.read_task_info(lib, 42)
        self.assertEqual(lib.args, (42, 4, 0, 96))
        self.assertEqual(info.thread_count, 74)
        self.assertEqual(info.running_threads, 2)
        self.assertEqual(info.context_switches, 1234)

    def test_rejects_failed_or_partial_native_read(self):
        class FakeLib:
            def proc_pidinfo(self, *args):
                return self.result

        for result in (0, 95):
            with self.subTest(result=result):
                lib = FakeLib()
                lib.result = result
                with self.assertRaises(OSError):
                    sampler.read_task_info(lib, 42)


if __name__ == '__main__':
    unittest.main()

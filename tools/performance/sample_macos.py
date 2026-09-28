#!/usr/bin/env python3
"""Sample process CPU deltas and memory without requesting Flutter frames.

CPU 100% means one fully occupied logical core. Works for release builds.
Usage: python3 tools/performance/sample_macos.py PID --seconds 60 > samples.jsonl
"""
import argparse
import ctypes
import datetime
import json
import sys
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('pid', type=int)
    parser.add_argument('--seconds', type=int, default=60)
    args = parser.parse_args()
    if sys.platform != 'darwin':
        parser.error('This sampler requires macOS libproc')
    if args.seconds <= 0:
        parser.error('--seconds must be positive')
    lib = ctypes.CDLL('/usr/lib/libproc.dylib', use_errno=True)
    lib.proc_pid_rusage.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_void_p]
    lib.proc_pid_rusage.restype = ctypes.c_int

    def read():
        # rusage_info_v2 starts with UUID[16], then uint64 user/system times
        # in ns. resident_size and phys_footprint are uint64 fields 6 and 7.
        buffer = ctypes.create_string_buffer(1024)
        if lib.proc_pid_rusage(args.pid, 2, ctypes.byref(buffer)) != 0:
            raise OSError(ctypes.get_errno(), 'proc_pid_rusage failed')
        counters = (ctypes.c_uint64 * 18).from_buffer(buffer, 16)
        return time.monotonic(), counters[0] + counters[1], counters[6], counters[7]

    previous, cpu, _, _ = read()
    for _ in range(args.seconds):
        time.sleep(1)
        now, current, rss, footprint = read()
        print(json.dumps({
            'at': datetime.datetime.now(datetime.timezone.utc).isoformat(),
            'pid': args.pid, 'interval_s': now - previous,
            'cpu_percent': (current - cpu) / 1e9 / (now - previous) * 100,
            'rss_mib': rss / 1048576, 'footprint_mib': footprint / 1048576,
        }), flush=True)
        previous, cpu = now, current


if __name__ == '__main__':
    main()

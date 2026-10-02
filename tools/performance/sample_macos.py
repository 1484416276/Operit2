#!/usr/bin/env python3
"""Sample process CPU, memory, threads, wakeups and disk I/O without Flutter frames.

CPU 100% means one fully occupied logical core. Works for release builds.
Usage: python3 tools/performance/sample_macos.py PID --seconds 60 > samples.jsonl
"""
import argparse
import ctypes
import datetime
import json
import sys
import time


class MachTimebaseInfo(ctypes.Structure):
    _fields_ = [('numer', ctypes.c_uint32), ('denom', ctypes.c_uint32)]


class ProcTaskInfo(ctypes.Structure):
    """PROC_PIDTASKINFO layout from the macOS SDK's sys/proc_info.h."""
    _fields_ = [
        (name, ctypes.c_uint64) for name in (
            'virtual_size', 'resident_size', 'total_user', 'total_system',
            'threads_user', 'threads_system',
        )
    ] + [
        (name, ctypes.c_int32) for name in (
            'policy', 'faults', 'pageins', 'cow_faults', 'messages_sent',
            'messages_received', 'syscalls_mach', 'syscalls_unix',
            'context_switches', 'thread_count', 'running_threads', 'priority',
        )
    ]


def read_task_info(lib, pid):
    info = ProcTaskInfo()
    size = ctypes.sizeof(info)
    actual = lib.proc_pidinfo(pid, 4, 0, ctypes.byref(info), size)
    if actual != size:
        raise OSError(ctypes.get_errno(), 'proc_pidinfo(PROC_PIDTASKINFO) failed')
    return info


def cpu_percent(cpu_ticks, interval_s, numer, denom):
    """Convert proc_pid_rusage Mach absolute ticks, not nanoseconds, to CPU %."""
    return cpu_ticks * numer / denom / 1e9 / interval_s * 100


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('pid', type=int)
    parser.add_argument('--seconds', type=int, default=60)
    args = parser.parse_args()
    if sys.platform != 'darwin':
        parser.error('This sampler requires macOS libproc')
    if args.seconds <= 0:
        parser.error('--seconds must be positive')
    system = ctypes.CDLL('/usr/lib/libSystem.dylib')
    system.mach_timebase_info.argtypes = [ctypes.POINTER(MachTimebaseInfo)]
    system.mach_timebase_info.restype = ctypes.c_int
    timebase = MachTimebaseInfo()
    if system.mach_timebase_info(ctypes.byref(timebase)) != 0 or timebase.denom == 0:
        raise RuntimeError('mach_timebase_info failed')
    lib = ctypes.CDLL('/usr/lib/libproc.dylib', use_errno=True)
    lib.proc_pid_rusage.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_void_p]
    lib.proc_pid_rusage.restype = ctypes.c_int
    lib.proc_pidinfo.argtypes = [
        ctypes.c_int, ctypes.c_int, ctypes.c_uint64, ctypes.c_void_p, ctypes.c_int,
    ]
    lib.proc_pidinfo.restype = ctypes.c_int

    def read():
        # rusage_info_v2 starts with UUID[16], then uint64 user/system times
        # in Mach absolute ticks. Convert using mach_timebase_info (Apple
        # Silicon does not necessarily use a 1:1 nanosecond timebase).
        # resident_size and phys_footprint are uint64 fields 6 and 7.
        buffer = ctypes.create_string_buffer(1024)
        if lib.proc_pid_rusage(args.pid, 2, ctypes.byref(buffer)) != 0:
            raise OSError(ctypes.get_errno(), 'proc_pid_rusage failed')
        counters = list((ctypes.c_uint64 * 18).from_buffer(buffer, 16))
        info = read_task_info(lib, args.pid)
        return time.monotonic(), counters, info

    previous, counters, info = read()
    for _ in range(args.seconds):
        time.sleep(1)
        now, current, current_info = read()
        interval = now - previous
        cpu_ticks = current[0] + current[1] - counters[0] - counters[1]
        print(json.dumps({
            'at': datetime.datetime.now(datetime.timezone.utc).isoformat(),
            'pid': args.pid, 'interval_s': interval,
            'cpu_percent': cpu_percent(cpu_ticks, interval, timebase.numer, timebase.denom),
            'mach_timebase_numer': timebase.numer, 'mach_timebase_denom': timebase.denom,
            'rss_mib': current[6] / 1048576,
            'footprint_mib': current[7] / 1048576,
            'threads': current_info.thread_count,
            'running_threads': current_info.running_threads,
            'idle_wakeups_per_s': (current[2] - counters[2]) / interval,
            'interrupt_wakeups_per_s': (current[3] - counters[3]) / interval,
            'context_switches_per_s': (
                current_info.context_switches - info.context_switches
            ) / interval,
            'disk_read_bytes': current[16] - counters[16],
            'disk_write_bytes': current[17] - counters[17],
        }), flush=True)
        previous, counters, info = now, current, current_info


if __name__ == '__main__':
    main()

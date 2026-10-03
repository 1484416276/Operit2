#!/usr/bin/env python3
"""Real two-process workspace replication, using the existing network fixture."""
import argparse
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import uuid

from network_control_session import Terminal, pair, join


def wait_file(path, expected, timeout=60):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            if expected is None:
                if not path.exists():
                    return
            elif path.read_bytes() == expected:
                return
        except (FileNotFoundError, IsADirectoryError):
            pass
        time.sleep(.15)
    raise AssertionError(f'Workspace did not converge: {path}, expected {expected!r}')


def exercise(a, b, transport):
    wa = a.root / 'workspaces' / 'identities' / a.name
    wb = b.root / 'workspaces' / 'identities' / b.name
    project = wa / 'portable'
    project.mkdir(parents=True, exist_ok=True)
    (project / 'before-join.txt').write_bytes(b'existing desktop data')
    (wb / 'phone-project').mkdir(parents=True)
    (wb / 'phone-project/local.txt').write_bytes(b'existing phone data')
    pair(b, a, transport)
    time.sleep(2.5)
    assert not (wb / 'portable/before-join.txt').exists(), 'Pairing alone must not share files'
    join(b, a)
    wait_file(wb / 'portable/before-join.txt', b'existing desktop data')
    wait_file(wa / 'phone-project/local.txt', b'existing phone data')

    (project / '中文.txt').write_bytes(b'desktop')
    (project / 'binary.bin').write_bytes(bytes(range(256)) * 600)
    (project / 'empty.txt').write_bytes(b'')
    wait_file(wb / 'portable/中文.txt', b'desktop')
    wait_file(wb / 'portable/binary.bin', bytes(range(256)) * 600)
    wait_file(wb / 'portable/empty.txt', b'')
    (wb / 'portable/中文.txt').write_bytes(b'phone')
    wait_file(project / '中文.txt', b'phone')
    (project / '中文.txt').rename(project / 'renamed.txt')
    wait_file(wb / 'portable/renamed.txt', b'phone')
    wait_file(wb / 'portable/中文.txt', None)
    (wb / 'portable/binary.bin').unlink()
    wait_file(project / 'binary.bin', None)

    # Links to files, external folders, broken targets and loops never leave the PC.
    outside = a.root / 'external'
    outside.mkdir()
    (outside / 'secret.txt').write_bytes(b'local only')
    for name, target in [('external-link', outside), ('loop', project),
                         ('file-link', outside / 'secret.txt'), ('broken', outside / 'missing')]:
        (project / name).symlink_to(target)
    (project / 'after-links.txt').write_bytes(b'scanner still works')
    wait_file(wb / 'portable/after-links.txt', b'scanner still works')
    for name in ['external-link', 'loop', 'file-link', 'broken']:
        assert not (wb / 'portable' / name).exists()

    # Offline edits and deletes on *both* sides, including edits while neither app runs.
    a.close(); b.close()
    (project / 'renamed.txt').write_bytes(b'edited while stopped')
    (wb / 'portable/offline-phone.txt').write_bytes(b'phone offline')
    (wb / 'portable/empty.txt').unlink()
    a.start(); b.start()
    wait_file(wb / 'portable/renamed.txt', b'edited while stopped')
    wait_file(project / 'offline-phone.txt', b'phone offline')
    wait_file(project / 'empty.txt', None)
    # Repeat scans/exchanges must not create feedback operations forever.
    a.command('space', 'sync'); b.command('space', 'sync')
    wait_file(project / 'renamed.txt', b'edited while stopped')
    print(f'PASS {transport}: pre-existing files, pairing isolation, binary/empty/Unicode, '
          'bidirectional edits, rename/delete, links, both-side offline edits and restart', flush=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--transport', choices=['tcp', 'http', 'ws'], default='tcp')
    parser.add_argument('--binary', type=Path, default=Path(__file__).resolve().parents[1] / 'target/debug/operit2')
    args = parser.parse_args()
    root = Path(tempfile.mkdtemp(prefix=f'operit-workspace-{args.transport}-'))
    home = root / 'home'; home.mkdir()
    print(f'Test artifacts: {root}', flush=True)
    if sys.platform == 'darwin':
        env = dict(os.environ, HOME=str(home))
        keychain = home / 'Library/Keychains/login.keychain-db'
        keychain.parent.mkdir(parents=True)
        original = subprocess.check_output(['security', 'default-keychain', '-d', 'user'])
        subprocess.run(['security', 'create-keychain', '-p', str(uuid.uuid4()), str(keychain)], env=env, check=True)
        assert subprocess.check_output(['security', 'default-keychain', '-d', 'user']) == original
    terminals = []
    try:
        for name in ('A', 'B'):
            terminals.append(Terminal(args.binary.resolve(), root, name, args.transport, home))
        exercise(*terminals, args.transport)
    finally:
        for terminal in terminals:
            terminal.close()
    if sys.platform == 'darwin':
        assert subprocess.check_output(['security', 'default-keychain', '-d', 'user']) == original


if __name__ == '__main__':
    main()

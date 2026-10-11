#!/usr/bin/env python3
# Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0
"""Build the actual tool/exporter and run the Rust CLI integration proof."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]


def sha(path):
    value = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            value.update(block)
    return value.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--toolchain', required=True)
    args = parser.parse_args()
    target = Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target')).resolve()
    parent = target / 'native-tool-proof' / args.toolchain
    parent.mkdir(parents=True, exist_ok=True)
    output = Path(tempfile.mkdtemp(prefix='run-', dir=parent))
    commands = []
    def run(label, command, environment=None):
        started = datetime.now(timezone.utc).isoformat()
        result = subprocess.run(command, cwd=ROOT, env=os.environ | (environment or {}), capture_output=True, text=True)
        (output / (label + '.stdout')).write_text(result.stdout)
        (output / (label + '.stderr')).write_text(result.stderr)
        commands.append(dict(argv=command, environment=environment or {}, exit_code=result.returncode,
                             started_at=started, finished_at=datetime.now(timezone.utc).isoformat()))
        if result.returncode:
            print(result.stdout + result.stderr, file=sys.stderr)
            raise RuntimeError(f'{label} failed; complete logs: {output}')
        return result
    test_build = run('build-tests', ['cargo', '+' + args.toolchain, 'test', '--no-run', '--locked', '--all-features',
        '-p', 'up-native-profile', '--test', 'cli', '--message-format=json'])
    exporter_build = run('build-exporter', ['cargo', '+' + args.toolchain, 'build', '--locked', '--all-features',
        '-p', 'native-inventory-fixture-export', '--bin', 'native-inventory-fixture-export', '--message-format=json'])
    executables = {}
    for build in [test_build, exporter_build]:
        for line in build.stdout.splitlines():
            record = json.loads(line)
            if record.get('reason') == 'compiler-artifact' and record.get('executable'):
                executables[record['target']['name']] = Path(record['executable'])
    assert set(executables) == {'up-native-profile', 'native-inventory-fixture-export', 'cli'}
    before = {name: sha(path) for name, path in executables.items()}
    identities = {name: dict(path=str(path), sha256=before[name]) for name, path in executables.items()}
    (output / 'FROZEN.json').write_text(json.dumps(dict(
        frozen_at=datetime.now(timezone.utc).isoformat(), executables=identities), indent=2) + '\n')
    tests = run('tests', [str(executables['cli']), '--nocapture'], {
            'UP_NATIVE_PROFILE_EXPORTER': str(executables['native-inventory-fixture-export']),
            'UP_NATIVE_PROFILE_PROOF_DIR': str(output / 'artifacts'),
        })
    assert 'exported_inventory_drives_all_six_cli_subcommands ... ok' in tests.stdout
    assert '1 passed; 0 failed; 0 ignored;' in tests.stdout
    assert {name: sha(path) for name, path in executables.items()} == before, 'test launch rebuilt a prebuilt tool identity'
    files = {str(path.relative_to(output)): sha(path) for path in sorted(output.rglob('*')) if path.is_file()}
    assert len([name for name in files if name.startswith('artifacts/')]) == 9
    receipt = dict(result='PASS_NATIVE_TOOL_CLI_AND_EXPORTER', toolchain=args.toolchain,
        output=str(output), commands=commands,
        executables=identities, files=files)
    (output / 'RESULT.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps(receipt, indent=2))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())

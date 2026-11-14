#!/usr/bin/env python3
# Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0
"""Build a native-required negative and an otherwise identical standalone control."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

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
    output = target / 'constructor-probe' / args.toolchain
    output.mkdir(parents=True, exist_ok=True)
    build = subprocess.run(['cargo', '+' + args.toolchain, 'build', '--locked', '--all-features',
                            '--lib', '-p', 'up-rust', '--message-format=json'], cwd=ROOT,
                           capture_output=True, text=True)
    (output / 'build.stdout').write_text(build.stdout)
    (output / 'build.stderr').write_text(build.stderr)
    if build.returncode:
        print(build.stderr, file=sys.stderr)
        return build.returncode
    libraries = []
    for line in build.stdout.splitlines():
        record = json.loads(line)
        if record.get('reason') == 'compiler-artifact' and Path(record['manifest_path']) == ROOT / 'Cargo.toml':
            libraries.extend(Path(name) for name in record['filenames'] if name.endswith('.rlib'))
    assert len(libraries) == 1, 'Cargo must identify the exact SDK library artifact'
    library = libraries[0]
    dependency_dir = library.parent if library.parent.name == 'deps' else library.parent / 'deps'
    assert dependency_dir.is_dir()
    fixture = ROOT / 'tests/ui/native_required_wire_without_agreement.rs'
    source = fixture.read_text()
    needle = 'const REQUIRES_NATIVE_AGREEMENT: bool = true;'
    assert source.count(needle) == 1
    control = output / 'standalone_control.rs'
    control.write_text(source.replace(needle, 'const REQUIRES_NATIVE_AGREEMENT: bool = false;'))
    commands = []
    for name, path in [('negative', fixture), ('control', control),
                       ('stable-negative', ROOT / 'tests/ui/stable_container_requires_agreement.rs'),
                       ('stable-control', ROOT / 'tests/ui/stable_container_with_agreement.rs')]:
        binary = output / name
        command = ['rustc', '+' + args.toolchain, '--edition=2021', '-Dwarnings', '--emit=link',
                   '--extern', 'up_rust=' + str(library), '-L', 'dependency=' + str(dependency_dir),
                   str(path), '-o', str(binary)]
        result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True)
        (output / (name + '.stderr')).write_text(result.stderr)
        commands.append(dict(argv=command, exit_code=result.returncode, source_sha256=sha(path),
                             stderr_sha256=sha(output / (name + '.stderr'))))
        if name in {'negative', 'stable-negative'}:
            assert result.returncode != 0, 'native-required ordinary constructor unexpectedly built'
            assert 'E0080' in result.stderr and 'this wire requires an explicit native agreement constructor' in result.stderr, result.stderr
        else:
            assert result.returncode == 0, result.stderr
            subprocess.run([str(binary)], cwd=ROOT, check=True)
    receipt = dict(result='PASS_CONSTRUCTOR_CODEGEN_PROOF', toolchain=args.toolchain,
                   sdk_library=str(library), sdk_library_sha256=sha(library), commands=commands,
                    positive_control_sha256=sha(output / 'control'), positive_control_executed=True,
                    stable_control_sha256=sha(output / 'stable-control'), stable_control_executed=True)
    (output / 'RESULT.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps(receipt, indent=2))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())

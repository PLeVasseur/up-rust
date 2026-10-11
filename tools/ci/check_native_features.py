#!/usr/bin/env python3
# Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0
"""Check native hashing's runtime dependency boundary with the selected Cargo."""
import argparse
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--toolchain', required=True)
    args = parser.parse_args()
    for feature, expected in [(None, False), ('native-identity', True),
                              ('selected-wire-codec-core', True)]:
        command = ['cargo', '+' + args.toolchain, 'tree', '--locked',
                   '--no-default-features', '--edges', 'normal,build',
                   '--prefix', 'none', '--format', '{p}']
        if feature:
            command += ['--features', feature]
        result = subprocess.run(command, check=True, capture_output=True, text=True)
        packages = {line.split()[0] for line in result.stdout.splitlines() if line.strip()}
        if 'up-rust' not in packages or ('sha2' in packages) != expected:
            raise RuntimeError(f'Unexpected native dependency graph for {feature or "default"}:\n{result.stdout}')
        print(f'PASS {feature or "default"}: runtime sha2={expected}; argv={command!r}')


if __name__ == '__main__':
    main()

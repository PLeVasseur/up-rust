#!/usr/bin/env python3
# Copyright (c) 2026 Contributors to the Eclipse Foundation
# See the NOTICE file(s) distributed with this work for copyright ownership.
# Licensed under the Apache License, Version 2.0.
# SPDX-License-Identifier: Apache-2.0
"""Execute one CI command with a bounded lifetime and nonempty test completion."""
import argparse
from pathlib import Path
import re
import signal
import subprocess
import sys


def completed(text, kind):
    text = re.sub(r'\x1b\[[0-9;]*[A-Za-z]', '', text)
    if kind == 'command':
        return True
    if kind == 'libtest':
        counts = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed;', text)
        return bool(counts) and sum(int(passed) for passed, _ in counts) > 0 and all(int(failed) == 0 for _, failed in counts)
    if kind == 'cucumber':
        return bool(re.search(r'\b[1-9]\d* scenarios?\b', text)) and not re.search(r'\b[1-9]\d* failed\b', text)
    raise ValueError('unknown completion format')


def execute(command, log, timeout, completion):
    log.parent.mkdir(parents=True, exist_ok=True)
    with log.open('wb') as output:
        child = subprocess.Popen(command, stdout=output, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            code = child.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            import os
            os.killpg(child.pid, signal.SIGTERM)
            try:
                child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(child.pid, signal.SIGKILL)
                child.wait()
            code = 124
    text = log.read_text(errors='replace')
    print(text, end='')
    if code == 0 and not completed(text, completion):
        print(f'Expected nonempty {completion} completion in {log}', file=sys.stderr)
        return 1
    return code


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--log', type=Path, required=True)
    parser.add_argument('--timeout', type=int, required=True)
    parser.add_argument('--completion', choices=['command', 'libtest', 'cucumber'], default='command')
    parser.add_argument('command', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ['--'] else args.command
    if not command or args.timeout <= 0:
        parser.error('a command and positive timeout are required')
    return execute(command, args.log, args.timeout, args.completion)


if __name__ == '__main__':
    raise SystemExit(main())

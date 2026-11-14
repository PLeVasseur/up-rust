#!/usr/bin/env python3
# Copyright (c) 2026 Contributors to the Eclipse Foundation
# See the NOTICE file(s) distributed with this work for copyright ownership.
# Licensed under the Apache License, Version 2.0.
# SPDX-License-Identifier: Apache-2.0
"""Execute one CI command with a bounded lifetime and nonempty test completion."""
import argparse
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import time


def completed(text, kind):
    text = re.sub(r'\x1b\[[0-9;]*[A-Za-z]', '', text)
    if kind == 'command':
        return True
    if kind in {'libtest', 'trybuild'}:
        announced = [int(count) for count in re.findall(r'^\s*running (\d+) tests?\s*$', text, re.M)]
        summaries = [tuple(map(int, row)) for row in re.findall(
            r'^\s*test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured;', text, re.M)]
        if not announced or len(announced) != len(summaries):
            return False
        if any(expected != sum(counts) or counts[1] for expected, counts in zip(announced, summaries)):
            return False
        if not any(counts[0] for counts in summaries):
            return False
        binaries = re.findall(r'^\s*Running (?:unittests |tests[/\\]).+$', text, re.M)
        if binaries and len(binaries) != len(summaries):
            return False
        if kind == 'trybuild':
            return bool(re.search(r'^test tests/ui/\S+(?: \[[^\]]+\])? \.\.\. ok\s*$', text, re.M))
        return True
    if kind == 'cucumber':
        return bool(re.search(r'\b[1-9]\d* scenarios?\b', text)) and not re.search(r'\b[1-9]\d* failed\b', text)
    raise ValueError('unknown completion format')


def signal_group(group, sig):
    """Signal only the session created for this check; tolerate concurrent exit."""
    try:
        os.killpg(group, sig)
        return True
    except ProcessLookupError:
        return False


def terminate_group(child):
    """Bound TERM grace for the whole group, even after its leader has exited."""
    signal_group(child.pid, signal.SIGTERM)
    deadline = time.monotonic() + 5
    while signal_group(child.pid, 0):
        child.poll()  # Reap the leader without treating its exit as group completion.
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            signal_group(child.pid, signal.SIGKILL)
            break
        time.sleep(min(0.05, remaining))
    child.wait(timeout=5)


def execute(command, log, timeout, completion):
    """Preserve command status, return 124 on timeout, and reject empty test passes."""
    started = time.monotonic()
    log.parent.mkdir(parents=True, exist_ok=True)
    with log.open('wb') as output:
        child = subprocess.Popen(command, stdout=output, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            code = child.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            terminate_group(child)
            code = 124
    text = log.read_text(errors='replace')
    print(text, end='')
    if code == 0 and not completed(text, completion):
        print(f'Expected nonempty {completion} completion in {log}', file=sys.stderr)
        code = 1
    print('CI_CHECK ' + json.dumps(dict(name=log.stem, seconds=time.monotonic() - started,
        timeout_seconds=timeout, exit_code=code, completion=completion)), flush=True)
    return code


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--log', type=Path, required=True)
    parser.add_argument('--timeout', type=int, required=True)
    parser.add_argument('--completion', choices=['command', 'libtest', 'trybuild', 'cucumber'], default='command')
    parser.add_argument('command', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ['--'] else args.command
    if not command or args.timeout <= 0:
        parser.error('a command and positive timeout are required')
    return execute(command, args.log, args.timeout, args.completion)


if __name__ == '__main__':
    raise SystemExit(main())

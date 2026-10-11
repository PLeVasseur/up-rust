# Copyright (c) 2026 Contributors to the Eclipse Foundation
# See the NOTICE file(s) distributed with this work for copyright ownership.
# Licensed under the Apache License, Version 2.0.
# SPDX-License-Identifier: Apache-2.0
"""Behavioral checks for generated gates, policy validation and command outcomes."""
import copy
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
import yaml

from generate import ROOT, load_policy, render
from run_check import completed, execute


class GateTests(unittest.TestCase):
    def test_render_is_deterministic_and_workflow_uses_the_same_groups(self):
        policy = load_policy(ROOT / 'tools/ci/policy.yaml')
        shell, text = render(policy)
        self.assertEqual((shell, text), render(policy))
        workflow = yaml.safe_load(text)
        for role in policy['toolchains']:
            groups = {check['group'] for check in policy['checks'] if role in check['roles']}
            for group in groups:
                job = workflow['jobs'][f'{role}-{group}']
                self.assertIn({'run': f'bash tools/ci/gate.sh --configuration {role} --group {group}'}, job['steps'])
        self.assertNotIn('CARGO_BUILD_JOBS', shell)
        self.assertNotIn('RUST_TEST_THREADS', shell)
        self.assertIn('test-results', text)
        self.assertIn('check-links', workflow['jobs'])
        self.assertIn('feature-check', workflow['jobs'])
        self.assertIn('run: |', text)
        self.assertIn('SPDX-License-Identifier: Apache-2.0', shell)
        for name in ['build', 'linter', 'api-docs', 'test']:
            self.assertIn(f'[impl->req~up-language-ci-{name}~1]', text)
        self.assertEqual(workflow['env']['RUSTFLAGS'], '-Dwarnings')
        self.assertEqual(workflow['jobs']['feature-check']['steps'][1]['with']['toolchain'], policy['toolchains']['current'])
        names = {check['name'] for check in policy['checks']}
        self.assertTrue({'lib-core', 'lib-default', 'lib-all', 'doctests-core', 'doctests-default', 'doctests-all'} <= names)

    def test_duplicate_policy_check_fails(self):
        policy = load_policy(ROOT / 'tools/ci/policy.yaml')
        policy['checks'].append(copy.deepcopy(policy['checks'][0]))
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'policy.yaml'
            path.write_text(yaml.safe_dump(policy))
            with self.assertRaises(ValueError):
                load_policy(path)

    def test_missing_or_zero_test_completion_fails(self):
        self.assertFalse(completed('running 12 tests\n', 'libtest'))
        self.assertFalse(completed('test result: ok. 0 passed; 0 failed;', 'libtest'))
        self.assertTrue(completed('test result: ok. 12 passed; 0 failed;', 'libtest'))
        self.assertFalse(completed('4 scenarios (1 failed)', 'cucumber'))
        self.assertTrue(completed('4 scenarios (4 passed)', 'cucumber'))

    def test_nonzero_exit_and_timeout_are_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / 'command.log'
            self.assertEqual(execute([sys.executable, '-c', 'raise SystemExit(7)'], log, 5, 'command'), 7)
            self.assertEqual(execute([sys.executable, '-c', 'import time; time.sleep(10)'], log, 1, 'command'), 124)

    def test_empty_success_is_not_a_test_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            self.assertEqual(execute([sys.executable, '-c', 'pass'], Path(directory) / 'empty.log', 5, 'libtest'), 1)

    def test_missing_required_script_is_not_skipped(self):
        with tempfile.TemporaryDirectory() as directory:
            absent = Path(directory) / 'required-check.py'
            self.assertEqual(execute([sys.executable, str(absent)], Path(directory) / 'missing.log', 5, 'command'), 2)

    @unittest.skipUnless(sys.platform == 'linux', 'Linux descendant state inspection')
    def test_timeout_kills_descendant_after_leader_exits(self):
        with tempfile.TemporaryDirectory() as directory:
            pid_file = Path(directory) / 'descendant.pid'
            child = ('import os,signal,time; from pathlib import Path; '
                     'signal.signal(signal.SIGTERM,signal.SIG_IGN); '
                     f'Path({str(pid_file)!r}).write_text(str(os.getpid())); time.sleep(60)')
            leader = ('import subprocess,sys,time; '
                      f'subprocess.Popen([sys.executable,"-c",{child!r}]); time.sleep(60)')
            try:
                started = time.monotonic()
                self.assertEqual(execute([sys.executable, '-c', leader], Path(directory) / 'timeout.log', 2, 'command'), 124)
                self.assertLess(time.monotonic() - started, 20, 'timeout cleanup must be bounded')
                pid = int(pid_file.read_text())
                stat = Path(f'/proc/{pid}/stat')
                # SIGKILL delivery and orphan reaping need not finish in the same
                # scheduler tick. A zombie has terminated and cannot keep working.
                def running():
                    try:
                        return stat.read_text().split()[2] != 'Z'
                    except FileNotFoundError:
                        return False
                deadline = time.monotonic() + 1
                while running() and time.monotonic() < deadline:
                    time.sleep(0.02)
                self.assertFalse(running(), 'TERM-resistant descendant survived its leader')
            finally:
                # Also clean up the deliberate parent-code negative control.
                if pid_file.exists():
                    try:
                        os.kill(int(pid_file.read_text()), 9)
                    except ProcessLookupError:
                        pass

    def test_current_skip_and_bad_options_fail_before_cargo(self):
        env = os.environ | {'UP_RUST_SKIP_TRYBUILD': '0'}
        command = ['bash', str(ROOT / 'tools/ci/gate.sh'), '--configuration', 'current', '--all']
        result = subprocess.run(command, env=env, capture_output=True, text=True)
        self.assertEqual(result.returncode, 2)
        self.assertIn('forbidden', result.stderr)
        for suffix in [['--all', '--group', 'check'], ['--group', 'absent']]:
            result = subprocess.run(command[:-1] + suffix, capture_output=True, text=True)
            self.assertEqual(result.returncode, 2)


if __name__ == '__main__':
    unittest.main(verbosity=2)

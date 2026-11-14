#!/usr/bin/env python3
# Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0
"""Fail-closed lock, coverage and subprocess controls for isolated feature checks."""
import os
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import feature_gate


class FeatureGateTests(unittest.TestCase):
    def test_cargo_hack_requires_an_explicitly_qualified_version(self):
        for version in ['cargo-hack 0.6.45', 'cargo-hack 0.6.46']:
            feature_gate.validate_cargo_hack(version)
        for version in ['cargo-hack 0.6.44', 'cargo-hack 0.6.47', 'cargo-hack 0.7.0',
                        'cargo-hack 0.6.46-dev', 'cargo-hack 0.6.46\nextra']:
            with self.subTest(version=version), self.assertRaises(ValueError):
                feature_gate.validate_cargo_hack(version)

    def exercise_observation_boundary(self, failed_phase=None, change_source=False):
        with tempfile.TemporaryDirectory() as folder:
            parent = Path(folder).resolve()
            root = parent / 'root'
            root.mkdir()
            lock = '[[package]]\nname = "example"\nversion = "1.0.0"\n'
            (root / 'Cargo.lock').write_text(lock)
            (root / 'Cargo.toml').write_text('[package]\nname = "example"\nversion = "1.0.0"\n')
            output = parent / 'result'

            def git(directory, *arguments):
                if arguments == ('rev-parse', 'HEAD'):
                    return 'spec' if directory.name == 'up-spec' else 'head'
                return {('write-tree',): 'tree', ('ls-files',): 'Cargo.lock\nCargo.toml',
                        ('ls-files', '--stage', 'up-spec'): '160000 spec 0\tup-spec'}.get(arguments, '')

            def export(directory, tree, destination):
                destination.mkdir(parents=True)
                if directory == root:
                    (destination / 'Cargo.lock').write_text(lock)
                    (destination / 'Cargo.toml').write_text((root / 'Cargo.toml').read_text())

            def invoke(command, cwd, env, log, timeout):
                if '--feature-powerset' in command:
                    log.write('case (1/1)\n')
                    if change_source:
                        (root / 'Cargo.toml').write_text('changed during isolated execution')
                return 7 if failed_phase and failed_phase in command else 0

            arguments = ['feature_gate.py', '--root', str(root), '--target', str(parent / 'target'),
                         '--result', str(output), '--toolchain', 'stable']
            clean_environment = {key: value for key, value in os.environ.items()
                                 if key not in {'RUSTC_BOOTSTRAP', 'UP_RUST_SKIP_TRYBUILD'}}
            with patch.dict(os.environ, clean_environment, clear=True), \
                    patch.object(sys, 'argv', arguments), \
                    patch.object(feature_gate, 'git', side_effect=git), \
                    patch.object(feature_gate, 'export_tree', side_effect=export), \
                    patch.object(feature_gate, 'invoke', side_effect=invoke), \
                    patch.object(feature_gate.subprocess, 'check_output',
                                 side_effect=['rustc fixture', 'cargo-hack 0.6.46']), \
                    patch.object(feature_gate, 'source_files', wraps=feature_gate.source_files) as reads:
                if failed_phase or change_source:
                    with self.assertRaises((RuntimeError, ValueError)):
                        feature_gate.main()
                else:
                    feature_gate.main()
                self.assertEqual(reads.call_count, 2)
            result = json.loads((output / 'RESULT.json').read_text())
            self.assertEqual(result['source_exit_check'], 'FAIL' if change_source else 'PASS')
            self.assertEqual(result['result'], 'FAIL' if failed_phase or change_source
                             else 'PASS_ISOLATED_LOCKED_FEATURE_POWERSET')

    def test_success_observes_source_content_only_at_entry_and_exit(self):
        self.exercise_observation_boundary()

    def test_failed_phase_still_observes_source_at_exit(self):
        for phase in ['--remove-dev-deps', 'metadata', '--feature-powerset']:
            with self.subTest(phase=phase):
                self.exercise_observation_boundary(failed_phase=phase)

    def test_exit_rejects_content_drift_with_unchanged_git_identity(self):
        self.exercise_observation_boundary(change_source=True)

    def test_derived_lock_accepts_removals_but_rejects_identity_drift(self):
        original = ('dependency', '1.2.3', 'registry+example', 'checksum')
        before = {original, ('dev-only', '2.0.0', None, None)}
        self.assertEqual(feature_gate.audit_lock(before, {original}), 1)
        for index, replacement in enumerate(['new-name', '1.2.4', 'registry+other', 'different-checksum']):
            changed = list(original)
            changed[index] = replacement
            with self.subTest(field=index), self.assertRaises(ValueError):
                feature_gate.audit_lock(before, {tuple(changed)})

    def test_progress_requires_each_case_once_with_one_denominator(self):
        self.assertEqual(feature_gate.audit_progress('case (1/3)\ncase (2/3)\ncase (3/3)\n'), 3)
        for text in ['', '(0/0)', '(1/2)', '(1/2) (1/2) (2/2)', '(1/2) (2/3) (3/3)', '(2/2) (1/2)']:
            with self.subTest(text=text), self.assertRaises(ValueError):
                feature_gate.audit_progress(text)

    def test_source_bytes_are_checked_even_if_git_identity_is_stable(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            path = root / 'input.rs'
            path.write_text('original')
            original = {'input.rs': feature_gate.sha(path)}
            def git(directory, *args):
                if args == ('rev-parse', 'HEAD'):
                    return 'spec' if directory.name == 'up-spec' else 'head'
                return {('write-tree',): 'tree', ('ls-files',): 'input.rs'}.get(args, '')
            with patch.object(feature_gate, 'git', side_effect=git):
                feature_gate.verify_source(root, 'head', 'tree', original, 'spec')
                path.write_text('changed')
                with self.assertRaises(ValueError):
                    feature_gate.verify_source(root, 'head', 'tree', original, 'spec')

    def test_invocation_preserves_failures_and_cleans_timed_out_commands(self):
        with tempfile.TemporaryDirectory() as folder:
            with (Path(folder) / 'nonzero.log').open('w') as log:
                self.assertEqual(feature_gate.invoke([sys.executable, '-c', 'raise SystemExit(7)'], folder, os.environ.copy(), log, 5), 7)
            with (Path(folder) / 'timeout.log').open('w') as log:
                with self.assertRaises(subprocess.TimeoutExpired):
                    feature_gate.invoke([sys.executable, '-c', 'import time; time.sleep(60)'], folder, os.environ.copy(), log, 0.2)


if __name__ == '__main__':
    unittest.main()

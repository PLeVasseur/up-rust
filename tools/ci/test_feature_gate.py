#!/usr/bin/env python3
# Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0
"""Fail-closed lock, coverage and subprocess controls for isolated feature checks."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import feature_gate


class FeatureGateTests(unittest.TestCase):
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

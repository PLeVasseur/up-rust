import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

import yaml

from generate import ROOT, SETUP, UPLOAD, load_policy, render


class ToolchainTests(unittest.TestCase):
    def test_workflow_contract_and_upstream_pins(self):
        policy = load_policy(ROOT / 'tools/ci/policy.yaml')
        workflow = yaml.safe_load(render(policy)[1])
        selected = workflow['env']['RUST_TOOLCHAIN']
        triggers = workflow.get('on', workflow.get(True))
        for event in ('workflow_call', 'workflow_dispatch'):
            self.assertEqual(triggers[event]['inputs']['RUST_TOOLCHAIN']['default'], 'stable')
        for role, expected in [('current', selected), ('msrv', policy['toolchains']['msrv'])]:
            job = workflow['jobs'][role + '-check']
            self.assertEqual(job['steps'][1]['with']['toolchain'], expected)
            self.assertEqual(job['env']['RUSTUP_TOOLCHAIN'], expected)
        self.assertEqual(workflow['jobs']['feature-check']['env']['RUSTUP_TOOLCHAIN'], selected)
        self.assertIn('ecabd13d1c56bd1345c230e542e9144811ad706f', SETUP)
        self.assertIn('cf430e030ddbb5b0abf93d22962f4752f3646cd9', UPLOAD)
        self.assertIn('/releases/latest/download/', render(policy)[1])
        self.assertIn('gh attestation verify', render(policy)[1])
        self.assertIn('jobs.current-tests.outputs.test_results_url', render(policy)[1])
        release = yaml.safe_load((ROOT / '.github/workflows/release.yaml').read_text())
        self.assertIn('inputs.RUST_TOOLCHAIN', release['jobs']['check']['with']['RUST_TOOLCHAIN'])

    def test_selection_reaches_cargo_and_msrv_is_independent(self):
        policy = load_policy(ROOT / 'tools/ci/policy.yaml')
        for role, requested, expected in [('current', '', 'stable'),
                ('current', '1.95.0', '1.95.0'),
                ('current', 'nightly-2026-01-01', 'nightly-2026-01-01'),
                ('msrv', '1.95.0', policy['toolchains']['msrv'])]:
            with self.subTest(role=role, requested=requested):
                result, calls = self.run_gate(role, requested)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertTrue(calls)
                for call in calls:
                    self.assertEqual(call['argv'][0], '+' + expected)
                    self.assertEqual(call['selected'], expected)

    def test_feature_toolchain_is_a_quoted_runtime_argument(self):
        workflow = yaml.safe_load(render(load_policy(ROOT / 'tools/ci/policy.yaml'))[1])
        feature = workflow['jobs']['feature-check']
        step = next(step for step in feature['steps']
                    if step.get('name') == 'Run isolated locked feature powerset')
        self.assertIn('--toolchain "$RUSTUP_TOOLCHAIN"', step['run'])
        self.assertNotIn('${{', step['run'])
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / 'python3'
            binary.write_text(f'#!{sys.executable}\nimport json,os,sys\n'
                              'open(os.environ["FEATURE_CALL"],"w").write(json.dumps(sys.argv[1:]))\n')
            binary.chmod(0o755)
            call = root / 'call.json'
            selected = 'stable; touch SHOULD_NOT_EXIST'
            environment = os.environ | {'PATH': str(root) + os.pathsep + os.environ['PATH'],
                'FEATURE_CALL': str(call), 'RUSTUP_TOOLCHAIN': selected,
                'GITHUB_WORKSPACE': str(root), 'RUNNER_TEMP': str(root / 'temporary')}
            subprocess.run(['bash', '-eu', '-c', step['run']], cwd=root, env=environment,
                           check=True, timeout=10)
            arguments = json.loads(call.read_text())
            self.assertEqual(arguments[arguments.index('--toolchain') + 1], selected)
            self.assertFalse((root / 'SHOULD_NOT_EXIST').exists())

    def test_invalid_selection_fails_before_commands(self):
        for requested in ('-V', 'stable;touch bad', '$(touch bad)', 'stable\nnightly'):
            with self.subTest(requested=requested):
                result, calls = self.run_gate('current', requested)
                self.assertEqual(result.returncode, 2)
                self.assertIn('Invalid Rust toolchain', result.stderr)
                self.assertFalse(calls)

    def run_gate(self, role, requested):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            scripts = root / 'tools/ci'
            scripts.mkdir(parents=True)
            policy = load_policy(ROOT / 'tools/ci/policy.yaml')
            policy['checks'] = [check for check in policy['checks'] if check['command'][0] == 'cargo']
            (scripts / 'gate.sh').write_text(render(policy)[0])
            shutil.copyfile(ROOT / 'tools/ci/run_check.py', scripts / 'run_check.py')
            binaries = root / 'bin'
            binaries.mkdir()
            fake = ('#!/usr/bin/env python3\nimport json,os,sys\n'
                    'if len(sys.argv)>1 and sys.argv[1].startswith("+"):\n'
                    ' with open(os.environ["TOOLCHAIN_CALLS"],"a") as output:\n'
                    '  output.write(json.dumps({"argv":sys.argv[1:],"selected":os.environ.get("RUSTUP_TOOLCHAIN")})+"\\n")\n'
                    'print("fixture version")\n')
            for name in ('cargo', 'rustc'):
                path = binaries / name
                path.write_text(fake)
                path.chmod(0o755)
            log = root / 'calls.jsonl'
            environment = os.environ | {'PATH': str(binaries) + os.pathsep + os.environ['PATH'],
                'RUST_TOOLCHAIN': requested, 'TOOLCHAIN_CALLS': str(log)}
            for key in ('UP_RUST_SKIP_TRYBUILD', 'RUSTC_BOOTSTRAP', 'CARGO_TARGET_DIR'):
                environment.pop(key, None)
            result = subprocess.run(['bash', str(scripts / 'gate.sh'), '--configuration', role,
                '--group', 'check'], cwd=root, env=environment, capture_output=True, text=True, timeout=10)
            return result, [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []

#!/usr/bin/env python3
# Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0
"""Check isolated feature combinations with an audited, seeded offline lock."""
import argparse
from datetime import datetime, timezone
import hashlib
import io
import json
import os
from pathlib import Path
import re
import subprocess
import tarfile
import time
import tomllib

from run_check import terminate_group


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def git(root, *args):
    return subprocess.check_output(['git', '-C', str(root), *args], text=True).strip()


def source_files(root):
    return {name: sha(root / name) for name in git(root, 'ls-files').splitlines() if (root / name).is_file()}


def verify_source_identity(root, head, tree, spec):
    if (git(root, 'rev-parse', 'HEAD') != head or git(root, 'write-tree') != tree
            or git(root, 'diff', '--name-only') or git(root, 'ls-files', '--others', '--exclude-standard')
            or git(root / 'up-spec', 'rev-parse', 'HEAD') != spec
            or git(root / 'up-spec', 'status', '--porcelain')):
        raise ValueError('owning source or specification changed during feature checks')


def verify_source(root, head, tree, files, spec):
    verify_source_identity(root, head, tree, spec)
    if source_files(root) != files:
        raise ValueError('owning source or specification changed during feature checks')


def validate_cargo_hack(version):
    if version not in {'cargo-hack 0.6.45', 'cargo-hack 0.6.46'}:
        raise ValueError('use a qualified cargo-hack version: 0.6.45 or 0.6.46')


def export_tree(root, tree, output):
    output.mkdir(parents=True, exist_ok=True)
    data = subprocess.check_output(['git', '-C', str(root), 'archive', tree])
    with tarfile.open(fileobj=io.BytesIO(data)) as archive:
        archive.extractall(output, filter='data')


def package_identities(lock):
    return {(p['name'], p['version'], p.get('source'), p.get('checksum'))
            for p in tomllib.loads(lock.read_text())['package']}


def audit_lock(before, after):
    if not after <= before:
        raise ValueError('derived lock introduced a package/version/source/checksum')
    return len(before - after)


def audit_progress(text):
    progress = [(int(n), int(total)) for n, total in re.findall(r'\((\d+)/(\d+)\)', text)]
    if not progress:
        raise ValueError('feature powerset has no progress inventory')
    total = progress[0][1]
    if total <= 0 or progress != [(n, total) for n in range(1, total + 1)]:
        raise ValueError('incomplete, duplicate or inconsistent feature powerset')
    return total


def invoke(command, cwd, env, log, timeout):
    process = subprocess.Popen(command, cwd=cwd, env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
    try:
        return process.wait(timeout=timeout)
    except (subprocess.TimeoutExpired, KeyboardInterrupt):
        terminate_group(process)
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--target', type=Path, required=True)
    parser.add_argument('--result', type=Path, required=True)
    parser.add_argument('--toolchain', required=True)
    args = parser.parse_args()
    if 'RUSTC_BOOTSTRAP' in os.environ or 'UP_RUST_SKIP_TRYBUILD' in os.environ:
        raise ValueError('compiler/diagnostic overrides are not accepted by the current feature gate')
    root, target, output = args.root.resolve(), args.target.resolve(), args.result.resolve()
    if output.exists() or output.is_relative_to(root) or root.is_relative_to(output):
        raise ValueError('use a fresh result directory outside the source checkout')
    if target == root or root.is_relative_to(target) or target == output or output.is_relative_to(target) or target.is_relative_to(output):
        raise ValueError('target must be separate from source and result roots')
    if git(root, 'diff', '--name-only') or git(root, 'ls-files', '--others', '--exclude-standard'):
        raise ValueError('source index/worktree must match')
    tree, head = git(root, 'write-tree'), git(root, 'rev-parse', 'HEAD')
    files = source_files(root)
    spec = git(root, 'ls-files', '--stage', 'up-spec').split()[1]
    verify_source_identity(root, head, tree, spec)
    before = package_identities(root / 'Cargo.lock')
    output.mkdir(parents=True)
    snapshot = output / 'source'
    env = os.environ | {'CARGO_TARGET_DIR': str(target), 'CARGO_NET_OFFLINE': 'true',
                        'RUSTFLAGS': '-Dwarnings', 'CARGO_TERM_COLOR': 'never'}
    environment_keys = ['CARGO_TARGET_DIR', 'CARGO_NET_OFFLINE', 'CARGO_BUILD_JOBS', 'CARGO_INCREMENTAL',
                        'CARGO_PROFILE_DEV_DEBUG', 'CARGO_PROFILE_TEST_DEBUG', 'CARGO_TERM_COLOR',
                        'RUSTFLAGS', 'RUSTDOCFLAGS', 'RUST_TEST_THREADS']
    record = dict(result='IN_PROGRESS', head=head, index_tree=tree, spec_gitlink=spec,
        source_files=files, source_lock_sha256=files['Cargo.lock'], snapshot=str(snapshot),
        runner_sha256=sha(Path(__file__)), timeout_helper_sha256=sha(Path(__file__).with_name('run_check.py')),
        toolchain=args.toolchain, started_at=datetime.now(timezone.utc).isoformat(), checks=[],
        environment={key: env[key] for key in environment_keys if key in env},
        source_observation_boundary='one entry content pass and one exit content pass; isolated snapshot execution')

    def save():
        temporary = output / 'RESULT.json.tmp'
        with temporary.open('w') as receipt:
            json.dump(record, receipt, indent=2)
            receipt.write('\n')
            receipt.flush()
            os.fsync(receipt.fileno())
        os.replace(temporary, output / 'RESULT.json')

    def run(name, command):
        path = output / (name + '.log')
        started = datetime.now(timezone.utc).isoformat()
        begin = time.monotonic()
        error = None
        with path.open('x') as log:
            try:
                code = invoke(command, snapshot, env, log, 7200)
            except (OSError, subprocess.TimeoutExpired, KeyboardInterrupt) as failure:
                code = 124 if isinstance(failure, subprocess.TimeoutExpired) else 130 if isinstance(failure, KeyboardInterrupt) else 127
                error = str(failure) or 'interrupted'
        record['checks'].append(dict(name=name, command=command, cwd=str(snapshot), exit_code=code,
            error=error, started_at=started, finished_at=datetime.now(timezone.utc).isoformat(),
            seconds=time.monotonic() - begin, log_sha256=sha(path)))
        save()
        if code:
            raise RuntimeError('feature gate failed: ' + name)
        return path

    save()
    try:
        record['compiler'] = subprocess.check_output(['rustc', '+' + args.toolchain, '-vV'], text=True)
        record['cargo_hack'] = subprocess.check_output(['cargo', '+' + args.toolchain, 'hack', '--version'], text=True).strip()
        validate_cargo_hack(record['cargo_hack'])
        export_tree(root, tree, snapshot)
        export_tree(root / 'up-spec', spec, snapshot / 'up-spec')
        if sha(snapshot / 'Cargo.lock') != files['Cargo.lock']:
            raise ValueError('snapshot must start with the exact owning lock')
        run('remove-dev-dependencies', ['cargo', '+' + args.toolchain, 'hack', '--remove-dev-deps'])
        # Only this disposable pruned snapshot gets an unlocked setup resolution.
        # Its existing lock seeds offline resolution, and every identity is audited.
        run('derive-pruned-lock', ['cargo', '+' + args.toolchain, 'metadata', '--offline', '--format-version=1'])
        record['removed_package_count'] = audit_lock(before, package_identities(snapshot / 'Cargo.lock'))
        record['derived_lock_sha256'] = sha(snapshot / 'Cargo.lock')
        manifests = {str(path.relative_to(snapshot)): sha(path) for path in snapshot.rglob('Cargo.toml')}
        record['pruned_manifests'] = manifests
        save()
        log = run('feature-powerset', ['cargo', '+' + args.toolchain, 'hack', 'check', '--locked', '--offline',
            '-p', 'up-rust', '--no-dev-deps', '--feature-powerset', '--group-features',
            'communication,usubscription,udiscovery,symphony', '--depth', '2'])
        cases = audit_progress(log.read_text())
        if sha(snapshot / 'Cargo.lock') != record['derived_lock_sha256']:
            raise ValueError('locked feature checks changed their derived lock')
        if any(sha(snapshot / name) != digest for name, digest in manifests.items()):
            raise ValueError('feature checks changed a pruned manifest')
        record.update(result='PASS_ISOLATED_LOCKED_FEATURE_POWERSET', cases=cases)
    except (Exception, KeyboardInterrupt) as error:
        record.update(result='FAIL', error=str(error) or 'interrupted')
        raise
    finally:
        try:
            verify_source(root, head, tree, files, spec)
            record['source_exit_check'] = 'PASS'
        except Exception as source_error:
            record.update(result='FAIL', source_exit_check='FAIL', source_exit_error=str(source_error))
            raise
        finally:
            record['finished_at'] = datetime.now(timezone.utc).isoformat()
            save()
    print(json.dumps(dict(result=record['result'], cases=cases, receipt=str(output / 'RESULT.json')), indent=2))


if __name__ == '__main__':
    main()

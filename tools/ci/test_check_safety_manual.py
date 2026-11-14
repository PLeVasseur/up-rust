# SPDX-License-Identifier: Apache-2.0
"""Tiny independent document fixtures; no SDK compiler, OFT or native inputs."""
import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).with_name('check_safety_manual.py')
spec = importlib.util.spec_from_file_location('safety_manual_checker_under_test', SCRIPT)
checker = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = checker
spec.loader.exec_module(checker)

# Independent ownership/revision oracle, not generated from checker constants.
REVISIONS = {
    'supervisor-restart': 3, 'process-isolation': 1, 'integrity-levels': 4,
    'peer-access': 4, 'timing-budgets': 5, 'clock': 2, 'waiver-approval': 2,
    'e2e-protection': 6, 'qualified-bindings-and-wires': 4,
    'target-environment': 3, 'build-configuration': 4, 'deployment-data': 5,
    'capacity-sizing': 1, 'application-reacts': 3, 'integrator-platform': 3,
    'peer-rates': 1, 'build-order': 2,
}


def source(name, revision):
    return (f'[.specitem,oft-sid="aou~seooc-{name}~{revision}",oft-title="{name}",'
            'oft-needs="uman",oft-tags="SafetyManual"]\n--\n'
            f'The integrator *SHALL* check {name}.\n\n* Retain the result.\n--\n')


def manual(name, revision):
    return f'''# {name}

`uman~sdk-aou-{name}~1`

Covers:
- aou~seooc-{name}~{revision}

## Status and scope

Fixture documentation only.

## Assumption (verbatim)

<!-- aou-statement:start -->
> The integrator *SHALL* check {name}.
>
> * Retain the result.
<!-- aou-statement:end -->

## Integrator actions

- [ ] Inspect the deployment and retain its result.

## Violation manifestations

The configured assumption can be violated.

## Detection limits

This fixture is no product qualification.
'''


class SafetyManualTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='manual-check-')
        self.addCleanup(self.temp.cleanup)
        root = Path(self.temp.name)
        self.s = root / 'spec'
        self.m = root / 'manual'
        self.s.mkdir()
        self.m.mkdir()
        all_revisions = dict(REVISIONS, **{'monitor-thresholds': 2, 'operator-access': 2})
        self.aou = self.s / 'assumptions.adoc'
        self.aou.write_text('\n'.join(source(k, v) for k, v in all_revisions.items()))
        for name, revision in REVISIONS.items():
            (self.m / f'{name}.md').write_text(manual(name, revision))
        self.one = self.m / 'clock.md'

    def replace(self, path, before, after):
        text = path.read_text()
        self.assertIn(before, text)
        path.write_text(text.replace(before, after, 1))

    def fails(self, diagnostic):
        with self.assertRaisesRegex(checker.Invalid, diagnostic):
            checker.check(self.s, self.m)

    def test_positive_exact_17_2_with_multiline_quote(self):
        result = checker.check(self.s, self.m)
        self.assertEqual((result['sdk_entries'], result['streamer_excluded']), (17, 2))
        self.assertEqual(result['files_read'], 18)

    def test_positive_covers_word_inside_statement(self):
        for path in (self.aou, self.one):
            self.replace(path, 'check clock.', 'check clock; the data covers: values.')
        checker.check(self.s, self.m)

    def test_positive_coordinated_revision_and_text_change(self):
        for path in (self.aou, self.one):
            self.replace(path, 'aou~seooc-clock~2', 'aou~seooc-clock~3')
            self.replace(path, 'check clock.', 'check both clocks.')
        checker.check(self.s, self.m)

    def test_positive_crlf_delimiters(self):
        self.one.write_bytes(self.one.read_bytes().replace(b'\n', b'\r\n'))
        checker.check(self.s, self.m)

    def test_same_revision_spec_wording_drift(self):
        self.replace(self.aou, 'check clock.', 'check both clocks.')
        self.fails('STATEMENT_MISMATCH')

    def test_same_revision_manual_wording_drift(self):
        self.replace(self.one, '*SHALL*', '*SHOULD*')
        self.fails('STATEMENT_MISMATCH')

    def test_whitespace_is_not_normalized(self):
        self.replace(self.one, 'check clock.', 'check  clock.')
        self.fails('STATEMENT_MISMATCH')

    def test_missing_quote_bullet(self):
        self.replace(self.one, '> * Retain the result.\n', '')
        self.fails('STATEMENT_MISMATCH')

    def test_spec_revision_drift(self):
        self.replace(self.aou, 'aou~seooc-clock~2', 'aou~seooc-clock~3')
        self.fails('REVISION_MISMATCH')

    def test_manual_revision_drift(self):
        self.replace(self.one, 'aou~seooc-clock~2', 'aou~seooc-clock~1')
        self.fails('REVISION_MISMATCH')

    def test_missing_manual(self):
        self.one.unlink()
        self.fails('MANUAL_MISSING')

    def test_duplicate_manual(self):
        (self.m / 'duplicate.md').write_text(self.one.read_text())
        self.fails('MANUAL_DUPLICATE')

    def test_missing_spec_aou(self):
        self.replace(self.aou, source('clock', 2), '')
        self.fails('OWNERSHIP_SPEC')

    def test_new_spec_aou_requires_explicit_ownership(self):
        with self.aou.open('a') as stream:
            stream.write(source('new-duty', 1))
        self.fails('OWNERSHIP_SPEC')

    def test_spec_aou_moved_to_other_file_still_detected(self):
        self.replace(self.aou, source('clock', 2), '')
        (self.s / 'other.adoc').write_text(source('clock', 2))
        checker.check(self.s, self.m)

    def test_duplicate_spec_aou_other_file(self):
        (self.s / 'other.adoc').write_text(source('clock', 2))
        self.fails('SPEC_DUPLICATE')

    def test_duplicate_spec_family_different_revision(self):
        (self.s / 'other.adoc').write_text(source('clock', 3))
        self.fails('SPEC_DUPLICATE')

    def test_streamer_manual_forbidden(self):
        (self.m / 'operator-access.md').write_text(manual('operator-access', 2))
        self.fails('OWNERSHIP_MANUAL')

    def test_streamer_revision_drift(self):
        self.replace(self.aou, 'aou~seooc-monitor-thresholds~2', 'aou~seooc-monitor-thresholds~3')
        self.fails('OWNERSHIP_STREAMER_REVISION')

    def test_aou_utest_need_forbidden(self):
        self.replace(self.aou, 'oft-needs="uman"', 'oft-needs="uman,utest"')
        self.fails('OWNERSHIP_NEEDS')

    def test_aou_missing_allocation_tag(self):
        self.replace(self.aou, 'oft-tags="SafetyManual"', 'oft-tags="SafetyUpRustCore"')
        self.fails('OWNERSHIP_NEEDS')

    def test_utest_link_in_manual_forbidden(self):
        with self.one.open('a') as stream:
            stream.write('\n[' + 'utest' + '->' + 'aou~seooc-clock~2]\n')
        self.fails('MANUAL_LINK_TYPE')

    def test_impl_link_in_manual_forbidden(self):
        with self.one.open('a') as stream:
            stream.write('\n[' + 'impl' + '->' + 'aou~seooc-clock~2]\n')
        self.fails('MANUAL_LINK_TYPE')

    def test_wrong_item_kind(self):
        self.replace(self.one, '`uman~', '`utest~')
        self.fails('MANUAL_ID')

    def test_duplicate_covers(self):
        self.replace(self.one, 'Covers:\n- aou~seooc-clock~2',
                     'Covers:\n- aou~seooc-clock~2\nCovers:\n- aou~seooc-clock~2')
        self.fails('MANUAL_COVERS')

    def test_two_ids_in_one_covers(self):
        self.replace(self.one, 'Covers:\n- aou~seooc-clock~2',
                     'Covers:\n- aou~seooc-clock~2, aou~seooc-build-order~2')
        self.fails('MANUAL_COVERS')

    def test_malformed_covers(self):
        self.replace(self.one, 'Covers:', 'Covers :')
        self.fails('MANUAL_COVERS')

    def test_hidden_fenced_item(self):
        self.one.write_text('```\n' + self.one.read_text() + '```\n')
        self.fails('MANUAL_MARKUP')

    def test_comment_hidden_metadata(self):
        self.replace(self.one, '`uman~', '<!--\n`uman~')
        self.fails('MANUAL_MARKUP')

    def test_malformed_quote_line(self):
        self.replace(self.one, '> The integrator', 'The integrator')
        self.fails('MANUAL_QUOTE')

    def test_missing_quote_end(self):
        self.replace(self.one, '<!-- aou-statement:end -->', '')
        self.fails('MANUAL_QUOTE')

    def test_empty_detection_section(self):
        self.replace(self.one, 'This fixture is no product qualification.', '')
        self.fails('MANUAL_SECTIONS')

    def test_missing_checklist(self):
        self.replace(self.one, '- [ ] Inspect', 'Inspect')
        self.fails('MANUAL_CHECKLIST')

    def test_missing_spec_delimiter(self):
        self.replace(self.aou, '\n--\n', '\n')
        self.fails('SPEC_MARKUP')

    def test_duplicate_spec_attribute(self):
        self.replace(self.aou, 'oft-needs="uman"', 'oft-needs="uman",oft-needs="uman"')
        self.fails('SPEC_MARKUP')

    def test_commented_spec_block(self):
        self.aou.write_text('////\n' + self.aou.read_text() + '////\n')
        self.fails('SPEC_MARKUP')

    def test_readme_cannot_hide_coverage(self):
        (self.m / 'README.md').write_text(self.one.read_text())
        self.fails('MANUAL_README')

    def test_unrecognized_file_not_silently_skipped(self):
        self.one.rename(self.m / 'clock.txt')
        self.fails('INPUT_FORMAT')

    def test_symlink_not_followed(self):
        (self.m / 'alias.md').symlink_to(self.one.name)
        self.fails('INPUT_SYMLINK')

    def test_size_limit_fail_closed(self):
        self.one.write_text('x' * (checker.MAX_FILE_BYTES + 1))
        self.fails('INPUT_BUDGET')

    def test_cli_success_and_failure_are_real_exit_codes(self):
        command = [sys.executable, '-B', str(SCRIPT), '--spec-root', str(self.s),
                   '--manual-root', str(self.m)]
        result = subprocess.run(command, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.replace(self.one, 'check clock.', 'check wrong clock.')
        result = subprocess.run(command, capture_output=True, text=True)
        self.assertEqual(result.returncode, 1)
        self.assertIn('STATEMENT_MISMATCH', result.stderr)

    def test_cli_missing_spec_fails(self):
        result = subprocess.run([sys.executable, '-B', str(SCRIPT), '--spec-root',
                                 str(self.s / 'missing'), '--manual-root', str(self.m)],
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 1)
        self.assertIn('INPUT_ROOT', result.stderr)


if __name__ == '__main__':
    unittest.main()

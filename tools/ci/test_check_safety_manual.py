# SPDX-License-Identifier: Apache-2.0
"""Tiny independent document fixtures; no SDK compiler, OFT or native inputs."""
import ast
import dataclasses
import importlib.util
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).with_name('check_safety_manual.py')
spec = importlib.util.spec_from_file_location('safety_manual_checker_under_test', SCRIPT)
checker = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = checker
spec.loader.exec_module(checker)

SDK, STREAMER, ICEORYX2 = 'up-rust', 'up-streamer-rust', 'up-transport-iceoryx2-rust'
AOU, DEPLOYMENT = 'seooc-up-rust/assumptions-of-use.adoc', 'up-l1/deployment.adoc'
ICEORYX2_SPEC, STREAMER_SPEC = 'up-l1/iceoryx2.adoc', 'up-l2/streamer.adoc'
# Independent ownership/revision oracle typed from ownership registry v2, not
# generated from checker constants: AoU revision, owner, manual item revision, file.
ORACLE = {
    'supervisor-restart': (4, SDK, 2, AOU),
    'process-isolation': (3, SDK, 2, AOU),
    'integrity-levels': (6, SDK, 3, DEPLOYMENT),
    'capability-ceiling': (1, SDK, 1, AOU),
    'peer-access': (5, SDK, 2, DEPLOYMENT),
    'timing-budgets': (7, SDK, 3, AOU),
    'clock': (3, SDK, 2, AOU),
    'waiver-approval': (3, SDK, 2, AOU),
    'e2e-protection': (7, SDK, 2, AOU),
    'qualified-bindings-and-wires': (7, SDK, 4, DEPLOYMENT),
    'resource-guarantees': (1, SDK, 1, AOU),
    'target-environment': (4, SDK, 1, AOU),
    'build-configuration': (5, SDK, 2, AOU),
    'deployment-data': (6, SDK, 3, AOU),
    'capacity-sizing': (3, SDK, 2, AOU),
    'application-reacts': (4, SDK, 2, AOU),
    'integrator-platform': (4, SDK, 2, AOU),
    'peer-rates': (2, SDK, 1, AOU),
    'build-order': (2, SDK, 1, AOU),
    'operator-access': (2, STREAMER, None, STREAMER_SPEC),
    'monitor-thresholds': (2, STREAMER, None, STREAMER_SPEC),
    'iceoryx2-level-separation': (1, ICEORYX2, 1, ICEORYX2_SPEC),
    'iceoryx2-limits': (1, ICEORYX2, 1, ICEORYX2_SPEC),
}
# The four named exclusions of registry v2, typed from its checker_policy block:
# product-neutral reasons and destinations, compared as exact strings.
STREAMER_REASON = ('The Streamer owns this assumption. Its safety-manual entry belongs in the '
                   'safety manual of up-streamer-rust, and the up-rust safety manual has no '
                   'entry for it.')
ICEORYX2_REASON = ('The iceoryx2 binding owns this assumption, which binds only a deployment '
                   'that uses that binding. Its safety-manual entry belongs in the safety manual '
                   'of up-transport-iceoryx2-rust, and the up-rust safety manual has no entry '
                   'for it.')
EXCLUSIONS = {
    'operator-access': (STREAMER_REASON, 'up-streamer-rust, its safety manual'),
    'monitor-thresholds': (STREAMER_REASON, 'up-streamer-rust, its safety manual'),
    'iceoryx2-level-separation': (ICEORYX2_REASON, 'up-transport-iceoryx2-rust, its safety '
                                  'manual: safety-manual/iceoryx2-level-separation.md'),
    'iceoryx2-limits': (ICEORYX2_REASON, 'up-transport-iceoryx2-rust, its safety manual: '
                        'safety-manual/iceoryx2-limits.md'),
}
# The anchor declaration spellings that CHECKER-HANDOFF.md section 5.2 names as
# failures, apart from the supported whole line [[faulty-peer]].
UNSUPPORTED_SPELLINGS = (
    '[[faulty-peer,the faulty-peer model]]', '[[faulty-peer,]]',
    '[#faulty-peer]', '[#faulty-peer.model]', '[#faulty-peer%autowidth]',
    '[#faulty-peer,role=model]', 'anchor:faulty-peer[]', 'anchor:faulty-peer[the model]',
    '[id=faulty-peer]', '[id="faulty-peer"]', '[id=faulty-peer,reftext=model]',
    '[id="faulty-peer",reftext="model"]',
)
# The negative grammar of CHECKER-HANDOFF.md section 5.1, one line each.
NEGATIVE_LINES = ('////', '----', '....', '++++', '```', '```rust', '````', '~~~', '~~~~',
                  'include::not-read.adoc[]', 'ifdef::missing[]', 'ifndef::missing[]',
                  'ifeval::[{level} > 1]', 'endif::missing[]', 'endif::[]')
README = ('= Safety\n\nAt this revision <<faulty-peer>> is the one normative definition.\n\n'
          '[[faulty-peer]]\n=== The faulty-peer model\n\nA faulty peer may write any value.\n')
REQUIREMENT = ('[.specitem,oft-sid="req~ssr-binding-fixture~2",oft-title="fixture",'
               'oft-needs="impl,utest",oft-tags="SafetyBinding"]\n--\n'
               'Each binding *SHALL* keep the fixture, as '
               'xref:../README.adoc#faulty-peer[the faulty-peer model] says.\n--\n')


def tags(owner):
    return 'SafetyManual,SafetyGateUpRust' if owner == SDK else 'SafetyManual'


def source(name, revision, tag_list):
    return (f'[.specitem,oft-sid="aou~seooc-{name}~{revision}",oft-title="{name}",'
            f'oft-needs="uman",oft-tags="{tag_list}"]\n--\n'
            f'The integrator *SHALL* check {name}.\n\n* Retain the result.\n--\n')


def manual(name, revision, prefix='sdk', entry=1):
    return f'''# {name}

`uman~{prefix}-aou-{name}~{entry}`

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
        self.round = 0
        self.build()

    def build(self):
        """A fresh fixture holding exactly the documents registry v2 describes."""
        self.round += 1
        base = Path(self.temp.name) / f'fixture-{self.round}'
        self.s, self.m, self.o = base / 'spec', base / 'manual', base / 'owner'
        texts = {}
        for name, (revision, owner, _, path) in ORACLE.items():
            texts[path] = texts.get(path, '') + source(name, revision, tags(owner)) + '\n'
        texts[DEPLOYMENT] += REQUIREMENT
        texts['README.adoc'] = README
        for path, text in texts.items():
            (self.s / path).parent.mkdir(parents=True, exist_ok=True)
            (self.s / path).write_text(text)
        for root, prefix, owner in ((self.m, 'sdk', SDK), (self.o, 'iceoryx2', ICEORYX2)):
            root.mkdir(parents=True)
            (root / 'README.md').write_text('# Safety manual\n\nStatus prose only.\n')
            for name, (revision, who, entry, _) in ORACLE.items():
                if who == owner:
                    (root / f'{name}.md').write_text(manual(name, revision, prefix, entry))
        self.aou, self.deployment = self.s / AOU, self.s / DEPLOYMENT
        self.iceoryx2, self.streamer = self.s / ICEORYX2_SPEC, self.s / STREAMER_SPEC
        self.readme, self.one = self.s / 'README.adoc', self.m / 'clock.md'
        self.own = self.o / 'iceoryx2-limits.md'

    def replace(self, path, before, after):
        text = path.read_text()
        self.assertIn(before, text)
        path.write_text(text.replace(before, after, 1))

    def fails(self, code, owners=None, registry=checker.REGISTRY):
        with self.assertRaisesRegex(checker.Invalid, f'^{code}: '):
            checker.check(self.s, self.m, owners, registry)

    def owner_fails(self, code):
        self.fails(code, {ICEORYX2: self.o})

    def registry(self, **changes):
        return dataclasses.replace(checker.REGISTRY, **changes)

    def family(self, target, **changes):
        families = tuple(dataclasses.replace(family, **changes) if family.name == target
                         else family for family in checker.REGISTRY.families)
        return self.registry(families=families)

    def line_of(self, path, text):
        """1-based number of the one line equal to text, read from the fixture itself."""
        lines = path.read_text().split('\n')
        self.assertEqual(lines.count(text), 1, text)
        return lines.index(text) + 1

    def fails_at(self, code, path, line, detail, owners=None):
        """The run fails with this code at this file and line, and the detail matches."""
        with self.assertRaises(checker.Invalid) as caught:
            checker.check(self.s, self.m, owners)
        message = str(caught.exception)
        self.assertTrue(message.startswith(f'{code}: {path}:{line}: '), message)
        self.assertRegex(message, detail)

    def anchor_count_fails(self, found):
        """The anchor is not declared exactly once under the whole specification root."""
        with self.assertRaisesRegex(checker.Invalid, f'^SPEC_ANCHOR: {re.escape(str(self.s))}: '
                                    rf'.*declared exactly once; found {found}: '):
            checker.check(self.s, self.m)

    def spelling_fails(self, path, line):
        self.fails_at('SPEC_ANCHOR', path, line, 'unsupported declaration spelling')

    def add_to_statement(self, added, name='clock', quote=True):
        """Add raw lines after the first line of a family's statement and, if quote, to its
        manual quotation, so that a comparison of the two alone would pass."""
        revision, owner, _, relative = ORACLE[name]
        first = f'The integrator *SHALL* check {name}.'
        self.replace(self.s / relative, first + '\n',
                     first + '\n' + ''.join(line + '\n' for line in added))
        if quote:
            entry = (self.m if owner == SDK else self.o) / f'{name}.md'
            self.replace(entry, f'> {first}\n', f'> {first}\n' + ''.join(
                ('> ' + line if line else '>') + '\n' for line in added))
        return self.s / relative

    # Registry v2 and its own consistency.

    def test_pinned_registry_equals_independent_oracle(self):
        registry = checker.REGISTRY
        self.assertEqual(registry.version, 2)
        self.assertEqual({f.name: (f.revision, f.owner, f.entry) for f in registry.families},
                         {name: row[:3] for name, row in ORACLE.items()})
        self.assertEqual(len(registry.families), 23)
        self.assertEqual(registry.counts, {'families': 23, SDK: 19, STREAMER: 2, ICEORYX2: 2})
        self.assertEqual({o.repository: o.prefix for o in registry.owners},
                         {SDK: 'uman~sdk-aou-', STREAMER: None, ICEORYX2: 'uman~iceoryx2-aou-'})
        self.assertEqual(registry.anchors, ('faulty-peer',))
        for family in registry.families:
            if family.owner != SDK:
                self.assertTrue(family.reason and family.destination, family.name)
                self.assertTrue(family.destination.startswith(family.owner), family.name)
        self.assertEqual({f.name: (f.reason, f.destination) for f in registry.families
                          if f.owner != SDK}, EXCLUSIONS)
        self.assertTrue(all(f.reason is None and f.destination is None
                            for f in registry.families if f.owner == SDK))
        checker.validate_registry(registry)

    def test_v2_timing_budgets_revisions(self):
        self.assertEqual(ORACLE['timing-budgets'][:3], (7, SDK, 3))
        for path in (self.aou, self.m / 'timing-budgets.md'):
            self.replace(path, 'aou~seooc-timing-budgets~7', 'aou~seooc-timing-budgets~6')
        with self.assertRaisesRegex(checker.Invalid, r'^OWNERSHIP_REVISION: .*'
                                    r'aou~seooc-timing-budgets~6: registry v2 pins '
                                    r'aou~seooc-timing-budgets~7 \(up-rust\)'):
            checker.check(self.s, self.m)
        self.build()
        self.replace(self.m / 'timing-budgets.md', '`uman~sdk-aou-timing-budgets~3`',
                     '`uman~sdk-aou-timing-budgets~2`')
        with self.assertRaisesRegex(checker.Invalid, r'^MANUAL_ID_REVISION: .*'
                                    r'uman~sdk-aou-timing-budgets~2: registry v2 pins '
                                    r'uman~sdk-aou-timing-budgets~3;'):
            checker.check(self.s, self.m)

    def test_no_review_package_or_installation_claims(self):
        """No constant, diagnostic or help text names a review package, a candidate
        directory or an installation state (CHECKER-HANDOFF.md section 6.4)."""
        tree = ast.parse(SCRIPT.read_text(encoding='utf-8'))
        texts = '\n'.join(node.value for node in ast.walk(tree)
                          if isinstance(node, ast.Constant) and isinstance(node.value, str))
        for fragment in ('this candidate', 'candidate/', 'candidate under', 'owner-manuals',
                         'review package', 'not written', 'installed', 'integrated',
                         'proposal', 'proposed', 'safety-layer'):
            with self.subTest(fragment=fragment):
                self.assertNotIn(fragment, texts.lower())
        for family in checker.REGISTRY.families:
            for text in (family.reason or '', family.destination or ''):
                self.assertNotIn('candidate', text.lower(), family.name)

    def test_registry_counts_disagree_with_family_list(self):
        for counts in ({'families': 24, SDK: 19, STREAMER: 2, ICEORYX2: 2},
                       {'families': 23, SDK: 17, STREAMER: 2, ICEORYX2: 2},
                       {'families': 23, SDK: 18, STREAMER: 3, ICEORYX2: 2},
                       {'families': 23, SDK: 19, STREAMER: 2},
                       {'families': 23, SDK: 19, STREAMER: 2, ICEORYX2: 2, 'other': 0}):
            with self.subTest(counts=counts):
                self.fails('REGISTRY_COUNTS', registry=self.registry(counts=counts))

    def test_registry_family_under_two_owners(self):
        extra = checker.Family('clock', 3, STREAMER, None, 'Streamer reason.', 'up-streamer-rust')
        self.fails('REGISTRY_OWNERSHIP', registry=self.registry(
            families=checker.REGISTRY.families + (extra,)))

    def test_registry_family_both_required_and_excluded(self):
        self.fails('REGISTRY_OWNERSHIP', registry=self.family(
            'clock', reason='Owned elsewhere.', destination='elsewhere/clock.md'))

    def test_registry_family_listed_twice(self):
        clock = next(f for f in checker.REGISTRY.families if f.name == 'clock')
        self.fails('REGISTRY_DUPLICATE', registry=self.registry(
            families=checker.REGISTRY.families + (clock,)))

    def test_registry_unjustified_exclusion(self):
        for changes in ({'reason': None}, {'reason': '  '}, {'destination': None},
                        {'destination': ''}, {'reason': None, 'destination': None}):
            for name in ('operator-access', 'iceoryx2-limits'):
                with self.subTest(name=name, changes=changes):
                    self.fails('REGISTRY_EXCLUSION', registry=self.family(name, **changes))

    def test_registry_has_no_catch_all_owner(self):
        for owner in ('anyone', '*', None):
            with self.subTest(owner=owner):
                self.fails('REGISTRY_OWNERSHIP', registry=self.family('clock', owner=owner))

    def test_registry_malformed_entries(self):
        cases = (
            ('family name', self.family('clock', name='Clock')),
            ('wildcard family', self.family('clock', name='*')),
            ('revision zero', self.family('clock', revision=0)),
            ('revision bool', self.family('clock', revision=True)),
            ('revision text', self.family('clock', revision='3')),
            ('missing SDK manual revision', self.family('clock', entry=None)),
            ('manual revision without an owner manual', self.family('operator-access', entry=1)),
            ('missing owner manual revision', self.family('iceoryx2-limits', entry=None)),
            ('bad prefix', self.registry(owners=(
                checker.Owner(SDK, 'uman~sdk-aou-'), checker.Owner(STREAMER, None),
                checker.Owner(ICEORYX2, 'uman~iceoryx2-')))),
            ('version', self.registry(version=0)),
            ('anchor', self.registry(anchors=('faulty-peer', 'faulty-peer'))),
        )
        for label, registry in cases:
            with self.subTest(label):
                self.fails('REGISTRY_FORMAT', registry=registry)

    def test_registry_owner_contradictions(self):
        shared = (checker.Owner(SDK, 'uman~sdk-aou-'), checker.Owner(STREAMER, None),
                  checker.Owner(ICEORYX2, 'uman~sdk-aou-'))
        twice = checker.REGISTRY.owners + (checker.Owner(STREAMER, None),)
        no_sdk = (checker.Owner(STREAMER, None), checker.Owner(ICEORYX2, 'uman~iceoryx2-aou-'))
        for label, owners in (('shared prefix', shared), ('owner twice', twice),
                              ('no SDK owner', no_sdk)):
            with self.subTest(label):
                self.fails('REGISTRY_OWNERSHIP', registry=self.registry(owners=owners))

    # Positive controls.

    def test_positive_pinned_registry_19_entries_4_exclusions(self):
        result = checker.check(self.s, self.m)
        self.assertEqual((result['registry_version'], result['assumption_families'],
                          result['sdk_entries'], result['excluded']), (2, 23, 19, 4))
        self.assertEqual(result['spec_items_parsed'], 24)
        self.assertEqual(result['files_read'], 5 + 20)
        self.assertEqual(result['owner_manuals_checked'], '0/1')
        owners = {record['owner']: record for record in result['owners']}
        self.assertEqual((owners[SDK]['families'], owners[SDK]['entries']), (19, 19))
        self.assertEqual((owners[STREAMER]['families'], owners[STREAMER]['owner_manual']),
                         (2, 'not_checked'))
        self.assertEqual((owners[ICEORYX2]['families'], owners[ICEORYX2]['owner_manual']),
                         (2, 'not_checked'))
        self.assertIn('README.adoc:5', result['anchors']['faulty-peer'])

    def test_positive_owner_root_checked(self):
        result = checker.check(self.s, self.m, {ICEORYX2: self.o})
        owners = {record['owner']: record for record in result['owners']}
        self.assertEqual((owners[ICEORYX2]['owner_manual'], owners[ICEORYX2]['entries']),
                         ('checked', 2))
        self.assertEqual(owners[STREAMER]['owner_manual'], 'not_checked')
        self.assertEqual((result['owner_manuals_checked'], result['files_read']), ('1/1', 28))

    def test_report_lines_name_counts_by_owner(self):
        absent = checker.report(checker.check(self.s, self.m))
        self.assertTrue(absent[0].startswith('safety-manual: PASS: registry_version=2, '
                                             'assumption_families=23, spec_items_parsed=24, '
                                             'sdk_entries=19, excluded=4, '
                                             'owner_manuals_checked=0/1, '))
        self.assertIn('safety-manual: owner up-rust: required_entries=19, entries=19, '
                      'quotations=exact, gate_tag=SafetyGateUpRust', absent)
        self.assertIn('safety-manual: owner up-streamer-rust: excluded=2, owner_manual='
                      'not_checked (registry v2 registers no owner manual)', absent)
        self.assertIn('safety-manual: owner up-transport-iceoryx2-rust: excluded=2, '
                      'owner_manual=not_checked (no --owner-manual-root given)', absent)
        present = checker.report(checker.check(self.s, self.m, {ICEORYX2: self.o}))
        self.assertIn('owner_manuals_checked=1/1', present[0])
        self.assertIn('safety-manual: owner up-transport-iceoryx2-rust: excluded=2, '
                      'owner_manual=checked, entries=2, quotations=exact', present)
        self.assertIn('not proof that the ownership is right', present[-1])
        # The frozen order: summary, one line per owner, one per anchor, the limit.
        self.assertEqual(len(present), 6)
        self.assertRegex(present[0], r'^safety-manual: PASS: registry_version=2, '
                         r'assumption_families=23, spec_items_parsed=24, sdk_entries=19, '
                         r'excluded=4, owner_manuals_checked=1/1, files_read=28, '
                         r'content_bytes_read=\d+$')
        self.assertEqual([line.split(':')[1] for line in present[1:4]],
                         [f' owner {SDK}', f' owner {STREAMER}', f' owner {ICEORYX2}'])
        self.assertEqual(present[4], 'safety-manual: shared definition [[faulty-peer]] '
                         f'declared once at {self.readme}:5')
        self.assertEqual(present[5], 'Document consistency against ownership registry v2, an '
                         'author assertion: not proof that the ownership is right, and no '
                         'product, OFT, G1a, integrator-evidence or deployment admission.')

    def test_positive_covers_word_inside_statement(self):
        for path in (self.aou, self.one):
            self.replace(path, 'check clock.', 'check clock; the data covers: values.')
        checker.check(self.s, self.m)

    def test_positive_registry_change_with_documents(self):
        for path in (self.aou, self.one):
            self.replace(path, 'aou~seooc-clock~3', 'aou~seooc-clock~4')
            self.replace(path, 'check clock.', 'check both clocks.')
        self.fails('OWNERSHIP_REVISION')
        checker.check(self.s, self.m, registry=self.family('clock', revision=4))

    def test_positive_crlf_delimiters(self):
        for path in (self.one, self.aou, self.own):
            path.write_bytes(path.read_bytes().replace(b'\n', b'\r\n'))
        checker.check(self.s, self.m, {ICEORYX2: self.o})

    def test_spec_aou_moved_to_other_file_still_detected(self):
        self.replace(self.aou, source('clock', 3, tags(SDK)), '')
        (self.s / 'up-l1' / 'other.adoc').write_text(source('clock', 3, tags(SDK)))
        checker.check(self.s, self.m)

    # Exact collection, revisions, needs and tags.

    def test_unknown_family_fails_whatever_its_tags(self):
        for tag_list in ('SafetyManual', 'SafetyManual,SafetyGateUpRust', 'SafetyBinding'):
            for relative in (AOU, STREAMER_SPEC):
                with self.subTest(tags=tag_list, path=relative):
                    self.build()
                    with (self.s / relative).open('a') as stream:
                        stream.write(source('new-duty', 1, tag_list))
                    self.fails('OWNERSHIP_SPEC')

    def test_unknown_family_without_seooc_prefix(self):
        with self.deployment.open('a') as stream:
            stream.write(source('x', 1, 'SafetyManual').replace('aou~seooc-x~1', 'aou~clock~3'))
        self.fails('OWNERSHIP_SPEC')

    def test_registry_family_missing_from_specification(self):
        for name, path in (('clock', AOU), ('peer-access', DEPLOYMENT),
                           ('operator-access', STREAMER_SPEC),
                           ('iceoryx2-limits', ICEORYX2_SPEC)):
            with self.subTest(name=name):
                self.build()
                revision, owner, _, _ = ORACLE[name]
                self.replace(self.s / path, source(name, revision, tags(owner)), '')
                self.fails('OWNERSHIP_SPEC')

    def test_family_at_another_revision(self):
        for name, path, revision in (('clock', AOU, 4), ('clock', AOU, 2),
                                     ('integrity-levels', DEPLOYMENT, 5),
                                     ('monitor-thresholds', STREAMER_SPEC, 3),
                                     ('iceoryx2-limits', ICEORYX2_SPEC, 2)):
            with self.subTest(name=name, revision=revision):
                self.build()
                pinned = ORACLE[name][0]
                self.replace(self.s / path, f'aou~seooc-{name}~{pinned}',
                             f'aou~seooc-{name}~{revision}')
                self.fails('OWNERSHIP_REVISION')

    def test_duplicate_spec_aou_other_file(self):
        (self.s / 'other.adoc').write_text(source('clock', 3, tags(SDK)))
        self.fails('SPEC_DUPLICATE')

    def test_duplicate_spec_family_different_revision(self):
        for name, revision in (('clock', 4), ('iceoryx2-limits', 2), ('operator-access', 1)):
            with self.subTest(name=name):
                self.build()
                (self.s / 'other.adoc').write_text(source(name, revision, 'SafetyManual'))
                self.fails('SPEC_DUPLICATE')

    def test_duplicate_requirement_family_different_revision(self):
        with self.iceoryx2.open('a') as stream:
            stream.write('\n' + REQUIREMENT.replace('fixture~2', 'fixture~1'))
        self.fails('SPEC_DUPLICATE')

    def test_aou_utest_need_forbidden(self):
        self.replace(self.aou, 'oft-needs="uman",oft-tags="SafetyManual,SafetyGateUpRust"]\n--\n'
                     'The integrator *SHALL* check clock.',
                     'oft-needs="uman,utest",oft-tags="SafetyManual,SafetyGateUpRust"]\n--\n'
                     'The integrator *SHALL* check clock.')
        self.fails('OWNERSHIP_NEEDS')

    def test_aou_missing_manual_tag(self):
        for name, path in (('clock', AOU), ('iceoryx2-limits', ICEORYX2_SPEC)):
            with self.subTest(name=name):
                self.build()
                revision, owner, _, _ = ORACLE[name]
                self.replace(self.s / path, source(name, revision, tags(owner)),
                             source(name, revision, tags(owner).replace(
                                 'SafetyManual', 'SafetyUpRustCore')))
                self.fails('OWNERSHIP_NEEDS')

    def test_excluded_family_with_gate_tag(self):
        for name, path in (('operator-access', STREAMER_SPEC), ('iceoryx2-limits', ICEORYX2_SPEC)):
            with self.subTest(name=name):
                self.build()
                revision = ORACLE[name][0]
                self.replace(self.s / path, source(name, revision, 'SafetyManual'),
                             source(name, revision, 'SafetyManual,SafetyGateUpRust'))
                self.fails('OWNERSHIP_GATE')

    def test_sdk_family_without_gate_tag(self):
        for name, path in (('clock', AOU), ('peer-access', DEPLOYMENT)):
            with self.subTest(name=name):
                self.build()
                revision = ORACLE[name][0]
                self.replace(self.s / path, source(name, revision, tags(SDK)),
                             source(name, revision, 'SafetyManual'))
                self.fails('OWNERSHIP_GATE')

    def test_unregistered_tag_needs_registry_change(self):
        for name, path, tag_list in (
                ('clock', AOU, 'SafetyManual,SafetyGateUpRust,SafetyBinding'),
                ('iceoryx2-limits', ICEORYX2_SPEC, 'SafetyManual,SafetyIceoryx2')):
            with self.subTest(name=name):
                self.build()
                revision, owner, _, _ = ORACLE[name]
                self.replace(self.s / path, source(name, revision, tags(owner)),
                             source(name, revision, tag_list))
                self.fails('OWNERSHIP_TAGS')

    def test_faulty_peer_anchor_exactly_once(self):
        self.replace(self.readme, '[[faulty-peer]]\n', '')
        self.anchor_count_fails(0)
        self.build()
        with self.streamer.open('a') as stream:
            stream.write('\n[#faulty-peer]\n=== Again\n\nText.\n')
        self.spelling_fails(self.streamer, self.line_of(self.streamer, '[#faulty-peer]'))
        self.build()
        self.replace(self.readme, '[[faulty-peer]]', '[[faulty-peer-model]]')
        self.anchor_count_fails(0)

    # Anchor declarations (IR-03): CHECKER-HANDOFF.md section 5.2 and the cases of 6.2.

    def test_anchor_control_one_declaration(self):
        for owners in (None, {ICEORYX2: self.o}):
            with self.subTest(owners=owners):
                result = checker.check(self.s, self.m, owners)
                self.assertEqual(result['anchors'], {'faulty-peer': f'{self.readme}:5'})

    def test_anchor_comment_only_is_no_declaration(self):
        self.replace(self.readme, '[[faulty-peer]]\n', '// [[faulty-peer]]\n')
        self.anchor_count_fails(0)

    def test_anchor_escape_only_is_no_declaration(self):
        self.replace(self.readme, '[[faulty-peer]]\n', '\\[[faulty-peer]]\n')
        self.anchor_count_fails(0)

    def test_anchor_declaration_with_commented_copy(self):
        self.replace(self.readme, '[[faulty-peer]]\n', '[[faulty-peer]]\n// [[faulty-peer]]\n')
        result = checker.check(self.s, self.m)
        self.assertEqual(result['anchors'], {'faulty-peer': f'{self.readme}:5'})

    def test_anchor_declaration_with_escaped_copy(self):
        for copy in ('\\[[faulty-peer]]', 'The line \\[[faulty-peer]] declares the model.'):
            with self.subTest(copy=copy):
                self.build()
                self.replace(self.readme, 'A faulty peer may write any value.\n',
                             f'A faulty peer may write any value.\n\n{copy}\n')
                result = checker.check(self.s, self.m)
                self.assertEqual(result['anchors'], {'faulty-peer': f'{self.readme}:5'})

    def test_anchor_comments_and_escapes_of_every_spelling(self):
        """Commented or escaped, no declaration spelling counts or fails."""
        for spelling in ('[[faulty-peer]]',) + UNSUPPORTED_SPELLINGS:
            for copy in (f'// {spelling}', f'//{spelling}', f'\\{spelling}',
                         f'Text \\{spelling} text.'):
                with self.subTest(copy=copy):
                    self.build()
                    with self.streamer.open('a') as stream:
                        stream.write(f'\n{copy}\n')
                    result = checker.check(self.s, self.m)
                    self.assertEqual(result['anchors'], {'faulty-peer': f'{self.readme}:5'})
                    self.replace(self.readme, '[[faulty-peer]]\n', '')
                    self.anchor_count_fails(0)

    def test_anchor_declared_twice(self):
        with self.readme.open('a') as stream:
            stream.write('\n[[faulty-peer]]\n== Again\n')
        self.anchor_count_fails(2)
        self.build()
        with self.streamer.open('a') as stream:
            stream.write('\n[[faulty-peer]]\n=== Again\n\nText.\n')
        with self.assertRaisesRegex(checker.Invalid, re.escape(
                f"found 2: ['{self.readme}:5', '{self.streamer}:")):
            checker.check(self.s, self.m)

    def test_anchor_references_are_neither_declarations_nor_failures(self):
        references = ('See <<faulty-peer>> and <<faulty-peer,the model>>.',
                      'See xref:README.adoc#faulty-peer[the model] and '
                      '<<README.adoc#faulty-peer>>.')
        self.add_to_statement(['', references[0]])
        self.add_to_statement(['', references[1]], name='peer-access')
        with self.streamer.open('a') as stream:
            stream.write('\n' + '\n'.join(references) + '\n')
        result = checker.check(self.s, self.m)
        self.assertEqual(result['anchors'], {'faulty-peer': f'{self.readme}:5'})
        self.replace(self.readme, '[[faulty-peer]]\n', '')  # references alone declare nothing
        self.anchor_count_fails(0)

    def test_anchor_only_another_spelling(self):
        for spelling in UNSUPPORTED_SPELLINGS + ('See [[faulty-peer]] here.',):
            with self.subTest(spelling=spelling):
                self.build()
                self.replace(self.readme, '[[faulty-peer]]\n', spelling + '\n')
                self.spelling_fails(self.readme, 5)

    def test_anchor_declaration_and_another_spelling(self):
        for spelling in UNSUPPORTED_SPELLINGS + ('[[faulty-peer]]',):
            for line in ((spelling, f'Prose {spelling} prose.') if spelling != '[[faulty-peer]]'
                         else ('See [[faulty-peer]] here.',)):
                with self.subTest(line=line):
                    self.build()
                    self.replace(self.readme, 'A faulty peer may write any value.\n',
                                 f'A faulty peer may write any value.\n{line}\n')
                    self.spelling_fails(self.readme, 9)

    def test_anchor_inline_or_padded_block_spelling(self):
        """Only a whole line equal to [[faulty-peer]] declares; no whitespace is normalized."""
        for line in ('See [[faulty-peer]] here.', '[[faulty-peer]] ', ' [[faulty-peer]]',
                     '[[faulty-peer]][[faulty-peer]]', '  // [[faulty-peer]]',
                     '\\[[faulty-peer]][[faulty-peer]]'):
            with self.subTest(line=line):
                self.build()
                self.replace(self.readme, '[[faulty-peer]]\n', line + '\n')
                self.spelling_fails(self.readme, 5)
        self.build()
        self.readme.write_bytes(self.readme.read_bytes().replace(b'\n', b'\r\n'))
        result = checker.check(self.s, self.m)
        self.assertEqual(result['anchors'], {'faulty-peer': f'{self.readme}:5'})

    def test_anchor_declaration_inside_statement(self):
        for keep_readme in (True, False):
            with self.subTest(keep_readme=keep_readme):
                self.build()
                if not keep_readme:
                    self.replace(self.readme, '[[faulty-peer]]\n', '')
                path = self.add_to_statement(['', '[[faulty-peer]]'])
                self.fails_at('SPEC_ANCHOR', path, self.line_of(path, '[[faulty-peer]]'),
                              r'is declared inside the statement of aou~seooc-clock~3')
        self.build()  # the last line of a statement is inside it
        self.replace(self.aou, 'check clock.\n\n* Retain the result.\n--',
                     'check clock.\n\n* Retain the result.\n[[faulty-peer]]\n--')
        self.replace(self.one, '> * Retain the result.\n<!--',
                     '> * Retain the result.\n> [[faulty-peer]]\n<!--')
        self.fails_at('SPEC_ANCHOR', self.aou, self.line_of(self.aou, '[[faulty-peer]]'),
                      r'inside the statement of aou~seooc-clock~3')
        self.build()
        with self.deployment.open('a') as stream:
            stream.write(REQUIREMENT.replace('fixture~2', 'other~1').replace(
                'Each binding', '[[faulty-peer]]\nEach binding'))
        self.fails_at('SPEC_ANCHOR', self.deployment, self.line_of(self.deployment,
                      '[[faulty-peer]]'), r'inside the statement of req~ssr-binding-other~1')

    def test_anchor_spelling_in_hidden_or_nested_context(self):
        """No context lets a spelling escape the guard: statements of every kind, item
        attributes, later pages, a last line without LF, a spelling inside an escaped one."""
        path = self.add_to_statement(['See [#faulty-peer] here.'])
        self.spelling_fails(path, self.line_of(path, 'See [#faulty-peer] here.'))
        self.build()
        path = self.add_to_statement(['', '* anchor:faulty-peer[]'], name='iceoryx2-limits')
        self.fails_at('SPEC_ANCHOR', path, self.line_of(path, '* anchor:faulty-peer[]'),
                      'unsupported declaration spelling', {ICEORYX2: self.o})
        self.build()
        self.replace(self.aou, 'oft-title="clock"', 'oft-title="clock [[faulty-peer]]"')
        self.spelling_fails(self.aou, self.line_of(
            self.aou, next(line for line in self.aou.read_text().split('\n')
                           if 'oft-title="clock [[faulty-peer]]"' in line)))
        self.build()
        self.replace(self.deployment, 'Each binding *SHALL* keep the fixture,',
                     'Each binding *SHALL* keep the fixture [id="faulty-peer"],')
        self.spelling_fails(self.deployment, self.line_of(self.deployment, next(
            line for line in self.deployment.read_text().split('\n') if '[id="faulty' in line)))
        self.build()
        self.streamer.write_text(self.streamer.read_text() + 'Last [[faulty-peer,model]]')
        self.spelling_fails(self.streamer, len(self.streamer.read_text().split('\n')))
        self.build()
        with self.streamer.open('a') as stream:
            stream.write('\n\\[[faulty-peer, see [#faulty-peer] ]]\n')
        self.spelling_fails(self.streamer, self.line_of(
            self.streamer, '\\[[faulty-peer, see [#faulty-peer] ]]'))

    def test_anchor_spelling_of_another_id_is_not_this_anchor(self):
        for line in ('[[faulty-peers]]', '[#faulty-peer-model]', 'anchor:faulty-peer-x[]',
                     '[id=faulty-peer-x]', '[[xfaulty-peer]]'):
            with self.subTest(line=line):
                self.build()
                with self.streamer.open('a') as stream:
                    stream.write(f'\n{line}\n')
                result = checker.check(self.s, self.m)
                self.assertEqual(result['anchors'], {'faulty-peer': f'{self.readme}:5'})

    def test_review_probes_rebound_to_v2(self):
        """The six review probes (PROBE-RESULTS.json), rebuilt on the registry-v2 fixture
        with the same edits; their expected outcomes, now required of this checker."""
        line = 'The integrator *SHALL* check clock.'
        cases = (('control', None, None, None),
                 ('comment_only_anchor', '// [[faulty-peer]]', None, 'SPEC_ANCHOR'),
                 ('escaped_only_anchor', '\\[[faulty-peer]]', None, 'SPEC_ANCHOR'),
                 ('active_plus_comment', '[[faulty-peer]]\n// [[faulty-peer]]', None, None),
                 ('include_in_statement', None, 'include::not-read.adoc[]', 'SPEC_MARKUP'),
                 ('conditional_in_statement', None,
                  'ifdef::missing[]\nHidden obligation.\nendif::[]', 'SPEC_MARKUP'))
        for name, anchor, added, expected in cases:
            with self.subTest(probe=name):
                self.build()
                if anchor is not None:
                    self.replace(self.readme, '[[faulty-peer]]', anchor)
                if added:
                    self.replace(self.aou, line, line + '\n' + added)
                    self.replace(self.one, '> ' + line, '> ' + line + '\n' + '\n'.join(
                        '> ' + part for part in added.split('\n')))
                if expected is None:
                    checker.check(self.s, self.m, {ICEORYX2: self.o})
                else:
                    self.fails(expected, {ICEORYX2: self.o})

    # Statement grammar (IR-04): CHECKER-HANDOFF.md section 5.1 and the cases of 6.3.

    def test_positive_statement_paragraphs_and_lists(self):
        added = ['', 'A second paragraph', 'over two lines.', '', '* A first-level item',
                 '** A second-level item', '* Another first-level item']
        self.add_to_statement(added)
        checker.check(self.s, self.m)
        for paths in ((self.aou,), (self.one,), (self.aou, self.one)):
            with self.subTest(crlf=[path.name for path in paths]):
                self.build()
                self.add_to_statement(added)
                for path in paths:
                    path.write_bytes(path.read_bytes().replace(b'\n', b'\r\n'))
                checker.check(self.s, self.m)

    def test_statement_include_fails_before_quotation(self):
        """The include names a file that exists outside the specification root; the line
        itself fails, whether or not the manual quotes it, and nothing is included."""
        for quote in (True, False):
            with self.subTest(quote=quote):
                self.build()
                (self.s.parent / 'outside.adoc').write_text('Replacement statement.\n')
                path = self.add_to_statement(['include::../outside.adoc[]'], quote=quote)
                self.fails_at('SPEC_MARKUP', path,
                              self.line_of(path, 'include::../outside.adoc[]'),
                              r'^SPEC_MARKUP: \S+: aou~seooc-clock~3: unsupported delimiter, '
                              r'include or conditional line inside the statement; it fails '
                              r'before any quotation is compared$')

    def test_statement_conditionals_fail_before_quotation(self):
        for added in (['ifdef::missing[]', 'Hidden obligation.', 'endif::[]'],
                      ['ifndef::present[]', 'Hidden obligation.', 'endif::present[]'],
                      ['ifeval::[{level} > 1]', 'Hidden obligation.', 'endif::[]'],
                      ['Shown obligation.', 'endif::[]']):
            with self.subTest(added=added):
                self.build()
                path = self.add_to_statement(added)
                first = next(line for line in added if line.startswith(('if', 'endif')))
                self.fails_at('SPEC_MARKUP', path, self.line_of(path, first),
                              'aou~seooc-clock~3: unsupported .* inside the statement')

    def test_statement_every_negative_grammar_line(self):
        for line in NEGATIVE_LINES:
            with self.subTest(line=line):
                self.build()
                path = self.add_to_statement(['', line, 'Inside the block.', line])
                number = self.line_of(path, 'Inside the block.') - 1
                self.fails_at('SPEC_MARKUP', path, number, 'inside the statement')

    def test_statement_rejection_in_every_statement_kind(self):
        """Requirements, external-owner assumptions, the last item of a page, CRLF."""
        with self.deployment.open('a') as stream:
            stream.write(REQUIREMENT.replace('fixture~2', 'last~1').replace(
                'Each binding', 'include::x.adoc[]\nEach binding'))
        self.fails_at('SPEC_MARKUP', self.deployment,
                      self.line_of(self.deployment, 'include::x.adoc[]'),
                      'req~ssr-binding-last~1: unsupported')
        self.build()
        path = self.add_to_statement(['----'], name='iceoryx2-limits')
        self.fails_at('SPEC_MARKUP', path, self.line_of(path, '----'),
                      'aou~seooc-iceoryx2-limits~1: unsupported', {ICEORYX2: self.o})
        self.build()
        path = self.add_to_statement(['++++'], name='monitor-thresholds', quote=False)
        self.fails_at('SPEC_MARKUP', path, self.line_of(path, '++++'),
                      'aou~seooc-monitor-thresholds~2: unsupported')
        self.build()
        path = self.add_to_statement(['ifdef::x[]'])
        number = self.line_of(path, 'ifdef::x[]')
        path.write_bytes(path.read_bytes().replace(b'\n', b'\r\n'))
        self.fails_at('SPEC_MARKUP', path, number, 'aou~seooc-clock~3: unsupported')

    def test_outside_item_negative_grammar_kept(self):
        for line in ('--',) + NEGATIVE_LINES:
            with self.subTest(line=line):
                self.build()
                with self.streamer.open('a') as stream:
                    stream.write(f'\nOutside.\n{line}\n')
                self.fails_at('SPEC_MARKUP', self.streamer,
                              self.line_of(self.streamer, 'Outside.') + 1,
                              'unexpected delimiter or unsupported')

    def test_statement_boundaries_and_nesting_kept(self):
        first = 'The integrator *SHALL* check clock.'
        after_clock = '[.specitem,oft-sid="aou~seooc-waiver'
        nested = ('[.specitem,oft-sid="aou~seooc-x~1",oft-title="x",oft-needs="uman",'
                  'oft-tags="SafetyManual"]')
        cases = (('empty', f'--\n{first}\n\n* Retain the result.\n--', '--\n--',
                  'SPEC_STATEMENT'),
                 ('blank only', f'{first}\n\n* Retain the result.', '   ', 'SPEC_STATEMENT'),
                 ('blank first line', f'--\n{first}', f'--\n\n{first}', 'SPEC_STATEMENT'),
                 ('blank last line', f'* Retain the result.\n--\n\n{after_clock}',
                  f'* Retain the result.\n\n--\n\n{after_clock}', 'SPEC_STATEMENT'),
                 ('nested item', first, f'{first}\n{nested}', 'SPEC_MARKUP'),
                 ('declaration text', first, f'{first} See oft-sid="aou~seooc-x~1".',
                  'SPEC_MARKUP'),
                 ('closing line inside', first, f'{first}\n--', 'SPEC_MARKUP'))
        for label, before, after, code in cases:
            with self.subTest(label):
                self.build()
                self.replace(self.aou, before, after)
                self.fails(code)

    def test_known_limit_negative_grammar_is_not_a_parser(self):
        """CHECKER-HANDOFF.md section 8: these lines are not rejected inside a statement
        or outside an item. Widening the list is an interface change, not a test fix."""
        for line in ('-----', '.....', '+++++', '/////', '|===', '====', '****', '____',
                     '// a line comment', ':attribute: value', 'Inline *strong* and `code`.',
                     ' ----', '---- '):
            with self.subTest(line=line):
                self.build()
                self.add_to_statement([line])
                with self.streamer.open('a') as stream:
                    stream.write(f'\n{line}\n')
                checker.check(self.s, self.m)

    def test_legacy_default_subtree_cannot_pass(self):
        with self.assertRaisesRegex(checker.Invalid, '^OWNERSHIP_SPEC: .*missing=.*'
                                    'aou~seooc-operator-access'):
            checker.check(self.s / 'seooc-up-rust', self.m)

    # Specification grammar (kept).

    def test_missing_spec_delimiter(self):
        self.replace(self.aou, '\n--\n', '\n')
        self.fails('SPEC_MARKUP')

    def test_duplicate_spec_attribute(self):
        self.replace(self.aou, 'oft-needs="uman"', 'oft-needs="uman",oft-needs="uman"')
        self.fails('SPEC_MARKUP')

    def test_commented_spec_block(self):
        for relative in (AOU, ICEORYX2_SPEC):
            with self.subTest(path=relative):
                self.build()
                path = self.s / relative
                path.write_text('////\n' + path.read_text() + '////\n')
                self.fails('SPEC_MARKUP')

    def test_hidden_declaration_outside_specitem(self):
        with self.streamer.open('a') as stream:
            stream.write('\nSee oft-sid="aou~seooc-hidden~1" for details.\n')
        self.fails('SPEC_MARKUP')

    def test_hidden_declaration_inside_attribute_value(self):
        self.replace(self.aou, 'oft-title="clock"', 'oft-title="clock oft-sid=aou~seooc-hidden~1"')
        with self.assertRaisesRegex(checker.Invalid, '^SPEC_MARKUP: .*hidden, duplicated'):
            checker.check(self.s, self.m)

    def test_invalid_spec_id(self):
        self.replace(self.aou, 'aou~seooc-clock~3', 'aou~seooc-clock~03')
        self.fails('SPEC_ID')

    # Manual entries in the up-rust root.

    def test_missing_manual(self):
        for name in ('clock', 'capability-ceiling', 'resource-guarantees'):
            with self.subTest(name=name):
                self.build()
                (self.m / f'{name}.md').unlink()
                self.fails('MANUAL_MISSING')

    def test_duplicate_manual(self):
        (self.m / 'duplicate.md').write_text(self.one.read_text())
        self.fails('MANUAL_DUPLICATE')

    def test_stale_covers_revision(self):
        self.replace(self.one, 'aou~seooc-clock~3', 'aou~seooc-clock~2')
        self.fails('REVISION_MISMATCH')

    def test_manual_item_revision_is_pinned(self):
        for entry in (1, 3):
            with self.subTest(entry=entry):
                self.build()
                self.replace(self.one, '`uman~sdk-aou-clock~2`', f'`uman~sdk-aou-clock~{entry}`')
                self.fails('MANUAL_ID_REVISION')

    def test_same_revision_spec_wording_drift(self):
        self.replace(self.aou, 'check clock.', 'check both clocks.')
        self.fails('STATEMENT_MISMATCH')

    def test_same_revision_manual_wording_drift(self):
        self.replace(self.one, '*SHALL*', '*SHOULD*')
        self.fails('STATEMENT_MISMATCH')

    def test_quotation_differs_by_one_character(self):
        self.replace(self.one, 'check clock.', 'check clock,')
        self.fails('STATEMENT_MISMATCH')

    def test_whitespace_is_not_normalized(self):
        self.replace(self.one, 'check clock.', 'check  clock.')
        self.fails('STATEMENT_MISMATCH')

    def test_missing_quote_bullet(self):
        self.replace(self.one, '> * Retain the result.\n', '')
        self.fails('STATEMENT_MISMATCH')

    def test_changed_blank_line(self):
        for after in ('', '>\n>\n', '>  \n'):
            with self.subTest(after=after):
                self.build()
                self.replace(self.one, 'check clock.\n>\n', 'check clock.\n' + after)
                self.fails('STATEMENT_MISMATCH')

    def test_manual_name_must_match_covered_family(self):
        self.replace(self.one, '`uman~sdk-aou-clock~2`', '`uman~sdk-aou-peer-rates~1`')
        self.fails('MANUAL_ID')

    def test_sdk_entry_for_excluded_family(self):
        for name in ('operator-access', 'monitor-thresholds', 'iceoryx2-level-separation'):
            with self.subTest(name=name):
                self.build()
                (self.m / f'{name}.md').write_text(manual(name, ORACLE[name][0], 'sdk', 1))
                with self.assertRaisesRegex(checker.Invalid, '^OWNERSHIP_MANUAL: .*named '
                                            'exclusion.*Owning manual: up-'):
                    checker.check(self.s, self.m)

    def test_sdk_root_entry_with_owner_prefix(self):
        shutil.copy(self.own, self.m / 'iceoryx2-limits.md')
        self.fails('MANUAL_PREFIX')

    def test_entry_for_unregistered_family(self):
        with self.aou.open('a') as stream:
            stream.write(source('new-duty', 1, tags(SDK)))
        (self.m / 'new-duty.md').write_text(manual('new-duty', 1))
        self.fails('OWNERSHIP_SPEC')
        self.build()
        (self.m / 'new-duty.md').write_text(manual('new-duty', 1))
        self.fails('OWNERSHIP_MANUAL')

    def test_utest_link_in_manual_forbidden(self):
        with self.one.open('a') as stream:
            stream.write('\n[' + 'utest' + '->' + 'aou~seooc-clock~3]\n')
        self.fails('MANUAL_LINK_TYPE')

    def test_impl_link_in_manual_forbidden(self):
        with self.one.open('a') as stream:
            stream.write('\n[' + 'impl' + '->' + 'aou~seooc-clock~3]\n')
        self.fails('MANUAL_LINK_TYPE')

    def test_wrong_item_kind(self):
        self.replace(self.one, '`uman~', '`utest~')
        self.fails('MANUAL_ID')

    def test_duplicate_covers(self):
        self.replace(self.one, 'Covers:\n- aou~seooc-clock~3',
                     'Covers:\n- aou~seooc-clock~3\nCovers:\n- aou~seooc-clock~3')
        self.fails('MANUAL_COVERS')

    def test_two_ids_in_one_covers(self):
        self.replace(self.one, 'Covers:\n- aou~seooc-clock~3',
                     'Covers:\n- aou~seooc-clock~3, aou~seooc-build-order~2')
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

    def test_readme_cannot_hide_coverage(self):
        (self.m / 'README.md').write_text(self.one.read_text())
        self.fails('MANUAL_README')

    def test_unrecognized_file_not_silently_skipped(self):
        self.one.rename(self.m / 'clock.txt')
        self.fails('INPUT_FORMAT')

    def test_symlink_not_followed(self):
        (self.m / 'alias.md').symlink_to(self.one.name)
        self.fails('INPUT_SYMLINK')

    def test_input_encoding_fail_closed(self):
        for label, data in (('bare CR', b'# x\rStatus.\n'), ('NUL', b'# x\n\x00\n'),
                            ('BOM', '\ufeff# x\n'.encode('utf-8'))):
            with self.subTest(label):
                self.build()
                (self.m / 'README.md').write_bytes(data)
                self.fails('INPUT_ENCODING')

    def test_file_count_limit_fail_closed(self):
        for number in range(checker.MAX_FILES):
            (self.m / f'extra-{number}.md').write_text('# extra\n')
        self.fails('INPUT_BUDGET')

    def test_size_limit_fail_closed(self):
        self.one.write_text('x' * (checker.MAX_FILE_BYTES + 1))
        self.fails('INPUT_BUDGET')

    # The optional owner manual root.

    def test_owner_root_missing_or_empty(self):
        self.fails('INPUT_ROOT', {ICEORYX2: self.o / 'missing'})
        empty = self.o.parent / 'empty'
        empty.mkdir()
        self.fails('INPUT_EMPTY', {ICEORYX2: empty})
        for path in self.o.glob('iceoryx2-*.md'):
            path.unlink()
        self.owner_fails('MANUAL_MISSING')

    def test_owner_root_missing_entry(self):
        (self.o / 'iceoryx2-level-separation.md').unlink()
        self.owner_fails('MANUAL_MISSING')

    def test_owner_root_duplicate_entry(self):
        (self.o / 'copy.md').write_text(self.own.read_text())
        self.owner_fails('MANUAL_DUPLICATE')

    def test_owner_root_wrong_prefix(self):
        self.replace(self.own, '`uman~iceoryx2-aou-', '`uman~sdk-aou-')
        self.owner_fails('MANUAL_PREFIX')

    def test_owner_root_entry_for_family_of_another_owner(self):
        for name in ('clock', 'operator-access'):
            with self.subTest(name=name):
                self.build()
                (self.o / f'{name}.md').write_text(manual(name, ORACLE[name][0], 'iceoryx2', 1))
                self.owner_fails('OWNERSHIP_MANUAL')

    def test_owner_root_exact_quotation_and_revisions(self):
        self.replace(self.own, '*SHALL*', '*SHOULD*')
        self.owner_fails('STATEMENT_MISMATCH')
        self.build()
        self.replace(self.own, 'aou~seooc-iceoryx2-limits~1', 'aou~seooc-iceoryx2-limits~2')
        self.owner_fails('REVISION_MISMATCH')
        self.build()
        self.replace(self.own, '`uman~iceoryx2-aou-iceoryx2-limits~1`',
                     '`uman~iceoryx2-aou-iceoryx2-limits~2`')
        self.owner_fails('MANUAL_ID_REVISION')

    def test_owner_root_grammar_and_readme(self):
        self.replace(self.own, '- [ ] Inspect', 'Inspect')
        self.owner_fails('MANUAL_CHECKLIST')
        self.build()
        (self.o / 'README.md').write_text('# Owner\n\nCovers:\n- aou~seooc-iceoryx2-limits~1\n')
        self.owner_fails('MANUAL_README')

    def test_owner_root_only_for_registered_owner_manuals(self):
        for repository in (STREAMER, SDK, 'up-transport-lola-rust'):
            with self.subTest(repository=repository):
                self.fails('OWNER_ROOT', {repository: self.o})

    # Command line.

    def run_cli(self, *arguments, cwd=None, script=SCRIPT):
        return subprocess.run([sys.executable, '-B', str(script), *arguments],
                              capture_output=True, text=True, cwd=cwd)

    def test_cli_success_and_failure_are_real_exit_codes(self):
        arguments = ('--spec-root', str(self.s), '--manual-root', str(self.m))
        result = self.run_cli(*arguments)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('sdk_entries=19, excluded=4, owner_manuals_checked=0/1', result.stdout)
        self.replace(self.one, 'check clock.', 'check wrong clock.')
        result = self.run_cli(*arguments)
        self.assertEqual(result.returncode, 1)
        self.assertIn('STATEMENT_MISMATCH', result.stderr)
        self.assertEqual(result.stdout, '')

    def test_cli_grammar_and_anchor_failures_keep_the_fail_line(self):
        def roots():  # the paths of the current fixture, rebuilt below
            return ('--spec-root', str(self.s), '--manual-root', str(self.m))
        path = self.add_to_statement(['include::not-read.adoc[]'])
        number = self.line_of(path, 'include::not-read.adoc[]')
        result = self.run_cli(*roots())
        self.assertEqual((result.returncode, result.stdout), (1, ''))
        self.assertEqual(result.stderr, f'safety-manual: FAIL: SPEC_MARKUP: {path}:{number}: '
                         'aou~seooc-clock~3: unsupported delimiter, include or conditional '
                         'line inside the statement; it fails before any quotation is '
                         'compared\n')
        self.build()
        self.replace(self.readme, '[[faulty-peer]]\n', '[#faulty-peer]\n')
        result = self.run_cli(*roots())
        self.assertEqual((result.returncode, result.stdout), (1, ''))
        self.assertTrue(result.stderr.startswith(
            f"safety-manual: FAIL: SPEC_ANCHOR: {self.readme}:5: unsupported declaration "
            "spelling '[#faulty-peer]' of faulty-peer: "), result.stderr)
        self.build()
        result = self.run_cli(*roots())
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.split('\n')[4], 'safety-manual: shared definition '
                         f'[[faulty-peer]] declared once at {self.readme}:5')

    def test_cli_missing_spec_fails(self):
        result = self.run_cli('--spec-root', str(self.s / 'missing'), '--manual-root', str(self.m))
        self.assertEqual(result.returncode, 1)
        self.assertIn('INPUT_ROOT', result.stderr)

    def test_cli_owner_root_reported(self):
        result = self.run_cli('--spec-root', str(self.s), '--manual-root', str(self.m),
                              '--owner-manual-root', f'{ICEORYX2}={self.o}')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('owner_manuals_checked=1/1', result.stdout)
        self.assertIn(f'owner {ICEORYX2}: excluded=2, owner_manual=checked, entries=2',
                      result.stdout)
        result = self.run_cli('--spec-root', str(self.s), '--manual-root', str(self.m),
                              '--owner-manual-root', f'{ICEORYX2}={self.o / "missing"}')
        self.assertEqual(result.returncode, 1)
        self.assertIn('INPUT_ROOT', result.stderr)

    def test_cli_owner_root_usage_errors(self):
        good = f'{ICEORYX2}={self.o}'
        for values in ([f'{STREAMER}={self.o}'], [f'{SDK}={self.m}'], [f'lola={self.o}'],
                       [str(self.o)], [f'{ICEORYX2}='], [good, good]):
            with self.subTest(values=values):
                arguments = ['--spec-root', str(self.s), '--manual-root', str(self.m)]
                for value in values:
                    arguments += ['--owner-manual-root', value]
                result = self.run_cli(*arguments)
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertIn('--owner-manual-root', result.stderr)

    def test_cli_default_roots_are_the_safety_tree_and_manual(self):
        repository = Path(self.temp.name) / 'repository'
        (repository / 'tools' / 'ci').mkdir(parents=True)
        script = repository / 'tools' / 'ci' / SCRIPT.name
        shutil.copy(SCRIPT, script)
        shutil.copytree(self.s, repository / 'up-spec' / 'safety')
        shutil.copytree(self.m, repository / 'safety-manual')
        result = self.run_cli(cwd=repository, script=script)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('sdk_entries=19', result.stdout)
        result = self.run_cli('--spec-root', 'up-spec/safety/seooc-up-rust', cwd=repository,
                              script=script)
        self.assertEqual(result.returncode, 1)
        self.assertIn('OWNERSHIP_SPEC', result.stderr)


if __name__ == '__main__':
    unittest.main()

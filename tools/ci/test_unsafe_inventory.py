# Copyright (c) 2026 Contributors to the Eclipse Foundation
# See the NOTICE file(s) distributed with this work for copyright ownership.
# Licensed under the Apache License, Version 2.0.
# SPDX-License-Identifier: Apache-2.0
"""Negative controls for the `unsafe` inventory gate, on scratch trees."""
import contextlib
import io
from pathlib import Path
import tempfile
import unittest

import check_unsafe_inventory as gate


def run_gate(root, *args, paths=('src', 'tests')):
    """Runs the gate on `root`; returns its exit code and everything it printed."""
    out = io.StringIO()
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(out):
        code = gate.main(['--root', str(root), '--paths', *paths, *args])
    return code, out.getvalue()


def write(root, name, text):
    path = root / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)


def inventory_path(root):
    return root / 'inventory.toml'


ROW = 'row:demo'


def seed(root, source, reason=ROW):
    """Writes `source`, lists every site in it with `reason`, and proves the gate passes."""
    write(root, 'src/lib.rs', source)
    (root / 'tests').mkdir(exist_ok=True)
    entries = [gate.Entry(identity[0], identity[1], identity[2], count, reason)
               for identity, count in gate.site_counts(gate.scan_tree(root, ['src', 'tests'])).items()]
    inventory_path(root).write_text(gate.format_inventory(
        {'demo': 'A demo row'}, entries, ['tests/*', 'src/test_util/*', '*/tests.rs']))
    code, output = run_gate(root, '--inventory', 'inventory.toml')
    assert code == 0, output
    return entries


SOURCE = '''\
fn demo() {
    let a = 1;
    unsafe { dangerous(a) };
}

pub struct Handle(*const u8);

unsafe impl Send for Handle {}
'''


class ScannerControls(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)

    def sites(self, source):
        return gate.scan_source(source, 'src/lib.rs')

    def test_every_kind_of_site_is_found(self):
        source = '''\
unsafe trait Demo {}
unsafe impl Demo for u8 {}
unsafe fn free_function() {}
unsafe extern "C" fn callback() {}
unsafe extern "C" {
    fn declared(value: u8);
}
#[unsafe(no_mangle)]
pub extern "C" fn exported() {}
struct Table { call: unsafe fn(u8) -> u8 }
fn caller() { let _ = unsafe { free_function() }; }
'''
        kinds = [site.kind for site in self.sites(source)]
        self.assertEqual(kinds, ['trait', 'impl', 'fn', 'extern fn', 'extern', 'attribute', 'fn', 'block'])

    def test_comments_doc_comments_strings_and_raw_strings_are_not_sites(self):
        source = '''\
#![forbid(unsafe_code)]
//! unsafe { x() } in an inner doc comment
/// unsafe fn in a doc comment
/** unsafe impl Foo for Bar {} in a block doc comment */
/* unsafe { x() } /* unsafe { nested } */ still a comment */
fn demo() {
    // unsafe { x() }
    let a = "unsafe { x() }";
    let b = r"unsafe { x() }";
    let c = r#"unsafe { x() } " still raw"#;
    let d = r##"unsafe "# still raw"##;
    let e = b"unsafe { x() }";
    let f = br#"unsafe { x() }"#;
    let g = c"unsafe { x() }";
    let h = cr#"unsafe { x() }"#;
    let i = 'u';
    let j: &'static str = "";
    let unsafe_thing = 1;
    let not_unsafe = 2;
    let r#unsafe = 3;
    deny_unsafe_op_in_unsafe_fn();
}
'''
        self.assertEqual(self.sites(source), [])

    def test_a_string_cannot_hide_real_code(self):
        source = '''\
fn demo() {
    let a = "/* not a comment";
    unsafe { one() };
    let b = "*/";
    let c = '"';
    unsafe { two() };
    let d = '\\'';
    let e = "\\\\";
    unsafe { three() };
    // a comment with a quote " and a /* opener
    unsafe { four() };
    let f = r#"raw with " and /* and // inside"#;
    unsafe { five() };
    let g = b'"';
    let h = b'\\'';
    unsafe { six() };
    let i = "escaped \\" quote and unsafe { not_a_site() }";
    unsafe { seven() };
    let j = "spanning
    unsafe { not_a_site() }
    three lines";
    unsafe { eight() };
    let k: &'a str = "";
    unsafe { nine() };
}
'''
        found = [(site.kind, site.text, site.line) for site in self.sites(source)]
        self.assertEqual(found, [
            ('block', 'unsafe { one() }', 3),
            ('block', 'unsafe { two() }', 6),
            ('block', 'unsafe { three() }', 9),
            ('block', 'unsafe { four() }', 11),
            ('block', 'unsafe { five() }', 13),
            ('block', 'unsafe { six() }', 16),
            ('block', 'unsafe { seven() }', 18),
            ('block', 'unsafe { eight() }', 22),
            ('block', 'unsafe { nine() }', 24),
        ])

    def test_normalized_text_ignores_layout_and_comments(self):
        one = 'fn demo() {\n    unsafe { first(a, b); second() };\n}\n'
        two = 'fn demo() {\n    unsafe {\n        // why\n        first(\n            a,\n            b,\n        );\n        second()\n    };\n}\n'
        self.assertEqual(self.sites(one)[0].text, self.sites(two)[0].text)

    def test_enclosing_items_are_named(self):
        source = '''\
mod outer {
    impl<T: Bound> Thing<T> for Other<T> where T: Sized {
        fn method(&self) { unsafe { a() }; }
    }
    pub unsafe trait Marker {}
}
macro_rules! implement { ($t:ty) => { unsafe impl Marker for $t {} }; }
const FIXED: u8 = unsafe { b() };
'''
        self.assertEqual([site.item for site in self.sites(source)], [
            'outer::impl Thing for Other::method',
            'outer',
            'macro_rules! implement',
            '(file)',
        ])

    def test_unterminated_input_is_refused_not_ignored(self):
        with self.assertRaises(gate.ScanError):
            self.sites('fn demo() { /* never closed')
        with self.assertRaises(gate.ScanError):
            self.sites('fn demo() { let a = "never closed; }')


class GateControls(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        seed(self.root, SOURCE)

    def gate(self, *args):
        return run_gate(self.root, '--inventory', 'inventory.toml', *args)

    def test_a_new_unsafe_block_fails_naming_file_and_item(self):
        write(self.root, 'src/lib.rs', SOURCE + 'fn added() { unsafe { other() }; }\n')
        code, output = self.gate()
        self.assertEqual(code, 1, output)
        self.assertIn('new site', output)
        self.assertIn('src/lib.rs', output)
        self.assertIn('added', output)

    def test_a_new_unsafe_impl_fails(self):
        write(self.root, 'src/lib.rs', SOURCE + 'unsafe impl Sync for Handle {}\n')
        code, output = self.gate()
        self.assertEqual(code, 1, output)
        self.assertIn('unsafe impl Sync for Handle', output)

    def test_the_word_in_a_comment_or_a_string_passes(self):
        write(self.root, 'src/lib.rs', SOURCE + '// unsafe { fake() }\nconst NOTE: &str = "unsafe { fake() }";\n')
        code, output = self.gate()
        self.assertEqual(code, 0, output)

    def test_a_removed_site_fails(self):
        write(self.root, 'src/lib.rs', SOURCE.replace('unsafe impl Send for Handle {}\n', ''))
        code, output = self.gate()
        self.assertEqual(code, 1, output)
        self.assertIn('gone', output)
        self.assertIn('src/lib.rs', output)

    def test_a_moved_site_passes(self):
        write(self.root, 'src/lib.rs', '// a new line above\n\n' + SOURCE)
        code, output = self.gate()
        self.assertEqual(code, 0, output)

    def test_a_reformatted_site_passes(self):
        write(self.root, 'src/lib.rs', SOURCE.replace('unsafe { dangerous(a) };', 'unsafe {\n        dangerous(\n            a,\n        )\n    };'))
        code, output = self.gate()
        self.assertEqual(code, 0, output)

    def test_changing_the_unsafe_code_fails(self):
        write(self.root, 'src/lib.rs', SOURCE.replace('dangerous(a)', 'dangerous(a + 1)'))
        code, output = self.gate()
        self.assertEqual(code, 1, output)
        self.assertIn('new site', output)
        self.assertIn('gone', output)

    def test_a_duplicate_site_is_counted(self):
        write(self.root, 'src/lib.rs', SOURCE.replace('unsafe { dangerous(a) };', 'unsafe { dangerous(a) };\n    unsafe { dangerous(a) };'))
        code, output = self.gate()
        self.assertEqual(code, 1, output)
        self.assertIn('1 more than listed', output)

    def test_a_site_with_no_reason_fails(self):
        inventory = gate.load_inventory(inventory_path(self.root))
        inventory.entries[0].reason = ''
        inventory_path(self.root).write_text(gate.format_inventory(inventory.rows, inventory.entries, inventory.test_paths))
        code, output = self.gate()
        self.assertEqual(code, 1, output)
        self.assertIn('no reason', output)
        self.assertIn('src/lib.rs', output)

    def test_a_reason_naming_a_row_that_does_not_exist_fails(self):
        inventory = gate.load_inventory(inventory_path(self.root))
        inventory.entries[0].reason = 'row:not-a-row'
        inventory_path(self.root).write_text(gate.format_inventory(inventory.rows, inventory.entries, inventory.test_paths))
        code, output = self.gate()
        self.assertEqual(code, 1, output)
        self.assertIn('not-a-row', output)
        self.assertIn('does not exist', output)

    def test_a_reason_of_neither_form_fails(self):
        inventory = gate.load_inventory(inventory_path(self.root))
        inventory.entries[0].reason = 'because'
        inventory_path(self.root).write_text(gate.format_inventory(inventory.rows, inventory.entries, inventory.test_paths))
        code, output = self.gate()
        self.assertEqual(code, 1, output)
        self.assertIn('neither', output)

    def test_a_test_only_reason_is_refused_on_production_code(self):
        inventory = gate.load_inventory(inventory_path(self.root))
        inventory.entries[0].reason = 'test-only: not really'
        inventory_path(self.root).write_text(gate.format_inventory(inventory.rows, inventory.entries, inventory.test_paths))
        code, output = self.gate()
        self.assertEqual(code, 1, output)
        self.assertIn('outside test paths', output)

    def test_a_test_only_reason_stating_nothing_fails(self):
        write(self.root, 'tests/fixture.rs', 'fn t() { unsafe { x() }; }\n')
        write(self.root, 'src/lib.rs', SOURCE)
        code, output = self.gate('--update')
        self.assertEqual(code, 0, output)
        inventory = gate.load_inventory(inventory_path(self.root))
        for entry in inventory.entries:
            entry.reason = 'test-only:' if entry.file.startswith('tests/') else ROW
        inventory_path(self.root).write_text(gate.format_inventory(inventory.rows, inventory.entries, inventory.test_paths))
        code, output = self.gate()
        self.assertEqual(code, 1, output)
        self.assertIn('states nothing', output)

    def test_a_test_only_reason_is_accepted_on_test_code_and_cfg_test(self):
        write(self.root, 'tests/fixture.rs', 'fn t() { unsafe { x() }; }\n')
        write(self.root, 'src/lib.rs', SOURCE + '#[cfg(test)]\nmod tests {\n    fn t() { unsafe { x() }; }\n}\n'
              '#[tokio::test(flavor = "multi_thread")]\nasync fn tests_async() { unsafe { y() }; }\n')
        code, output = self.gate('--update')
        self.assertEqual(code, 0, output)
        inventory = gate.load_inventory(inventory_path(self.root))
        for entry in inventory.entries:
            test_code = entry.file.startswith('tests/') or 'tests' in entry.item
            entry.reason = 'test-only: a fixture' if test_code else ROW
        inventory_path(self.root).write_text(gate.format_inventory(inventory.rows, inventory.entries, inventory.test_paths))
        code, output = self.gate()
        self.assertEqual(code, 0, output)

    def test_update_keeps_reasons_and_leaves_new_sites_failing(self):
        write(self.root, 'src/lib.rs', SOURCE + 'fn added() { unsafe { other() }; }\n')
        code, output = self.gate('--update')
        self.assertEqual(code, 0, output)
        self.assertIn('1 still need a reason', output)
        code, output = self.gate()
        self.assertEqual(code, 1, output)
        self.assertIn('no reason', output)
        kept = [entry for entry in gate.load_inventory(inventory_path(self.root)).entries if entry.reason == ROW]
        self.assertEqual(len(kept), 2, 'the old sites keep their reasons')

    def test_a_malformed_inventory_is_refused(self):
        inventory_path(self.root).write_text('schema = 2\n')
        code, output = self.gate()
        self.assertEqual(code, 2, output)
        inventory_path(self.root).write_text('schema = 1\n[[site]]\nfile = "src/lib.rs"\n')
        code, output = self.gate()
        self.assertEqual(code, 2, output)


class PathArgumentControls(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)

    def test_another_crate_is_checked_through_path_arguments(self):
        write(self.root, 'crate-a/src/lib.rs', 'pub fn safe() {}\n')
        write(self.root, 'crate-b/src/lib.rs', 'fn risky() { unsafe { x() }; }\n')
        crate_a = self.root / 'crate-a'
        crate_b = self.root / 'crate-b'
        code, output = run_gate(crate_a, '--require-zero', paths=('src',))
        self.assertEqual(code, 0, output)
        code, output = run_gate(crate_b, '--require-zero', paths=('src',))
        self.assertEqual(code, 1, output)
        self.assertIn('risky', output)
        # A crate with sites keeps its own inventory, wherever it lives.
        write(crate_b, 'inventory.toml', 'schema = 1\n[rows]\nffi = "Foreign calls"\n')
        code, output = run_gate(crate_b, '--inventory', 'inventory.toml', paths=('src',))
        self.assertEqual(code, 1, output)
        self.assertIn('new site', output)
        code, output = run_gate(crate_b, '--inventory', 'inventory.toml', '--update', paths=('src',))
        self.assertEqual(code, 0, output)
        inventory = gate.load_inventory(crate_b / 'inventory.toml')
        self.assertEqual([entry.item for entry in inventory.entries], ['risky'])
        inventory.entries[0].reason = 'row:ffi'
        (crate_b / 'inventory.toml').write_text(gate.format_inventory(inventory.rows, inventory.entries, inventory.test_paths))
        code, output = run_gate(crate_b, '--inventory', 'inventory.toml', paths=('src',))
        self.assertEqual(code, 0, output)

    def test_a_missing_path_is_an_error_not_an_empty_pass(self):
        (self.root / 'src').mkdir()
        code, output = run_gate(self.root, '--require-zero', paths=('src', 'absent'))
        self.assertEqual(code, 2, output)


class DefaultRoots(unittest.TestCase):
    def test_every_rust_file_in_the_repository_is_under_a_default_root(self):
        missed = []
        for path in gate.ROOT.rglob('*.rs'):
            parts = path.relative_to(gate.ROOT).parts
            if set(parts) & gate.SKIPPED_DIRS:
                continue
            if parts[0] not in gate.DEFAULT_PATHS:
                missed.append('/'.join(parts))
        self.assertEqual(sorted(missed), [])


if __name__ == '__main__':
    unittest.main(verbosity=2)

#!/usr/bin/env python3
# Copyright (c) 2026 Contributors to the Eclipse Foundation
# Licensed under the Apache License, Version 2.0. SPDX-License-Identifier: Apache-2.0
"""R34/R48/R59 source-applicability controls; no compiler or product tests."""
import pathlib
import tempfile
import unittest
from check_core_gates import cfg_value, core_source_violations, hook_violations, rust_tokens


class SourceApplicability(unittest.TestCase):
    def tree(self, sources):
        with tempfile.TemporaryDirectory(prefix='source-scan-', dir=pathlib.Path(__file__).parent) as directory:
            root = pathlib.Path(directory)
            for name, source in sources.items():
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(source, encoding='utf-8')
            return core_source_violations(root)

    def source(self, text):
        return self.tree({'core/src/lib.rs': text})

    def test_compound_test_only_scopes(self):
        for predicate in ('test', 'all(test, unix)', 'not(not(test))',
                          'any(all(test, target_os = "linux"), all(test, windows))', 'all()', 'any()'):
            with self.subTest(predicate=predicate):
                self.assertEqual(bool(self.source(f'#[cfg({predicate})] mod only {{ use std::boxed::Box; }}')),
                                 predicate == 'all()')
        self.assertEqual(self.source('''
            #[cfg(test)] mod a { mod nested { extern crate std; } }
            #[cfg(test)] fn helper() { use core::sync as s; }
            #[cfg(test)] extern crate std;
            #[cfg(test)] mod name_is_irrelevant { pub fn __hook() {} }
        '''), [])

    def test_unknown_flags_and_targets_remain_shipping(self):
        for predicate in ('made_up_flag', 'feature = "future"', 'target_os = "other"', 'not(test)',
                          'any(test, windows)', 'any(test, up_rust_test_hooks)', 'all(unix, not(unix))'):
            with self.subTest(predicate=predicate):
                self.assertTrue(self.source(f'#[cfg({predicate})] mod x {{ use std::boxed::Box; }}'))

    def test_disabled_scope_does_not_extend_to_later_items(self):
        for suffix in ('use std::boxed::Box;', 'use core::sync as a;', 'pub fn __unguarded() {}'):
            with self.subTest(suffix=suffix):
                self.assertTrue(self.source('#[cfg(test)] mod tests {}\n' + suffix))
                self.assertTrue(self.source('#[cfg(test)] fn f() {}\n' + suffix))

    def test_nested_and_external_test_modules(self):
        self.assertEqual(self.tree({'core/src/lib.rs': '#[cfg(test)] mod checks;',
            'core/src/checks.rs': 'mod deeper; use std::boxed::Box;',
            'core/src/checks/deeper.rs': 'extern crate std; pub fn __test() {}'}), [])
        self.assertEqual(self.tree({'core/src/lib.rs': 'mod runtime { #[cfg(test)] mod cases; }',
            'core/src/runtime/cases.rs': 'use core::sync::atomic::AtomicU32;'}), [])

    def test_path_alias_shared_by_test_and_shipping_edges(self):
        for declarations in (
            '#[cfg(test)] #[path="shared.rs"] mod a; #[path="shared.rs"] mod b;',
            '#[path="shared.rs"] mod b; #[cfg(test)] #[path="shared.rs"] mod a;'):
            with self.subTest(declarations=declarations):
                hits = self.tree({'core/src/lib.rs': declarations, 'core/src/shared.rs': 'use std::boxed::Box;'})
                self.assertTrue(any('shared.rs:1: forbidden' in hit for hit in hits), hits)

    def test_non_test_path_aliases_remain_checked(self):
        hits = self.tree({'core/src/lib.rs': '#[cfg(target_os="windows")] #[path="x.rs"] mod x;',
                          'core/src/x.rs': 'use core::sync as s;'})
        self.assertTrue(any('atomic import' in hit for hit in hits), hits)

    def test_explicit_path_on_test_module(self):
        self.assertEqual(self.tree({'core/src/lib.rs': '#[cfg(test)] #[path="fixtures.rs"] mod x;',
                                    'core/src/fixtures.rs': 'use std::boxed::Box;'}), [])

    def test_include_inherits_test_and_shipping_context(self):
        cases = {'core/src/lib.rs': '#[cfg(test)] mod x { include!("bits.rs"); }',
                 'core/src/bits.rs': 'use std::boxed::Box;'}
        self.assertEqual(self.tree(cases), [])
        cases['core/src/lib.rs'] += '\ninclude!("bits.rs");'
        self.assertTrue(self.tree(cases))

    def test_include_preserves_module_context(self):
        self.assertEqual(self.tree({'core/src/lib.rs': '#[cfg(test)] mod a { include!("fragment.rs"); }',
            'core/src/fragment.rs': 'mod b;', 'core/src/a/b.rs': 'use std::boxed::Box;'}), [])

    def test_orphan_is_not_inferred_to_be_a_test(self):
        hits = self.tree({'core/src/lib.rs': '', 'core/src/looks_like_tests.rs': 'extern crate std;'})
        self.assertTrue(any('looks_like_tests.rs' in hit for hit in hits), hits)

    def test_missing_ambiguous_and_cyclic_sources_refuse(self):
        for files in ({'core/src/lib.rs': 'mod absent;'},
            {'core/src/lib.rs': 'mod a;', 'core/src/a.rs': '', 'core/src/a/mod.rs': ''},
            {'core/src/lib.rs': 'include!("lib.rs");'}):
            with self.subTest(files=files):
                self.assertTrue(any('source applicability refused' in hit for hit in self.tree(files)))

    def test_inner_cfg_and_scope_inheritance(self):
        self.assertEqual(self.source('mod x { #![cfg(test)] use std::boxed::Box; }'), [])
        self.assertEqual(self.tree({'core/src/lib.rs': 'mod a;',
                                    'core/src/a.rs': '#![cfg(test)] use std::boxed::Box;'}), [])
        self.assertTrue(self.source('mod x {} #![cfg(test)] use std::boxed::Box;'))

    def test_no_filename_based_atomic_facade_exemption(self):
        self.assertEqual(self.tree({'core/src/lib.rs': 'mod platform;', 'core/src/platform/mod.rs': 'mod atomic;',
            'core/src/platform/atomic.rs': 'pub use core::sync::atomic::AtomicU32;'}), [])
        self.assertTrue(self.tree({'core/src/lib.rs': 'mod atomic;',
                                  'core/src/atomic.rs': 'pub use core::sync::atomic::AtomicU32;'}))

    def test_shipping_forbidden_modules_and_spacing(self):
        for text in ('std::boxed::Box', 'alloc :: vec :: Vec', 'core :: future :: Future',
                     'core::task::Poll', 'core::pin::Pin', 'r#std::boxed::Box',
                     'core/*nested /*comment*/ */::future::Future'):
            with self.subTest(text=text):
                self.assertTrue(self.source(f'fn f() {{ let _ = {text}; }}'))
        for crate in ('std', 'alloc'):
            self.assertTrue(self.source(f'extern crate {crate};'))
        for text in ('use core::{future::Future, fmt};', 'use core::{task as t};',
                     'use {core::{pin::{self as p}}};', 'use alloc as a;'):
            with self.subTest(text=text):
                self.assertTrue(self.source(text))

    def test_shipping_direct_grouped_and_aliased_atomics(self):
        for text in ('use core::sync::atomic::AtomicU32;', 'use core as c;', 'use core::{sync as s};',
                     'use core::{sync::{atomic as a}};', 'use core::*;', 'extern crate core as c;',
                     'fn f() { let _ = core::sync::atomic::AtomicU32::new(0); }'):
            with self.subTest(text=text):
                self.assertTrue(any('atomic import' in hit for hit in self.source(text)))

    def test_comments_and_all_string_families_cannot_change_scope(self):
        source = '''
            // #[cfg(test)] mod hidden { std::boxed::Box }
            /* outer /* #[cfg(test)] */ mod fake { } */
            const A: &str = "#[cfg(test)] mod hidden {";
            const B: &str = r###"} #[cfg(test)] /*"###;
            const C: &core::ffi::CStr = cr##"#[cfg(test)] } /*"##;
            const D: &core::ffi::CStr = c"std::sync::atomic";
            const E: &[u8] = br#"core::future::Future"#;
        '''
        self.assertEqual(self.source(source), [])
        self.assertTrue(self.source(source + '\nuse std::boxed::Box;'))
        self.assertTrue(self.source(source + '\nuse core::sync as s;'))

    def test_malformed_and_unknown_cfg_operator_refuse(self):
        for cfg in ('not()', 'not(test, unix)', 'all(test unix)', 'unknown(test)',
                    'all(test, unknown())', 'feature = 3', 'test =', 'all(test) junk'):
            with self.subTest(cfg=cfg):
                self.assertTrue(any('source applicability refused' in hit for hit in
                                    self.source(f'#[cfg({cfg})] mod m {{}}')))
        for text in ('#[cfg] mod x {}', '#[cfg(test] mod x {}', '#[cfg(test)]',
                     '/*', '/* unclosed', 'const S: &str = "unclosed', 'const S: &str = r##"unclosed'):
            with self.subTest(text=text):
                self.assertTrue(self.source(text))

    def test_cfg_attr_structural_forms_fail_closed(self):
        for attr in ('cfg_attr(test, cfg(test))', 'cfg_attr(unix, path="x.rs")',
                     'cfg_attr(test, cfg_attr(unix, path="x.rs"))'):
            with self.subTest(attr=attr):
                self.assertTrue(self.source(f'#[{attr}] mod x {{}}'))
        self.assertEqual(self.source('#[cfg_attr(unix, allow(dead_code))] fn f() {}'), [])

    def test_path_and_include_limits_are_explicit(self):
        for text in ('include!(concat!("a", ".rs"));', '#[path=concat!("a", ".rs")] mod a;',
                     '#[path="a.rs"] mod a {}', '#[path="a\\x2ers"] mod a;',
                     'include!("../../../../outside.rs");'):
            with self.subTest(text=text):
                self.assertTrue(any('source applicability refused' in hit for hit in self.source(text)))

    def test_exported_macros_do_not_gain_cfg_exemptions_at_definition(self):
        for body in ('#[cfg(test)] fn f() { use std::boxed::Box; }',
                     '#[cfg(test)] fn f() { use core::sync as s; }', '#[cfg(test)] pub fn __hook() {}'):
            with self.subTest(body=body):
                self.assertTrue(self.source('#[macro_export] macro_rules! m { () => {' + body + '}; }'))
        self.assertEqual(self.source('#[cfg(test)] #[macro_export] macro_rules! m { () => { use std::boxed::Box; }; }'), [])

    def test_macro_generated_module_and_include_fail_closed(self):
        for body in ('mod shipping;', '#[cfg(test)] mod shipping {}', 'include!("other.rs");'):
            with self.subTest(body=body):
                self.assertTrue(any('macro-generated module/include' in hit for hit in
                    self.source('macro_rules! m { () => {' + body + '}; }')))

    def test_macro_interpolation_cannot_construct_unchecked_namespaces_or_hooks(self):
        for text in (
            'macro_rules! m { ($c:ident) => { $c::boxed::Box }; } m!(std);',
            'macro_rules! m { ($m:ident) => { core::$m::Future }; } m!(future);',
            'macro_rules! m { ($n:ident) => { pub fn $n() {} }; } m!(__hook);',
            'm!(core, sync, atomic);', 'concat_idents!(co, re);',
        ):
            with self.subTest(text=text):
                self.assertTrue(any('source applicability refused' in hit for hit in self.source(text)))
        self.assertEqual(self.tree({'core/src/lib.rs': '#[cfg(test)] mod t;',
            'core/src/t.rs': 'macro_rules! m { () => { std::boxed::Box }; }'}), [])

    def test_hook_file_with_two_parent_conditions_remains_visible(self):
        self.assertTrue(self.tree({
            'core/src/lib.rs': '#[cfg(up_rust_test_hooks)] #[path="h.rs"] mod a; #[path="h.rs"] mod b;',
            'core/src/h.rs': 'pub fn __hook() {}',
        }))

    def test_literal_paths_with_inline_and_non_mod_rs_context(self):
        self.assertEqual(self.tree({
            'core/src/lib.rs': 'mod a;',
            'core/src/a.rs': '#[cfg(test)] mod inline { #[path="cases.rs"] mod c; }',
            'core/src/a/inline/cases.rs': 'use std::boxed::Box;',
        }), [])
        self.assertEqual(self.tree({
            'core/src/lib.rs': '#[cfg(test)] #[path="external.rs"] mod a;',
            'core/src/external.rs': 'mod child;',
            'core/src/child.rs': 'use std::boxed::Box;',
        }), [])

    def test_function_pointer_and_boolean_negation_are_not_items_or_macros(self):
        self.assertEqual(self.source('struct S { f: fn(u8) -> u8 } fn g(f: impl Fn(u8)) { if !false {} }'), [])

    def test_hook_gate_compound_predicates_and_inheritance(self):
        for cfg in ('up_rust_test_hooks', 'any(test, up_rust_test_hooks)', 'all(up_rust_test_hooks, unix)',
                    'all(test, feature="f")', 'not(not(up_rust_test_hooks))'):
            with self.subTest(cfg=cfg):
                self.assertEqual(hook_violations(f'#[cfg({cfg})] pub fn __hook() {{}}'), [])
                self.assertEqual(hook_violations(f'#[cfg({cfg})] mod x {{ pub fn __hook() {{}} }}'), [])
                self.assertEqual(hook_violations(f'#[cfg({cfg})] impl T {{ pub fn __hook() {{}} }}'), [])
        self.assertEqual(self.tree({'core/src/lib.rs': '#[cfg(any(test, up_rust_test_hooks))] mod hooks;',
                                    'core/src/hooks.rs': 'pub fn __hook() {}'}), [])

    def test_hook_gate_negative_controls(self):
        for cfg in ('any(test, feature="test-util")', 'any(up_rust_test_hooks, windows)',
                    'not(test)', 'not(up_rust_test_hooks)', 'unknown', 'all()'):
            with self.subTest(cfg=cfg):
                self.assertTrue(hook_violations(f'#[cfg({cfg})] pub fn __hook() {{}}'))
        for text in ('#[doc(hidden)] pub fn __hook() {}',
                     '// #[cfg(up_rust_test_hooks)]\npub fn __hook() {}',
                     'const S: &str="#[cfg(up_rust_test_hooks)]"; pub fn __hook() {}',
                     'pub const fn\n__hook() {}', 'pub fn startup_values() {}'):
            with self.subTest(text=text):
                self.assertTrue(hook_violations(text))

    def test_documented_seam_stays_exact(self):
        for name in ('__route_wake', '__wake', '__ttl', '__taken_value', '__taken_payload'):
            self.assertEqual(hook_violations(f'pub fn {name}() {{}}'), [])
        self.assertTrue(hook_violations('pub fn __wake_more() {}'))

    def test_cfg_truth_table_is_conservative(self):
        for source, expected in [('test', False), ('all()', True), ('any()', False),
                                 ('all(test, unknown)', False), ('any(test, unknown)', None),
                                 ('not(unknown)', None), ('test="yes"', None)]:
            with self.subTest(source=source):
                self.assertIs(cfg_value([t for t, _ in rust_tokens(source, literals=True)], frozenset({'test'})), expected)


if __name__ == '__main__':
    unittest.main(verbosity=2)

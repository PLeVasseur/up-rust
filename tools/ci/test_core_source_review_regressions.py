#!/usr/bin/env python3
# Copyright (c) 2026 Contributors to the Eclipse Foundation
# Licensed under the Apache License, Version 2.0. SPDX-License-Identifier: Apache-2.0
"""F1-F4 review regressions: helper fixtures only, without Rust execution."""
import pathlib
import tempfile
import unittest

from check_core_gates import SourceScanError, SourceView, core_source_violations, hook_violations, rust_tokens


class ReviewRegressions(unittest.TestCase):
    def tree(self, sources):
        with tempfile.TemporaryDirectory(prefix='review-source-', dir=pathlib.Path(__file__).parent) as directory:
            root = pathlib.Path(directory)
            for name, source in sources.items():
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(source, encoding='utf-8')
            return core_source_violations(root)

    def source(self, source):
        return self.tree({'core/src/lib.rs': source})

    def test_f1_exact_header_macro_does_not_mask_shipping_tokens(self):
        for value, diagnostic in (('core::sync::atomic::AtomicU32', 'atomic import'),
                                  ('core::future::Ready<()>', 'forbidden runtime'),
                                  ('core::task::Poll<()>', 'forbidden runtime'),
                                  ('core::pin::Pin<&u8>', 'forbidden runtime'),
                                  ('std::boxed::Box<u8>', 'forbidden runtime'),
                                  ('alloc::vec::Vec<u8>', 'forbidden runtime')):
            source = '''macro_rules! discard_marker {
                (#![$ignored:meta] $ty:ty) => { $ty };
            }
            pub type Shipping = discard_marker! { #![cfg(test)] VALUE };
            '''.replace('VALUE', value)
            with self.subTest(value=value):
                view = SourceView(source, frozenset({'test'}), modules=True)
                self.assertIn(value, view.text())
                self.assertTrue(any(diagnostic in hit for hit in self.source(source)))

    def test_f1_every_header_delimiter_and_nested_position_is_opaque(self):
        for macro in ('identity!{#![cfg(test)] core::sync::atomic::AtomicU32}',
                      'identity!(#![cfg(test)] core::sync::atomic::AtomicU32)',
                      'identity![#![cfg(test)] core::sync::atomic::AtomicU32]',
                      'outer!(inner!{#![cfg(test)] core::sync::atomic::AtomicU32})',
                      'path::identity!{#![cfg(test)] core::sync::atomic::AtomicU32}'):
            for template in ('pub type T = M;', 'pub type T = Wrapper<M>;',
                             'pub fn f(_: M) {}', 'pub fn f() -> M { loop {} }',
                             'struct S { field: M }', 'type A = [u8; M];',
                             'static S: usize = M;', 'const C: usize = M;',
                             'fn f() { let _ = M; }', 'enum E { A = M }'):
                with self.subTest(macro=macro, template=template):
                    source = template.replace('M', macro)
                    self.assertIn('core::sync::atomic', SourceView(source, frozenset({'test'}), modules=True).text())
                    self.assertTrue(any('atomic import' in hit for hit in self.source(source)))

    def test_f1_macro_payload_cfg_is_not_source_cfg(self):
        for predicate in ('test', 'any(test, unknown)', 'unknown(test)', 'not()'):
            source = f'type T = identity!{{#![cfg({predicate})] core::future::Future}};'
            with self.subTest(predicate=predicate):
                self.assertTrue(any('forbidden runtime' in hit for hit in self.source(source)))
        # Only the real enclosing source condition may exclude the invocation.
        self.assertEqual(self.source('#[cfg(test)] type T = identity!{#![cfg(not(test))] std::boxed::Box<u8>};'), [])
        self.assertTrue(self.source('#[cfg(any(test, other))] type T = identity!{#![cfg(test)] std::boxed::Box<u8>};'))

    def test_f1_header_macro_cannot_hide_hook_declarations(self):
        source = 'const C: usize = maker!{#![cfg(test)] pub fn __shipping_hook() {} 0};'
        self.assertTrue(hook_violations(source))
        self.assertEqual(hook_violations('#[cfg(any(test, up_rust_test_hooks))] ' + source), [])

    def test_f2_exact_static_include_outside_core_src(self):
        source = 'pub static SIZE: usize = include!("../shipping-expression.rs");'
        hits = self.tree({'core/src/lib.rs': source,
                          'core/shipping-expression.rs': '{ core::sync::atomic::AtomicU32::new(0); 0 }'})
        self.assertTrue(any('core/shipping-expression.rs:1: atomic import' in hit for hit in hits), hits)
        view = SourceView(source, frozenset({'test'}), modules=True)
        self.assertEqual(view.edges[0][:4], ('include', '../shipping-expression.rs', (), True))

    def test_f2_include_in_headers_initializers_and_expressions(self):
        for template in ('static N: usize = I;', 'const N: usize = I;',
                         'type T = I;', 'type T = [u8; I];', 'fn f(_: I) {}',
                         'fn f() -> I { loop {} }', 'struct S { t: I }',
                         'fn f() { let _ = I; }', 'fn f() { let _ = (I, 1); }',
                         'fn f() { let _ = [I]; }', '#[doc = I] fn f() {}',
                         'enum E { A = I }', 'static N: usize = { I };'):
            with self.subTest(template=template):
                source = template.replace('I', 'include!("../piece.rs")')
                hits = self.tree({'core/src/lib.rs': source, 'core/piece.rs': 'core::future::Future'})
                self.assertTrue(any('core/piece.rs:1: forbidden runtime' in hit for hit in hits), hits)

    def test_f2_test_and_shipping_includes_do_not_depend_on_edge_order(self):
        shipping = 'static N: usize = include!("shared.rs");'
        test = '#[cfg(test)] #[path="shared.rs"] mod tests;'
        for source in (test + shipping, shipping + test):
            with self.subTest(source=source):
                hits = self.tree({'core/src/lib.rs': source,
                                  'core/src/shared.rs': 'core::sync::atomic::AtomicU32::new(0)'})
                self.assertTrue(any('core/src/shared.rs:1: atomic import' in hit for hit in hits), hits)
        self.assertEqual(self.tree({'core/src/lib.rs': '#[cfg(test)] ' + shipping,
            'core/src/shared.rs': 'core::sync::atomic::AtomicU32::new(0)'}), [])

    def test_f2_include_context_and_nested_relative_paths(self):
        self.assertEqual(self.tree({
            'core/src/lib.rs': '#[cfg(test)] mod outer { const N: usize = include!("../first.rs"); }',
            'core/first.rs': 'include!("second.rs")',
            'core/second.rs': 'core::future::Future',
        }), [])
        hits = self.tree({
            'core/src/lib.rs': '#[cfg(any(test, unknown))] mod outer { const N: usize = include!("../first.rs"); }',
            'core/first.rs': 'include!("second.rs")',
            'core/second.rs': 'core::future::Future',
        })
        self.assertTrue(any('core/second.rs:1: forbidden runtime' in hit for hit in hits), hits)

    def test_f2_computed_header_include_and_generated_nested_include_refuse(self):
        for source in ('static N: usize = include!(concat!("a", ".rs"));',
                       'type T = include!(env!("INPUT"));',
                       'static N: usize = wrapper!(include!("a.rs"));',
                       'type T = wrapper!{include!("a.rs")};'):
            with self.subTest(source=source):
                self.assertTrue(any('source applicability refused' in hit for hit in self.source(source)))
        self.assertEqual(self.source('const S: &str = include_str!("missing.txt"); const B: &[u8] = include_bytes!("missing.bin");'), [])

    def test_f3_exact_metavariable_macro_invocation_refuses(self):
        for invocation in ('$m!("../shipping-items.rs");', '$ m ! {"../shipping-items.rs"}',
                           '$m!["../shipping-items.rs"];', '$crate::$m!("../shipping-items.rs");',
                           '$($m)?!("../shipping-items.rs");', '$($m)*!("../shipping-items.rs");',
                           '$($m),*!("../shipping-items.rs");'):
            with self.subTest(invocation=invocation):
                source = 'macro_rules! invoke { ($m:ident) => {' + invocation + '}; } invoke!(include);'
                hits = self.source(source)
                self.assertTrue(any('metavariable macro invocation' in hit for hit in hits), hits)

    def test_f3_header_macro_cannot_conceal_dynamic_invocation(self):
        self.assertTrue(any('metavariable macro invocation' in hit for hit in
            self.source('type T = wrapper! { $m!("x.rs") };')))
        self.assertEqual(self.source('#[cfg(test)] macro_rules! m { ($m:ident) => {$m!("x.rs")}; }'), [])
        self.assertEqual(self.source('macro_rules! m { ($name:ident) => {pub fn $name() {} $crate::helper!();}; } m!(send);'), [])
        self.assertEqual(self.source('macro_rules! m { () => { let _ = f() * !(false); }; }'), [])

    def test_f3_include_names_and_import_aliases_do_not_gain_an_unseen_edge(self):
        for source in ('use core::include as load; static N: usize = load!("../x.rs");',
                       'use core::{include as load}; static N: usize = load!("../x.rs");',
                       'macro_rules! m { ($p:path) => { use $p as load; }; } m!(core::include);',
                       'm!(include);'):
            with self.subTest(source=source):
                self.assertTrue(any('source applicability refused' in hit for hit in self.source(source)))
        self.assertEqual(self.source('#[cfg(test)] use core::include as load;'), [])

    def test_f4_unicode_hook_suffix_and_raw_identifier_refuse(self):
        for name in ('__wakeé', '__wake\u0301', '__wаke', 'r#__wakeé', '__taken_payloadλ', 'startup_valuesé'):
            with self.subTest(name=name):
                source = f'pub fn {name}() {{}}'
                self.assertTrue(any('non-ASCII code token' in hit for hit in hook_violations(source)))
                self.assertTrue(any('non-ASCII code token' in hit for hit in self.source(source)))
        with self.assertRaisesRegex(SourceScanError, 'non-ASCII code token'):
            rust_tokens('macro_rules! m { ($é:ident) => {}; }')

    def test_f4_unicode_comments_and_literal_families_are_not_code(self):
        source = '''// __wakeé
            /* nested /* __wakeé */ harmless */
            const S: &str = "__wakeé";
            const R: &str = r#"__wakeé"#;
            const C: char = 'é';
            const B: &[u8] = b"é";
            const BR: &[u8] = br#"é"#;
            const CS: &core::ffi::CStr = c"é";
            const CR: &core::ffi::CStr = cr#"é"#;
            pub fn __wake() {}
        '''
        self.assertEqual(hook_violations(source), [])
        self.assertEqual(self.source(source), [])
        self.assertTrue(any('non-ASCII code token' in hit for hit in hook_violations(source + '\npub fn __wakeé() {}')))
        self.assertEqual(self.tree({'core/src/lib.rs': 'const N: usize = include!("café.rs");',
                                    'core/src/café.rs': '0'}), [])

    def test_f4_non_ascii_code_never_takes_an_allowed_ascii_prefix(self):
        for source in ('#[cfg(tést)] mod x {}', 'mod naïve;', 'pub fn __wakeé() {}',
                       'type T = macroé!{#![cfg(test)] std::boxed::Box<u8>};'):
            with self.subTest(source=source):
                self.assertTrue(any('non-ASCII code token' in hit for hit in self.source(source)))


if __name__ == '__main__':
    unittest.main(verbosity=2)

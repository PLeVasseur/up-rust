#!/usr/bin/env python3
# Copyright (c) 2026 Contributors to the Eclipse Foundation
# Licensed under the Apache License, Version 2.0. SPDX-License-Identifier: Apache-2.0
"""Seven integration syntax regressions; no SDK source, compiler or runtime work."""
import pathlib
import tempfile
import unittest

from check_core_gates import SourceView, core_source_violations, hook_violations


class UnaryNegation(unittest.TestCase):
    def tree(self, sources):
        with tempfile.TemporaryDirectory(prefix='unary-source-', dir=pathlib.Path(__file__).parent) as directory:
            root = pathlib.Path(directory)
            for name, source in sources.items():
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(source, encoding='utf-8')
            return core_source_violations(root)

    def source(self, source):
        return self.tree({'core/src/lib.rs': source})

    def test_seven_reported_sites_use_ordinary_expression_groups(self):
        # Bounded token-equivalent excerpts; names/types are intentionally not
        # resolved or compiled. The owning source scope is core or hook-only.
        cases = {
            'core/src/frame/header.rs:788': '''fn validate_parts() {
                let concrete = |uri: &UUri| {
                    !(uri.has_wildcard_authority()
                      || uri.has_wildcard_entity_instance()
                      || uri.has_wildcard_entity_type()
                      || uri.has_wildcard_version()
                      || uri.has_wildcard_resource_id())
                };
            }''',
            'src/heap_guard.rs:386': '''fn charge() {
                let matches_window = generation.is_none_or(|expected| expected == current)
                    && window.open.load(Ordering::SeqCst)
                    && !(generation.is_some() && window.all_threads.load(Ordering::SeqCst));
                if matches_window { window.charge(op, size, align); }
            }''',
            'src/owned_frame.rs:347': '''fn attach() {
                let attachment = crate::OwnedAttachment::new(move || {
                    listeners.write().expect("lock").retain(|(sf, kf, l)| {
                        !(sf == &source && kf == &sink && std::sync::Arc::ptr_eq(l, &exact))
                    });
                    Ok(())
                });
            }''',
            'src/test_util/handoff.rs:719': '''fn receive() {
                entries.iter().all(|entry| {
                    let sink_matches = match pair {
                        (None, Some(_)) | (Some(_), None) => false,
                        _ => true,
                    };
                    !(kind_matches && source_matches && sink_matches) || entry.accepts(&lock(&entry.queue))
                });
            }''',
            'src/test_util/lending.rs:437': '''fn create() {
                let payload_start = (base + metadata_capacity + alignment - 1) & !(alignment - 1);
                slots.push(TxSlot { payload_offset: payload_start - base });
            }''',
            'src/zero_copy/pool_core.rs:400': '''fn setup() {
                let maximum = length.checked_add(alignment - 1)
                    .and_then(|value| {
                        (value & !(self.inner.config.alignment - 1)).checked_add(limits.max_payload_len)
                    }).ok_or(SetupError::InvalidConfig("route size overflow"))?;
            }''',
            'src/zero_copy/route.rs:443': '''fn send() {
                if bytes.len() != mem::size_of::<P>()
                    || !(bytes.as_ptr() as usize).is_multiple_of(mem::align_of::<P>())
                { return Err(SendError::PayloadStorage); }
            }''',
        }
        for site, source in cases.items():
            with self.subTest(site=site):
                view = SourceView(source, frozenset({'test', 'up_rust_test_hooks'}))
                self.assertEqual(view.errors, [])
                self.assertEqual(view.masked, [])
                self.assertEqual(view.text(), source)
                self.assertEqual(hook_violations(source), [])
                if site.startswith('core/'):
                    self.assertEqual(self.source(source), [])

    def test_punctuation_and_nested_unary_operands_are_not_macros(self):
        for expression in ('!(flag)', '!!(flag)', '(flags & !(mask))',
                           '(left && !(right))', '(left || !(right))',
                           '(!{ flag })', '(![flag][0])'):
            with self.subTest(expression=expression):
                source = 'fn f() { let value = ' + expression + '; }'
                view = SourceView(source, frozenset({'test'}), modules=True)
                self.assertEqual(view.macros, {})
                self.assertEqual(view.text(), source)
                self.assertEqual(self.source(source), [])

    def test_forbidden_tokens_inside_unary_groups_remain_visible(self):
        for body, diagnostic in (('std::mem::size_of::<u8>()', 'forbidden runtime'),
                                 ('core::sync::atomic::AtomicU32::new(0)', 'atomic import')):
            for expression in ('!({BODY})', '0 & !({BODY})', '|| {{ !({BODY}) }}'):
                source = 'fn f() { let _ = ' + expression.replace('{BODY}', body) + '; }'
                with self.subTest(source=source):
                    self.assertIn(body, SourceView(source, frozenset({'test'}), modules=True).text())
                    self.assertTrue(any(diagnostic in hit for hit in self.source(source)))

    def test_real_macro_after_unary_operator_keeps_cfg_payload_opaque(self):
        for invocation in ('emit! { #![cfg(test)] core::sync::atomic::AtomicU32 }',
                           'emit!(#![cfg(test)] core::sync::atomic::AtomicU32)',
                           'path::emit![#![cfg(test)] core::sync::atomic::AtomicU32]',
                           'r#emit!{#![cfg(test)] core::sync::atomic::AtomicU32}'):
            with self.subTest(invocation=invocation):
                source = 'fn f() { let _ = !(' + invocation + '); }'
                view = SourceView(source, frozenset({'test'}), modules=True)
                self.assertIn('core::sync::atomic', view.text())
                self.assertTrue(any('atomic import' in hit for hit in self.source(source)))
        source = 'fn f() { let _ = !(emit!{#![cfg(test)] pub fn __hook() {} 0}); }'
        self.assertTrue(hook_violations(source))

    def test_literal_include_below_unary_group_is_followed(self):
        hits = self.tree({
            'core/src/lib.rs': 'fn f() { let mask = |n| { n & !(include!("../operand.rs")) }; }',
            'core/operand.rs': 'core::sync::atomic::AtomicU32::new(0)',
        })
        self.assertTrue(any('core/operand.rs:1: atomic import' in hit for hit in hits), hits)

    def test_dynamic_macro_names_remain_refused_in_negated_payload(self):
        for invocation in ('$m!("../x.rs")', '$($m)?!("../x.rs")', '$($m),*!("../x.rs")'):
            source = 'macro_rules! emit { ($m:ident) => { !(' + invocation + ') }; } emit!(include);'
            with self.subTest(source=source):
                self.assertTrue(any('metavariable macro invocation' in hit for hit in self.source(source)))
        self.assertTrue(any('macro-generated module/include' in hit for hit in
                            self.source('fn f() { let _ = !(emit!(include!("../x.rs"))); }')))

    def test_outer_cfg_and_following_hooks_keep_their_scope(self):
        body = 'fn f() { let _ = !(std::mem::size_of::<u8>()); }'
        self.assertEqual(self.source('#[cfg(test)] ' + body), [])
        self.assertTrue(self.source('#[cfg(any(test, unknown))] ' + body))
        self.assertTrue(hook_violations('fn f() { let _ = !(flag); } pub fn __shipping_hook() {}'))


if __name__ == '__main__':
    unittest.main(verbosity=2)

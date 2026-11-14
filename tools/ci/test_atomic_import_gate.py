#!/usr/bin/env python3
# Copyright (c) 2026 Contributors to the Eclipse Foundation
# Licensed under the Apache License, Version 2.0. SPDX-License-Identifier: Apache-2.0
"""Focused source-gate controls. No compiler, target creation or whole gate matrix."""
import pathlib
import sys
import unittest

from check_core_gates import atomic_import_violations


class AtomicImportGate(unittest.TestCase):
    # Preparation for req~ssr-atomic-width~1; no gated requirement at neutral pin 733b006.
    # method: genuine direct, qualified, grouped, aliased, glob and whitespace bypasses.
    def test_atomic_import_negative_controls(self):
        cases = [
            "use core::sync::atomic::AtomicU32;",
            "pub use std::sync::atomic::{AtomicU32 as Word, Ordering};",
            "fn f() { let _ = ::core::sync::atomic::AtomicBool::new(false); }",
            "use core :: sync :: atomic :: *;",
            "use r#core::r#sync::r#atomic::AtomicU32;",
            "use core/* outer /* nested */ comment */::sync\n::atomic::Ordering;",
            "use core::{fmt, sync::{atomic::{AtomicU32, Ordering}}};",
            "use {core::{sync::{self as s}}, std::fmt};",
            "use core::sync as s;",
            "use core::{sync as s};",
            "use core as c;",
            "use {core};",
            "use core::*;",
            "use core::{self as c};",
            "extern crate core as c;",
            "use core::sync::*;",
            "#[cfg(any())] use core::sync::atomic::AtomicU64;",
            "macro_rules! bypass { () => { core::sync::atomic::AtomicU32 }; }",
        ]
        for source in cases:
            with self.subTest(source=source):
                self.assertTrue(atomic_import_violations(source, pathlib.PurePath("core/src/memory.rs")))

    # Preparation for req~ssr-atomic-width~1. method: allow facade and ordinary core imports.
    def test_atomic_import_positive_controls(self):
        cases = [
            "use crate::platform::atomic::{AtomicU64, Ordering};",
            "use core::{fmt, mem::{self, size_of}};",
            "// use core::sync::atomic::AtomicU32;\nuse core::fmt;",
            'const TEXT: &str = "core::sync::atomic";',
            'const TEXT: &str = r###"core::sync::atomic // /*"###;',
            "/* outer /* core::sync::atomic */ comment */ use core::fmt;",
            "fn core_value() {}",
        ]
        for source in cases:
            with self.subTest(source=source):
                self.assertEqual(atomic_import_violations(source, pathlib.PurePath("core/src/memory.rs")), [])
        source = "pub(crate) use core::sync::atomic::{AtomicU64, Ordering};"
        self.assertEqual(atomic_import_violations(source, pathlib.PurePath("core/src/platform/atomic.rs")), [])

    # Preparation for req~ssr-atomic-width~1. method: exact exemption and line diagnostics.
    def test_atomic_import_exemption_and_line(self):
        source = "// preface\nuse core::sync::atomic::AtomicU64;"
        for path in ["core/src/transport/atomic.rs", "core/src/platform/atomic.rs.bak",
                     "core/src/platform/nested/atomic.rs"]:
            self.assertTrue(atomic_import_violations(source, pathlib.PurePath(path)))
        self.assertEqual(atomic_import_violations(source, pathlib.PurePath("core/src/memory.rs"))[0].split(":")[0], "2")


    # Preparation for req~ssr-atomic-width~1. F1: literal data cannot hide imports.
    def test_atomic_import_c_string_boundaries(self):
        literals = [
            'cr#""/*"#',
            'cr##""# // /* core::sync::atomic"##',
            'cr"/*"',
            'c"\\\"/*"',
            'c"core::sync::atomic"',
        ]
        for literal in literals:
            source = 'const TEXT: &core::ffi::CStr = ' + literal + ';\n'
            with self.subTest(literal=literal):
                self.assertEqual(atomic_import_violations(
                    source + 'use core::fmt;', pathlib.PurePath('core/src/memory.rs')), [])
                errors = atomic_import_violations(
                    source + 'use core::sync::atomic::AtomicU32;',
                    pathlib.PurePath('core/src/memory.rs'))
                self.assertTrue(errors)
                self.assertTrue(all(error.startswith('2:') for error in errors))

    # Preparation for req~ssr-atomic-width~1. F2: unexpanded macro tokens terminate.
    def test_atomic_import_macro_tokens_progress(self):
        cases = [
            ('macro_rules! accepts_use { (use) => { let _ = 0; }; }\n'
             'fn example() { accepts_use!(use); }', False),
            ('macro_rules! accepts_use { (use {$name:ident}) => {{ let _ = 0; }}; }', False),
            ('macro_rules! accepts_use { (use) => { use core::{fmt, mem}; }; }', False),
            ('macro_rules! accepts_use { (use) => { use core::{sync as s}; }; }', True),
            ('macro_rules! accepts_use { (use) => { let _ = core::sync::atomic::AtomicU32::new(0); }; }', True),
            ('macro_rules! accepts_use { (use) => { let _ = 0; }; }\n'
             'use {core::{sync::{self as s}}};', True),
        ]
        for source, expected in cases:
            events = 0
            def bounded(frame, event, arg):
                nonlocal events
                if event == 'line' and frame.f_code.co_filename == atomic_import_violations.__code__.co_filename:
                    events += 1
                    if events > 10000:
                        raise AssertionError('atomic import parser exceeded finite line-event budget')
                return bounded
            prior = sys.gettrace()
            with self.subTest(source=source):
                try:
                    sys.settrace(bounded)
                    errors = atomic_import_violations(source, pathlib.PurePath('core/src/memory.rs'))
                finally:
                    sys.settrace(prior)
                self.assertEqual(bool(errors), expected)


if __name__ == "__main__":
    unittest.main(verbosity=2)

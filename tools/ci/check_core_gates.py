#!/usr/bin/env python3
# Copyright (c) 2026 Contributors to the Eclipse Foundation
# See the NOTICE file(s) distributed with this work for copyright ownership.
# Licensed under the Apache License, Version 2.0.
# SPDX-License-Identifier: Apache-2.0
"""Mechanical gates for up-rust-core (R34, R48).

Each gate fails loudly. Negative controls prove a gate can fail.
"""
from __future__ import annotations

import json
import argparse
import os
import pathlib
import platform
import re
import shutil
import subprocess
import sys
import tomllib

COMMAND_LOG = None
COMMAND_INDEX = 0

ROOT = pathlib.Path(__file__).resolve().parents[2]
CORE = ROOT / "core"
FORBIDDEN_TOKENS = ("core::task", "core::future", "core::pin", "alloc::", "extern crate alloc", "extern crate std", "std::")

# Preparatory mapping: req~ssr-atomic-width~1 at neutral spec 733b006.
# Only facade routing is enforced here; the width/protocol port and G3 are pending.
ATOMIC_FACADE = pathlib.PurePosixPath("core/src/platform/atomic.rs")


class SourceScanError(ValueError):
    """Source applicability could not be established conservatively."""


def rust_tokens(source: str, *, literals: bool = False) -> list[tuple[str, int]]:
    """Tokens/offsets for the import gate, excluding comments and string literals.

    This is a source gate, not Rust name resolution or macro expansion. Ancestor
    imports/aliases are refused so spelling an atomic through an alias cannot
    bypass it. Nested comments, raw identifiers and multiline use trees count.
    """
    tokens = []
    pattern = re.compile(r'//[^\n]*|/\*|(?:br|cr|r)(\#*)"|[bc]?"(?:\\.|[^"\\])*"|'
                         r"b?'(?:\\(?:u\{[0-9a-fA-F_]+\}|x[0-9a-fA-F]{2}|.)|[^'\\])'|"
                         r'(?:r#)?[A-Za-z_][A-Za-z_0-9]*|::|[^\s]', re.DOTALL)
    pos = 0
    while match := pattern.search(source, pos):
        token = match[0]
        pos = match.end()
        if token == "/*":
            depth = 1
            while depth and pos < len(source):
                marker = re.search(r'/\*|\*/', source[pos:])
                if marker is None:
                    raise SourceScanError("unterminated block comment")
                depth += 1 if marker[0] == "/*" else -1
                pos += marker.end()
            if depth:
                raise SourceScanError("unterminated block comment")
        elif match[1] is not None:
            end = source.find('"' + match[1], pos)
            if end < 0:
                raise SourceScanError("unterminated raw string")
            pos = end + 1 + len(match[1])
            if literals:
                tokens.append((source[match.start():pos], match.start()))
        elif token.startswith(("//", '"', 'b"', 'c"', "'", "b'")):
            if token == '"':
                raise SourceScanError("unterminated string")
            if literals and not token.startswith("//"):
                tokens.append((token, match.start()))
            continue
        else:
            # The source gate deliberately supports ASCII code identifiers only.
            # Splitting a Unicode identifier could accept an allowed prefix
            # (e.g. __wake) while overlooking its distinct, forbidden full name.
            # Comments and all literal families were consumed above.
            if not token.isascii():
                line = source.count('\n', 0, match.start()) + 1
                raise SourceScanError(f'non-ASCII code token at line {line} is unsupported')
            tokens.append((token.removeprefix("r#"), match.start()))
    return tokens


def atomic_import_violations(source: str, path: pathlib.PurePath) -> list[str]:
    """Forbid atomic paths and alias/glob imports of their ancestors outside the facade."""
    if pathlib.PurePosixPath(path.as_posix()) == ATOMIC_FACADE:
        return []
    tokens = rust_tokens(source)
    words = [token for token, _ in tokens]
    hits = set()

    def forbidden(parts: list[str]) -> bool:
        return bool(parts and parts[0] in ("core", "std") and
                    (len(parts) == 1 or parts[1] in ("sync", "*")))

    def use_tree(i: int, prefix: list[str], origin: int) -> int:
        parts = list(prefix)
        while i < len(words):
            word = words[i]
            if word == "::":
                i += 1
            elif word == "{":
                i += 1
                while i < len(words) and words[i] != "}":
                    if words[i] == ",":
                        i += 1
                    else:
                        next_i = use_tree(i, parts, origin)
                        # Macro token trees can contain `use` without an import.
                        # Abandon this parse if a child cannot consume a token;
                        # the outer scan still checks every later `use`/path.
                        if next_i <= i:
                            return i
                        i = next_i
                return i + 1
            elif word in ("as", ",", ";", "}"):
                if forbidden(parts):
                    hits.add(origin)
                return i + 2 if word == "as" else i
            else:
                if not re.fullmatch(r"[A-Za-z_][A-Za-z_0-9]*|\*", word):
                    return i
                if word != "self":
                    parts.append(word)
                i += 1
        return i

    for i, word in enumerate(words):
        if word == "use":
            use_tree(i + 1, [], i)
        elif word in ("core", "std") and words[i + 1:i + 3] == ["::", "sync"]:
            hits.add(i)
        elif word == "extern" and words[i + 1:i + 2] == ["crate"] and words[i + 2:i + 3] in (["core"], ["std"]):
            hits.add(i)
    return [f"{source.count(chr(10), 0, tokens[i][1]) + 1}: atomic import bypasses platform::atomic"
            for i in sorted(hits)]


# R59's exact shipped seam. A new name needs an explicit design decision.
ASYNC_SEAM = {"__route_wake", "__wake", "__ttl", "__taken_value", "__taken_payload"}

def hook_unresolved(name: str, returncode: int, stderr: str) -> bool:
    """Accept only a primary missing-value/function diagnostic for this exact hook."""
    diagnostic = re.compile(
        r"^error\[(?:E0425|E0599)\]: "
        r"(?:cannot find (?:value|function) `(?P<free>[^`]+)`|"
        r"no (?:associated item|function or associated item|associated function or constant) "
        r"named `(?P<associated>[^`]+)` found)", re.MULTILINE)
    return returncode != 0 and any(
        (match.group("free") or match.group("associated")) == name
        for match in diagnostic.finditer(stderr))

# A deliberately conservative source-applicability parser, not rustc expansion.
# Only test and (for hook visibility) up_rust_test_hooks have known values.
# Unknown target/feature/custom atoms remain possible on every host running CI.
def cfg_value(words: list[str], disabled: frozenset[str]) -> bool | None:
    def expr(i: int) -> tuple[bool | None, int]:
        if i >= len(words) or not re.fullmatch(r'[A-Za-z_][A-Za-z_0-9]*', words[i]):
            raise SourceScanError('malformed cfg predicate')
        name = words[i]
        i += 1
        if i < len(words) and words[i] == '(':
            if name not in ('all', 'any', 'not'):
                raise SourceScanError(f'unsupported cfg operator {name}')
            values = []
            i += 1
            while i < len(words) and words[i] != ')':
                value, i = expr(i)
                values.append(value)
                if i < len(words) and words[i] == ',':
                    i += 1
                elif i >= len(words) or words[i] != ')':
                    raise SourceScanError('malformed cfg argument list')
            if i >= len(words):
                raise SourceScanError('unterminated cfg predicate')
            if name == 'not':
                if len(values) != 1:
                    raise SourceScanError('cfg not requires exactly one predicate')
                value = None if values[0] is None else not values[0]
            elif name == 'all':
                value = False if False in values else (None if None in values else True)
            else:
                value = True if True in values else (None if None in values else False)
            return value, i + 1
        if i < len(words) and words[i] == '=':
            if i + 1 >= len(words) or not string_literal(words[i + 1]):
                raise SourceScanError('cfg value must be a string literal')
            return None, i + 2
        return (False if name in disabled else None), i

    value, end = expr(0)
    # The cfg attribute, unlike nested predicates, permits a trailing comma.
    if words[end:] not in ([], [',']):
        raise SourceScanError('extra tokens in cfg predicate')
    return value


def string_literal(word: str) -> bool:
    return bool(re.fullmatch(r'"(?:\\.|[^"\\])*"|r(\#*)".*"\1', word, re.DOTALL))


def literal_path(word: str) -> str:
    # Rust escape syntax is not JSON's. Refuse it rather than misresolve a file.
    if not string_literal(word):
        raise SourceScanError('path/include requires one literal string')
    value = word[word.index('"') + 1:word.rindex('"')]
    if '\\' in value or '\n' in value or not value:
        raise SourceScanError('escaped, empty or multiline path/include is unsupported')
    return value


def cfg_and(left: bool | None, right: bool | None) -> bool | None:
    return False if False in (left, right) else (True if left is True and right is True else None)


class SourceView:
    """Mask only structurally bounded, definitely disabled items.

    Token-tree delimiters are matched before masking. Macro bodies/arguments
    stay opaque: cfg inside a macro is evaluated at expansion, not definition.
    Their literal tokens are still checked, and generated module/include edges
    are refused. This does not implement Rust name resolution or expansion.
    """
    def __init__(self, source: str, disabled: frozenset[str], *, modules: bool = False):
        self.source = source
        self.tokens = rust_tokens(source, literals=True)
        self.words = [word for word, _ in self.tokens]
        self.pairs: dict[int, int] = {}
        self.disabled = disabled
        self.modules = modules
        self.edges: list[tuple[str, str, tuple[str, ...], bool | None, int]] = []
        self.hooks: list[tuple[str, int]] = []
        self.errors: list[str] = []
        self.masked: list[tuple[int, int]] = []
        stack = []
        for i, word in enumerate(self.words):
            if word in ('(', '[', '{'):
                stack.append(i)
            elif word in (')', ']', '}'):
                if not stack or self.words[stack[-1]] != {')': '(', ']': '[', '}': '{'}[word]:
                    raise SourceScanError(f'unbalanced delimiter at line {self.line(i)}')
                opening = stack.pop()
                self.pairs[opening] = i
        if stack:
            raise SourceScanError(f'unclosed delimiter at line {self.line(stack[-1])}')
        self.closings = {closing: opening for opening, closing in self.pairs.items()}
        # Classify macro delimiters before walking any item/header. A brace in
        # a type- or expression-position macro is not an ordinary Rust scope;
        # an apparent inner cfg in that token tree cannot mask shipping tokens.
        self.macros: dict[int, tuple[int, str]] = {}
        for i, word in enumerate(self.words):
            if word == 'macro_rules' and self.words[i + 1:i + 2] == ['!'] and i + 3 in self.pairs:
                self.macros[i + 3] = (i, word)
            elif (re.fullmatch(r'[A-Za-z_][A-Za-z_0-9]*', word) and
                  self.words[i + 1:i + 2] == ['!'] and i + 2 in self.pairs):
                self.macros[i + 2] = (i, word)
        self.walk(0, len(self.words), True, ())

    def line(self, i: int) -> int:
        return self.source.count('\n', 0, self.tokens[i][1]) + 1

    def attrs(self, i: int, end: int) -> tuple[list[list[str]], int, bool]:
        attributes = []
        inner = False
        while i < end and self.words[i] == '#':
            j = i + 1
            this_inner = j < end and self.words[j] == '!'
            j += int(this_inner)
            if j >= end or self.words[j] != '[':
                raise SourceScanError(f'malformed attribute at line {self.line(i)}')
            if attributes and inner != this_inner:
                break
            inner = this_inner
            closing = self.pairs[j]
            attributes.append(self.words[j + 1:closing])
            i = closing + 1
        return attributes, i, inner

    def condition(self, attrs: list[list[str]]) -> bool | None:
        result: bool | None = True
        for attr in attrs:
            if not attr:
                raise SourceScanError('empty attribute')
            if attr[0] == 'cfg':
                if len(attr) < 3 or attr[1] != '(' or attr[-1] != ')':
                    raise SourceScanError('malformed cfg attribute')
                result = cfg_and(result, cfg_value(attr[2:-1], self.disabled))
            elif attr[0] == 'cfg_attr':
                # Even a conditional cfg/path could change file reachability.
                # Do not infer its absence. Nonstructural attributes are kept.
                if len(attr) < 5 or attr[1] != '(' or attr[-1] != ')':
                    raise SourceScanError('malformed cfg_attr attribute')
                depth = 0
                split = None
                for j in range(2, len(attr) - 1):
                    if attr[j] in ('(', '[', '{'):
                        depth += 1
                    elif attr[j] in (')', ']', '}'):
                        depth -= 1
                    elif attr[j] == ',' and depth == 0:
                        split = j
                        break
                if split is None:
                    raise SourceScanError('cfg_attr needs predicate and attributes')
                cfg_value(attr[2:split], self.disabled)
                if not attr[split + 1:-1] or attr[split + 1:-1] == [',']:
                    raise SourceScanError('cfg_attr needs at least one attribute')
                if any(word in ('cfg', 'cfg_attr', 'path') for word in attr[split + 1:]):
                    raise SourceScanError('cfg_attr changing cfg/path is unsupported; use explicit cfg items')
        return result

    def mask(self, start: int, end: int) -> None:
        if end > start:
            begin = self.tokens[start][1]
            finish = self.tokens[end - 1][1] + len(self.words[end - 1])
            self.masked.append((begin, finish))

    def opaque(self, start: int, end: int, condition: bool | None) -> None:
        if condition is False:
            return
        words = self.words
        for i in range(start, end):
            if (self.modules and words[i] == '$' and
                words[i + 2:i + 3] == ['!']):
                self.errors.append(f'metavariable macro invocation at line {self.line(i)} is unsupported')
            if self.modules and words[i] == '!' and i + 1 in self.pairs:
                # A repetition or metavariable expression can emit the macro
                # name too: `$($name)?!(...)` and `${...}!(...)` are not static
                # invocations. Do not rely on finding the literal `include`.
                prior = [i - 1]
                if words[i - 1:i] in (['*'], ['+'], ['?']):
                    prior = [i - 2, i - 3]  # with or without a separator
                if any(self.closings.get(j, 0) > 0 and
                       words[self.closings[j] - 1:self.closings[j]] == ['$'] for j in prior):
                    self.errors.append(f'metavariable macro invocation at line {self.line(i)} is unsupported')
            if self.modules and words[i] in ('mod', 'include'):
                # An identifier argument can become an include! name or alias
                # after substitution; requiring a literal trailing ! misses it.
                self.errors.append(f'macro-generated module/include at line {self.line(i)} is unsupported')
            if self.modules and (
                words[i] in ('concat_idents', 'paste') or
                (words[i] == '$' and words[i + 1:i + 2] != ['crate'] and words[i + 2:i + 3] == ['::']) or
                (words[i] == 'core' and words[i + 1:i + 2] not in (['::'], [':']) and
                 words[i - 1:i] not in (['.'], ['$'])) or
                (words[i] == 'core' and words[i + 1:i + 3] == ['::', '$']) or
                words[i] in ('std', 'alloc')
            ):
                self.errors.append(f'macro namespace construction at line {self.line(i)} is unsupported')
            # Literal identifiers supplied to a name-generating macro are still
            # visible in this source. Do not mistake an identifier argument for
            # proof that the generated function is safely gated. References,
            # macro names and metavariables do not declare functions here.
            if ((words[i].startswith('__') or words[i] == 'startup_values') and
                words[i] not in ASYNC_SEAM and words[i - 1:i] not in (['.'], ['::'], ['$']) and
                words[i + 1:i + 2] not in (['!'], ['::'])):
                self.errors.append(f'macro argument {words[i]} at line {self.line(i)} may name an unguarded test hook')
            # Do not treat cfg in an expansion as protection for a hook.
            if words[i] == 'pub':
                j = i + 1
                while j < end and words[j] in ('const', 'unsafe', 'async', 'extern'):
                    j += 1
                    if words[j:j + 1] and string_literal(words[j]):
                        j += 1
                if words[j:j + 1] == ['fn'] and j + 1 < end:
                    self.hook(words[j + 1], j + 1)

    def macro_group(self, opening: int, condition: bool | None,
                    inline: tuple[str, ...]) -> int:
        """Handle a known macro group in any position, without entering walk."""
        origin, kind = self.macros[opening]
        finish = self.pairs[opening] + 1
        if kind == 'include':
            # include! is expanded in place even in a type/initializer/header.
            # Never rely on the core/src orphan sweep to find its source.
            if self.modules:
                args = self.words[opening + 1:finish - 1]
                if args[-1:] == [',']:
                    args = args[:-1]
                if len(args) != 1:
                    raise SourceScanError('computed include path is unsupported')
                self.edges.append(('include', literal_path(args[0]), inline,
                                   condition, self.line(origin)))
        else:
            if self.modules and condition is not False:
                if kind in ('concat_idents', 'paste'):
                    self.errors.append(f'macro namespace construction at line {self.line(origin)} is unsupported')
                if self.words[origin - 1:origin] == ['$']:
                    self.errors.append(f'metavariable macro invocation at line {self.line(origin)} is unsupported')
            self.opaque(opening + 1, finish - 1, condition)
        return finish

    def header(self, start: int, end: int, condition: bool | None,
               inline: tuple[str, ...]) -> None:
        """Find macro/include groups in tokens consumed as an item header.

        A header is not parsed as an item scope: cfg-looking tokens there grant
        no exclusion. Ordinary delimiter groups are traversed, but macro groups
        always use macro_group, which retains their literal tokens or refuses.
        """
        i = start
        while i < end:
            if i in self.macros:
                i = self.macro_group(i, condition, inline)
            elif i in self.pairs:
                finish = self.pairs[i] + 1
                self.header(i + 1, finish - 1, condition, inline)
                i = finish
            else:
                i += 1

    def hook(self, name: str, i: int) -> None:
        if (name.startswith('__') or name == 'startup_values') and name not in ASYNC_SEAM:
            self.hooks.append((name, self.line(i)))

    def walk(self, start: int, end: int, ambient: bool | None, inline: tuple[str, ...]) -> None:
        i = start
        while i < end:
            begin = i
            attrs, i, inner = self.attrs(i, end)
            condition = cfg_and(ambient, self.condition(attrs))
            self.header(begin, i, condition, inline)
            if inner:
                # Inner attributes govern the entire containing scope and must
                # precede its contents. Later ones are not an omission licence.
                if begin != start:
                    raise SourceScanError(f'inner attribute after contents at line {self.line(begin)}')
                ambient = condition
                if ambient is False:
                    self.mask(start, end)
                start = i
                continue
            if i >= end:
                raise SourceScanError('attribute without an item')
            head = i
            public = self.words[i] == 'pub' and self.words[i + 1:i + 2] != ['(']
            if self.words[i] == 'pub':
                i += 1
                if self.words[i:i + 1] == ['(']:
                    i = self.pairs[i] + 1
            qualifier = i
            while i < end and self.words[i] in ('unsafe', 'async', 'default', 'const'):
                i += 1
            if self.words[qualifier:qualifier + 1] == ['const'] and self.words[i:i + 1] != ['fn']:
                i = qualifier  # const item/initializer, rather than const fn
            if self.words[i:i + 1] == ['extern'] and self.words[i + 1:i + 2] != ['crate']:
                i += 1
                if i < end and string_literal(self.words[i]):
                    i += 1
            kind = self.words[i] if i < end else ''
            if kind == 'mod':
                name = self.words[i + 1] if i + 1 < end else ''
                if not re.fullmatch(r'[A-Za-z_][A-Za-z_0-9]*', name):
                    raise SourceScanError('unsupported module name')
                j = i + 2
                paths = [attr for attr in attrs if attr[0] == 'path']
                path = None
                if paths:
                    if len(paths) != 1 or len(paths[0]) != 3 or paths[0][1] != '=':
                        raise SourceScanError('unsupported module path attribute')
                    path = literal_path(paths[0][2])
                if self.words[j:j + 1] == [';']:
                    self.edges.append(('path' if path else 'mod', path or name, inline, condition, self.line(i)))
                    finish = j + 1
                elif self.words[j:j + 1] == ['{']:
                    if path:
                        raise SourceScanError('path attribute on inline module is unsupported')
                    finish = self.pairs[j] + 1
                    self.walk(j + 1, finish - 1, condition, inline + (name,))
                else:
                    raise SourceScanError('unsupported module declaration')
                if condition is False:
                    self.mask(begin, finish)
                i = finish
                continue
            if self.modules and any(attr[0] == 'path' for attr in attrs):
                raise SourceScanError('path attribute outside a module')
            # Macro definitions and invocations are never parsed as Rust items.
            if kind == 'macro':
                raise SourceScanError('declarative macro 2.0 syntax is unsupported')
            if kind == 'include' and self.words[i + 1:i + 2] == ['!'] and i + 2 not in self.macros:
                raise SourceScanError('malformed include invocation')
            # Use the same identifier/definition classification as the global
            # macro index. Punctuation followed by unary !(...) is not a macro.
            macro = kind == 'macro_rules' or self.macros.get(i + 2) == (i, kind)
            if macro:
                j = i + (3 if kind == 'macro_rules' else 2)
                if j not in self.macros:
                    raise SourceScanError(f'unsupported macro syntax at line {self.line(i)}')
                finish = self.macro_group(j, condition, inline)
                if condition is False:
                    self.mask(begin, finish)
                i = finish
                continue
            item = kind in ('fn', 'impl', 'trait', 'struct', 'enum', 'union', 'use', 'static', 'const', 'type', 'extern')
            if kind == 'fn' and not re.fullmatch(r'[A-Za-z_][A-Za-z_0-9]*', self.words[i + 1] if i + 1 < end else ''):
                item = False  # a function pointer type, not a function item
            if item:
                if public and kind == 'fn' and condition is not False and i + 1 < end:
                    self.hook(self.words[i + 1], i + 1)
                j = i + 1
                while j < end and self.words[j] != ';':
                    if j in self.macros or self.words[j] in ('(', '['):
                        j = self.pairs[j] + 1
                    elif self.words[j] == '{':
                        break
                    else:
                        j += 1
                if j == end:
                    if attrs or public:
                        raise SourceScanError(f'unbounded item at line {self.line(i)}')
                    i += 1  # e.g. `impl Trait` in a parameter; omit nothing
                    continue
                self.header(head, j, condition, inline)
                finish = self.pairs[j] + 1 if self.words[j] == '{' else j + 1
                if (self.modules and kind == 'use' and condition is not False and
                    'include' in self.words[i:finish]):
                    self.errors.append(f'include macro import at line {self.line(i)} is unsupported')
                if self.words[j] == '{':
                    # A brace in a const-generic header can shorten this extent;
                    # it cannot cause later source to be masked accidentally.
                    self.walk(j + 1, finish - 1, condition, inline)
                if condition is False:
                    self.mask(begin, finish)
                i = finish
                continue
            # Unsupported attributed expressions are retained, never omitted.
            # Recurse into ordinary token groups so nested local modules count.
            i = head
            if i in self.macros:
                i = self.macro_group(i, ambient, inline)
            elif self.words[i] in ('(', '[', '{'):
                finish = self.pairs[i] + 1
                self.walk(i + 1, finish - 1, ambient, inline)
                i = finish
            else:
                i += 1

    def text(self) -> str:
        result = list(self.source)
        for start, end in self.masked:
            for i in range(start, end):
                if result[i] != '\n':
                    result[i] = ' '
        return ''.join(result)


def runtime_token_violations(source: str) -> list[str]:
    tokens = rust_tokens(source)
    words = [word for word, _ in tokens]
    hits = set()

    def import_tree(i: int, prefix: list[str], origin: int) -> int:
        parts = list(prefix)
        while i < len(words):
            word = words[i]
            if word == '::':
                i += 1
            elif word == '{':
                i += 1
                while i < len(words) and words[i] != '}':
                    if words[i] == ',':
                        i += 1
                    else:
                        following = import_tree(i, parts, origin)
                        if following <= i:
                            return i
                        i = following
                return i + 1
            elif word in ('as', ',', ';', '}'):
                if parts and (parts[0] in ('std', 'alloc') or
                              (parts[0] == 'core' and len(parts) > 1 and
                               parts[1] in ('future', 'task', 'pin'))):
                    hits.add(origin)
                return i + 2 if word == 'as' else i
            elif re.fullmatch(r'[A-Za-z_][A-Za-z_0-9]*|\*', word):
                if word != 'self':
                    parts.append(word)
                i += 1
            else:
                return i
        return i

    for i, word in enumerate(words):
        if word == 'use':
            import_tree(i + 1, [], i)
        forbidden = (word in ('std', 'alloc') and words[i + 1:i + 2] == ['::'])
        forbidden |= word == 'core' and words[i + 1:i + 2] == ['::'] and words[i + 2:i + 3] in (['future'], ['task'], ['pin'])
        forbidden |= word == 'extern' and words[i + 1:i + 2] == ['crate'] and words[i + 2:i + 3] in (['std'], ['alloc'])
        if forbidden:
            hits.add(i)
    return [f'{source.count(chr(10), 0, tokens[i][1]) + 1}: forbidden runtime {words[i]} path'
            for i in sorted(hits)]


def core_source_violations(root: pathlib.Path) -> list[str]:
    """Resolve source edges, including disabled ones, before classifying files.

    A file reached through both a test and shipping edge is shipping. Unattached
    .rs files are conservatively scanned as shipping too. Reads are cached only
    within this invocation; no inode/mtime or persistent content cache is used.
    """
    root = root.resolve()
    base = root / 'core' / 'src'
    cache: dict[pathlib.Path, tuple[SourceView, SourceView]] = {}
    reached: set[pathlib.Path] = set()
    shipping: set[pathlib.Path] = set()
    hook_shipping: set[pathlib.Path] = set()
    active: set[pathlib.Path] = set()
    done: set[tuple[pathlib.Path, pathlib.Path, bool, bool]] = set()
    hits = []

    def visit(path: pathlib.Path, module_dir: pathlib.Path, enabled: bool = True,
              hooks_enabled: bool = True) -> None:
        path = path.resolve()
        if not path.is_relative_to(root):
            raise SourceScanError(f'module/include outside repository: {path}')
        if not path.is_file():
            raise SourceScanError(f'missing module/include: {path.relative_to(root)}')
        reached.add(path)
        if enabled:
            shipping.add(path)
        if hooks_enabled:
            hook_shipping.add(path)
        key = (path, module_dir, enabled, hooks_enabled)
        if key in done:
            return
        if path in active:
            raise SourceScanError(f'cyclic module/include: {path.relative_to(root)}')
        active.add(path)
        if path not in cache:
            source = path.read_text(encoding='utf-8')
            try:
                cache[path] = (SourceView(source, frozenset({'test'}), modules=True),
                               SourceView(source, frozenset({'test', 'up_rust_test_hooks'}), modules=True))
            except SourceScanError as error:
                raise SourceScanError(f'{path.relative_to(root)}: {error}') from error
        runtime, hooks = cache[path]
        for edge, hook_edge in zip(runtime.edges, hooks.edges, strict=True):
            kind, name, inline, condition, line = edge
            directory = module_dir.joinpath(*inline)
            if kind == 'mod':
                candidates = [directory / (name + '.rs'), directory / name / 'mod.rs']
                existing = [candidate for candidate in candidates if candidate.is_file()]
                if len(existing) != 1:
                    raise SourceScanError(f'{path.relative_to(root)}:{line}: missing/ambiguous module {name}')
                target = existing[0]
                child_dir = directory / name
            elif kind == 'path':
                # Out-of-line path attributes are relative to the containing file;
                # within inline modules, Rust adds the inline module directory.
                target = (directory if inline else path.parent) / name
                child_dir = target.parent
            else:
                target = path.parent / name
                child_dir = directory
            visit(target, child_dir, enabled and condition is not False,
                  hooks_enabled and hook_edge[3] is not False)
        active.remove(path)
        done.add(key)

    try:
        visit(base / 'lib.rs', base)
        for path in sorted(base.rglob('*.rs')):
            if path.resolve() not in reached:
                visit(path, path.parent if path.name == 'mod.rs' else path.with_suffix(''))
        for path in sorted(shipping):
            runtime, hooks = cache[path]
            relative = path.relative_to(root)
            source = runtime.text()
            hits.extend(f'{relative}: source applicability refused: {error}' for error in runtime.errors)
            hits.extend(f'{relative}:{hit}' for hit in atomic_import_violations(source, relative))
            hits.extend(f'{relative}:{hit}' for hit in runtime_token_violations(source))
            if path in hook_shipping:
                hits.extend(f'{relative}: source applicability refused: {error}' for error in hooks.errors
                            if error not in runtime.errors)
                hits.extend(f'{relative}:{line}: {name}' for name, line in hooks.hooks)
    except (SourceScanError, OSError, UnicodeError) as error:
        hits.append(f'source applicability refused: {error}')
    return hits


def hook_violations(source: str) -> list[str]:
    try:
        view = SourceView(source, frozenset({'test', 'up_rust_test_hooks'}))
        return ([f'source applicability refused: {error}' for error in view.errors] +
                [f'{line}: {name}' for name, line in view.hooks])
    except SourceScanError as error:
        return [f'source applicability refused: {error}']


def run(cmd: list[str], **kwargs) -> subprocess.CompletedProcess[str]:
    print("+", " ".join(cmd), flush=True)
    global COMMAND_INDEX
    result = subprocess.run(cmd, text=True, capture_output=True, check=False, **kwargs)
    if COMMAND_LOG is not None:
        COMMAND_INDEX += 1
        prefix = COMMAND_LOG / str(COMMAND_INDEX)
        prefix.with_suffix(".stdout").write_text(result.stdout)
        prefix.with_suffix(".stderr").write_text(result.stderr)
        prefix.with_suffix(".json").write_text(json.dumps({"argv": cmd, "exit": result.returncode}))
    return result


class Gates:
    def __init__(self, toolchain: str, target_dir: pathlib.Path) -> None:
        global COMMAND_LOG
        COMMAND_LOG = target_dir / "observations" / str(os.getpid())
        COMMAND_LOG.mkdir(parents=True, exist_ok=False)
        self.toolchain = toolchain
        self.rustc = ["rustc", f"+{toolchain}"]
        self.target_dir = target_dir
        self.host = self._print("host")
        self.failures = 0

    def _print(self, what: str) -> str:
        output = run(self.rustc + ["-vV"]).stdout
        for line in output.splitlines():
            if line.startswith(f"{what}: "):
                return line.split(": ", 1)[1].strip()
        raise SystemExit(f"cannot determine {what}")

    def fail(self, name: str, detail: str = "") -> None:
        self.failures += 1
        print(f"FAIL {name}\n{detail}", flush=True)

    def ok(self, name: str, detail: str = "") -> None:
        print(f"PASS {name} {detail}".rstrip(), flush=True)

    # 1. The manifest stays dependency-free and feature-free.
    def manifest(self) -> None:
        data = tomllib.loads((CORE / "Cargo.toml").read_text(encoding="utf-8"))
        forbidden = [key for key in ("features", "dependencies", "build-dependencies", "dev-dependencies", "target") if key in data]
        if forbidden:
            self.fail("manifest", f"core/Cargo.toml has forbidden sections: {forbidden}")
        else:
            self.ok("manifest", "no [features], [dependencies], [build-dependencies], [dev-dependencies] or [target]")

    # 2. The certified surface excludes these modules; alloc and std cannot appear.
    def source_scan(self) -> None:
        hits = core_source_violations(ROOT)
        # Keep the legacy facade's existing textual source scope. Generated
        # OUT_DIR code and procedural macro expansion belong to compiler gates.
        for path in sorted((ROOT / 'src').rglob('*.rs')):
            hits.extend(f"{path.relative_to(ROOT)}:{hit}" for hit in
                        hook_violations(path.read_text(encoding='utf-8')))
        controls = (
            ("pub fn __unlisted() {}", True),
            ("pub const fn __unlisted() {}", True),
            ("#[doc(hidden)]\npub fn __unlisted() {}", True),
            ("#[cfg(up_rust_test_hooks)]\npub fn __unlisted() {}", False),
            ("#[cfg(any(test, up_rust_test_hooks))]\npub fn __unlisted() {}", False),
            ("#[cfg(any(test, feature = \"extra\"))]\npub fn __unlisted() {}", True),
            ("pub fn __wake() {}", False),
            ("pub fn startup_values() {}", True),
        )
        if any(bool(hook_violations(text)) != expected for text, expected in controls):
            hits.append("test-hook source scan negative/positive control failed")
        if hits:
            self.fail("source-scan", "\n".join(hits))
        else:
            self.ok("source-scan", f"no {', '.join(FORBIDDEN_TOKENS)}")

    def _restricted_sysroot(self, name: str, source: pathlib.Path, extra: list[str] | None = None) -> subprocess.CompletedProcess[str]:
        sysroot = run(self.rustc + ["--print", "sysroot"]).stdout.strip()
        libdir = pathlib.Path(sysroot) / "lib" / "rustlib" / self.host / "lib"
        restricted = self.target_dir / "restricted" / name / "sysroot" / "lib" / "rustlib" / self.host / "lib"
        restricted.mkdir(parents=True, exist_ok=True)
        wanted = re.compile(r"^(libcore-|libcompiler_builtins-|librustc_std_workspace_core-)")
        for entry in libdir.iterdir():
            if wanted.match(entry.name):
                shutil.copy2(entry, restricted / entry.name)
        output = restricted.parents[3] / "out"
        output.mkdir(parents=True, exist_ok=True)
        return run(
            self.rustc
            + [
                "--sysroot",
                str(restricted.parents[3]),
                "--crate-name",
                "up_rust_core",
                "--crate-type",
                "rlib",
                "--edition",
                "2021",
                "--out-dir",
                str(output),
            ]
            + (extra or [])
            + [str(source)]
        )

    # 3. It builds on a sysroot with no alloc or std, and the control proves that sysroot lacks alloc.
    def sysroot(self) -> None:
        result = self._restricted_sysroot("core", CORE / "src" / "lib.rs")
        if result.returncode != 0:
            self.fail("sysroot-only-core", result.stderr[-2000:])
        else:
            self.ok("sysroot-only-core", "builds with only core, compiler_builtins and rustc_std_workspace_core")
        control = self.target_dir / "control_alloc.rs"
        control.parent.mkdir(parents=True, exist_ok=True)
        control.write_text("#![no_std]\nextern crate alloc;\npub fn f() -> alloc::string::String { alloc::string::String::new() }\n", encoding="utf-8")
        result = self._restricted_sysroot("control", control)
        if result.returncode == 0 or "can't find crate for `alloc`" not in result.stderr:
            self.fail("sysroot-control-alloc", "the restricted sysroot unexpectedly provides alloc:\n" + result.stderr[-1000:])
        else:
            self.ok("sysroot-control-alloc", "extern crate alloc fails on the restricted sysroot")

    # The derive macro the two-mode fixture needs, built once for the host.
    def _macros(self) -> str | None:
        if getattr(self, "_macro_path", None) is not None:
            return self._macro_path
        env = dict(os.environ)
        env["CARGO_TARGET_DIR"] = str(self.target_dir / "macros")
        result = run(["cargo", f"+{self.toolchain}", "build", "--offline", "--locked", "-p", "up-rust-macros", "--message-format=json"], cwd=ROOT, env=env)
        if result.returncode != 0:
            self.fail("panic-link-macros", result.stderr[-3000:])
            return None
        for line in result.stdout.splitlines():
            if '"up_rust_macros"' in line and '"filenames"' in line:
                data = __import__("json").loads(line)
                for name in data.get("filenames", []):
                    if name.endswith((".so", ".dylib")):
                        self._macro_path = name
                        return name
        self.fail("panic-link-macros", "no proc-macro artifact found")
        return None

    # 4. Link-time no-panic check, plus a symbol check on the linked image.
    def _build_fixture(self, inject: bool, fixture: str = "panic_free") -> tuple[subprocess.CompletedProcess[str], pathlib.Path]:
        name = ("inject" if inject else "clean") + ("" if fixture == "panic_free" else "-two-mode")
        out = self.target_dir / "panic-link" / name
        out.mkdir(parents=True, exist_ok=True)
        flags = ["--edition", "2021", "-C", "opt-level=3", "-C", "panic=abort", "-C", "codegen-units=1", "-C", "overflow-checks=on", "-C", "debuginfo=0"]
        rlib = out / "libup_rust_core.rlib"
        built = run(self.rustc + ["--crate-name", "up_rust_core", "--crate-type", "rlib"] + flags + ["-o", str(rlib), str(CORE / "src" / "lib.rs")])
        if built.returncode != 0:
            return built, out
        # An undefined symbol must fail the link so a surviving panic path is caught:
        # the macOS linker refuses them by default; on Linux a shared object needs the -z defs linker flag.
        link_args = (
            ["-C", "link-arg=-lSystem"]
            if platform.system() == "Darwin"
            else ["-C", "link-arg=-Wl,-z,defs", "-C", "link-arg=-lc"]
        )
        cmd = self.rustc + ["--crate-name", "panic_free", "--crate-type", "cdylib"] + flags + ["-C", "lto=fat"] + link_args + ["--extern", f"up_rust_core={rlib}", "-o", str(out / "libpanic_free.so")]
        if fixture != "panic_free":
            macros = self._macros()
            if macros is None:
                return subprocess.CompletedProcess(cmd, 1, "", "the macro crate did not build"), out
            cmd += ["--extern", f"up_rust_macros={macros}"]
        if inject:
            cmd += ["--cfg", "inject_panic"]
        cmd += [str(CORE / "verify" / f"{fixture}.rs")]
        return run(cmd), out

    def _check_panic_fixture(self, fixture: str) -> None:
        result, out = self._build_fixture(inject=False, fixture=fixture)
        if result.returncode != 0:
            self.fail(f"panic-link[{fixture}]", result.stdout[-2000:] + result.stderr[-4000:])
        else:
            self.ok(f"panic-link[{fixture}]", "no panic path survived link-time optimization (overflow checks on)")
            nm = run(["nm", "-u", str(out / "libpanic_free.so")])
            print(nm.stdout, flush=True)
            bad = re.findall(r"\b(__rust_alloc\w*|__rust_dealloc\w*|__rust_realloc\w*|malloc|calloc|realloc|free|_Unwind_\w+|__rust_panic\w*)\b", nm.stdout)
            if nm.returncode != 0 or bad:
                self.fail(f"no-heap-symbols[{fixture}]", f"linked image references: {bad}\n{nm.stderr}")
            else:
                self.ok(f"no-heap-symbols[{fixture}]", "the linked image references no allocator or unwinder")
        result, _ = self._build_fixture(inject=True, fixture=fixture)
        combined = result.stdout + result.stderr
        if result.returncode == 0 or "__up_rust_core_panic_path_survived" not in combined:
            self.fail(f"panic-link-control[{fixture}]", "an injected panic path was not caught:\n" + combined[-2000:])
        else:
            self.ok(f"panic-link-control[{fixture}]", "an injected panic path fails the link, naming the trap symbol")

    def panic_link(self) -> None:
        self._check_panic_fixture("panic_free")
        self._check_panic_fixture("panic_free_two_mode")

    # 5. It builds for a bare-metal target (no Native backend there), and that
    # target must have 64-bit atomics: `FreeIndices` and `CallSlot` need them.
    def bare_metal(self) -> None:
        targets = run(["rustup", "target", "list", "--installed", "--toolchain", self.toolchain]).stdout.split()
        candidates = ["aarch64-unknown-none", "x86_64-unknown-none", "riscv64gc-unknown-none-elf"]
        chosen = next((t for t in candidates if t in targets), None)
        if chosen is None:
            self.fail("bare-metal", f"no bare-metal target with 64-bit atomics is installed for {self.toolchain}: tried {candidates}")
            return
        atomics = run(self.rustc + ["--print", "cfg", "--target", chosen]).stdout
        if 'target_has_atomic="64"' not in atomics:
            self.fail("bare-metal", f"{chosen} has no 64-bit atomics")
            return
        env = dict(os.environ)
        env["CARGO_TARGET_DIR"] = str(self.target_dir / "bare-metal")
        result = run(["cargo", f"+{self.toolchain}", "build", "--offline", "--locked", "-p", "up-rust-core", "--target", chosen], cwd=ROOT, env=env)
        if result.returncode != 0:
            self.fail("bare-metal", result.stderr[-3000:])
        else:
            self.ok("bare-metal", f"builds for {chosen}")


    # The QNX backend is source only, but it must at least type-check.
    def qnx_typecheck(self) -> None:
        env = dict(os.environ)
        env["CARGO_TARGET_DIR"] = str(self.target_dir / "qnx-typecheck")
        env["RUSTFLAGS"] = "--cfg check_qnx_backend"
        result = run(["cargo", f"+{self.toolchain}", "check", "--offline", "--locked", "-p", "up-rust-core"], cwd=ROOT, env=env)
        if result.returncode != 0:
            self.fail("qnx-typecheck", result.stderr[-3000:])
        else:
            self.ok("qnx-typecheck", "the source-only QNX backend type-checks on this host")

    # 6. A crate depending only on up-rust-core and up-rust-macros compiles, with the
    # derives on the core-only path.
    def consumer(self) -> None:
        env = dict(os.environ)
        env["CARGO_TARGET_DIR"] = str(self.target_dir / "consumer")
        env["RUSTFLAGS"] = ""
        env.pop("CARGO_ENCODED_RUSTFLAGS", None)
        base = ["cargo", f"+{self.toolchain}", "check", "--offline", "--locked", "-p", "up-rust-core-consumer"]
        result = run(base, cwd=ROOT, env=env)
        if result.returncode != 0:
            self.fail("consumer", result.stderr[-3000:])
        elif self.shipped_hooks(env):
            self.ok("consumer", "core consumer and shipped SDK/Tokio compile without hooks; all test hooks fail to resolve; URI warnings name replacements")


    def shipped_hooks(self, env: dict[str, str]) -> bool:
        command = ["cargo", f"+{self.toolchain}", "build", "--offline", "--locked",
                   "-p", "up-rust", "-p", "up-rust-tokio",
                   "--features", "up-rust/test-util,up-rust/zero-copy-transport",
                   "--message-format=json"]
        result = run(command, cwd=ROOT, env=env)
        if result.returncode:
            self.fail("consumer", result.stdout[-12000:] + result.stderr[-4000:])
            return False
        libraries = {}
        for line in result.stdout.splitlines():
            try:
                item = json.loads(line)
            except ValueError:
                continue
            if item.get("reason") == "compiler-artifact":
                for name in item.get("filenames", []):
                    if name.endswith(".rlib"):
                        libraries[item["target"]["name"]] = name
        out = self.target_dir / "hook-probes"
        out.mkdir(parents=True, exist_ok=True)
        probes = {
            "__free_value_slot": "up_rust_core::communication::two_mode::Publisher::<u32>::__free_value_slot::<()>",
            "__claim_first_entry": "up_rust_core::transport::inbound::PeerTable::__claim_first_entry",
            "__slice_for_tests": "up_rust_core::memory::StartupMemory::__slice_for_tests::<u32>",
            "__admit_for_tests": "up_rust_core::transport::envelope::__admit_for_tests",
            "__encode_head_for_tests": "up_rust_core::transport::envelope::__encode_head_for_tests",
            "__parse_for_tests": "up_rust_core::transport::envelope::__parse_for_tests",
            "__contract_slot": "up_rust_core::wire::__contract_slot::<u32>",
            "__contract_value": "up_rust_core::wire::__contract_value::<u32>",
            "__abi_offsets": "up_rust_core::uri::UUri::__abi_offsets",
            "__resolve_type_identity_for_test": "up_rust::NativeProfileAgreement::__resolve_type_identity_for_test::<u32, up_rust::NativeProfileError>",
            "startup_values": "up_rust::two_mode::startup_values",
        }
        args = self.rustc + ["--edition=2021", "--crate-type=lib", "--emit=metadata",
                            "-L", "dependency=" + str(pathlib.Path(env["CARGO_TARGET_DIR"]) / "debug" / "deps")]
        for name in ("up_rust", "up_rust_core"):
            args += ["--extern", name + "=" + libraries[name]]
        success = True
        for name, expression in probes.items():
            source = out / (name + ".rs")
            source.write_text(f"pub fn probe() {{ let _ = {expression}; }}\n")
            probe = run(args + ["--crate-name", "probe", "-o", str(out / (name + ".rmeta")), str(source)], env=env)
            (out / (name + ".stderr")).write_text(probe.stderr)
            if not hook_unresolved(name, probe.returncode, probe.stderr):
                self.fail("consumer", f"hook {name} did not fail specifically at resolution:\n" + probe.stderr)
                success = False
        for crate in ("up_rust_core::uri", "up_rust"):
            name = crate.replace("::", "_") + "_uri_aliases"
            source = out / (name + ".rs")
            source.write_text(f"pub fn probe(uri: &{crate}::UUri) {{ let _ = (uri.ue_type_id(), uri.ue_instance_id()); let _ = (uri.uentity_type_id(), uri.uentity_instance_id()); }}\n")
            probe = run(args + ["--crate-name", "probe", "-o", str(out / (name + ".rmeta")), str(source)], env=env)
            (out / (name + ".stderr")).write_text(probe.stderr)
            if probe.returncode or not all("use " + method + " instead" in probe.stderr for method in ("ue_type_id", "ue_instance_id")):
                self.fail("consumer", "URI deprecation compile proof failed:\n" + probe.stderr)
                success = False
        return success


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--toolchain", default="stable")
    parser.add_argument("--target-dir", default=os.environ.get("CARGO_TARGET_DIR", str(ROOT / "target")))
    parser.add_argument("--only", choices=["manifest", "source_scan", "sysroot", "panic_link", "bare_metal", "qnx_typecheck", "consumer"])
    args = parser.parse_args()
    gates = Gates(args.toolchain, pathlib.Path(args.target_dir) / "core-gates" / args.toolchain)
    for name in ("manifest", "source_scan", "sysroot", "panic_link", "bare_metal", "qnx_typecheck", "consumer"):
        if args.only is None or args.only == name:
            getattr(gates, name)()
    print(f"core gates on {args.toolchain}: {gates.failures} failure(s)", flush=True)
    return 1 if gates.failures else 0


if __name__ == "__main__":
    sys.exit(main())

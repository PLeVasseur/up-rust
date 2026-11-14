#!/usr/bin/env python3
# Copyright (c) 2026 Contributors to the Eclipse Foundation
# See the NOTICE file(s) distributed with this work for copyright ownership.
# Licensed under the Apache License, Version 2.0.
# SPDX-License-Identifier: Apache-2.0
"""Fail when `unsafe` code and its checked-in inventory disagree.

Every `unsafe` block, `unsafe fn`, `unsafe impl`, `unsafe trait`, `unsafe extern`
and `#[unsafe(...)]` in the scanned paths must be listed in the inventory, with a
reason: a row of the design's `unsafe` table, or a stated test-only reason. A site
is identified by its file, its enclosing item and its normalized text. Line numbers
are never part of a site's identity, so moving code does not disturb the list.

The scanner is a real lexer for the parts of Rust that can hide a word: comments,
doc comments, string and raw-string literals, char literals and lifetimes. It is
not a Rust parser. Item names are found by following braces, which is enough to
name where a site is, and nothing depends on them being a complete parse.

Exit status: 0 when the code and the list agree, 1 when they do not, 2 on bad input.

Reuse by another crate needs only path arguments:

    check_unsafe_inventory.py --root PATH --paths src tests \\
        --inventory PATH/unsafe_inventory.toml      # crate with `unsafe`
    check_unsafe_inventory.py --root PATH --paths src tests --require-zero

`--update` rewrites the inventory from the code, keeping the reasons of sites that
still exist. A new site gets an empty reason, which fails until someone fills it in.
"""
from __future__ import annotations

import argparse
import collections
import dataclasses
import fnmatch
import pathlib
import re
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[2]
DEFAULT_PATHS = (
    "core",
    "macros",
    "src",
    "up-rust-tokio",
    "test-fixtures",
    "tests",
    "benches",
    "examples",
    "tools",
    "abi",
    "build.rs",
)
DEFAULT_INVENTORY = "tools/ci/unsafe_inventory.toml"
SKIPPED_DIRS = {"target", ".git", "vendor", "node_modules"}
DEFAULT_TEST_PATHS = ("tests/*", "benches/*", "*/tests/*", "*/tests.rs")

ROW_PREFIX = "row:"
TEST_PREFIX = "test-only:"

PUNCTUATION = sorted(
    (
        "<<=", ">>=", "...", "..=", "::", "->", "=>", "==", "!=", "<=", ">=", "&&", "||",
        "+=", "-=", "*=", "/=", "%=", "^=", "&=", "|=", "<<", ">>", "..",
        "+", "-", "*", "/", "%", "^", "!", "&", "|", "=", "<", ">", "@", ".", ",", ";",
        ":", "#", "$", "?", "~", "\\", "(", ")", "{", "}", "[", "]",
    ),
    key=len,
    reverse=True,
)
WORD_KINDS = {"ident", "number", "string", "char", "lifetime"}
SPACED_KEYWORDS = {
    "as", "const", "dyn", "else", "for", "if", "impl", "in", "let", "match", "move", "mut",
    "return", "unsafe", "where", "while",
}
BINARY_SPACED = {"=", "==", "!=", "=>", "->", "+=", "-=", "*=", "/=", "%=", "<=", ">=", "&&", "||"}
OPENERS = {"(", "[", "{"}
CLOSERS = {")", "]", "}"}


@dataclasses.dataclass(frozen=True)
class Token:
    kind: str  # ident, number, string, char, lifetime or punct
    text: str
    line: int


class ScanError(Exception):
    """The source could not be read as Rust tokens."""


def lex(source: str, name: str) -> list[Token]:
    """Tokens of `source`, with comments and doc comments gone.

    Strings, raw strings, char literals and lifetimes each become one token, so
    nothing inside them can look like code, and they cannot hide code either.
    """
    tokens: list[Token] = []
    length = len(source)
    index = 0
    line = 1
    if source.startswith("#!") and not source[2:].lstrip().startswith("["):
        newline = source.find("\n")
        index = length if newline < 0 else newline

    def fail(message: str) -> ScanError:
        return ScanError(f"{name}:{line}: {message}")

    while index < length:
        char = source[index]
        if char == "\n":
            line += 1
            index += 1
            continue
        if char.isspace():
            index += 1
            continue
        if source.startswith("//", index):
            newline = source.find("\n", index)
            index = length if newline < 0 else newline
            continue
        if source.startswith("/*", index):
            depth = 1
            cursor = index + 2
            while depth:
                if cursor >= length:
                    raise fail("unterminated block comment")
                if source.startswith("/*", cursor):
                    depth += 1
                    cursor += 2
                elif source.startswith("*/", cursor):
                    depth -= 1
                    cursor += 2
                else:
                    if source[cursor] == "\n":
                        line += 1
                    cursor += 1
            index = cursor
            continue
        if char == "_" or char.isalpha():
            start = index
            while index < length and (source[index] == "_" or source[index].isalnum()):
                index += 1
            word = source[start:index]
            if word in {"r", "br", "cr"} and index < length and source[index] in {'"', "#"}:
                hashes = 0
                cursor = index
                while cursor < length and source[cursor] == "#":
                    hashes += 1
                    cursor += 1
                if cursor < length and source[cursor] == '"':
                    terminator = '"' + "#" * hashes
                    end = source.find(terminator, cursor + 1)
                    if end < 0:
                        raise fail("unterminated raw string")
                    index = end + len(terminator)
                    text = source[start:index]
                    tokens.append(Token("string", text, line))
                    line += text.count("\n")
                    continue
                if word == "r" and hashes == 1 and cursor < length and (source[cursor] == "_" or source[cursor].isalpha()):
                    # A raw identifier: `r#unsafe` names something, it is not the keyword.
                    cursor += 1
                    while cursor < length and (source[cursor] == "_" or source[cursor].isalnum()):
                        cursor += 1
                    tokens.append(Token("ident", source[start:cursor], line))
                    index = cursor
                    continue
                # Not a literal: fall through and treat the word as an identifier.
            if word in {"b", "c"} and index < length and source[index] == '"':
                end = _end_of_quoted(source, index, '"', line, name)
                text = source[start:end]
                tokens.append(Token("string", text, line))
                line += text.count("\n")
                index = end
                continue
            if word == "b" and index < length and source[index] == "'":
                end = _end_of_quoted(source, index, "'", line, name)
                tokens.append(Token("char", source[start:end], line))
                index = end
                continue
            tokens.append(Token("ident", word, line))
            continue
        if char.isdigit():
            start = index
            while index < length and (source[index].isalnum() or source[index] == "_"):
                index += 1
            tokens.append(Token("number", source[start:index], line))
            continue
        if char == '"':
            end = _end_of_quoted(source, index, '"', line, name)
            text = source[index:end]
            tokens.append(Token("string", text, line))
            line += text.count("\n")
            index = end
            continue
        if char == "'":
            if index + 1 < length and source[index + 1] == "\\":
                end = _end_of_quoted(source, index, "'", line, name)
                tokens.append(Token("char", source[index:end], line))
                index = end
                continue
            if index + 2 < length and source[index + 2] == "'" and source[index + 1] != "'":
                tokens.append(Token("char", source[index : index + 3], line))
                index += 3
                continue
            cursor = index + 1
            while cursor < length and (source[cursor] == "_" or source[cursor].isalnum()):
                cursor += 1
            if cursor > index + 1:
                tokens.append(Token("lifetime", source[index:cursor], line))
                index = cursor
                continue
            tokens.append(Token("punct", "'", line))
            index += 1
            continue
        for candidate in PUNCTUATION:
            if source.startswith(candidate, index):
                tokens.append(Token("punct", candidate, line))
                index += len(candidate)
                break
        else:
            raise fail(f"unexpected character {char!r}")
    return tokens


def _end_of_quoted(source: str, start: int, quote: str, line: int, name: str) -> int:
    """The index just past a quoted literal starting at `start`, honoring escapes."""
    cursor = start + 1
    while cursor < len(source):
        char = source[cursor]
        if char == "\\":
            cursor += 2
            continue
        if char == quote:
            return cursor + 1
        cursor += 1
    raise ScanError(f"{name}:{line}: unterminated literal")


def render(tokens: list[Token]) -> str:
    """One canonical text for a token run, whatever its original layout."""
    kept = [
        token
        for position, token in enumerate(tokens)
        if not (token.text == "," and position + 1 < len(tokens) and tokens[position + 1].text in CLOSERS)
    ]
    pieces: list[str] = []
    for position, token in enumerate(kept):
        if position and _spaced(kept[position - 1], token):
            pieces.append(" ")
        pieces.append(token.text)
    return "".join(pieces)


def _spaced(left: Token, right: Token) -> bool:
    if left.text in {",", ";"} and right.text not in CLOSERS:
        return True
    if right.text in {",", ";", ")", "]", ".", "?", ":", "::"}:
        return False
    if left.text == "{" and right.text == "}":
        return False
    if left.text in {"{", "}"} or right.text in {"{", "}"}:
        return True
    if left.text == ":":
        return True
    if left.text in {"(", "[", ".", "::", "#", "$"}:
        return False
    if left.text in BINARY_SPACED or right.text in BINARY_SPACED:
        return True
    left_word = left.kind in WORD_KINDS
    right_word = right.kind in WORD_KINDS
    if left_word and right_word:
        return True
    if right.text in {"#", "$"} and left_word:
        return True
    if left.text in SPACED_KEYWORDS and right.text in {"*", "&", "(", "!", "-", "["}:
        return True
    if left.text in {")", "]", ">"} and right_word:
        return True
    return False


@dataclasses.dataclass(frozen=True)
class Scope:
    label: str | None
    test: bool


@dataclasses.dataclass(frozen=True)
class Site:
    file: str
    item: str
    kind: str
    text: str
    line: int
    test: bool


def _top_level(tokens: list[Token]) -> list[Token]:
    """The tokens outside any parentheses or brackets."""
    depth = 0
    result: list[Token] = []
    for token in tokens:
        if token.text in {"(", "["}:
            depth += 1
        elif token.text in {")", "]"}:
            depth = max(0, depth - 1)
        elif depth == 0:
            result.append(token)
    return result


def _strip_angles(tokens: list[Token]) -> list[Token]:
    """Removes `<...>` groups, so an item's name does not carry its bounds."""
    depth = 0
    result: list[Token] = []
    for token in tokens:
        if token.text == "<":
            depth += 1
        elif token.text == "<<":
            depth += 2
        elif token.text == ">":
            depth = max(0, depth - 1)
        elif token.text == ">>":
            depth = max(0, depth - 2)
        elif depth == 0:
            result.append(token)
    return result


def _is_test_attribute(attribute: list[Token]) -> bool:
    words = [token.text for token in attribute if token.kind == "ident"]
    if not words:
        return False
    # The attribute's own path comes before any arguments: test, tokio::test, ...
    path: list[str] = []
    for token in attribute:
        if token.text in {"(", "="}:
            break
        if token.kind == "ident":
            path.append(token.text)
    if path and path[-1] == "test":
        return True
    return words[0] == "cfg" and len(words) > 1 and "not" not in words and (words[1] == "test" or (words[1] == "all" and "test" in words[2:]))


def _attributes_mark_test(tokens: list[Token]) -> bool:
    """Whether the attributes before an item confine it to test builds."""
    position = 0
    while position < len(tokens) - 2:
        token = tokens[position]
        if token.text == "#":
            cursor = position + 1
            if tokens[cursor].text == "!":
                cursor += 1
            if tokens[cursor].text == "[":
                depth = 1
                group = cursor + 1
                while group < len(tokens) and depth:
                    if tokens[group].text == "[":
                        depth += 1
                    elif tokens[group].text == "]":
                        depth -= 1
                    group += 1
                if _is_test_attribute(tokens[cursor + 1 : group - 1]):
                    return True
                position = group
                continue
        position += 1
    return False


def _scope_for(pending: list[Token]) -> Scope:
    """What the `{` that follows `pending` opens: an item's body, or an anonymous block."""
    test = _attributes_mark_test(pending)
    tokens = _top_level(pending)
    words = [token.text for token in tokens]
    for position, word in enumerate(words):
        following = words[position + 1] if position + 1 < len(words) else None
        if following in {"#", "$"} and position + 2 < len(words):
            # A name that is itself a macro variable, as in quote! bodies.
            following = following + words[position + 2]
        if word == "fn" and following not in {"(", None}:
            return Scope(following, test)
        if word in {"struct", "enum", "union", "trait", "mod"} and following is not None:
            prefix = "" if word == "mod" else word + " "
            return Scope(prefix + following, test)
        if word == "impl":
            header: list[Token] = []
            for token in _strip_angles(tokens[position + 1:]):
                if token.text == "where":
                    break
                header.append(token)
            return Scope("impl " + render(header), test)
        if word == "extern" and (following is None or (position + 2 >= len(words) and tokens[position + 1].kind == "string")):
            return Scope("extern " + (following or ""), test)
        if word == "macro_rules" and following == "!" and position + 2 < len(words):
            return Scope("macro_rules! " + words[position + 2], test)
        if word in {"const", "static"} and position + 1 < len(tokens):
            name = position + 1
            if words[name] == "mut":
                name += 1
            if name + 1 < len(words) and words[name + 1] == ":" and tokens[name].kind == "ident" and words[name] != "fn":
                return Scope(word + " " + words[name], test)
    return Scope(None, test)


def _matching(tokens: list[Token], start: int) -> int:
    """The index of the bracket closing the one at `start`."""
    pairs = {"(": ")", "[": "]", "{": "}"}
    opener = tokens[start].text
    closer = pairs[opener]
    depth = 0
    for position in range(start, len(tokens)):
        if tokens[position].text == opener:
            depth += 1
        elif tokens[position].text == closer:
            depth -= 1
            if depth == 0:
                return position
    raise ScanError(f"line {tokens[start].line}: unbalanced {opener}")


def _extent(tokens: list[Token], start: int) -> tuple[str, int]:
    """A site's kind and the index of its last token. `start` is the `unsafe` token."""
    count = len(tokens)
    next_index = start + 1
    next_text = tokens[next_index].text if next_index < count else ""
    if next_text == "{":
        return "block", _matching(tokens, next_index)
    if next_text == "(":
        return "attribute", _matching(tokens, next_index)
    if next_text in {"impl", "trait", "auto"}:
        kind = "impl" if next_text == "impl" else "trait"
        depth = 0
        for position in range(next_index, count):
            text = tokens[position].text
            if text in {"(", "["}:
                depth += 1
            elif text in {")", "]"}:
                depth -= 1
            elif depth == 0 and text in {"{", ";"}:
                return kind, position - 1
        raise ScanError(f"line {tokens[start].line}: {kind} without a body")
    cursor = next_index
    extern = False
    if next_text == "extern":
        extern = True
        cursor += 1
        if cursor < count and tokens[cursor].kind == "string":
            cursor += 1
        if cursor < count and tokens[cursor].text == "{":
            return "extern", _matching(tokens, cursor)
    if cursor < count and tokens[cursor].text == "fn":
        return ("extern fn" if extern else "fn"), _end_of_fn(tokens, cursor)
    return "other", _end_of_other(tokens, start)


def _end_of_fn(tokens: list[Token], fn_index: int) -> int:
    """The last token of a function's signature, or of a function-pointer type."""
    count = len(tokens)
    following = tokens[fn_index + 1].text if fn_index + 1 < count else ""
    if following != "(":
        # A function item: the signature runs to its body or its semicolon.
        depth = 0
        for position in range(fn_index, count):
            text = tokens[position].text
            if text in {"(", "["}:
                depth += 1
            elif text in {")", "]"}:
                depth -= 1
            elif depth == 0 and text in {"{", ";"}:
                return position - 1
        raise ScanError(f"line {tokens[fn_index].line}: function without a body")
    # A function-pointer type: its parameters, then an optional return type.
    close = _matching(tokens, fn_index + 1)
    if close + 1 >= count or tokens[close + 1].text != "->":
        return close
    depth = 0
    angle = 0
    position = close + 2
    while position < count:
        text = tokens[position].text
        if text in {"(", "["}:
            depth += 1
        elif text in {")", "]"}:
            if depth == 0:
                return position - 1
            depth -= 1
        elif text == "<":
            angle += 1
        elif text == ">" and angle:
            angle -= 1
        elif text == ">>" and angle:
            angle = max(0, angle - 2)
        elif depth == 0 and angle == 0 and text in {",", ";", "=", "{", "}"}:
            return position - 1
        position += 1
    return count - 1


def _end_of_other(tokens: list[Token], start: int) -> int:
    depth = 0
    for position in range(start, len(tokens)):
        text = tokens[position].text
        if text in OPENERS:
            depth += 1
        elif text in CLOSERS:
            if depth == 0:
                return position - 1
            depth -= 1
        elif depth == 0 and text in {";", ","}:
            return position - 1
    return len(tokens) - 1


def scan_source(source: str, name: str) -> list[Site]:
    """Every `unsafe` site in one file."""
    tokens = lex(source, name)
    stack: list[Scope] = []
    pending_stack: list[list[Token]] = []
    pending: list[Token] = []
    bracket_depth = 0
    sites: list[Site] = []
    for position, token in enumerate(tokens):
        if token.kind == "ident" and token.text == "unsafe":
            try:
                kind, last = _extent(tokens, position)
            except ScanError as error:
                raise ScanError(f"{name}: {error}") from error
            item = "::".join(scope.label for scope in stack if scope.label) or "(file)"
            test = any(scope.test for scope in stack) or _attributes_mark_test(pending)
            sites.append(Site(name, item, kind, render(tokens[position : last + 1]), token.line, test))
        if token.text == "{":
            stack.append(_scope_for(pending))
            pending_stack.append(pending)
            pending = []
            bracket_depth = 0
            continue
        if token.text == "}":
            if not stack:
                raise ScanError(f"{name}:{token.line}: unbalanced }}")
            scope = stack.pop()
            restored = pending_stack.pop()
            pending = [] if scope.label or not _open_group(restored) else restored
            bracket_depth = _open_group(pending)
            continue
        if token.text in {"(", "["}:
            bracket_depth += 1
        elif token.text in {")", "]"}:
            bracket_depth = max(0, bracket_depth - 1)
        if token.text == ";" and bracket_depth == 0:
            pending = []
            continue
        pending.append(token)
    if stack:
        raise ScanError(f"{name}: unbalanced {{")
    return sites


def _open_group(tokens: list[Token]) -> int:
    """How many parentheses or brackets are still open in `tokens`."""
    depth = 0
    for token in tokens:
        if token.text in {"(", "["}:
            depth += 1
        elif token.text in {")", "]"}:
            depth = max(0, depth - 1)
    return depth


def scan_tree(root: pathlib.Path, paths: list[str]) -> list[Site]:
    """Every `unsafe` site under `paths`, each path relative to `root`."""
    sites: list[Site] = []
    for relative in paths:
        base = root / relative
        if not base.exists():
            raise ScanError(f"{relative}: no such path under {root}")
        if base.is_file():
            files = [base]
        else:
            files = sorted(
                path
                for path in base.rglob("*.rs")
                if not (set(path.relative_to(root).parts) & SKIPPED_DIRS)
            )
        for path in files:
            name = path.relative_to(root).as_posix()
            sites.extend(scan_source(path.read_text(encoding="utf-8"), name))
    return sites


Identity = tuple[str, str, str]


def site_counts(sites: list[Site]) -> collections.Counter[Identity]:
    return collections.Counter((site.file, site.item, site.text) for site in sites)


@dataclasses.dataclass
class Entry:
    file: str
    item: str
    text: str
    count: int
    reason: str

    @property
    def identity(self) -> Identity:
        return (self.file, self.item, self.text)


@dataclasses.dataclass
class Inventory:
    rows: dict[str, str]
    entries: list[Entry]
    test_paths: list[str] = dataclasses.field(default_factory=lambda: list(DEFAULT_TEST_PATHS))


class InventoryError(Exception):
    """The inventory file is unreadable or malformed."""


def load_inventory(path: pathlib.Path) -> Inventory:
    try:
        with path.open("rb") as handle:
            data = tomllib.load(handle)
    except FileNotFoundError as error:
        raise InventoryError(f"{path}: no inventory file") from error
    except tomllib.TOMLDecodeError as error:
        raise InventoryError(f"{path}: not valid TOML: {error}") from error
    if data.get("schema") != 1:
        raise InventoryError(f"{path}: schema must be 1")
    rows = data.get("rows", {})
    if not isinstance(rows, dict) or not all(isinstance(value, str) and value for value in rows.values()):
        raise InventoryError(f"{path}: [rows] must map row ids to non-empty descriptions")
    for key in rows:
        if not re.fullmatch(r"[a-z0-9][a-z0-9-]*", key):
            raise InventoryError(f"{path}: invalid row id {key!r}")
    test_paths = data.get("test_paths", list(DEFAULT_TEST_PATHS))
    if not isinstance(test_paths, list) or not all(isinstance(item, str) and item for item in test_paths):
        raise InventoryError(f"{path}: test_paths must be a list of glob patterns")
    entries: list[Entry] = []
    seen: set[Identity] = set()
    for raw in data.get("site", []):
        unknown = set(raw) - {"file", "item", "text", "count", "reason"}
        if unknown:
            raise InventoryError(f"{path}: unknown keys {sorted(unknown)} on a site")
        try:
            file_name, item, text = raw["file"], raw["item"], raw["text"]
        except KeyError as error:
            raise InventoryError(f"{path}: a site is missing {error}") from error
        count = raw.get("count", 1)
        if not isinstance(count, int) or isinstance(count, bool) or count < 1:
            raise InventoryError(f"{path}: {file_name}: {item}: count must be a positive integer")
        entry = Entry(file_name, item, text, count, raw.get("reason", ""))
        if not isinstance(entry.reason, str):
            raise InventoryError(f"{path}: {file_name}: {item}: reason must be a string")
        if entry.identity in seen:
            raise InventoryError(f"{path}: duplicate entry for {file_name}: {item}: {text}")
        seen.add(entry.identity)
        entries.append(entry)
    return Inventory(rows, entries, test_paths)


def _toml_string(value: str) -> str:
    escaped = value.replace("\\", "\\\\").replace('"', '\\"')
    out: list[str] = []
    for char in escaped:
        code = ord(char)
        if char == "\n":
            out.append("\\n")
        elif char == "\t":
            out.append("\\t")
        elif char == "\r":
            out.append("\\r")
        elif code < 0x20 or code == 0x7f:
            out.append(f"\\u{code:04x}")
        else:
            out.append(char)
    return '"' + "".join(out) + '"'


HEADER = """\
# Every `unsafe` site in this repository, checked by tools/ci/check_unsafe_inventory.py.
#
# A site is identified by its file, its enclosing item and its normalized text.
# Line numbers are never part of a site's identity.
#
# reason = "row:ID"            maps the site to a row of [rows] below, which is
#                              the design's table of `unsafe` obligations.
# reason = "test-only: WHY"    is a stated reason for test code. It is refused on
#                              code outside test_paths and #[cfg(test)] items.
#
# When the gate fails: run the script with --update, write a reason for each new
# entry, and review the change as you would the `unsafe` code itself.
schema = 1
"""


def format_inventory(rows: dict[str, str], entries: list[Entry], test_paths: list[str]) -> str:
    lines = [HEADER]
    lines.append("test_paths = [")
    for pattern in test_paths:
        lines.append(f"    {_toml_string(pattern)},")
    lines.append("]")
    lines.append("")
    lines.append("[rows]")
    for key in sorted(rows):
        lines.append(f"{key} = {_toml_string(rows[key])}")
    for entry in sorted(entries, key=lambda e: (e.file, e.item, e.text)):
        lines.append("")
        lines.append("[[site]]")
        lines.append(f"file = {_toml_string(entry.file)}")
        lines.append(f"item = {_toml_string(entry.item)}")
        lines.append(f"text = {_toml_string(entry.text)}")
        if entry.count != 1:
            lines.append(f"count = {entry.count}")
        lines.append(f"reason = {_toml_string(entry.reason)}")
    return "\n".join(lines) + "\n"


def _describe(identity: Identity) -> str:
    file_name, item, text = identity
    return f"{file_name}: {item}: {text}"


def check(sites: list[Site], inventory: Inventory) -> list[str]:
    """Every disagreement between the code and the inventory, one per line."""
    problems: list[str] = []
    actual = site_counts(sites)
    listed = {entry.identity: entry for entry in inventory.entries}
    test_sites = {(site.file, site.item, site.text): site.test for site in sites}
    def is_test_code(identity: Identity) -> bool:
        if test_sites.get(identity, False):
            return True
        return any(fnmatch.fnmatch(identity[0], pattern) for pattern in inventory.test_paths)
    for identity, count in sorted(actual.items()):
        entry = listed.get(identity)
        have = entry.count if entry else 0
        if count > have:
            lines = sorted(site.line for site in sites if (site.file, site.item, site.text) == identity)
            problems.append(f"new site, not in the list: {_describe(identity)} ({count - have} more than listed; see line {lines[0]})")
    for identity, entry in sorted(listed.items()):
        count = actual.get(identity, 0)
        if count < entry.count:
            problems.append(f"listed site is gone: {_describe(identity)} ({entry.count - count} fewer in the code than listed)")
    for identity, entry in sorted(listed.items()):
        if identity not in actual:
            continue
        reason = entry.reason.strip()
        if not reason:
            problems.append(f"site has no reason: {_describe(identity)}")
        elif reason.startswith(ROW_PREFIX):
            row = reason[len(ROW_PREFIX) :].strip()
            if not row or row not in inventory.rows:
                problems.append(f"reason names a design row that does not exist ({row or 'empty'!r}): {_describe(identity)}")
        elif reason.startswith(TEST_PREFIX):
            if not reason[len(TEST_PREFIX) :].strip():
                problems.append(f"test-only reason states nothing: {_describe(identity)}")
            elif not is_test_code(identity):
                problems.append(f"test-only reason on code outside test paths and #[cfg(test)]: {_describe(identity)}")
        else:
            problems.append(f"reason is neither row:ID nor test-only: WHY: {_describe(identity)}: {reason!r}")
    return problems


def update(path: pathlib.Path, sites: list[Site]) -> int:
    """Rewrites the inventory from the code, keeping reasons that still apply."""
    if path.exists():
        old = load_inventory(path)
    else:
        old = Inventory(rows={}, entries=[], test_paths=list(DEFAULT_TEST_PATHS))
    reasons = {entry.identity: entry.reason for entry in old.entries}
    entries = [
        Entry(identity[0], identity[1], identity[2], count, reasons.get(identity, ""))
        for identity, count in site_counts(sites).items()
    ]
    path.write_text(format_inventory(old.rows, entries, old.test_paths), encoding="utf-8")
    missing = sum(1 for entry in entries if not entry.reason.strip())
    print(f"unsafe-inventory: wrote {path}: {sum(site_counts(sites).values())} sites, {missing} still need a reason")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--root", type=pathlib.Path, default=ROOT, help="repository root; default: this repository")
    parser.add_argument("--paths", nargs="+", default=list(DEFAULT_PATHS), help="paths under the root to scan")
    parser.add_argument("--inventory", type=pathlib.Path, default=None, help="inventory file, relative to the root")
    parser.add_argument("--require-zero", action="store_true", help="the scanned paths must have no sites at all; no inventory is read")
    parser.add_argument("--update", action="store_true", help="rewrite the inventory from the code, keeping reasons")
    parser.add_argument("--list", action="store_true", help="print every site")
    args = parser.parse_args(argv)
    root = args.root.resolve()
    inventory_path = root / (args.inventory if args.inventory is not None else DEFAULT_INVENTORY)
    try:
        sites = scan_tree(root, args.paths)
    except ScanError as error:
        print(f"unsafe-inventory: {error}", file=sys.stderr)
        return 2
    if args.list:
        for site in sorted(sites, key=lambda s: (s.file, s.line)):
            print(f"{site.file}:{site.line}: [{site.kind}] {site.item}: {site.text}")
        print(f"{len(sites)} sites")
        return 0
    if args.require_zero:
        for site in sorted(sites, key=lambda s: (s.file, s.line)):
            print(f"FAIL new site, this crate must have none: {site.file}: {site.item}: {site.text} (line {site.line})")
        if sites:
            print(f"unsafe-inventory: {len(sites)} site(s) where none are allowed", file=sys.stderr)
            return 1
        print(f"unsafe-inventory: no unsafe sites under {', '.join(args.paths)}")
        return 0
    if args.update:
        return update(inventory_path, sites)
    try:
        inventory = load_inventory(inventory_path)
    except InventoryError as error:
        print(f"unsafe-inventory: {error}", file=sys.stderr)
        return 2
    problems = check(sites, inventory)
    for problem in problems:
        print(f"FAIL {problem}")
    if problems:
        print(f"unsafe-inventory: {len(problems)} disagreement(s) between the code and {inventory_path}", file=sys.stderr)
        return 1
    print(f"unsafe-inventory: {len(sites)} sites in {len({site.file for site in sites})} files, all listed with reasons")
    return 0


if __name__ == "__main__":
    sys.exit(main())

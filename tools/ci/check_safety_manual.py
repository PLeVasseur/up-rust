#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Check exact safety-manual quotations against the AoU ownership registry.

This is a deliberately narrow document checker, not an OFT implementation or a
safety assessment. No network, subprocesses, code execution or evidence-tree
walks. Parse each selected UTF-8 file once per invocation. REGISTRY below
transcribes ownership registry version 2: every AoU family under the
specification root, its exact revision and owner, and either the up-rust manual
entry that quotes it or the named exclusion that sends it to its owner's
manual. The registry is an author assertion: a pass shows that the documents
match it, not that the ownership is right or that integrator evidence is
adequate. New syntax/ownership requires a reviewed registry and checker change,
never silent omission. All failures exit 1; argparse usage errors exit 2. LF
and CRLF are equivalent line delimiters; no other whitespace, punctuation,
inline markup or Unicode normalization is done.

The specification grammar is a list of lines that fail, not a parser. Outside
an item and on every line of a statement, a line equal to ////, ----, .... or
++++, or starting with ```, ~~~, include::, ifdef::, ifndef::, ifeval:: or
endif::, fails before any quotation is compared; nothing is included, evaluated
or rendered. A -- line closes a statement and fails outside an item. A
registered anchor is declared by exactly one whole line [[<id>]] outside every
statement. A line starting with // is a comment and is not scanned; a spelling
directly after a backslash is escaped. Recognized unsupported spellings fail;
other AsciiDoc ID forms are outside this finite guard, which does not establish
general AsciiDoc ID uniqueness.
"""
from __future__ import annotations

import argparse
from collections.abc import Mapping
from dataclasses import dataclass
from pathlib import Path
import re
import sys
from types import MappingProxyType


@dataclass(frozen=True)
class Owner:
    repository: str
    prefix: str | None  # manual item ID prefix; None: no owner manual registered


@dataclass(frozen=True)
class Family:
    name: str  # AoU family name after "aou~seooc-"
    revision: int  # the only accepted AoU revision
    owner: str  # owning repository
    entry: int | None  # revision <n> of the owner's manual item ID, if registered
    reason: str | None = None  # named exclusions only
    destination: str | None = None  # named exclusions only: the owning manual


@dataclass(frozen=True)
class Registry:
    version: int
    owners: tuple[Owner, ...]
    families: tuple[Family, ...]
    counts: Mapping[str, int]  # declared: 'families' and one count per owner
    anchors: tuple[str, ...]  # shared normative definitions: declared exactly once


SDK = 'up-rust'
STREAMER = 'up-streamer-rust'
ICEORYX2 = 'up-transport-iceoryx2-rust'
MANUAL_TAG = 'SafetyManual'
GATE_TAG = 'SafetyGateUpRust'  # carried by every AoU that up-rust must cover
_STREAMER_REASON = ('The Streamer owns this assumption. Its safety-manual entry belongs in '
                    'the safety manual of up-streamer-rust, and the up-rust safety manual has '
                    'no entry for it.')
_STREAMER_MANUAL = 'up-streamer-rust, its safety manual'
_ICEORYX2_REASON = ('The iceoryx2 binding owns this assumption, which binds only a deployment '
                    'that uses that binding. Its safety-manual entry belongs in the safety '
                    'manual of up-transport-iceoryx2-rust, and the up-rust safety manual has no '
                    'entry for it.')
# Ownership registry version 2, its checker_policy block (SHA-256 eaf01381...90c4).
# Changing any fact here needs a new registry version and review, never a quiet edit.
REGISTRY = Registry(
    version=2,
    owners=(Owner(SDK, 'uman~sdk-aou-'), Owner(STREAMER, None),
            Owner(ICEORYX2, 'uman~iceoryx2-aou-')),
    families=(
        # name, AoU revision, owner, manual item revision[, exclusion reason, owning manual]
        Family('supervisor-restart', 4, SDK, 2),
        Family('process-isolation', 3, SDK, 2),
        Family('integrity-levels', 6, SDK, 3),
        Family('capability-ceiling', 1, SDK, 1),
        Family('peer-access', 5, SDK, 2),
        Family('timing-budgets', 7, SDK, 3),
        Family('clock', 3, SDK, 2),
        Family('waiver-approval', 3, SDK, 2),
        Family('e2e-protection', 7, SDK, 2),
        Family('qualified-bindings-and-wires', 7, SDK, 4),
        Family('resource-guarantees', 1, SDK, 1),
        Family('target-environment', 4, SDK, 1),
        Family('build-configuration', 5, SDK, 2),
        Family('deployment-data', 6, SDK, 3),
        Family('capacity-sizing', 3, SDK, 2),
        Family('application-reacts', 4, SDK, 2),
        Family('integrator-platform', 4, SDK, 2),
        Family('peer-rates', 2, SDK, 1),
        Family('build-order', 2, SDK, 1),
        Family('operator-access', 2, STREAMER, None, _STREAMER_REASON, _STREAMER_MANUAL),
        Family('monitor-thresholds', 2, STREAMER, None, _STREAMER_REASON, _STREAMER_MANUAL),
        Family('iceoryx2-level-separation', 1, ICEORYX2, 1, _ICEORYX2_REASON,
               'up-transport-iceoryx2-rust, its safety manual: '
               'safety-manual/iceoryx2-level-separation.md'),
        Family('iceoryx2-limits', 1, ICEORYX2, 1, _ICEORYX2_REASON,
               'up-transport-iceoryx2-rust, its safety manual: '
               'safety-manual/iceoryx2-limits.md'),
    ),
    counts=MappingProxyType({'families': 23, SDK: 19, STREAMER: 2, ICEORYX2: 2}),
    anchors=('faulty-peer',),
)
ID = re.compile(r'(aou|req)~([a-z][a-z0-9-]*)~([1-9][0-9]*)\Z')
ATTR = re.compile(r',([a-z][a-z-]*)="([^"\n]*)"')
NAME = re.compile(r'[a-z][a-z0-9-]*\Z')
PREFIX = re.compile(r'uman~[a-z][a-z0-9]*(?:-[a-z0-9]+)*-aou-\Z')
MANUAL_ID = re.compile(r'`(uman~[a-z][a-z0-9-]*~[1-9][0-9]*)`\Z')
START = '<!-- aou-statement:start -->'
END = '<!-- aou-statement:end -->'
HEADINGS = ('## Status and scope', '## Assumption (verbatim)',
            '## Integrator actions', '## Violation manifestations',
            '## Detection limits')
MAX_FILE_BYTES = 1_000_000
MAX_TOTAL_BYTES = 8_000_000
MAX_FILES = 256
# The negative statement grammar: a line that fails outside an item and inside
# every statement. It is a list of lines that fail, not a parser: a longer
# delimiter line, a table, an example, sidebar or quotation block, a line
# comment, an attribute entry or inline markup is not rejected by it.
UNSUPPORTED_LINES = frozenset({'////', '----', '....', '++++'})
UNSUPPORTED_PREFIXES = ('```', '~~~', 'include::', 'ifdef::', 'ifndef::', 'ifeval::', 'endif::')


class Invalid(ValueError):
    """An input cannot be accepted under the documented grammar/policy."""


def demand(condition: bool, code: str, where: str, detail: str) -> None:
    if not condition:
        raise Invalid(f'{code}: {where}: {detail}')


def validate_registry(registry: Registry) -> None:
    """Fail unless the registry is self-consistent; it has no catch-all owner."""
    where = 'registry'
    demand(type(registry.version) is int and registry.version >= 1, 'REGISTRY_FORMAT',
           where, 'version must be a positive integer')
    where = f'registry v{registry.version}'
    owners: dict[str, Owner] = {}
    prefixes: dict[str, str] = {}
    for owner in registry.owners:
        demand(isinstance(owner.repository, str) and NAME.fullmatch(owner.repository)
               is not None, 'REGISTRY_FORMAT', where, f'invalid owner {owner.repository!r}')
        demand(owner.repository not in owners, 'REGISTRY_OWNERSHIP', where,
               f'owner {owner.repository} is listed twice')
        if owner.prefix is not None:
            demand(isinstance(owner.prefix, str) and PREFIX.fullmatch(owner.prefix) is not None,
                   'REGISTRY_FORMAT', where, f'{owner.repository}: invalid manual prefix '
                   f'{owner.prefix!r}')
            demand(owner.prefix not in prefixes, 'REGISTRY_OWNERSHIP', where,
                   f'{owner.repository} and {prefixes.get(owner.prefix)} share {owner.prefix}')
            prefixes[owner.prefix] = owner.repository
        owners[owner.repository] = owner
    demand(SDK in owners and owners[SDK].prefix is not None, 'REGISTRY_OWNERSHIP', where,
           f'the {SDK} owner and its manual prefix are required')
    seen: dict[str, Family] = {}
    for family in registry.families:
        demand(isinstance(family.name, str) and NAME.fullmatch(family.name) is not None,
               'REGISTRY_FORMAT', where, f'invalid family name {family.name!r}')
        label = f'{where}: aou~seooc-{family.name}'
        if family.name in seen:
            demand(seen[family.name].owner == family.owner, 'REGISTRY_OWNERSHIP', label,
                   f'listed under two owners, {seen[family.name].owner} and {family.owner}')
            raise Invalid(f'REGISTRY_DUPLICATE: {label}: listed twice')
        seen[family.name] = family
        demand(type(family.revision) is int and family.revision >= 1, 'REGISTRY_FORMAT',
               label, 'revision must be a positive integer')
        demand(family.owner in owners, 'REGISTRY_OWNERSHIP', label,
               f'unknown owner {family.owner!r}; there is no catch-all owner')
        if family.owner == SDK:
            demand(family.reason is None and family.destination is None, 'REGISTRY_OWNERSHIP',
                   label, 'both a required up-rust entry and a named exclusion')
        else:
            demand(all(isinstance(text, str) and text.strip()
                       for text in (family.reason, family.destination)),
                   'REGISTRY_EXCLUSION', label,
                   'a named exclusion needs a reason and an owning manual destination')
        if owners[family.owner].prefix is None:
            demand(family.entry is None, 'REGISTRY_FORMAT', label,
                   f'{family.owner} registers no manual, so no manual item revision')
        else:
            demand(type(family.entry) is int and family.entry >= 1, 'REGISTRY_FORMAT',
                   label, 'manual item revision must be a positive integer')
    computed = {'families': len(registry.families)}
    computed.update({repository: sum(family.owner == repository
                                     for family in registry.families)
                     for repository in owners})
    demand(dict(registry.counts) == computed, 'REGISTRY_COUNTS', where,
           f'declared counts {dict(registry.counts)} disagree with the family list {computed}')
    demand(len(set(registry.anchors)) == len(registry.anchors)
           and all(isinstance(anchor, str) and NAME.fullmatch(anchor) is not None
                   for anchor in registry.anchors),
           'REGISTRY_FORMAT', where, 'anchors must be distinct lower-case IDs')


def owner_of(registry: Registry, repository: str) -> Owner:
    """The owner whose manual root may be supplied in addition to up-rust's."""
    owner = next((owner for owner in registry.owners if owner.repository == repository), None)
    accepted = sorted(owner.repository for owner in registry.owners
                      if owner.repository != SDK and owner.prefix is not None)
    demand(owner is not None and repository != SDK and owner.prefix is not None,
           'OWNER_ROOT', repository, f'registry v{registry.version} has no owner manual for '
           f'it; accepted: {accepted} (the up-rust root is --manual-root)')
    return owner


class Reader:
    def __init__(self) -> None:
        self.bytes = 0
        self.files = 0

    def read(self, path: Path) -> str:
        demand(not path.is_symlink(), 'INPUT_SYMLINK', str(path),
               'symlink inputs are not supported')
        with path.open('rb') as stream:
            raw = stream.read(MAX_FILE_BYTES + 1)
        self.bytes += len(raw)
        self.files += 1
        demand(len(raw) <= MAX_FILE_BYTES and self.bytes <= MAX_TOTAL_BYTES
               and self.files <= MAX_FILES, 'INPUT_BUDGET', str(path),
               'document size/count exceeds the bounded checker input budget')
        text = raw.decode('utf-8')
        text = text.replace('\r\n', '\n')
        demand('\r' not in text and '\x00' not in text and '﻿' not in text,
               'INPUT_ENCODING', str(path), 'bare CR, NUL and BOM are forbidden')
        return text


def files_under(root: Path, suffix: str) -> list[Path]:
    demand(root.is_dir() and not root.is_symlink(), 'INPUT_ROOT', str(root),
           'expected a real directory')
    result = []
    # Only these small documentation roots are traversed, never the SDK tree.
    def visit(directory: Path) -> None:
        for path in sorted(directory.iterdir()):
            demand(not path.is_symlink(), 'INPUT_SYMLINK', str(path),
                   'symlink inside documentation root is forbidden')
            if path.is_dir():
                visit(path)
            else:
                demand(path.is_file() and path.suffix == suffix, 'INPUT_FORMAT',
                       str(path), f'only {suffix} documents belong in this root')
                result.append(path)
                demand(len(result) <= MAX_FILES, 'INPUT_BUDGET', str(root),
                       'too many documentation files')
    visit(root)
    demand(bool(result), 'INPUT_EMPTY', str(root), 'no documentation files')
    return result


@dataclass(frozen=True)
class Item:
    sid: str
    statement: str
    needs: tuple[str, ...]
    tags: tuple[str, ...]
    location: str
    body: tuple[int, int]  # first and last line number of the statement body


def unsupported(line: str) -> bool:
    """A line of the negative grammar: a disallowed delimiter, fence, include or conditional."""
    return line in UNSUPPORTED_LINES or line.startswith(UNSUPPORTED_PREFIXES)


def parse_spec(text: str, location: str) -> list[Item]:
    lines = text.split('\n')
    result = []
    seen_declarations = 0
    index = 0
    while index < len(lines):
        line = lines[index]
        demand(line != '--' and not unsupported(line),
               'SPEC_MARKUP', f'{location}:{index + 1}',
               'unexpected delimiter or unsupported conditional/include/literal block')
        if 'oft-sid=' not in line and '.specitem' not in line:
            index += 1
            continue
        where = f'{location}:{index + 1}'
        demand(line.startswith('[.specitem,') and line.endswith(']'),
               'SPEC_MARKUP', where, 'expected a standalone specitem declaration')
        content = line[len('[.specitem'):-1]
        attrs = list(ATTR.finditer(content))
        demand(''.join(m.group(0) for m in attrs) == content,
               'SPEC_MARKUP', where, 'unrecognized/malformed specitem attributes')
        keys = [m.group(1) for m in attrs]
        demand(len(keys) == len(set(keys)), 'SPEC_MARKUP', where,
               'duplicate specitem attribute')
        fields = {m.group(1): m.group(2) for m in attrs}
        demand(set(fields) == {'oft-sid', 'oft-title', 'oft-needs', 'oft-tags'},
               'SPEC_MARKUP', where, 'specitem must have sid/title/needs/tags only')
        sid = fields['oft-sid']
        demand(ID.fullmatch(sid) is not None and bool(fields['oft-title']),
               'SPEC_ID', where, 'invalid ID/revision or empty title')
        demand(index + 1 < len(lines) and lines[index + 1] == '--',
               'SPEC_MARKUP', where, 'missing opening statement delimiter --')
        stop = index + 2
        while stop < len(lines) and lines[stop] != '--':
            demand('.specitem' not in lines[stop] and 'oft-sid=' not in lines[stop],
                   'SPEC_MARKUP', where, 'nested item or missing closing --')
            demand(not unsupported(lines[stop]), 'SPEC_MARKUP', f'{location}:{stop + 1}',
                   f'{sid}: unsupported delimiter, include or conditional line inside the '
                   'statement; it fails before any quotation is compared')
            stop += 1
        demand(stop < len(lines), 'SPEC_MARKUP', where,
               'missing closing statement delimiter --')
        statement = '\n'.join(lines[index + 2:stop])
        demand(bool(statement.strip()) and not statement.startswith('\n')
               and not statement.endswith('\n'), 'SPEC_STATEMENT', where,
               'empty statement or blank outer line')
        needs = tuple(fields['oft-needs'].split(','))
        tags = tuple(fields['oft-tags'].split(','))
        demand(all(needs) and all(tags) and len(needs) == len(set(needs))
               and len(tags) == len(set(tags)), 'SPEC_MARKUP', where,
               'empty/duplicate needs or tags')
        result.append(Item(sid, statement, needs, tags, where, (index + 3, stop)))
        seen_declarations += 1
        index = stop + 1
    demand(seen_declarations == text.count('oft-sid='), 'SPEC_MARKUP', location,
           'hidden, duplicated or unparsed item declaration')
    return result


def anchor_declarations(lines: list[str], statements: Mapping[int, str], anchor: str,
                        location: str) -> list[str]:
    """Places of the one supported declaration of a registered anchor in one file.

    A declaration is a whole line [[<anchor>]] outside every statement. A line
    starting with // is a comment and is not scanned. A spelling directly after
    a backslash is escaped: neither a declaration nor a failure. Recognized
    unsupported spellings fail, as does the supported declaration inside a
    statement. Other AsciiDoc ID forms are outside this finite guard.
    References such as <<anchor>> or xref:...#anchor[...] are neither. The guard
    is syntactic: it shows nothing about the definition or the items using it.
    """
    block = f'[[{anchor}]]'
    name = re.escape(anchor)
    # Every start position, so that one spelling cannot hide another.
    spelling = re.compile(rf'(?=(\[\[{name}(?:\]\]|,.*?\]\])|\[#{name}[\].%,]'
                          rf'|anchor:{name}\[|\[id=(?:{name}|"{name}")[\],]))')
    places = []
    for number, line in enumerate(lines, 1):
        if line.startswith('//'):
            continue
        where = f'{location}:{number}'
        if line == block:
            demand(number not in statements, 'SPEC_ANCHOR', where,
                   f'{block} is declared inside the statement of {statements.get(number)}; '
                   'the declaration belongs outside every statement')
            places.append(where)
            continue
        for match in spelling.finditer(line):
            demand(match.start() > 0 and line[match.start() - 1] == '\\', 'SPEC_ANCHOR', where,
                   f'unsupported declaration spelling {match.group(1)!r} of {anchor}: only a '
                   f'whole line {block} outside every statement declares it; a line starting '
                   'with // is a comment, and a spelling after a backslash is escaped')
    return places


def parse_manual(text: str, location: str, prefix: str) -> tuple[str, int, str, str]:
    lines = text.split('\n')
    demand(lines[0].startswith('# ') and not any(line.startswith('# ') for line in lines[1:]),
           'MANUAL_MARKUP', location, 'exactly one level-one heading is required')
    identifiers = [(i, MANUAL_ID.fullmatch(line)) for i, line in enumerate(lines)
                   if 'uman~' in line]
    demand(len(identifiers) == 1 and identifiers[0][1] is not None,
           'MANUAL_ID', location, 'exactly one standalone backticked uman ID is required')
    uman = identifiers[0][1].group(1)
    demand(uman.startswith(prefix), 'MANUAL_PREFIX', location,
           f'{uman}: entries in this manual root use the prefix {prefix}')
    name, revision = uman[len(prefix):].rsplit('~', 1)
    demand(NAME.fullmatch(name) is not None, 'MANUAL_ID', location,
           f'{uman}: malformed name after {prefix}')
    cover_lines = [(i, line) for i, line in enumerate(lines)
                   if re.match(r'^\s*(?:>\s*)?covers\s*:', line, re.IGNORECASE)]
    demand(len(cover_lines) == 1 and cover_lines[0][1] == 'Covers:',
           'MANUAL_COVERS', location, 'exactly one unindented Covers: list is required')
    cover_index, _ = cover_lines[0]
    demand(cover_index + 2 < len(lines)
           and lines[cover_index + 1].startswith('- ')
           and lines[cover_index + 2] == '',
           'MANUAL_COVERS', location, 'Covers requires exactly one immediate list entry and a blank line')
    cover = lines[cover_index + 1][2:]
    demand(ID.fullmatch(cover) is not None and cover.startswith('aou~'),
           'MANUAL_COVERS', location, 'Covers must contain exactly one full AoU ID')
    demand(cover.split('~')[1] == 'seooc-' + name, 'MANUAL_ID', location,
           'uman name must correspond to its covered AoU family')
    demand(text.count(START) == 1 and text.count(END) == 1
           and START in lines and END in lines, 'MANUAL_QUOTE', location,
           'exactly one standalone statement marker pair is required')
    start, end = lines.index(START), lines.index(END)
    demand(start < end, 'MANUAL_QUOTE', location, 'reversed quotation markers')
    quote = []
    for line in lines[start + 1:end]:
        demand(line == '>' or line.startswith('> '), 'MANUAL_QUOTE', location,
               'every quoted line must start with > and exactly one separator space')
        quote.append(line[2:] if line.startswith('> ') else '')
    demand(bool(quote) and bool('\n'.join(quote).strip()), 'MANUAL_QUOTE', location,
           'empty statement quote')
    demand(not any(re.search(r'\[(?:utest|impl)\s*->', line) for line in lines),
           'MANUAL_LINK_TYPE', location, 'AoUs have uman coverage only, never impl/utest')
    # Restrict prose grammar around the item so a code fence/comment cannot hide
    # valid-looking metadata from OFT. Quotes themselves retain AsciiDoc verbatim.
    outside = lines[:start] + lines[end + 1:]
    demand(not any('```' in line or '~~~' in line or '<!--' in line or '-->' in line
                   or line.startswith(('    ', '\t')) for line in outside),
           'MANUAL_MARKUP', location, 'fenced/indented/comment-hidden item markup is forbidden')
    positions = []
    for heading in HEADINGS:
        demand(lines.count(heading) == 1, 'MANUAL_SECTIONS', location,
               f'exactly one {heading!r} section is required')
        positions.append(lines.index(heading))
    demand(identifiers[0][0] < cover_index < positions[0] < positions[1] < start
           < end < positions[2] < positions[3] < positions[4],
           'MANUAL_SECTIONS', location, 'item metadata, quote and sections are out of order')
    for heading, begin, finish in zip(HEADINGS, positions, positions[1:] + [len(lines)]):
        demand(bool('\n'.join(lines[begin + 1:finish]).strip()), 'MANUAL_SECTIONS',
               location, f'empty {heading} section')
    demand(any(line.startswith('- [ ] ') and len(line) > 12
               for line in lines[positions[2] + 1:positions[3]]),
           'MANUAL_CHECKLIST', location, 'integrator actions need a concrete checklist')
    return name, int(revision), cover, '\n'.join(quote)


def check_manual_root(root: Path, owner: Owner, registry: Registry,
                      aous: dict[str, Item], reader: Reader) -> int:
    """Exactly one exact entry per family of this owner, and nothing else."""
    families = {family.name: family for family in registry.families}
    expected = {family.name for family in registry.families if family.owner == owner.repository}
    entries: dict[str, str] = {}
    for path in files_under(root, '.md'):
        text = reader.read(path)
        if path == root / 'README.md':
            demand(not re.search(r'(?:uman~|Covers:|\[(?:utest|impl)\s*->)', text),
                   'MANUAL_README', str(path), 'README is status prose, not a hidden coverage entry')
            continue
        name, revision, cover, quote = parse_manual(text, str(path), owner.prefix)
        family = families.get(name)
        demand(family is not None, 'OWNERSHIP_MANUAL', str(path),
               f'aou~seooc-{name} is not a family of registry v{registry.version}')
        if family.owner == SDK:
            detail = f'aou~seooc-{name} is owned by {SDK}; its entry belongs in the up-rust root'
        else:
            detail = (f'aou~seooc-{name} is owned by {family.owner}, a named exclusion from '
                      f'the up-rust root: {family.reason} Owning manual: {family.destination}')
        demand(family.owner == owner.repository, 'OWNERSHIP_MANUAL', str(path), detail)
        demand(name not in entries, 'MANUAL_DUPLICATE', str(path),
               f'duplicate manual coverage for {name}; first in {entries.get(name)}')
        entries[name] = str(path)
        demand(cover in aous, 'REVISION_MISMATCH', str(path),
               f'{cover} absent in specification; current candidates: '
               f'{[sid for sid in aous if sid.rsplit("~", 1)[0] == cover.rsplit("~", 1)[0]]}')
        pinned = f'{owner.prefix}{name}~{family.entry}'
        demand(revision == family.entry, 'MANUAL_ID_REVISION', str(path),
               f'{owner.prefix}{name}~{revision}: registry v{registry.version} pins {pinned}; '
               'a manual item revision change needs a reviewed registry change')
        source = aous[cover].statement
        if quote != source:
            left, right = source.split('\n'), quote.split('\n')
            mismatch = next((i for i, (a, b) in enumerate(zip(left, right), 1) if a != b),
                            min(len(left), len(right)) + 1)
            raise Invalid(f'STATEMENT_MISMATCH: {path}: {cover}: quoted statement differs '
                          f'at line {mismatch}; same-revision wording changes require manual review '
                          f'(spec {aous[cover].location})')
    missing = expected - entries.keys()
    demand(not missing and len(entries) == len(expected), 'MANUAL_MISSING', str(root),
           f'expected exactly {len(expected)} {owner.repository} entries; missing={sorted(missing)}')
    return len(entries)


def check(spec_root: Path, manual_root: Path, owner_roots: dict[str, Path] | None = None,
          registry: Registry = REGISTRY) -> dict:
    validate_registry(registry)
    roots = {repository: (owner_of(registry, repository), path)
             for repository, path in (owner_roots or {}).items()}
    version = registry.version
    reader = Reader()
    specs: dict[str, Item] = {}
    families: set[str] = set()
    anchors: dict[str, list[str]] = {anchor: [] for anchor in registry.anchors}
    for path in files_under(spec_root, '.adoc'):
        text = reader.read(path)
        statements: dict[int, str] = {}
        for item in parse_spec(text, str(path)):
            family = item.sid.rsplit('~', 1)[0]
            demand(family not in families, 'SPEC_DUPLICATE', item.location,
                   f'duplicate requirement family (including different revisions): {family}')
            families.add(family)
            specs[item.sid] = item
            statements.update(dict.fromkeys(range(item.body[0], item.body[1] + 1), item.sid))
        for anchor in registry.anchors:
            anchors[anchor] += anchor_declarations(text.split('\n'), statements, anchor,
                                                   str(path))
    aous = {sid: item for sid, item in specs.items() if sid.startswith('aou~')}
    found = {sid.rsplit('~', 1)[0]: item for sid, item in aous.items()}
    expected = {f'aou~seooc-{family.name}': family for family in registry.families}
    missing, unexpected = expected.keys() - found.keys(), found.keys() - expected.keys()
    demand(not missing and not unexpected, 'OWNERSHIP_SPEC', str(spec_root),
           f'expected exactly the {len(expected)} AoU families of registry v{version} '
           f'(no catch-all owner); missing={sorted(missing)}, unexpected={sorted(unexpected)}')
    for family_id, family in expected.items():
        item = found[family_id]
        pinned = f'{family_id}~{family.revision}'
        demand(item.sid == pinned, 'OWNERSHIP_REVISION', item.location,
               f'{item.sid}: registry v{version} pins {pinned} ({family.owner}); '
               'a revision change needs a reviewed registry change')
        demand(item.needs == ('uman',) and MANUAL_TAG in item.tags, 'OWNERSHIP_NEEDS',
               item.location, f'{item.sid} must need only uman and have {MANUAL_TAG}')
        required = family.owner == SDK
        demand((GATE_TAG in item.tags) == required, 'OWNERSHIP_GATE', item.location,
               f'{item.sid} is owned by {family.owner}, so it '
               f'{"must" if required else "must not"} carry {GATE_TAG}')
        tags = {MANUAL_TAG, GATE_TAG} if required else {MANUAL_TAG}
        demand(set(item.tags) == tags, 'OWNERSHIP_TAGS', item.location,
               f'{item.sid} has tags {sorted(item.tags)}; registry v{version} gives '
               f'{sorted(tags)}; a tag change needs a reviewed registry change')
    for anchor, places in anchors.items():
        demand(len(places) == 1, 'SPEC_ANCHOR', str(spec_root),
               f'shared normative definition [[{anchor}]] must be declared exactly once; '
               f'found {len(places)}: {places}')
    owners = []
    for owner in registry.owners:
        mine = [family for family in registry.families if family.owner == owner.repository]
        record = {'owner': owner.repository, 'families': len(mine)}
        if owner.repository == SDK:
            record.update(relationship='required_entry', root=str(manual_root),
                          entries=check_manual_root(manual_root, owner, registry, aous, reader))
        elif owner.repository in roots:
            record.update(relationship='external_owner_exclusion', owner_manual='checked',
                          root=str(roots[owner.repository][1]),
                          entries=check_manual_root(roots[owner.repository][1], owner,
                                                    registry, aous, reader))
        else:
            record.update(relationship='external_owner_exclusion', owner_manual='not_checked',
                          why=('no --owner-manual-root given' if owner.prefix is not None
                               else f'registry v{version} registers no owner manual'))
        owners.append(record)
    return {
        'registry_version': version,
        'assumption_families': len(expected),
        'spec_items_parsed': len(specs),
        'sdk_entries': next(record['entries'] for record in owners if record['owner'] == SDK),
        'excluded': sum(record['families'] for record in owners if record['owner'] != SDK),
        'owner_manuals_checked': (f'{len(roots)}/' + str(sum(
            owner.prefix is not None for owner in registry.owners if owner.repository != SDK))),
        'files_read': reader.files,
        'content_bytes_read': reader.bytes,
        'owners': owners,
        'anchors': {anchor: places[0] for anchor, places in anchors.items()},
    }


def report(result: dict) -> list[str]:
    keys = ('registry_version', 'assumption_families', 'spec_items_parsed', 'sdk_entries',
            'excluded', 'owner_manuals_checked', 'files_read', 'content_bytes_read')
    lines = ['safety-manual: PASS: ' + ', '.join(f'{key}={result[key]}' for key in keys)]
    for record in result['owners']:
        if record['relationship'] == 'required_entry':
            text = (f"required_entries={record['families']}, entries={record['entries']}, "
                    f'quotations=exact, gate_tag={GATE_TAG}')
        elif record['owner_manual'] == 'checked':
            text = (f"excluded={record['families']}, owner_manual=checked, "
                    f"entries={record['entries']}, quotations=exact")
        else:
            text = f"excluded={record['families']}, owner_manual=not_checked ({record['why']})"
        lines.append(f"safety-manual: owner {record['owner']}: {text}")
    for anchor, place in result['anchors'].items():
        lines.append(f'safety-manual: shared definition [[{anchor}]] declared once at {place}')
    lines.append(f"Document consistency against ownership registry v{result['registry_version']},"
                 ' an author assertion: not proof that the ownership is right, and no product,'
                 ' OFT, G1a, integrator-evidence or deployment admission.')
    return lines


def owner_root(value: str) -> tuple[str, Path]:
    repository, separator, path = value.partition('=')
    if not separator or not repository or not path:
        raise argparse.ArgumentTypeError(f'expected REPOSITORY=PATH, got {value!r}')
    return repository, Path(path)


def main(argv: list[str] | None = None) -> int:
    root = Path(__file__).resolve().parents[2]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--spec-root', type=Path, default=root / 'up-spec/safety',
                        help='specification safety tree; every file below it must be .adoc')
    parser.add_argument('--manual-root', type=Path, default=root / 'safety-manual',
                        help='the up-rust safety manual root')
    parser.add_argument('--owner-manual-root', type=owner_root, action='append',
                        metavar='REPOSITORY=PATH',
                        help='optional manual root of an owner of named exclusions; '
                        f'registry v{REGISTRY.version} accepts ' + ', '.join(
                            owner.repository for owner in REGISTRY.owners
                            if owner.repository != SDK and owner.prefix is not None))
    args = parser.parse_args(argv)
    roots: dict[str, Path] = {}
    for repository, path in args.owner_manual_root or []:
        try:
            owner_of(REGISTRY, repository)
        except Invalid as error:
            parser.error(f'--owner-manual-root: {error}')
        if repository in roots:
            parser.error(f'--owner-manual-root: {repository} is given twice')
        roots[repository] = path
    try:
        result = check(args.spec_root, args.manual_root, roots)
    except (Invalid, OSError, UnicodeError, RecursionError) as error:
        print(f'safety-manual: FAIL: {error}', file=sys.stderr)
        return 1
    print('\n'.join(report(result)))
    return 0


if __name__ == '__main__':
    sys.exit(main())

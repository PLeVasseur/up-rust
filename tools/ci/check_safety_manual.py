#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Check exact safety-manual quotations and the SDK/Streamer AoU boundary.

This is a deliberately narrow document checker, not an OFT implementation or a
safety assessment. No network, subprocesses, code execution or evidence-tree
walks. Parse each selected UTF-8 file once per invocation. New syntax/ownership
requires a reviewed checker change, never silent omission. All failures exit 1;
argparse usage errors exit 2. LF and CRLF are equivalent line delimiters; no
other whitespace, punctuation, inline markup or Unicode normalization is done.
"""
from __future__ import annotations

import argparse
from dataclasses import dataclass
from pathlib import Path
import re
import sys


SDK_NAMES = frozenset('''supervisor-restart process-isolation integrity-levels
peer-access timing-budgets clock waiver-approval e2e-protection
qualified-bindings-and-wires target-environment build-configuration
deployment-data capacity-sizing application-reacts integrator-platform peer-rates
build-order'''.split())
STREAMER = {'operator-access': 2, 'monitor-thresholds': 2}
ID = re.compile(r'(aou|req)~([a-z][a-z0-9-]*)~([1-9][0-9]*)\Z')
ATTR = re.compile(r',([a-z][a-z-]*)="([^"\n]*)"')
MANUAL_ID = re.compile(r'`uman~sdk-aou-([a-z][a-z0-9-]*)~([1-9][0-9]*)`\Z')
START = '<!-- aou-statement:start -->'
END = '<!-- aou-statement:end -->'
HEADINGS = ('## Status and scope', '## Assumption (verbatim)',
            '## Integrator actions', '## Violation manifestations',
            '## Detection limits')
MAX_FILE_BYTES = 1_000_000
MAX_TOTAL_BYTES = 8_000_000
MAX_FILES = 256


class Invalid(ValueError):
    """An input cannot be accepted under the documented grammar/policy."""


def demand(condition: bool, code: str, where: str, detail: str) -> None:
    if not condition:
        raise Invalid(f'{code}: {where}: {detail}')


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
        demand('\r' not in text and '\x00' not in text and '\ufeff' not in text,
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


def parse_spec(text: str, location: str) -> list[Item]:
    lines = text.split('\n')
    result = []
    seen_declarations = 0
    index = 0
    while index < len(lines):
        line = lines[index]
        demand(line not in {'--', '////', '----', '....', '++++'}
               and not line.startswith(('```', '~~~', 'include::', 'ifdef::',
                                        'ifndef::', 'ifeval::', 'endif::')),
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
        result.append(Item(sid, statement, needs, tags, where))
        seen_declarations += 1
        index = stop + 1
    demand(seen_declarations == text.count('oft-sid='), 'SPEC_MARKUP', location,
           'hidden, duplicated or unparsed item declaration')
    return result


def parse_manual(text: str, location: str) -> tuple[str, str, str]:
    lines = text.split('\n')
    demand(lines[0].startswith('# ') and not any(line.startswith('# ') for line in lines[1:]),
           'MANUAL_MARKUP', location, 'exactly one level-one heading is required')
    identifiers = [(i, MANUAL_ID.fullmatch(line)) for i, line in enumerate(lines)
                   if 'uman~' in line]
    demand(len(identifiers) == 1 and identifiers[0][1] is not None,
           'MANUAL_ID', location, 'exactly one standalone backticked uman ID is required')
    name = identifiers[0][1].group(1)
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
    return name, cover, '\n'.join(quote)


def check(spec_root: Path, manual_root: Path) -> dict[str, int]:
    reader = Reader()
    specs: dict[str, Item] = {}
    families: set[str] = set()
    for path in files_under(spec_root, '.adoc'):
        for item in parse_spec(reader.read(path), str(path)):
            family = item.sid.rsplit('~', 1)[0]
            demand(family not in families, 'SPEC_DUPLICATE', item.location,
                   f'duplicate requirement family (including different revisions): {family}')
            families.add(family)
            specs[item.sid] = item
    aous = {sid: item for sid, item in specs.items() if sid.startswith('aou~')}
    actual = {sid.split('~')[1].removeprefix('seooc-') for sid in aous}
    expected = SDK_NAMES | STREAMER.keys()
    demand(actual == expected and len(aous) == 19, 'OWNERSHIP_SPEC', str(spec_root),
           f'expected 17 SDK + 2 Streamer AoUs; missing={sorted(expected - actual)}, '
           f'unexpected={sorted(actual - expected)}')
    for item in aous.values():
        demand(item.needs == ('uman',) and 'SafetyManual' in item.tags,
               'OWNERSHIP_NEEDS', item.location, f'{item.sid} must need only uman and have SafetyManual')
    for name, revision in STREAMER.items():
        demand(f'aou~seooc-{name}~{revision}' in aous, 'OWNERSHIP_STREAMER_REVISION',
               str(spec_root), f'Streamer {name} revision changed; review the ownership registry')
    manuals: dict[str, str] = {}
    for path in files_under(manual_root, '.md'):
        text = reader.read(path)
        if path == manual_root / 'README.md':
            demand(not re.search(r'(?:uman~|Covers:|\[(?:utest|impl)\s*->)', text),
                   'MANUAL_README', str(path), 'README is status prose, not a hidden coverage entry')
            continue
        name, cover, quote = parse_manual(text, str(path))
        demand(name in SDK_NAMES, 'OWNERSHIP_MANUAL', str(path),
               f'{name} is not an SDK AoU; Streamer entries belong in Stage7')
        demand(name not in manuals, 'MANUAL_DUPLICATE', str(path),
               f'duplicate manual coverage for {name}; first in {manuals.get(name)}')
        manuals[name] = str(path)
        demand(cover in aous, 'REVISION_MISMATCH', str(path),
               f'{cover} absent in specification; current candidates: '
               f'{[sid for sid in aous if sid.rsplit("~", 1)[0] == cover.rsplit("~", 1)[0]]}')
        source = aous[cover].statement
        if quote != source:
            left, right = source.split('\n'), quote.split('\n')
            mismatch = next((i for i, (a, b) in enumerate(zip(left, right), 1) if a != b),
                            min(len(left), len(right)) + 1)
            raise Invalid(f'STATEMENT_MISMATCH: {path}: {cover}: quoted statement differs '
                          f'at line {mismatch}; same-revision wording changes require manual review '
                          f'(spec {aous[cover].location})')
    missing = SDK_NAMES - manuals.keys()
    demand(not missing and len(manuals) == 17, 'MANUAL_MISSING', str(manual_root),
           f'expected exactly 17 SDK entries; missing={sorted(missing)}')
    return {'sdk_entries': len(manuals), 'streamer_excluded': len(STREAMER),
            'spec_items_parsed': len(specs), 'files_read': reader.files,
            'content_bytes_read': reader.bytes}


def main() -> int:
    root = Path(__file__).resolve().parents[2]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--spec-root', type=Path,
                        default=root / 'up-spec/safety/seooc-up-rust')
    parser.add_argument('--manual-root', type=Path, default=root / 'safety-manual')
    args = parser.parse_args()
    try:
        result = check(args.spec_root, args.manual_root)
    except (Invalid, OSError, UnicodeError, RecursionError) as error:
        print(f'safety-manual: FAIL: {error}', file=sys.stderr)
        return 1
    print('safety-manual: PASS: ' + ', '.join(f'{key}={value}' for key, value in result.items()))
    print('Document consistency only; no product, OFT, G1a or deployment admission.')
    return 0


if __name__ == '__main__':
    sys.exit(main())

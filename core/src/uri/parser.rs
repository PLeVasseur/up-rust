/********************************************************************************
 * Copyright (c) 2026 Contributors to the Eclipse Foundation
 *
 * See the NOTICE file(s) distributed with this work for additional
 * information regarding copyright ownership.
 *
 * This program and the accompanying materials are made available under the
 * terms of the Apache License Version 2.0 which is available at
 * https://www.apache.org/licenses/LICENSE-2.0
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/

//! A hand-written, allocation-free URI-reference reader.
//!
//! The grammar and acceptance rules come from RFC 3986 and mirror the URI
//! reference reading that `up-rust` used through `fluent-uri` (ASCII only, scheme
//! optional). `up-rust`'s differential test checks this code against
//! `fluent-uri` on every input it generates, including mutated inputs.

/// The kind of a URI-reference syntax error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ParseErrorKind {
    /// A percent-encoded octet is incomplete or not hexadecimal.
    InvalidPctEncodedOctet,
    /// A character is not allowed at this position.
    UnexpectedChar,
    /// An IPv6 literal is invalid.
    InvalidIpv6Addr,
}

/// A URI-reference syntax error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ParseError {
    pub(crate) index: usize,
    pub(crate) kind: ParseErrorKind,
}

impl ParseError {
    const fn new(index: usize, kind: ParseErrorKind) -> Self {
        Self { index, kind }
    }
}

type Result<T> = core::result::Result<T, ParseError>;

/// The kind of a parsed host.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Host {
    Ipv4,
    Ipv6,
    IpvFuture,
    RegName,
}

/// An authority component; all bounds are relative to its own text.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Authority<'a> {
    pub(crate) text: &'a str,
    pub(crate) host_start: usize,
    pub(crate) host_end: usize,
    pub(crate) host: Host,
}

impl<'a> Authority<'a> {
    pub(crate) fn has_port(&self) -> bool {
        self.host_end != self.text.len()
    }

    pub(crate) fn has_userinfo(&self) -> bool {
        self.host_start != 0
    }

    pub(crate) fn host_text(&self) -> &'a str {
        self.text.get(self.host_start..self.host_end).unwrap_or("")
    }
}

/// A parsed URI reference.
#[derive(Clone, Copy, Debug)]
pub(crate) struct UriRef<'a> {
    pub(crate) scheme: Option<&'a str>,
    pub(crate) authority: Option<Authority<'a>>,
    pub(crate) path: &'a str,
    pub(crate) has_query: bool,
    pub(crate) has_fragment: bool,
}

struct Table {
    allow: [bool; 256],
    pct: bool,
}

impl Table {
    fn allows(&self, byte: u8) -> bool {
        self.allow.get(byte as usize).copied().unwrap_or(false)
    }
}

const ALPHA: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const DIGIT: &[u8] = b"0123456789";
const HEX_ALPHA: &[u8] = b"ABCDEFabcdef";
const UNRESERVED_EXTRA: &[u8] = b"-._~";
const SUB_DELIMS: &[u8] = b"!$&'()*+,;=";

// Const-evaluated at compile time, so the indexing can never panic at run time.
#[allow(clippy::indexing_slicing)]
const fn table(parts: &[&[u8]], pct: bool) -> Table {
    let mut allow = [false; 256];
    let mut part = 0;
    while part < parts.len() {
        let bytes = parts[part];
        let mut index = 0;
        while index < bytes.len() {
            allow[bytes[index] as usize] = true;
            index += 1;
        }
        part += 1;
    }
    Table { allow, pct }
}

static SCHEME: Table = table(&[ALPHA, DIGIT, b"+-."], false);
static USERINFO: Table = table(&[ALPHA, DIGIT, UNRESERVED_EXTRA, SUB_DELIMS, b":"], true);
static IPV_FUTURE: Table = table(&[ALPHA, DIGIT, UNRESERVED_EXTRA, SUB_DELIMS, b":"], false);
static REG_NAME: Table = table(&[ALPHA, DIGIT, UNRESERVED_EXTRA, SUB_DELIMS], true);
static SEGMENT_NZ_NC: Table = table(&[ALPHA, DIGIT, UNRESERVED_EXTRA, SUB_DELIMS, b"@"], true);
static PATH: Table = table(&[ALPHA, DIGIT, UNRESERVED_EXTRA, SUB_DELIMS, b":@/"], true);
static QUERY_OR_FRAGMENT: Table =
    table(&[ALPHA, DIGIT, UNRESERVED_EXTRA, SUB_DELIMS, b":@/?"], true);
static HEXDIG: Table = table(&[DIGIT, HEX_ALPHA], false);

fn hex_value(byte: u8) -> Option<u16> {
    match byte {
        b'0'..=b'9' => Some(u16::from(byte - b'0')),
        b'a'..=b'f' => Some(u16::from(byte - b'a') + 10),
        b'A'..=b'F' => Some(u16::from(byte - b'A') + 10),
        _ => None,
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

enum PathKind {
    General,
    AbEmpty,
    ContinuedNoScheme,
}

enum Seg {
    // *1":" 1*4HEXDIG
    Normal(bool),
    // "::"
    Ellipsis,
    // *1":" 1*4HEXDIG "."
    MaybeV4(bool),
    // ":"
    SingleColon,
}

impl<'a> Reader<'a> {
    fn peek(&self, index: usize) -> Option<u8> {
        self.bytes.get(self.pos.wrapping_add(index)).copied()
    }

    fn peek_digit(&self, index: usize) -> Option<u32> {
        self.peek(index)
            .and_then(|byte| (byte as char).to_digit(10))
    }

    fn skip(&mut self, count: usize) {
        self.pos = self.pos.wrapping_add(count);
    }

    fn has_remaining(&self) -> bool {
        self.pos < self.bytes.len()
    }

    /// Reads while `table` allows the bytes. Returns whether anything was read.
    fn read(&mut self, table: &Table) -> Result<bool> {
        let start = self.pos;
        self.read_with(table, |_, _| {})?;
        Ok(self.pos > start)
    }

    fn read_with(&mut self, table: &Table, mut visit: impl FnMut(usize, u8)) -> Result<()> {
        let mut index = self.pos;
        while let Some(&byte) = self.bytes.get(index) {
            if table.pct && byte == b'%' {
                let hi = self.bytes.get(index.wrapping_add(1)).copied();
                let lo = self.bytes.get(index.wrapping_add(2)).copied();
                match (hi, lo) {
                    (Some(hi), Some(lo)) if HEXDIG.allows(hi) && HEXDIG.allows(lo) => {
                        index = index.wrapping_add(3);
                    }
                    _ => {
                        return Err(ParseError::new(
                            index,
                            ParseErrorKind::InvalidPctEncodedOctet,
                        ))
                    }
                }
            } else if table.allows(byte) {
                visit(index, byte);
                index = index.wrapping_add(1);
            } else {
                break;
            }
        }
        self.pos = index;
        Ok(())
    }

    fn read_str(&mut self, text: &str) -> bool {
        if self
            .bytes
            .get(self.pos..)
            .is_some_and(|rest| rest.starts_with(text.as_bytes()))
        {
            self.skip(text.len());
            true
        } else {
            false
        }
    }

    /// `true` when a valid IPv6 address was read.
    fn read_v6(&mut self) -> bool {
        let mut ellipsis_at = 8_usize;
        let mut index = 0_usize;
        while index < 8 {
            match self.read_v6_segment() {
                Some(Seg::Normal(colon)) => {
                    if colon == (index == 0 || index == ellipsis_at) {
                        // Leading colon, triple colons, or no colon.
                        return false;
                    }
                    index = index.wrapping_add(1);
                }
                Some(Seg::Ellipsis) => {
                    if ellipsis_at != 8 {
                        // Multiple ellipses.
                        return false;
                    }
                    ellipsis_at = index;
                }
                Some(Seg::MaybeV4(colon)) => {
                    if index > 6 || colon == (index == ellipsis_at) {
                        // Not enough space, triple colons, or no colon.
                        return false;
                    }
                    if self.read_v4().is_none() {
                        return false;
                    }
                    index = index.wrapping_add(2);
                    break;
                }
                Some(Seg::SingleColon) => return false,
                None => break,
            }
        }

        if ellipsis_at == 8 {
            // No ellipsis: the address must be complete.
            index == 8
        } else {
            // An ellipsis must elide at least one group.
            index != 8
        }
    }

    fn read_v6_segment(&mut self) -> Option<Seg> {
        let colon = self.read_str(":");
        if !self.has_remaining() {
            return colon.then_some(Seg::SingleColon);
        }

        let first = self.peek(0)?;
        if hex_value(first).is_none() {
            return colon.then(|| {
                if first == b':' {
                    self.skip(1);
                    Seg::Ellipsis
                } else {
                    Seg::SingleColon
                }
            });
        }
        let mut count = 1_usize;
        while count < 4 {
            let Some(byte) = self.peek(count) else {
                self.skip(count);
                return None;
            };
            if hex_value(byte).is_some() {
                count = count.wrapping_add(1);
                continue;
            }
            if byte == b'.' {
                return Some(Seg::MaybeV4(colon));
            }
            break;
        }
        self.skip(count);
        Some(Seg::Normal(colon))
    }

    fn read_v4(&mut self) -> Option<u32> {
        let mut address = self.read_v4_octet()?.wrapping_shl(24);
        for index in (0..3_u32).rev() {
            if !self.read_str(".") {
                return None;
            }
            address |= self.read_v4_octet()?.wrapping_shl(index.wrapping_mul(8));
        }
        Some(address)
    }

    fn read_v4_octet(&mut self) -> Option<u32> {
        let mut value = self.peek_digit(0)?;
        if value == 0 {
            self.skip(1);
            return Some(0);
        }
        for index in 1..3_usize {
            let Some(digit) = self.peek_digit(index) else {
                self.skip(index);
                return Some(value);
            };
            value = value.wrapping_mul(10).wrapping_add(digit);
        }
        self.skip(3);
        u8::try_from(value).is_ok().then_some(value)
    }

    fn read_port(&mut self) {
        if self.read_str(":") {
            let mut count = 0_usize;
            while self.peek_digit(count).is_some() {
                count = count.wrapping_add(1);
            }
            self.skip(count);
        }
    }

    fn read_ip_literal(&mut self) -> Result<Option<Host>> {
        if !self.read_str("[") {
            return Ok(None);
        }
        let start = self.pos;
        let host = if self.read_v6() {
            Host::Ipv6
        } else if self.pos == start {
            self.read_ipv_future()?;
            Host::IpvFuture
        } else {
            return Err(ParseError::new(start, ParseErrorKind::InvalidIpv6Addr));
        };
        if !self.read_str("]") {
            return Err(ParseError::new(self.pos, ParseErrorKind::UnexpectedChar));
        }
        Ok(Some(host))
    }

    fn read_ipv_future(&mut self) -> Result<()> {
        if let Some(b'v' | b'V') = self.peek(0) {
            self.skip(1);
            if self.read(&HEXDIG)? && self.read_str(".") && self.read(&IPV_FUTURE)? {
                return Ok(());
            }
        }
        Err(ParseError::new(self.pos, ParseErrorKind::UnexpectedChar))
    }
}

fn parse_v4_or_reg_name(bytes: &[u8]) -> Host {
    let mut reader = Reader { bytes, pos: 0 };
    match reader.read_v4() {
        Some(_) if !reader.has_remaining() => Host::Ipv4,
        _ => Host::RegName,
    }
}

impl Reader<'_> {
    fn read_v4_or_reg_name(&mut self) -> Result<Host> {
        Ok(match (self.read_v4(), self.read(&REG_NAME)?) {
            (Some(_), false) => Host::Ipv4,
            _ => Host::RegName,
        })
    }

    fn read_host(&mut self) -> Result<Host> {
        match self.read_ip_literal()? {
            Some(host) => Ok(host),
            None => self.read_v4_or_reg_name(),
        }
    }
}

#[derive(Default)]
struct Meta {
    scheme_end: Option<usize>,
    auth: Option<(usize, usize, Host)>,
    path_bounds: (usize, usize),
    query_end: Option<usize>,
    has_fragment: bool,
}

struct Parser<'a> {
    reader: Reader<'a>,
    meta: Meta,
}

impl Parser<'_> {
    fn parse_from_scheme(&mut self) -> Result<()> {
        self.reader.read(&SCHEME)?;

        if self.reader.peek(0) == Some(b':') {
            // Scheme starts with a letter.
            if self.reader.pos > 0
                && self
                    .reader
                    .bytes
                    .first()
                    .is_some_and(u8::is_ascii_alphabetic)
            {
                self.meta.scheme_end = Some(self.reader.pos);
            } else {
                return Err(ParseError::new(0, ParseErrorKind::UnexpectedChar));
            }

            self.reader.skip(1);
            return if self.reader.read_str("//") {
                self.parse_from_authority()
            } else {
                self.parse_from_path(PathKind::General)
            };
        } else if self.reader.pos == 0 {
            // Nothing read.
            if self.reader.read_str("//") {
                return self.parse_from_authority();
            }
        }
        // Scheme chars are valid for path.
        self.parse_from_path(PathKind::ContinuedNoScheme)
    }

    fn parse_from_authority(&mut self) -> Result<()> {
        let host;

        let mut colon_count = 0_usize;
        let mut colon_at = 0_usize;
        let authority_start = self.reader.pos;

        // The userinfo table covers userinfo, registered name, ':' and port.
        self.reader.read_with(&USERINFO, |index, byte| {
            if byte == b':' {
                colon_count = colon_count.wrapping_add(1);
                colon_at = index;
            }
        })?;

        if self.reader.peek(0) == Some(b'@') {
            // Userinfo present.
            self.reader.skip(1);

            let host_start = self.reader.pos;
            let kind = self.reader.read_host()?;
            host = (host_start, self.reader.pos, kind);

            self.reader.read_port();
        } else if self.reader.pos == authority_start {
            // Nothing read. We are at the start of an IP literal or the path.
            if let Some(kind) = self.reader.read_ip_literal()? {
                host = (authority_start, self.reader.pos, kind);
                self.reader.read_port();
            } else {
                // Empty authority.
                host = (self.reader.pos, self.reader.pos, Host::RegName);
            }
        } else {
            // The whole authority was read. Parse the host and port.
            let host_end = match colon_count {
                // All host.
                0 => self.reader.pos,
                // Host and port.
                1 => {
                    let mut index = colon_at.wrapping_add(1);
                    while index < self.reader.pos {
                        if !self.reader.bytes.get(index).is_some_and(u8::is_ascii_digit) {
                            return Err(ParseError::new(index, ParseErrorKind::UnexpectedChar));
                        }
                        index = index.wrapping_add(1);
                    }
                    colon_at
                }
                // Multiple colons.
                _ => return Err(ParseError::new(colon_at, ParseErrorKind::UnexpectedChar)),
            };
            let bytes = self
                .reader
                .bytes
                .get(authority_start..host_end)
                .unwrap_or(&[]);
            host = (authority_start, host_end, parse_v4_or_reg_name(bytes));
        }

        self.meta.auth = Some(host);
        self.parse_from_path(PathKind::AbEmpty)
    }

    fn parse_from_path(&mut self, kind: PathKind) -> Result<()> {
        self.meta.path_bounds = match kind {
            PathKind::General => {
                let start = self.reader.pos;
                self.reader.read(&PATH)?;
                (start, self.reader.pos)
            }
            PathKind::AbEmpty => {
                let start = self.reader.pos;
                // Either empty or starting with '/'.
                if self.reader.read(&PATH)? && self.reader.bytes.get(start) != Some(&b'/') {
                    return Err(ParseError::new(start, ParseErrorKind::UnexpectedChar));
                }
                (start, self.reader.pos)
            }
            PathKind::ContinuedNoScheme => {
                self.reader.read(&SEGMENT_NZ_NC)?;
                if self.reader.peek(0) == Some(b':') {
                    // In a relative reference, the first path segment cannot
                    // contain a colon character.
                    return Err(ParseError::new(
                        self.reader.pos,
                        ParseErrorKind::UnexpectedChar,
                    ));
                }
                self.reader.read(&PATH)?;
                (0, self.reader.pos)
            }
        };

        if self.reader.read_str("?") {
            self.reader.read(&QUERY_OR_FRAGMENT)?;
            self.meta.query_end = Some(self.reader.pos);
        }

        if self.reader.read_str("#") {
            self.reader.read(&QUERY_OR_FRAGMENT)?;
            self.meta.has_fragment = true;
        }

        if self.reader.has_remaining() {
            return Err(ParseError::new(
                self.reader.pos,
                ParseErrorKind::UnexpectedChar,
            ));
        }
        Ok(())
    }
}

/// Parses a URI reference: ASCII only, scheme optional.
pub(crate) fn parse(input: &str) -> Result<UriRef<'_>> {
    let mut parser = Parser {
        reader: Reader {
            bytes: input.as_bytes(),
            pos: 0,
        },
        meta: Meta::default(),
    };
    parser.parse_from_scheme()?;
    let meta = parser.meta;

    let scheme = meta.scheme_end.and_then(|end| input.get(..end));
    let authority = meta.auth.and_then(|(host_start, host_end, host)| {
        let start = match meta.scheme_end {
            Some(end) => end.wrapping_add(3),
            None => 2,
        };
        let end = meta.path_bounds.0;
        let text = input.get(start..end)?;
        Some(Authority {
            text,
            host_start: host_start.wrapping_sub(start),
            host_end: host_end.wrapping_sub(start),
            host,
        })
    });
    let path = input
        .get(meta.path_bounds.0..meta.path_bounds.1)
        .unwrap_or("");
    Ok(UriRef {
        scheme,
        authority,
        path,
        has_query: meta.query_end.is_some(),
        has_fragment: meta.has_fragment,
    })
}

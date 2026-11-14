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

//! The wire contract used in runtime mode (L1, R7).
//!
//! A wire is a value that can hold state, such as a schema table (R17). It
//! encodes through a checked [`PayloadWriter`] and decodes into a valid `&mut T`
//! over a slot initialized at startup. Implementing, wrapping or composing a wire
//! needs no `unsafe` (R46). Every wire gets the same spelling, `.wire(w)` (R21);
//! a wire crate can offer sugar through its own extension trait (R22).

use core::fmt;

mod metadata;
mod native;
mod slot;
mod table;
mod writer;

pub use metadata::{
    exact_prefix_len, render_selected_prefix, selected_prefix_len, WireIdentityNamespace,
    FORMAT_VERSION, ID_REF_COMPACT, ID_REF_LITERAL, NATIVE_EXPLICIT_PAYLOAD_FAMILY_ID,
    NATIVE_PREFIX_MAGIC, PROTOBUF_PAYLOAD_FAMILY_ID, PROTOBUF_WIRE_ID,
    STABLE_CONTAINER_PAYLOAD_FAMILY_ID, STABLE_CONTAINER_WIRE_ID, UFRAME_FIELDS_METADATA_LAYOUT_ID,
    UPROTOCOL_NATIVE_WIRE_ID, XCDR_V2_PAYLOAD_FAMILY_ID, XCDR_V2_WIRE_ID,
};
pub use native::NativeWire;
#[cfg(up_rust_test_hooks)]
pub use slot::{__contract_slot, __contract_value};
pub use slot::{assert_no_drop, assert_zero_slot, Ready, StartupSlot, ZeroSlot};
pub use writer::{PayloadWriter, ReservedRange};

/// What `#[derive(ZeroSlot)]` expands to. Not a stable API.
#[doc(hidden)]
pub mod __zero_slot {
    pub use super::slot::{assert_no_drop, assert_zero_slot, ZeroSlot};
}

use crate::frame::native::NativeTypeToken;
use crate::payload::stable::StablePayload;
use crate::transport::waiver::RuntimePermit;
use crate::uattributes::PayloadEncoding;

/// Stable wire, payload-family or metadata-layout identity.
///
/// The literal names the meaning agreed by the interpreting peers. Only the
/// registered first-wave literal and compact pairs may use compact-only
/// references; a custom literal does not become registered by choosing a low
/// compact value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WireIdentity {
    literal_id: &'static str,
    compact_id: u16,
}

impl WireIdentity {
    /// Creates an identity from its literal and compact representations.
    #[must_use]
    pub const fn new(literal_id: &'static str, compact_id: u16) -> Self {
        Self {
            literal_id,
            compact_id,
        }
    }

    /// Returns the language-neutral literal identity.
    #[must_use]
    pub const fn literal_id(self) -> &'static str {
        self.literal_id
    }

    /// Returns the compact first-wave identity.
    #[must_use]
    pub const fn compact_id(self) -> u16 {
        self.compact_id
    }
}

/// A fixed-size refusal from a wire. Carries no heap data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum WireError {
    /// The output storage is too small.
    BufferTooSmall {
        /// Bytes the codec needed.
        needed: usize,
        /// Bytes the storage offered.
        available: usize,
    },
    /// Received bytes have the wrong length.
    InvalidLength {
        /// Expected length.
        expected: usize,
        /// Actual length.
        actual: usize,
    },
    /// Received bytes are misaligned for the type.
    Misaligned {
        /// Required alignment.
        required: usize,
    },
    /// Received bytes are not a valid value of the type.
    InvalidBits,
    /// The received payload is malformed for this wire.
    Malformed(&'static str),
    /// The value cannot be represented by this wire.
    Unencodable(&'static str),
    /// The frame's encoding is not one this wire reads.
    UnknownEncoding(PayloadEncoding),
    /// The wire replaced the writer the SDK handed it (design section 4.1).
    WriterReplaced,
}

impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BufferTooSmall { needed, available } => {
                write!(
                    f,
                    "output storage is too small: needed {needed} bytes, {available} available"
                )
            }
            Self::InvalidLength { expected, actual } => {
                write!(f, "expected {expected} bytes, got {actual}")
            }
            Self::Misaligned { required } => {
                write!(f, "received bytes need alignment {required}")
            }
            Self::InvalidBits => f.write_str("received bytes are not a valid value"),
            Self::Malformed(reason) => write!(f, "malformed payload: {reason}"),
            Self::Unencodable(reason) => write!(f, "value cannot be encoded: {reason}"),
            Self::UnknownEncoding(encoding) => {
                write!(f, "payload encoding {encoding:?} is not read by this wire")
            }
            Self::WriterReplaced => f.write_str("the wire replaced the SDK's writer"),
        }
    }
}

impl core::error::Error for WireError {}

/// How large an encoded value will be, reported by [`Wire::encoded_size`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EncodedSize {
    /// Exactly this many bytes.
    Exact(usize),
    /// At most this many bytes.
    AtMost(usize),
    /// Not known ahead of encoding.
    Unknown,
}

/// What a wire or binding does with the heap in runtime mode.
///
/// Plain data, so a config-chosen wire can answer per value and every wire keeps
/// one spelling (R21). There is no default (R46): both directions of a wire and
/// every prepared route state theirs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeHeap {
    /// Neither allocates nor frees heap across the declared operation's full
    /// lifecycle. Requires the heap-free wire contract checks and source evidence.
    None,
    /// Allocates or frees heap during the declared operation's full lifecycle,
    /// including initialization, error handling and temporary/owned-value cleanup.
    /// Permitted for serialized payload paths with owned-value lifecycle support;
    /// refused on native/mixed paths, naming this reason.
    /// This declaration does not establish panic, OOM, or timing behavior.
    Allocates(&'static str),
    /// The binding's middleware allocates inside its own code. Valid only on a
    /// [`PreparedRoute`](crate::transport::PreparedRoute), and only with a named
    /// waiver (R4). A wire must not declare this.
    Middleware(&'static RuntimePermit),
}

/// The identity a message type travels under.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WirePayloadIdentity {
    /// The encoding written in every frame's header.
    pub encoding: PayloadEncoding,
    /// The native representation token; serialized wires carry none.
    pub native_type_token: Option<NativeTypeToken>,
}

impl WirePayloadIdentity {
    /// A serialized identity with no native representation claim.
    #[must_use]
    pub const fn serialized(encoding: PayloadEncoding) -> Self {
        Self {
            encoding,
            native_type_token: None,
        }
    }
}

/// A wire's identity and capability, stated by value during initialization.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct WireInfo {
    /// Stable name, for refusals and reports.
    pub name: &'static str,
    /// The selected wire identity written into a selected-wire prefix.
    pub wire_id: WireIdentity,
    /// The payload family, admitted together with the envelope (R10).
    pub payload_family: WireIdentity,
    /// The payload identity carried on every frame.
    pub identity: WirePayloadIdentity,
    /// The largest encoded payload, when the wire can bound it.
    pub max_encoded_len: Option<usize>,
    /// The payload's required alignment.
    pub alignment: usize,
    /// Whether every value that encodes successfully has an exact size from
    /// [`Wire::encoded_size`]. Unencodable values may report unknown size, but
    /// [`Wire::encode`] must then refuse them before publication.
    pub exact_size: bool,
    encode_heap: RuntimeHeap,
    decode_heap: RuntimeHeap,
}

impl WireInfo {
    /// Declares a wire. Both heap arguments are required, so there is no default
    /// claim (R46). Their scope is the complete lifecycle documented by
    /// [`Self::encode_heap`] and [`Self::decode_heap`], including initialization
    /// and owned-value cleanup, not merely the named callback bodies.
    #[must_use]
    pub const fn new(
        name: &'static str,
        wire_id: WireIdentity,
        payload_family: WireIdentity,
        identity: WirePayloadIdentity,
        encode_heap: RuntimeHeap,
        decode_heap: RuntimeHeap,
    ) -> Self {
        Self {
            name,
            wire_id,
            payload_family,
            identity,
            max_encoded_len: None,
            alignment: 1,
            exact_size: false,
            encode_heap,
            decode_heap,
        }
    }

    /// Declares a wrapper around `inner`.
    ///
    /// The wrapper states its own heap class per direction: it cannot inherit a
    /// claim by accident, because the fields are read-only and no constructor
    /// copies them from `inner`. Payload identity, family and alignment do carry
    /// over; a wrapper that changes them builds a [`WireInfo::new`] instead.
    #[must_use]
    pub const fn wrapping(
        inner: &WireInfo,
        name: &'static str,
        wire_id: WireIdentity,
        encode_heap: RuntimeHeap,
        decode_heap: RuntimeHeap,
    ) -> Self {
        Self {
            name,
            wire_id,
            payload_family: inner.payload_family,
            identity: inner.identity,
            max_encoded_len: inner.max_encoded_len,
            alignment: inner.alignment,
            exact_size: false,
            encode_heap,
            decode_heap,
        }
    }

    /// Bounds the encoded size. A route whose wire has no bound needs `.max_payload(n)`.
    #[must_use]
    pub const fn max_encoded_len(mut self, len: usize) -> Self {
        self.max_encoded_len = Some(len);
        self
    }

    /// Promises every successfully encodable value has an exact size from
    /// [`Wire::encoded_size`], admitting bindings that cannot truncate. An
    /// unencodable value may report unknown size only if [`Wire::encode`] returns
    /// an error; the SDK still checks actual length and refuses disagreement.
    #[must_use]
    pub const fn exact(mut self) -> Self {
        self.exact_size = true;
        self
    }

    /// States the payload's alignment. Defaults to 1.
    #[must_use]
    pub const fn alignment(mut self, alignment: usize) -> Self {
        self.alignment = alignment;
        self
    }

    /// Heap class of encoding, including callback/error temporaries and cleanup
    /// caused by the operation. This declaration is not a panic/OOM proof.
    #[must_use]
    pub const fn encode_heap(&self) -> RuntimeHeap {
        self.encode_heap
    }

    /// Heap class of the full decoded-value lifecycle: init_slot (including
    /// runtime reinitialization), decode/replacement, and destruction on error or
    /// release. This declaration is not a panic/OOM proof.
    #[must_use]
    pub const fn decode_heap(&self) -> RuntimeHeap {
        self.decode_heap
    }
}

/// A wire for message type `T`: a plain safe trait (R46).
///
/// `encode` and `decode` must not panic on any input: a malformed frame is `Err`.
/// Encoding must be repeatable: a refused send may be retried with the same value.
/// These are behavioral/resource obligations, not unsafe invariants: arbitrary
/// safe implementations may violate them without invalidating the SDK's references.
/// Heap declarations include initializer, replacement and owned-value cleanup;
/// qualification of a strict profile must cover panic/OOM across that lifecycle.
pub trait Wire<T>: Send + Sync + 'static {
    /// Declares this wire's identity and capability.
    fn describe(&self) -> WireInfo;

    /// Estimates the encoded size. An exact answer lets a route loan exactly.
    fn encoded_size(&self, value: &T) -> EncodedSize {
        let _ = value;
        EncodedSize::Unknown
    }

    /// Writes `value` through `out`, once.
    ///
    /// # Errors
    /// Reports a value that cannot be represented or a destination too small.
    fn encode(&self, value: &T, out: &mut PayloadWriter<'_>) -> Result<(), WireError>;

    /// Gives a value slot its first valid value. Native/no-drop slots are seeded
    /// during setup; owned serialized slots may call this again in runtime after
    /// their previous value was released. Route configuration remains setup-only.
    fn init_slot<'s>(&self, slot: StartupSlot<'s, T>) -> Ready<'s, T>;

    /// Decodes `bytes` into `into`, which holds a valid `T` from an earlier use or from startup.
    ///
    /// # Errors
    /// Reports malformed input or an encoding this wire does not read.
    fn decode(
        &self,
        encoding: PayloadEncoding,
        bytes: &[u8],
        into: &mut T,
    ) -> Result<(), WireError>;
}

/// How a wire carries a message: [`Encoded`] or [`InPlace`]. Sealed.
pub trait WireMode: sealed::Sealed + Send + Sync + 'static {}

/// The payload is encoded directly into, and decoded from, transport storage.
#[derive(Clone, Copy, Debug, Default)]
pub struct Encoded;
/// The payload is initialized in place in transport storage and borrowed typed.
#[derive(Clone, Copy, Debug, Default)]
pub struct InPlace;

impl sealed::Sealed for Encoded {}
impl sealed::Sealed for InPlace {}
impl WireMode for Encoded {}
impl WireMode for InPlace {}

mod sealed {
    pub trait Sealed {}
}

/// What a [`RoleWire`] gives its role at setup. Hidden; an implementation detail.
#[doc(hidden)]
#[allow(missing_debug_implementations)]
pub enum RoleWireKind<T: 'static> {
    /// A serialized wire value, placed in startup memory.
    Encoded(&'static dyn Wire<T>),
    /// Genuine in-place native carriage; nothing to place.
    InPlace,
}

/// The sealed adapter behind `.wire(w)`.
///
/// One spelling for every wire (R21): any [`Wire<T>`] is a role wire in
/// [`Encoded`] mode, and [`NativeWire`] is one in [`InPlace`] mode. `M` is
/// inferred at `.build()`.
pub trait RoleWire<T: 'static, M: WireMode>:
    role_sealed::Sealed<T, M> + Send + Sync + 'static
{
    #[doc(hidden)]
    fn __describe(&self) -> Result<WireInfo, &'static str>;

    #[doc(hidden)]
    fn __place(
        self,
        memory: &crate::memory::StartupMemory,
    ) -> Result<RoleWireKind<T>, crate::memory::StartupError>
    where
        Self: Sized;
}

mod role_sealed {
    pub trait Sealed<T, M> {}
}

impl<T: 'static, W: Wire<T>> role_sealed::Sealed<T, Encoded> for W {}
impl<T: StablePayload> role_sealed::Sealed<T, InPlace> for NativeWire {}

impl<T: 'static, W: Wire<T>> RoleWire<T, Encoded> for W {
    fn __describe(&self) -> Result<WireInfo, &'static str> {
        Ok(self.describe())
    }

    fn __place(
        self,
        memory: &crate::memory::StartupMemory,
    ) -> Result<RoleWireKind<T>, crate::memory::StartupError> {
        let placed: &'static mut W = memory.place(self)?;
        Ok(RoleWireKind::Encoded(&*placed))
    }
}

impl<T: StablePayload> RoleWire<T, InPlace> for NativeWire {
    fn __describe(&self) -> Result<WireInfo, &'static str> {
        self.info::<T>()
    }

    fn __place(
        self,
        _memory: &crate::memory::StartupMemory,
    ) -> Result<RoleWireKind<T>, crate::memory::StartupError> {
        Ok(RoleWireKind::InPlace)
    }
}

/// What an RPC role's wire placed: one wire used both ways, or native carriage.
#[doc(hidden)]
#[allow(missing_debug_implementations)]
pub enum RpcWireKind<Req: 'static, Resp: 'static> {
    /// A serialized wire value, placed once and used for both messages.
    Encoded {
        /// The wire's Rust type.
        type_id: core::any::TypeId,
        /// The wire, for requests.
        request: &'static dyn Wire<Req>,
        /// The same wire, for responses.
        response: &'static dyn Wire<Resp>,
    },
    /// Genuine in-place native carriage, both ways; nothing to place.
    InPlace,
}

/// The sealed adapter behind an RPC role's `.wire(w)`: one wire, used both ways.
///
/// A type that is a [`Wire`] of both messages is an RPC wire in [`Encoded`] mode,
/// and [`NativeWire`] is one in [`InPlace`] mode. `M` is inferred at `.build()`.
pub trait RpcRoleWire<Req: 'static, Resp: 'static, M: WireMode>:
    rpc_role_sealed::Sealed<Req, Resp, M> + Send + Sync + 'static
{
    #[doc(hidden)]
    fn __describe(&self) -> Result<(WireInfo, WireInfo), &'static str>;

    #[doc(hidden)]
    fn __place(
        self,
        memory: &crate::memory::StartupMemory,
    ) -> Result<RpcWireKind<Req, Resp>, crate::memory::StartupError>
    where
        Self: Sized;
}

mod rpc_role_sealed {
    pub trait Sealed<Req, Resp, M> {}
}

impl<Req: 'static, Resp: 'static, W: Wire<Req> + Wire<Resp>>
    rpc_role_sealed::Sealed<Req, Resp, Encoded> for W
{
}

impl<Req: StablePayload, Resp: StablePayload> rpc_role_sealed::Sealed<Req, Resp, InPlace>
    for NativeWire
{
}

impl<Req: 'static, Resp: 'static, W: Wire<Req> + Wire<Resp>> RpcRoleWire<Req, Resp, Encoded> for W {
    fn __describe(&self) -> Result<(WireInfo, WireInfo), &'static str> {
        Ok((Wire::<Req>::describe(self), Wire::<Resp>::describe(self)))
    }

    fn __place(
        self,
        memory: &crate::memory::StartupMemory,
    ) -> Result<RpcWireKind<Req, Resp>, crate::memory::StartupError> {
        let placed: &'static W = memory.place(self)?;
        Ok(RpcWireKind::Encoded {
            type_id: core::any::TypeId::of::<W>(),
            request: placed,
            response: placed,
        })
    }
}

impl<Req: StablePayload, Resp: StablePayload> RpcRoleWire<Req, Resp, InPlace> for NativeWire {
    fn __describe(&self) -> Result<(WireInfo, WireInfo), &'static str> {
        Ok((self.info::<Req>()?, self.info::<Resp>()?))
    }

    fn __place(
        self,
        _memory: &crate::memory::StartupMemory,
    ) -> Result<RpcWireKind<Req, Resp>, crate::memory::StartupError> {
        Ok(RpcWireKind::InPlace)
    }
}

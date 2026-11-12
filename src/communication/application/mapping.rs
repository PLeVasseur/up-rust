use crate::{
    DecodePayload, EncodePayload, PayloadIdentity, UOwnedTransport, UWireError, WireIdentity,
};
use bytes::Bytes;

/// Schema-owned stateless encoder and its explicit payload declaration.
/// This table never selects an envelope or infers a domain schema from bytes.
///
/// A declaration without an actual schema mapping cannot construct a table:
///
/// ```compile_fail
/// use up_rust::communication::application::Encoder;
/// struct NoDoorMapping;
/// let _ = Encoder::<String>::from_schema_mapping::<NoDoorMapping>();
/// ```
pub struct Encoder<P> {
    identity: PayloadIdentity,
    family: Option<WireIdentity>,
    encode: fn(&P) -> Result<Bytes, UWireError>,
}
/// Schema-owned decoder producing an owned value, never an erased native borrow.
pub struct Decoder<P> {
    identity: PayloadIdentity,
    family: Option<WireIdentity>,
    decode: fn(&[u8]) -> Result<P, UWireError>,
}
impl<P> Copy for Encoder<P> {}
impl<P> Clone for Encoder<P> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<P> Copy for Decoder<P> {}
impl<P> Clone for Decoder<P> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<P> std::fmt::Debug for Encoder<P> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Encoder")
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}
impl<P> std::fmt::Debug for Decoder<P> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Decoder")
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}
fn fixed(identity: PayloadIdentity) -> Result<PayloadIdentity, UWireError> {
    if matches!(identity, PayloadIdentity::Fixed(_)) {
        Ok(identity)
    } else {
        Err(UWireError::invalid_payload(
            "serialized mapping requires fixed identity",
        ))
    }
}
impl<P> Encoder<P> {
    /// Schema-package escape hatch for a stateless, explicitly declared mapping.
    ///
    /// # Errors
    /// Native identities cannot enter the owned serialization path.
    pub fn from_schema_mapping<K: EncodePayload<P>>() -> Result<Self, UWireError> {
        Ok(Self {
            identity: fixed(K::payload_identity(None)?)?,
            family: K::payload_family(),
            encode: K::encode_payload_owned,
        })
    }
    /// Admits the payload family against independently configured carriage context.
    ///
    /// # Errors
    /// Returns the carriage's exact declaration/capability refusal.
    pub fn admit(&self, transport: &dyn UOwnedTransport) -> Result<(), UWireError> {
        transport.admit_payload_contract(self.identity, self.family)
    }
    pub(super) fn admit_carriage(&self, transport: &dyn UOwnedTransport) -> Result<(), UWireError> {
        transport.admit_carriage_payload_contract(self.identity, self.family)
    }
    pub(super) fn encode(&self, value: &P) -> Result<Bytes, UWireError> {
        (self.encode)(value)
    }
    /// Exact declared identity, not a schema discovery mechanism.
    pub fn identity(&self) -> PayloadIdentity {
        self.identity
    }
    pub(super) fn family(&self) -> Option<WireIdentity> {
        self.family
    }
}
fn decode_owned<P, K: for<'bytes> DecodePayload<'bytes, P>>(bytes: &[u8]) -> Result<P, UWireError> {
    K::decode_payload(bytes)
}
impl<P> Decoder<P> {
    /// Schema-package escape hatch for an owned, stateless decoding mapping.
    ///
    /// # Errors
    /// Native identities cannot enter the owned decoding path.
    pub fn from_schema_mapping<K: for<'bytes> DecodePayload<'bytes, P>>() -> Result<Self, UWireError>
    {
        Ok(Self {
            identity: fixed(K::payload_identity(None)?)?,
            family: K::payload_family(),
            decode: decode_owned::<P, K>,
        })
    }
    /// Admits the independently selected payload and envelope context.
    ///
    /// # Errors
    /// Returns the carriage's exact declaration/capability refusal.
    pub fn admit(&self, transport: &dyn UOwnedTransport) -> Result<(), UWireError> {
        transport.admit_payload_contract(self.identity, self.family)
    }
    pub(super) fn admit_carriage(&self, transport: &dyn UOwnedTransport) -> Result<(), UWireError> {
        transport.admit_carriage_payload_contract(self.identity, self.family)
    }
    pub(super) fn decode(&self, bytes: &[u8]) -> Result<P, UWireError> {
        (self.decode)(bytes)
    }
    /// Exact declaration checked before decoding.
    pub fn identity(&self) -> PayloadIdentity {
        self.identity
    }
    pub(super) fn family(&self) -> Option<WireIdentity> {
        self.family
    }
}

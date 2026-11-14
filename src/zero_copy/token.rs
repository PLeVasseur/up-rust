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
//! Fixed-capacity access-token ownership for prepared senders.

use std::sync::atomic::{compiler_fence, Ordering};

/// Failure to reserve a token buffer during setup.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum TokenBufferSetupError {
    /// Capacity exceeds the canonical u16 token-length field.
    #[error("token capacity exceeds wire range")]
    InvalidCapacity,
    /// The requested storage could not be allocated at creation.
    #[error("token storage cannot be reserved")]
    Allocation,
}

/// Fixed-size failure to update an existing token buffer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum TokenError {
    /// The new token exceeds the reserved capacity; the prior value is unchanged.
    #[error("token exceeds reserved capacity")]
    TooLong,
}

/// An optional UTF-8 token whose storage is reserved once at creation.
///
/// Overwrite, clear and Drop wipe the owned bytes. This does not wipe the caller's
/// input, copies made elsewhere, or native storage owned by another component.
/// Debug output contains only capacity, presence and length.
pub struct TokenBuffer {
    bytes: Box<[u8]>,
    len: usize,
    present: bool,
}

impl TokenBuffer {
    /// Creates an initially absent token with the requested byte capacity.
    ///
    /// Zero capacity can represent an absent or present empty token. A route may
    /// impose a separate token-presence policy.
    ///
    /// # Errors
    /// Rejects a capacity outside the wire range or a failed setup allocation.
    pub fn new(capacity: usize) -> Result<Self, TokenBufferSetupError> {
        if u16::try_from(capacity).is_err() {
            return Err(TokenBufferSetupError::InvalidCapacity);
        }
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(capacity)
            .map_err(|_| TokenBufferSetupError::Allocation)?;
        bytes.resize(capacity, 0);
        Ok(Self {
            bytes: bytes.into_boxed_slice(),
            len: 0,
            present: false,
        })
    }

    /// Replaces the token without allocating and wipes the previous value first.
    ///
    /// # Errors
    /// Returns TooLong before changing any byte when the token will not fit.
    pub fn set(&mut self, value: &str) -> Result<(), TokenError> {
        if value.len() > self.bytes.len() {
            return Err(TokenError::TooLong);
        }
        self.wipe();
        self.bytes
            .get_mut(..value.len())
            .expect("checked token capacity")
            .copy_from_slice(value.as_bytes());
        self.len = value.len();
        self.present = true;
        Ok(())
    }

    /// Clears presence and wipes the owned token bytes.
    pub fn clear(&mut self) {
        self.wipe();
        self.len = 0;
        self.present = false;
    }

    /// Borrows the current token, preserving present-empty versus absent.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        self.present.then(|| {
            std::str::from_utf8(self.bytes.get(..self.len).expect("stored token length"))
                .expect("token is copied only from UTF-8 input")
        })
    }

    /// Reserved byte capacity, unchanged after creation.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.bytes.len()
    }

    /// Current token byte length; an absent token has length zero.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the current token has no bytes; use as_str to inspect presence.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn wipe(&mut self) {
        for byte in &mut self.bytes {
            // SAFETY: the buffer uniquely owns each valid u8 destination. Volatile
            // stores make the erasure observable even immediately before release.
            unsafe { std::ptr::write_volatile(byte, 0) };
        }
        compiler_fence(Ordering::SeqCst);
    }
}

impl std::fmt::Debug for TokenBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenBuffer")
            .field("capacity", &self.capacity())
            .field("present", &self.present)
            .field("len", &self.len)
            .finish()
    }
}

impl Drop for TokenBuffer {
    fn drop(&mut self) {
        self.wipe();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overwrite_and_clear_erase_owned_bytes() {
        let mut token = TokenBuffer::new(32).unwrap();
        token.set("a-long-private-token").unwrap();
        token.set("new").unwrap();
        assert_eq!(token.as_str(), Some("new"));
        assert!(token.bytes.get(3..).unwrap().iter().all(|byte| *byte == 0));
        assert!(!format!("{token:?}").contains("new"));
        token.clear();
        assert_eq!(token.as_str(), None);
        assert!(token.bytes.iter().all(|byte| *byte == 0));
    }

    #[test]
    fn oversized_update_keeps_the_previous_token() {
        let mut token = TokenBuffer::new(4).unwrap();
        token.set("old").unwrap();
        assert_eq!(token.set("too-long"), Err(TokenError::TooLong));
        assert_eq!(token.as_str(), Some("old"));
        assert_eq!(token.capacity(), 4);
    }

    #[test]
    fn byte_capacity_and_empty_presence_are_distinct() {
        let mut empty = TokenBuffer::new(0).unwrap();
        assert_eq!(empty.as_str(), None);
        empty.set("").unwrap();
        assert_eq!(empty.as_str(), Some(""));
        assert_eq!(empty.set("x"), Err(TokenError::TooLong));
        let mut unicode = TokenBuffer::new(2).unwrap();
        unicode.set("é").unwrap();
        assert_eq!(unicode.len(), 2);
        assert_eq!(unicode.set("éx"), Err(TokenError::TooLong));
        assert!(matches!(
            TokenBuffer::new(usize::MAX),
            Err(TokenBufferSetupError::InvalidCapacity)
        ));
    }
}

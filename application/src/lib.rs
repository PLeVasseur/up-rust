//! Application operations above the synchronous, dependency-free safety core.
//!
//! This SDK crate is the same on hosts and MCUs. It has no allocator, executor,
//! target-selected API, or transport implementation. The core continues to own
//! route admission, startup storage, identity, clocks and transport dispatch.
//! Futures live here so the core's prohibition on futures remains intact.
#![no_std]
#![forbid(unsafe_code)]

pub mod application;

//! Compile the real existing temperature/pressure service on host or no_std targets.
//! No substitute domain, engine, platform, transport or runtime harness lives here.
#![no_std]
#![forbid(unsafe_code)]
// These shared support files also serve the host SDK's existing test targets.
// Their host feature names are deliberately absent from this no-feature package.
#![allow(unexpected_cfgs)]

#[allow(dead_code, unused_imports)]
#[path = "../../examples/support/independent_schema.rs"]
mod schema;
#[allow(dead_code, unused_imports)]
#[path = "../../examples/support/independent_apps.rs"]
mod apps;

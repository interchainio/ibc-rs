#![forbid(unsafe_code)]
#![deny(
    warnings,
    // missing_docs,
    trivial_casts,
    trivial_numeric_casts,
    unused_import_braces,
    unused_qualifications,
    rust_2018_idioms
)]
#![allow(dead_code)]

//! Implementation of the following ICS modules:
//!
//! - ICS 02: Client
//! - ICS 07: Tendermint Client
//! - ICS 23: Vector Commitment Scheme
//! - ICS 24: Host Requirements

pub mod ics02_client;
pub mod ics07_tendermint;
// pub mod ics23_commitment;
pub mod ics24_host;

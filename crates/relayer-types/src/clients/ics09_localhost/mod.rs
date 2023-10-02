//! ICS 09: Loopback Client
//! Loopback client, designed to be used for interaction over the
//! IBC interface with modules present on the same ledger.
//!
//! Loopback clients may be useful in cases where the calling module
//! does not have prior knowledge of where precisely the destination
//! module lives and would like to use the uniform IBC message-passing
//! interface (similar to 127.0.0.1 in TCP/IP).

pub mod v1;
pub mod v2;

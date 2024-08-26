//! ICS 20: Token Transfer implementation allows for multi-chain denomination handling, which
//! constitutes a "fungible token transfer bridge module" between the IBC routing module and an
//! asset tracking module.

pub mod acknowledgement;
pub mod amount;
pub mod coin;
pub mod denom;
pub mod error;
pub mod events;
pub mod msgs;
pub mod packet;
pub mod v1;
pub mod v2;

pub use amount::*;
pub use coin::*;
pub use denom::*;

/// Module identifier for the ICS20 application.
pub const MODULE_ID_STR: &str = "transfer";

/// The port identifier that the ICS20 applications
/// typically bind with.
pub const PORT_ID_STR: &str = "transfer";

/// ICS20 application current version.
pub const VERSION_PREFIX: &str = "ics20";

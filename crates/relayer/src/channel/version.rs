//! Helper module for the relayer channel logic.
//!
//! Provides support for resolving the appropriate
//! channel version to be used in a channel open
//! handshake.

use ibc_relayer_types::{applications::transfer, core::ics24_host::identifier::PortId};

pub use ibc_relayer_types::core::ics04_channel::version::Version;

/// Returns the default channel version, depending on the given [`PortId`].
pub fn default_by_port(port_id: &PortId) -> Option<Version> {
    if port_id.as_str() == transfer::PORT_ID_STR {
        // https://github.com/cosmos/ibc/tree/master/spec/app/ics-020-fungible-token-transfer#forwards-compatibility
        Some(Version::ics20(1))
    } else {
        None
    }
}

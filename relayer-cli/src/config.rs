//! Cli Config
//!
//! See instructions in `commands.rs` to specify the path to your
//! application's configuration file and/or command-line options
//! for specifying it.

use alloc::collections::BTreeSet;
use std::path::PathBuf;

use flex_error::{define_error, TraceError};
use ibc::ics24_host::identifier::ChainId;
use ibc_relayer::config::Config;
use tendermint_light_client::types::TrustThreshold;
use tracing_subscriber::filter::ParseError;

use crate::application::app_reader;

/// Get the path to configuration file
pub fn config_path() -> Option<PathBuf> {
    let app = app_reader();
    app.config_path().cloned()
}

// Specifies all the possible syntactic errors
// that a Hermes configuration file could contain.
define_error! {
    Error {
        ZeroChain
            |_| { "config file does not specify any chain" },

        InvalidLogLevel
            { log_level: String, }
            [ TraceError<ParseError> ]
            |e| {
                format!("config file specifies an invalid log level ('{0}'), caused by",
                    e.log_level)
            },

        DuplicateChains
            { chain_id: ChainId }
            |e| {
                format!("config file has duplicate entry for the chain with id {0}",
                    e.chain_id)
            },

        InvalidTrustThreshold
            {
                threshold: TrustThreshold,
                chain_id: ChainId,
                reason: String
            }
            |e| {
                format!("config file specifies an invalid trust threshold ({0}) for the chain with id {1}, caused by: {2}",
                    e.threshold, e.chain_id, e.reason)
            },
    }
}

/// Method for syntactic validation of the input configuration file.
pub fn validate_config(config: &Config) -> Result<(), Error> {
    // Check for duplicate chain configuration and invalid trust thresholds
    let mut unique_chain_ids = BTreeSet::new();
    for c in config.chains.iter() {
        if !unique_chain_ids.insert(c.id.clone()) {
            return Err(Error::duplicate_chains(c.id.clone()));
        }

        validate_trust_threshold(&c.id, c.trust_threshold)?;
    }

    Ok(())
}

/// Check that the trust threshold is:
///
/// a) non-zero
/// b) greater or equal to 1/3
/// c) strictly less than 1
fn validate_trust_threshold(id: &ChainId, trust_threshold: TrustThreshold) -> Result<(), Error> {
    if trust_threshold.denominator() == 0 {
        return Err(Error::invalid_trust_threshold(
            trust_threshold,
            id.clone(),
            "trust threshold denominator cannot be zero".to_string(),
        ));
    }

    if trust_threshold.numerator() * 3 < trust_threshold.denominator() {
        return Err(Error::invalid_trust_threshold(
            trust_threshold,
            id.clone(),
            "trust threshold cannot be < 1/3".to_string(),
        ));
    }

    if trust_threshold.numerator() >= trust_threshold.denominator() {
        return Err(Error::invalid_trust_threshold(
            trust_threshold,
            id.clone(),
            "trust threshold cannot be >= 1".to_string(),
        ));
    }

    Ok(())
}

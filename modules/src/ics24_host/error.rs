use crate::prelude::*;
use flex_error::define_error;
use serde::Serialize;

define_error! {
    #[derive(Debug, PartialEq, Eq, Serialize)]
    ValidationError {
        ContainSeparator
            { id : String }
            | e | { format_args!("identifier {0} cannot contain separator '/'", e.id) },

        InvalidLength
            {
                id: String,
                length: usize,
                min: usize,
                max: usize,
            }
            | e | { format_args!("identifier {0} has invalid length {1} must be between {2}-{3} characters", e.id, e.length, e.min, e.max) },

        InvalidCharacter
            { id: String }
            | e | { format_args!("identifier {0} must only contain alphanumeric characters or `.`, `_`, `+`, `-`, `#`, - `[`, `]`, `<`, `>`", e.id) },

        Empty
            | _ | { "identifier cannot be empty" },

        ChainIdInvalidFormat
            { id: String }
            | e | { format_args!("chain identifiers are expected to be in epoch format {0}", e.id) },

        InvalidCounterpartyChannelId
            |_| { "Invalid channel id in counterparty" }
    }
}

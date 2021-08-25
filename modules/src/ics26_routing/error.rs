use crate::prelude::*;
use flex_error::{define_error, TraceError};

use crate::application::ics20_fungible_token_transfer;
use crate::ics02_client;
use crate::ics03_connection;
use crate::ics04_channel;

define_error! {
    #[derive(Debug, PartialEq, Eq)]
    Error {
        Ics02Client
            [ ics02_client::error::Error ]
            | _ | { "ICS02 client error" },

        Ics03Connection
            [ ics03_connection::error::Error ]
            | _ | { "ICS03 connection error" },

        Ics04Channel
            [ ics04_channel::error::Error ]
            | _ | { "ICS04 channel error" },

        Ics20FungibleTokenTransfer
            [ ics20_fungible_token_transfer::error::Error ]
            | _ | { "ICS20 fungible token transfer error" },

        UnknownMessageTypeUrl
            { url: String }
            | e | { format_args!("unknown type URL {0}", e.url) },

        MalformedMessageBytes
            [ TraceError<tendermint_proto::Error> ]
            | _ | { "the message is malformed and cannot be decoded" },
    }
}

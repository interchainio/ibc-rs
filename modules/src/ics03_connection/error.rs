use crate::ics02_client::error as client_error;
use crate::ics24_host::error::ValidationError;
use crate::ics24_host::identifier::{ClientId, ConnectionId};
use crate::proofs::ProofError;
use crate::Height;
use flex_error::define_error;

define_error! {
    #[derive(Debug, PartialEq, Eq)]
    Error {
        Ics02Client
            [ client_error::Error ]
            | _ | { "ics02 client error" },

        InvalidState
            { state: i32 }
            | e | { format_args!("connection state is unknown: {}", e.state) },

        ConnectionExistsAlready
            { connection_id: ConnectionId }
            | e | {
                format_args!("connection exists (was initialized) already: {0}",
                    e.connection_id)
            },

        ConnectionMismatch
            { connection_id: ConnectionId }
            | e | {
                format_args!("connection end for identifier {0} was never initialized",
                    e.connection_id)
            },

        UninitializedConnection
            { connection_id: ConnectionId }
            | e | {
                format_args!("connection end for identifier {0} was never initialized",
                    e.connection_id)
            },

        InvalidConsensusHeight
            {
                target_height: Height,
                currrent_height: Height
            }
            | e | {
                format_args!("consensus height claimed by the client on the other party is too advanced: {0} (host chain current height: {1})",
                    e.target_height, e.currrent_height)
            },

        StaleConsensusHeight
            {
                target_height: Height,
                oldest_height: Height
            }
            | e | {
                format_args!("consensus height claimed by the client on the other party has been pruned: {0} (host chain oldest height: {1})",
                    e.target_height, e.oldest_height)
            },

        InvalidIdentifier
            [ ValidationError ]
            | _ | { "identifier error" },

        EmptyProtoConnectionEnd
            | _ | { "ConnectionEnd domain object could not be constructed out of empty proto object" },

        EmptyVersions
            | _ | { "empty supported versions" },

        EmptyFeatures
            | _ | { "empty supported features" },

        NoCommonVersion
            | _ | { "no common version" },

        InvalidAddress
            | _ | { "invalid address" },

        MissingProofHeight
            | _ | { "missing proof height" },

        MissingConsensusHeight
            | _ | { "missing consensus height" },

        InvalidProof
            [ ProofError ]
            | _ | { "invalid connection proof" },

        VerifyConnectionState
            [ client_error::Error ]
            | _ | { "error verifying connnection state" },

        InvalidSigner
            | _ | { "invalid signer" },

        ConnectionNotFound
            { connection_id: ConnectionId }
            | e | {
                format_args!("no connection was found for the previous connection id provided {0}",
                    e.connection_id)
            },

        InvalidCounterparty
            | _ | { "invalid signer" },

        ConnectionIdMismatch
            {
                connection_id: ConnectionId,
                counterparty_connection_id: ConnectionId,
            }
            | e | {
                format_args!("counterparty chosen connection id {0} is different than the connection id {1}",
                    e.connection_id, e.counterparty_connection_id)
            },

        MissingCounterparty
            | _ | { "missing counterparty" },


        MissingCounterpartyPrefix
            | _ | { "missing counterparty prefix" },

        MissingClient
            { client_id: ClientId }
            | e | {
                format_args!("the client id does not match any client state: {0}",
                    e.client_id)
            },

        NullClientProof
            | _ | { "client proof must be present" },

        FrozenClient
            { client_id: ClientId }
            | e | {
                format_args!("the client id does not match any client state: {0}",
                    e.client_id)
            },

        ConnectionVerificationFailure
            | _ | { "the connection proof verification failed" },

        MissingClientConsensusState
            {
                height: Height,
                client_id: ClientId,
            }
            | e | {
                format_args!("the consensus state at height {0} for client id {1} could not be retrieved",
                    e.height, e.client_id)
            },

        MissingLocalConsensusState
            { height: Height }
            | e | { format_args!("the local consensus state could not be retrieved for height {}", e.height) },

        ConsensusStateVerificationFailure
            { height: Height }
            [ client_error::Error ]
            | e | {
                format_args!("the consensus proof verification failed (height: {0})",
                    e.height)
            },

        // TODO: use more specific error source
        ClientStateVerificationFailure
            {
                client_id: ClientId,
            }
            [ client_error::Error ]
            | e | {
                format_args!("the client state proof verification failed for client id {0}",
                    e.client_id)
            },

        ImplementationSpecific
            | _ | { "implementation specific error" },
    }
}

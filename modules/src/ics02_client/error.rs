use crate::ics02_client::client_type::ClientType;
use crate::ics07_tendermint::error::Error as Ics07Error;
use crate::ics23_commitment::error::Error as Ics23Error;
use crate::ics24_host::error::ValidationError;
use crate::ics24_host::identifier::ClientId;
use crate::timestamp::Timestamp;
use crate::Height;
use std::num::TryFromIntError;
use tendermint_proto::Error as TendermintError;

use flex_error::{define_error, TraceError};

define_error! {
    Error {
        UnknownClientType
            { client_type: String }
            | e | { format_args!("unknown client type: {0}", e.client_type) },

        ClientIdentifierConstructor
            { client_type: ClientType, counter: u64 }
            [ ValidationError ]
            | e | {
                format_args!("Client identifier constructor failed for type {0} with counter {1}",
                    e.client_type, e.counter)
            },

        ClientAlreadyExists
            { client_id: ClientId }
            | e | { format_args!("client already exists: {0}", e.client_id) },

        ClientNotFound
            { client_id: ClientId }
            | e | { format_args!("client not found: {0}", e.client_id) },

        ClientFrozen
            { client_id: ClientId }
            | e | { format_args!("client is frozen: {0}", e.client_id) },

        ConsensusStateNotFound
            { client_id: ClientId, height: Height }
            | e | {
                format_args!("consensus state not found at: {0} at height {1}",
                    e.client_id, e.height)
            },

        ImplementationSpecific
            | _ | { "implementation specific error" },

        HeaderVerificationFailure
            { reason: String }
            | e | { format_args!("header verification failed with reason: {}", e.reason) },

        UnknownClientStateType
            { client_state_type: String }
            | e | { format_args!("unknown client state type: {0}", e.client_state_type) },

        EmptyClientStateResponse
            | _ | { "the client state was not found" },

        EmptyPrefix
            { source: crate::ics23_commitment::merkle::EmptyPrefixError }
            | _ | { "empty prefix" },

        UnknownConsensusStateType
            { consensus_state_type: String }
            | e | {
                format_args!("unknown client consensus state type: {0}",
                    e.consensus_state_type)
            },

        EmptyConsensusStateResponse
            | _ | { "the client state was not found" },

        UnknownHeaderType
            { header_type: String }
            | e | {
                format_args!("unknown header type: {0}",
                    e.header_type)
            },

        UnknownMisbehaviourType
            { misbehavior_type: String }
            | e | {
                format_args!("unknown misbehaviour type: {0}",
                    e.misbehavior_type)
            },

        InvalidRawClientId
            { client_id: String }
            [ ValidationError ]
            | e | {
                format_args!("invalid raw client identifier {0}",
                    e.client_id)
            },

        DecodeRawClientState
            [ TraceError<TendermintError> ]
            | _ | { "error decoding raw client state" },

        MissingRawClientState
            | _ | { "missing raw client state" },

        InvalidRawConsensusState
            [ TraceError<TendermintError> ]
            | _ | { "invalid raw client consensus state" },

        MissingRawConsensusState
            | _ | { "missing raw client consensus state" },

        InvalidMsgUpdateClientId
            [ ValidationError ]
            | _ | { "invalid client id in the update client message" },

        Decode
            [ TraceError<prost::DecodeError> ]
            | _ | { "decode error" },

        MissingHeight
            | _ | { "invalid raw client consensus state: the height field is missing" },

        InvalidClientIdentifier
            [ ValidationError ]
            | _ | { "invalid client identifier" },

        InvalidRawHeader
            [ TraceError<TendermintError> ]
            | _ | { "invalid raw header" },

        MissingRawHeader
            | _ | { "missing raw header" },

        DecodeRawMisbehaviour
            [ TraceError<TendermintError> ]
            | _ | { "invalid raw misbehaviour" },

        InvalidRawMisbehaviour
            [ ValidationError ]
            | _ | { "invalid raw misbehaviour" },

        MissingRawMisbehaviour
            | _ | { "missing raw misbehaviour" },

        InvalidHeightResult
            | _ | { "height cannot end up zero or negative" },

        InvalidAddress
            | _ | { "invalid address" },

        InvalidUpgradeClientProof
            [ Ics23Error ]
            | _ | { "invalid proof for the upgraded client state" },

        InvalidUpgradeConsensusStateProof
            [ Ics23Error ]
            | _ | { "invalid proof for the upgraded consensus state" },

        Tendermint
            [ Ics07Error ]
            | _ | { "tendermint error" },

        InvalidPacketTimestamp
            [ TraceError<TryFromIntError> ]
            | _ | { "invalid packet timeout timestamp value" },

        ClientArgsTypeMismatch
            { client_type: ClientType }
            | e | {
                format_args!("mismatch between client and arguments types, expected: {0:?}",
                    e.client_type)
            },

        InsufficientVotingPower
            { reason: String}
            |e|{
                format_args!("Insufficient overlap {}", e.reason )
            },

        RawClientAndConsensusStateTypesMismatch
            {
                state_type: ClientType,
                consensus_type: ClientType,
            }
            | e | {
                format_args!("mismatch in raw client consensus state {} with expected state {}",
                    e.state_type, e.consensus_type)
            },

        LowHeaderHeight
            {
                header_height: Height,
                latest_height: Height
            }
            | e | {
                format!("received header height ({:?}) is lower than (or equal to) client latest height ({:?})",
                    e.header_height, e.latest_height)
            },

        LowUpgradeHeight
            {
                upgraded_height: Height,
                client_height: Height,
            }
            | e | {
                format_args!("upgraded client height {} must be at greater than current client height {}",
                    e.upgraded_height, e.client_height)
            },

            InvalidConsensusStateTimestamp
            {
                time1: Timestamp,
                time2: Timestamp,
            }
            | e |{
                format_args!("Timestamp none or {} and now {}", e.time1, e.time2)
            },



            HeaderNotWithinTrustPeriod
            {
                latest_time:Timestamp,
                update_time: Timestamp,
            }
            | e | {
                format_args!("Header not withing trusting period: expires_at={0} now={1}",e.latest_time, e.update_time)
            },

            TendermintHandlerError
            { err: Ics07Error }
            | e | { format_args!("Tendermint-specific handler error: {}",e.err) },

    }
}

impl From<Ics07Error> for Error {
    fn from(e: Ics07Error) -> Error {
        Error::tendermint_handler_error(e)
    }
}

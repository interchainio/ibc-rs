use crate::ics24_host::error::ValidationError;
use crate::Height;
use flex_error::{define_error, DisplayOnly, TraceError};
use tendermint::account::Id;
use tendermint::hash::Hash;

define_error! {
    Error {
        InvalidTrustingPeriod
            { reason: String }
            | _ | { "invalid trusting period" },

        InvalidUnboundingPeriod
            { reason: String }
            | _ | { "invalid unbonding period" },

        InvalidAddress
            | _ | { "invalid address" },

        InvalidHeader
            { reason: String }
            [ DisplayOnly<Box<dyn std::error::Error + Send + Sync>> ]
            | _ | { "invalid header, failed basic validation" },

        InvalidTrustThreshold
            { reason: String }
            | e | {
                format_args!("invalid client state trust threshold: {}",
                    e.reason)
            },

        MissingSignedHeader
            | _ | { "missing signed header" },

        Validation
            { reason: String }
            | _ | { "invalid header, failed basic validation" },

        InvalidRawClientState
            { reason: String }
            | _ | { "invalid raw client state" },

        MissingValidatorSet
            | _ | { "missing validator set" },

        MissingTrustedValidatorSet
            | _ | { "missing trusted validator set" },

        MissingTrustedHeight
            | _ | { "missing trusted height" },

        MissingTrustingPeriod
            | _ | { "missing trusting period" },

        MissingUnbondingPeriod
            | _ | { "missing unbonding period" },

        InvalidChainIdentifier
            [ ValidationError ]
            | _ | { "Invalid chain identifier" },

        NegativeTrustingPeriod
            | _ | { "negative trusting period" },

        NegativeUnbondingPeriod
            | _ | { "negative unbonding period" },

        MissingMaxClockDrift
            | _ | { "missing max clock drift" },

        NegativeMaxClockDrift
            | _ | {  "negative max clock drift" },

        MissingLatestHeight
            | _ | { "missing latest height" },

        MissingFrozenHeight
            | _ | { "missing frozen height" },

        InvalidChainId
            { raw_value: String }
            [ ValidationError ]
            | e | { format_args!("invalid chain identifier: raw value {0}", e.raw_value) },

        InvalidRawHeight
            | _ | { "invalid raw height" },

        InvalidRawConsensusState
            { reason: String }
            | _ | { "invalid raw client consensus state" },

        InvalidRawHeader
            [ DisplayOnly<Box<dyn std::error::Error + Send + Sync>> ]
            | _ | { "invalid raw header" },

        InvalidRawMisbehaviour
            { reason: String }
            | _ | { "invalid raw misbehaviour" },

        Decode
            [ TraceError<prost::DecodeError> ]
            | _ | { "decode error" },

        InsufficientVotingPower
            { reason: String}
            |e|{
                format_args!("Insufficient overlap {}", e.reason )
            },

        LowUpdateTimestamp
        {
            low:String,
            high: String
        }
            |e|{
                format_args!("Hearder timestamp {0} must be at
                greater than current client consensus state timestamp {1}",e.low,e.high)
            },

        HeaderTimestampOutsideTrustingTime
            {
                low:String,
                high: String
            }
            |e|{
                format_args!("Header timestamp {0} is outside the trusting period w.r.t. consenus state timestamp {1}",e.low,e.high)
            },


        InvalidHeaderHeight
            {height: Height}
            |e|{
                format_args!("Header height = {0} is invalid",e.height)
            },

        InvalidTrustedHeaderHeight
        {trusted_header_height: Height,
         height_header: Height}
        |e|{
            format_args!("The header height is {0} and it is lower than the trusted header height, which is {1} ",e.height_header, e.trusted_header_height)
        },

        LowUpdateHeight
            {
                low: Height,
                high: Height
            }|e|{
                format_args!("The header height is {0} and it must be at greater than the current client height which is {1}", e.low, e.high)
            },

        MismatchedRevisions
        {
            current_revision: u64,
            update_revision: u64,
        }
        | e | {
            format_args!("The header's revision height, which is {0}, and the update's revision height, which is {1}, should coincide ", e.current_revision, e.update_revision)
        },

        InvalidValidatorSet
        {
              hash1: Hash,
              hash2: Hash,
          }
          | e | {
              format_args!("Invalid validator set: header_validators_hash={} and validators_hash={}", e.hash1, e.hash2)
          },

        NotEnoughTrustedValsSigned
        {reason: String}
        | e |{
            format_args!("Not enough trust because insufficient validators overlap: {}", e.reason)
        },

    }
}

define_error! {
     VerificationError {

        InvalidSignature
        | _ | { "Couldn't verify validator signature " },


        DuplicateValidator
        { id: Id}
        |e| {
            format_args!{"Duplicate validator in commit signatures with address {}", e.id}
        },

        InsufficientOverlap
        {q1: u64,  q2: u64}
        |e| {
            format_args!("Insufficient signers overlap between {0} and {1}", e.q1, e.q2)
        }
}
}

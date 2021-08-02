use std::convert::{TryFrom, TryInto};

use bytes::Buf;
use prost::Message;
use serde_derive::{Deserialize, Serialize};
use tendermint::block::signed_header::SignedHeader;
use tendermint::validator::Set as ValidatorSet;
use tendermint::Time;
use tendermint_proto::Protobuf;

use crate::ics07_tendermint::error::VerificationError;
use crate::timestamp::Timestamp;
use tendermint::block::{Commit, CommitSig};
use tendermint::trust_threshold::TrustThreshold;
use tendermint::trust_threshold::TrustThresholdFraction;
use tendermint::vote::{SignedVote, ValidatorIndex, Vote};

use ibc_proto::ibc::lightclients::tendermint::v1::Header as RawHeader;

use crate::ics02_client::client_type::ClientType;
use crate::ics02_client::header::AnyHeader;
use crate::ics07_tendermint::client_state::ClientState;
use crate::ics07_tendermint::consensus_state::ConsensusState;
use crate::ics07_tendermint::error::Error;
use crate::ics24_host::identifier::ChainId;
use crate::Height;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::ops::Sub;

/// Tendermint consensus header
#[derive(Clone, PartialEq, Deserialize, Serialize)] // TODO: Add Eq bound once present in tendermint-rs
pub struct Header {
    pub signed_header: SignedHeader, // contains the commitment root
    pub validator_set: ValidatorSet, // the validator set that signed Header
    pub trusted_height: Height, // the height of a trusted header seen by client less than or equal to Header
    pub trusted_validator_set: ValidatorSet, // the last trusted validator set at trusted height
}

impl std::fmt::Debug for Header {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, " Header {{...}}")
    }
}

impl Header {
    pub fn height(&self) -> Height {
        Height::new(
            ChainId::chain_version(self.signed_header.header.chain_id.as_str()),
            u64::from(self.signed_header.header.height),
        )
    }

    pub fn time(&self) -> Time {
        self.signed_header.header.time
    }

    pub fn compatible_with(&self, other_header: &Header) -> bool {
        headers_compatible(&self.signed_header, &other_header.signed_header)
    }
}

pub fn headers_compatible(header: &SignedHeader, other: &SignedHeader) -> bool {
    let ibc_client_height = other.header.height;
    let self_header_height = header.header.height;

    match self_header_height.cmp(&ibc_client_height) {
        Ordering::Equal => {
            // 1 - fork
            header.commit.block_id == other.commit.block_id
        }
        Ordering::Greater => {
            // 2 - BFT time violation
            header.header.time > other.header.time
        }
        Ordering::Less => {
            // 3 - BFT time violation
            header.header.time < other.header.time
        }
    }
}

pub fn monotonicity_checks(
    latest_consensus_state: ConsensusState,
    header: Header,
    client_state: ClientState,
) -> Result<(), Error> {
    if client_state.latest_height() >= header.height() {
        return Err(Error::low_update_height(
            header.height(),
            client_state.latest_height,
        ));
    }

    if header.height().is_zero() {
        return Err(Error::invalid_header_height(header.height()));
    }

    //check header timestamp is increasing
    if latest_consensus_state.timestamp >= header.signed_header.header().time {
        return Err(Error::header_timestamp_outside_trusting_time(
            header.signed_header.header().time.as_rfc3339(),
            latest_consensus_state.timestamp.as_rfc3339(),
        ));
    };

    // check that the header is not outside the trusting period
    if header
        .signed_header
        .header()
        .time
        .sub(client_state.trusting_period)
        >= latest_consensus_state.timestamp
    {
        return Err(Error::low_update_timestamp(
            header.signed_header.header().time.as_rfc3339(),
            latest_consensus_state.timestamp.as_rfc3339(),
        ));
    };

    // check monotonicity of header height vs trusted height.
    // unclear needed
    if header.trusted_height >= header.height() {
        return Err(Error::invalid_trusted_header_height(
            header.trusted_height,
            header.height(),
        ));
    };

    Ok(())
}

/// Compute the voting power in a header and its commit against a validator set.
///
/// The `trust_threshold` is currently not used, but might be in the future
/// for optimization purposes.
pub fn voting_power_in(
    signed_header: &SignedHeader,
    validator_set: &ValidatorSet,
    trust_threshold: TrustThresholdFraction,
) -> Result<(), Box<dyn std::error::Error>> {
    let signatures = &signed_header.commit.signatures;

    let mut tallied_voting_power = 0_u64;
    let mut seen_validators = HashSet::new();

    // Get non-absent votes from the signatures
    let non_absent_votes = signatures.iter().enumerate().flat_map(|(idx, signature)| {
        non_absent_vote(
            signature,
            ValidatorIndex::try_from(idx).unwrap(),
            &signed_header.commit,
        )
        .map(|vote| (signature, vote))
    });

    let total_voting_power = total_power_of(validator_set);

    for (signature, vote) in non_absent_votes {
        // Ensure we only count a validator's power once
        if seen_validators.contains(&vote.validator_address) {
            return Err(VerificationError::duplicate_validator(vote.validator_address).into());
        } else {
            seen_validators.insert(vote.validator_address);
        }

        let validator = match validator_set.validator(vote.validator_address) {
            Some(validator) => validator,
            None => continue, // Cannot find matching validator, so we skip the vote
        };

        let signed_vote = SignedVote::new(
            vote.clone(),
            signed_header.header.chain_id.clone(),
            vote.validator_address,
            vote.signature,
        );

        let sign_bytes = signed_vote.sign_bytes();
        //     Check vote is valid
        if validator
            .verify_signature(&sign_bytes, signed_vote.signature())
            .is_err()
        {
            //continue;
            return Err(VerificationError::invalid_signature().into());
        }

        // If the vote is neither absent nor nil, tally its power
        if signature.is_commit() {
            tallied_voting_power += validator.power();
            if trust_threshold.is_enough_power(tallied_voting_power, total_voting_power) {
                return Ok(());
            }
        } else {
            // It's OK. We include stray signatures (~votes for nil)
            // to measure validator availability.
        }
    }

    Err(VerificationError::insufficient_overlap(tallied_voting_power, total_voting_power).into())
}

/// Compute the total voting power in a validator set
fn total_power_of(validator_set: &ValidatorSet) -> u64 {
    validator_set
        .validators()
        .iter()
        .fold(0u64, |total, val_info| total + val_info.power.value())
}

fn non_absent_vote(
    commit_sig: &CommitSig,
    validator_index: ValidatorIndex,
    commit: &Commit,
) -> Option<Vote> {
    let (validator_address, timestamp, signature, block_id) = match commit_sig {
        CommitSig::BlockIdFlagAbsent { .. } => return None,
        CommitSig::BlockIdFlagCommit {
            validator_address,
            timestamp,
            signature,
        } => (
            *validator_address,
            *timestamp,
            signature,
            Some(commit.block_id),
        ),
        CommitSig::BlockIdFlagNil {
            validator_address,
            timestamp,
            signature,
        } => (*validator_address, *timestamp, signature, None),
    };

    Some(Vote {
        vote_type: tendermint::vote::Type::Precommit,
        height: commit.height,
        round: commit.round,
        block_id,
        timestamp: Some(timestamp),
        validator_address,
        validator_index,
        signature: *signature,
    })
}

impl crate::ics02_client::header::Header for Header {
    fn client_type(&self) -> ClientType {
        ClientType::Tendermint
    }

    fn height(&self) -> Height {
        self.height()
    }

    fn timestamp(&self) -> Timestamp {
        self.time().into()
    }

    fn wrap_any(self) -> AnyHeader {
        AnyHeader::Tendermint(self)
    }
}

impl Protobuf<RawHeader> for Header {}

impl TryFrom<RawHeader> for Header {
    type Error = Error;

    fn try_from(raw: RawHeader) -> Result<Self, Self::Error> {
        Ok(Self {
            signed_header: raw
                .signed_header
                .ok_or_else(Error::missing_signed_header)?
                .try_into()
                .map_err(|e| Error::invalid_header("signed header conversion".to_string(), e))?,
            validator_set: raw
                .validator_set
                .ok_or_else(Error::missing_validator_set)?
                .try_into()
                .map_err(Error::invalid_raw_header)?,
            trusted_height: raw
                .trusted_height
                .ok_or_else(Error::missing_trusted_height)?
                .into(),
            trusted_validator_set: raw
                .trusted_validators
                .ok_or_else(Error::missing_trusted_validator_set)?
                .try_into()
                .map_err(Error::invalid_raw_header)?,
        })
    }
}

pub fn decode_header<B: Buf>(buf: B) -> Result<Header, Error> {
    RawHeader::decode(buf).map_err(Error::decode)?.try_into()
}

impl From<Header> for RawHeader {
    fn from(value: Header) -> Self {
        RawHeader {
            signed_header: Some(value.signed_header.into()),
            validator_set: Some(value.validator_set.into()),
            trusted_height: Some(value.trusted_height.into()),
            trusted_validators: Some(value.trusted_validator_set.into()),
        }
    }
}

#[cfg(test)]
pub mod test_util {
    use std::convert::TryInto;

    use subtle_encoding::hex;
    use tendermint::block::signed_header::SignedHeader;
    use tendermint::validator::Info as ValidatorInfo;
    use tendermint::validator::Set as ValidatorSet;
    use tendermint::PublicKey;

    use crate::ics07_tendermint::header::Header;
    use crate::Height;

    pub fn get_dummy_tendermint_header() -> tendermint::block::Header {
        serde_json::from_str::<SignedHeader>(include_str!("../../tests/support/signed_header.json"))
            .unwrap()
            .header
    }

    // TODO: This should be replaced with a ::default() or ::produce().
    // The implementation of this function comprises duplicate code (code borrowed from
    // `tendermint-rs` for assembling a Header).
    // See https://github.com/informalsystems/tendermint-rs/issues/381.
    //
    // The normal flow is:
    // - get the (trusted) signed header and the `trusted_validator_set` at a `trusted_height`
    // - get the `signed_header` and the `validator_set` at latest height
    // - build the ics07 Header
    // For testing purposes this function does:
    // - get the `signed_header` from a .json file
    // - create the `validator_set` with a single validator that is also the proposer
    // - assume a `trusted_height` of 1 and no change in the validator set since height 1,
    //   i.e. `trusted_validator_set` = `validator_set`
    pub fn get_dummy_ics07_header() -> Header {
        // Build a SignedHeader from a JSON file.
        let shdr = serde_json::from_str::<SignedHeader>(include_str!(
            "../../tests/support/signed_header.json"
        ))
        .unwrap();

        // Build a set of validators.
        // Below are test values inspired form `test_validator_set()` in tendermint-rs.
        let v1: ValidatorInfo = ValidatorInfo::new(
            PublicKey::from_raw_ed25519(
                &hex::decode_upper(
                    "F349539C7E5EF7C49549B09C4BFC2335318AB0FE51FBFAA2433B4F13E816F4A7",
                )
                .unwrap(),
            )
            .unwrap(),
            281_815_u64.try_into().unwrap(),
        );

        let vs = ValidatorSet::new(vec![v1.clone()], Some(v1));

        Header {
            signed_header: shdr,
            validator_set: vs.clone(),
            trusted_height: Height::new(0, 1),
            trusted_validator_set: vs,
        }
    }
}

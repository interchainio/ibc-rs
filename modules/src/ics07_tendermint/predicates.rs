use crate::ics02_client::error::Kind;
use crate::ics07_tendermint::client_state::ClientState;
use crate::ics07_tendermint::consensus_state::ConsensusState;
use crate::ics07_tendermint::error::VerificationError;
use crate::ics07_tendermint::header::Header;
use std::collections::HashSet;
use std::convert::TryFrom;
use std::ops::Sub;
use tendermint::block::signed_header::SignedHeader;
use tendermint::block::{Commit, CommitSig};
use tendermint::trust_threshold::TrustThreshold;
use tendermint::trust_threshold::TrustThresholdFraction;
use tendermint::validator::Set as ValidatorSet;
use tendermint::vote::{SignedVote, ValidatorIndex, Vote};

#[derive(Copy, Clone, Debug, Default)]
pub struct Predicates;

impl Predicates {
    pub fn monotonicity_checks(
        &self,
        latest_consensus_state: ConsensusState,
        header: Header,
        client_state: ClientState,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if client_state.latest_height() >= header.height() {
            return Err(Kind::LowUpdateHeight(header.height(), client_state.latest_height).into());
        }

        if header.height().is_zero() {
            return Err(Kind::InvalidHeaderHeight(header.height()).into());
        }

        //check header timestamp is increasing
        if latest_consensus_state.timestamp >= header.signed_header.header().time {
            return Err(Kind::HeaderTimestampOutsideTrustingTime(
                header.signed_header.header().time.as_rfc3339(),
                latest_consensus_state.timestamp.as_rfc3339(),
            )
            .into());
        };

        // check that the header is not outside the trusting period
        if header
            .signed_header
            .header()
            .time
            .sub(client_state.trusting_period)
            >= latest_consensus_state.timestamp
        {
            return Err(Kind::LowUpdateTimestamp(
                header.signed_header.header().time.as_rfc3339(),
                latest_consensus_state.timestamp.as_rfc3339(),
            )
            .into());
        };

        // check monotonicity of header height vs trusted height.
        // unclear needed
        if header.trusted_height >= header.height() {
            return Err(format!(
                "non monotonic height update w.r.t trusted header {}, {:?}",
                header.trusted_height,
                header.height()
            )
            .into());
        };

        Ok(())
    }

    /// Compute the voting power in a header and its commit against a validator set.
    ///
    /// The `trust_threshold` is currently not used, but might be in the future
    /// for optimization purposes.
    pub fn voting_power_in(
        &self,
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

        let total_voting_power = self.total_power_of(validator_set);

        for (signature, vote) in non_absent_votes {
            // Ensure we only count a validator's power once
            if seen_validators.contains(&vote.validator_address) {
                return Err(VerificationError::DuplicateValidator(vote.validator_address).into());
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
                return Err((VerificationError::InvalidSignature {
                    signature: signed_vote.signature().to_bytes(),
                    validator: Box::new(validator),
                    sign_bytes,
                })
                .into());
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

        Err(VerificationError::InsufficientOverlap(tallied_voting_power, total_voting_power).into())
    }

    /// Compute the total voting power in a validator set
    fn total_power_of(&self, validator_set: &ValidatorSet) -> u64 {
        validator_set
            .validators()
            .iter()
            .fold(0u64, |total, val_info| total + val_info.power.value())
    }
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

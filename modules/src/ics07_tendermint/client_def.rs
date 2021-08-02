use ibc_proto::ibc::core::commitment::v1::MerkleProof;

use crate::ics02_client::client_consensus::AnyConsensusState;
use crate::ics02_client::client_def::ClientDef;
use crate::ics02_client::client_state::AnyClientState;
use crate::ics02_client::client_type::ClientType;
use crate::ics02_client::context::ClientReader;
use crate::ics02_client::error::Error as Ics02Error;
use crate::ics07_tendermint::error::Error;
//use crate::ics07_tendermint::error::VerificationError;
use crate::ics03_connection::connection::ConnectionEnd;
use crate::ics04_channel::channel::ChannelEnd;
use crate::ics04_channel::packet::Sequence;
use crate::ics07_tendermint::client_state::ClientState;
use crate::ics07_tendermint::consensus_state::ConsensusState;
use crate::ics07_tendermint::header::Header;
use crate::ics07_tendermint::header::{monotonicity_checks, voting_power_in};

use crate::ics23_commitment::commitment::{CommitmentPrefix, CommitmentProofBytes, CommitmentRoot};
use crate::ics24_host::identifier::ConnectionId;
use crate::ics24_host::identifier::{ChannelId, ClientId, PortId};
use crate::Height;
use tendermint::trust_threshold::TrustThresholdFraction;
use tendermint::validator::Set;

//Box<dyn std::error::Error>
use crate::downcast;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TendermintClient;

impl ClientDef for TendermintClient {
    type Header = Header;
    type ClientState = ClientState;
    type ConsensusState = ConsensusState;

    fn check_header_and_update_state(
        &self,
        ctx: &dyn ClientReader,
        client_id: ClientId,
        client_state: Self::ClientState,
        header: Self::Header,
    ) -> Result<(Self::ClientState, Self::ConsensusState), Ics02Error> {
        // check if a consensus state is already installed; if so it should
        // match the untrusted header.

        if let Some(cs) = ctx.consensus_state(&client_id, header.height()) {
            //could the header height be zero ?
            let consensus_state = downcast!(
                cs => AnyConsensusState::Tendermint
            )
            .ok_or_else(|| Ics02Error::client_args_type_mismatch(ClientType::Tendermint))?;

            if consensus_state != ConsensusState::from(header.clone()) {
                //freeze the client and return the installed consensus state
                return Ok((
                    client_state.with_set_frozen(header.height()),
                    consensus_state,
                ));
            } else {
                return Ok((client_state, consensus_state));
            }
        };

        let latest_consensus_state =
            match ctx.consensus_state(&client_id, client_state.latest_height) {
                //could the header height be zero ?
                Some(cs) => downcast!(
                    cs => AnyConsensusState::Tendermint
                )
                .ok_or_else(|| Ics02Error::client_args_type_mismatch(ClientType::Tendermint))?,
                None => {
                    return Err(Ics02Error::consensus_state_not_found(
                        client_id.clone(),
                        client_state.latest_height,
                    ));
                }
            };

        monotonicity_checks(latest_consensus_state, header.clone(), client_state.clone())
            .map_err(Ics02Error::tendermint_handler_error)?;

        // check that the versions of the client state and the header match
        if client_state.latest_height.revision_number != header.height().revision_number {
            return Err(Error::mismatched_revisions(
                client_state.latest_height.revision_number,
                header.height().revision_number,
            )
            .into());
        };

        let trusted_consensus_state = match ctx.consensus_state(&client_id, header.trusted_height) {
            Some(ts) => downcast!(
                ts => AnyConsensusState::Tendermint
            )
            .ok_or_else(|| Ics02Error::client_args_type_mismatch(ClientType::Tendermint))?,
            None => {
                return Err(Ics02Error::consensus_state_not_found(
                    client_id,
                    header.trusted_height,
                ))
            }
        };

        if header.height() == header.trusted_height.increment() {
            //adjacent

            // check that the header's trusted validator set is
            // the next_validator_set of the trusted consensus state
            if Set::hash(&header.validator_set) != trusted_consensus_state.next_validators_hash {
                return Err(Error::invalid_validator_set(
                    trusted_consensus_state.next_validators_hash,
                    Set::hash(&header.validator_set),
                )
                .into());
            }

            // check that the validators that sign the commit of the untrusted header
            // have 2/3 of the voting power of the current validator set.
            if let Err(e) = voting_power_in(
                &header.signed_header,
                &header.validator_set,
                TrustThresholdFraction::TWO_THIRDS,
            ) {
                return Err(Error::insufficient_voting_power(e.to_string()).into());
            }
        } else {
            //Non-adjacent

            //check that a subset of the trusted validator set, having 1/3 of the voting power
            //signes the commit of the untrusted header
            if let Err(e) = voting_power_in(
                &header.signed_header,
                &header.trusted_validator_set,
                TrustThresholdFraction::default(),
            ) {
                return Err(Error::not_enough_trusted_vals_signed(e.to_string()).into());
            };

            // check that the validators that sign the commit of the untrusted header
            // have 2/3 of the voting power of the current validator set.
            if let Err(e) = voting_power_in(
                &header.signed_header,
                &header.validator_set,
                TrustThresholdFraction::TWO_THIRDS,
            ) {
                return Err(Error::insufficient_voting_power(e.to_string()).into());
            };
        }

        Ok((
            client_state.with_header(header.clone()),
            ConsensusState::from(header),
        ))
    }

    fn verify_client_consensus_state(
        &self,
        _client_state: &Self::ClientState,
        _height: Height,
        _prefix: &CommitmentPrefix,
        _proof: &CommitmentProofBytes,
        _client_id: &ClientId,
        _consensus_height: Height,
        _expected_consensus_state: &AnyConsensusState,
    ) -> Result<(), Ics02Error> {
        todo!()
    }

    fn verify_connection_state(
        &self,
        _client_state: &Self::ClientState,
        _height: Height,
        _prefix: &CommitmentPrefix,
        _proof: &CommitmentProofBytes,
        _connection_id: Option<&ConnectionId>,
        _expected_connection_end: &ConnectionEnd,
    ) -> Result<(), Ics02Error> {
        todo!()
    }

    fn verify_channel_state(
        &self,
        _client_state: &Self::ClientState,
        _height: Height,
        _prefix: &CommitmentPrefix,
        _proof: &CommitmentProofBytes,
        _port_id: &PortId,
        _channel_id: &ChannelId,
        _expected_channel_end: &ChannelEnd,
    ) -> Result<(), Ics02Error> {
        todo!()
    }

    fn verify_client_full_state(
        &self,
        _client_state: &Self::ClientState,
        _height: Height,
        _root: &CommitmentRoot,
        _prefix: &CommitmentPrefix,
        _client_id: &ClientId,
        _proof: &CommitmentProofBytes,
        _expected_client_state: &AnyClientState,
    ) -> Result<(), Ics02Error> {
        unimplemented!()
    }

    fn verify_packet_data(
        &self,
        _client_state: &Self::ClientState,
        _height: Height,
        _proof: &CommitmentProofBytes,
        _port_id: &PortId,
        _channel_id: &ChannelId,
        _seq: &Sequence,
        _data: String,
    ) -> Result<(), Ics02Error> {
        todo!()
    }

    fn verify_packet_acknowledgement(
        &self,
        _client_state: &Self::ClientState,
        _height: Height,
        _proof: &CommitmentProofBytes,
        _port_id: &PortId,
        _channel_id: &ChannelId,
        _seq: &Sequence,
        _data: Vec<u8>,
    ) -> Result<(), Ics02Error> {
        todo!()
    }

    fn verify_next_sequence_recv(
        &self,
        _client_state: &Self::ClientState,
        _height: Height,
        _proof: &CommitmentProofBytes,
        _port_id: &PortId,
        _channel_id: &ChannelId,
        _seq: &Sequence,
    ) -> Result<(), Ics02Error> {
        todo!()
    }

    fn verify_packet_receipt_absence(
        &self,
        _client_state: &Self::ClientState,
        _height: Height,
        _proof: &CommitmentProofBytes,
        _port_id: &PortId,
        _channel_id: &ChannelId,
        _seq: &Sequence,
    ) -> Result<(), Ics02Error> {
        todo!()
    }

    fn verify_upgrade_and_update_state(
        &self,
        _client_state: &Self::ClientState,
        _consensus_state: &Self::ConsensusState,
        _proof_upgrade_client: MerkleProof,
        _proof_upgrade_consensus_state: MerkleProof,
    ) -> Result<(Self::ClientState, Self::ConsensusState), Ics02Error> {
        todo!()
    }
}

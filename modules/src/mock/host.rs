//! Host chain types and methods, used by context mock.

use crate::prelude::*;
use core::convert::TryFrom;

use tendermint::chain::Id as TMChainId;
use tendermint_testgen::light_block::TmLightBlock;
use tendermint_testgen::{Generator, LightBlock as TestgenLightBlock};

use crate::ics02_client::client_consensus::AnyConsensusState;
use crate::ics02_client::header::AnyHeader;
use crate::ics07_tendermint::consensus_state::ConsensusState as TMConsensusState;
use crate::ics07_tendermint::header::Header as TMHeader;
use crate::ics24_host::identifier::ChainId;
use crate::mock::header::MockHeader;
use crate::timestamp::Timestamp;
use crate::Height;

/// Defines the different types of host chains that a mock context can emulate.
/// The variants are as follows:
/// - `Mock` defines that the context history consists of `MockHeader` blocks.
/// - `SyntheticTendermint`: the context has synthetically-generated Tendermint (light) blocks.
/// See also the `HostBlock` enum to get more insights into the underlying block type.
#[derive(Clone, Debug, Copy)]
pub enum HostType {
    Mock,
    SyntheticTendermint,
}

/// Depending on `HostType` (the type of host chain underlying a context mock), this enum defines
/// the type of blocks composing the history of the host chain.
#[derive(Clone, Debug)]
pub enum HostBlock {
    Mock(MockHeader),
    SyntheticTendermint(Box<TmLightBlock>),
}

impl HostBlock {
    /// Returns the height of a block.
    pub fn height(&self) -> Height {
        match self {
            HostBlock::Mock(header) => header.height(),
            HostBlock::SyntheticTendermint(light_block) => Height::new(
                ChainId::chain_version(light_block.signed_header.header.chain_id.as_str()),
                light_block.signed_header.header.height.value(),
            ),
        }
    }

    /// Generates a new block at `height` for the given chain identifier and chain type.
    pub fn generate_block(chain_id: ChainId, chain_type: HostType, height: u64) -> HostBlock {
        match chain_type {
            HostType::Mock => HostBlock::Mock(MockHeader {
                height: Height::new(chain_id.version(), height),
                timestamp: Timestamp::from_nanoseconds(1).unwrap(),
            }),
            HostType::SyntheticTendermint => {
                HostBlock::SyntheticTendermint(Box::new(Self::generate_tm_block(chain_id, height)))
            }
        }
    }

    pub fn generate_tm_block(chain_id: ChainId, height: u64) -> TmLightBlock {
        let mut block = TestgenLightBlock::new_default(height).generate().unwrap();
        block.signed_header.header.chain_id = TMChainId::try_from(chain_id.to_string()).unwrap();

        block
    }
}

impl From<TmLightBlock> for AnyConsensusState {
    fn from(light_block: TmLightBlock) -> Self {
        let cs = TMConsensusState::from(light_block.signed_header.header);
        AnyConsensusState::Tendermint(cs)
    }
}

impl From<HostBlock> for AnyConsensusState {
    fn from(any_block: HostBlock) -> Self {
        match any_block {
            HostBlock::Mock(mock_header) => mock_header.into(),
            HostBlock::SyntheticTendermint(light_block) => (*light_block).into(),
        }
    }
}

impl From<HostBlock> for AnyHeader {
    fn from(any_block: HostBlock) -> Self {
        match any_block {
            HostBlock::Mock(mock_header) => mock_header.into(),
            HostBlock::SyntheticTendermint(light_block_box) => {
                // Conversion from TMLightBlock to AnyHeader
                AnyHeader::Tendermint((*light_block_box).into())
            }
        }
    }
}

impl From<TmLightBlock> for TMHeader {
    fn from(light_block: TmLightBlock) -> Self {
        // TODO: This conversion is incorrect for `trusted_height` and `trusted_validator_set`.
        TMHeader {
            signed_header: light_block.signed_header,
            validator_set: light_block.validators,
            trusted_height: Default::default(),
            trusted_validator_set: light_block.next_validators,
        }
    }
}

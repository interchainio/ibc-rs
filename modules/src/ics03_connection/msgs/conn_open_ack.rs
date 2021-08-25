use crate::prelude::*;
use core::convert::{TryFrom, TryInto};
use ibc_proto::ibc::core::connection::v1::MsgConnectionOpenAck as RawMsgConnectionOpenAck;
use tendermint_proto::Protobuf;

use crate::ics02_client::client_state::AnyClientState;
use crate::ics03_connection::error::Error;
use crate::ics03_connection::version::Version;
use crate::ics23_commitment::commitment::CommitmentProofBytes;
use crate::ics24_host::identifier::ConnectionId;
use crate::proofs::{ConsensusProof, Proofs};
use crate::signer::Signer;
use crate::tx_msg::Msg;
use crate::Height;

pub const TYPE_URL: &str = "/ibc.core.connection.v1.MsgConnectionOpenAck";

/// Message definition `MsgConnectionOpenAck`  (i.e., `ConnOpenAck` datagram).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MsgConnectionOpenAck {
    pub connection_id: ConnectionId,
    pub counterparty_connection_id: ConnectionId,
    pub client_state: Option<AnyClientState>,
    pub proofs: Proofs,
    pub version: Version,
    pub signer: Signer,
}

impl MsgConnectionOpenAck {
    /// Getter for accessing the connection identifier of this message.
    pub fn connection_id(&self) -> &ConnectionId {
        &self.connection_id
    }

    /// Getter for accessing the counterparty's connection identifier from this message.
    pub fn counterparty_connection_id(&self) -> &ConnectionId {
        &self.counterparty_connection_id
    }

    /// Getter for accessing the client state.
    pub fn client_state(&self) -> Option<AnyClientState> {
        self.client_state.clone()
    }

    /// Getter for accessing (borrow) the proofs in this message.
    pub fn proofs(&self) -> &Proofs {
        &self.proofs
    }

    /// Getter for the version field.
    pub fn version(&self) -> &Version {
        &self.version
    }

    /// Getter for accessing the `consensus_height` field from this message. Returns the special
    /// value `Height(0)` if this field is not set.
    pub fn consensus_height(&self) -> Height {
        match self.proofs.consensus_proof() {
            None => Height::zero(),
            Some(p) => p.height(),
        }
    }
}

impl Msg for MsgConnectionOpenAck {
    type ValidationError = Error;
    type Raw = RawMsgConnectionOpenAck;

    fn route(&self) -> String {
        crate::keys::ROUTER_KEY.to_string()
    }

    fn type_url(&self) -> String {
        TYPE_URL.to_string()
    }
}

impl Protobuf<RawMsgConnectionOpenAck> for MsgConnectionOpenAck {}

impl TryFrom<RawMsgConnectionOpenAck> for MsgConnectionOpenAck {
    type Error = Error;

    fn try_from(msg: RawMsgConnectionOpenAck) -> Result<Self, Self::Error> {
        let consensus_height = msg
            .consensus_height
            .ok_or_else(Error::missing_consensus_height)?
            .into();
        let consensus_proof_obj = ConsensusProof::new(msg.proof_consensus.into(), consensus_height)
            .map_err(Error::invalid_proof)?;

        let proof_height = msg
            .proof_height
            .ok_or_else(Error::missing_proof_height)?
            .into();

        let client_proof = Some(msg.proof_client)
            .filter(|x| !x.is_empty())
            .map(CommitmentProofBytes::from);

        Ok(Self {
            connection_id: msg
                .connection_id
                .parse()
                .map_err(Error::invalid_identifier)?,
            counterparty_connection_id: msg
                .counterparty_connection_id
                .parse()
                .map_err(Error::invalid_identifier)?,
            client_state: msg
                .client_state
                .map(AnyClientState::try_from)
                .transpose()
                .map_err(Error::ics02_client)?,
            version: msg.version.ok_or_else(Error::empty_versions)?.try_into()?,
            proofs: Proofs::new(
                msg.proof_try.into(),
                client_proof,
                Option::from(consensus_proof_obj),
                None,
                proof_height,
            )
            .map_err(Error::invalid_proof)?,
            signer: msg.signer.into(),
        })
    }
}

impl From<MsgConnectionOpenAck> for RawMsgConnectionOpenAck {
    fn from(ics_msg: MsgConnectionOpenAck) -> Self {
        RawMsgConnectionOpenAck {
            connection_id: ics_msg.connection_id.as_str().to_string(),
            counterparty_connection_id: ics_msg.counterparty_connection_id.as_str().to_string(),
            client_state: ics_msg
                .client_state
                .map_or_else(|| None, |v| Some(v.into())),
            proof_height: Some(ics_msg.proofs.height().into()),
            proof_try: ics_msg.proofs.object_proof().clone().into(),
            proof_client: ics_msg
                .proofs
                .client_proof()
                .clone()
                .map_or_else(Vec::new, |v| v.into()),
            proof_consensus: ics_msg
                .proofs
                .consensus_proof()
                .map_or_else(Vec::new, |v| v.proof().clone().into()),
            consensus_height: ics_msg
                .proofs
                .consensus_proof()
                .map_or_else(|| None, |h| Some(h.height().into())),
            version: Some(ics_msg.version.into()),
            signer: ics_msg.signer.to_string(),
        }
    }
}

#[cfg(test)]
pub mod test_util {
    use crate::prelude::*;
    use ibc_proto::ibc::core::client::v1::Height;
    use ibc_proto::ibc::core::connection::v1::MsgConnectionOpenAck as RawMsgConnectionOpenAck;

    use crate::ics03_connection::version::Version;
    use crate::ics24_host::identifier::ConnectionId;
    use crate::test_utils::{get_dummy_bech32_account, get_dummy_proof};

    pub fn get_dummy_raw_msg_conn_open_ack(
        proof_height: u64,
        consensus_height: u64,
    ) -> RawMsgConnectionOpenAck {
        RawMsgConnectionOpenAck {
            connection_id: ConnectionId::default().to_string(),
            counterparty_connection_id: ConnectionId::default().to_string(),
            proof_try: get_dummy_proof(),
            proof_height: Some(Height {
                revision_number: 0,
                revision_height: proof_height,
            }),
            proof_consensus: get_dummy_proof(),
            consensus_height: Some(Height {
                revision_number: 0,
                revision_height: consensus_height,
            }),
            client_state: None,
            proof_client: Vec::new(),
            version: Some(Version::default().into()),
            signer: get_dummy_bech32_account(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::prelude::*;
    use core::convert::TryFrom;
    use test_env_log::test;

    use ibc_proto::ibc::core::client::v1::Height;
    use ibc_proto::ibc::core::connection::v1::MsgConnectionOpenAck as RawMsgConnectionOpenAck;

    use crate::ics03_connection::msgs::conn_open_ack::test_util::get_dummy_raw_msg_conn_open_ack;
    use crate::ics03_connection::msgs::conn_open_ack::MsgConnectionOpenAck;

    #[test]
    fn parse_connection_open_ack_msg() {
        #[derive(Clone, Debug, PartialEq)]
        struct Test {
            name: String,
            raw: RawMsgConnectionOpenAck,
            want_pass: bool,
        }

        let default_ack_msg = get_dummy_raw_msg_conn_open_ack(5, 5);

        let tests: Vec<Test> = vec![
            Test {
                name: "Good parameters".to_string(),
                raw: default_ack_msg.clone(),
                want_pass: true,
            },
            Test {
                name: "Bad connection id, non-alpha".to_string(),
                raw: RawMsgConnectionOpenAck {
                    connection_id: "con007".to_string(),
                    ..default_ack_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Bad version, missing version".to_string(),
                raw: RawMsgConnectionOpenAck {
                    version: None,
                    ..default_ack_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Bad proof height, height is 0".to_string(),
                raw: RawMsgConnectionOpenAck {
                    proof_height: Some(Height {
                        revision_number: 1,
                        revision_height: 0,
                    }),
                    ..default_ack_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Bad consensus height, height is 0".to_string(),
                raw: RawMsgConnectionOpenAck {
                    consensus_height: Some(Height {
                        revision_number: 1,
                        revision_height: 0,
                    }),
                    ..default_ack_msg
                },
                want_pass: false,
            },
        ]
        .into_iter()
        .collect();

        for test in tests {
            let msg = MsgConnectionOpenAck::try_from(test.raw.clone());

            assert_eq!(
                test.want_pass,
                msg.is_ok(),
                "MsgConnOpenAck::new failed for test {}, \nmsg {:?} with error {:?}",
                test.name,
                test.raw,
                msg.err(),
            );
        }
    }

    #[test]
    fn to_and_from() {
        let raw = get_dummy_raw_msg_conn_open_ack(5, 6);
        let msg = MsgConnectionOpenAck::try_from(raw.clone()).unwrap();
        let raw_back = RawMsgConnectionOpenAck::from(msg.clone());
        let msg_back = MsgConnectionOpenAck::try_from(raw_back.clone()).unwrap();
        assert_eq!(raw, raw_back);
        assert_eq!(msg, msg_back);
    }
}

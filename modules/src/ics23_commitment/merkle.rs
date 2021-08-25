use crate::prelude::*;
use tendermint::merkle::proof::Proof;

use ibc_proto::ibc::core::commitment::v1::MerklePath;
use ibc_proto::ibc::core::commitment::v1::MerkleProof as RawMerkleProof;

use crate::ics23_commitment::commitment::{CommitmentPrefix, CommitmentProofBytes};
use crate::ics23_commitment::error::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmptyPrefixError;

pub fn apply_prefix(
    prefix: &CommitmentPrefix,
    mut path: Vec<String>,
) -> Result<MerklePath, EmptyPrefixError> {
    if prefix.is_empty() {
        return Err(EmptyPrefixError);
    }

    let mut result: Vec<String> = vec![format!("{:?}", prefix)];
    result.append(&mut path);

    Ok(MerklePath { key_path: result })
}

#[derive(Clone, Debug, PartialEq)]
pub struct MerkleProof {
    pub proof: Vec<CommitmentProofBytes>,
}

// Merkle Proof serialization notes:
// "Proof" id currently defined in a number of forms and included in a number of places
// - TmProof: in tendermint-rs/src/merkle/proof.rs:Proof
// - RawProofOps: in tendermint-proto/tendermint.cyrpto.rs:ProofOps
// - RawMerkleProof: in ibc-proto/ibc.core.commitment.v1.rs:MerkleProof
//     - structure that includes a RawProofOps in its only `proof` field.
//         #[derive(Clone, PartialEq, ::prost::Message)]
//         pub struct MerkleProof {
//             #[prost(message, optional, tag="1")]
//             pub proof: ::core::option::Option<::tendermint_proto::crypto::ProofOps>,
//         }
//  - Vec<u8>: RawMerkleProof is not explicitly used but, serialized as Vec<u8>, it is
//       included in all handshake messages that require proofs (i.e. all except the two `OpenInit`),
//       and also in all queries that require proofs
//  - MerkleProof: Domain type for RawMerkleProof, currently not used and identical to RawMerkleProof.
//       This will change with verification implementation.
//  - CommitmentProof: Defined in ibc-rs as Vec<u8> and currently used in all its messages
//
// Here are a couple of flows that illustrate the different conversions:
// IBC Messages and Handlers: sink happens in the handle verification
//    Vec<u8> -> CommitmentProof -> RawMerkleProof -> MerkleProof
//
// Relayer: from the proof in the  query response to the proof being included in a message
//    TmProof -> RawProofOps => RawMerkleProof -> MerkleProof -> verify()
//      -> MerkleProof -> RawMerkleProof -> CommitmentProof -> Vec<u8>
// Note: current implementation for ^ is simplified since verification is not yet implemented:
//    TmProof -> RawProofOps => RawMerkleProof -> CommitmentProof -> Vec<u8>
//
// Implementations of (de)serializers and conversions:
//  - commitment.rs:
//      Vec<u8> <-> CommitmentProof
//      CommitmentProof <-> RawMerkleProof
//  - merkle.rs:
//      RawMerkleProof <-> MerkleProof
//  - tendermint-rs/src/merkle/proof.rs:
//      TmProof <-> RawProofOps
//  - cosmos.rs:abci_query() converts from query proof to Merkle proof:
//      RawProofOps => RawMerkleProof
//
// impl TryFrom<RawMerkleProof> for MerkleProof {
//     type Error = Error;
//     fn try_from(value: RawMerkleProof) -> Result<Self, Self::Error> {
//         Ok(MerkleProof { proof: value.proofs.into_iter().map(|v| v.into()).collect() })
//     }
// }
//
// impl From<MerkleProof> for RawMerkleProof {
//     fn from(value: MerkleProof) -> Self {
//         RawMerkleProof { proof: value.proof }
//     }
// }

pub fn convert_tm_to_ics_merkle_proof(tm_proof: &Proof) -> Result<RawMerkleProof, Error> {
    let mut proofs = Vec::new();

    for op in &tm_proof.ops {
        let mut parsed = ibc_proto::ics23::CommitmentProof { proof: None };
        prost::Message::merge(&mut parsed, op.data.as_slice())
            .map_err(Error::commitment_proof_decoding_failed)?;

        proofs.push(parsed);
    }

    Ok(RawMerkleProof { proofs })
}

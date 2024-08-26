use tendermint::merkle::proof::ProofOps as TendermintProof;

use ibc_proto::ibc::core::commitment::v1::MerkleProof as RawMerkleProof;
use ibc_proto::ibc::core::commitment::v1::MerkleRoot;
use ibc_proto::ibc::core::commitment::v2::MerklePath;
use ics23::commitment_proof::Proof;
use ics23::{
    calculate_existence_root, verify_membership, verify_non_membership, CommitmentProof,
    NonExistenceProof,
};

use crate::core::ics23_commitment::commitment::CommitmentRoot;
use crate::core::ics23_commitment::error::Error;
use crate::core::ics23_commitment::specs::ProofSpecs;

impl From<CommitmentRoot> for MerkleRoot {
    fn from(root: CommitmentRoot) -> Self {
        Self {
            hash: root.into_vec(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MerkleProof {
    pub proofs: Vec<CommitmentProof>,
}

/// Convert to ics23::CommitmentProof
impl From<RawMerkleProof> for MerkleProof {
    fn from(proof: RawMerkleProof) -> Self {
        Self {
            proofs: proof.proofs,
        }
    }
}

impl From<MerkleProof> for RawMerkleProof {
    fn from(proof: MerkleProof) -> Self {
        Self {
            proofs: proof.proofs,
        }
    }
}

impl MerkleProof {
    pub fn verify_membership(
        &self,
        specs: &ProofSpecs,
        root: MerkleRoot,
        keys: MerklePath,
        value: Vec<u8>,
        start_index: usize,
    ) -> Result<(), Error> {
        // validate arguments
        if self.proofs.is_empty() {
            return Err(Error::empty_merkle_proof());
        }
        if root.hash.is_empty() {
            return Err(Error::empty_merkle_root());
        }
        let num = self.proofs.len();
        let ics23_specs = Vec::<ics23::ProofSpec>::from(specs.clone());
        if ics23_specs.len() != num {
            return Err(Error::number_of_specs_mismatch());
        }
        if keys.key_path.len() != num {
            return Err(Error::number_of_keys_mismatch());
        }
        if value.is_empty() {
            return Err(Error::empty_verified_value());
        }

        let mut subroot = value.clone();
        let mut value = value;
        // keys are represented from root-to-leaf
        for ((proof, spec), key) in self
            .proofs
            .iter()
            .zip(ics23_specs.iter())
            .zip(keys.key_path.iter().rev())
            .skip(start_index)
        {
            match &proof.proof {
                Some(Proof::Exist(existence_proof)) => {
                    subroot =
                        calculate_existence_root::<ics23::HostFunctionsManager>(existence_proof)
                            .map_err(|_| Error::invalid_merkle_proof())?;

                    if !verify_membership::<ics23::HostFunctionsManager>(
                        proof, spec, &subroot, key, &value,
                    ) {
                        return Err(Error::verification_failure());
                    }
                    value.clone_from(&subroot);
                }
                _ => return Err(Error::invalid_merkle_proof()),
            }
        }

        if root.hash != subroot {
            return Err(Error::verification_failure());
        }

        Ok(())
    }

    pub fn verify_non_membership(
        &self,
        specs: &ProofSpecs,
        root: MerkleRoot,
        keys: MerklePath,
    ) -> Result<(), Error> {
        // validate arguments
        if self.proofs.is_empty() {
            return Err(Error::empty_merkle_proof());
        }
        if root.hash.is_empty() {
            return Err(Error::empty_merkle_root());
        }
        let num = self.proofs.len();
        let ics23_specs = Vec::<ics23::ProofSpec>::from(specs.clone());
        if ics23_specs.len() != num {
            return Err(Error::number_of_specs_mismatch());
        }
        if keys.key_path.len() != num {
            return Err(Error::number_of_keys_mismatch());
        }

        // verify the absence of key in lowest subtree
        let proof = self
            .proofs
            .first()
            .ok_or_else(Error::invalid_merkle_proof)?;
        let spec = ics23_specs
            .first()
            .ok_or_else(Error::invalid_merkle_proof)?;
        // keys are represented from root-to-leaf
        let key = keys
            .key_path
            .get(num - 1)
            .ok_or_else(Error::invalid_merkle_proof)?;
        match &proof.proof {
            Some(Proof::Nonexist(non_existence_proof)) => {
                let subroot = calculate_non_existence_root(non_existence_proof)?;

                if !verify_non_membership::<ics23::HostFunctionsManager>(proof, spec, &subroot, key)
                {
                    return Err(Error::verification_failure());
                }

                // verify membership proofs starting from index 1 with value = subroot
                self.verify_membership(specs, root, keys, subroot, 1)
            }
            _ => Err(Error::invalid_merkle_proof()),
        }
    }
}

// TODO move to ics23
fn calculate_non_existence_root(proof: &NonExistenceProof) -> Result<Vec<u8>, Error> {
    if let Some(left) = &proof.left {
        calculate_existence_root::<ics23::HostFunctionsManager>(left)
            .map_err(|_| Error::invalid_merkle_proof())
    } else if let Some(right) = &proof.right {
        calculate_existence_root::<ics23::HostFunctionsManager>(right)
            .map_err(|_| Error::invalid_merkle_proof())
    } else {
        Err(Error::invalid_merkle_proof())
    }
}

// Merkle Proof serialization notes:
// "Proof" id currently defined in a number of forms and included in a number of places
// - TmProof: in tendermint-rs/src/merkle/proof.rs:Proof
// - RawProofOps: in tendermint-proto/tendermint.crypto.rs:ProofOps
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

pub fn convert_tm_to_ics_merkle_proof(tm_proof: &TendermintProof) -> Result<MerkleProof, Error> {
    let mut proofs = Vec::new();

    for op in &tm_proof.ops {
        let mut parsed = CommitmentProof { proof: None };

        prost::Message::merge(&mut parsed, op.data.as_slice())
            .map_err(Error::commitment_proof_decoding_failed)?;

        proofs.push(parsed);
    }

    Ok(MerkleProof::from(RawMerkleProof { proofs }))
}

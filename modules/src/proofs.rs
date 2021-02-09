use serde::Serialize;

use crate::ics23_commitment::commitment::CommitmentProofBytes;
use crate::Height;

/// Structure comprising proofs in a message. Proofs are typically present in messages for
/// handshake protocols, e.g., ICS3 connection (open) handshake or ICS4 channel (open and close)
/// handshake, as well as for ICS4 packets, timeouts, and acknowledgements.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Proofs {
    object_proof: CommitmentProofBytes,
    client_proof: Option<CommitmentProofBytes>,
    consensus_proof: Option<ConsensusProof>,
    /// Currently used for proof_close for MsgTimeoutOnCLose where object_proof is proof_unreceived
    pub(crate) other_proof: Option<CommitmentProofBytes>,
    /// Height for the commitment root for proving the proofs above.
    /// When creating these proofs, the chain is queried at `height-1`.
    height: Height,
}

impl Proofs {
    pub fn new(
        object_proof: CommitmentProofBytes,
        client_proof: Option<CommitmentProofBytes>,
        consensus_proof: Option<ConsensusProof>,
        other_proof: Option<CommitmentProofBytes>,
        height: Height,
    ) -> Result<Self, String> {
        if height.is_zero() {
            return Err("Proofs height cannot be zero".to_string());
        }

        if object_proof.is_empty() {
            return Err("Object proof cannot be empty".to_string());
        }

        Ok(Self {
            object_proof,
            client_proof,
            consensus_proof,
            other_proof,
            height,
        })
    }

    /// Getter for the consensus_proof field of this proof. Intuitively, this is a proof that a
    /// client on the source chain stores a consensus state for the destination chain.
    pub fn consensus_proof(&self) -> Option<ConsensusProof> {
        self.consensus_proof.clone()
    }

    /// Getter for the height field of this proof (i.e., the consensus height where this proof was
    /// created).
    pub fn height(&self) -> Height {
        self.height
    }

    /// Getter for the object-specific proof (e.g., proof for connection state or channel state).
    pub fn object_proof(&self) -> &CommitmentProofBytes {
        &self.object_proof
    }

    /// Getter for the client_proof.
    pub fn client_proof(&self) -> &Option<CommitmentProofBytes> {
        &self.client_proof
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ConsensusProof {
    proof: CommitmentProofBytes,
    height: Height,
}

impl ConsensusProof {
    pub fn new(
        consensus_proof: CommitmentProofBytes,
        consensus_height: Height,
    ) -> Result<Self, String> {
        if consensus_height.is_zero() {
            return Err("Consensus height cannot be zero".to_string());
        }
        if consensus_proof.is_empty() {
            return Err("Proof cannot be empty".to_string());
        }

        Ok(Self {
            proof: consensus_proof,
            height: consensus_height,
        })
    }

    /// Getter for the height field of this consensus proof.
    pub fn height(&self) -> Height {
        self.height
    }

    /// Getter for the proof (CommitmentProof) field of this consensus proof.
    pub fn proof(&self) -> &CommitmentProofBytes {
        &self.proof
    }
}

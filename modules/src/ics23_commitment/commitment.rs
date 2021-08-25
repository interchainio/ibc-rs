use crate::ics23_commitment::error::Error;
use crate::prelude::*;

use core::{convert::TryFrom, fmt};
use ibc_proto::ibc::core::commitment::v1::MerkleProof as RawMerkleProof;
use serde::{Deserialize, Serialize};
use subtle_encoding::{Encoding, Hex};

#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct CommitmentRoot {
    #[serde(serialize_with = "crate::serializers::ser_hex_upper")]
    bytes: Vec<u8>,
}

impl fmt::Debug for CommitmentRoot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hex = Hex::upper_case().encode_to_string(&self.bytes).unwrap();
        f.debug_tuple("CommitmentRoot").field(&hex).finish()
    }
}

impl CommitmentRoot {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            bytes: Vec::from(bytes),
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_vec(self) -> Vec<u8> {
        self.bytes
    }
}

impl From<Vec<u8>> for CommitmentRoot {
    fn from(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommitmentPath;

#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct CommitmentProofBytes {
    #[serde(serialize_with = "crate::serializers::ser_hex_upper")]
    bytes: Vec<u8>,
}

impl fmt::Debug for CommitmentProofBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hex = Hex::upper_case().encode_to_string(&self.bytes).unwrap();
        f.debug_tuple("CommitmentProof").field(&hex).finish()
    }
}

impl CommitmentProofBytes {
    pub fn is_empty(&self) -> bool {
        self.bytes.len() == 0
    }
}

impl From<Vec<u8>> for CommitmentProofBytes {
    fn from(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }
}

impl From<CommitmentProofBytes> for Vec<u8> {
    fn from(p: CommitmentProofBytes) -> Vec<u8> {
        p.bytes
    }
}

// impl From<MerkleProof> for CommitmentProofBytes {
//     fn from(proof: MerkleProof) -> Self {
//         let raw_proof: RawMerkleProof = proof.into();
//         raw_proof.into()
//     }
// }

impl From<RawMerkleProof> for CommitmentProofBytes {
    fn from(proof: RawMerkleProof) -> Self {
        let mut buf = Vec::new();
        prost::Message::encode(&proof, &mut buf).unwrap();
        buf.into()
    }
}

impl TryFrom<CommitmentProofBytes> for RawMerkleProof {
    type Error = Error;

    fn try_from(value: CommitmentProofBytes) -> Result<Self, Self::Error> {
        let value: Vec<u8> = value.into();
        let res: RawMerkleProof =
            prost::Message::decode(value.as_ref()).map_err(Error::invalid_raw_merkle_proof)?;
        Ok(res)
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Deserialize, Default)]
pub struct CommitmentPrefix {
    bytes: Vec<u8>,
}

impl CommitmentPrefix {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            bytes: Vec::from(bytes),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.len() == 0
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_vec(self) -> Vec<u8> {
        self.bytes
    }
}

impl From<Vec<u8>> for CommitmentPrefix {
    fn from(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }
}

impl fmt::Debug for CommitmentPrefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let converted = core::str::from_utf8(self.as_bytes());
        match converted {
            Ok(s) => write!(f, "{}", s),
            Err(_e) => write!(f, "<not valid UTF8: {:?}>", self.as_bytes()),
        }
    }
}

impl Serialize for CommitmentPrefix {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        format!("{:?}", self).serialize(serializer)
    }
}

#[cfg(test)]
pub mod test_util {
    use crate::prelude::*;
    use ibc_proto::ibc::core::commitment::v1::MerkleProof as RawMerkleProof;

    /// Returns a dummy `RawMerkleProof`, for testing only!
    pub fn get_dummy_merkle_proof() -> RawMerkleProof {
        let parsed = ibc_proto::ics23::CommitmentProof { proof: None };
        let mproofs: Vec<ibc_proto::ics23::CommitmentProof> = vec![parsed];
        RawMerkleProof { proofs: mproofs }
    }
}

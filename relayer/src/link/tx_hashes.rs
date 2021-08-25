use core::fmt;

use tendermint::abci::transaction;

use crate::link::relay_sender::AsyncReply;

/// A collection of transaction hashes.
#[derive(Clone)]
pub struct TxHashes(pub Vec<transaction::Hash>);

impl From<AsyncReply> for TxHashes {
    fn from(r: AsyncReply) -> Self {
        Self(r.responses.into_iter().map(|e| e.hash).collect())
    }
}

impl From<TxHashes> for Vec<transaction::Hash> {
    fn from(hs: TxHashes) -> Self {
        hs.0
    }
}

impl fmt::Display for TxHashes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TxHashes: count={}", self.0.len())?;
        self.0.iter().try_for_each(|r| write!(f, "; {}", r))
    }
}

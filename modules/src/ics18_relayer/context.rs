use crate::prelude::*;
use prost_types::Any;

use crate::events::IbcEvent;
use crate::ics02_client::client_state::AnyClientState;
use crate::ics02_client::header::AnyHeader;

use crate::ics18_relayer::error::Error;
use crate::ics24_host::identifier::ClientId;
use crate::signer::Signer;
use crate::Height;
/// Trait capturing all dependencies (i.e., the context) which algorithms in ICS18 require to
/// relay packets between chains. This trait comprises the dependencies towards a single chain.
/// Most of the functions in this represent wrappers over the ABCI interface.
/// This trait mimics the `Chain` trait, but at a lower level of abstraction (no networking, header
/// types, light client, RPC client, etc.)
pub trait Ics18Context {
    /// Returns the latest height of the chain.
    fn query_latest_height(&self) -> Height;

    /// Returns this client state for the given `client_id` on this chain.
    /// Wrapper over the `/abci_query?path=..` endpoint.
    fn query_client_full_state(&self, client_id: &ClientId) -> Option<AnyClientState>;

    /// Returns the most advanced header of this chain.
    fn query_latest_header(&self) -> Option<AnyHeader>;

    /// Interface that the relayer uses to submit a datagram to this chain.
    /// One can think of this as wrapping around the `/broadcast_tx_commit` ABCI endpoint.
    fn send(&mut self, msgs: Vec<Any>) -> Result<Vec<IbcEvent>, Error>;

    /// Temporary solution. Similar to `CosmosSDKChain::key_and_signer()` but simpler.
    fn signer(&self) -> Signer;
}

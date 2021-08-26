//! ICS2 (client) context. The two traits `ClientReader` and `ClientKeeper` define the interface
//! that any host chain must implement to be able to process any `ClientMsg`. See
//! "ADR 003: IBC protocol implementation" for more details.

use crate::ics02_client::client_consensus::AnyConsensusState;
use crate::ics02_client::client_state::AnyClientState;
use crate::ics02_client::client_type::ClientType;
use crate::ics02_client::error::Error;
use crate::ics02_client::handler::ClientResult::{self, Create, Update, Upgrade};
use crate::ics24_host::identifier::ClientId;
use crate::Height;

/// Defines the read-only part of ICS2 (client functions) context.
pub trait ClientReader {
    fn client_type(&self, client_id: &ClientId) -> Result<ClientType, Error>;
    fn client_state(&self, client_id: &ClientId) -> Result<AnyClientState, Error>;
    fn consensus_state(
        &self,
        client_id: &ClientId,
        height: Height,
    ) -> Result<AnyConsensusState, Error>;

    /// Returns a natural number, counting how many clients have been created thus far.
    /// The value of this counter should increase only via method `ClientKeeper::increase_client_counter`.
    fn client_counter(&self) -> Result<u64, Error>;
}

/// Defines the write-only part of ICS2 (client functions) context.
pub trait ClientKeeper {
    fn store_client_result(&mut self, handler_res: ClientResult) -> Result<(), Error> {
        match handler_res {
            Create(res) => {
                let client_id = res.client_id.clone();

                self.store_client_type(client_id.clone(), res.client_type)?;
                self.store_client_state(client_id.clone(), res.client_state.clone())?;
                self.store_consensus_state(
                    client_id,
                    res.client_state.latest_height(),
                    res.consensus_state,
                )?;
                self.increase_client_counter();
                Ok(())
            }
            Update(res) => {
                self.store_client_state(res.client_id.clone(), res.client_state.clone())?;
                self.store_consensus_state(
                    res.client_id.clone(),
                    res.client_state.latest_height(),
                    res.consensus_state,
                )?;
                Ok(())
            }
            Upgrade(res) => {
                self.store_client_state(res.client_id.clone(), res.client_state.clone())?;
                self.store_consensus_state(
                    res.client_id.clone(),
                    res.client_state.latest_height(),
                    res.consensus_state,
                )?;
                Ok(())
            }
        }
    }

    /// Called upon successful client creation
    fn store_client_type(
        &mut self,
        client_id: ClientId,
        client_type: ClientType,
    ) -> Result<(), Error>;

    /// Called upon successful client creation and update
    fn store_client_state(
        &mut self,
        client_id: ClientId,
        client_state: AnyClientState,
    ) -> Result<(), Error>;

    /// Called upon successful client creation and update
    fn store_consensus_state(
        &mut self,
        client_id: ClientId,
        height: Height,
        consensus_state: AnyConsensusState,
    ) -> Result<(), Error>;

    /// Called upon client creation.
    /// Increases the counter which keeps track of how many clients have been created.
    /// Should never fail.
    fn increase_client_counter(&mut self);
}

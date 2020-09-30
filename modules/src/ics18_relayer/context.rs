use crate::handler::HandlerOutput;
use crate::ics02_client::client_def::{AnyClientState, AnyHeader};
use crate::ics18_relayer::error::Error;
use crate::ics24_host::identifier::ClientId;
use crate::ics26_routing::msgs::ICS26Envelope;
use crate::Height;

/// Trait capturing all dependencies (i.e., the context) which algorithms in ICS18 require to
/// relay packets between chains. This trait comprises the dependencies towards a single chain.
/// TODO -- eventually this trait should mirror the `Chain` trait.
pub trait ICS18Context {
    /// Returns the latest height of the chain.
    fn query_latest_height(&self) -> Height;

    /// MockClientState->latest_height() of this client
    fn query_client_full_state(&self, client_id: &ClientId) -> Option<AnyClientState>;

    fn query_latest_header(&self) -> Option<AnyHeader>;

    fn send(&mut self, msg: ICS26Envelope) -> Result<HandlerOutput<()>, Error>;
}

use std::time::Duration;

use crate::chain::counterparty::connection_state_on_destination;
use crate::util::retry::RetryResult;
use flex_error::define_error;
use ibc_proto::ibc::core::connection::v1::QueryConnectionsRequest;
use prost_types::Any;
use serde::Serialize;
use tracing::debug;
use tracing::{error, warn};

use ibc::events::IbcEvent;
use ibc::ics02_client::height::Height;
use ibc::ics03_connection::connection::{
    ConnectionEnd, Counterparty, IdentifiedConnectionEnd, State,
};
use ibc::ics03_connection::msgs::conn_open_ack::MsgConnectionOpenAck;
use ibc::ics03_connection::msgs::conn_open_confirm::MsgConnectionOpenConfirm;
use ibc::ics03_connection::msgs::conn_open_init::MsgConnectionOpenInit;
use ibc::ics03_connection::msgs::conn_open_try::MsgConnectionOpenTry;
use ibc::ics24_host::identifier::{ChainId, ClientId, ConnectionId};
use ibc::timestamp::ZERO_DURATION;
use ibc::tx_msg::Msg;

use crate::chain::handle::ChainHandle;
use crate::error::Error as RelayerError;
use crate::foreign_client::{ForeignClient, ForeignClientError};
use crate::object::Connection as WorkerConnectionObject;
use crate::supervisor::Error as SupervisorError;

/// Maximum value allowed for packet delay on any new connection that the relayer establishes.
pub const MAX_PACKET_DELAY: Duration = Duration::from_secs(120);

const MAX_RETRIES: usize = 5;

define_error! {
    ConnectionError {
        Relayer
            [ RelayerError ]
            |e| { format_args!("relayer error: {}", e.source) },

        MissingLocalConnectionId
            |_| { "failed due to missing local connection id" },

        MissingCounterpartyConnectionIdField
            { counterparty: Counterparty }
            |e| {
                format!("the connection end has no connection id field in the counterparty: {:?}",
                    e.counterparty)
            },

        MissingCounterpartyConnectionId
            |_| { "failed due to missing counterparty connection id" },

        ChainQuery
            { chain_id: ChainId }
            [ RelayerError ]
            |e| {
                format!("failed during a query to chain id {0}", e.chain_id)
            },

        ConnectionQuery
            { connection_id: ConnectionId }
            [ RelayerError ]
            |e| {
                format!("failed to query the connection for {}", e.connection_id)
            },

        ClientOperation
            {
                client_id: ClientId,
                chain_id: ChainId,
            }
            [ ForeignClientError ]
            |e| {
                format!("failed during an operation on client ({0}) hosted by chain ({1})",
                    e.client_id, e.chain_id)
            },

        Submit
            { chain_id: ChainId }
            [ RelayerError ]
            |e| {
                format!("failed during a transaction submission step to chain id {0}",
                    e.chain_id)
            },

        MaxDelayPeriod
            { delay_period: Duration }
            |e| {
                format!("Invalid delay period '{:?}': should be at max '{:?}'",
                    e.delay_period, MAX_PACKET_DELAY)
            },

        InvalidEvent
            { event: IbcEvent }
            |e| {
                format!("a connection object cannot be built from {}",
                    e.event)
            },

        TxResponse
            { event: String }
            |e| {
                format!("tx response event consists of an error: {}",
                    e.event)
            },

        ConnectionClientIdMismatch
            {
                client_id: ClientId,
                foreign_client_id: ClientId
            }
            |e| {
                format!("the client id in the connection end ({}) does not match the foreign client id ({})",
                    e.client_id, e.foreign_client_id)
            },

        ChainIdMismatch
            {
                source_chain_id: ChainId,
                destination_chain_id: ChainId
            }
            |e| {
                format!("the source chain of client a ({}) does not not match the destination chain of client b ({})",
                    e.source_chain_id, e.destination_chain_id)
            },

        ConnectionNotOpen
            {
                state: State,
            }
            |e| {
                format!("the connection end is expected to be in state 'Open'; found state: {:?}",
                    e.state)
            },

        MaxRetry
            |_| {
                format!("Failed to finish connection handshake in {:?} iterations",
                    MAX_RETRIES)
            },

        Supervisor
            [ SupervisorError ]
            |_| { "supervisor error" },

        MissingConnectionId
            {
                chain_id: ChainId,
            }
            |e| {
                format!("missing connection on source chain {}",
                    e.chain_id)
            },

        Signer
            { chain_id: ChainId }
            [ RelayerError ]
            |e| {
                format!("failed while fetching the signer for chain ({})",
                    e.chain_id)
            },

        MissingConnectionIdFromEvent
            |_| { "cannot extract connection_id from result" },

        MissingConnectionInitEvent
            |_| { "no conn init event was in the response" },

        MissingConnectionTryEvent
            |_| { "no conn try event was in the response" },

        MissingConnectionAckEvent
            |_| { "no conn ack event was in the response" },

        MissingConnectionConfirmEvent
            |_| { "no conn confirm event was in the response" },

        ConnectionProof
            [ RelayerError ]
            |_| { "failed to build connection proofs" },

        ConnectionAlreadyExist
            { connection_id: ConnectionId }
            |e| {
                format!("connection {} already exist in an incompatible state", e.connection_id)
            },

    }
}

#[derive(Clone, Debug)]
pub struct ConnectionSide<Chain: ChainHandle> {
    pub(crate) chain: Chain,
    client_id: ClientId,
    connection_id: Option<ConnectionId>,
}

impl<Chain: ChainHandle> ConnectionSide<Chain> {
    pub fn new(chain: Chain, client_id: ClientId, connection_id: Option<ConnectionId>) -> Self {
        Self {
            chain,
            client_id,
            connection_id,
        }
    }
    pub fn connection_id(&self) -> Option<&ConnectionId> {
        self.connection_id.as_ref()
    }
}

impl<Chain: ChainHandle> Serialize for ConnectionSide<Chain> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(Debug, Serialize)]
        struct ConnectionSide<'a> {
            client_id: &'a ClientId,
            connection_id: &'a Option<ConnectionId>,
        }

        let value = ConnectionSide {
            client_id: &self.client_id,
            connection_id: &self.connection_id,
        };

        value.serialize(serializer)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Connection<ChainA: ChainHandle, ChainB: ChainHandle> {
    pub delay_period: Duration,
    pub a_side: ConnectionSide<ChainA>,
    pub b_side: ConnectionSide<ChainB>,
}

impl<ChainA: ChainHandle, ChainB: ChainHandle> Connection<ChainA, ChainB> {
    /// Create a new connection, ensuring that the handshake has succeeded and the two connection
    /// ends exist on each side.
    pub fn new(
        a_client: ForeignClient<ChainA, ChainB>,
        b_client: ForeignClient<ChainB, ChainA>,
        delay_period: Duration,
    ) -> Result<Self, ConnectionError> {
        Self::validate_clients(&a_client, &b_client)?;

        // Validate the delay period against the upper bound
        if delay_period > MAX_PACKET_DELAY {
            return Err(ConnectionError::max_delay_period(delay_period));
        }

        let mut c = Self {
            delay_period,
            a_side: ConnectionSide::new(
                a_client.dst_chain(),
                a_client.id().clone(),
                Default::default(),
            ),
            b_side: ConnectionSide::new(
                b_client.dst_chain(),
                b_client.id().clone(),
                Default::default(),
            ),
        };

        c.handshake()?;

        Ok(c)
    }
    pub fn restore_from_event(
        chain: ChainA,
        counterparty_chain: ChainB,
        connection_open_event: IbcEvent,
    ) -> Result<Connection<ChainA, ChainB>, ConnectionError> {
        let connection_event_attributes = connection_open_event
            .connection_attributes()
            .ok_or_else(|| ConnectionError::invalid_event(connection_open_event.clone()))?;

        let connection_id = connection_event_attributes.connection_id.clone();

        let counterparty_connection_id = connection_event_attributes
            .counterparty_connection_id
            .clone();

        let client_id = connection_event_attributes.client_id.clone();
        let counterparty_client_id = connection_event_attributes.counterparty_client_id.clone();

        Ok(Connection {
            // The event does not include the connection delay.
            delay_period: Default::default(),
            a_side: ConnectionSide::new(chain, client_id, connection_id),
            b_side: ConnectionSide::new(
                counterparty_chain,
                counterparty_client_id,
                counterparty_connection_id,
            ),
        })
    }

    /// Recreates a 'Connection' object from the worker's object built from chain state scanning.
    /// The connection must exist on chain.
    pub fn restore_from_state(
        chain: ChainA,
        counterparty_chain: ChainB,
        connection: WorkerConnectionObject,
        height: Height,
    ) -> Result<(Connection<ChainA, ChainB>, State), ConnectionError> {
        let a_connection = chain
            .query_connection(&connection.src_connection_id, height)
            .map_err(ConnectionError::relayer)?;

        let client_id = a_connection.client_id();
        let delay_period = a_connection.delay_period();

        let counterparty_connection_id = a_connection.counterparty().connection_id.clone();

        let counterparty_client_id = a_connection.counterparty().client_id();

        let mut handshake_connection = Connection {
            delay_period,
            a_side: ConnectionSide::new(
                chain,
                client_id.clone(),
                Some(connection.src_connection_id.clone()),
            ),
            b_side: ConnectionSide::new(
                counterparty_chain.clone(),
                counterparty_client_id.clone(),
                counterparty_connection_id.clone(),
            ),
        };

        if a_connection.state_matches(&State::Init) && counterparty_connection_id.is_none() {
            let req = QueryConnectionsRequest {
                pagination: ibc_proto::cosmos::base::query::pagination::all(),
            };
            let connections: Vec<IdentifiedConnectionEnd> = counterparty_chain
                .query_connections(req)
                .map_err(ConnectionError::relayer)?;

            for conn in connections {
                if !conn
                    .connection_end
                    .client_id_matches(a_connection.counterparty().client_id())
                {
                    continue;
                }
                if let Some(remote_connection_id) =
                    conn.connection_end.counterparty().connection_id()
                {
                    if remote_connection_id == &connection.src_connection_id {
                        handshake_connection.b_side.connection_id = Some(conn.connection_id);
                        break;
                    }
                }
            }
        }

        Ok((handshake_connection, *a_connection.state()))
    }

    pub fn find(
        a_client: ForeignClient<ChainA, ChainB>,
        b_client: ForeignClient<ChainB, ChainA>,
        conn_end_a: &IdentifiedConnectionEnd,
    ) -> Result<Connection<ChainA, ChainB>, ConnectionError> {
        Self::validate_clients(&a_client, &b_client)?;

        // Validate the connection end
        if conn_end_a.end().client_id().ne(a_client.id()) {
            return Err(ConnectionError::connection_client_id_mismatch(
                conn_end_a.end().client_id().clone(),
                a_client.id().clone(),
            ));
        }
        if conn_end_a.end().counterparty().client_id() != b_client.id() {
            return Err(ConnectionError::connection_client_id_mismatch(
                conn_end_a.end().counterparty().client_id().clone(),
                b_client.id().clone(),
            ));
        }
        if !conn_end_a.end().state_matches(&State::Open) {
            return Err(ConnectionError::connection_not_open(
                *conn_end_a.end().state(),
            ));
        }
        let b_conn_id = conn_end_a
            .end()
            .counterparty()
            .connection_id()
            .cloned()
            .ok_or_else(|| {
                ConnectionError::missing_counterparty_connection_id_field(
                    conn_end_a.end().counterparty().clone(),
                )
            })?;

        let c = Connection {
            delay_period: conn_end_a.end().delay_period(),
            a_side: ConnectionSide {
                chain: a_client.dst_chain.clone(),
                client_id: a_client.id,
                connection_id: Some(conn_end_a.id().clone()),
            },
            b_side: ConnectionSide {
                chain: b_client.dst_chain.clone(),
                client_id: b_client.id,
                connection_id: Some(b_conn_id),
            },
        };

        Ok(c)
    }

    // Verifies that the two clients are mutually consistent, i.e., they serve the same two chains.
    fn validate_clients(
        a_client: &ForeignClient<ChainA, ChainB>,
        b_client: &ForeignClient<ChainB, ChainA>,
    ) -> Result<(), ConnectionError> {
        if a_client.src_chain().id() != b_client.dst_chain().id() {
            return Err(ConnectionError::chain_id_mismatch(
                a_client.src_chain().id(),
                b_client.dst_chain().id(),
            ));
        }

        if a_client.dst_chain().id() != b_client.src_chain().id() {
            return Err(ConnectionError::chain_id_mismatch(
                a_client.dst_chain().id(),
                b_client.src_chain().id(),
            ));
        }

        Ok(())
    }

    pub fn src_chain(&self) -> ChainA {
        self.a_side.chain.clone()
    }

    pub fn dst_chain(&self) -> ChainB {
        self.b_side.chain.clone()
    }

    pub fn src_client_id(&self) -> &ClientId {
        &self.a_side.client_id
    }

    pub fn dst_client_id(&self) -> &ClientId {
        &self.b_side.client_id
    }

    pub fn src_connection_id(&self) -> Option<&ConnectionId> {
        self.a_side.connection_id()
    }

    pub fn dst_connection_id(&self) -> Option<&ConnectionId> {
        self.b_side.connection_id()
    }

    pub fn flipped(&self) -> Connection<ChainB, ChainA> {
        Connection {
            a_side: self.b_side.clone(),
            b_side: self.a_side.clone(),
            delay_period: self.delay_period,
        }
    }

    /// Executes a connection handshake protocol (ICS 003) for this connection object
    fn handshake(&mut self) -> Result<(), ConnectionError> {
        let done = '🥂';

        let a_chain = self.a_side.chain.clone();
        let b_chain = self.b_side.chain.clone();

        // Try connOpenInit on a_chain
        let mut counter = 0;
        while counter < MAX_RETRIES {
            counter += 1;
            match self.flipped().build_conn_init_and_send() {
                Err(e) => {
                    error!("Failed ConnInit {:?}: {}", self.a_side, e);
                    continue;
                }
                Ok(result) => {
                    self.a_side.connection_id = Some(extract_connection_id(&result)?.clone());
                    println!("🥂  {} => {:#?}\n", self.a_side.chain.id(), result);
                    break;
                }
            }
        }

        // Try connOpenTry on b_chain
        counter = 0;
        while counter < MAX_RETRIES {
            counter += 1;
            match self.build_conn_try_and_send() {
                Err(e) => {
                    error!("Failed ConnTry {:?}: {}", self.b_side, e);
                    continue;
                }
                Ok(result) => {
                    self.b_side.connection_id = Some(extract_connection_id(&result)?.clone());
                    println!("{}  {} => {:#?}\n", done, self.b_side.chain.id(), result);
                    break;
                }
            }
        }

        counter = 0;
        while counter < MAX_RETRIES {
            counter += 1;

            let src_connection_id = self
                .src_connection_id()
                .ok_or_else(ConnectionError::missing_local_connection_id)?;
            let dst_connection_id = self
                .dst_connection_id()
                .ok_or_else(ConnectionError::missing_counterparty_connection_id)?;

            // Continue loop if query error
            let a_connection = a_chain.query_connection(src_connection_id, Height::zero());
            if a_connection.is_err() {
                continue;
            }
            let b_connection = b_chain.query_connection(dst_connection_id, Height::zero());
            if b_connection.is_err() {
                continue;
            }

            match (a_connection.unwrap().state(), b_connection.unwrap().state()) {
                (State::Init, State::TryOpen) | (State::TryOpen, State::TryOpen) => {
                    // Ack to a_chain
                    match self.flipped().build_conn_ack_and_send() {
                        Err(e) => error!("Failed ConnAck {:?}: {}", self.a_side, e),
                        Ok(event) => {
                            println!("{}  {} => {:#?}\n", done, self.a_side.chain.id(), event)
                        }
                    }
                }
                (State::Open, State::TryOpen) => {
                    // Confirm to b_chain
                    match self.build_conn_confirm_and_send() {
                        Err(e) => error!("Failed ConnConfirm {:?}: {}", self.b_side, e),
                        Ok(event) => {
                            println!("{}  {} => {:#?}\n", done, self.b_side.chain.id(), event)
                        }
                    }
                }
                (State::TryOpen, State::Open) => {
                    // Confirm to a_chain
                    match self.flipped().build_conn_confirm_and_send() {
                        Err(e) => error!("Failed ConnConfirm {:?}: {}", self.a_side, e),
                        Ok(event) => {
                            println!("{}  {} => {:#?}\n", done, self.a_side.chain.id(), event)
                        }
                    }
                }
                (State::Open, State::Open) => {
                    println!(
                        "{0}{0}{0}  Connection handshake finished for [{1:#?}]\n",
                        done, self
                    );
                    return Ok(());
                }
                _ => {}
            }
        }

        Err(ConnectionError::max_retry())
    }

    pub fn counterparty_state(&self) -> Result<State, ConnectionError> {
        // Source connection ID must be specified
        let connection_id = self
            .src_connection_id()
            .ok_or_else(ConnectionError::missing_local_connection_id)?;

        let connection_end = self
            .src_chain()
            .query_connection(connection_id, Height::zero())
            .map_err(|e| ConnectionError::connection_query(connection_id.clone(), e))?;

        let connection = IdentifiedConnectionEnd {
            connection_end,
            connection_id: connection_id.clone(),
        };

        connection_state_on_destination(connection, &self.dst_chain())
            .map_err(ConnectionError::supervisor)
    }

    pub fn handshake_step(&mut self, state: State) -> Result<Vec<IbcEvent>, ConnectionError> {
        match (state, self.counterparty_state()?) {
            (State::Init, State::Uninitialized) => Ok(vec![self.build_conn_try_and_send()?]),
            (State::Init, State::Init) => Ok(vec![self.build_conn_try_and_send()?]),
            (State::TryOpen, State::Init) => Ok(vec![self.build_conn_ack_and_send()?]),
            (State::TryOpen, State::TryOpen) => Ok(vec![self.build_conn_ack_and_send()?]),
            (State::Open, State::TryOpen) => Ok(vec![self.build_conn_confirm_and_send()?]),
            _ => Ok(vec![]),
        }
    }

    pub fn step_state(&mut self, state: State, index: u64) -> RetryResult<(), u64> {
        let done = '🥳';

        match self.handshake_step(state) {
            Err(e) => {
                error!("failed {:?} with error {}", state, e);
                RetryResult::Retry(index)
            }
            Ok(ev) => {
                debug!("{} => {:#?}\n", done, ev);
                RetryResult::Ok(())
            }
        }
    }

    pub fn step_event(&mut self, event: IbcEvent, index: u64) -> RetryResult<(), u64> {
        let state = match event {
            IbcEvent::OpenInitConnection(_) => State::Init,
            IbcEvent::OpenTryConnection(_) => State::TryOpen,
            IbcEvent::OpenAckConnection(_) => State::Open,
            IbcEvent::OpenConfirmConnection(_) => State::Open,
            _ => State::Uninitialized,
        };

        self.step_state(state, index)
    }

    /// Retrieves the connection from destination and compares against the expected connection
    /// built from the message type (`msg_type`) and options (`opts`).
    /// If the expected and the destination connections are compatible, it returns the expected connection
    fn validated_expected_connection(
        &self,
        msg_type: ConnectionMsgType,
    ) -> Result<ConnectionEnd, ConnectionError> {
        let dst_connection_id = self
            .dst_connection_id()
            .ok_or_else(ConnectionError::missing_counterparty_connection_id)?;

        let prefix = self
            .src_chain()
            .query_commitment_prefix()
            .map_err(|e| ConnectionError::chain_query(self.src_chain().id(), e))?;

        // If there is a connection present on the destination chain, it should look like this:
        let counterparty = Counterparty::new(
            self.src_client_id().clone(),
            self.src_connection_id().cloned(),
            prefix,
        );

        // The highest expected state, depends on the message type:
        let highest_state = match msg_type {
            ConnectionMsgType::OpenAck => State::TryOpen,
            ConnectionMsgType::OpenConfirm => State::TryOpen,
            _ => State::Uninitialized,
        };

        let versions = self
            .src_chain()
            .query_compatible_versions()
            .map_err(|e| ConnectionError::chain_query(self.src_chain().id(), e))?;

        let dst_expected_connection = ConnectionEnd::new(
            highest_state,
            self.dst_client_id().clone(),
            counterparty,
            versions,
            ZERO_DURATION,
        );

        // Retrieve existing connection if any
        let dst_connection = self
            .dst_chain()
            .query_connection(dst_connection_id, Height::zero())
            .map_err(|e| ConnectionError::chain_query(self.dst_chain().id(), e))?;

        // Check if a connection is expected to exist on destination chain
        // A connection must exist on destination chain for Ack and Confirm Tx-es to succeed
        if dst_connection.state_matches(&State::Uninitialized) {
            return Err(ConnectionError::missing_connection_id(
                self.dst_chain().id(),
            ));
        }

        check_destination_connection_state(
            dst_connection_id.clone(),
            dst_connection,
            dst_expected_connection.clone(),
        )?;

        Ok(dst_expected_connection)
    }

    pub fn build_update_client_on_src(&self, height: Height) -> Result<Vec<Any>, ConnectionError> {
        let client = self.restore_src_client();
        client.build_update_client(height).map_err(|e| {
            ConnectionError::client_operation(
                self.src_client_id().clone(),
                self.src_chain().id(),
                e,
            )
        })
    }

    pub fn build_update_client_on_dst(&self, height: Height) -> Result<Vec<Any>, ConnectionError> {
        let client = self.restore_dst_client();
        client.build_update_client(height).map_err(|e| {
            ConnectionError::client_operation(
                self.dst_client_id().clone(),
                self.dst_chain().id(),
                e,
            )
        })
    }

    pub fn build_conn_init(&self) -> Result<Vec<Any>, ConnectionError> {
        // Get signer
        let signer = self
            .dst_chain()
            .get_signer()
            .map_err(|e| ConnectionError::signer(self.dst_chain().id(), e))?;

        let prefix = self
            .src_chain()
            .query_commitment_prefix()
            .map_err(|e| ConnectionError::chain_query(self.src_chain().id(), e))?;

        let counterparty = Counterparty::new(self.src_client_id().clone(), None, prefix);

        let version = self
            .dst_chain()
            .query_compatible_versions()
            .map_err(|e| ConnectionError::chain_query(self.dst_chain().id(), e))?[0]
            .clone();

        // Build the domain type message
        let new_msg = MsgConnectionOpenInit {
            client_id: self.dst_client_id().clone(),
            counterparty,
            version,
            delay_period: self.delay_period,
            signer,
        };

        Ok(vec![new_msg.to_any()])
    }

    pub fn build_conn_init_and_send(&self) -> Result<IbcEvent, ConnectionError> {
        let dst_msgs = self.build_conn_init()?;

        let events = self
            .dst_chain()
            .send_messages_and_wait_commit(dst_msgs)
            .map_err(|e| ConnectionError::submit(self.dst_chain().id(), e))?;

        // Find the relevant event for connection init
        let result = events
            .into_iter()
            .find(|event| {
                matches!(event, IbcEvent::OpenInitConnection(_))
                    || matches!(event, IbcEvent::ChainError(_))
            })
            .ok_or_else(ConnectionError::missing_connection_init_event)?;

        // TODO - make chainError an actual error
        match result {
            IbcEvent::OpenInitConnection(_) => Ok(result),
            IbcEvent::ChainError(e) => Err(ConnectionError::tx_response(e)),
            _ => panic!("internal error"),
        }
    }

    /// Attempts to build a MsgConnOpenTry.
    pub fn build_conn_try(&self) -> Result<Vec<Any>, ConnectionError> {
        let src_connection_id = self
            .src_connection_id()
            .ok_or_else(ConnectionError::missing_local_connection_id)?;

        let src_connection = self
            .src_chain()
            .query_connection(src_connection_id, Height::zero())
            .map_err(|e| ConnectionError::chain_query(self.src_chain().id(), e))?;

        // TODO - check that the src connection is consistent with the try options

        // Cross-check the delay_period
        let delay = if src_connection.delay_period() != self.delay_period {
            warn!("`delay_period` for ConnectionEnd @{} is {}s; delay period on local Connection object is set to {}s",
                self.src_chain().id(), src_connection.delay_period().as_secs_f64(), self.delay_period.as_secs_f64());

            warn!(
                "Overriding delay period for local connection object to {}s",
                src_connection.delay_period().as_secs_f64()
            );

            src_connection.delay_period()
        } else {
            self.delay_period
        };

        // Build add send the message(s) for updating client on source
        // TODO - add check if update client is required
        let src_client_target_height = self
            .dst_chain()
            .query_latest_height()
            .map_err(|e| ConnectionError::chain_query(self.dst_chain().id(), e))?;
        let client_msgs = self.build_update_client_on_src(src_client_target_height)?;
        self.src_chain()
            .send_messages_and_wait_commit(client_msgs)
            .map_err(|e| ConnectionError::submit(self.src_chain().id(), e))?;

        let query_height = self
            .src_chain()
            .query_latest_height()
            .map_err(|e| ConnectionError::chain_query(self.src_chain().id(), e))?;
        let (client_state, proofs) = self
            .src_chain()
            .build_connection_proofs_and_client_state(
                ConnectionMsgType::OpenTry,
                src_connection_id,
                self.src_client_id(),
                query_height,
            )
            .map_err(ConnectionError::connection_proof)?;

        // Build message(s) for updating client on destination
        let mut msgs = self.build_update_client_on_dst(proofs.height())?;

        let counterparty_versions = if src_connection.versions().is_empty() {
            self.src_chain()
                .query_compatible_versions()
                .map_err(|e| ConnectionError::chain_query(self.src_chain().id(), e))?
        } else {
            src_connection.versions()
        };

        // Get signer
        let signer = self
            .dst_chain()
            .get_signer()
            .map_err(|e| ConnectionError::signer(self.dst_chain().id(), e))?;

        let prefix = self
            .src_chain()
            .query_commitment_prefix()
            .map_err(|e| ConnectionError::chain_query(self.src_chain().id(), e))?;

        let counterparty = Counterparty::new(
            self.src_client_id().clone(),
            self.src_connection_id().cloned(),
            prefix,
        );

        let previous_connection_id = if src_connection.counterparty().connection_id.is_none() {
            self.b_side.connection_id.clone()
        } else {
            src_connection.counterparty().connection_id.clone()
        };

        let new_msg = MsgConnectionOpenTry {
            client_id: self.dst_client_id().clone(),
            client_state,
            previous_connection_id,
            counterparty,
            counterparty_versions,
            proofs,
            delay_period: delay,
            signer,
        };

        msgs.push(new_msg.to_any());
        Ok(msgs)
    }

    pub fn build_conn_try_and_send(&self) -> Result<IbcEvent, ConnectionError> {
        let dst_msgs = self.build_conn_try()?;

        let events = self
            .dst_chain()
            .send_messages_and_wait_commit(dst_msgs)
            .map_err(|e| ConnectionError::submit(self.dst_chain().id(), e))?;

        // Find the relevant event for connection try transaction
        let result = events
            .into_iter()
            .find(|event| {
                matches!(event, IbcEvent::OpenTryConnection(_))
                    || matches!(event, IbcEvent::ChainError(_))
            })
            .ok_or_else(ConnectionError::missing_connection_try_event)?;

        match result {
            IbcEvent::OpenTryConnection(_) => Ok(result),
            IbcEvent::ChainError(e) => Err(ConnectionError::tx_response(e)),
            _ => panic!("internal error"),
        }
    }

    /// Attempts to build a MsgConnOpenAck.
    pub fn build_conn_ack(&self) -> Result<Vec<Any>, ConnectionError> {
        let src_connection_id = self
            .src_connection_id()
            .ok_or_else(ConnectionError::missing_local_connection_id)?;
        let dst_connection_id = self
            .dst_connection_id()
            .ok_or_else(ConnectionError::missing_counterparty_connection_id)?;

        let _expected_dst_connection =
            self.validated_expected_connection(ConnectionMsgType::OpenAck)?;

        let src_connection = self
            .src_chain()
            .query_connection(src_connection_id, Height::zero())
            .map_err(|e| ConnectionError::chain_query(self.src_chain().id(), e))?;

        // TODO - check that the src connection is consistent with the ack options

        // Build add **send** the message(s) for updating client on source.
        // TODO - add check if it is required
        let src_client_target_height = self
            .dst_chain()
            .query_latest_height()
            .map_err(|e| ConnectionError::chain_query(self.dst_chain().id(), e))?;
        let client_msgs = self.build_update_client_on_src(src_client_target_height)?;
        self.src_chain()
            .send_messages_and_wait_commit(client_msgs)
            .map_err(|e| ConnectionError::submit(self.src_chain().id(), e))?;

        let query_height = self
            .src_chain()
            .query_latest_height()
            .map_err(|e| ConnectionError::chain_query(self.src_chain().id(), e))?;

        let (client_state, proofs) = self
            .src_chain()
            .build_connection_proofs_and_client_state(
                ConnectionMsgType::OpenAck,
                src_connection_id,
                self.src_client_id(),
                query_height,
            )
            .map_err(ConnectionError::connection_proof)?;

        // Build message(s) for updating client on destination
        let mut msgs = self.build_update_client_on_dst(proofs.height())?;

        // Get signer
        let signer = self
            .dst_chain()
            .get_signer()
            .map_err(|e| ConnectionError::signer(self.dst_chain().id(), e))?;

        let new_msg = MsgConnectionOpenAck {
            connection_id: dst_connection_id.clone(),
            counterparty_connection_id: src_connection_id.clone(),
            client_state,
            proofs,
            version: src_connection.versions()[0].clone(),
            signer,
        };

        msgs.push(new_msg.to_any());
        Ok(msgs)
    }

    pub fn build_conn_ack_and_send(&self) -> Result<IbcEvent, ConnectionError> {
        let dst_msgs = self.build_conn_ack()?;

        let events = self
            .dst_chain()
            .send_messages_and_wait_commit(dst_msgs)
            .map_err(|e| ConnectionError::submit(self.dst_chain().id(), e))?;

        // Find the relevant event for connection ack
        let result = events
            .into_iter()
            .find(|event| {
                matches!(event, IbcEvent::OpenAckConnection(_))
                    || matches!(event, IbcEvent::ChainError(_))
            })
            .ok_or_else(ConnectionError::missing_connection_ack_event)?;

        match result {
            IbcEvent::OpenAckConnection(_) => Ok(result),
            IbcEvent::ChainError(e) => Err(ConnectionError::tx_response(e)),
            _ => panic!("internal error"),
        }
    }

    /// Attempts to build a MsgConnOpenConfirm.
    pub fn build_conn_confirm(&self) -> Result<Vec<Any>, ConnectionError> {
        let src_connection_id = self
            .src_connection_id()
            .ok_or_else(ConnectionError::missing_local_connection_id)?;
        let dst_connection_id = self
            .dst_connection_id()
            .ok_or_else(ConnectionError::missing_counterparty_connection_id)?;

        let _expected_dst_connection =
            self.validated_expected_connection(ConnectionMsgType::OpenAck)?;

        let query_height = self
            .src_chain()
            .query_latest_height()
            .map_err(|e| ConnectionError::chain_query(self.src_chain().id(), e))?;

        let _src_connection = self
            .src_chain()
            .query_connection(src_connection_id, query_height)
            .map_err(|e| ConnectionError::connection_query(src_connection_id.clone(), e))?;

        // TODO - check that the src connection is consistent with the confirm options

        let (_, proofs) = self
            .src_chain()
            .build_connection_proofs_and_client_state(
                ConnectionMsgType::OpenConfirm,
                src_connection_id,
                self.src_client_id(),
                query_height,
            )
            .map_err(ConnectionError::connection_proof)?;

        // Build message(s) for updating client on destination
        let mut msgs = self.build_update_client_on_dst(proofs.height())?;

        // Get signer
        let signer = self
            .dst_chain()
            .get_signer()
            .map_err(|e| ConnectionError::signer(self.dst_chain().id(), e))?;

        let new_msg = MsgConnectionOpenConfirm {
            connection_id: dst_connection_id.clone(),
            proofs,
            signer,
        };

        msgs.push(new_msg.to_any());
        Ok(msgs)
    }

    pub fn build_conn_confirm_and_send(&self) -> Result<IbcEvent, ConnectionError> {
        let dst_msgs = self.build_conn_confirm()?;

        let events = self
            .dst_chain()
            .send_messages_and_wait_commit(dst_msgs)
            .map_err(|e| ConnectionError::submit(self.dst_chain().id(), e))?;

        // Find the relevant event for connection confirm
        let result = events
            .into_iter()
            .find(|event| {
                matches!(event, IbcEvent::OpenConfirmConnection(_))
                    || matches!(event, IbcEvent::ChainError(_))
            })
            .ok_or_else(ConnectionError::missing_connection_confirm_event)?;

        match result {
            IbcEvent::OpenConfirmConnection(_) => Ok(result),
            IbcEvent::ChainError(e) => Err(ConnectionError::tx_response(e)),
            _ => panic!("internal error"),
        }
    }

    fn restore_src_client(&self) -> ForeignClient<ChainA, ChainB> {
        ForeignClient::restore(
            self.src_client_id().clone(),
            self.src_chain(),
            self.dst_chain(),
        )
    }

    fn restore_dst_client(&self) -> ForeignClient<ChainB, ChainA> {
        ForeignClient::restore(
            self.dst_client_id().clone(),
            self.dst_chain(),
            self.src_chain(),
        )
    }
}

fn extract_connection_id(event: &IbcEvent) -> Result<&ConnectionId, ConnectionError> {
    match event {
        IbcEvent::OpenInitConnection(ev) => ev.connection_id().as_ref(),
        IbcEvent::OpenTryConnection(ev) => ev.connection_id().as_ref(),
        IbcEvent::OpenAckConnection(ev) => ev.connection_id().as_ref(),
        IbcEvent::OpenConfirmConnection(ev) => ev.connection_id().as_ref(),
        _ => None,
    }
    .ok_or_else(ConnectionError::missing_connection_id_from_event)
}

/// Enumeration of proof carrying ICS3 message, helper for relayer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectionMsgType {
    OpenTry,
    OpenAck,
    OpenConfirm,
}

fn check_destination_connection_state(
    connection_id: ConnectionId,
    existing_connection: ConnectionEnd,
    expected_connection: ConnectionEnd,
) -> Result<(), ConnectionError> {
    let good_client_ids = existing_connection.client_id() == expected_connection.client_id()
        && existing_connection.counterparty().client_id()
            == expected_connection.counterparty().client_id();

    let good_state = *existing_connection.state() as u32 <= *expected_connection.state() as u32;

    let good_connection_ids = existing_connection.counterparty().connection_id().is_none()
        || existing_connection.counterparty().connection_id()
            == expected_connection.counterparty().connection_id();

    // TODO check versions and store prefix

    if good_state && good_client_ids && good_connection_ids {
        Ok(())
    } else {
        Err(ConnectionError::connection_already_exist(connection_id))
    }
}

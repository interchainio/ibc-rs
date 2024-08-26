use alloc::sync::Arc;
use core::{future::Future, str::FromStr, time::Duration};
use query::connection::query_connection_params;
use std::{cmp::Ordering, thread};

use bytes::{Buf, Bytes};
use futures::future::join_all;
use num_bigint::BigInt;
use tokio::runtime::Runtime as TokioRuntime;
use tonic::codegen::http::Uri;
use tonic::metadata::AsciiMetadataValue;
use tracing::{debug, error, instrument, trace, warn};

use ibc_proto::cosmos::base::node::v1beta1::ConfigResponse;
use ibc_proto::cosmos::base::tendermint::v1beta1::service_client::ServiceClient;
use ibc_proto::cosmos::base::tendermint::v1beta1::{GetSyncingRequest, GetSyncingResponse};
use ibc_proto::cosmos::staking::v1beta1::Params as StakingParams;
use ibc_proto::ibc::apps::fee::v1::{
    QueryIncentivizedPacketRequest, QueryIncentivizedPacketResponse,
};
use ibc_proto::ibc::core::channel::v1::{QueryUpgradeErrorRequest, QueryUpgradeRequest};
use ibc_proto::interchain_security::ccv::v1::ConsumerParams as CcvConsumerParams;
use ibc_proto::Protobuf;
use ibc_relayer_types::applications::ics31_icq::response::CrossChainQueryResponse;
use ibc_relayer_types::clients::ics07_tendermint::client_state::{
    AllowUpdate, ClientState as TmClientState,
};
use ibc_relayer_types::clients::ics07_tendermint::consensus_state::ConsensusState as TmConsensusState;
use ibc_relayer_types::clients::ics07_tendermint::header::Header as TmHeader;
use ibc_relayer_types::core::ics02_client::client_type::ClientType;
use ibc_relayer_types::core::ics02_client::error::Error as ClientError;
use ibc_relayer_types::core::ics02_client::events::UpdateClient;
use ibc_relayer_types::core::ics03_connection::connection::{
    ConnectionEnd, IdentifiedConnectionEnd,
};
use ibc_relayer_types::core::ics04_channel::channel::{ChannelEnd, IdentifiedChannelEnd};
use ibc_relayer_types::core::ics04_channel::channel::{State, UpgradeState};
use ibc_relayer_types::core::ics04_channel::packet::Sequence;
use ibc_relayer_types::core::ics23_commitment::commitment::CommitmentPrefix;
use ibc_relayer_types::core::ics23_commitment::merkle::MerkleProof;
use ibc_relayer_types::core::ics24_host::identifier::{
    ChainId, ChannelId, ClientId, ConnectionId, PortId,
};
use ibc_relayer_types::core::ics24_host::path::{
    AcksPath, ChannelEndsPath, ChannelUpgradeErrorPath, ChannelUpgradePath,
    ClientConsensusStatePath, ClientStatePath, CommitmentsPath, ConnectionsPath, ReceiptsPath,
    SeqRecvsPath,
};
use ibc_relayer_types::core::ics24_host::{
    ClientUpgradePath, Path, IBC_QUERY_PATH, SDK_UPGRADE_QUERY_PATH,
};
use ibc_relayer_types::core::{
    ics02_client::height::Height, ics04_channel::upgrade::ErrorReceipt,
    ics04_channel::upgrade::Upgrade,
};
use ibc_relayer_types::signer::Signer;
use ibc_relayer_types::Height as ICSHeight;

use tendermint::block::Height as TmHeight;
use tendermint::node::{self, info::TxIndexStatus};
use tendermint::time::Time as TmTime;
use tendermint_light_client::verifier::types::LightBlock as TmLightBlock;
use tendermint_rpc::client::CompatMode;
use tendermint_rpc::endpoint::broadcast::tx_sync::Response;
use tendermint_rpc::endpoint::status;
use tendermint_rpc::{Client, HttpClient, Order};

use crate::account::Balance;
use crate::chain::client::ClientSettings;
use crate::chain::cosmos::batch::{
    send_batched_messages_and_wait_check_tx, send_batched_messages_and_wait_commit,
    sequential_send_batched_messages_and_wait_commit,
};
use crate::chain::cosmos::encode::key_pair_to_signer;
use crate::chain::cosmos::fee::maybe_register_counterparty_payee;
use crate::chain::cosmos::gas::{calculate_fee, mul_ceil};
use crate::chain::cosmos::query::account::get_or_fetch_account;
use crate::chain::cosmos::query::balance::{query_all_balances, query_balance};
use crate::chain::cosmos::query::consensus_state::query_consensus_state_heights;
use crate::chain::cosmos::query::custom::cross_chain_query_via_rpc;
use crate::chain::cosmos::query::denom_trace::query_denom_trace;
use crate::chain::cosmos::query::fee::query_incentivized_packet;
use crate::chain::cosmos::query::status::query_status;
use crate::chain::cosmos::query::tx::{
    filter_matching_event, query_packets_from_block, query_packets_from_txs, query_txs,
};
use crate::chain::cosmos::query::{abci_query, fetch_version_specs, packet_query, QueryResponse};
use crate::chain::cosmos::types::account::Account;
use crate::chain::cosmos::types::config::TxConfig;
use crate::chain::cosmos::types::gas::{
    default_gas_from_config, gas_multiplier_from_config, max_gas_from_config,
};
use crate::chain::endpoint::{ChainEndpoint, ChainStatus, HealthCheck};
use crate::chain::handle::Subscription;
use crate::chain::requests::*;
use crate::chain::tracking::TrackedMsgs;
use crate::client_state::{AnyClientState, IdentifiedAnyClientState};
use crate::config::Error as ConfigError;
use crate::config::{parse_gas_prices, ChainConfig, GasPrice};
use crate::consensus_state::AnyConsensusState;
use crate::denom::DenomTrace;
use crate::error::Error;
use crate::event::source::{EventSource, TxEventSourceCmd};
use crate::event::IbcEventWithHeight;
use crate::keyring::{KeyRing, Secp256k1KeyPair, SigningKeyPair};
use crate::light_client::tendermint::LightClient as TmLightClient;
use crate::light_client::{LightClient, Verified};
use crate::misbehaviour::MisbehaviourEvidence;
use crate::util::compat_mode::compat_mode_from_version;
use crate::util::pretty::PrettySlice;
use crate::util::pretty::{
    PrettyIdentifiedChannel, PrettyIdentifiedClientState, PrettyIdentifiedConnection,
};
use crate::HERMES_VERSION;

use self::gas::dynamic_gas_price;
use self::types::gas::GasConfig;
use self::version::Specs;

pub mod batch;
pub mod client;
pub mod compatibility;
pub mod config;
pub mod eip_base_fee;
pub mod encode;
pub mod estimate;
pub mod fee;
pub mod gas;
pub mod query;
pub mod retry;
pub mod simulate;
pub mod tx;
pub mod types;
pub mod version;
pub mod wait;

/// Defines an upper limit on how large any transaction can be.
/// This upper limit is defined as a fraction relative to the block's
/// maximum bytes. For example, if the fraction is `0.9`, then
/// `max_tx_size` will not be allowed to exceed 0.9 of the
/// maximum block size of any Cosmos SDK network.
///
/// The default fraction we use is `0.9`; anything larger than that
/// would be risky, as transactions might be rejected; a smaller value
/// might be un-necessarily restrictive on the relayer side.
/// The [default max. block size in Tendermint 0.37 is 21MB](tm-37-max).
/// With a fraction of `0.9`, then Hermes will never permit the configuration
/// of `max_tx_size` to exceed ~18.9MB.
///
/// [tm-37-max]: https://github.com/tendermint/tendermint/blob/v0.37.0-rc1/types/params.go#L79
pub const BLOCK_MAX_BYTES_MAX_FRACTION: f64 = 0.9;

pub struct CosmosSdkChain {
    config: config::CosmosSdkConfig,
    tx_config: TxConfig,
    pub rpc_client: HttpClient,
    compat_mode: CompatMode,
    grpc_addr: Uri,
    light_client: TmLightClient,
    rt: Arc<TokioRuntime>,
    keybase: KeyRing<Secp256k1KeyPair>,

    /// A cached copy of the account information
    account: Option<Account>,

    tx_monitor_cmd: Option<TxEventSourceCmd>,
}

impl CosmosSdkChain {
    /// Get a reference to the configuration for this chain.
    pub fn config(&self) -> &config::CosmosSdkConfig {
        &self.config
    }

    /// The maximum size of any transaction sent by the relayer to this chain
    fn max_tx_size(&self) -> usize {
        self.config.max_tx_size.into()
    }

    fn key(&self) -> Result<Secp256k1KeyPair, Error> {
        self.keybase()
            .get_key(&self.config.key_name)
            .map_err(Error::key_base)
    }

    /// Fetches the trusting period as a `Duration` from the chain config.
    /// If no trusting period exists in the config, the trusting period is calculated
    /// as two-thirds of the `unbonding_period`.
    fn trusting_period(&self, unbonding_period: Duration) -> Duration {
        self.config
            .trusting_period
            .unwrap_or(2 * unbonding_period / 3)
    }

    /// Performs validation of the relayer's configuration
    /// for a specific chain against the parameters of that chain.
    ///
    /// Currently, validates the following:
    ///     - the configured `max_tx_size` is appropriate
    ///     - the trusting period is greater than zero
    ///     - the trusting period is smaller than the unbonding period
    ///     - the default gas is smaller than the max gas
    ///
    /// Emits a log warning in case any error is encountered and
    /// exits early without doing subsequent validations.
    pub fn validate_params(&mut self) -> Result<(), Error> {
        let unbonding_period = self.unbonding_period()?;
        let trusting_period = self.trusting_period(unbonding_period);

        // Check that the trusting period is greater than zero
        if trusting_period <= Duration::ZERO {
            return Err(Error::config_validation_trusting_period_smaller_than_zero(
                self.id().clone(),
                trusting_period,
            ));
        }

        // Check that the trusting period is smaller than the unbounding period
        if trusting_period >= unbonding_period {
            return Err(
                Error::config_validation_trusting_period_greater_than_unbonding_period(
                    self.id().clone(),
                    trusting_period,
                    unbonding_period,
                ),
            );
        }

        let max_gas = max_gas_from_config(&self.config);
        let default_gas = default_gas_from_config(&self.config);

        // If the default gas is strictly greater than the max gas and the tx simulation fails,
        // Hermes won't be able to ever submit that tx because the gas amount wanted will be
        // greater than the max gas.
        if default_gas > max_gas {
            return Err(Error::config_validation_default_gas_too_high(
                self.id().clone(),
                default_gas,
                max_gas,
            ));
        }

        // Get the latest height
        let latest_height = self.query_chain_latest_height()?;

        // Check on the configured max_tx_size against the consensus parameters at latest height
        let result = self
            .block_on(self.rpc_client.consensus_params(latest_height))
            .map_err(|e| {
                Error::config_validation_json_rpc(
                    self.id().clone(),
                    self.config.rpc_addr.to_string(),
                    "/consensus_params".to_string(),
                    e,
                )
            })?;

        let max_bound = result.consensus_params.block.max_bytes;
        let max_allowed = mul_ceil(max_bound, BLOCK_MAX_BYTES_MAX_FRACTION);
        let max_tx_size = BigInt::from(self.max_tx_size());

        if max_tx_size > max_allowed {
            return Err(Error::config_validation_tx_size_out_of_bounds(
                self.id().clone(),
                self.max_tx_size(),
                max_bound,
            ));
        }

        // Check that the configured max gas is lower or equal to the consensus params max gas.
        let consensus_max_gas = result.consensus_params.block.max_gas;

        // If the consensus max gas is < 0, we don't need to perform the check.
        if consensus_max_gas >= 0 {
            let consensus_max_gas: u64 = consensus_max_gas
                .try_into()
                .expect("cannot over or underflow because it is positive");

            let max_gas = max_gas_from_config(&self.config);

            if max_gas > consensus_max_gas {
                return Err(Error::config_validation_max_gas_too_high(
                    self.id().clone(),
                    max_gas,
                    result.consensus_params.block.max_gas,
                ));
            }
        }

        let gas_multiplier = gas_multiplier_from_config(&self.config);

        if gas_multiplier < 1.1 {
            return Err(Error::config_validation_gas_multiplier_low(
                self.id().clone(),
                gas_multiplier,
            ));
        }

        // Query Connection Params with gRPC endpoint to retrieve the `max_expected_time_per_block` value and verify the
        // configured `max_block_time`.
        // If it is not found, the verification for the configured `max_block_time` is skipped.
        match self.block_on(query_connection_params(&self.grpc_addr)) {
            Ok(params) => {
                debug!(
                    "queried `max_expected_time_per_block`: `{}ns`",
                    params.max_expected_time_per_block
                );
                let new_max_block_time = Duration::from_nanos(params.max_expected_time_per_block);

                if new_max_block_time != self.config.max_block_time {
                    warn!(
                        "configured `max_block_time` value of `{}s` does not match queried value of `{}s`. \
                        `max_block_time` will be updated with queried value",
                        self.config.max_block_time.as_secs(),
                        new_max_block_time.as_secs(),
                    );
                    self.config.max_block_time = new_max_block_time;
                }
            }
            Err(e) => {
                warn!(
                    "configured value for max_block_time: `{}s` could not be verified. Error: {e}",
                    self.config.max_block_time.as_secs()
                );
            }
        }

        Ok(())
    }

    fn init_event_source(&mut self) -> Result<TxEventSourceCmd, Error> {
        crate::time!(
            "init_event_source",
            {
                "src_chain": self.config().id.to_string(),
            }
        );

        use crate::config::EventSourceMode as Mode;

        let (event_source, monitor_tx) = match &self.config.event_source {
            Mode::Push { url, batch_delay } => EventSource::websocket(
                self.config.id.clone(),
                url.clone(),
                self.compat_mode,
                *batch_delay,
                self.rt.clone(),
            ),
            Mode::Pull {
                interval,
                max_retries,
            } => EventSource::rpc(
                self.config.id.clone(),
                self.rpc_client.clone(),
                *interval,
                *max_retries,
                self.rt.clone(),
            ),
        }
        .map_err(Error::event_source)?;

        thread::spawn(move || event_source.run());

        Ok(monitor_tx)
    }

    /// Performs a gRPC query to fetch CCV Consumer chain staking parameters.
    pub fn query_ccv_consumer_chain_params(&self) -> Result<CcvConsumerParams, Error> {
        crate::time!(
            "query_ccv_consumer_chain_params",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        crate::telemetry!(query, self.id(), "query_ccv_consumer_chain_params");

        let mut client = self
            .block_on(
                ibc_proto::interchain_security::ccv::consumer::v1::query_client::QueryClient::connect(
                    self.grpc_addr.clone()
                ),
            )
            .map_err(Error::grpc_transport)?;

        client = client
            .max_decoding_message_size(self.config().max_grpc_decoding_size.get_bytes() as usize);

        let request = tonic::Request::new(
            ibc_proto::interchain_security::ccv::consumer::v1::QueryParamsRequest {},
        );

        let response = self
            .block_on(client.query_params(request))
            .map_err(|e| Error::grpc_status(e, "query_ccv_consumer_chain_params".to_owned()))?;

        let params = response
            .into_inner()
            .params
            .ok_or_else(|| Error::grpc_response_param("no staking params".to_string()))?;

        Ok(params)
    }

    /// Performs a gRPC query for Cosmos chain staking parameters.
    pub fn query_staking_params(&self) -> Result<StakingParams, Error> {
        crate::time!(
            "query_staking_params",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        crate::telemetry!(query, self.id(), "query_staking_params");

        let mut client = self
            .block_on(
                ibc_proto::cosmos::staking::v1beta1::query_client::QueryClient::connect(
                    self.grpc_addr.clone(),
                ),
            )
            .map_err(Error::grpc_transport)?;

        client = client
            .max_decoding_message_size(self.config().max_grpc_decoding_size.get_bytes() as usize);

        let request =
            tonic::Request::new(ibc_proto::cosmos::staking::v1beta1::QueryParamsRequest {});

        let response = self
            .block_on(client.params(request))
            .map_err(|e| Error::grpc_status(e, "query_staking_params".to_owned()))?;

        let params = response
            .into_inner()
            .params
            .ok_or_else(|| Error::grpc_response_param("no staking params".to_string()))?;

        Ok(params)
    }

    /// Performs a gRPC query to fetch the configuration parameters of the node.
    ///
    /// ### Note: This query endpoint was introduced in SDK v0.46.3/v0.45.10. Not available before that.
    ///
    /// Returns:
    ///     - `Ok(Some(..))` if the query was successful.
    ///     - `Ok(None) in case the query endpoint is not available.
    ///     - `Err` for any other error.
    pub fn query_config_params(&self) -> Result<Option<ConfigResponse>, Error> {
        crate::time!(
            "query_config_params",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        crate::telemetry!(query, self.id(), "query_config_params");

        // Helper function to diagnose if the node config query is unimplemented
        // by matching on the error details.
        fn is_unimplemented_node_query(err_status: &tonic::Status) -> bool {
            if err_status.code() != tonic::Code::Unimplemented {
                return false;
            }

            err_status
                .message()
                .contains("unknown service cosmos.base.node.v1beta1.Service")
        }

        let mut client = self
            .block_on(
                ibc_proto::cosmos::base::node::v1beta1::service_client::ServiceClient::connect(
                    self.grpc_addr.clone(),
                ),
            )
            .map_err(Error::grpc_transport)?;

        client = client
            .max_decoding_message_size(self.config().max_grpc_decoding_size.get_bytes() as usize);

        let request = tonic::Request::new(ibc_proto::cosmos::base::node::v1beta1::ConfigRequest {});

        match self.block_on(client.config(request)) {
            Ok(response) => {
                let params = response.into_inner();

                Ok(Some(params))
            }
            Err(e) => {
                if is_unimplemented_node_query(&e) {
                    Ok(None)
                } else {
                    Err(Error::grpc_status(e, "query_config_params".to_owned()))
                }
            }
        }
    }

    /// The minimum gas price that this node accepts
    pub fn min_gas_price(&self) -> Result<Option<Vec<GasPrice>>, Error> {
        crate::time!(
            "min_gas_price",
            {
                "src_chain": self.config().id.to_string(),
            }
        );

        let min_gas_price: Option<Vec<GasPrice>> = self
            .query_config_params()?
            .map(|cfg_response| parse_gas_prices(cfg_response.minimum_gas_price));

        Ok(min_gas_price)
    }

    pub fn dynamic_gas_price(&self) -> GasPrice {
        let gas_config = GasConfig::from(self.config());

        self.rt.block_on(dynamic_gas_price(
            &gas_config,
            &self.config.id,
            &self.config.rpc_addr,
        ))
    }

    /// The unbonding period of this chain
    pub fn unbonding_period(&self) -> Result<Duration, Error> {
        crate::time!(
            "unbonding_period",
            {
                "src_chain": self.config().id.to_string(),
            }
        );

        let unbonding_time = if self.config.ccv_consumer_chain {
            self.query_ccv_consumer_chain_params()?
                .unbonding_period
                .ok_or_else(|| {
                    Error::grpc_response_param("no unbonding time in staking params".to_string())
                })?
        } else {
            self.query_staking_params()?.unbonding_time.ok_or_else(|| {
                Error::grpc_response_param("no unbonding time in staking params".to_string())
            })?
        };

        Ok(Duration::new(
            unbonding_time.seconds as u64,
            unbonding_time.nanos as u32,
        ))
    }

    /// The number of historical entries kept by this chain
    pub fn historical_entries(&self) -> Result<u32, Error> {
        crate::time!(
            "historical_entries",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        if self.config.ccv_consumer_chain {
            let ccv_parameters = self.query_ccv_consumer_chain_params()?;
            ccv_parameters.historical_entries.try_into().map_err(|_| {
                Error::invalid_historical_entries(
                    self.id().clone(),
                    ccv_parameters.historical_entries,
                )
            })
        } else {
            self.query_staking_params().map(|p| p.historical_entries)
        }
    }

    /// Run a future to completion on the Tokio runtime.
    fn block_on<F: Future>(&self, f: F) -> F::Output {
        self.rt.block_on(f)
    }

    fn query(
        &self,
        data: impl Into<Path>,
        height_query: QueryHeight,
        prove: bool,
    ) -> Result<QueryResponse, Error> {
        let data = data.into();
        if !data.is_provable() & prove {
            return Err(Error::private_store());
        }

        let response = self.block_on(abci_query(
            &self.rpc_client,
            &self.config.rpc_addr,
            IBC_QUERY_PATH.to_string(),
            data.to_string(),
            height_query.into(),
            prove,
        ))?;

        // TODO: Verify response proof, if requested.

        Ok(response)
    }

    /// Perform an ABCI query against the client upgrade sub-store.
    ///
    /// The data is returned in its raw format `Vec<u8>`, and is either the
    /// client state (if the target path is [`UpgradedClientState`]), or the
    /// client consensus state ([`UpgradedClientConsensusState`]).
    ///
    /// Note: This is a special query in that it will only succeed if the chain
    /// is halted after reaching the height proposed in a successful governance
    /// proposal to upgrade the chain. In this scenario, let P be the height at
    /// which the chain is planned to upgrade. We assume that the chain is
    /// halted at height P. Tendermint will be at height P (as reported by the
    /// /status RPC query), but the application will be at height P-1 (as
    /// reported by the /abci_info RPC query).
    ///
    /// Therefore, `query_height` needs to be P-1. However, the path specified
    /// in `query_data` needs to be constructed with height `P`, as this is how
    /// the chain will have stored it in its upgrade sub-store.
    fn query_client_upgrade_state(
        &self,
        query_data: ClientUpgradePath,
        query_height: ICSHeight,
    ) -> Result<(Vec<u8>, MerkleProof), Error> {
        let path = SDK_UPGRADE_QUERY_PATH.into();

        let response: QueryResponse = self.block_on(abci_query(
            &self.rpc_client,
            &self.config.rpc_addr,
            path,
            Path::Upgrade(query_data).to_string(),
            query_height.into(),
            true,
        ))?;

        let proof = response.proof.ok_or_else(Error::empty_response_proof)?;

        Ok((response.value, proof))
    }

    /// Query the chain status via an RPC query.
    ///
    /// Returns an error if the node is still syncing and has not caught up,
    /// ie. if `sync_info.catching_up` is `true`.
    fn chain_rpc_status(&self) -> Result<status::Response, Error> {
        crate::time!(
            "chain_rpc_status",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        crate::telemetry!(query, self.id(), "rpc_status");

        let status = self
            .block_on(self.rpc_client.status())
            .map_err(|e| Error::rpc(self.config.rpc_addr.clone(), e))?;

        if status.sync_info.catching_up {
            Err(Error::chain_not_caught_up(
                self.config.rpc_addr.to_string(),
                self.config().id.clone(),
            ))
        } else {
            Ok(status)
        }
    }

    /// Query the chain syncing status via a gRPC query.
    ///
    /// Returns an error if the node is still syncing and has not caught up,
    /// ie. if `sync_info.syncing` is `true`.
    fn chain_grpc_status(&self) -> Result<GetSyncingResponse, Error> {
        crate::time!(
            "chain_grpc_status",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        crate::telemetry!(query, self.id(), "grpc_status");

        let grpc_addr = self.grpc_addr.clone();
        let grpc_addr_string = grpc_addr.to_string();

        let mut client = self
            .block_on(ServiceClient::connect(grpc_addr.clone()))
            .map_err(Error::grpc_transport)?;

        let request = tonic::Request::new(GetSyncingRequest {});

        let sync_info = self
            .block_on(client.get_syncing(request))
            .map_err(|e| Error::grpc_status(e, "get_syncing".to_string()))?
            .into_inner();

        if sync_info.syncing {
            Err(Error::chain_not_caught_up(
                grpc_addr_string,
                self.config().id.clone(),
            ))
        } else {
            Ok(sync_info)
        }
    }

    /// Query the chain status of the RPC and gRPC nodes.
    ///
    /// Returns an error if any of the node is still syncing and has not caught up.
    fn chain_status(&self) -> Result<status::Response, Error> {
        crate::time!(
            "chain_status",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        crate::telemetry!(query, self.id(), "status");

        let rpc_status = self.chain_rpc_status()?;

        if rpc_status.sync_info.catching_up {
            return Err(Error::chain_not_caught_up(
                self.config.rpc_addr.to_string(),
                self.config().id.clone(),
            ));
        }

        let grpc_status = self.chain_grpc_status()?;

        if grpc_status.syncing {
            return Err(Error::chain_not_caught_up(
                self.config.grpc_addr.to_string(),
                self.config().id.clone(),
            ));
        }

        Ok(rpc_status)
    }

    /// Query the chain's latest height
    pub fn query_chain_latest_height(&self) -> Result<ICSHeight, Error> {
        crate::time!(
            "query_latest_height",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        crate::telemetry!(query, self.id(), "query_latest_height");

        let status = self.rt.block_on(query_status(
            self.id(),
            &self.rpc_client,
            &self.config.rpc_addr,
        ))?;

        Ok(status.height)
    }

    #[instrument(
        name = "send_messages_and_wait_commit",
        level = "error",
        skip_all,
        fields(
            chain = %self.id(),
            tracking_id = %tracked_msgs.tracking_id()
        ),
    )]
    async fn do_send_messages_and_wait_commit(
        &mut self,
        tracked_msgs: TrackedMsgs,
    ) -> Result<Vec<IbcEventWithHeight>, Error> {
        crate::time!(
            "send_messages_and_wait_commit",
            {
                "src_chain": self.config().id.to_string(),
            }
        );

        let proto_msgs = tracked_msgs.msgs;

        let key_pair = self.key()?;
        let key_account = key_pair.account();

        let account =
            get_or_fetch_account(&self.grpc_addr, &key_account, &mut self.account).await?;

        let memo_prefix = if let Some(memo_overwrite) = &self.config.memo_overwrite {
            memo_overwrite.clone()
        } else {
            self.config.memo_prefix.clone()
        };

        if self.config.sequential_batch_tx {
            sequential_send_batched_messages_and_wait_commit(
                &self.rpc_client,
                &self.tx_config,
                &key_pair,
                account,
                &memo_prefix,
                proto_msgs,
            )
            .await
        } else {
            send_batched_messages_and_wait_commit(
                &self.rpc_client,
                &self.tx_config,
                &key_pair,
                account,
                &memo_prefix,
                proto_msgs,
            )
            .await
        }
    }

    #[instrument(
        name = "send_messages_and_wait_check_tx",
        level = "error",
        skip_all,
        fields(
            chain = %self.id(),
            tracking_id = %tracked_msgs.tracking_id()
        ),
    )]
    async fn do_send_messages_and_wait_check_tx(
        &mut self,
        tracked_msgs: TrackedMsgs,
    ) -> Result<Vec<Response>, Error> {
        crate::time!(
            "send_messages_and_wait_check_tx",
            {
                "src_chain": self.config().id.to_string(),
            }
        );

        let proto_msgs = tracked_msgs.msgs;

        let key_pair = self.key()?;
        let key_account = key_pair.account();

        let account =
            get_or_fetch_account(&self.grpc_addr, &key_account, &mut self.account).await?;

        let memo_prefix = if let Some(memo_overwrite) = &self.config.memo_overwrite {
            memo_overwrite.clone()
        } else {
            self.config.memo_prefix.clone()
        };

        send_batched_messages_and_wait_check_tx(
            &self.rpc_client,
            &self.tx_config,
            &key_pair,
            account,
            &memo_prefix,
            proto_msgs,
        )
        .await
    }

    fn query_packet_from_block(
        &self,
        request: &QueryPacketEventDataRequest,
        seqs: &[Sequence],
        block_height: &ICSHeight,
    ) -> Result<(Vec<IbcEventWithHeight>, Vec<IbcEventWithHeight>), Error> {
        crate::time!(
            "query_block: query block packet events",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        crate::telemetry!(query, self.id(), "query_block");

        let tm_height =
            tendermint::block::Height::try_from(block_height.revision_height()).unwrap();

        let response = self
            .block_on(self.rpc_client.block_results(tm_height))
            .map_err(|e| Error::rpc(self.config.rpc_addr.clone(), e))?;

        let response_height = ICSHeight::new(self.id().version(), u64::from(response.height))
            .map_err(|_| Error::invalid_height_no_source())?;

        let begin_block_events = response
            .begin_block_events
            .unwrap_or_default()
            .iter()
            .filter_map(|ev| filter_matching_event(ev, request, seqs))
            .map(|ev| IbcEventWithHeight::new(ev, response_height))
            .collect();

        let mut end_block_events: Vec<_> = response
            .end_block_events
            .unwrap_or_default()
            .iter()
            .filter_map(|ev| filter_matching_event(ev, request, seqs))
            .map(|ev| IbcEventWithHeight::new(ev, response_height))
            .collect();

        // Since CometBFT 0.38, block events are returned in the
        // finalize_block_events field and the other *_block_events fields
        // are no longer present. We put these in place of the end_block_events
        // in older protocol.
        end_block_events.extend(
            response
                .finalize_block_events
                .iter()
                .filter_map(|ev| filter_matching_event(ev, request, seqs))
                .map(|ev| IbcEventWithHeight::new(ev, response_height)),
        );

        Ok((begin_block_events, end_block_events))
    }

    fn query_packets_from_blocks(
        &self,
        request: &QueryPacketEventDataRequest,
    ) -> Result<(Vec<IbcEventWithHeight>, Vec<IbcEventWithHeight>), Error> {
        crate::time!(
            "query_blocks: query block packet events",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        crate::telemetry!(query, self.id(), "query_blocks");

        let mut begin_block_events = vec![];
        let mut end_block_events = vec![];

        for seq in request.sequences.iter().copied() {
            let response = self
                .block_on(self.rpc_client.block_search(
                    packet_query(request, seq),
                    // We only need the first page
                    1,
                    // There should only be a single match for this query, but due to
                    // the fact that the indexer treat the query as a disjunction over
                    // all events in a block rather than a conjunction over a single event,
                    // we may end up with partial matches and therefore have to account for
                    // that by fetching multiple results and filter it down after the fact.
                    // In the worst case we get N blocks where N is the number of channels,
                    // but 10 seems to work well enough in practice while keeping the response
                    // size, and therefore pressure on the node, fairly low.
                    10,
                    // We could pick either ordering here, since matching blocks may be at pretty
                    // much any height relative to the target blocks, so we went with most recent
                    // blocks first.
                    Order::Descending,
                ))
                .map_err(|e| Error::rpc(self.config.rpc_addr.clone(), e))?;

            for block in response.blocks.into_iter().map(|response| response.block) {
                let response_height =
                    ICSHeight::new(self.id().version(), u64::from(block.header.height))
                        .map_err(|_| Error::invalid_height_no_source())?;

                if let QueryHeight::Specific(query_height) = request.height.get() {
                    if response_height > query_height {
                        continue;
                    }
                }

                // `query_packet_from_block` retrieves the begin and end block events
                // and filter them to retain only those matching the query
                let (new_begin_block_events, new_end_block_events) =
                    self.query_packet_from_block(request, &[seq], &response_height)?;

                begin_block_events.extend(new_begin_block_events);
                end_block_events.extend(new_end_block_events);
            }
        }

        Ok((begin_block_events, end_block_events))
    }
}

impl ChainEndpoint for CosmosSdkChain {
    type LightBlock = TmLightBlock;
    type Header = TmHeader;
    type ConsensusState = TmConsensusState;
    type ClientState = TmClientState;
    type Time = TmTime;
    type SigningKeyPair = Secp256k1KeyPair;

    fn id(&self) -> &ChainId {
        &self.config.id
    }

    fn bootstrap(config: ChainConfig, rt: Arc<TokioRuntime>) -> Result<Self, Error> {
        #[allow(irrefutable_let_patterns)]
        let ChainConfig::CosmosSdk(config) = config
        else {
            return Err(Error::config(ConfigError::wrong_type()));
        };

        let mut rpc_client = HttpClient::builder(config.rpc_addr.clone().try_into().unwrap())
            .user_agent(format!("hermes/{}", HERMES_VERSION))
            .build()
            .map_err(|e| Error::rpc(config.rpc_addr.clone(), e))?;

        let node_info = rt.block_on(fetch_node_info(&rpc_client, &config))?;

        let compat_mode = compat_mode_from_version(&config.compat_mode, node_info.version)?.into();
        rpc_client.set_compat_mode(compat_mode);

        let light_client = TmLightClient::from_cosmos_sdk_config(&config, node_info.id)?;

        // Initialize key store and load key
        let keybase = KeyRing::new_secp256k1(
            config.key_store_type,
            &config.account_prefix,
            &config.id,
            &config.key_store_folder,
        )
        .map_err(Error::key_base)?;

        let grpc_addr = Uri::from_str(&config.grpc_addr.to_string())
            .map_err(|e| Error::invalid_uri(config.grpc_addr.to_string(), e))?;

        let tx_config = TxConfig::try_from(&config)?;

        // Retrieve the version specification of this chain

        let chain = Self {
            config,
            rpc_client,
            compat_mode,
            grpc_addr,
            light_client,
            rt,
            keybase,
            tx_config,
            account: None,
            tx_monitor_cmd: None,
        };

        Ok(chain)
    }

    fn shutdown(self) -> Result<(), Error> {
        if let Some(monitor_tx) = self.tx_monitor_cmd {
            monitor_tx.shutdown().map_err(Error::event_source)?;
        }

        Ok(())
    }

    fn keybase(&self) -> &KeyRing<Self::SigningKeyPair> {
        &self.keybase
    }

    fn keybase_mut(&mut self) -> &mut KeyRing<Self::SigningKeyPair> {
        &mut self.keybase
    }

    fn get_key(&self) -> Result<Self::SigningKeyPair, Error> {
        // Get the key from key seed file
        let key_pair = self
            .keybase()
            .get_key(&self.config.key_name)
            .map_err(|e| Error::key_not_found(self.config().key_name.clone(), e))?;

        Ok(key_pair)
    }

    fn subscribe(&mut self) -> Result<Subscription, Error> {
        let tx_monitor_cmd = match &self.tx_monitor_cmd {
            Some(tx_monitor_cmd) => tx_monitor_cmd,
            None => {
                let tx_monitor_cmd = self.init_event_source()?;
                self.tx_monitor_cmd = Some(tx_monitor_cmd);
                self.tx_monitor_cmd.as_ref().unwrap()
            }
        };

        let subscription = tx_monitor_cmd.subscribe().map_err(Error::event_source)?;
        Ok(subscription)
    }

    /// Does multiple RPC calls to the full node, to check for
    /// reachability and some basic APIs are available.
    ///
    /// Currently this checks that:
    ///     - the node responds OK to `/health` RPC call;
    ///     - the node has transaction indexing enabled;
    ///     - the SDK & IBC versions are supported;
    ///
    /// Emits a log warning in case anything is amiss.
    /// Exits early if any health check fails, without doing any
    /// further checks.
    fn health_check(&mut self) -> Result<HealthCheck, Error> {
        if let Err(e) = do_health_check(self) {
            warn!("health check failed for chain '{}'", self.id());
            warn!("reason: {}", e.detail());
            warn!("some Hermes features may not work in this mode!");

            return Ok(HealthCheck::Unhealthy(Box::new(e)));
        }

        if let Err(e) = self.validate_params() {
            warn!("found potential misconfiguration for chain '{}'", self.id());
            warn!("reason: {}", e.detail());
            warn!("some Hermes features may not work in this mode!");

            return Ok(HealthCheck::Unhealthy(Box::new(e)));
        }

        Ok(HealthCheck::Healthy)
    }

    /// Fetch a header from the chain at the given height and verify it.
    fn verify_header(
        &mut self,
        trusted: ICSHeight,
        target: ICSHeight,
        client_state: &AnyClientState,
    ) -> Result<Self::LightBlock, Error> {
        crate::time!(
            "verify_header",
            {
                "src_chain": self.config().id.to_string(),
            }
        );

        let now = self.chain_status()?.sync_info.latest_block_time;

        self.light_client
            .verify(trusted, target, client_state, now)
            .map(|v| v.target)
    }

    /// Perform misbehavior detection for the given client state and update event.
    fn check_misbehaviour(
        &mut self,
        update: &UpdateClient,
        client_state: &AnyClientState,
    ) -> Result<Option<MisbehaviourEvidence>, Error> {
        crate::time!(
            "check_misbehaviour",
            {
                "src_chain": self.config().id.to_string(),
            }
        );

        let now = self.chain_status()?.sync_info.latest_block_time;

        self.light_client
            .detect_misbehaviour(update, client_state, now)
    }

    // Queries

    /// Send one or more transactions that include all the specified messages.
    /// The `proto_msgs` are split in transactions such they don't exceed the configured maximum
    /// number of messages per transaction and the maximum transaction size.
    /// Then `send_tx()` is called with each Tx. `send_tx()` determines the fee based on the
    /// on-chain simulation and if this exceeds the maximum gas specified in the configuration file
    /// then it returns error.
    /// TODO - more work is required here for a smarter split maybe iteratively accumulating/ evaluating
    /// msgs in a Tx until any of the max size, max num msgs, max fee are exceeded.
    fn send_messages_and_wait_commit(
        &mut self,
        tracked_msgs: TrackedMsgs,
    ) -> Result<Vec<IbcEventWithHeight>, Error> {
        let runtime = self.rt.clone();

        runtime.block_on(self.do_send_messages_and_wait_commit(tracked_msgs))
    }

    fn send_messages_and_wait_check_tx(
        &mut self,
        tracked_msgs: TrackedMsgs,
    ) -> Result<Vec<Response>, Error> {
        let runtime = self.rt.clone();

        runtime.block_on(self.do_send_messages_and_wait_check_tx(tracked_msgs))
    }

    /// Get the account for the signer
    fn get_signer(&self) -> Result<Signer, Error> {
        // Get the key from key seed file
        let key_pair = self.key()?;

        let signer = key_pair_to_signer(&key_pair)?;

        Ok(signer)
    }

    /// Get the chain configuration
    fn config(&self) -> ChainConfig {
        ChainConfig::CosmosSdk(self.config.clone())
    }

    fn version_specs(&self) -> Result<Specs, Error> {
        let version_specs = self.block_on(fetch_version_specs(self.id(), &self.grpc_addr))?;
        Ok(version_specs)
    }

    fn query_balance(&self, key_name: Option<&str>, denom: Option<&str>) -> Result<Balance, Error> {
        crate::time!(
            "query_balance",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        // If a key_name is given, extract the account hash.
        // Else retrieve the account from the configuration file.
        let key = match key_name {
            Some(key_name) => self.keybase().get_key(key_name).map_err(Error::key_base)?,
            None => self.key()?,
        };
        let account = key.account();

        let denom = denom.unwrap_or(&self.config.gas_price.denom);
        let balance = self.block_on(query_balance(&self.grpc_addr, &account, denom))?;

        Ok(balance)
    }

    fn query_all_balances(&self, key_name: Option<&str>) -> Result<Vec<Balance>, Error> {
        crate::time!(
            "query_all_balances",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        // If a key_name is given, extract the account hash.
        // Else retrieve the account from the configuration file.
        let key = match key_name {
            Some(key_name) => self.keybase().get_key(key_name).map_err(Error::key_base)?,
            None => self.key()?,
        };
        let account = key.account();

        let balance = self.block_on(query_all_balances(&self.grpc_addr, &account))?;

        Ok(balance)
    }

    fn query_denom_trace(&self, hash: String) -> Result<DenomTrace, Error> {
        let denom_trace = self.block_on(query_denom_trace(&self.rpc_client, &hash))?;

        Ok(denom_trace)
    }

    fn query_commitment_prefix(&self) -> Result<CommitmentPrefix, Error> {
        crate::time!(
            "query_commitment_prefix",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        crate::telemetry!(query, self.id(), "query_commitment_prefix");

        // TODO - do a real chain query
        CommitmentPrefix::try_from(self.config.store_prefix.as_bytes().to_vec())
            .map_err(|_| Error::ics02(ClientError::empty_prefix()))
    }

    /// Query the application status
    fn query_application_status(&self) -> Result<ChainStatus, Error> {
        crate::time!(
            "query_application_status",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        crate::telemetry!(query, self.id(), "query_application_status");

        // We cannot rely on `/status` endpoint to provide details about the latest block.
        // Instead, we need to pull block height via `/abci_info` and then fetch block
        // metadata at the given height via `/blockchain` endpoint.
        let abci_info = self
            .block_on(self.rpc_client.abci_info())
            .map_err(|e| Error::rpc(self.config.rpc_addr.clone(), e))?;

        // Query `/header` endpoint to pull the latest block that the application committed.
        let response = self
            .block_on(self.rpc_client.header(abci_info.last_block_height))
            .map_err(|e| Error::rpc(self.config.rpc_addr.clone(), e))?;

        let height = ICSHeight::new(
            ChainId::chain_version(response.header.chain_id.as_str()),
            u64::from(abci_info.last_block_height),
        )
        .map_err(|_| Error::invalid_height_no_source())?;

        let timestamp = response.header.time.into();
        Ok(ChainStatus { height, timestamp })
    }

    /// Performs a `QueryClientStatesRequest` gRPC query to fetch all the client states
    /// associated with the chain.
    fn query_clients(
        &self,
        request: QueryClientStatesRequest,
    ) -> Result<Vec<IdentifiedAnyClientState>, Error> {
        crate::time!(
            "query_clients",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        crate::telemetry!(query, self.id(), "query_clients");

        let mut client = self
            .block_on(
                ibc_proto::ibc::core::client::v1::query_client::QueryClient::connect(
                    self.grpc_addr.clone(),
                ),
            )
            .map_err(Error::grpc_transport)?;

        client = client
            .max_decoding_message_size(self.config().max_grpc_decoding_size.get_bytes() as usize);

        let request = tonic::Request::new(request.into());
        let response = self
            .block_on(client.client_states(request))
            .map_err(|e| Error::grpc_status(e, "query_clients".to_owned()))?
            .into_inner();

        // Deserialize into domain type
        let mut clients: Vec<IdentifiedAnyClientState> = response
            .client_states
            .into_iter()
            .filter_map(|cs| {
                IdentifiedAnyClientState::try_from(cs.clone())
                    .map_err(|e| {
                        let (client_type, client_id) = (if let Some(client_state) = &cs.client_state { client_state.type_url.clone() } else { "None".to_string() }, &cs.client_id);
                        warn!("encountered unsupported client type `{}` while scanning client `{}`, skipping the client", client_type, client_id);
                        debug!("failed to parse client state {}. Error: {}", PrettyIdentifiedClientState(&cs), e)
                    })
                    .ok()
            })
            .collect();

        // Sort by client identifier counter
        clients.sort_by_cached_key(|c| client_id_suffix(&c.client_id).unwrap_or(0));

        Ok(clients)
    }

    fn query_client_state(
        &self,
        request: QueryClientStateRequest,
        include_proof: IncludeProof,
    ) -> Result<(AnyClientState, Option<MerkleProof>), Error> {
        crate::time!(
            "query_client_state",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        crate::telemetry!(query, self.id(), "query_client_state");

        let res = self.query(
            ClientStatePath(request.client_id.clone()),
            request.height,
            matches!(include_proof, IncludeProof::Yes),
        )?;
        let client_state = AnyClientState::decode_vec(&res.value).map_err(Error::decode)?;

        match include_proof {
            IncludeProof::Yes => {
                let proof = res.proof.ok_or_else(Error::empty_response_proof)?;
                Ok((client_state, Some(proof)))
            }
            IncludeProof::No => Ok((client_state, None)),
        }
    }

    fn query_upgraded_client_state(
        &self,
        request: QueryUpgradedClientStateRequest,
    ) -> Result<(AnyClientState, MerkleProof), Error> {
        crate::time!(
            "query_upgraded_client_state",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        crate::telemetry!(query, self.id(), "query_upgraded_client_state");

        // Query for the value and the proof.
        let upgrade_height = request.upgrade_height;
        let query_height = upgrade_height
            .decrement()
            .map_err(|_| Error::invalid_height_no_source())?;

        let (upgraded_client_state_raw, proof) = self.query_client_upgrade_state(
            ClientUpgradePath::UpgradedClientState(upgrade_height.revision_height()),
            query_height,
        )?;

        let client_state = AnyClientState::decode_vec(&upgraded_client_state_raw)
            .map_err(Error::conversion_from_any)?;

        Ok((client_state, proof))
    }

    fn query_upgraded_consensus_state(
        &self,
        request: QueryUpgradedConsensusStateRequest,
    ) -> Result<(AnyConsensusState, MerkleProof), Error> {
        crate::time!(
            "query_upgraded_consensus_state",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        crate::telemetry!(query, self.id(), "query_upgraded_consensus_state");

        let upgrade_height = request.upgrade_height;
        let query_height = upgrade_height
            .decrement()
            .map_err(|_| Error::invalid_height_no_source())?;

        // Fetch the consensus state and its proof.
        let (upgraded_consensus_state_raw, proof) = self.query_client_upgrade_state(
            ClientUpgradePath::UpgradedClientConsensusState(upgrade_height.revision_height()),
            query_height,
        )?;

        let consensus_state = AnyConsensusState::decode_vec(&upgraded_consensus_state_raw)
            .map_err(Error::conversion_from_any)?;

        Ok((consensus_state, proof))
    }

    fn query_consensus_state_heights(
        &self,
        request: QueryConsensusStateHeightsRequest,
    ) -> Result<Vec<ICSHeight>, Error> {
        self.block_on(query_consensus_state_heights(
            self.id(),
            &self.grpc_addr,
            request,
        ))
    }

    fn query_consensus_state(
        &self,
        request: QueryConsensusStateRequest,
        include_proof: IncludeProof,
    ) -> Result<(AnyConsensusState, Option<MerkleProof>), Error> {
        crate::time!(
            "query_consensus_state",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        crate::telemetry!(query, self.id(), "query_consensus_state");

        let res = self.query(
            ClientConsensusStatePath {
                client_id: request.client_id.clone(),
                epoch: request.consensus_height.revision_number(),
                height: request.consensus_height.revision_height(),
            },
            request.query_height,
            matches!(include_proof, IncludeProof::Yes),
        )?;

        let consensus_state = AnyConsensusState::decode_vec(&res.value).map_err(Error::decode)?;

        if !matches!(consensus_state, AnyConsensusState::Tendermint(_)) {
            return Err(Error::consensus_state_type_mismatch(
                ClientType::Tendermint,
                consensus_state.client_type(),
            ));
        }

        match include_proof {
            IncludeProof::Yes => {
                let proof = res.proof.ok_or_else(Error::empty_response_proof)?;
                Ok((consensus_state, Some(proof)))
            }
            IncludeProof::No => Ok((consensus_state, None)),
        }
    }

    /// Performs a `QueryClientConnectionsRequest` gRPC query to fetch all the connection
    /// identifiers associated with a given client.
    fn query_client_connections(
        &self,
        request: QueryClientConnectionsRequest,
    ) -> Result<Vec<ConnectionId>, Error> {
        crate::time!(
            "query_client_connections",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        crate::telemetry!(query, self.id(), "query_client_connections");

        let mut client = self
            .block_on(
                ibc_proto::ibc::core::connection::v1::query_client::QueryClient::connect(
                    self.grpc_addr.clone(),
                ),
            )
            .map_err(Error::grpc_transport)?;

        client = client
            .max_decoding_message_size(self.config().max_grpc_decoding_size.get_bytes() as usize);

        let request = tonic::Request::new(request.into());

        let response = match self.block_on(client.client_connections(request)) {
            Ok(res) => res.into_inner(),
            Err(e) if e.code() == tonic::Code::NotFound => return Ok(vec![]),
            Err(e) => return Err(Error::grpc_status(e, "query_client_connections".to_owned())),
        };

        let ids = response
            .connection_paths
            .iter()
            .filter_map(|id| {
                ConnectionId::from_str(id)
                    .map_err(|e| warn!("connection with ID {} failed parsing. Error: {}", id, e))
                    .ok()
            })
            .collect();

        Ok(ids)
    }

    /// Performs a `QueryConnectionsRequest` gRPC query to fetch all connections
    /// associated with the chain.
    fn query_connections(
        &self,
        request: QueryConnectionsRequest,
    ) -> Result<Vec<IdentifiedConnectionEnd>, Error> {
        crate::time!(
            "query_connections",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        crate::telemetry!(query, self.id(), "query_connections");

        let mut client = self
            .block_on(
                ibc_proto::ibc::core::connection::v1::query_client::QueryClient::connect(
                    self.grpc_addr.clone(),
                ),
            )
            .map_err(Error::grpc_transport)?;

        client = client
            .max_decoding_message_size(self.config().max_grpc_decoding_size.get_bytes() as usize);

        let request = tonic::Request::new(request.into());

        let response = self
            .block_on(client.connections(request))
            .map_err(|e| Error::grpc_status(e, "query_connections".to_owned()))?
            .into_inner();

        let connections = response
            .connections
            .into_iter()
            .filter_map(|co| {
                IdentifiedConnectionEnd::try_from(co.clone())
                    .map_err(|e| {
                        warn!(
                            "connection with ID {} failed parsing. Error: {}",
                            PrettyIdentifiedConnection(&co),
                            e
                        )
                    })
                    .ok()
            })
            .collect();

        Ok(connections)
    }

    fn query_connection(
        &self,
        request: QueryConnectionRequest,
        include_proof: IncludeProof,
    ) -> Result<(ConnectionEnd, Option<MerkleProof>), Error> {
        crate::time!(
            "query_connection",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        crate::telemetry!(query, self.id(), "query_connection");

        async fn do_query_connection(
            chain: &CosmosSdkChain,
            connection_id: &ConnectionId,
            height_query: QueryHeight,
        ) -> Result<ConnectionEnd, Error> {
            use ibc_proto::ibc::core::connection::v1 as connection;
            use tonic::IntoRequest;

            let mut client =
                connection::query_client::QueryClient::connect(chain.grpc_addr.clone())
                    .await
                    .map_err(Error::grpc_transport)?;

            client = client.max_decoding_message_size(
                chain.config().max_grpc_decoding_size.get_bytes() as usize,
            );

            let mut request = connection::QueryConnectionRequest {
                connection_id: connection_id.to_string(),
            }
            .into_request();

            let height_param = AsciiMetadataValue::try_from(height_query)?;

            request
                .metadata_mut()
                .insert("x-cosmos-block-height", height_param);

            let response = client.connection(request).await.map_err(|e| {
                if e.code() == tonic::Code::NotFound {
                    Error::connection_not_found(connection_id.clone())
                } else {
                    Error::grpc_status(e, "query_connection".to_owned())
                }
            })?;

            match response.into_inner().connection {
                Some(raw_connection) => {
                    let connection_end = raw_connection.try_into().map_err(Error::ics03)?;

                    Ok(connection_end)
                }
                None => {
                    // When no connection is found, the GRPC call itself should return
                    // the NotFound error code. Nevertheless even if the call is successful,
                    // the connection field may not be present, because in protobuf3
                    // everything is optional.
                    Err(Error::connection_not_found(connection_id.clone()))
                }
            }
        }

        match include_proof {
            IncludeProof::Yes => {
                let res = self.query(
                    ConnectionsPath(request.connection_id.clone()),
                    request.height,
                    true,
                )?;
                let connection_end =
                    ConnectionEnd::decode_vec(&res.value).map_err(Error::decode)?;

                Ok((
                    connection_end,
                    Some(res.proof.ok_or_else(Error::empty_response_proof)?),
                ))
            }
            IncludeProof::No => self
                .block_on(async {
                    do_query_connection(self, &request.connection_id, request.height).await
                })
                .map(|conn_end| (conn_end, None)),
        }
    }

    /// Performs a `QueryConnectionChannelsRequest` gRPC query in order to
    /// fetch all channels associated with a given connection.
    fn query_connection_channels(
        &self,
        request: QueryConnectionChannelsRequest,
    ) -> Result<Vec<IdentifiedChannelEnd>, Error> {
        crate::time!(
            "query_connection_channels",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        crate::telemetry!(query, self.id(), "query_connection_channels");

        let mut client = self
            .block_on(
                ibc_proto::ibc::core::channel::v1::query_client::QueryClient::connect(
                    self.grpc_addr.clone(),
                ),
            )
            .map_err(Error::grpc_transport)?;

        client = client
            .max_decoding_message_size(self.config().max_grpc_decoding_size.get_bytes() as usize);

        let request = tonic::Request::new(request.into());

        let response = self
            .block_on(client.connection_channels(request))
            .map_err(|e| Error::grpc_status(e, "query_connection_channels".to_owned()))?
            .into_inner();

        let height = self.query_chain_latest_height()?;

        let channels: Vec<IdentifiedChannelEnd> = response
            .channels
            .into_iter()
            .filter_map(|ch| {
                IdentifiedChannelEnd::try_from(ch.clone())
                    .map_err(|e| {
                        warn!(
                            "channel with ID {} failed parsing. Error: {}",
                            PrettyIdentifiedChannel(&ch),
                            e
                        )
                    })
                    .ok()
            })
            .map(|mut channel| {
                // If the channel is open, look for an upgrade in order to correctly set the
                // state to Open(Upgrading) or Open(NotUpgrading)
                if channel.channel_end.is_open()
                    && self
                        .query_upgrade(
                            QueryUpgradeRequest {
                                port_id: channel.port_id.to_string(),
                                channel_id: channel.channel_id.to_string(),
                            },
                            height,
                            IncludeProof::No,
                        )
                        .is_ok()
                {
                    channel.channel_end.state = State::Open(UpgradeState::Upgrading);
                }
                channel
            })
            .collect();

        Ok(channels)
    }

    /// Performs a `QueryChannelsRequest` gRPC query in order to fetch all channels
    /// associated with the chain.
    fn query_channels(
        &self,
        request: QueryChannelsRequest,
    ) -> Result<Vec<IdentifiedChannelEnd>, Error> {
        crate::time!(
            "query_channels",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        crate::telemetry!(query, self.id(), "query_channels");

        let mut client = self
            .block_on(
                ibc_proto::ibc::core::channel::v1::query_client::QueryClient::connect(
                    self.grpc_addr.clone(),
                ),
            )
            .map_err(Error::grpc_transport)?;

        client = client
            .max_decoding_message_size(self.config().max_grpc_decoding_size.get_bytes() as usize);

        let request = tonic::Request::new(request.into());

        let response = self
            .block_on(client.channels(request))
            .map_err(|e| Error::grpc_status(e, "query_channels".to_owned()))?
            .into_inner();

        let height = self.query_chain_latest_height()?;

        let channels = response
            .channels
            .into_iter()
            .filter_map(|ch| {
                IdentifiedChannelEnd::try_from(ch.clone())
                    .map_err(|e| {
                        warn!(
                            "channel with ID {} failed parsing. Error: {}",
                            PrettyIdentifiedChannel(&ch),
                            e
                        )
                    })
                    .ok()
            })
            .map(|mut channel| {
                // If the channel is open, look for an upgrade in order to correctly set the
                // state to Open(Upgrading) or Open(NotUpgrading)
                if channel.channel_end.is_open()
                    && self
                        .query_upgrade(
                            QueryUpgradeRequest {
                                port_id: channel.port_id.to_string(),
                                channel_id: channel.channel_id.to_string(),
                            },
                            height,
                            IncludeProof::No,
                        )
                        .is_ok()
                {
                    channel.channel_end.state = State::Open(UpgradeState::Upgrading);
                }
                channel
            })
            .collect();

        Ok(channels)
    }

    fn query_channel(
        &self,
        request: QueryChannelRequest,
        include_proof: IncludeProof,
    ) -> Result<(ChannelEnd, Option<MerkleProof>), Error> {
        crate::time!(
            "query_channel",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        crate::telemetry!(query, self.id(), "query_channel");

        let res = self.query(
            ChannelEndsPath(request.port_id.clone(), request.channel_id.clone()),
            request.height,
            matches!(include_proof, IncludeProof::Yes),
        )?;

        let mut channel_end = ChannelEnd::decode_vec(&res.value).map_err(Error::decode)?;

        if channel_end.is_open() {
            let height = match request.height {
                QueryHeight::Latest => self.query_chain_latest_height()?,
                QueryHeight::Specific(height) => height,
            };
            // In order to determine if the channel is Open upgrading or not the Upgrade is queried.
            // If an upgrade is ongoing then the query will succeed in finding an Upgrade.
            if self
                .query_upgrade(
                    QueryUpgradeRequest {
                        port_id: request.port_id.to_string(),
                        channel_id: request.channel_id.to_string(),
                    },
                    height,
                    IncludeProof::No,
                )
                .is_ok()
            {
                channel_end.state = State::Open(UpgradeState::Upgrading);
            }
        }

        match include_proof {
            IncludeProof::Yes => {
                let proof = res.proof.ok_or_else(Error::empty_response_proof)?;
                Ok((channel_end, Some(proof)))
            }
            IncludeProof::No => Ok((channel_end, None)),
        }
    }

    /// Performs a `QueryChannelClientStateRequest` gRPC query in order to fetch the client state
    /// associated with a given channel, if it exists.
    fn query_channel_client_state(
        &self,
        request: QueryChannelClientStateRequest,
    ) -> Result<Option<IdentifiedAnyClientState>, Error> {
        crate::time!(
            "query_channel_client_state",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        crate::telemetry!(query, self.id(), "query_channel_client_state");

        let mut client = self
            .block_on(
                ibc_proto::ibc::core::channel::v1::query_client::QueryClient::connect(
                    self.grpc_addr.clone(),
                ),
            )
            .map_err(Error::grpc_transport)?;

        client = client
            .max_decoding_message_size(self.config().max_grpc_decoding_size.get_bytes() as usize);

        let request = tonic::Request::new(request.into());

        let response = self
            .block_on(client.channel_client_state(request))
            .map_err(|e| Error::grpc_status(e, "query_channel_client_state".to_owned()))?
            .into_inner();

        let client_state: Option<IdentifiedAnyClientState> = response
            .identified_client_state
            .map_or_else(|| None, |proto_cs| proto_cs.try_into().ok());

        Ok(client_state)
    }

    fn query_packet_commitment(
        &self,
        request: QueryPacketCommitmentRequest,
        include_proof: IncludeProof,
    ) -> Result<(Vec<u8>, Option<MerkleProof>), Error> {
        crate::time!(
            "query_packet_commitment",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        let res = self.query(
            CommitmentsPath {
                port_id: request.port_id,
                channel_id: request.channel_id,
                sequence: request.sequence,
            },
            request.height,
            matches!(include_proof, IncludeProof::Yes),
        )?;

        match include_proof {
            IncludeProof::Yes => {
                let proof = res.proof.ok_or_else(Error::empty_response_proof)?;

                Ok((res.value, Some(proof)))
            }
            IncludeProof::No => Ok((res.value, None)),
        }
    }

    /// Performs a `QueryPacketCommitmentsRequest` gRPC query to fetch the packet commitment
    /// hashes associated with a channel.
    fn query_packet_commitments(
        &self,
        request: QueryPacketCommitmentsRequest,
    ) -> Result<(Vec<Sequence>, ICSHeight), Error> {
        crate::time!(
            "query_packet_commitments",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        crate::telemetry!(query, self.id(), "query_packet_commitments");

        let mut client = self
            .block_on(
                ibc_proto::ibc::core::channel::v1::query_client::QueryClient::connect(
                    self.grpc_addr.clone(),
                ),
            )
            .map(|client| {
                client.max_decoding_message_size(
                    self.config().max_grpc_decoding_size.get_bytes() as usize
                )
            })
            .map_err(Error::grpc_transport)?;

        if request.pagination.is_enabled() {
            let mut results = Vec::new();
            let mut page_key = Vec::new();

            let pagination_information = request.pagination.get_values();
            let mut current_results = 0;

            loop {
                crate::time!(
                    "query_packet_commitments_loop_iteration",
                    {
                        "src_chain": self.config().id.to_string(),
                    }
                );
                let mut raw_request =
                    ibc_proto::ibc::core::channel::v1::QueryPacketCommitmentsRequest::from(
                        request.clone(),
                    );

                if let Some(pagination) = raw_request.pagination.as_mut() {
                    pagination.key = page_key;
                }

                let mut tonic_request = tonic::Request::new(raw_request);
                // TODO: This should either be configurable or inferred from the pagination
                tonic_request.set_timeout(Duration::from_secs(10));

                let response = self.rt.block_on(async {
                    client
                        .packet_commitments(tonic_request)
                        .await
                        .map_err(|e| Error::grpc_status(e, "query_packet_commitments".to_owned()))
                });

                match response {
                    Ok(response) => {
                        let inner_response = response.into_inner().clone();
                        let next_key = inner_response
                            .pagination
                            .as_ref()
                            .map(|p| p.next_key.clone());

                        results.push(Ok(inner_response));
                        current_results += pagination_information.0;

                        match next_key {
                            Some(next_key) if !next_key.is_empty() => {
                                page_key = next_key;
                            }
                            _ => break,
                        }
                    }
                    Err(e) => {
                        results.push(Err(e));
                        break;
                    }
                }
                if current_results >= pagination_information.1 {
                    break;
                }
            }

            let responses = results.into_iter().collect::<Result<Vec<_>, _>>()?;

            let mut commitment_sequences = Vec::new();

            for response in &responses {
                commitment_sequences.extend(
                    response
                        .commitments
                        .iter()
                        .map(|commit| Sequence::from(commit.sequence)),
                );
            }

            let height = responses
                .first()
                .and_then(|res| res.height)
                .and_then(|raw_height| raw_height.try_into().ok())
                .ok_or_else(|| Error::grpc_response_param("height".to_string()))?;

            Ok((commitment_sequences, height))
        } else {
            let request = tonic::Request::new(request.into());
            let response = self
                .block_on(client.packet_commitments(request))
                .map_err(|e| Error::grpc_status(e, "query_packet_commitments".to_owned()))?
                .into_inner();

            let mut commitment_sequences: Vec<Sequence> = response
                .commitments
                .into_iter()
                .map(|v| v.sequence.into())
                .collect();
            commitment_sequences.sort_unstable();

            let height = response
                .height
                .and_then(|raw_height| raw_height.try_into().ok())
                .ok_or_else(|| Error::grpc_response_param("height".to_string()))?;

            Ok((commitment_sequences, height))
        }
    }

    fn query_packet_receipt(
        &self,
        request: QueryPacketReceiptRequest,
        include_proof: IncludeProof,
    ) -> Result<(Vec<u8>, Option<MerkleProof>), Error> {
        crate::time!(
            "query_packet_receipt",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        let res = self.query(
            ReceiptsPath {
                port_id: request.port_id,
                channel_id: request.channel_id,
                sequence: request.sequence,
            },
            request.height,
            matches!(include_proof, IncludeProof::Yes),
        )?;

        match include_proof {
            IncludeProof::Yes => {
                let proof = res.proof.ok_or_else(Error::empty_response_proof)?;

                Ok((res.value, Some(proof)))
            }
            IncludeProof::No => Ok((res.value, None)),
        }
    }

    /// Performs a `QueryUnreceivedPacketsRequest` gRPC query to fetch the unreceived packet sequences
    /// associated with a channel.
    fn query_unreceived_packets(
        &self,
        request: QueryUnreceivedPacketsRequest,
    ) -> Result<Vec<Sequence>, Error> {
        crate::time!(
            "query_unreceived_packets",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        crate::telemetry!(query, self.id(), "query_unreceived_packets");

        let mut client = self
            .block_on(
                ibc_proto::ibc::core::channel::v1::query_client::QueryClient::connect(
                    self.grpc_addr.clone(),
                ),
            )
            .map_err(Error::grpc_transport)?;

        client = client
            .max_decoding_message_size(self.config().max_grpc_decoding_size.get_bytes() as usize);

        let request = tonic::Request::new(request.into());

        let mut response = self
            .block_on(client.unreceived_packets(request))
            .map_err(|e| Error::grpc_status(e, "query_unreceived_packets".to_owned()))?
            .into_inner();

        response.sequences.sort_unstable();
        Ok(response
            .sequences
            .into_iter()
            .map(|seq| seq.into())
            .collect())
    }

    fn query_packet_acknowledgement(
        &self,
        request: QueryPacketAcknowledgementRequest,
        include_proof: IncludeProof,
    ) -> Result<(Vec<u8>, Option<MerkleProof>), Error> {
        crate::time!(
            "query_packet_acknowledgement",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        let res = self.query(
            AcksPath {
                port_id: request.port_id,
                channel_id: request.channel_id,
                sequence: request.sequence,
            },
            request.height,
            matches!(include_proof, IncludeProof::Yes),
        )?;

        match include_proof {
            IncludeProof::Yes => {
                let proof = res.proof.ok_or_else(Error::empty_response_proof)?;

                Ok((res.value, Some(proof)))
            }
            IncludeProof::No => Ok((res.value, None)),
        }
    }

    /// Performs a `QueryPacketAcknowledgementsRequest` gRPC query to fetch the packet acknowledgment
    /// hashes associated with a channel.
    fn query_packet_acknowledgements(
        &self,
        request: QueryPacketAcknowledgementsRequest,
    ) -> Result<(Vec<Sequence>, ICSHeight), Error> {
        crate::telemetry!(query, self.id(), "query_packet_acknowledgements");
        crate::time!(
            "query_packet_acknowledgements",
            {
                "src_chain": self.config().id.to_string(),
            }
        );

        if request.packet_commitment_sequences.is_empty() {
            return Ok((Vec::new(), self.query_chain_latest_height()?));
        }

        let mut client = self
            .block_on(
                ibc_proto::ibc::core::channel::v1::query_client::QueryClient::connect(
                    self.grpc_addr.clone(),
                ),
            )
            .map(|client| {
                client.max_decoding_message_size(
                    self.config().max_grpc_decoding_size.get_bytes() as usize
                )
            })
            .map_err(Error::grpc_transport)?;

        if request.pagination.is_enabled() {
            let mut results = Vec::new();
            let mut page_key = Vec::new();

            loop {
                let mut raw_request =
                    ibc_proto::ibc::core::channel::v1::QueryPacketAcknowledgementsRequest::from(
                        request.clone(),
                    );

                if let Some(pagination) = raw_request.pagination.as_mut() {
                    pagination.key = page_key;
                }

                let mut tonic_request = tonic::Request::new(raw_request);
                // TODO: This should either be configurable or inferred from the pagination
                tonic_request.set_timeout(Duration::from_secs(10));

                let response = self.rt.block_on(async {
                    client
                        .packet_acknowledgements(tonic_request)
                        .await
                        .map_err(|e| {
                            Error::grpc_status(e, "query_packet_acknowledgements".to_owned())
                        })
                });

                match response {
                    Ok(response) => {
                        let inner_response = response.into_inner().clone();
                        let next_key = inner_response
                            .pagination
                            .as_ref()
                            .map(|p| p.next_key.clone());

                        results.push(Ok(inner_response));

                        match next_key {
                            Some(next_key) if !next_key.is_empty() => {
                                page_key = next_key;
                            }
                            _ => break,
                        }
                    }
                    Err(e) => {
                        results.push(Err(e));
                        break;
                    }
                }
            }

            let responses = results.into_iter().collect::<Result<Vec<_>, _>>()?;

            let mut acks_sequences = Vec::new();

            for response in &responses {
                acks_sequences.extend(
                    response
                        .acknowledgements
                        .iter()
                        .map(|commit| Sequence::from(commit.sequence)),
                );
            }

            let height = responses
                .first()
                .and_then(|res| res.height)
                .and_then(|raw_height| raw_height.try_into().ok())
                .ok_or_else(|| Error::grpc_response_param("height".to_string()))?;

            Ok((acks_sequences, height))
        } else {
            let request = tonic::Request::new(request.into());
            let response = self
                .block_on(client.packet_acknowledgements(request))
                .map_err(|e| Error::grpc_status(e, "query_packet_commitments".to_owned()))?
                .into_inner();

            let mut acks_sequences: Vec<Sequence> = response
                .acknowledgements
                .into_iter()
                .map(|v| v.sequence.into())
                .collect();
            acks_sequences.sort_unstable();

            let height = response
                .height
                .and_then(|raw_height| raw_height.try_into().ok())
                .ok_or_else(|| Error::grpc_response_param("height".to_string()))?;

            Ok((acks_sequences, height))
        }
    }

    /// Performs a `QueryUnreceivedAcksRequest` gRPC query to fetch the unreceived acknowledgements
    /// sequences associated with a channel.
    fn query_unreceived_acknowledgements(
        &self,
        request: QueryUnreceivedAcksRequest,
    ) -> Result<Vec<Sequence>, Error> {
        crate::time!(
            "query_unreceived_acknowledgements",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        crate::telemetry!(query, self.id(), "query_unreceived_acknowledgements");

        let mut client = self
            .block_on(
                ibc_proto::ibc::core::channel::v1::query_client::QueryClient::connect(
                    self.grpc_addr.clone(),
                ),
            )
            .map_err(Error::grpc_transport)?;

        client = client
            .max_decoding_message_size(self.config().max_grpc_decoding_size.get_bytes() as usize);

        let request = tonic::Request::new(request.into());

        let mut response = self
            .block_on(client.unreceived_acks(request))
            .map_err(|e| Error::grpc_status(e, "query_unreceived_acknowledgements".to_owned()))?
            .into_inner();

        response.sequences.sort_unstable();
        Ok(response
            .sequences
            .into_iter()
            .map(|seq| seq.into())
            .collect())
    }

    /// Performs a `QueryNextSequenceReceiveRequest` gRPC query to fetch the sequence number of the next
    /// packet to be received at a specified height.
    fn query_next_sequence_receive(
        &self,
        request: QueryNextSequenceReceiveRequest,
        include_proof: IncludeProof,
    ) -> Result<(Sequence, Option<MerkleProof>), Error> {
        crate::time!(
            "query_next_sequence_receive",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        crate::telemetry!(query, self.id(), "query_next_sequence_receive");

        let prove = include_proof.to_bool();

        let res = self.query(
            SeqRecvsPath(request.port_id, request.channel_id),
            request.height,
            true,
        )?;

        // Note: We expect the return to be a u64 encoded in big-endian. Refer to ibc-go:
        // https://github.com/cosmos/ibc-go/blob/25767f6bdb5bab2c2a116b41d92d753c93e18121/modules/core/04-channel/client/utils/utils.go#L191
        if res.value.len() != 8 {
            return Err(Error::query(format!(
                "next_sequence_receive: expected a u64 but got {} bytes of data",
                res.value.len()
            )));
        }

        let seq: Sequence = Bytes::from(res.value).get_u64().into();

        let proof = if prove {
            Some(res.proof.ok_or_else(Error::empty_response_proof)?)
        } else {
            None
        };

        Ok((seq, proof))
    }

    /// This function queries transactions for events matching certain criteria.
    /// 1. Client Update request - returns a vector with at most one update client event
    /// 2. Transaction event request - returns all IBC events resulted from a Tx execution
    fn query_txs(&self, request: QueryTxRequest) -> Result<Vec<IbcEventWithHeight>, Error> {
        crate::telemetry!(query, self.id(), "query_txs");

        self.block_on(query_txs(
            self.id(),
            &self.rpc_client,
            &self.config.rpc_addr,
            request,
        ))
    }

    /// This function queries transactions for packet events matching certain criteria.
    /// It returns at most one packet event for each sequence specified in the request.
    ///    Note - there is no way to format the packet query such that it asks for Tx-es with either
    ///    sequence (the query conditions can only be AND-ed).
    ///    There is a possibility to include "<=" and ">=" conditions but it doesn't work with
    ///    string attributes (sequence is emitted as a string).
    ///    Therefore, for packets we perform one tx_search for each sequence.
    ///    Alternatively, a single query for all packets could be performed but it would return all
    ///    packets ever sent.
    fn query_packet_events(
        &self,
        mut request: QueryPacketEventDataRequest,
    ) -> Result<Vec<IbcEventWithHeight>, Error> {
        crate::time!(
            "query_packet_events",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        crate::telemetry!(query, self.id(), "query_packet_events");

        match request.height {
            // Usage note: `Qualified::Equal` is currently only used in the call hierarchy involving
            // the CLI methods, namely the CLI for `tx packet-recv` and `tx packet-ack` when the
            // user passes the flag `packet-data-query-height`.
            Qualified::Equal(_) => self.block_on(query_packets_from_block(
                self.id(),
                &self.rpc_client,
                &self.config.rpc_addr,
                &request,
            )),
            Qualified::SmallerEqual(_) => {
                let tx_events = self.block_on(query_packets_from_txs(
                    self.id(),
                    &self.rpc_client,
                    &self.config.rpc_addr,
                    &request,
                ))?;

                let recvd_sequences: Vec<_> = tx_events
                    .iter()
                    .filter_map(|eh| eh.event.packet().map(|p| p.sequence))
                    .collect();

                request
                    .sequences
                    .retain(|seq| !recvd_sequences.contains(seq));

                let (start_block_events, end_block_events) = if !request.sequences.is_empty() {
                    self.query_packets_from_blocks(&request)?
                } else {
                    Default::default()
                };

                trace!("start_block_events {:?}", start_block_events);
                trace!("tx_events {:?}", tx_events);
                trace!("end_block_events {:?}", end_block_events);

                // Events should be ordered in the following fashion,
                // for any two blocks b1, b2 at height h1, h2 with h1 < h2:
                // b1.start_block_events
                // b1.tx_events
                // b1.end_block_events
                // b2.start_block_events
                // b2.tx_events
                // b2.end_block_events
                //
                // As of now, we just sort them by sequence number which should
                // yield a similar result and will revisit this approach in the future.
                let mut events = vec![];
                events.extend(start_block_events);
                events.extend(tx_events);
                events.extend(end_block_events);

                sort_events_by_sequence(&mut events);

                Ok(events)
            }
        }
    }

    fn query_host_consensus_state(
        &self,
        request: QueryHostConsensusStateRequest,
    ) -> Result<Self::ConsensusState, Error> {
        crate::time!(
            "query_host_consensus_state",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        let height = match request.height {
            QueryHeight::Latest => TmHeight::from(0u32),
            QueryHeight::Specific(ibc_height) => TmHeight::from(ibc_height),
        };

        let header = if height.value() == 0 {
            self.block_on(async {
                self.rpc_client
                    .latest_block()
                    .await
                    .map(|response| response.block.header)
            })
        } else {
            self.block_on(async {
                self.rpc_client
                    .header(height)
                    .await
                    .map(|response| response.header)
            })
        };

        let header = header.map_err(|e| Error::rpc(self.config.rpc_addr.clone(), e))?;
        Ok(header.into())
    }

    fn build_client_state(
        &self,
        height: ICSHeight,
        settings: ClientSettings,
    ) -> Result<Self::ClientState, Error> {
        crate::time!(
            "build_client_state",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        let ClientSettings::Tendermint(settings) = settings;
        let unbonding_period = self.unbonding_period()?;
        let trusting_period = settings
            .trusting_period
            .unwrap_or_else(|| self.trusting_period(unbonding_period));

        let proof_specs = self.config.proof_specs.clone().unwrap_or_default();

        // Build the client state.
        TmClientState::new(
            self.id().clone(),
            settings.trust_threshold,
            trusting_period,
            unbonding_period,
            settings.max_clock_drift,
            height,
            proof_specs,
            vec!["upgrade".to_string(), "upgradedIBCState".to_string()],
            AllowUpdate {
                after_expiry: true,
                after_misbehaviour: true,
            },
        )
        .map_err(Error::ics07)
    }

    fn build_consensus_state(
        &self,
        light_block: Self::LightBlock,
    ) -> Result<Self::ConsensusState, Error> {
        crate::time!(
            "build_consensus_state",
            {
                "src_chain": self.config().id.to_string(),
            }
        );

        Ok(TmConsensusState::from(light_block.signed_header.header))
    }

    fn build_header(
        &mut self,
        trusted_height: ICSHeight,
        target_height: ICSHeight,
        client_state: &AnyClientState,
    ) -> Result<(Self::Header, Vec<Self::Header>), Error> {
        crate::time!(
            "build_header",
            {
                "src_chain": self.config().id.to_string(),
            }
        );

        let now = self.chain_status()?.sync_info.latest_block_time;

        // Get the light block at target_height from chain.
        let Verified { target, supporting } = self.light_client.header_and_minimal_set(
            trusted_height,
            target_height,
            client_state,
            now,
        )?;

        Ok((target, supporting))
    }

    fn maybe_register_counterparty_payee(
        &mut self,
        channel_id: &ChannelId,
        port_id: &PortId,
        counterparty_payee: &Signer,
    ) -> Result<(), Error> {
        let address = self.get_signer()?;
        let key_pair = self.key()?;

        let memo_prefix = if let Some(memo_overwrite) = &self.config.memo_overwrite {
            memo_overwrite.clone()
        } else {
            self.config.memo_prefix.clone()
        };

        self.rt.block_on(maybe_register_counterparty_payee(
            &self.rpc_client,
            &self.tx_config,
            &key_pair,
            &mut self.account,
            &memo_prefix,
            channel_id,
            port_id,
            &address,
            counterparty_payee,
        ))
    }

    fn cross_chain_query(
        &self,
        requests: Vec<CrossChainQueryRequest>,
    ) -> Result<Vec<CrossChainQueryResponse>, Error> {
        let tasks = requests
            .into_iter()
            .map(|req| cross_chain_query_via_rpc(&self.rpc_client, req))
            .collect::<Vec<_>>();

        let joined_tasks = join_all(tasks);
        let results: Vec<Result<CrossChainQueryResponse, _>> = self.rt.block_on(joined_tasks);
        let responses = results
            .into_iter()
            .filter_map(|req| req.ok())
            .collect::<Vec<CrossChainQueryResponse>>();

        Ok(responses)
    }

    fn query_incentivized_packet(
        &self,
        request: QueryIncentivizedPacketRequest,
    ) -> Result<QueryIncentivizedPacketResponse, Error> {
        let incentivized_response =
            self.block_on(query_incentivized_packet(&self.grpc_addr, request))?;
        Ok(incentivized_response)
    }

    fn query_consumer_chains(&self) -> Result<Vec<(ChainId, ClientId)>, Error> {
        crate::time!(
            "query_consumer_chains",
            {
                "src_chain": self.config().id.to_string(),
            }
        );
        crate::telemetry!(query, self.id(), "query_consumer_chains");

        let mut client = self.block_on(
            ibc_proto::interchain_security::ccv::provider::v1::query_client::QueryClient::connect(
                self.grpc_addr.clone(),
            ),
        )
        .map_err(Error::grpc_transport)?;

        let request = tonic::Request::new(
            ibc_proto::interchain_security::ccv::provider::v1::QueryConsumerChainsRequest {},
        );

        let response = self
            .block_on(client.query_consumer_chains(request))
            .map_err(|e| Error::grpc_status(e, "query_consumer_chains".to_owned()))?
            .into_inner();

        let result = response
            .chains
            .into_iter()
            .map(|c| (c.chain_id.parse().unwrap(), c.client_id.parse().unwrap()))
            .collect();

        Ok(result)
    }

    fn query_upgrade(
        &self,
        request: QueryUpgradeRequest,
        height: Height,
        include_proof: IncludeProof,
    ) -> Result<(Upgrade, Option<MerkleProof>), Error> {
        let port_id = PortId::from_str(&request.port_id)
            .map_err(|_| Error::invalid_port_string(request.port_id))?;
        let channel_id = ChannelId::from_str(&request.channel_id)
            .map_err(|_| Error::invalid_channel_string(request.channel_id))?;
        let res = self.query(
            ChannelUpgradePath {
                port_id,
                channel_id,
            },
            QueryHeight::Specific(height),
            true,
        )?;
        let upgrade = Upgrade::decode_vec(&res.value).map_err(Error::decode)?;

        match include_proof {
            IncludeProof::Yes => {
                let proof = res.proof.ok_or_else(Error::empty_response_proof)?;
                Ok((upgrade, Some(proof)))
            }
            IncludeProof::No => Ok((upgrade, None)),
        }
    }

    fn query_upgrade_error(
        &self,
        request: QueryUpgradeErrorRequest,
        height: Height,
        include_proof: IncludeProof,
    ) -> Result<(ErrorReceipt, Option<MerkleProof>), Error> {
        let port_id = PortId::from_str(&request.port_id)
            .map_err(|_| Error::invalid_port_string(request.port_id))?;
        let channel_id = ChannelId::from_str(&request.channel_id)
            .map_err(|_| Error::invalid_channel_string(request.channel_id))?;
        let res = self.query(
            ChannelUpgradeErrorPath {
                port_id,
                channel_id,
            },
            QueryHeight::Specific(height),
            true,
        )?;
        let error_receipt = ErrorReceipt::decode_vec(&res.value).map_err(Error::decode)?;

        match include_proof {
            IncludeProof::Yes => {
                let proof = res.proof.ok_or_else(Error::empty_response_proof)?;
                Ok((error_receipt, Some(proof)))
            }
            IncludeProof::No => Ok((error_receipt, None)),
        }
    }
}

fn sort_events_by_sequence(events: &mut [IbcEventWithHeight]) {
    events.sort_by(|a, b| {
        a.event
            .packet()
            .zip(b.event.packet())
            .map(|(pa, pb)| pa.sequence.cmp(&pb.sequence))
            .unwrap_or(Ordering::Equal)
    });
}

async fn fetch_node_info(
    rpc_client: &HttpClient,
    config: &config::CosmosSdkConfig,
) -> Result<node::Info, Error> {
    crate::time!("fetch_node_info",
    {
        "src_chain": config.id.to_string(),
    });

    rpc_client
        .status()
        .await
        .map(|s| s.node_info)
        .map_err(|e| Error::rpc(config.rpc_addr.clone(), e))
}

/// Returns the suffix counter for a CosmosSDK client id.
/// Returns `None` if the client identifier is malformed
/// and the suffix could not be parsed.
fn client_id_suffix(client_id: &ClientId) -> Option<u64> {
    client_id
        .as_str()
        .split('-')
        .last()
        .and_then(|e| e.parse::<u64>().ok())
}

/// Performs a health check on a Cosmos chain.
///
/// This health check checks on the following in this order:
/// 1. Checks on the self-reported health endpoint.
/// 2. Checks that the staking module maintains some historical entries such
///    that local header information is stored in the IBC state and thus
///    client proofs that are part of the connection handshake can be verified.
/// 3. Checks that transaction indexing is enabled.
/// 4. Checks that the chain identifier matches the network name.
/// 5. Checks that the underlying SDK and ibc-go versions are compatible.
/// 6. Checks that the `gas_price` parameter in Hermes is >= the `min_gas_price`
///    advertised by the node Hermes is connected to.
fn do_health_check(chain: &CosmosSdkChain) -> Result<(), Error> {
    let chain_id = chain.id();
    let grpc_address = chain.grpc_addr.to_string();
    let rpc_address = chain.config.rpc_addr.to_string();

    if !chain.config.excluded_sequences.map.is_empty() {
        for (channel_id, seqs) in chain.config.excluded_sequences.map.iter() {
            if !seqs.is_empty() {
                warn!(
                    "chain '{chain_id}' will not clear packets on channel '{channel_id}' with sequences: {}. \
                    Ignore this warning if this configuration is correct.", PrettySlice(seqs)
                );
            }
        }
    }

    chain.block_on(chain.rpc_client.health()).map_err(|e| {
        Error::health_check_json_rpc(
            chain_id.clone(),
            rpc_address.clone(),
            "/health".to_string(),
            e,
        )
    })?;

    let status = chain.chain_status()?;

    if status.node_info.other.tx_index != TxIndexStatus::On {
        return Err(Error::tx_indexing_disabled(chain_id.clone()));
    }

    if status.node_info.network.as_str() != chain_id.as_str() {
        // Log the error, continue optimistically
        error!(
            "/status endpoint from chain '{}' reports network identifier to be '{}'. \
            This is usually a sign of misconfiguration, please check your config.toml",
            chain_id, status.node_info.network
        );
    }

    let relayer_gas_price = &chain.config.gas_price;
    let node_min_gas_prices_result = chain.min_gas_price()?;

    match node_min_gas_prices_result {
        Some(node_min_gas_prices) if !node_min_gas_prices.is_empty() => {
            let mut found_matching_denom = false;

            for price in node_min_gas_prices {
                match relayer_gas_price.partial_cmp(&price) {
                    Some(Ordering::Less) => return Err(Error::gas_price_too_low(chain_id.clone())),
                    Some(_) => {
                        found_matching_denom = true;
                        break;
                    }
                    None => continue,
                }
            }

            if !found_matching_denom {
                warn!(
                        "chain '{}' has no minimum gas price of denomination '{}' \
                        that is strictly less than the `gas_price` specified for that chain in the Hermes configuration. \
                        This is usually a sign of misconfiguration, please check your chain and Hermes configurations",
                        chain_id, relayer_gas_price.denom
                    );
            }
        }

        Some(_) => warn!(
            "chain '{}' has no minimum gas price value configured for denomination '{}'. \
            This is usually a sign of misconfiguration, please check your chain and relayer configurations",
            chain_id, relayer_gas_price.denom
        ),

        None => warn!(
            "chain '{}' does not implement the `cosmos.base.node.v1beta1.Service/Params` endpoint. \
            It is impossible to check whether the chain's minimum-gas-prices matches the ones specified in config",
            chain_id,
        ),
    }

    let version_specs = chain.block_on(fetch_version_specs(&chain.config.id, &chain.grpc_addr))?;

    if let Err(diagnostic) = compatibility::run_diagnostic(&version_specs) {
        return Err(Error::compat_check_failed(
            chain_id.clone(),
            grpc_address,
            diagnostic.to_string(),
        ));
    }

    if chain.historical_entries()? == 0 {
        return Err(Error::no_historical_entries(chain_id.clone()));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::calculate_fee;
    use crate::config::GasPrice;

    #[test]
    fn mul_ceil() {
        // Because 0.001 cannot be expressed precisely
        // as a 64-bit floating point number (it is
        // stored as 0.001000000047497451305389404296875),
        // `num_rational::BigRational` will represent it as
        // 1152921504606847/1152921504606846976 instead
        // which will sometimes round up to the next
        // integer in the computations below.
        // This is not a problem for the way we compute the fee
        // and gas adjustment as those are already based on simulated
        // gas which is not 100% precise.
        assert_eq!(super::mul_ceil(300_000, 0.001), 301.into());
        assert_eq!(super::mul_ceil(300_004, 0.001), 301.into());
        assert_eq!(super::mul_ceil(300_040, 0.001), 301.into());
        assert_eq!(super::mul_ceil(300_400, 0.001), 301.into());
        assert_eq!(super::mul_ceil(304_000, 0.001), 305.into());
        assert_eq!(super::mul_ceil(340_000, 0.001), 341.into());
        assert_eq!(super::mul_ceil(340_001, 0.001), 341.into());
    }

    /// Before https://github.com/informalsystems/hermes/pull/1568,
    /// this test would have panic'ed with:
    ///
    /// thread 'chain::cosmos::tests::fee_overflow' panicked at 'attempt to multiply with overflow'
    #[test]
    fn fee_overflow() {
        let gas_amount = 90000000000000_u64;
        let gas_price = GasPrice {
            price: 1000000000000.0,
            denom: "uatom".to_string(),
        };

        let fee = calculate_fee(gas_amount, &gas_price);
        assert_eq!(&fee.amount, "90000000000000000000000000");
    }
}

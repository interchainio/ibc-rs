use ibc_relayer::config::{self, ChainConfig, ModeConfig};
use ibc_relayer_types::core::ics03_connection::connection::State as ConnectionState;
use ibc_relayer_types::core::ics04_channel::channel::State as ChannelState;

use ibc_test_framework::bootstrap::binary::chain::bootstrap_foreign_client_pair;
use ibc_test_framework::bootstrap::binary::channel::{
    bootstrap_channel_with_chains, bootstrap_channel_with_connection,
};
use ibc_test_framework::bootstrap::binary::connection::bootstrap_connection;
use ibc_test_framework::prelude::*;
use ibc_test_framework::relayer::channel::{
    assert_eventually_channel_established, init_channel, query_channel_end,
};
use ibc_test_framework::relayer::connection::{
    assert_eventually_connection_established, init_connection, query_connection_end,
};
use ibc_test_framework::relayer::refresh::spawn_refresh_client_tasks;

// The cosmos ChainHandle handles requests in serial, and a refresh client
// request may get blocked by other operations and cause the refresh to fail
// if the expiry time is too short.
const CLIENT_EXPIRY: Duration = Duration::from_secs(15);

/**
    A test to verify that the connection and channel workers are properly
    terminated instead of looping indefinitely when it is not possible to
    perform handshake due to the client being expired or frozen.

    Since the test involves long-running background tasks, it has to
    be verified manually by inspecting the logs. Run the test with
    the following command:

    ```bash
    RUST_BACKTRACE=0 RUST_LOG=info cargo test \
        -p ibc-integration-test -- test_channel_expiration
    ```

    And you should see error logs such as:

    ```log
    ERROR ibc_relayer::connection: failed to establish connection handshake on frozen client:
    0: failed during an operation on client (07-tendermint-0) hosted by chain (ibc-beta-6fe01a9b)
    1: client 07-tendermint-0 on chain id ibc-beta-6fe01a9b is expired or frozen
    ERROR ibc_relayer::util::task: aborting task ConnectionWorker(connection::connection-1:ibc-beta-6fe01a9b -> ibc-alpha-43544e24) after encountering fatal error:
    0: Worker failed after 1 retries
    INFO ibc_relayer::util::task: task ConnectionWorker(connection::connection-1:ibc-beta-6fe01a9b -> ibc-alpha-43544e24) has terminated
    ```

    The error messages should not repeat more than once. In the original code,
    the connection worker would keep retrying and indefinitely flooding
    the log with errors.
*/
#[test]
fn test_channel_expiration() -> Result<(), Error> {
    run_binary_chain_test(&ChannelExpirationTest)
}

#[test]
fn test_packet_expiration() -> Result<(), Error> {
    run_binary_chain_test(&PacketExpirationTest)
}

#[test]
fn test_create_on_expired_client() -> Result<(), Error> {
    run_binary_chain_test(&CreateOnExpiredClientTest)
}

#[cfg(feature = "manual")]
#[test]
fn test_misbehavior_expiration() -> Result<(), Error> {
    run_binary_chain_test(&MisbehaviorExpirationTest)
}

fn wait_for_client_expiry() {
    let sleep_time = CLIENT_EXPIRY + Duration::from_secs(5);

    info!(
        "Sleeping for {} seconds to wait for IBC client to expire",
        sleep_time.as_secs()
    );

    sleep(sleep_time);
}

pub struct ExpirationTestOverrides;

pub struct ChannelExpirationTest;

pub struct PacketExpirationTest;

pub struct CreateOnExpiredClientTest;

pub struct MisbehaviorExpirationTest;

impl TestOverrides for ExpirationTestOverrides {
    fn modify_relayer_config(&self, config: &mut Config) {
        config.mode = ModeConfig {
            clients: config::Clients {
                enabled: true,
                refresh: true,
                misbehaviour: true,
            },
            connections: config::Connections { enabled: true },
            channels: config::Channels { enabled: true },
            packets: config::Packets {
                enabled: true,
                clear_interval: 10,
                clear_on_start: true,
                tx_confirmation: true,
                ..Default::default()
            },
        };

        for chain_config in config.chains.iter_mut() {
            match chain_config {
                ChainConfig::CosmosSdk(chain_config) => {
                    chain_config.trusting_period = Some(CLIENT_EXPIRY);
                }
            }
        }
    }

    fn should_spawn_supervisor(&self) -> bool {
        false
    }
}

impl BinaryChainTest for ChannelExpirationTest {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
    ) -> Result<(), Error> {
        let connection = {
            let _refresh_tasks = spawn_refresh_client_tasks(&chains.foreign_clients)?;

            bootstrap_connection(&chains.foreign_clients, Default::default())?
        };

        wait_for_client_expiry();

        relayer.with_supervisor(|| {
            let port_a = tagged_transfer_port();
            let port_b = tagged_transfer_port();

            {
                info!("Trying to create connection and channel after client is expired");

                match init_connection(
                    &chains.handle_a,
                    &chains.handle_b,
                    &chains.client_id_a(),
                    &chains.client_id_b(),
                ) {
                    // Up to simapp v7.0.0 sending a `ConnOpenInit` with an expired client would not
                    // return an error so it is required to assert the connection and channel remain
                    // in the init state.
                    Ok((connection_id_b, _)) => {

                        let (channel_id_b, _) = init_channel(
                            &chains.handle_a,
                            &chains.handle_b,
                            &chains.client_id_a(),
                            &chains.client_id_b(),
                            &connection.connection_id_a.as_ref(),
                            &connection.connection_id_b.as_ref(),
                            &port_a.as_ref(),
                            &port_b.as_ref(),
                        )?;


                    info!("Sleeping for 10 seconds to make sure that connection and channel fails to establish");

                    sleep(Duration::from_secs(10));

                    {
                        let connection_end_b =
                            query_connection_end(&chains.handle_b, &connection_id_b.as_ref())?;

                        assert_eq(
                            "connection end status should remain init",
                            connection_end_b.value().state(),
                            &ConnectionState::Init,
                        )?;

                        assert_eq(
                            "connection end should not have counterparty",
                            &connection_end_b.tagged_counterparty_connection_id(),
                            &None,
                        )?;
                    }


                {
                    let channel_end_b =
                        query_channel_end(&chains.handle_b, &channel_id_b.as_ref(), &port_b.as_ref())?;

                    assert_eq(
                        "channel end status should remain init",
                        channel_end_b.value().state(),
                        &ChannelState::Init,
                    )?;

                    assert_eq(
                        "channel end should not have counterparty",
                        &channel_end_b.tagged_counterparty_channel_id(),
                        &None,
                    )?;
                }

                    },
                    // From simapp version v7.1.0 if `ConnOpenInit` is sent while the client
                    // is expired, an error will be returned.
                    // See https://github.com/cosmos/ibc-go/blob/v7.1.0/modules/core/03-connection/keeper/handshake.go#L40
                    Err(e) => assert!(e.to_string().contains("status is Expired: client state is not active")),
                }
            }

            {
                info!(
                    "Trying to create new channel and worker after previous connection worker failed"
                );

                let foreign_clients_2 = bootstrap_foreign_client_pair(
                    &chains.handle_a,
                    &chains.handle_b,
                    Default::default(),
                )?;

                // Need to spawn refresh client for new clients to make sure they don't expire

                let _refresh_tasks = spawn_refresh_client_tasks(&foreign_clients_2)?;

                let (connection_id_b, _) = init_connection(
                    &chains.handle_a,
                    &chains.handle_b,
                    &foreign_clients_2.client_b_to_a.tagged_client_id(),
                    &foreign_clients_2.client_a_to_b.tagged_client_id(),
                )?;

                let connection_id_a = assert_eventually_connection_established(
                    &chains.handle_b,
                    &chains.handle_a,
                    &connection_id_b.as_ref(),
                )?;

                let (channel_id_b_2, _) = init_channel(
                    &chains.handle_a,
                    &chains.handle_b,
                    &foreign_clients_2.client_b_to_a.tagged_client_id(),
                    &foreign_clients_2.client_a_to_b.tagged_client_id(),
                    &connection_id_a.as_ref(),
                    &connection_id_b.as_ref(),
                    &port_a.as_ref(),
                    &port_b.as_ref(),
                )?;

                // At this point the misbehavior task may raise error, because it
                // try to check on a client update event that is already expired.
                // This happens because the misbehavior task is only started when
                // there is at least one channel in it, _not_ when the client
                // is created.
                //
                // Source of error:
                // https://github.com/informalsystems/tendermint-rs/blob/c45ea8c82773de1946f7ae2eece13150f07ca5fe/light-client/src/light_client.rs#L216-L222

                assert_eventually_channel_established(
                    &chains.handle_b,
                    &chains.handle_a,
                    &channel_id_b_2.as_ref(),
                    &port_b.as_ref(),
                )?;
            }

            Ok(())
        })
    }
}

impl BinaryChainTest for PacketExpirationTest {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
    ) -> Result<(), Error> {
        let channels = {
            let _refresh_tasks = spawn_refresh_client_tasks(&chains.foreign_clients)?;

            bootstrap_channel_with_chains(
                &chains,
                &PortId::transfer(),
                &PortId::transfer(),
                Default::default(),
                Default::default(),
            )?
        };

        chains.node_a.chain_driver().ibc_transfer_token(
            &channels,
            &chains.node_a.wallets().user1(),
            &chains.node_b.wallets().user1().address(),
            &vec![chains.node_a.denom().with_amount(100u64).as_ref()],
        )?;

        wait_for_client_expiry();

        info!("Packet worker should fail after client expires");

        relayer.with_supervisor(|| {
            let denom_a = chains.node_a.denom();

            let denom_b = derive_ibc_denom(
                &channels.port_b.as_ref(),
                &channels.channel_id_b.as_ref(),
                &denom_a,
            )?;

            sleep(Duration::from_secs(10));

            let balance_b = chains.node_b.chain_driver().query_balance(
                &chains.node_b.wallets().user1().address(),
                &denom_b.as_ref(),
            )?;

            assert_eq(
                "balance on wallet B should remain zero",
                &balance_b.amount(),
                &0u64.into(),
            )?;

            Ok(())
        })
    }
}

impl BinaryChainTest for CreateOnExpiredClientTest {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        _relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
    ) -> Result<(), Error> {
        // Create a connection before the IBC client expires, so that we can try create
        // new channel with the connection after the client expired.
        let connection = {
            let _refresh_tasks = spawn_refresh_client_tasks(&chains.foreign_clients)?;

            bootstrap_connection(&chains.foreign_clients, Default::default())?
        };

        wait_for_client_expiry();

        info!("trying to bootstrap connection after IBC client is expired");

        let res = bootstrap_connection(&chains.foreign_clients, Default::default());

        match res {
            Ok(_) => {
                return Err(Error::generic(eyre!(
                    "expected bootstrap_connection to fail"
                )))
            }
            Err(e) => {
                info!("bootstrap_connection failed with expected error {}", e);
            }
        }

        sleep(Duration::from_secs(5));

        info!("trying to bootstrap channel after IBC client is expired");

        let res = bootstrap_channel_with_connection(
            &chains.handle_a,
            &chains.handle_b,
            connection,
            &DualTagged::new(&PortId::transfer()),
            &DualTagged::new(&PortId::transfer()),
            Default::default(),
        );

        match res {
            Ok(_) => {
                return Err(Error::generic(eyre!(
                    "expected bootstrap_channel_with_connection to fail"
                )))
            }
            Err(e) => {
                info!(
                    "bootstrap_channel_with_connection failed with expected error {}",
                    e
                );
            }
        }

        Ok(())
    }
}

impl BinaryChainTest for MisbehaviorExpirationTest {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        _relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
    ) -> Result<(), Error> {
        /*
           This test reproduce the error log when a misbehavior task is
           first started. The error arise when `detect_misbehaviour_and_submit_evidence`
           is called with `None`, and the initial headers are already expired.

           Run this test with the `manual` feature and log level `trace`:

           ```text
           $ RUST_BACKTRACE=0 RUST_LOG=trace cargo test --features manual -p ibc-integration-test -- test_misbehavior_expiration
           ```

           and logs such as follow will be shown:

           ```log
           TRACE ibc_relayer::foreign_client: [ibc-beta-96682bb3 -> ibc-alpha-4095d39d:07-tendermint-0] checking misbehaviour for consensus state heights (first 50 shown here): 0-14, 0-9, 0-5, 0-3, total: 4
           TRACE ibc_relayer::light_client::tendermint: light client verification trusted=0-9 target=0-14
           TRACE ibc_relayer::light_client::tendermint: light client verification trusted=0-5 target=0-9
           TRACE ibc_relayer::light_client::tendermint: light client verification trusted=0-3 target=0-5
           WARN ibc_relayer::foreign_client: [ibc-beta-96682bb3 -> ibc-alpha-4095d39d:07-tendermint-0] misbehaviour checking result:
           0: error raised while checking for misbehaviour evidence: failed to check misbehaviour for 07-tendermint-0 at consensus height 0-5
           1: Light client error for RPC address ibc-beta-96682bb3
           2:
           2:    0: trusted state outside of trusting period
           2021-12-21T21:00:23.796731Z  INFO ibc_integration_test::tests::client_expiration: misbehavior result: ValidClient
           ```
        */

        {
            let _refresh_tasks = spawn_refresh_client_tasks(&chains.foreign_clients)?;

            // build a client header that will be expired
            chains
                .foreign_clients
                .client_b_to_a
                .build_latest_update_client_and_send()
                .map_err(handle_generic_error)?;

            info!("waiting for the initial client header to expire, while keeping the IBC client refreshed");

            wait_for_client_expiry();
        }

        // Calling detect_misbehaviour_and_submit_evidence(None) will always produce error logs
        for _ in 0..3 {
            let misbehavior_result = chains
                .foreign_clients
                .client_b_to_a
                .detect_misbehaviour_and_submit_evidence(None);

            info!("misbehavior result: {:?}", misbehavior_result);
        }

        suspend()
    }
}

impl HasOverrides for CreateOnExpiredClientTest {
    type Overrides = ExpirationTestOverrides;

    fn get_overrides(&self) -> &ExpirationTestOverrides {
        &ExpirationTestOverrides
    }
}

impl HasOverrides for ChannelExpirationTest {
    type Overrides = ExpirationTestOverrides;

    fn get_overrides(&self) -> &ExpirationTestOverrides {
        &ExpirationTestOverrides
    }
}

impl HasOverrides for PacketExpirationTest {
    type Overrides = ExpirationTestOverrides;

    fn get_overrides(&self) -> &ExpirationTestOverrides {
        &ExpirationTestOverrides
    }
}

impl HasOverrides for MisbehaviorExpirationTest {
    type Overrides = ExpirationTestOverrides;

    fn get_overrides(&self) -> &ExpirationTestOverrides {
        &ExpirationTestOverrides
    }
}

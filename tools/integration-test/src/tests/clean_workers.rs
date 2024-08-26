use ibc_relayer::config::{self, ModeConfig};
use ibc_relayer::object::ObjectType;
use ibc_test_framework::relayer::channel::init_channel;
use ibc_test_framework::{prelude::*, util::random::random_u128_range};

#[test]
fn test_clean_packet_workers() -> Result<(), Error> {
    run_binary_channel_test(&CleanPacketWorkersTest)
}

#[test]
fn test_clean_channel_workers() -> Result<(), Error> {
    run_binary_channel_test(&CleanChannelWorkersTest)
}

pub struct CleanPacketWorkersTest;

impl TestOverrides for CleanPacketWorkersTest {
    fn modify_relayer_config(&self, config: &mut Config) {
        config.mode = ModeConfig {
            clients: config::Clients {
                enabled: false,
                refresh: false,
                misbehaviour: false,
            },
            connections: config::Connections { enabled: false },
            channels: config::Channels { enabled: false },
            packets: config::Packets {
                enabled: true,
                clear_interval: 10,
                clear_on_start: false,
                tx_confirmation: false,
                ..Default::default()
            },
        };
    }

    fn should_spawn_supervisor(&self) -> bool {
        false
    }
}

impl BinaryChannelTest for CleanPacketWorkersTest {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channel: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        let denom_a = chains.node_a.denom();

        let wallet_a = chains.node_a.wallets().user1().cloned();
        let wallet_b = chains.node_b.wallets().user1().cloned();

        let amount1 = random_u128_range(1000, 5000);

        let supervisor = relayer.spawn_supervisor()?;

        // Transfer tokens so that the packet workers are spawned
        chains.node_a.chain_driver().ibc_transfer_token(
            &channel,
            &wallet_a.as_ref(),
            &wallet_b.address(),
            &vec![denom_a.with_amount(amount1).as_ref()],
        )?;

        // Assert the packet workers are correctly spawned
        assert_eventual_workers_removed(&supervisor, &ObjectType::Packet, 2)?;

        // Assert that idle packet workers are eventually removed
        assert_eventual_workers_removed(&supervisor, &ObjectType::Packet, 0)?;

        Ok(())
    }
}

pub struct CleanChannelWorkersTest;

impl TestOverrides for CleanChannelWorkersTest {
    fn modify_relayer_config(&self, config: &mut Config) {
        config.mode = ModeConfig {
            clients: config::Clients {
                enabled: false,
                refresh: false,
                misbehaviour: false,
            },
            connections: config::Connections { enabled: false },
            channels: config::Channels { enabled: true },
            packets: config::Packets {
                enabled: false,
                clear_interval: 10,
                clear_on_start: false,
                tx_confirmation: false,
                ..Default::default()
            },
        };
    }

    fn should_spawn_supervisor(&self) -> bool {
        false
    }
}

impl BinaryChannelTest for CleanChannelWorkersTest {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channel: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        let supervisor = relayer.spawn_supervisor()?;

        // Optimistically send chan-open-init to B, without a connection available yet on A
        init_channel(
            &chains.handle_a,
            &chains.handle_b,
            &chains.client_id_a(),
            &chains.client_id_b(),
            &channel.connection.connection_id_a.as_ref(),
            &channel.connection.connection_id_b.as_ref(),
            &channel.port_a.as_ref(),
            &channel.port_b.as_ref(),
        )?;

        // Assert that idle packet workers are eventually removed
        assert_eventual_workers_removed(&supervisor, &ObjectType::Channel, 0)?;

        Ok(())
    }
}

fn assert_eventual_workers_removed(
    supervisor: &SupervisorHandle,
    worker_type: &ObjectType,
    goal: usize,
) -> Result<(), Error> {
    assert_eventually_succeed(
        &format!("eventual {worker_type} workers"),
        50,
        Duration::from_secs(5),
        || {
            let state = supervisor.dump_state()?;

            let packet_workers_amount = if let Some(packet_workers) = state.workers.get(worker_type)
            {
                packet_workers.len()
            } else {
                0
            };
            if packet_workers_amount == goal {
                return Ok(());
            }
            Err(Error::generic(eyre!(
                "current number of workers `{packet_workers_amount}` does not match goal `{goal}`"
            )))
        },
    )?;

    Ok(())
}

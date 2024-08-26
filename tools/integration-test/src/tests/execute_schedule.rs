//! This test ensures that the `RelayPath::execute_schedule` method does not
//! drop any scheduled `OperationalData` when events associated with a prior
//! piece of operational data fails to send. Subsequent pieces of operational
//! data that were scheduled should be re-queued and not dropped.
//!
//! In order to test this behavior, the test manually relays a batch (i.e. at least
//! 2) IBC transfers from chain A to chain B. Chain B is then shut down in order to
//! force the batch of messages (in the form of their associated pieces of operational
//! data) to be queued up again for re-submission.
//!
//! It is expected that the first message of the batch gets dropped (i.e. it is not
//! later found in the pending queue), but all of the subsequent messages should
//! exist in the pending queue.

use ibc_test_framework::prelude::*;
use ibc_test_framework::util::random::random_u128_range;

use ibc_relayer::link::{Link, LinkParameters};

/// The number of messages to be sent in a batch contained in a piece of operational data.
const BATCH_SIZE: usize = 10;

#[test]
fn test_execute_schedule() -> Result<(), Error> {
    run_binary_channel_test(&ExecuteScheduleTest)
}

pub struct ExecuteScheduleTest;

impl TestOverrides for ExecuteScheduleTest {
    fn should_spawn_supervisor(&self) -> bool {
        false
    }
}

impl BinaryChannelTest for ExecuteScheduleTest {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channel: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        let amount1 = random_u128_range(1000, 5000);
        let packet_config = relayer.config.mode.packets;

        let chain_a_link_opts = LinkParameters {
            src_port_id: channel.port_a.clone().into_value(),
            src_channel_id: channel.channel_id_a.clone().into_value(),
            max_memo_size: packet_config.ics20_max_memo_size,
            max_receiver_size: packet_config.ics20_max_receiver_size,
            exclude_src_sequences: vec![],
        };

        let chain_a_link = Link::new_from_opts(
            chains.handle_a().clone(),
            chains.handle_b().clone(),
            chain_a_link_opts,
            true,
            false,
        )?;

        let mut relay_path_a_to_b = chain_a_link.a_to_b;

        // Construct `BATCH_SIZE` pieces of operational data and queue them up to be sent to chain B.
        for i in 0..BATCH_SIZE {
            chains.node_a.chain_driver().ibc_transfer_token(
                &channel,
                &chains.node_a.wallets().user1(),
                &chains.node_b.wallets().user1().address(),
                &vec![chains.node_a.denom().with_amount(amount1).as_ref()],
            )?;

            relay_path_a_to_b
                .schedule_packet_clearing(None, relayer.config.mode.packets.clear_limit)?;

            info!("Performing IBC send packet with a token transfer #{} from chain A to be received by chain B", i);
        }

        // We should see that all of the events in the batch are queued up to be sent to chain B.
        assert_eq!(relay_path_a_to_b.dst_operational_data.len(), BATCH_SIZE);

        chains.node_b.value().kill()?;

        // With chain B inactive, if we attempt to send the batch of messages, we expect to see
        // `BATCH_SIZE` - 1 messages in the batch since the initial event should have failed to
        // be relayed and was thus dropped. The subsequent messages in the batch should have all
        // been re-added to the pending queue.
        match relay_path_a_to_b.execute_schedule() {
            Ok(_) => panic!("Expected an error when relaying tx from A to B"),
            Err(_) => assert_eq!(relay_path_a_to_b.dst_operational_data.len(), BATCH_SIZE - 1),
        }

        Ok(())
    }
}

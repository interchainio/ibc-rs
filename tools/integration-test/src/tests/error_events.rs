use ibc_relayer::chain::tracking::TrackedMsgs;
use ibc_relayer_types::events::IbcEvent;
use ibc_test_framework::prelude::*;
use ibc_test_framework::relayer::transfer::build_transfer_message;

#[test]
fn test_error_events() -> Result<(), Error> {
    run_binary_channel_test(&ErrorEventsTest)
}

pub struct ErrorEventsTest;

impl TestOverrides for ErrorEventsTest {}

impl BinaryChannelTest for ErrorEventsTest {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        _relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channel: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        let denom_a = chains.node_a.denom();

        let wallet_a = chains.node_a.wallets().user1().cloned();
        let wallet_b = chains.node_b.wallets().user1().cloned();

        let balance_a = chains
            .node_a
            .chain_driver()
            .query_balance(&wallet_a.address(), &denom_a)?;

        // Create 4x transfer messages where each transfers
        // (1/3 + 1) of the total balance the user has.
        // So the third and fourth message should fail.

        let balance_a_amount: u128 = balance_a.value().amount.0.as_u128();

        let channel_version = channel.channel.src_version().ok_or_else(|| {
            Error::generic(eyre!(
                "failed to retrieve channel version for channel `{:#?}`",
                channel.channel.src_channel_id()
            ))
        })?;

        let transfer_message = build_transfer_message(
            &channel.port_a.as_ref(),
            &channel.channel_id_a.as_ref(),
            channel_version,
            &wallet_a.as_ref(),
            &wallet_b.address(),
            &vec![denom_a.with_amount((balance_a_amount / 3) + 1).as_ref()],
            Duration::from_secs(30),
            None,
        )?;

        let messages = TrackedMsgs::new_static(vec![transfer_message; 4], "test_error_events");

        let events = chains.handle_a().send_messages_and_wait_commit(messages)?;

        // We expect 4 error events to be returned, corresponding to the
        // 4 messages sent.

        assert_eq!(events.len(), 4);

        for event_with_height in events {
            match event_with_height.event {
                IbcEvent::ChainError(_) => {}
                _ => {
                    panic!("expect all events to be error events");
                }
            }
        }

        Ok(())
    }
}

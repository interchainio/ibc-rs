//! Tests the capability of the fee module to pay packet fees for a packet that has already
//! been sent.
//!
//! This test starts off by performing an `ibc_token_transfer_with_fee` with the relayer configured
//! to not spawn a supervisor. The token transfer will thus be in a pending un-relayed state, with
//! the fees locked in escrow (i.e. they've been debited from the source chain's balance).
//!
//! A token transfer with fee operation consists of two separate events: the event itself,
//! in the form of an `IbcEvent::SendPacket`, as well as a separate event containing the fees,
//! in the form of an `IbcEvent::IncentivizedPacket`. The test checks that these events' sequences
//! match up.
//!
//! The test then checks the behavior or the `pay_packet_fee` function, which pays some additional
//! fees to the relayer of the `IbcEvent::SendPacket` after it has already been sent. In this case,
//! calling `pay_packet_fee` doesn't construct a new `IbcEvent::IncentivizedPacket`; the additional
//! fees are added to the initial fees contained in the incentivized packet.
//!
//! Finally, the test initializes the supervisor in order to relay the pending packets so that the
//! balances on the two chains can be asserted.

use ibc_relayer_types::core::ics04_channel::version::Version;
use ibc_relayer_types::events::IbcEvent;
use ibc_test_framework::prelude::*;
use ibc_test_framework::util::random::random_u128_range;

#[test]
fn test_pay_packet_fee_async() -> Result<(), Error> {
    run_binary_channel_test(&PayPacketFeeAsyncTest)
}

struct PayPacketFeeAsyncTest;

impl TestOverrides for PayPacketFeeAsyncTest {
    fn should_spawn_supervisor(&self) -> bool {
        false
    }

    fn channel_version(&self) -> Version {
        Version::ics20_with_fee(1)
    }
}

impl BinaryChannelTest for PayPacketFeeAsyncTest {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channel: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        let chain_driver_a = chains.node_a.chain_driver();
        let chain_driver_b = chains.node_b.chain_driver();

        let chain_id_a = chain_driver_a.chain_id();
        let chain_id_b = chain_driver_b.chain_id();

        let denom_a = chains.node_a.denom();

        let port_a = channel.port_a.as_ref();
        let port_b = channel.port_b.as_ref();
        let channel_id_a = channel.channel_id_a.as_ref();

        let channel_id_b = channel.channel_id_b.as_ref();

        let wallets_a = chains.node_a.wallets();
        let wallets_b = chains.node_b.wallets();

        let relayer_a = wallets_a.relayer();
        let relayer_b = wallets_b.relayer();

        info!(
            "registering counterparty address of relayer {} on chain {} to be {} on chain {}",
            relayer_b.address(),
            chain_id_b,
            relayer_a.address(),
            chain_id_a
        );

        chain_driver_b.register_counterparty_payee(
            &relayer_b,
            &relayer_a.address(),
            &channel_id_b,
            &port_b,
        )?;

        let user_a = wallets_a.user1();

        let user_b = wallets_b.user1();

        let balance_a1 = chain_driver_a.query_balance(&user_a.address(), &denom_a)?;

        let relayer_balance_a = chain_driver_a.query_balance(&relayer_a.address(), &denom_a)?;

        let send_amount = random_u128_range(1000, 2000);
        let receive_fee = random_u128_range(300, 400);
        let ack_fee = random_u128_range(200, 300);
        let timeout_fee = random_u128_range(100, 200);

        let events = chain_driver_a.ibc_token_transfer_with_fee(
            &channel,
            &user_a,
            &user_b.address(),
            &vec![denom_a.with_amount(send_amount).as_ref()],
            &denom_a.with_amount(receive_fee).as_ref(),
            &denom_a.with_amount(ack_fee).as_ref(),
            &denom_a.with_amount(timeout_fee).as_ref(),
            Duration::from_secs(300),
        )?;

        let balance_a2 = balance_a1.clone() - send_amount;

        chain_driver_a.assert_eventual_escrowed_amount_ics29(
            &user_a.address(),
            &balance_a2.clone().as_ref(),
            receive_fee,
            ack_fee,
            timeout_fee,
        )?;

        let sequence = {
            let send_packet_event = events
                .iter()
                .find_map(|event| match &event.event {
                    IbcEvent::SendPacket(e) => Some(e),
                    _ => None,
                })
                .ok_or_else(|| eyre!("expect send packet event"))?;

            send_packet_event.packet.sequence
        };

        {
            let event = events
                .iter()
                .find_map(|ev| {
                    if let IbcEvent::IncentivizedPacket(ev) = &ev.event {
                        Some(ev)
                    } else {
                        None
                    }
                })
                .ok_or_else(|| eyre!("expect incentivized packet event"))?;

            assert_eq!(event.sequence, sequence);

            assert_eq!(event.total_recv_fee.len(), 1);
            assert_eq!(event.total_ack_fee.len(), 1);
            assert_eq!(event.total_timeout_fee.len(), 1);

            assert_eq!(
                &event.total_recv_fee[0],
                &denom_a.with_amount(receive_fee).as_coin(),
            );
            assert_eq!(
                &event.total_ack_fee[0],
                &denom_a.with_amount(ack_fee).as_coin()
            );

            assert_eq!(
                &event.total_timeout_fee[0],
                &denom_a.with_amount(timeout_fee).as_coin(),
            );
        }

        {
            let packets = chain_driver_a.query_incentivized_packets(&channel_id_a, &port_a)?;

            assert_eq!(packets.len(), 1);

            let packet = &packets[0];
            assert_eq!(packet.packet_id.sequence, sequence);

            assert_eq!(packet.packet_fees.len(), 1);
            let packet_fee = &packet.packet_fees[0];

            assert_eq!(packet_fee.fee.recv_fee.len(), 1);
            assert_eq!(packet_fee.fee.ack_fee.len(), 1);
            assert_eq!(packet_fee.fee.timeout_fee.len(), 1);

            assert_eq!(
                &packet_fee.fee.recv_fee[0],
                &denom_a.with_amount(receive_fee).as_coin(),
            );
            assert_eq!(
                &packet_fee.fee.ack_fee[0],
                &denom_a.with_amount(ack_fee).as_coin()
            );

            assert_eq!(
                &packet_fee.fee.timeout_fee[0],
                &denom_a.with_amount(timeout_fee).as_coin(),
            );

            assert_eq!(
                packet_fee.refund_address.as_ref(),
                user_a.value().address.as_str(),
            );
        }

        let receive_fee_2 = random_u128_range(300, 400);
        let ack_fee_2 = random_u128_range(200, 300);
        let timeout_fee_2 = random_u128_range(100, 200);

        let events2 = chain_driver_a.pay_packet_fee(
            &port_a,
            &channel_id_a,
            &DualTagged::new(sequence),
            &user_a,
            &denom_a.with_amount(receive_fee_2).as_ref(),
            &denom_a.with_amount(ack_fee_2).as_ref(),
            &denom_a.with_amount(timeout_fee_2).as_ref(),
        )?;

        chain_driver_a.assert_eventual_escrowed_amount_ics29(
            &user_a.address(),
            &balance_a2.as_ref(),
            receive_fee + receive_fee_2,
            ack_fee + ack_fee_2,
            timeout_fee + timeout_fee_2,
        )?;

        {
            let event = events2
                .iter()
                .find_map(|ev| {
                    if let IbcEvent::IncentivizedPacket(ev) = &ev.event {
                        Some(ev)
                    } else {
                        None
                    }
                })
                .ok_or_else(|| eyre!("expect incentivized packet event"))?;

            assert_eq!(event.sequence, sequence);

            assert_eq!(event.total_recv_fee.len(), 1);
            assert_eq!(event.total_ack_fee.len(), 1);
            assert_eq!(event.total_timeout_fee.len(), 1);

            assert_eq!(
                &event.total_recv_fee[0],
                &denom_a.with_amount(receive_fee + receive_fee_2).as_coin(),
            );
            assert_eq!(
                &event.total_ack_fee[0],
                &denom_a.with_amount(ack_fee + ack_fee_2).as_coin()
            );

            assert_eq!(
                &event.total_timeout_fee[0],
                &denom_a.with_amount(timeout_fee + timeout_fee_2).as_coin(),
            );
        }

        let denom_b = derive_ibc_denom(
            &channel.port_b.as_ref(),
            &channel.channel_id_b.as_ref(),
            &denom_a,
        )?;

        relayer.with_supervisor(|| {
            chain_driver_b.assert_eventual_wallet_amount(
                &user_b.address(),
                &denom_b.with_amount(send_amount).as_ref(),
            )?;

            chain_driver_a.assert_eventual_wallet_amount(
                &user_a.address(),
                &(balance_a1 - send_amount - receive_fee - receive_fee_2 - ack_fee - ack_fee_2)
                    .as_ref(),
            )?;

            chain_driver_a.assert_eventual_wallet_amount(
                &relayer_a.address(),
                &(relayer_balance_a + ack_fee + receive_fee + ack_fee_2 + receive_fee_2).as_ref(),
            )?;

            Ok(())
        })
    }
}

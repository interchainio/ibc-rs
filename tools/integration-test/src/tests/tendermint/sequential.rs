use std::time::Instant;

use ibc_relayer::chain::tracking::TrackedMsgs;
use ibc_relayer::config::types::max_msg_num::MaxMsgNum;
use ibc_relayer::config::ChainConfig;
use ibc_test_framework::chain::config;
use ibc_test_framework::prelude::*;
use ibc_test_framework::relayer::transfer::build_transfer_message;

const MESSAGES_PER_BATCH: usize = 5;
const TOTAL_TRANSACTIONS: usize = 5;
const TOTAL_MESSAGES: usize = MESSAGES_PER_BATCH * TOTAL_TRANSACTIONS;
const BLOCK_TIME_MILLIS: u64 = 1000;
const BLOCK_TIME: Duration = Duration::from_millis(BLOCK_TIME_MILLIS);

#[test]
fn test_sequential_commit() -> Result<(), Error> {
    run_binary_channel_test(&SequentialCommitTest)
}

pub struct SequentialCommitTest;

impl TestOverrides for SequentialCommitTest {
    fn modify_node_config(&self, config: &mut toml::Value) -> Result<(), Error> {
        config::set_timeout_commit(config, BLOCK_TIME)?;
        config::set_timeout_propose(config, BLOCK_TIME)?;

        // Enable priority mempool
        config::set_mempool_version(config, "v1")?;

        Ok(())
    }

    fn modify_relayer_config(&self, config: &mut Config) {
        // Use sequential batching for chain A, and default parallel batching for chain B
        match &mut config.chains[0] {
            ChainConfig::CosmosSdk(chain_config_a) => {
                chain_config_a.max_msg_num = MaxMsgNum::new(MESSAGES_PER_BATCH).unwrap();
                chain_config_a.sequential_batch_tx = true;
            }
        };

        match &mut config.chains[1] {
            ChainConfig::CosmosSdk(chain_config_b) => {
                chain_config_b.max_msg_num = MaxMsgNum::new(MESSAGES_PER_BATCH).unwrap();
                chain_config_b.sequential_batch_tx = false;
            }
        };
    }

    fn should_spawn_supervisor(&self) -> bool {
        false
    }
}

impl BinaryChannelTest for SequentialCommitTest {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        _relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channel: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        let wallet_a = chains.node_a.wallets().relayer().cloned();
        let wallet_b = chains.node_b.wallets().relayer().cloned();

        {
            let denom_a = chains.node_a.denom();
            let channel_version = channel.channel.dst_version().ok_or_else(|| {
                Error::generic(eyre!(
                    "failed to retrieve channel version for channel `{:#?}`",
                    channel.channel.dst_channel_id()
                ))
            })?;

            let transfer_message = build_transfer_message(
                &channel.port_a.as_ref(),
                &channel.channel_id_a.as_ref(),
                channel_version,
                &wallet_a.as_ref(),
                &wallet_b.address(),
                &vec![denom_a.with_amount(100u64).as_ref()],
                Duration::from_secs(30),
                None,
            )?;

            let messages = TrackedMsgs::new_static(
                vec![transfer_message; TOTAL_MESSAGES],
                "test_error_events",
            );

            let start = Instant::now();

            chains.handle_a().send_messages_and_wait_commit(messages)?;

            let end = Instant::now();

            let duration = end.duration_since(start);

            info!(
                "time taken to send {} messages on chain A: {:?}",
                TOTAL_MESSAGES, duration
            );

            // Time taken for submitting sequential batches should be around number of transactions * block time

            assert!(
                duration
                    > Duration::from_millis((BLOCK_TIME_MILLIS * TOTAL_TRANSACTIONS as u64) - 1000)
            );
            assert!(
                duration
                    < Duration::from_millis((BLOCK_TIME_MILLIS * TOTAL_TRANSACTIONS as u64) + 1000)
            );
        }

        {
            let denom_b = chains.node_b.denom();
            let flipped_channel = channel.channel.flipped().clone();
            let channel_version = flipped_channel.dst_version().ok_or_else(|| {
                Error::generic(eyre!(
                    "failed to retrieve channel version for channel `{:#?}`",
                    channel.channel.dst_channel_id()
                ))
            })?;

            let transfer_message = build_transfer_message(
                &channel.port_b.as_ref(),
                &channel.channel_id_b.as_ref(),
                channel_version,
                &wallet_b.as_ref(),
                &wallet_a.address(),
                &vec![denom_b.with_amount(100u64).as_ref()],
                Duration::from_secs(30),
                None,
            )?;

            let messages = TrackedMsgs::new_static(
                vec![transfer_message; TOTAL_MESSAGES],
                "test_error_events",
            );

            let start = Instant::now();

            chains.handle_b().send_messages_and_wait_commit(messages)?;

            let end = Instant::now();

            let duration = end.duration_since(start);

            // Time taken for submitting sequential batches should be around a single block time,
            // since the number of transactions are small enough to fit in a single block.

            info!(
                "time taken to send {} messages on chain B: {:?}",
                TOTAL_MESSAGES, duration
            );

            assert!(duration < Duration::from_millis(BLOCK_TIME_MILLIS * 2));
        }

        Ok(())
    }
}

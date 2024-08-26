use ibc_test_framework::prelude::*;

#[test]
fn benchmark_query_commitments() -> Result<(), Error> {
    run_binary_channel_test(&QueryCommitmentsBenchmark)
}

pub struct QueryCommitmentsBenchmark;

impl TestOverrides for QueryCommitmentsBenchmark {
    fn modify_relayer_config(&self, config: &mut Config) {
        config.mode.packets.clear_on_start = false;
        config.mode.packets.clear_interval = 0;
    }

    fn should_spawn_supervisor(&self) -> bool {
        false
    }
}

impl BinaryChannelTest for QueryCommitmentsBenchmark {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        config: &TestConfig,
        relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channel: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        use std::path::Path;

        use ibc_relayer::util::profiling::open_or_create_profile_file;

        let now = time::OffsetDateTime::now_utc();
        let path_str = format!(
            "{}/hermes-{:04}-{:02}-{:02}-{:02}{:02}{:02}-prof.json",
            config.chain_store_dir.as_path().display(),
            now.year(),
            now.month(),
            now.day(),
            now.hour(),
            now.minute(),
            now.second()
        );

        open_or_create_profile_file(Path::new(&path_str));
        ibc_relayer::util::profiling::enable(false, true);

        let denom_a = chains.node_a.denom();

        let wallet_a = chains.node_a.wallets().user1().cloned();
        let wallet_b = chains.node_b.wallets().user1().cloned();

        let a_to_b_amount = 12u64;
        let num_msgs = 1000000u64;

        info!(
            "Sending {num_msgs} IBC transfers from chain {} to chain {} with amount of {} {}",
            chains.chain_id_a(),
            chains.chain_id_b(),
            a_to_b_amount,
            denom_a
        );

        chains.node_a.chain_driver().ibc_transfer_token_multiple(
            &channel,
            &wallet_a.as_ref(),
            &wallet_b.address(),
            &vec![denom_a.with_amount(a_to_b_amount).as_ref()],
            num_msgs as usize,
            None,
        )?;

        info!("Waiting for profiling to be populated");

        relayer.with_supervisor(|| {
            std::thread::sleep(core::time::Duration::from_secs(180));

            Ok(())
        })
    }
}

/*!
   Test for verifying the solution in
   [#1542](https://github.com/informalsystems/hermes/pull/1542)

   On running the test, the log should show messages like:

   ```text
   2021-11-05T14:12:09.633184Z  WARN ThreadId(30) [ibc-1] estimate_gas: failed to simulate tx, falling back on default gas because the error is potentially recoverable: gRPC call failed with status: status: InvalidArgument, message: "failed to execute message; message index: 0: acknowledge packet verification failed: packet acknowledgement verification failed: failed packet acknowledgement verification for client (07-tendermint-0): client state height < proof height ({0 243} < {0 554}): invalid height: invalid request", details: [], metadata: MetadataMap { headers: {"content-type": "application/grpc"} }
   2021-11-05T14:12:09.633290Z DEBUG ThreadId(30) [ibc-1] send_tx: using 900000000 gas, fee Fee { amount: "900000stake", gas_limit: 900000000 }
   2021-11-05T14:12:09.639044Z DEBUG ThreadId(30) [ibc-1] send_tx: broadcast_tx_sync: Response { code: Ok, data: Data([]), log: Log("[]"), hash: transaction::Hash(BA94AE4CA198F56E27B4A44DA5E508A2E2207E306F475E5285D873296D892170) }
   ```
*/

use ibc_relayer::config::{types::MaxMsgNum, ChainConfig};
use ibc_relayer::transfer::{build_and_send_transfer_messages, TransferOptions};
use ibc_relayer_types::events::IbcEvent;
use ibc_test_framework::prelude::*;

#[test]
fn test_simulation() -> Result<(), Error> {
    run_binary_channel_test(&SimulationTest)
}

const MAX_MSGS: usize = 5;

pub struct SimulationTest;

impl TestOverrides for SimulationTest {
    fn modify_relayer_config(&self, config: &mut Config) {
        for chain in config.chains.iter_mut() {
            match chain {
                ChainConfig::CosmosSdk(chain_config) => {
                    chain_config.max_msg_num = MaxMsgNum::new(MAX_MSGS).unwrap();
                }
            }
        }
    }
}

impl BinaryChannelTest for SimulationTest {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        _relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channel: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        tx_raw_ft_transfer(
            chains.handle_a(),
            chains.handle_b(),
            &channel,
            &chains.node_b.wallets().user1().address(),
            &chains.node_a.denom(),
            9999,
            1000,
            Duration::from_secs(0),
            MAX_MSGS,
            None,
        )?;

        suspend()
    }
}

/**
   Perform the same operation as `hermes tx ft-transfer`.

   The function call skips the checks done in the CLI, as we already
   have the necessary information given to us by the test framework.

   Note that we cannot change the sender's wallet in this case,
   as the current `send_tx` implementation in
   [`CosmosSdkChain`](ibc_relayer::chain::cosmos::CosmosSdkChain)
   always use the signer wallet configured in the
   [`ChainConfig`](ibc_relayer::config::ChainConfig).
*/
fn tx_raw_ft_transfer<SrcChain: ChainHandle, DstChain: ChainHandle>(
    src_handle: &SrcChain,
    dst_handle: &DstChain,
    channel: &ConnectedChannel<SrcChain, DstChain>,
    recipient: &MonoTagged<DstChain, &WalletAddress>,
    denom: &MonoTagged<SrcChain, &Denom>,
    amount: u64,
    timeout_height_offset: u64,
    timeout_duration: Duration,
    number_messages: usize,
    memo: Option<String>,
) -> Result<Vec<IbcEvent>, Error> {
    let tokens = vec![(amount.into(), denom.value().to_string())];
    let transfer_options = TransferOptions {
        src_port_id: channel.port_a.value().clone(),
        src_channel_id: channel.channel_id_a.value().clone(),
        tokens,
        receiver: Some(recipient.value().0.clone()),
        timeout_height_offset,
        timeout_duration,
        number_msgs: number_messages,
        memo,
    };

    let events_with_heights =
        build_and_send_transfer_messages(src_handle, dst_handle, &transfer_options)?;

    Ok(events_with_heights.into_iter().map(|ev| ev.event).collect())
}

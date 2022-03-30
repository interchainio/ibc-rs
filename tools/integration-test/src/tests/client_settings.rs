use std::time::Duration;

use ibc::core::ics02_client::trust_threshold::TrustThreshold;

use ibc::clients::ics07_tendermint::client_state::ClientState as TendermintClientState;
use ibc::core::ics02_client::client_state::AnyClientState;
use ibc::Height;
use ibc_relayer::chain::client::ClientSettings;
use ibc_relayer::chain::cosmos;

use ibc_test_framework::prelude::*;

/// A test to exercise default foreign client settings.
#[test]
fn test_client_defaults() -> Result<(), Error> {
    run_binary_chain_test(&ClientDefaultsTest)
}

/// A test to exercise customization of foreign client settings.
#[test]
fn test_client_settings() -> Result<(), Error> {
    run_binary_chain_test(&ClientSettingsTest)
}

struct ClientDefaultsTest;

struct ClientSettingsTest;

struct SettingsTestOverrides;

impl TestOverrides for ClientDefaultsTest {
    fn modify_relayer_config(&self, config: &mut Config) {
        config.chains[0].clock_drift = Duration::from_secs(3);
        config.chains[0].max_block_time = Duration::from_secs(5);
        config.chains[0].trusting_period = Some(Duration::from_secs(120_000));
        config.chains[0].trust_threshold = TrustThreshold::new(13, 23).unwrap().try_into().unwrap();

        config.chains[1].clock_drift = Duration::from_secs(6);
        config.chains[1].max_block_time = Duration::from_secs(15);
        config.chains[1].trusting_period = Some(Duration::from_secs(340_000));
        config.chains[1].trust_threshold = TrustThreshold::TWO_THIRDS.try_into().unwrap();
    }
}

impl BinaryChainTest for ClientDefaultsTest {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        _relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
    ) -> Result<(), Error> {
        let client_id = chains.foreign_clients.client_a_to_b.id();
        let state = query_client_state(chains.handle_b, client_id)?;
        assert_eq!(state.max_clock_drift, Duration::from_secs(24));
        assert_eq!(state.trusting_period, Duration::from_secs(120_000));
        assert_eq!(state.trust_level, TrustThreshold::new(13, 23).unwrap());

        let client_id = chains.foreign_clients.client_b_to_a.id();
        let state = query_client_state(chains.handle_a, client_id)?;
        assert_eq!(state.max_clock_drift, Duration::from_secs(14));
        assert_eq!(state.trusting_period, Duration::from_secs(340_000));
        assert_eq!(state.trust_level, TrustThreshold::TWO_THIRDS);
        Ok(())
    }
}

impl TestOverrides for SettingsTestOverrides {
    fn client_settings_a_to_b(&self) -> ClientSettings {
        ClientSettings::Cosmos(cosmos::client::Settings {
            max_clock_drift: Some(Duration::from_secs(3)),
            trusting_period: Some(Duration::from_secs(120_000)),
            trust_threshold: Some(TrustThreshold::new(13, 23).unwrap()),
        })
    }

    fn client_settings_b_to_a(&self) -> ClientSettings {
        ClientSettings::Cosmos(cosmos::client::Settings {
            max_clock_drift: Some(Duration::from_secs(6)),
            trusting_period: Some(Duration::from_secs(340_000)),
            trust_threshold: Some(TrustThreshold::TWO_THIRDS),
        })
    }
}

impl BinaryChainTest for ClientSettingsTest {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        _relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
    ) -> Result<(), Error> {
        let client_id = chains.foreign_clients.client_a_to_b.id();
        let state = query_client_state(chains.handle_b, client_id)?;
        assert_eq!(state.max_clock_drift, Duration::from_secs(3));
        assert_eq!(state.trusting_period, Duration::from_secs(120_000));
        assert_eq!(state.trust_level, TrustThreshold::new(13, 23).unwrap());

        let client_id = chains.foreign_clients.client_b_to_a.id();
        let state = query_client_state(chains.handle_a, client_id)?;
        assert_eq!(state.max_clock_drift, Duration::from_secs(6));
        assert_eq!(state.trusting_period, Duration::from_secs(340_000));
        assert_eq!(state.trust_level, TrustThreshold::TWO_THIRDS);
        Ok(())
    }
}

impl HasOverrides for ClientSettingsTest {
    type Overrides = SettingsTestOverrides;

    fn get_overrides(&self) -> &SettingsTestOverrides {
        &SettingsTestOverrides
    }
}

fn query_client_state<Chain: ChainHandle>(
    handle: Chain,
    id: &ClientId,
) -> Result<TendermintClientState, Error> {
    let state = handle.query_client_state(id, Height::zero())?;
    #[allow(unreachable_patterns)]
    match state {
        AnyClientState::Tendermint(state) => Ok(state),
        _ => unreachable!("unexpected client state type"),
    }
}

/*!
   Constructs for implementing overrides for test cases.
*/

use core::time::Duration;
use ibc_relayer::config::default::connection_delay as default_connection_delay;
use ibc_relayer::config::Config;
use ibc_relayer::foreign_client::CreateOptions as ClientOptions;
use ibc_relayer_types::core::ics04_channel::channel::Ordering;
use ibc_relayer_types::core::ics04_channel::version::Version;
use ibc_relayer_types::core::ics24_host::identifier::PortId;

use crate::error::Error;
use crate::framework::base::HasOverrides;
use crate::framework::base::TestConfigOverride;
use crate::framework::binary::chain::{ClientOptionsOverride, RelayerConfigOverride};
use crate::framework::binary::channel::{
    ChannelOrderOverride, ChannelVersionOverride, PortsOverride,
};
use crate::framework::binary::connection::ConnectionDelayOverride;
use crate::framework::binary::node::{NodeConfigOverride, NodeGenesisOverride};
use crate::framework::nary::channel::PortsOverride as NaryPortsOverride;
use crate::framework::supervisor::SupervisorOverride;
use crate::types::config::TestConfig;
use crate::types::topology::TopologyType;

use super::binary::chain::TopologyOverride;

/**
   This trait should be implemented for all test cases to allow overriding
   some parts of the behavior during the test setup.

   Since all methods in this trait have default implementation, test cases
   that do not need any override can have an empty implementation body for
   this trait.

   The trait provides generic implementation of the specialized traits such as
   [`RelayerConfigOverride`]. As a result, it is sufficient for test
   writers to only implement this trait instead of implementing the
   numerous override traits.

   When a new override trait is defined, the same trait method should
   also be defined inside this trait with a default method body.
*/
pub trait TestOverrides {
    fn modify_test_config(&self, _config: &mut TestConfig) {}

    /**
        Modify the full node config before the chain gets initialized.

        The config is in the dynamic-typed [`toml::Value`] format, as we do not
        want to model the full format of the node config in Rust. Test authors
        can use the helper methods in [`chain::config`](crate::chain::config)
        to modify common config fields.

        Implemented for [`NodeConfigOverride`].
    */
    fn modify_node_config(&self, _config: &mut toml::Value) -> Result<(), Error> {
        Ok(())
    }

    /**
        Modify the genesis file before the chain gets initialized.

        The config is in the dynamic-typed [`serde_json::Value`] format, as we do not
        want to model the full format of the genesis file in Rust.

        Implemented for [`NodeGenesisOverride`].
    */
    fn modify_genesis_file(&self, _genesis: &mut serde_json::Value) -> Result<(), Error> {
        Ok(())
    }

    /**
       Modify the relayer config before initializing the relayer. Does no
       modification by default.

       Implemented for [`RelayerConfigOverride`].
    */
    fn modify_relayer_config(&self, _config: &mut Config) {
        // No modification by default
    }

    /// Returns the settings for the foreign client on the first chain for the
    /// second chain. The defaults are for a client connecting two Cosmos chains
    /// with no custom settings.
    fn client_options_a_to_b(&self) -> ClientOptions {
        Default::default()
    }

    /// Returns the settings for the foreign client on the second chain for the
    /// first chain. The defaults are for a client connecting two Cosmos chains
    /// with no custom settings.
    fn client_options_b_to_a(&self) -> ClientOptions {
        Default::default()
    }

    fn should_spawn_supervisor(&self) -> bool {
        true
    }

    /**
       Return the connection delay used for creating connections as [`Duration`].
       Defaults to zero.

       Implemented for [`ConnectionDelayOverride`].
    */
    fn connection_delay(&self) -> Duration {
        default_connection_delay()
    }

    /**
       Return the port ID used for creating the channel for the first chain.
       Returns the "transfer" port by default.

       Implemented for [`PortsOverride`].
    */
    fn channel_port_a(&self) -> PortId {
        PortId::transfer()
    }

    /**
       Return the port ID used for creating the channel for the second chain.
       Returns the "transfer" port by default.

       Implemented for [`PortsOverride`].
    */
    fn channel_port_b(&self) -> PortId {
        PortId::transfer()
    }

    /**
       Return the channel ordering used for creating channels as [`Ordering`].
       Defaults to [`Ordering::Unordered`].

       Implemented for [`ChannelOrderOverride`].
    */
    fn channel_order(&self) -> Ordering {
        Ordering::Unordered
    }

    /**
       Return the channel version used for creating channels as [`Version`].
       Defaults to [`Version::ics20()`].

       Implemented for [`ChannelVersionOverride`].
    */
    fn channel_version(&self) -> Version {
        Version::ics20(1)
    }

    fn topology(&self) -> Option<TopologyType> {
        None
    }
}

impl<Test: TestOverrides> HasOverrides for Test {
    type Overrides = Self;

    fn get_overrides(&self) -> &Self {
        self
    }
}

impl<Test: TestOverrides> TestConfigOverride for Test {
    fn modify_test_config(&self, config: &mut TestConfig) {
        TestOverrides::modify_test_config(self, config)
    }
}

impl<Test: TestOverrides> NodeConfigOverride for Test {
    fn modify_node_config(&self, config: &mut toml::Value) -> Result<(), Error> {
        TestOverrides::modify_node_config(self, config)
    }
}

impl<Test: TestOverrides> NodeGenesisOverride for Test {
    fn modify_genesis_file(&self, genesis: &mut serde_json::Value) -> Result<(), Error> {
        TestOverrides::modify_genesis_file(self, genesis)
    }
}

impl<Test: TestOverrides> RelayerConfigOverride for Test {
    fn modify_relayer_config(&self, config: &mut Config) {
        TestOverrides::modify_relayer_config(self, config)
    }
}

impl<Test: TestOverrides> ClientOptionsOverride for Test {
    fn client_options_a_to_b(&self) -> ClientOptions {
        TestOverrides::client_options_a_to_b(self)
    }

    fn client_options_b_to_a(&self) -> ClientOptions {
        TestOverrides::client_options_b_to_a(self)
    }
}

impl<Test: TestOverrides> SupervisorOverride for Test {
    fn should_spawn_supervisor(&self) -> bool {
        TestOverrides::should_spawn_supervisor(self)
    }
}

impl<Test: TestOverrides> ConnectionDelayOverride for Test {
    fn connection_delay(&self) -> Duration {
        TestOverrides::connection_delay(self)
    }
}

impl<Test: TestOverrides> PortsOverride for Test {
    fn channel_port_a(&self) -> PortId {
        TestOverrides::channel_port_a(self)
    }

    fn channel_port_b(&self) -> PortId {
        TestOverrides::channel_port_b(self)
    }
}

impl<Test: TestOverrides> ChannelOrderOverride for Test {
    fn channel_order(&self) -> Ordering {
        TestOverrides::channel_order(self)
    }
}

impl<Test: TestOverrides> ChannelVersionOverride for Test {
    fn channel_version(&self) -> Version {
        TestOverrides::channel_version(self)
    }
}

impl<Test: TestOverrides> NaryPortsOverride<2> for Test {
    fn channel_ports(&self) -> [[PortId; 2]; 2] {
        let port_a = self.channel_port_a();
        let port_b = self.channel_port_b();

        [[port_a.clone(), port_b.clone()], [port_b, port_a]]
    }
}

impl<Test: TestOverrides> TopologyOverride for Test {
    fn topology(&self) -> Option<TopologyType> {
        TestOverrides::topology(self)
    }
}

pub mod handshake;

use core::convert::TryInto;
use core::str::FromStr;
use rusty_fork::rusty_fork_test;

use ibc::ics24_host::identifier::{ChainId, PortId};

use crate::init::init_test;

rusty_fork_test! {
    #[test]
    fn test_connection_handshake() {
        let config = init_test().unwrap();

        let chain_id_a: ChainId = "ibc-0".to_string().try_into().unwrap();
        let chain_id_b: ChainId = "ibc-1".to_string().try_into().unwrap();

        handshake::test_connection_handshake(
          &config, chain_id_a, chain_id_b
        ).unwrap()
    }

    #[test]
    fn test_connection_and_channel_handshake() {
        let config = init_test().unwrap();

        let chain_id_a: ChainId = "ibc-0".to_string().try_into().unwrap();
        let chain_id_b: ChainId = "ibc-1".to_string().try_into().unwrap();
        let port_id: PortId = PortId::from_str("transfer").unwrap();

        handshake::test_connection_and_channel_handshake(
          &config, chain_id_a, chain_id_b, port_id
        ).unwrap()
    }
}

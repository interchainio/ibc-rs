//! ## Simple Test
//!
//! Here's an example of a simple test that showcases some of the important features that the
//! framework exposes:
//!
//! ```no_run
//! # use ibc_test_framework::prelude::*;
//!
//! pub struct ExampleTest;
//!
//! #[test]
//! pub fn example_test() -> Result<(), Error> {
//!     run_binary_channel_test(&ExampleTest)
//! }
//!
//! impl TestOverrides for ExampleTest {}
//!
//! impl BinaryChannelTest for ExampleTest {
//!     fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
//!         &self,
//!         _config: &TestConfig,
//!         _relayer: RelayerDriver,
//!         _chains: ConnectedChains<ChainA, ChainB>,
//!         _channel: ConnectedChannel<ChainA, ChainB>,
//!     ) -> Result<(), Error> {
//!         suspend()
//!     }
//! }
//! ```
//!
//! The `example_test` function is the top-most function that drives the test itself. The top-
//! most test function typically does nothing more than to specify what kind of test we want
//! to run. In the case of this example, we're running a
//! [`BinaryChannelTest`](crate::framework::binary::channel::BinaryChannelTest),
//! which sets up a relayer instance between two full nodes connected via IBC channels with
//! completed handshakes.
//!
//! Note that the `run_binary_channel_test` (and indeed every `run_*` test function) takes as
//! its single parameter a struct that represents the test case. While in this case, the struct
//! is empty, fields can be added to the struct in the case that you want to run multiple tests
//! using it. See `tools/test-framework/src/docs/walkthroughs/memo.rs` as an example
//! of a test that utilizes a non-empty struct as input. In order to customize the behavior
//! of a test, different traits need to implemented on the empty struct, depending on how you
//! wish to modify the test.
//!
//! This example tests showcases implementing the `TestOverrides` trait, which is used to set
//! configuration and initialization values for the relayer instance that is being tested (in
//! this case though, nothing is being overridden).
//!
//! The main logic of the test is implemented in the `run` function of the `BinaryChannelTest`
//! trait. This trait is implemented for our empty test struct since we're choosing to run a
//! test between two chains connected via IBC channels. If we had instead opted to run a binary
//! _chain_ test using the `run_binary_chain_test`, then we would instead implement the
//! [`BinaryChainTest`](crate::framework::binary::channel::BinaryChannelTest)
//! trait for our empty test struct.
//!
//! The `run` function's parameters are:
//! 1. `config`: for accessing any test-specific configuration values during the course of the test
//! 2. `relayer`: the relayer instance that is being tested
//! 3. `chains`: handles to the two chains, `ChainA` and `ChainB`, that are being relayed between
//! 4. `channel`: handles to the uni-directional channels connecting both ends of the two chains
//!
//! In this simple example test, the `run` function simply calls the `suspend` function,
//! which suspends the test indefinitely. While this means the test will never actually
//! pass, we can use this as a starting point in order to perform _manual testing_ with
//! the chains that have been set up in the test.
//!
//! You can find the file containing this example test at `tools/integration-test/src/tests/example.rs`.

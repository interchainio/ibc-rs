extern crate alloc;

mod runner;

use modelator::{run_tla_steps, ModelChecker, TestError};
use runner::IbcTestRunner;

#[test]
fn mbt() {
    // we should be able to just return the `Result` once the following
    // issue is fixed: https://github.com/rust-lang/rust/issues/43301
    if let Err(e) = run_tests() {
        panic!("{}", e);
    }
}

fn run_tests() -> Result<(), TestError> {
    // run the test
    let tla_tests_file = "tests/support/model_based/IBCTests.tla";
    let tla_config_file = "tests/support/model_based/IBCTests.cfg";
    let mut opts = modelator::Options::default();
    opts.model_checker_options.model_checker = ModelChecker::Tlc;

    let mut runner = IbcTestRunner::new();
    run_tla_steps(tla_tests_file, tla_config_file, &opts, &mut runner)?;
    Ok(())
}

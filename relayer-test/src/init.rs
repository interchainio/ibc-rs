use ibc_relayer::config::{self, Config};
use std::env;
use tracing::info;

pub fn init_test() -> Result<Config, Box<dyn std::error::Error>> {
    color_eyre::install()?;
    tracing_subscriber::fmt::init();

    let config_path = match env::var("IBC_TEST_CONFIG") {
        Ok(path) => path,
        Err(_) => format!("{}/.hermes/config.toml", env::var("HOME")?),
    };

    info!("initializing test from test config at {}", config_path);

    let config = config::load("/home/soares/.hermes/config.toml")?;

    Ok(config)
}

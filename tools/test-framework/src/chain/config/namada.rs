use eyre::{eyre, Report as Error};
use toml::Value;

/// Set the `rpc` field in the full node config.
pub fn set_rpc_port(config: &mut Value, port: u16) -> Result<(), Error> {
    config
        .get_mut("ledger")
        .ok_or_else(|| eyre!("expected ledger section"))?
        .get_mut("cometbft")
        .ok_or_else(|| eyre!("expected cometbft section"))?
        .get_mut("rpc")
        .ok_or_else(|| eyre!("expected cometbft rpc section"))?
        .as_table_mut()
        .ok_or_else(|| eyre!("expected object"))?
        .insert(
            "laddr".to_string(),
            format!("tcp://0.0.0.0:{}", port).into(),
        );

    Ok(())
}

/// Set the `p2p` field in the full node config.
pub fn set_p2p_port(config: &mut Value, port: u16) -> Result<(), Error> {
    config
        .get_mut("ledger")
        .ok_or_else(|| eyre!("expected ledger section"))?
        .get_mut("cometbft")
        .ok_or_else(|| eyre!("expected cometbft section"))?
        .get_mut("p2p")
        .ok_or_else(|| eyre!("expected cometbft p2p section"))?
        .as_table_mut()
        .ok_or_else(|| eyre!("expected object"))?
        .insert(
            "laddr".to_string(),
            format!("tcp://0.0.0.0:{}", port).into(),
        );

    Ok(())
}

/// Set the `proxy_app` field in the full node config.
pub fn set_proxy_app_port(config: &mut Value, port: u16) -> Result<(), Error> {
    config
        .get_mut("ledger")
        .ok_or_else(|| eyre!("expected ledger section"))?
        .get_mut("cometbft")
        .ok_or_else(|| eyre!("expected cometbft section"))?
        .as_table_mut()
        .ok_or_else(|| eyre!("expected object"))?
        .insert(
            "proxy_app".to_string(),
            format!("tcp://0.0.0.0:{}", port).into(),
        );

    Ok(())
}

/// Set the `block_cache_bytes` field in the full node config.
pub fn set_block_cache_bytes(config: &mut Value, cache_size: i64) -> Result<(), Error> {
    config
        .get_mut("ledger")
        .ok_or_else(|| eyre!("expected ledger section"))?
        .get_mut("shell")
        .ok_or_else(|| eyre!("expected shell section"))?
        .as_table_mut()
        .ok_or_else(|| eyre!("expected object"))?
        .insert(
            "block_cache_bytes".to_string(),
            Value::Integer(cache_size),
        );

    Ok(())
}

pub fn set_unbonding_len(parameters: &mut Value, length: i64) -> Result<(), Error> {
    parameters
        .get_mut("pos_params")
        .ok_or_else(|| eyre!("expected pos_params section"))?
        .as_table_mut()
        .ok_or_else(|| eyre!("expected object"))?
        .insert("unbonding_len".to_string(), Value::Integer(length));

    Ok(())
}

pub fn set_pipeline_len(parameters: &mut Value, length: i64) -> Result<(), Error> {
    parameters
        .get_mut("pos_params")
        .ok_or_else(|| eyre!("expected pos_params section"))?
        .as_table_mut()
        .ok_or_else(|| eyre!("expected object"))?
        .insert("pipeline_len".to_string(), Value::Integer(length));

    Ok(())
}

pub fn set_default_mint_limit(
    parameters: &mut Value,
    limit: i64,
) -> Result<(), Error> {
    parameters
        .get_mut("ibc_params")
        .ok_or_else(|| eyre!("expected ibc_params section"))?
        .as_table_mut()
        .ok_or_else(|| eyre!("expected object"))?
        .insert(
            "default_mint_limit".to_string(),
            Value::String(limit.to_string()),
        );

    Ok(())
}

pub fn set_default_per_epoch_throughput_limit(
    parameters: &mut Value,
    limit: i64,
) -> Result<(), Error> {
    parameters
        .get_mut("ibc_params")
        .ok_or_else(|| eyre!("expected ibc_params section"))?
        .as_table_mut()
        .ok_or_else(|| eyre!("expected object"))?
        .insert(
            "default_per_epoch_throughput_limit".to_string(),
            Value::String(limit.to_string()),
        );

    Ok(())
}

pub fn set_epochs_per_year(parameters: &mut Value, epochs: i64) -> Result<(), Error> {
    parameters
        .get_mut("parameters")
        .ok_or_else(|| eyre!("expected parameters section"))?
        .as_table_mut()
        .ok_or_else(|| eyre!("expected object"))?
        .insert(
            "epochs_per_year".to_string(),
            Value::Integer(epochs),
        );

    Ok(())
}

use alloc::sync::Arc;
use std::error::Error;
use std::io;
use std::sync::RwLock;

use abscissa_core::{Command, Options, Runnable};
use crossbeam_channel::Sender;

use ibc_relayer::chain::handle::{ChainHandle, ProdChainHandle};
use ibc_relayer::config::reload::ConfigReload;
use ibc_relayer::config::Config;
use ibc_relayer::rest;
use ibc_relayer::supervisor::{cmd::SupervisorCmd, Supervisor};
use ibc_relayer::telemetry::Telemetry;

use crate::conclude::json;
use crate::conclude::Output;
use crate::prelude::*;

#[derive(Clone, Command, Debug, Options)]
pub struct StartCmd {}

impl Runnable for StartCmd {
    fn run(&self) {
        let config = (*app_config()).clone();
        let config = Arc::new(RwLock::new(config));

        let (supervisor, tx_cmd) = make_supervisor::<ProdChainHandle>(config.clone())
            .unwrap_or_else(|e| {
                Output::error(format!("Hermes failed to start, last error: {}", e)).exit();
                unreachable!()
            });

        match crate::config::config_path() {
            Some(config_path) => {
                let reload = ConfigReload::new(config_path, config, tx_cmd.clone());
                register_signals(reload, tx_cmd).unwrap_or_else(|e| {
                    warn!("failed to install signal handler: {}", e);
                });
            }
            None => {
                warn!("cannot figure out configuration path, skipping registration of signal handlers");
            }
        };

        info!("Hermes has started");
        match supervisor.run() {
            Ok(()) => Output::success_msg("done").exit(),
            Err(e) => Output::error(format!("Hermes failed to start, last error: {}", e)).exit(),
        }
    }
}

/// Register the SIGHUP and SIGUSR1 signals, and notify the supervisor.
/// - SIGHUP: Trigger a reload of the configuration.
/// - SIGUSR1: Ask the supervisor to dump its state and print it to the console.
fn register_signals(reload: ConfigReload, tx_cmd: Sender<SupervisorCmd>) -> Result<(), io::Error> {
    use signal_hook::{consts::signal::*, iterator::Signals};

    let sigs = vec![
        SIGHUP,  // Reload of configuration
        SIGUSR1, // Dump state
    ];

    let mut signals = Signals::new(&sigs)?;

    std::thread::spawn(move || {
        for signal in &mut signals {
            match signal {
                SIGHUP => {
                    info!("reloading configuration (triggered by SIGHUP)");
                    match reload.reload() {
                        Ok(true) => info!("configuration successfully reloaded"),
                        Ok(false) => info!("configuration did not change"),
                        Err(e) => error!("failed to reload configuration: {}", e),
                    }
                }
                SIGUSR1 => {
                    info!("Dumping state (triggered by SIGUSR1)");

                    let (tx, rx) = crossbeam_channel::bounded(1);
                    tx_cmd.try_send(SupervisorCmd::DumpState(tx)).unwrap();

                    std::thread::spawn(move || {
                        if let Ok(state) = rx.recv() {
                            if json() {
                                match serde_json::to_string(&state) {
                                    Ok(out) => println!("{}", out),
                                    Err(e) => {
                                        error!("failed to serialize relayer state to JSON: {}", e)
                                    }
                                }
                            } else {
                                state.print_info();
                            }
                        }
                    });
                }

                _ => (),
            }
        }
    });

    Ok(())
}

#[cfg(feature = "rest-server")]
fn spawn_rest_server(config: &Arc<RwLock<Config>>) -> Option<rest::Receiver> {
    let rest = config.read().expect("poisoned lock").rest.clone();

    if rest.enabled {
        let rest_config = ibc_relayer_rest::Config::new(rest.host, rest.port);
        let (_, rest_receiver) = ibc_relayer_rest::server::spawn(rest_config);
        Some(rest_receiver)
    } else {
        info!("[rest] address not configured, REST server disabled");
        None
    }
}

#[cfg(not(feature = "rest-server"))]
fn spawn_rest_server(config: &Arc<RwLock<Config>>) -> Option<rest::Receiver> {
    let rest = config.read().expect("poisoned lock").rest.clone();

    if rest.enabled {
        warn!(
            "REST server enabled in the config but Hermes was built without RESET support, \
             build Hermes with --features=rest-server to enable REST support."
        );

        None
    } else {
        None
    }
}

#[cfg(feature = "telemetry")]
fn spawn_telemetry_server(
    config: &Arc<RwLock<Config>>,
) -> Result<Telemetry, Box<dyn Error + Send + Sync>> {
    let state = ibc_telemetry::new_state();

    let telemetry = config.read().expect("poisoned lock").telemetry.clone();
    if telemetry.enabled {
        match ibc_telemetry::spawn((telemetry.host, telemetry.port), state.clone()) {
            Ok((addr, _)) => {
                info!(
                    "telemetry service running, exposing metrics at http://{}/metrics",
                    addr
                );
            }
            Err(e) => {
                error!("telemetry service failed to start: {}", e);
                return Err(e);
            }
        }
    }

    Ok(state)
}

#[cfg(not(feature = "telemetry"))]
fn spawn_telemetry_server(
    config: &Arc<RwLock<Config>>,
) -> Result<Telemetry, Box<dyn Error + Send + Sync>> {
    if config.read().expect("poisoned lock").telemetry.enabled {
        warn!(
            "telemetry enabled in the config but Hermes was built without telemetry support, \
             build Hermes with --features=telemetry to enable telemetry support."
        );
    }

    Ok(ibc_relayer::telemetry::TelemetryDisabled)
}

fn make_supervisor<Chain: ChainHandle + 'static>(
    config: Arc<RwLock<Config>>,
) -> Result<(Supervisor<Chain>, Sender<SupervisorCmd>), Box<dyn Error + Send + Sync>> {
    let telemetry = spawn_telemetry_server(&config)?;
    let rest = spawn_rest_server(&config);

    Ok(Supervisor::new(config, rest, telemetry))
}

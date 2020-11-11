use std::ops::Deref;

use abscissa_core::{application::fatal_error, error::BoxError, Command, Options, Runnable};

use relayer::config::Config;

use crate::{application::APPLICATION, prelude::*};

#[derive(Command, Debug, Options)]
pub struct StartCmd {
    #[options(help = "reset state from trust options", short = "r")]
    reset: bool,
}

impl StartCmd {
    async fn cmd(&self) -> Result<(), BoxError> {
        let config = app_config().clone();
        start(config, self.reset).await
    }
}

impl Runnable for StartCmd {
    fn run(&self) {
        abscissa_tokio::run(&APPLICATION, async move {
            self.cmd()
                .await
                .unwrap_or_else(|e| fatal_error(app_reader().deref(), &*e));
        })
        .unwrap();
    }
}

async fn start(config: Config, reset: bool) -> Result<(), BoxError> {
    todo!() // TODO: Move v0 command here
}

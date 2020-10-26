//! `light` subcommand

use abscissa_core::{Command, Options, Runnable};

mod add;

/// `light` subcommand
#[derive(Command, Debug, Options, Runnable)]
pub enum LightCmd {
    /// The `light add` subcommand
    #[options(help = "add a light client peer for a given chain")]
    Add(add::AddCmd),
}
